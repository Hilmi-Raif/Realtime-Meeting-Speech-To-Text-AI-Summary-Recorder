use std::fs;
use std::process::Command;

use super::options::normalized_output_dir;
use super::RmsApp;
use crate::audio::wasapi_loopback;

impl RmsApp {
    pub(super) fn refresh_devices(&mut self) {
        self.input_devices = wasapi_loopback::list_capture_devices().unwrap_or_default();
        self.output_devices = wasapi_loopback::list_render_devices().unwrap_or_default();
        self.config_notice = format!(
            "Device list refreshed: {} input, {} output",
            self.input_devices.len(),
            self.output_devices.len()
        );
    }

    pub(super) fn open_output_folder(&mut self) {
        let output_dir = normalized_output_dir(&self.options.output_dir);
        if let Err(err) = fs::create_dir_all(&output_dir) {
            self.push_error(format!("Output folder error: {err}"));
            return;
        }

        match Command::new("explorer").arg(&output_dir).spawn() {
            Ok(_) => self.push_log(format!("Output folder: opened {output_dir}")),
            Err(err) => self.push_error(format!("Open output folder failed: {err}")),
        }
    }
}
