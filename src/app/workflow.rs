use std::fs;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};

use eframe::egui;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

use super::messages::{bridge_audio_events_to_ui, emit, UiMessage};
use super::options::{
    normalized_output_dir, session_id, session_output_path_in_dir, AfterReviewTranscriptionMode,
    AppStage, FailedStage,
};
use super::{storage, RmsApp};
use crate::audio::analysis::{analyze_wav_activity, AudioActivityConfig};
use crate::audio::recorder;
use crate::audio::wasapi_loopback;
use crate::services::{assemblyai, deepgram, groq, summary};

impl RmsApp {
    pub(super) fn start_workflow(&mut self, ctx: egui::Context) {
        if self.stage.is_busy() {
            return;
        }

        self.stage = AppStage::Recording;
        self.transcript_dirty = false;
        self.transcript_last_edit_at = None;
        self.transcript_autosave_status = "Realtime transcript saved".to_string();
        self.transcripts.clear();
        self.assemblyai_transcripts.clear();
        self.logs.clear();
        self.interim_transcript.clear();
        self.groq_result.clear();
        self.summary_result.clear();
        self.review_wav_path = None;
        self.push_log("Workflow: start".to_string());

        let session_id = session_id();
        let output_dir = normalized_output_dir(&self.options.output_dir);
        if let Err(err) = fs::create_dir_all(&output_dir) {
            self.push_error(format!("Output folder error: {err}"));
            self.stage = AppStage::Error;
            return;
        }
        let audio_dir = storage::output_audio_dir(Path::new(&output_dir));
        let transcripts_dir = storage::output_transcripts_dir(Path::new(&output_dir));
        if let Err(err) = fs::create_dir_all(&audio_dir) {
            self.push_error(format!("Audio output folder error: {err}"));
            self.stage = AppStage::Error;
            return;
        }
        if let Err(err) = fs::create_dir_all(&transcripts_dir) {
            self.push_error(format!("Transcript output folder error: {err}"));
            self.stage = AppStage::Error;
            return;
        }
        self.options.output_dir = output_dir;
        self.options.wav_file_path = session_output_path_in_dir(
            &audio_dir.display().to_string(),
            &self.options.output_prefix,
            &session_id,
            "record",
            "wav",
        );
        self.options.log_file_path = session_output_path_in_dir(
            &transcripts_dir.display().to_string(),
            &self.options.output_prefix,
            &session_id,
            "deepgram",
            "txt",
        );
        self.options.groq_file_path = session_output_path_in_dir(
            &transcripts_dir.display().to_string(),
            &self.options.output_prefix,
            &session_id,
            "groq",
            "txt",
        );
        self.options.summary_file_path = session_output_path_in_dir(
            &transcripts_dir.display().to_string(),
            &self.options.output_prefix,
            &session_id,
            "summary",
            "txt",
        );
        self.push_log(format!("Session: {}", session_id));

        let options = self.options.clone();
        let deepgram_api_key = options.deepgram_api_key.clone();
        let groq_api_key = options.groq_api_key.clone();
        let log_file_path = options.log_file_path.clone();
        let wav_file_path = options.wav_file_path.clone();
        if options.auto_groq
            && (groq_api_key.is_empty() || groq_api_key == "your_groq_api_key_here")
        {
            self.push_error(
                "groq_api_key is empty or invalid; disable Auto Groq or fill settings.json"
                    .to_string(),
            );
            self.stage = AppStage::Error;
            return;
        }

        if options.enable_summary && !options.enable_deepgram && !options.auto_groq {
            self.push_error(
                "AI Summary needs Deepgram realtime or Auto Groq enabled so a transcript is available"
                    .to_string(),
            );
            self.stage = AppStage::Error;
            return;
        }

        if options.enable_summary
            && (options.summary_api_key.trim().is_empty()
                || options.summary_api_key == "your_openai_compatible_api_key_here")
        {
            self.push_error(
                "OPENAI_API_KEY is empty or invalid; disable AI Summary or fill settings.json"
                    .to_string(),
            );
            self.stage = AppStage::Error;
            return;
        }

        if options.input_device_names.is_empty() && options.output_device_names.is_empty() {
            self.push_error("Select at least one microphone or system audio device.".to_string());
            self.stage = AppStage::Error;
            return;
        }

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());
        let ui_tx = self.ui_tx.clone();
        let deepgram_final_seen = Arc::new(AtomicBool::new(false));

