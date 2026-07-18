use std::fs;

use serde::{Deserialize, Serialize};

use super::storage;
use crate::audio::wasapi_loopback::DEFAULT_DEVICE_NAME;
use crate::services::summary;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AppStage {
    Init,
    Recording,
    Finalizing,
    Review,
    GroqProcessing,
    SummaryProcessing,
    Done,
    Error,
}

impl AppStage {
    pub(super) fn is_busy(self) -> bool {
        matches!(
            self,
            AppStage::Recording
                | AppStage::Finalizing
                | AppStage::GroqProcessing
                | AppStage::SummaryProcessing
        )
    }

    pub(super) fn allows_transcript_edit(self) -> bool {
        matches!(
            self,
            AppStage::Recording | AppStage::Finalizing | AppStage::Review
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrimaryAction {
    Start,
    Stop,
    RunAi,
    LoadingStop,
    LoadingRunAi,
}

impl PrimaryAction {
    pub(super) fn label(self) -> &'static str {
        match self {
            PrimaryAction::Start => "Start",
            PrimaryAction::Stop => "Stop",
            PrimaryAction::RunAi => "Run AI",
            PrimaryAction::LoadingStop => "Stop",
            PrimaryAction::LoadingRunAi => "Run AI",
        }
    }

    pub(super) fn is_loading(self) -> bool {
        matches!(
            self,
            PrimaryAction::LoadingStop | PrimaryAction::LoadingRunAi
        )
    }
}

pub(super) fn primary_action_for(
    stage: AppStage,
    ai_after_review_enabled: bool,
) -> Option<PrimaryAction> {
    match stage {
        AppStage::Init | AppStage::Done | AppStage::Error => Some(PrimaryAction::Start),
        AppStage::Recording => Some(PrimaryAction::Stop),
        AppStage::Review if ai_after_review_enabled => Some(PrimaryAction::RunAi),
        AppStage::Finalizing => Some(PrimaryAction::LoadingStop),
        AppStage::GroqProcessing | AppStage::SummaryProcessing => Some(PrimaryAction::LoadingRunAi),
        AppStage::Review => Some(PrimaryAction::Start),
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct PersistedUiOptions {
    schema_version: Option<u32>,
    enable_deepgram: Option<bool>,
    auto_groq: Option<bool>,
    enable_summary: Option<bool>,
    input_device_names: Option<Vec<String>>,
    output_device_names: Option<Vec<String>>,
    deepgram_api_key: Option<String>,
    groq_api_key: Option<String>,
    summary_api_key: Option<String>,
    summary_base_url: Option<String>,
    deepgram_model: Option<String>,
    groq_model: Option<String>,
    summary_model: Option<String>,
    language: Option<String>,
    output_dir: Option<String>,
    output_prefix: Option<String>,
    log_file_path: Option<String>,
    wav_file_path: Option<String>,
    groq_file_path: Option<String>,
    summary_file_path: Option<String>,
    summary_prompt: Option<String>,
    pub(super) dark_mode: Option<bool>,
}

#[derive(Clone, PartialEq)]
pub(super) struct WorkflowOptions {
    pub(super) enable_deepgram: bool,
    pub(super) auto_groq: bool,
    pub(super) enable_summary: bool,
    pub(super) input_device_names: Vec<String>,
    pub(super) output_device_names: Vec<String>,
    pub(super) deepgram_api_key: String,
    pub(super) groq_api_key: String,
    pub(super) summary_api_key: String,
    pub(super) summary_base_url: String,
    pub(super) deepgram_model: String,
    pub(super) groq_model: String,
    pub(super) summary_model: String,
    pub(super) language: String,
    pub(super) output_dir: String,
    pub(super) output_prefix: String,
    pub(super) log_file_path: String,
    pub(super) wav_file_path: String,
    pub(super) groq_file_path: String,
    pub(super) summary_file_path: String,
    pub(super) summary_prompt: String,
}

impl Default for WorkflowOptions {
    fn default() -> Self {
        let output_dir = storage::default_output_dir();
        let audio_dir = storage::output_audio_dir(&output_dir);
        let transcripts_dir = storage::output_transcripts_dir(&output_dir);

        Self {
            enable_deepgram: true,
            auto_groq: false,
            enable_summary: false,
            input_device_names: vec![DEFAULT_DEVICE_NAME.to_string()],
            output_device_names: vec![DEFAULT_DEVICE_NAME.to_string()],
            deepgram_api_key: String::new(),
            groq_api_key: String::new(),
            summary_api_key: String::new(),
            summary_base_url: "https://api.openai.com/v1".to_string(),
            deepgram_model: "nova-3".to_string(),
            groq_model: "whisper-large-v3".to_string(),
            summary_model: "gpt-4o-mini".to_string(),
            language: "id".to_string(),
            output_dir: output_dir.display().to_string(),
            output_prefix: String::new(),
            log_file_path: transcripts_dir
                .join("transcript_log.txt")
                .display()
                .to_string(),
            wav_file_path: audio_dir.join("record.wav").display().to_string(),
            groq_file_path: transcripts_dir
                .join("transcript_whisper.txt")
                .display()
                .to_string(),
            summary_file_path: transcripts_dir.join("summary.txt").display().to_string(),
            summary_prompt: summary::default_system_prompt().to_string(),
        }
    }
}

impl WorkflowOptions {
    pub(super) fn apply_persisted(&mut self, persisted: &PersistedUiOptions) {
        if let Some(value) = persisted.enable_deepgram {
            self.enable_deepgram = value;
        }
        if let Some(value) = persisted.auto_groq {
            self.auto_groq = value;
        }
        if let Some(value) = persisted.enable_summary {
            self.enable_summary = value;
        }
        if let Some(devices) = &persisted.input_device_names {
            self.input_device_names = devices.clone();
        }
        if let Some(devices) = &persisted.output_device_names {
            self.output_device_names = devices.clone();
        }
        apply_optional_string(
            &mut self.deepgram_api_key,
            persisted.deepgram_api_key.as_deref(),
        );
        apply_optional_string(&mut self.groq_api_key, persisted.groq_api_key.as_deref());
        apply_optional_string(
            &mut self.summary_api_key,
            persisted.summary_api_key.as_deref(),
        );
        apply_non_empty(
            &mut self.summary_base_url,
            persisted.summary_base_url.as_deref(),
        );
        apply_non_empty(
            &mut self.deepgram_model,
            persisted.deepgram_model.as_deref(),
        );
        apply_non_empty(&mut self.groq_model, persisted.groq_model.as_deref());
        apply_non_empty(&mut self.summary_model, persisted.summary_model.as_deref());
        apply_non_empty(&mut self.language, persisted.language.as_deref());
        apply_non_empty(&mut self.output_dir, persisted.output_dir.as_deref());
        apply_optional_string(&mut self.output_prefix, persisted.output_prefix.as_deref());
        apply_non_empty(&mut self.log_file_path, persisted.log_file_path.as_deref());
        apply_non_empty(&mut self.wav_file_path, persisted.wav_file_path.as_deref());
        apply_non_empty(
            &mut self.groq_file_path,
            persisted.groq_file_path.as_deref(),
        );
        apply_non_empty(
            &mut self.summary_file_path,
            persisted.summary_file_path.as_deref(),
        );
        apply_non_empty(
            &mut self.summary_prompt,
            persisted.summary_prompt.as_deref(),
        );
    }

    fn to_persisted(&self, dark_mode: bool) -> PersistedUiOptions {
        PersistedUiOptions {
            schema_version: Some(1),
            enable_deepgram: Some(self.enable_deepgram),
            auto_groq: Some(self.auto_groq),
            enable_summary: Some(self.enable_summary),
            input_device_names: Some(self.input_device_names.clone()),
            output_device_names: Some(self.output_device_names.clone()),
            deepgram_api_key: Some(self.deepgram_api_key.clone()),
            groq_api_key: Some(self.groq_api_key.clone()),
            summary_api_key: Some(self.summary_api_key.clone()),
            summary_base_url: Some(self.summary_base_url.clone()),
            deepgram_model: Some(self.deepgram_model.clone()),
            groq_model: Some(self.groq_model.clone()),
            summary_model: Some(self.summary_model.clone()),
            language: Some(self.language.clone()),
            output_dir: Some(self.output_dir.clone()),
            output_prefix: Some(self.output_prefix.clone()),
            log_file_path: Some(self.log_file_path.clone()),
            wav_file_path: Some(self.wav_file_path.clone()),
            groq_file_path: Some(self.groq_file_path.clone()),
            summary_file_path: Some(self.summary_file_path.clone()),
            summary_prompt: Some(self.summary_prompt.clone()),
            dark_mode: Some(dark_mode),
        }
    }
}

fn apply_non_empty(target: &mut String, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        *target = value.to_string();
    }
}

fn apply_optional_string(target: &mut String, value: Option<&str>) {
    if let Some(value) = value {
        *target = value.trim().to_string();
    }
}

pub(super) fn load_persisted_options() -> Option<PersistedUiOptions> {
    let raw = fs::read_to_string(storage::settings_file_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

pub(super) fn save_persisted_options(
    options: &WorkflowOptions,
    dark_mode: bool,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(&options.to_persisted(dark_mode))
        .map_err(|e| format!("Serialize UI settings failed: {e}"))?;
    let settings_path = storage::settings_file_path();
    if let Some(parent) = settings_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| format!("Create UI settings folder failed: {e}"))?;
    }
    fs::write(&settings_path, raw).map_err(|e| format!("Save UI settings failed: {e}"))
}

pub(super) fn normalized_output_dir(output_dir: &str) -> String {
    let trimmed = output_dir.trim();
    if trimmed.is_empty() {
        storage::default_output_dir().display().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn session_id() -> String {
    let now = chrono::Local::now();
    format!(
        "{}_{:03}",
        now.format("%Y%m%d_%H%M%S"),
        now.timestamp_subsec_millis()
    )
}

pub(super) fn session_output_path_in_dir(
    output_dir: &str,
    output_prefix: &str,
    session_id: &str,
    name: &str,
    extension: &str,
) -> String {
    let dir = output_dir.trim().trim_end_matches(['/', '\\']).trim();
    let default_dir;
    let dir = if dir.is_empty() {
        default_dir = storage::default_output_dir().display().to_string();
        default_dir.as_str()
    } else {
        dir
    };
    let prefix = output_prefix.trim();
    let filename = if prefix.is_empty() {
        format!("{}_{}.{}", session_id, name, extension)
    } else {
        format!("{}_{}_{}.{}", prefix, session_id, name, extension)
    };
    format!("{}/{}", dir, filename)
}

#[cfg(test)]
mod tests {
    use super::{primary_action_for, AppStage, PrimaryAction};

    #[test]
    fn primary_action_progresses_through_record_review_ai() {
        assert_eq!(
            primary_action_for(AppStage::Init, true),
            Some(PrimaryAction::Start)
        );
        assert_eq!(
            primary_action_for(AppStage::Recording, true),
            Some(PrimaryAction::Stop)
        );
        assert_eq!(
            primary_action_for(AppStage::Review, true),
            Some(PrimaryAction::RunAi)
        );
    }

    #[test]
    fn primary_action_hides_run_ai_when_no_ai_is_enabled() {
        assert_eq!(
            primary_action_for(AppStage::Review, false),
            Some(PrimaryAction::Start)
        );
    }

    #[test]
    fn primary_action_stays_visible_as_loading_during_busy_stages() {
        assert_eq!(
            primary_action_for(AppStage::Finalizing, true),
            Some(PrimaryAction::LoadingStop)
        );
        assert_eq!(
            primary_action_for(AppStage::GroqProcessing, true),
            Some(PrimaryAction::LoadingRunAi)
        );
        assert_eq!(
            primary_action_for(AppStage::SummaryProcessing, true),
            Some(PrimaryAction::LoadingRunAi)
        );
    }

    #[test]
    fn primary_action_has_stable_user_facing_labels() {
        assert_eq!(PrimaryAction::Start.label(), "Start");
        assert_eq!(PrimaryAction::Stop.label(), "Stop");
        assert_eq!(PrimaryAction::RunAi.label(), "Run AI");
        assert_eq!(PrimaryAction::LoadingStop.label(), "Stop");
        assert_eq!(PrimaryAction::LoadingRunAi.label(), "Run AI");
    }

    #[test]
    fn transcript_editing_is_only_allowed_before_ai_processing() {
        assert!(AppStage::Recording.allows_transcript_edit());
        assert!(AppStage::Finalizing.allows_transcript_edit());
        assert!(AppStage::Review.allows_transcript_edit());
        assert!(!AppStage::GroqProcessing.allows_transcript_edit());
        assert!(!AppStage::SummaryProcessing.allows_transcript_edit());
        assert!(!AppStage::Done.allows_transcript_edit());
    }
}
