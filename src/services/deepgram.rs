use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{
    connect_async, tungstenite::http::header::HeaderValue, tungstenite::Message,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use url::Url;

use crate::app::UiMessage;
use crate::logger::log_transcript;
use crossbeam_channel::Sender;

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    channel: Option<Channel>,
    is_final: Option<bool>,
    err_code: Option<String>,
    err_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Channel {
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    transcript: String,
}

pub struct DeepgramStreamConfig {
    pub audio_rx: mpsc::Receiver<Vec<u8>>,
    pub api_key: String,
    pub sample_rate: u32,
    pub model: String,
    pub language: String,
    pub log_file_path: String,
    pub cancel_token: CancellationToken,
    pub final_transcript_seen: Arc<AtomicBool>,
    pub ui_tx: Sender<UiMessage>,
    pub ctx: eframe::egui::Context,
}

pub async fn start_deepgram_stream(config: DeepgramStreamConfig) {
    let DeepgramStreamConfig {
        mut audio_rx,
        api_key,
        sample_rate,
        model,
        language,
        log_file_path,
        cancel_token,
        final_transcript_seen,
        ui_tx,
        ctx,
    } = config;

    if api_key.trim().is_empty() || api_key == "your_deepgram_api_key_here" {
        emit(
            &ui_tx,
            &ctx,
            UiMessage::Error(
                "Deepgram API key is empty or invalid; realtime transcript skipped".to_string(),
            ),
        );
        drain_until_cancel_or_closed(&mut audio_rx, &cancel_token).await;
        return;
    }

    let base_url = "wss://api.deepgram.com/v1/listen";
    let url_str = format!(
        "{}?encoding=linear16&sample_rate={}&channels=1&model={}&language={}&interim_results=true&endpointing=100",
        base_url, sample_rate, model, language
    );

    let mut reconnect_attempt = 0_u32;
    loop {
        if cancel_token.is_cancelled() {
            break;
        }

        reconnect_attempt += 1;
        emit(
            &ui_tx,
            &ctx,
            UiMessage::Log(format!("Deepgram: connect attempt {}", reconnect_attempt)),
        );

        let request = match build_request(&url_str, &api_key) {
            Ok(request) => request,
            Err(err) => {
                emit(
                    &ui_tx,
                    &ctx,
                    UiMessage::Error(format!("Deepgram request error: {}", err)),
                );
                break;
            }
        };

        match connect_async(request).await {
            Ok((ws_stream, _)) => {
                reconnect_attempt = 0;
                emit(
                    &ui_tx,
                    &ctx,
                    UiMessage::Log(format!("Deepgram: connected model {}", model)),
                );
                let (mut write, mut read) = ws_stream.split();

                let log_file_clone = log_file_path.clone();
                let ui_tx_clone = ui_tx.clone();
                let ctx_clone = ctx.clone();
                let final_transcript_seen_clone = Arc::clone(&final_transcript_seen);
                let read_task = tokio::spawn(async move {
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => handle_deepgram_text(
                                &text,
                                &log_file_clone,
                                &ui_tx_clone,
                                &ctx_clone,
                                &final_transcript_seen_clone,
                            ),
                            Ok(Message::Close(_)) => break,
                            Err(e) => {
                                let _ = ui_tx_clone
                                    .send(UiMessage::Log(format!("Deepgram read error: {}", e)));
                                ctx_clone.request_repaint();
                                break;
                            }
                            _ => {}
                        }
                    }
                });

                let mut shutdown_requested = false;
                loop {
                    tokio::select! {
                        _ = cancel_token.cancelled() => {
                            let _ = write.send(Message::Binary(vec![])).await;
                            shutdown_requested = true;
                            break;
                        }
                        chunk_opt = audio_rx.recv() => {
                            if let Some(chunk) = chunk_opt {
                                if let Err(e) = write.send(Message::Binary(chunk)).await {
                                    emit(&ui_tx, &ctx, UiMessage::Log(format!("Deepgram send error: {}", e)));
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }

                if shutdown_requested {
                    emit(
                        &ui_tx,
                        &ctx,
                        UiMessage::Log("Deepgram: closing, wait final transcript".to_string()),
                    );
                    let _ = tokio::time::timeout(Duration::from_secs(3), read_task).await;
                    emit(
                        &ui_tx,
                        &ctx,
                        UiMessage::Log("Deepgram: closed safely".to_string()),
                    );
                    break;
                }

                read_task.abort();
                emit(
                    &ui_tx,
                    &ctx,
                    UiMessage::Log(
                        "Deepgram: dropped, retry while recorder keeps running".to_string(),
                    ),
                );
            }
            Err(e) => {
                emit(
                    &ui_tx,
                    &ctx,
                    UiMessage::Log(format!("Deepgram connect failed: {}", e)),
                );
            }
        }

        let backoff = 2_u64.pow(reconnect_attempt.min(5));
        tokio::select! {
            _ = cancel_token.cancelled() => break,
            _ = sleep(Duration::from_secs(backoff)) => {}
        }
    }

    info!("Deepgram task stopped");
}

fn build_request(
    url_str: &str,
    api_key: &str,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, String> {
    let url = Url::parse(url_str).map_err(|e| e.to_string())?;
    let mut request =
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(url)
            .map_err(|e| e.to_string())?;
    let auth_header =
        HeaderValue::from_str(&format!("Token {}", api_key)).map_err(|e| e.to_string())?;
    request.headers_mut().insert("Authorization", auth_header);
    Ok(request)
}

fn handle_deepgram_text(
    text: &str,
    log_file_path: &str,
    ui_tx: &Sender<UiMessage>,
    ctx: &eframe::egui::Context,
    final_transcript_seen: &AtomicBool,
) {
    let parsed = match serde_json::from_str::<DeepgramResponse>(text) {
        Ok(parsed) => parsed,
        Err(_) => {
            warn!("Unparsed Deepgram message: {}", text);
            return;
        }
    };

    if let Some(t) = &parsed.msg_type {
        if t == "Error" {
            let msg = format!(
                "Deepgram Error: {} - {}",
                parsed.err_code.unwrap_or_default(),
                parsed.err_msg.unwrap_or_default()
            );
            error!("{}", msg);
            emit(ui_tx, ctx, UiMessage::Error(msg));
            return;
        }
    }

    if let Some(channel) = parsed.channel {
        if let Some(alt) = channel.alternatives.first() {
            if alt.transcript.trim().is_empty() {
                return;
            }

            if parsed.is_final.unwrap_or(false) {
                final_transcript_seen.store(true, Ordering::SeqCst);
                log_transcript(&alt.transcript, log_file_path);
                emit(
                    ui_tx,
                    ctx,
                    UiMessage::FinalTranscript(alt.transcript.clone()),
                );
            } else {
                emit(
                    ui_tx,
                    ctx,
                    UiMessage::InterimTranscript(alt.transcript.clone()),
                );
            }
        }
    }
}

async fn drain_until_cancel_or_closed(
    audio_rx: &mut mpsc::Receiver<Vec<u8>>,
    cancel_token: &CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => break,
            item = audio_rx.recv() => {
                if item.is_none() {
                    break;
                }
            }
        }
    }
}

fn emit(ui_tx: &Sender<UiMessage>, ctx: &eframe::egui::Context, msg: UiMessage) {
    let _ = ui_tx.send(msg);
    ctx.request_repaint();
}
