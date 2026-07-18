use chrono::Local;
use hound::{WavReader, WavWriter};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

const MAX_CHUNK_SIZE_BYTES: u32 = 20_000_000;
const MAX_UPLOAD_ATTEMPTS: u32 = 4;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ChunkState {
    index: usize,
    status: String,
    text: String,
    attempts: u32,
    last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkflowManifest {
    source_file: String,
    model: String,
    language: String,
    updated_at: String,
    chunks: Vec<ChunkState>,
}

pub async fn transcribe_wav_idempotent<F>(
    wav_file_path: String,
    transcript_file_path: String,
    api_key: String,
    model: String,
    language: String,
    cancel_token: CancellationToken,
    mut log: F,
) -> Result<String, String>
where
    F: FnMut(String) + Send + 'static,
{
    if api_key.trim().is_empty() || api_key == "your_groq_api_key_here" {
        return Err("groq_api_key is empty or invalid in settings.json".to_string());
    }

    if !Path::new(&wav_file_path).exists() {
        return Err(format!("WAV file not found: {}", wav_file_path));
    }

    log(format!("Groq: reading WAV {}", wav_file_path));
    let chunks = build_wav_chunks(&wav_file_path)?;
    let manifest_path = format!("{}.workflow.json", wav_file_path);
    let mut manifest = load_or_create_manifest(
        &manifest_path,
        &wav_file_path,
        &model,
        &language,
        chunks.len(),
    )?;

    log(format!("Groq: total chunk {}", chunks.len()));
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("Groq HTTP client setup failed: {e}"))?;

    for (i, chunk_data) in chunks.into_iter().enumerate() {
        if cancel_token.is_cancelled() {
            save_manifest(&manifest_path, &manifest)?;
            return Err("Groq cancelled".to_string());
        }

        if manifest.chunks[i].status == "success" {
            log(format!(
                "Groq: chunk {} skipped (already successful)",
                i + 1
            ));
            continue;
        }

        let mut last_error = String::new();
        for attempt in 1..=MAX_UPLOAD_ATTEMPTS {
            if cancel_token.is_cancelled() {
                save_manifest(&manifest_path, &manifest)?;
                return Err("Groq cancelled".to_string());
            }

            manifest.chunks[i].attempts += 1;
            manifest.chunks[i].status = "uploading".to_string();
            manifest.chunks[i].last_error = None;
            save_manifest(&manifest_path, &manifest)?;

            log(format!("Groq: upload chunk {} attempt {}", i + 1, attempt));
            match upload_chunk(
                &client,
                &api_key,
                &model,
                &language,
                i + 1,
                chunk_data.clone(),
            )
            .await
            {
                Ok(text) => {
                    manifest.chunks[i].status = "success".to_string();
                    manifest.chunks[i].text = text.trim().to_string();
                    manifest.chunks[i].last_error = None;
                    save_manifest(&manifest_path, &manifest)?;
                    log(format!("Groq: chunk {} success", i + 1));
                    break;
                }
                Err(err) => {
                    last_error = err;
                    manifest.chunks[i].status = "failed".to_string();
                    manifest.chunks[i].last_error = Some(last_error.clone());
                    save_manifest(&manifest_path, &manifest)?;
                    log(format!("Groq: chunk {} failed: {}", i + 1, last_error));

                    let backoff = 2_u64.pow(attempt.min(5));
                    sleep(Duration::from_secs(backoff)).await;
                }
            }
        }

        if manifest.chunks[i].status != "success" {
            return Err(format!(
                "Groq chunk {} failed finally: {}",
                i + 1,
                last_error
            ));
        }
    }

    let final_text = manifest
        .chunks
        .iter()
        .map(|chunk| chunk.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    write_whisper_transcript(&transcript_file_path, &wav_file_path, &final_text)?;
    log("Groq: all chunks completed".to_string());

    Ok(final_text)
}

fn build_wav_chunks(wav_file_path: &str) -> Result<Vec<Vec<u8>>, String> {
    let mut reader = WavReader::open(wav_file_path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let total_samples = reader.duration() * spec.channels as u32;
    let bytes_per_sample = (spec.bits_per_sample / 8) as u32;
    let total_bytes = total_samples * bytes_per_sample;

    if total_bytes <= MAX_CHUNK_SIZE_BYTES {
        return fs::read(wav_file_path)
            .map(|data| vec![data])
            .map_err(|e| e.to_string());
    }

    let samples_per_chunk = MAX_CHUNK_SIZE_BYTES / bytes_per_sample;
    let mut chunks = Vec::new();
    let mut current_chunk_samples = 0;
    let mut cursor = Cursor::new(Vec::new());
    let mut writer = WavWriter::new(&mut cursor, spec).map_err(|e| e.to_string())?;

    for sample_result in reader.samples::<i16>() {
        let sample = sample_result.map_err(|e| e.to_string())?;
        writer.write_sample(sample).map_err(|e| e.to_string())?;
        current_chunk_samples += 1;

        if current_chunk_samples >= samples_per_chunk {
            writer.finalize().map_err(|e| e.to_string())?;
            chunks.push(cursor.into_inner());
            cursor = Cursor::new(Vec::new());
            writer = WavWriter::new(&mut cursor, spec).map_err(|e| e.to_string())?;
            current_chunk_samples = 0;
        }
    }

    if current_chunk_samples > 0 {
        writer.finalize().map_err(|e| e.to_string())?;
        chunks.push(cursor.into_inner());
    }

    Ok(chunks)
}

async fn upload_chunk(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    language: &str,
    chunk_number: usize,
    chunk_data: Vec<u8>,
) -> Result<String, String> {
    let part = multipart::Part::bytes(chunk_data)
        .file_name(format!("chunk_{}.wav", chunk_number))
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let form = multipart::Form::new()
        .part("file", part)
        .text("model", model.to_string())
        .text("response_format", "json")
        .text("language", language.to_string());

    let res = client
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = res.status();
    if status.is_success() {
        let response_data: WhisperResponse = res.json().await.map_err(|e| e.to_string())?;
        Ok(response_data.text)
    } else {
        let err_text = res.text().await.unwrap_or_default();
        Err(format!("HTTP {}: {}", status, err_text))
    }
}

fn load_or_create_manifest(
    manifest_path: &str,
    wav_file_path: &str,
    model: &str,
    language: &str,
    chunk_count: usize,
) -> Result<WorkflowManifest, String> {
    if Path::new(manifest_path).exists() {
        if let Ok(raw) = fs::read_to_string(manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<WorkflowManifest>(&raw) {
                if manifest.chunks.len() == chunk_count
                    && manifest.source_file == wav_file_path
                    && manifest.model == model
                    && manifest.language == language
                {
                    return Ok(manifest);
                }
            }
        }
    }

    let chunks = (0..chunk_count)
        .map(|i| ChunkState {
            index: i + 1,
            status: "pending".to_string(),
            text: String::new(),
            attempts: 0,
            last_error: None,
        })
        .collect();

    Ok(WorkflowManifest {
        source_file: wav_file_path.to_string(),
        model: model.to_string(),
        language: language.to_string(),
        updated_at: Local::now().to_rfc3339(),
        chunks,
    })
}

fn save_manifest(manifest_path: &str, manifest: &WorkflowManifest) -> Result<(), String> {
    let mut copy = WorkflowManifest {
        source_file: manifest.source_file.clone(),
        model: manifest.model.clone(),
        language: manifest.language.clone(),
        updated_at: Local::now().to_rfc3339(),
        chunks: manifest.chunks.clone(),
    };
    copy.updated_at = Local::now().to_rfc3339();
    let raw = serde_json::to_string_pretty(&copy).map_err(|e| e.to_string())?;
    fs::write(manifest_path, raw).map_err(|e| e.to_string())
}

fn write_whisper_transcript(
    transcript_file_path: &str,
    wav_file_path: &str,
    final_text: &str,
) -> Result<(), String> {
    if final_text.trim().is_empty() {
        return Ok(());
    }

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let content = format!(
        "[{}] [File: {}]\n{}\n\n",
        timestamp,
        wav_file_path,
        final_text.trim()
    );
    fs::write(transcript_file_path, content).map_err(|e| e.to_string())
}
