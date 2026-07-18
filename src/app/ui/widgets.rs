use eframe::egui;

use super::style::Theme;
use crate::app::options::AppStage;
use crate::app::{BUTTON_HEIGHT, BUTTON_WIDTH, PANEL_PAD, RADIUS_MD, RADIUS_SM, RADIUS_XL};
use crate::audio::wasapi_loopback::DEFAULT_DEVICE_NAME;

pub(crate) fn app_title(ui: &mut egui::Ui, theme: &Theme) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new("RMS AI Recorder")
                .size(24.0)
                .strong()
                .color(theme.text),
        );
        ui.add_space(3.0);
        ui.label(
            egui::RichText::new("Realtime meeting speech to text and AI summaries.")
                .size(13.0)
                .color(theme.muted),
        );
    });
}

pub(crate) fn app_button(
    label: &'static str,
    text_color: egui::Color32,
    fill: egui::Color32,
    border: egui::Color32,
) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label)
            .strong()
            .size(13.0)
            .color(text_color),
    )
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, border))
    .rounding(egui::Rounding::same(RADIUS_SM))
    .min_size(egui::vec2(BUTTON_WIDTH, BUTTON_HEIGHT))
}

pub(crate) fn panel_header(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    subtitle: &str,
    title_size: f32,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(title_size)
                .strong()
                .color(theme.text),
        );
        ui.add_space(4.0);
        ui.label(egui::RichText::new(subtitle).size(12.5).color(theme.muted));
    });
}

pub(crate) fn section_divider(ui: &mut egui::Ui) {
    ui.add_space(20.0);
    ui.separator();
    ui.add_space(16.0);
}

pub(crate) fn count_badge(ui: &mut egui::Ui, theme: &Theme, count: usize) {
    let noun = if count == 1 { "item" } else { "items" };
    egui::Frame::none()
        .fill(theme.surface_muted)
        .stroke(egui::Stroke::new(1.0, theme.border))
        .rounding(egui::Rounding::same(RADIUS_SM))
        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("{} {}", count, noun))
                    .size(12.0)
                    .strong()
                    .color(theme.muted),
            );
        });
}

pub(crate) fn panel_frame(theme: &Theme) -> egui::Frame {
    egui::Frame::none()
        .fill(theme.panel_bg)
        .stroke(egui::Stroke::new(1.0, theme.border))
        .rounding(egui::Rounding::same(RADIUS_XL))
        .inner_margin(egui::Margin::same(PANEL_PAD))
}

pub(crate) fn content_frame(_theme: &Theme, fill: egui::Color32) -> egui::Frame {
    // keep the background separate so the outer border does not clip during scroll
    egui::Frame::none()
        .fill(fill)
        .rounding(0.0)
        .inner_margin(egui::Margin {
            left: 14.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        })
}

pub(crate) fn device_checkbox_list(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_source: &'static str,
    devices: &[String],
    selected_devices: &mut Vec<String>,
    other_selected_count: usize,
) {
    ui.scope(|ui| {
        let mut strict_parent_clip = ui.clip_rect();
        strict_parent_clip.max.y -= 0.8;
        ui.set_clip_rect(strict_parent_clip);

        egui::Frame::none()
            .fill(theme.field_bg)
            .stroke(egui::Stroke::new(1.0, theme.border))
            .rounding(0.0)
            .inner_margin(egui::Margin {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            })
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().scroll.bar_inner_margin = 0.0;
                ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                ui.spacing_mut().scroll.floating_width = 4.0;

                egui::ScrollArea::vertical()
                    .id_source(id_source)
                    .max_height(87.0)
                    .min_scrolled_height(87.0)
                    .auto_shrink([false, false])
                    .show_viewport(ui, |ui, viewport| {
                        let parent_clip = ui.clip_rect();
                        let clip_min = ui.max_rect().min + viewport.min.to_vec2();
                        let clip_max = ui.max_rect().min + viewport.max.to_vec2();
                        ui.set_clip_rect(
                            egui::Rect::from_min_max(clip_min, clip_max).intersect(parent_clip),
                        );

                        egui::Frame::none()
                            .inner_margin(egui::Margin {
                                left: 8.0,
                                right: 12.0,
                                top: 8.0,
                                bottom: 8.0,
                            })
                            .show(ui, |ui| {
                                let mut default_checked = selected_devices
                                    .iter()
                                    .any(|device| device == DEFAULT_DEVICE_NAME);
                                let default_is_last_selected = default_checked
                                    && selected_devices.len() + other_selected_count <= 1;
                                if ui
                                    .add_enabled(
                                        !default_is_last_selected,
                                        egui::Checkbox::new(
                                            &mut default_checked,
                                            DEFAULT_DEVICE_NAME,
                                        ),
                                    )
                                    .changed()
                                {
                                    if default_checked {
                                        if !selected_devices
                                            .iter()
                                            .any(|device| device == DEFAULT_DEVICE_NAME)
                                        {
                                            selected_devices.push(DEFAULT_DEVICE_NAME.to_string());
                                        }
                                    } else {
                                        selected_devices
                                            .retain(|device| device != DEFAULT_DEVICE_NAME);
                                    }
                                }

                                for dev in devices {
                                    let mut is_checked = selected_devices.contains(dev);
                                    let is_last_selected = is_checked
                                        && selected_devices.len() + other_selected_count <= 1;
                                    if ui
                                        .add_enabled(
                                            !is_last_selected,
                                            egui::Checkbox::new(&mut is_checked, dev),
                                        )
                                        .changed()
                                    {
                                        if is_checked {
                                            if !selected_devices.contains(dev) {
                                                selected_devices.push(dev.clone());
                                            }
                                        } else {
                                            selected_devices.retain(|x| x != dev);
                                        }
                                    }
                                }
                            });
                    });
            });
    });
}

