use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const MAX_POLL_ATTEMPTS: u32 = 120;
const MAX_UPLOAD_ATTEMPTS: u32 = 3;

#[derive(Debug, Deserialize)]
struct UploadResponse {
    upload_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptRequest {
    pub audio_url: String,
    pub language_code: String,
    pub format_text: bool,
    pub speech_models: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptResponse {
    id: String,
    status: String,
    text: Option<String>,
    error: Option<String>,
}

pub fn build_transcript_request(audio_url: &str, model: &str, language: &str) -> TranscriptRequest {
    let normalized_model = if model.trim().is_empty() {
        "universal-2".to_string()
    } else {
        model.trim().to_string()
    };

    let normalized_language = match language.trim().to_lowercase().as_str() {
        "" | "id-id" | "ind" | "indonesian" | "bahasa" | "bahasa indonesia" => "id".to_string(),
        other => other.to_string(),
    };

    TranscriptRequest {
        audio_url: audio_url.to_string(),
        language_code: normalized_language,
        format_text: true,
        speech_models: vec![normalized_model],
    }
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
    if api_key.trim().is_empty() || api_key.trim() == "your_assemblyai_api_key_here" {
        return Err("AssemblyAI API Key is empty or default placeholder".to_string());
    }

    let path = Path::new(&wav_file_path);
    if !path.exists() {
        return Err(format!("WAV file not found: {}", wav_file_path));
    }

    log(format!("AssemblyAI: Uploading WAV file {}", wav_file_path));

    let client = reqwest::Client::builder()
        .timeout(UPLOAD_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let audio_bytes = tokio::fs::read(&wav_file_path)
        .await
        .map_err(|e| format!("Failed to read WAV file: {}", e))?;

    let upload_url = upload_wav(&client, &api_key, &audio_bytes, &cancel_token, &mut log).await?;
    log(format!("AssemblyAI: WAV uploaded successfully"));

    if cancel_token.is_cancelled() {
        return Err("AssemblyAI transcription cancelled by user".to_string());
    }

    log("AssemblyAI: Requesting batch transcription...".to_string());
    let req_body = build_transcript_request(&upload_url, &model, &language);
    let transcript_id = submit_transcript(&client, &api_key, &req_body).await?;

    log(format!(
        "AssemblyAI: Processing transcript ID {}",
        transcript_id
    ));
    let final_text =
        poll_transcript(&client, &api_key, &transcript_id, &cancel_token, &mut log).await?;

    write_assemblyai_transcript(&transcript_file_path, &wav_file_path, &final_text)?;
    log(format!(
        "AssemblyAI: Transcript saved to {}",
        transcript_file_path
    ));

    Ok(final_text)
}

async fn upload_wav<F>(
    client: &reqwest::Client,
    api_key: &str,
    bytes: &[u8],
    cancel_token: &CancellationToken,
    log: &mut F,
) -> Result<String, String>
where
    F: FnMut(String) + Send + 'static,
{
    let mut last_err = String::new();
    for attempt in 1..=MAX_UPLOAD_ATTEMPTS {
        if cancel_token.is_cancelled() {
            return Err("AssemblyAI transcription cancelled by user".to_string());
        }

        if attempt > 1 {
            log(format!(
                "AssemblyAI: Retrying upload (attempt {}/{})",
                attempt, MAX_UPLOAD_ATTEMPTS
            ));
            sleep(Duration::from_millis(1500 * attempt as u64)).await;
        }

        let response = match client
            .post("https://api.assemblyai.com/v2/upload")
            .header("Authorization", api_key.trim())
            .header("Content-Type", "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                last_err = format!("Upload HTTP error: {}", e);
                continue;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            last_err = format!("Upload failed (HTTP {}): {}", status, body);
            continue;
        }

        match response.json::<UploadResponse>().await {
            Ok(payload) => return Ok(payload.upload_url),
            Err(e) => {
                last_err = format!("Invalid upload JSON response: {}", e);
                continue;
            }
        }
    }

    Err(last_err)
}

async fn submit_transcript(
    client: &reqwest::Client,
    api_key: &str,
    req_body: &TranscriptRequest,
) -> Result<String, String> {
    let response = client
        .post("https://api.assemblyai.com/v2/transcript")
        .header("Authorization", api_key.trim())
        .header("Content-Type", "application/json")
        .json(req_body)
        .send()
        .await
        .map_err(|e| format!("Transcript submit HTTP error: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Transcript submit failed (HTTP {}): {}",
            status, body
        ));
    }

    let payload: TranscriptResponse = response
        .json()
        .await
        .map_err(|e| format!("Invalid submit JSON response: {}", e))?;

    Ok(payload.id)
}

async fn poll_transcript<F>(
    client: &reqwest::Client,
    api_key: &str,
    transcript_id: &str,
    cancel_token: &CancellationToken,
    log: &mut F,
) -> Result<String, String>
where
    F: FnMut(String) + Send + 'static,
{
    let url = format!("https://api.assemblyai.com/v2/transcript/{}", transcript_id);

    for attempt in 1..=MAX_POLL_ATTEMPTS {
        if cancel_token.is_cancelled() {
            return Err("AssemblyAI transcription cancelled by user".to_string());
        }

        let response = client
            .get(&url)
            .header("Authorization", api_key.trim())
            .send()
            .await
            .map_err(|e| format!("Poll HTTP error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Poll failed (HTTP {}): {}", status, body));
        }

        let payload: TranscriptResponse = response
            .json()
            .await
            .map_err(|e| format!("Invalid poll JSON response: {}", e))?;

        match payload.status.as_str() {
            "completed" => {
                return Ok(payload.text.unwrap_or_default());
            }
            "error" => {
                let err_msg = payload
                    .error
                    .unwrap_or_else(|| "Unknown AssemblyAI error".to_string());
                return Err(format!("AssemblyAI transcription error: {}", err_msg));
            }
            _ => {
                log(format!(
                    "AssemblyAI: Status '{}' (attempt {}/{})",
                    payload.status, attempt, MAX_POLL_ATTEMPTS
                ));
                sleep(POLL_INTERVAL).await;
            }
        }
    }

    Err("AssemblyAI polling timed out after 6 minutes".to_string())
}

fn write_assemblyai_transcript(
    transcript_file_path: &str,
    wav_file_path: &str,
    text: &str,
) -> Result<(), String> {
    if let Some(parent) = Path::new(transcript_file_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create transcript dir: {}", e))?;
        }
    }

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let content = format!("[{}] [File: {}]\n{}\n\n", timestamp, wav_file_path, text);

    fs::write(transcript_file_path, content).map_err(|e| {
        format!(
            "Failed to write transcript file {}: {}",
            transcript_file_path, e
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemblyai_batch_language_and_model() {
        let req = build_transcript_request("https://example.com/audio.wav", "universal-2", "id-ID");
        assert_eq!(req.language_code, "id");
        assert_eq!(req.speech_models[0], "universal-2");
    }
}