        let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(100);
        let (record_tx, record_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (audio_event_tx, audio_event_rx) = std::sync::mpsc::channel();

        let sample_rate = match wasapi_loopback::start_wasapi_loopback_capture(
            options.output_device_names.clone(),
            options.input_device_names.clone(),
            audio_tx,
            record_tx,
            audio_event_tx,
            cancel_token.clone(),
        ) {
            Ok(rate) => {
                self.push_log("Audio: Unified WASAPI loopback capture started".to_string());
                rate
            }
            Err(e) => {
                self.push_error(format!("WASAPI loopback error: {}", e));
                self.stage = AppStage::Error;
                self.cancel_token = None;
                return;
            }
        };

        bridge_audio_events_to_ui(
            audio_event_rx,
            self.ui_tx.clone(),
            ctx.clone(),
            cancel_token.clone(),
        );
        let ctx_for_workflow = ctx.clone();
        self.rt.spawn(async move {
            emit(
                &ui_tx,
                &ctx_for_workflow,
                UiMessage::Log("Recorder: start WAV task".to_string()),
            );
            let recorder_cancel = cancel_token.clone();
            let recorder_task = tokio::spawn(recorder::start_wav_recorder(
                record_rx,
                sample_rate,
                wav_file_path.clone(),
                recorder_cancel,
            ));

            let deepgram_task = if options.enable_deepgram {
                emit(
                    &ui_tx,
                    &ctx_for_workflow,
                    UiMessage::Log("Deepgram: realtime transcript enabled".to_string()),
                );
                let deepgram_cancel = cancel_token.clone();
                let ui_tx_clone = ui_tx.clone();
                let ctx_clone = ctx_for_workflow.clone();
                let deepgram_final_seen_clone = Arc::clone(&deepgram_final_seen);
                Some(tokio::spawn(deepgram::start_deepgram_stream(
                    deepgram::DeepgramStreamConfig {
                        audio_rx,
                        api_key: deepgram_api_key,
                        sample_rate,
                        model: options.deepgram_model.clone(),
                        language: options.language.clone(),
                        log_file_path,
                        cancel_token: deepgram_cancel,
                        final_transcript_seen: deepgram_final_seen_clone,
                        ui_tx: ui_tx_clone,
                        ctx: ctx_clone,
                    },
                )))
            } else {
                emit(
                    &ui_tx,
                    &ctx_for_workflow,
                    UiMessage::Log("Deepgram: disabled, record only".to_string()),
                );
                drop(audio_rx);
                None
            };

            cancel_token.cancelled().await;
            emit(
                &ui_tx,
                &ctx_for_workflow,
                UiMessage::Stage("Finalizing WAV".to_string()),
            );
            emit(
                &ui_tx,
                &ctx_for_workflow,
                UiMessage::Log("Workflow: stop requested, wait WAV finalize".to_string()),
            );

            let wav_result = match recorder_task.await {
                Ok(Ok(path)) => Ok(path),
                Ok(Err(err)) => Err(err),
                Err(err) => Err(format!("Recorder task join error: {}", err)),
            };

            if let Some(task) = deepgram_task {
                let _ = timeout(Duration::from_secs(5), task).await;
            }
            let wav_path = match wav_result {
                Ok(path) => path,
                Err(err) => {
                    emit(&ui_tx, &ctx_for_workflow, UiMessage::Error(err));
                    emit(&ui_tx, &ctx_for_workflow, UiMessage::Stopped);
                    return;
                }
            };

            emit(
                &ui_tx,
                &ctx_for_workflow,
                UiMessage::Log(format!("Recorder: saved {}", wav_path)),
            );
            emit(
                &ui_tx,
                &ctx_for_workflow,
                UiMessage::ReviewReady { wav_path },
            );
        });
    }

    pub(super) fn stop_workflow(&mut self) {
        if !matches!(self.stage, AppStage::Recording) {
            return;
        }

        self.push_log("UI: stop clicked".to_string());
        self.interim_transcript.clear();
        self.stage = AppStage::Finalizing;
        if let Some(token) = &self.cancel_token {
            token.cancel();
        }
    }

    pub(super) fn retry_failed_step(&mut self, ctx: egui::Context) {
        let failed = self.failed_stage;
        self.failed_stage = None;
        match failed {
            Some(FailedStage::Summary) => {
                self.push_log("[Summary] Retrying summary generation...".to_string());
                self.start_summary_task(ctx);
            }
            Some(FailedStage::AssemblyAi) => {
                self.push_log("[AssemblyAI] Retrying WAV transcription...".to_string());
                self.run_ai_after_review_internal(ctx, true);
            }
            Some(FailedStage::Groq) | None => {
                self.push_log("[Groq] Retrying WAV transcription...".to_string());
                self.run_ai_after_review_internal(ctx, false);
            }
        }
    }

    pub(super) fn run_ai_after_review(&mut self, ctx: egui::Context) {
        self.failed_stage = None;
        self.run_ai_after_review_internal(ctx, false);
    }