pub(crate) fn option_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    value: &mut bool,
    title: &str,
    subtitle: &str,
) {
    egui::Frame::none()
        .fill(theme.surface_muted)
        .stroke(egui::Stroke::new(1.0, theme.border))
        .rounding(egui::Rounding::same(RADIUS_MD))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(value, "");
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .size(13.0)
                            .strong()
                            .color(theme.text),
                    );
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(subtitle).size(12.0).color(theme.muted));
                });
            });
        });
}

pub(crate) fn labeled_text_field(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    value: &mut String,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(12.0)
                .strong()
                .color(theme.muted),
        );
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(f32::INFINITY)
                .margin(egui::vec2(10.0, 8.0)),
        );
    });
}

pub(crate) fn password_check_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    value: &mut String,
    status: &str,
) -> bool {
    let mut clicked = false;
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(12.0)
                .strong()
                .color(theme.muted),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let button_gap = 8.0;
            let field_width = (ui.available_width() - BUTTON_WIDTH - button_gap).max(120.0);
            ui.add_sized(
                [field_width, BUTTON_HEIGHT],
                egui::TextEdit::singleline(value)
                    .password(true)
                    .desired_width(field_width)
                    .margin(egui::vec2(10.0, 8.0)),
            );
            ui.add_space(button_gap);
            if ui
                .add_sized(
                    [BUTTON_WIDTH, BUTTON_HEIGHT],
                    app_button("Check", theme.text, theme.surface_muted, theme.border),
                )
                .clicked()
            {
                clicked = true;
            }
        });
        ui.add_space(3.0);
        let color = if status.starts_with("OK:") {
            theme.success_text
        } else if status == "Checking..." {
            theme.warning_text
        } else if status.starts_with("Failed:") {
            theme.danger_text
        } else {
            theme.muted
        };
        ui.label(egui::RichText::new(status).size(11.5).color(color));
    });
    clicked
}

pub(crate) fn multiline_text_field(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    helper: &str,
    value: &mut String,
    rows: usize,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(12.0)
                .strong()
                .color(theme.muted),
        );
        ui.add_space(3.0);
        ui.label(egui::RichText::new(helper).size(11.5).color(theme.muted));
        ui.add_space(6.0);

        let usable_width = (ui.available_width() - 20.0).max(120.0);
        let approx_char_width = 7.2;
        let chars_per_row = (usable_width / approx_char_width).max(20.0) as usize;
        let rows = value
            .lines()
            .map(|line| line.chars().count().max(1).div_ceil(chars_per_row))
            .sum::<usize>()
            .max(rows);

        egui::Frame::none()
            .fill(theme.field_bg)
            .stroke(egui::Stroke::new(1.0, theme.border))
            .rounding(egui::Rounding::same(RADIUS_SM))
            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.visuals_mut().extreme_bg_color = theme.field_bg;
                ui.visuals_mut().widgets.inactive.bg_fill = theme.field_bg;
                ui.visuals_mut().widgets.hovered.bg_fill = theme.field_bg;
                ui.visuals_mut().widgets.active.bg_fill = theme.field_bg;
                ui.visuals_mut().widgets.open.bg_fill = theme.field_bg;

                ui.add(
                    egui::TextEdit::multiline(value)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(rows)
                        .frame(false)
                        .margin(egui::vec2(0.0, 0.0)),
                );
            });
    });
}

