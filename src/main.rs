#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod logger;
mod services;

use app::RmsApp;
use std::io::Cursor;
use tracing::{info, warn};

const APP_ICON_ICO: &[u8] = include_bytes!("../assets/icon.ico");

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    info!("RMS AI Recorder started");

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1180.0, 720.0])
        .with_min_inner_size([940.0, 560.0]);
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "RMS AI Recorder",
        options,
        Box::new(|cc| {
            let app = RmsApp::new(cc);
            Box::new(app) as Box<dyn eframe::App>
        }),
    )
}

fn load_window_icon() -> Option<egui::IconData> {
    let icon_dir = match ico::IconDir::read(Cursor::new(APP_ICON_ICO)) {
        Ok(icon_dir) => icon_dir,
        Err(error) => {
            warn!("Failed to read window icon: {error}");
            return None;
        }
    };

    let Some(entry) = icon_dir
        .entries()
        .iter()
        .max_by_key(|entry| entry.width() * entry.height())
    else {
        warn!("Window icon file does not contain any images");
        return None;
    };

    let image = match entry.decode() {
        Ok(image) => image,
        Err(error) => {
            warn!("Failed to decode window icon: {error}");
            return None;
        }
    };

    Some(egui::IconData {
        rgba: image.rgba_data().to_vec(),
        width: image.width(),
        height: image.height(),
    })
}
