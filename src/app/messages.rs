use crossbeam_channel::Sender;
use eframe::egui;
use tokio_util::sync::CancellationToken;

use super::options::AppStage;
use super::RmsApp;
use crate::audio::wasapi_loopback;

pub enum UiMessage {
    InterimTranscript(String),
    FinalTranscript(String),
    ReviewReady {
        wav_path: String,
    },
    GroqTranscript(String),
    Summary(String),
    Log(String),
    Error(String),
    Stage(String),
    CredentialCheck {
        provider: CredentialProvider,
        result: Result<String, String>,
    },
    Stopped,
}

#[derive(Clone, Copy)]
pub enum CredentialProvider {
    Deepgram,
    Groq,
    Summary,
}

impl RmsApp {
    pub(super) fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.ui_rx.try_recv() {
            match msg {
                UiMessage::InterimTranscript(text) => self.interim_transcript = text,
                UiMessage::FinalTranscript(text) => {
                    self.transcripts.push(text);
                    self.interim_transcript.clear();
                }
                UiMessage::ReviewReady { wav_path } => {
                    self.review_wav_path = Some(wav_path.clone());
                    self.cancel_token = None;
                    self.stage = AppStage::Review;
                    self.push_log(format!(
                        "Review: transcript is editable before AI processing; WAV ready at {}",
                        wav_path
                    ));
                }
                UiMessage::GroqTranscript(text) => {
                    self.groq_result = text;
                    self.push_log("Groq: transcript saved to UI".to_string());
                    self.start_summary_task(ctx.clone());
                }
                UiMessage::Summary(text) => {
                    self.summary_result = text;
                    self.push_log("Summary: text ready in UI".to_string());
                    self.stage = AppStage::Done;
                }
                UiMessage::Log(line) => self.push_log(line),
                UiMessage::Error(err) => self.push_error(err),
                UiMessage::Stage(stage) => self.stage = parse_stage(&stage),
                UiMessage::CredentialCheck { provider, result } => {
                    let status = match result {
                        Ok(message) => message,
                        Err(err) => format!("Failed: {err}"),
                    };
                    match provider {
                        CredentialProvider::Deepgram => self.deepgram_check_status = status,
                        CredentialProvider::Groq => self.groq_check_status = status,
                        CredentialProvider::Summary => self.summary_check_status = status,
                    }
                }
                UiMessage::Stopped => {
                    self.stage = if matches!(self.stage, AppStage::Error) {
                        AppStage::Error
                    } else if matches!(self.stage, AppStage::SummaryProcessing) {
                        AppStage::SummaryProcessing
                    } else {
                        AppStage::Done
                    };
                    self.interim_transcript.clear();
                    self.cancel_token = None;
                    let can_try_summary = self.options.enable_summary
                        && self.summary_result.trim().is_empty()
                        && !matches!(self.stage, AppStage::Error | AppStage::SummaryProcessing);
                    if can_try_summary {
                        self.start_summary_task(ctx.clone());
                    }
                }
            }
            self.close_window_if_ready(ctx);
            ctx.request_repaint();
        }
    }

    pub(super) fn push_log(&mut self, line: String) {
        self.logs.push(format!(
            "[{}] {}",
            chrono::Local::now().format("%H:%M:%S"),
            line
        ));
        if self.logs.len() > 500 {
            let drain_to = self.logs.len() - 500;
            self.logs.drain(0..drain_to);
        }
    }

    pub(super) fn push_error(&mut self, err: String) {
        self.logs.push(format!(
            "[{}] ERROR: {}",
            chrono::Local::now().format("%H:%M:%S"),
            err
        ));
        self.transcripts.push(format!("ERROR: {}", err));
        self.stage = AppStage::Error;
    }
}

fn parse_stage(stage: &str) -> AppStage {
    match stage {
        "Finalizing WAV" => AppStage::Finalizing,
        "Review transcript" => AppStage::Review,
        "Groq chunk/upload" => AppStage::GroqProcessing,
        "Summary generation" => AppStage::SummaryProcessing,
        "Done" => AppStage::Done,
        _ => AppStage::Recording,
    }
}

pub(super) fn emit(ui_tx: &Sender<UiMessage>, ctx: &egui::Context, msg: UiMessage) {
    let _ = ui_tx.send(msg);
    ctx.request_repaint();
}

pub(super) fn bridge_audio_events_to_ui(
    audio_event_rx: std::sync::mpsc::Receiver<wasapi_loopback::WasapiCaptureEvent>,
    ui_tx: Sender<UiMessage>,
    ctx: egui::Context,
    cancel_token: CancellationToken,
) {
    let ui_tx_for_error = ui_tx.clone();
    let ctx_for_error = ctx.clone();
    let cancel_for_error = cancel_token.clone();
    if let Err(err) = std::thread::Builder::new()
        .name("wasapi-ui-events".to_string())
        .spawn(move || {
            while !cancel_token.is_cancelled() {
                match audio_event_rx.recv_timeout(std::time::Duration::from_millis(250)) {
                    Ok(wasapi_loopback::WasapiCaptureEvent::SourceStarted { source_id, label }) => {
                        emit(
                            &ui_tx,
                            &ctx,
                            UiMessage::Log(format!("Audio: source {source_id} started ({label})")),
                        );
                    }
                    Ok(wasapi_loopback::WasapiCaptureEvent::SourceFailed {
                        source_id,
                        label,
                        error,
                    }) => {
                        emit(
                            &ui_tx,
                            &ctx,
                            UiMessage::Error(format!(
                                "Audio source {source_id} failed ({label}): {error}"
                            )),
                        );
                        cancel_token.cancel();
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
    {
        emit(
            &ui_tx_for_error,
            &ctx_for_error,
            UiMessage::Error(format!("Audio event thread failed to start: {err}")),
        );
        cancel_for_error.cancel();
    }
}