    fn run_ai_after_review_internal(&mut self, ctx: egui::Context, skip_groq: bool) {
        if !matches!(self.stage, AppStage::Review | AppStage::Error) {
            return;
        }

        if self.transcript_dirty {
            match self.save_realtime_transcript_edits() {
                Ok(()) => {
                    self.transcript_dirty = false;
                    self.transcript_last_edit_at = None;
                    self.transcript_autosave_status = format!(
                        "Realtime transcript saved {}",
                        chrono::Local::now().format("%H:%M:%S")
                    );
                }
                Err(err) => {
                    self.push_error(format!("Transcript save before AI failed: {err}"));
                    return;
                }
            }
        }

        let Some(wav_path) = self.review_wav_path.clone() else {
            self.push_error("Review WAV path is missing; start a new recording".to_string());
            return;
        };

        let options = self.options.clone();
        let ui_tx = self.ui_tx.clone();
        let ctx_for_workflow = ctx.clone();

        self.push_log("Review: AI processing requested".to_string());

        let mode = options.after_review_transcription_mode();
        if mode == AfterReviewTranscriptionMode::None || (skip_groq && !mode.runs_assemblyai()) {
            if options.enable_summary {
                self.start_summary_task(ctx);
            } else {
                self.stage = AppStage::Done;
                self.push_log("Review: no AI post-processing enabled".to_string());
            }
            return;
        }

        self.stage = AppStage::GroqProcessing;

        self.rt.spawn(async move {
            let should_run =
                match analyze_wav_activity(&wav_path, AudioActivityConfig::default()) {
                    Ok(activity) => {
                        emit(
                            &ui_tx,
                            &ctx_for_workflow,
                            UiMessage::Log(format!(
                                "Audio: active {}ms/{}ms, rms {:.1} dBFS, peak {}",
                                activity.active_ms,
                                activity.duration_ms,
                                activity.rms_dbfs,
                                activity.peak_amplitude
                            )),
                        );
                        if activity.is_active {
                            true
                        } else {
                            emit(
                                &ui_tx,
                                &ctx_for_workflow,
                                UiMessage::Log(
                                    "After-review transcription skipped because the WAV is silent or has no valid audio"
                                        .to_string(),
                                ),
                            );
                            false
                        }
                    }
                    Err(err) => {
                        emit(
                            &ui_tx,
                            &ctx_for_workflow,
                            UiMessage::Error(format!("Audio analysis error: {}", err)),
                        );
                        false
                    }
                };

            if should_run {
                if !skip_groq && mode.runs_groq() {
                    emit(
                        &ui_tx,
                        &ctx_for_workflow,
                        UiMessage::Stage("Groq chunk/upload".to_string()),
                    );
                    emit(
                        &ui_tx,
                        &ctx_for_workflow,
                        UiMessage::Log("[Groq] Starting WAV transcription...".to_string()),
                    );
                    let groq_cancel = CancellationToken::new();
                    let ui_for_log = ui_tx.clone();
                    let ctx_for_log = ctx_for_workflow.clone();
                    match groq::transcribe_wav_idempotent(
                        wav_path.clone(),
                        options.groq_file_path.clone(),
                        options.groq_api_key.clone(),
                        options.groq_model.clone(),
                        options.language.clone(),
                        groq_cancel,
                        move |line| emit(&ui_for_log, &ctx_for_log, UiMessage::Log(line)),
                    )
                    .await
                    {
                        Ok(text) => emit(&ui_tx, &ctx_for_workflow, UiMessage::GroqTranscript(text)),
                        Err(err) => {
                            emit(
                                &ui_tx,
                                &ctx_for_workflow,
                                UiMessage::GroqError(err),
                            );
                            return;
                        }
                    }
                }

                if mode.runs_assemblyai() {
                    emit(
                        &ui_tx,
                        &ctx_for_workflow,
                        UiMessage::Stage("AssemblyAI WAV upload".to_string()),
                    );
                    emit(
                        &ui_tx,
                        &ctx_for_workflow,
                        UiMessage::Log("[AssemblyAI] Starting WAV upload...".to_string()),
                    );
                    let assembly_cancel = CancellationToken::new();
                    let ui_for_log = ui_tx.clone();
                    let ctx_for_log = ctx_for_workflow.clone();
                    match assemblyai::transcribe_wav_idempotent(
                        wav_path.clone(),
                        options.assemblyai_file_path.clone(),
                        options.assemblyai_api_key.clone(),
                        options.assemblyai_model.clone(),
                        options.language.clone(),
                        assembly_cancel,
                        move |line| emit(&ui_for_log, &ctx_for_log, UiMessage::Log(line)),
                    )
                    .await
                    {
                        Ok(text) => emit(&ui_tx, &ctx_for_workflow, UiMessage::AssemblyAiFinalTranscript(text)),
                        Err(err) => {
                            emit(
                                &ui_tx,
                                &ctx_for_workflow,
                                UiMessage::AssemblyAiError(err),
                            );
                            return;
                        }
                    }
                }

                emit(&ui_tx, &ctx_for_workflow, UiMessage::BatchTranscriptionDone);
            } else {
                emit(&ui_tx, &ctx_for_workflow, UiMessage::Stopped);
            }
        });
    }

