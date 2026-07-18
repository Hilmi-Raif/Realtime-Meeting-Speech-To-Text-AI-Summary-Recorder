use hound::{WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub async fn start_wav_recorder(
    mut record_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    sample_rate: u32,
    file_path: String,
    cancel_token: CancellationToken,
) -> Result<String, String> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(&file_path, spec).map_err(|e| {
        let msg = format!("Failed to create WAV file {}: {}", file_path, e);
        error!("{}", msg);
        msg
    })?;

    info!("WAV Recorder started. Saving audio to {}", file_path);

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Shutdown requested. Draining pending audio before finalizing WAV...");
                drain_pending_chunks(&mut record_rx, &mut writer).await;
                break;
            }
            chunk_opt = record_rx.recv() => {
                if let Some(chunk) = chunk_opt {
                    write_pcm_chunk(&mut writer, &chunk)?;
                } else {
                    break;
                }
            }
        }
    }

    writer.finalize().map_err(|e| {
        let msg = format!("Failed to finalize WAV file: {}", e);
        error!("{}", msg);
        msg
    })?;

    info!("WAV file successfully saved at {}", file_path);
    Ok(file_path)
}

async fn drain_pending_chunks(
    record_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    writer: &mut hound::WavWriter<BufWriter<File>>,
) {
    while let Ok(Some(chunk)) = timeout(Duration::from_millis(250), record_rx.recv()).await {
        if let Err(e) = write_pcm_chunk(writer, &chunk) {
            error!("Failed to drain WAV chunk: {}", e);
            break;
        }
    }
}

fn write_pcm_chunk(
    writer: &mut WavWriter<std::io::BufWriter<std::fs::File>>,
    chunk: &[u8],
) -> Result<(), String> {
    for bytes in chunk.chunks_exact(2) {
        let sample = i16::from_le_bytes([bytes[0], bytes[1]]);
        writer
            .write_sample(sample)
            .map_err(|e| format!("Failed to write sample to WAV: {}", e))?;
    }
    Ok(())
}
