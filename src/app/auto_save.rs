use eframe::egui;
use std::fs;
use std::time::Instant;

use super::options::save_persisted_options;
use super::{RmsApp, SETTINGS_AUTOSAVE_DEBOUNCE, TRANSCRIPT_AUTOSAVE_DEBOUNCE};

impl RmsApp {
    pub(super) fn mark_transcript_dirty(&mut self) {
        self.transcript_dirty = true;
        self.transcript_last_edit_at = Some(Instant::now());
        self.transcript_autosave_status = "Unsaved realtime transcript changes...".to_string();
    }

    pub(super) fn mark_settings_dirty(&mut self) {
        self.settings_dirty = true;
        self.settings_last_edit_at = Some(Instant::now());
        self.settings_autosave_status = "Saving settings...".to_string();
    }

    pub(super) fn maybe_autosave_settings(&mut self, ctx: &egui::Context) {
        if !self.settings_dirty {
            return;
        }

        let Some(last_edit_at) = self.settings_last_edit_at else {
            return;
        };

        let elapsed = last_edit_at.elapsed();
        if elapsed < SETTINGS_AUTOSAVE_DEBOUNCE {
            ctx.request_repaint_after(SETTINGS_AUTOSAVE_DEBOUNCE - elapsed);
            return;
        }

        match save_persisted_options(&self.options, self.dark_mode) {
            Ok(()) => {
                self.settings_dirty = false;
                self.settings_last_edit_at = None;
                self.settings_autosave_status =
                    format!("Settings saved {}", chrono::Local::now().format("%H:%M:%S"));
            }
            Err(err) => {
                self.settings_autosave_status = format!("Settings save failed: {err}");
                self.push_error(format!("Settings autosave failed: {err}"));
            }
        }
    }

    pub(super) fn maybe_autosave_transcript(&mut self, ctx: &egui::Context) {
        if !self.transcript_dirty {
            return;
        }

        let Some(last_edit_at) = self.transcript_last_edit_at else {
            return;
        };

        let elapsed = last_edit_at.elapsed();
        if elapsed < TRANSCRIPT_AUTOSAVE_DEBOUNCE {
            ctx.request_repaint_after(TRANSCRIPT_AUTOSAVE_DEBOUNCE - elapsed);
            return;
        }

        self.transcript_autosave_status = "Saving realtime transcript...".to_string();
        match self.save_realtime_transcript_edits() {
            Ok(()) => {
                self.transcript_dirty = false;
                self.transcript_last_edit_at = None;
                self.transcript_autosave_status = format!(
                    "Realtime transcript auto-saved {}",
                    chrono::Local::now().format("%H:%M:%S")
                );
            }
            Err(err) => {
                self.transcript_autosave_status = format!("Transcript save failed: {err}");
                self.push_error(format!("Transcript autosave failed: {err}"));
            }
        }
    }

    pub(super) fn save_realtime_transcript_edits(&self) -> Result<(), String> {
        let body = self
            .transcripts
            .iter()
            .enumerate()
            .map(|(index, text)| format!("Final #{}\n{}", index + 1, text.trim()))
            .collect::<Vec<_>>()
            .join("\n\n");

        let content = format!(
            "[Edited from UI: {}]\n\n{}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            body.trim()
        );

        fs::write(&self.options.log_file_path, content)
            .map_err(|e| format!("{}: {e}", self.options.log_file_path))
    }
}
