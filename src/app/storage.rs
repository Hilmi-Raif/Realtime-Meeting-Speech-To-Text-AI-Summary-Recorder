use directories::BaseDirs;
use std::path::{Path, PathBuf};

const SETTINGS_FILE: &str = "settings.json";
const APP_DATA_DIR: &str = "rms-ai-recorder";

pub(super) fn settings_file_path() -> PathBuf {
    app_data_dir().join(SETTINGS_FILE)
}

#[cfg(test)]
pub(super) fn settings_file_path_in(config_dir: &Path) -> PathBuf {
    app_data_dir_in(config_dir).join(SETTINGS_FILE)
}

pub(super) fn default_output_dir() -> PathBuf {
    app_data_dir().join("outputs")
}

#[cfg(test)]
pub(super) fn default_output_dir_in(data_dir: &Path) -> PathBuf {
    app_data_dir_in(data_dir).join("outputs")
}

pub(super) fn output_audio_dir(output_dir: &Path) -> PathBuf {
    output_dir.join("audio")
}

pub(super) fn output_transcripts_dir(output_dir: &Path) -> PathBuf {
    output_dir.join("transcripts")
}

fn app_data_dir() -> PathBuf {
    BaseDirs::new()
        .map(|dirs| app_data_dir_in(dirs.data_local_dir()))
        .unwrap_or_else(|| PathBuf::from(APP_DATA_DIR))
}

fn app_data_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join(APP_DATA_DIR)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn settings_file_lives_under_app_data_dir() {
        assert_eq!(
            super::settings_file_path_in(Path::new("base/local")),
            Path::new("base/local")
                .join("rms-ai-recorder")
                .join("settings.json")
        );
    }

    #[test]
    fn default_outputs_live_under_app_data_dir() {
        assert_eq!(
            super::default_output_dir_in(Path::new("base/local")),
            Path::new("base/local")
                .join("rms-ai-recorder")
                .join("outputs")
        );
    }

    #[test]
    fn output_subfolders_split_audio_and_transcripts() {
        let output_dir = Path::new("base/local")
            .join("rms-ai-recorder")
            .join("outputs");

        assert_eq!(
            super::output_audio_dir(&output_dir),
            output_dir.join("audio")
        );
        assert_eq!(
            super::output_transcripts_dir(&output_dir),
            output_dir.join("transcripts")
        );
    }
}
