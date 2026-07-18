use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

mod auto_save;
mod credentials;
mod devices;
mod messages;
mod options;
mod panels;
mod storage;
mod ui;
mod views;
mod workflow;

pub use messages::UiMessage;
use options::{load_persisted_options, save_persisted_options, AppStage, WorkflowOptions};
use ui::{configure_style, Theme};

const PANEL_GAP: f32 = 12.0;
const PANEL_PAD: f32 = 18.0;
const CONTENT_PAD: f32 = 14.0;
const LOG_WIDTH: f32 = 340.0;
const COMPACT_BREAKPOINT: f32 = 760.0;
const NARROW_TOP_BAR: f32 = 900.0;
const DIALOG_MAX_WIDTH: f32 = 550.0;
const DIALOG_MIN_WIDTH: f32 = 320.0;
const DIALOG_MAX_HEIGHT: f32 = 600.0;
const DIALOG_MIN_HEIGHT: f32 = 380.0;
const DIALOG_WIDTH_RATIO: f32 = 0.80;
const DIALOG_HEIGHT_RATIO: f32 = 0.75;
const OVERLAY_ALPHA: u8 = 120;
const BUTTON_WIDTH: f32 = 96.0;
const BUTTON_HEIGHT: f32 = 38.0;
const TRANSCRIPT_AUTOSAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(1500);
const SETTINGS_AUTOSAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(1000);
const RADIUS_SM: f32 = 8.0;
const RADIUS_MD: f32 = 10.0;
const RADIUS_XL: f32 = 14.0;
pub struct RmsApp {
    stage: AppStage,
    show_settings: bool,
    transcripts: Vec<String>,
    logs: Vec<String>,
    interim_transcript: String,
    groq_result: String,
    summary_result: String,
    review_wav_path: Option<String>,
    options: WorkflowOptions,
    input_devices: Vec<String>,
    output_devices: Vec<String>,
    config_notice: String,
    dark_mode: bool,
    settings_dirty: bool,
    settings_last_edit_at: Option<Instant>,
    settings_autosave_status: String,
    transcript_dirty: bool,
    transcript_last_edit_at: Option<Instant>,
    transcript_autosave_status: String,
    deepgram_check_status: String,
    groq_check_status: String,
    summary_check_status: String,
    close_after_busy: bool,
    ui_rx: Receiver<UiMessage>,
    ui_tx: Sender<UiMessage>,
    cancel_token: Option<CancellationToken>,
    rt: Arc<tokio::runtime::Runtime>,
}

impl RmsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let persisted_options = load_persisted_options();
        let dark_mode = persisted_options
            .as_ref()
            .and_then(|options| options.dark_mode)
            .unwrap_or_else(|| cc.egui_ctx.style().visuals.dark_mode);
        configure_style(&cc.egui_ctx, dark_mode);

        let mut options = WorkflowOptions::default();
        if let Some(persisted) = &persisted_options {
            options.apply_persisted(persisted);
        }

        let (ui_tx, ui_rx) = crossbeam_channel::unbounded();
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime"),
        );

        let mut app = Self {
            stage: AppStage::Init,
            show_settings: false,
            transcripts: Vec::new(),
            logs: vec!["Init: choose a mode, then click Start".to_string()],
            interim_transcript: String::new(),
            groq_result: String::new(),
            summary_result: String::new(),
            review_wav_path: None,
            options,
            input_devices: Vec::new(),
            output_devices: Vec::new(),
            config_notice: String::new(),
            dark_mode,
            settings_dirty: false,
            settings_last_edit_at: None,
            settings_autosave_status: "Settings saved".to_string(),
            transcript_dirty: false,
            transcript_last_edit_at: None,
            transcript_autosave_status: "Realtime transcript saved".to_string(),
            deepgram_check_status: "Unchecked".to_string(),
            groq_check_status: "Unchecked".to_string(),
            summary_check_status: "Unchecked".to_string(),
            close_after_busy: false,
            ui_rx,
            ui_tx,
            cancel_token: None,
            rt,
        };
        app.refresh_devices();
        app
    }
}

impl eframe::App for RmsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        configure_style(ctx, self.dark_mode);
        self.handle_close_request(ctx);
        self.handle_messages(ctx);
        self.maybe_autosave_transcript(ctx);
        self.maybe_autosave_settings(ctx);

        if self.show_settings && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_settings = false;
        }

        let theme = Theme::from_ctx(ctx);
        let available_width = ctx.available_rect().width();
        let compact = available_width < COMPACT_BREAKPOINT;
        let top_margin_x = if compact { CONTENT_PAD } else { PANEL_GAP };

        egui::TopBottomPanel::top("app_top_bar")
            .frame(
                egui::Frame::none()
                    .fill(theme.app_bg)
                    .inner_margin(egui::Margin::symmetric(top_margin_x, 14.0)),
            )
            .show(ctx, |ui| self.draw_top_bar(ui, ctx));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(theme.app_bg)
                    .inner_margin(if compact {
                        egui::Margin::symmetric(CONTENT_PAD, CONTENT_PAD)
                    } else {
                        egui::Margin {
                            left: PANEL_GAP,
                            right: PANEL_GAP,
                            top: PANEL_GAP,
                            bottom: PANEL_GAP,
                        }
                    }),
            )
            .show(ctx, |ui| {
                let avail_width = ui.available_width();

                if avail_width >= COMPACT_BREAKPOINT {
                    egui::SidePanel::right("log_panel")
                        .resizable(false)
                        .exact_width(LOG_WIDTH)
                        .frame(egui::Frame::none())
                        .show_separator_line(false)
                        .show_inside(ui, |ui| {
                            ui.add_space(0.0);
                            egui::Frame::none()
                                .inner_margin(egui::Margin {
                                    left: PANEL_GAP,
                                    right: 0.0,
                                    top: 0.0,
                                    bottom: 0.0,
                                })
                                .show(ui, |ui| {
                                    self.draw_log_panel(ui);
                                });
                        });

                    egui::CentralPanel::default()
                        .frame(egui::Frame::none())
                        .show_inside(ui, |ui| {
                            self.draw_transcript_panel(ui);
                        });
                } else {
                    egui::ScrollArea::vertical()
                        .id_source("compact_layout_scroll")
                        .show(ui, |ui| {
                            self.draw_transcript_panel(ui);
                            ui.add_space(PANEL_GAP);
                            self.draw_log_panel(ui);
                        });
                }
            });

        if self.show_settings {
            self.draw_settings_dialog(ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.settings_dirty {
            let _ = save_persisted_options(&self.options, self.dark_mode);
        }
        if let Some(token) = &self.cancel_token {
            token.cancel();
        }
    }
}