    pub(super) fn skip_ai_step(&mut self, stage: FailedStage, ctx: egui::Context) {
        match stage {
            FailedStage::Groq => {
                self.push_log("[Groq] Skipped by user.".to_string());
                self.stage = AppStage::Review;
                self.failed_stage = None;
                if self
                    .options
                    .after_review_transcription_mode()
                    .runs_assemblyai()
                {
                    self.run_ai_after_review(ctx);
                } else if self.options.enable_summary {
                    self.start_summary_task(ctx);
                } else {
                    self.stage = AppStage::Done;
                }
            }
            FailedStage::AssemblyAi => {
                self.push_log("[AssemblyAI] Skipped by user.".to_string());
                self.stage = AppStage::Review;
                self.failed_stage = None;
                if self.options.enable_summary {
                    self.start_summary_task(ctx);
                } else {
                    self.stage = AppStage::Done;
                }
            }
            FailedStage::Summary => {
                self.push_log("[Summary] Skipped by user.".to_string());
                self.stage = AppStage::Done;
                self.failed_stage = None;
            }
        }
    }

    pub(super) fn mark_done(&mut self) {
        self.stage = AppStage::Done;
        self.failed_stage = None;
        self.push_log("Workflow: marked as Done by user.".to_string());
    }

    pub(super) fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }

        if !self.stage.is_busy() {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if !self.close_after_busy {
            self.push_log(
                "UI: close requested, waiting for current workflow to finish safely".to_string(),
            );
        }
        self.close_after_busy = true;

        if matches!(self.stage, AppStage::Recording) {
            self.stop_workflow();
        }
    }

    pub(super) fn close_window_if_ready(&mut self, ctx: &egui::Context) {
        if self.close_after_busy && !self.stage.is_busy() {
            self.close_after_busy = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(super) fn reset_session(&mut self) {
        if self.stage.is_busy() {
            return;
        }

        self.stage = AppStage::Init;
        self.transcript_dirty = false;
        self.transcript_last_edit_at = None;
        self.transcript_autosave_status = "Realtime transcript saved".to_string();
        self.transcripts.clear();
        self.assemblyai_transcripts.clear();
        self.interim_transcript.clear();
        self.groq_result.clear();
        self.summary_result.clear();
        self.review_wav_path = None;
        self.logs.clear();
        self.push_log("Init: ready to start a new session".to_string());
    }

    pub(super) fn start_summary_task(&mut self, ctx: egui::Context) {
        if !self.options.enable_summary {
            return;
        }

        let deepgram_transcript = self
            .transcripts
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty() && !item.starts_with("ERROR:"))
            .collect::<Vec<_>>()
            .join("\n");
        let whisper_transcript = self.groq_result.trim().to_string();
        let assemblyai_transcript = self
            .assemblyai_transcripts
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty() && !item.starts_with("ERROR:"))
            .collect::<Vec<_>>()
            .join("\n");

        if deepgram_transcript.is_empty()
            && whisper_transcript.is_empty()
            && assemblyai_transcript.is_empty()
        {
            self.push_log(
                "Summary: skipped because Deepgram, Groq, and AssemblyAI transcripts are empty"
                    .to_string(),
            );
            return;
        }

        self.stage = AppStage::SummaryProcessing;
        self.push_log(
            "Summary: sending available transcripts to the OpenAI-compatible API".to_string(),
        );

        let api_key = self.options.summary_api_key.clone();
        let base_url = self.options.summary_base_url.clone();
        let model = self.options.summary_model.clone();
        let output_path = self.options.summary_file_path.clone();
        let system_prompt = self.options.summary_prompt.clone();
        let ui_tx = self.ui_tx.clone();
        let ctx_for_summary = ctx.clone();

        self.rt.spawn(async move {
            emit(
                &ui_tx,
                &ctx_for_summary,
                UiMessage::Stage("Summary generation".to_string()),
            );
            match summary::generate_summary_text(
                deepgram_transcript,
                whisper_transcript,
                assemblyai_transcript,
                system_prompt,
                api_key,
                base_url,
                model,
                output_path.clone(),
            )
            .await
            {
                Ok(summary_text) => {
                    emit(
                        &ui_tx,
                        &ctx_for_summary,
                        UiMessage::Log(format!("Summary: saved {}", output_path)),
                    );
                    emit(&ui_tx, &ctx_for_summary, UiMessage::Summary(summary_text));
                }
                Err(err) => emit(&ui_tx, &ctx_for_summary, UiMessage::SummaryError(err)),
            }
        });
    }
}