pub(crate) fn stage_badge(ui: &mut egui::Ui, theme: &Theme, stage: AppStage) {
    let (fill, color, border, label) = match stage {
        AppStage::Init => (
            theme.accent_soft,
            theme.accent,
            theme.accent_border,
            "Ready",
        ),
        AppStage::Recording => (
            theme.success_soft,
            theme.success_text,
            theme.success_border,
            "Recording",
        ),
        AppStage::Finalizing => (
            theme.warning_soft,
            theme.warning_text,
            theme.warning_border,
            "Finalizing",
        ),
        AppStage::Review => (
            theme.accent_soft,
            theme.accent,
            theme.accent_border,
            "Review",
        ),
        AppStage::GroqProcessing => (theme.accent_soft, theme.accent, theme.accent_border, "Groq"),
        AppStage::SummaryProcessing => (
            theme.accent_soft,
            theme.accent,
            theme.accent_border,
            "Summary",
        ),
        AppStage::Done => (
            theme.success_soft,
            theme.success_text,
            theme.success_border,
            "Done",
        ),
        AppStage::Error => (
            theme.danger_soft,
            theme.danger_text,
            theme.danger_border,
            "Error",
        ),
    };

    egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(RADIUS_SM))
        .inner_margin(egui::Margin::symmetric(12.0, 7.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).strong().size(12.5).color(color));
        });
}

pub(crate) fn empty_transcript(ui: &mut egui::Ui, theme: &Theme) {
    ui.vertical_centered(|ui| {
        ui.add_space(86.0);
        ui.label(
            egui::RichText::new("No transcript yet")
                .size(20.0)
                .strong()
                .color(theme.text),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Click Start when the standup or meeting begins.")
                .size(13.0)
                .color(theme.muted),
        );
    });
}

pub(crate) fn transcript_block(
    ui: &mut egui::Ui,
    theme: &Theme,
    index: usize,
    text: &mut String,
    editable: bool,
) -> bool {
    let mut changed = false;

    egui::Frame::none()
        .fill(theme.panel_bg)
        .stroke(egui::Stroke::new(1.0, theme.border))
        .rounding(egui::Rounding::same(RADIUS_MD))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            let status = if editable { "editable" } else { "locked" };
            ui.label(
                egui::RichText::new(format!("Realtime final #{index} — {status}"))
                    .size(11.5)
                    .strong()
                    .color(theme.muted),
            );
            ui.add_space(4.0);
            let response = ui.add(
                egui::TextEdit::multiline(text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .interactive(editable),
            );
            changed = editable && response.changed();
        });

    changed
}

pub(crate) fn interim_block(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let mut interim_text = text.to_string();

    egui::Frame::none()
        .fill(theme.accent_soft)
        .stroke(egui::Stroke::new(1.0, theme.accent_border))
        .rounding(egui::Rounding::same(RADIUS_MD))
        .inner_margin(egui::Margin {
            left: 16.0,
            right: 16.0,
            top: 8.0,
            bottom: 10.0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized([9.0, 9.0], egui::Spinner::new());
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.add_space(7.0);
                    ui.label(
                        egui::RichText::new("Live interim")
                            .size(11.5)
                            .strong()
                            .color(theme.accent),
                    );
                });
            });
            ui.add_space(-2.0);

            ui.visuals_mut().extreme_bg_color = theme.accent_soft;
            ui.visuals_mut().widgets.inactive.bg_fill = theme.accent_soft;
            ui.visuals_mut().widgets.hovered.bg_fill = theme.accent_soft;
            ui.visuals_mut().widgets.active.bg_fill = theme.accent_soft;
            ui.visuals_mut().widgets.open.bg_fill = theme.accent_soft;
            ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
            ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::NONE;
            ui.visuals_mut().widgets.active.bg_stroke = egui::Stroke::NONE;
            ui.visuals_mut().widgets.open.bg_stroke = egui::Stroke::NONE;

            ui.add(
                egui::TextEdit::multiline(&mut interim_text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .frame(false)
                    .interactive(false)
                    .margin(egui::vec2(0.0, 3.0)),
            );
        });
}

pub(crate) fn groq_block(ui: &mut egui::Ui, theme: &Theme, text: &mut String) {
    egui::Frame::none()
        .fill(theme.accent_soft)
        .stroke(egui::Stroke::new(1.0, theme.accent_border))
        .rounding(egui::Rounding::same(RADIUS_MD))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Groq final — read only")
                    .size(11.5)
                    .strong()
                    .color(theme.accent),
            );
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::multiline(text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .interactive(false),
            );
        });
}

pub(crate) fn summary_block(ui: &mut egui::Ui, theme: &Theme, text: &mut String) {
    egui::Frame::none()
        .fill(theme.success_soft)
        .stroke(egui::Stroke::new(1.0, theme.success_border))
        .rounding(egui::Rounding::same(RADIUS_MD))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("AI Summary TXT — read only")
                    .size(11.5)
                    .strong()
                    .color(theme.success_text),
            );
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::multiline(text)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .interactive(false),
            );
        });
}
