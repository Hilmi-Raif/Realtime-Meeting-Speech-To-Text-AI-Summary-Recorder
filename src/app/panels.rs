use eframe::egui;

use super::options::AppStage;
use super::ui::*;
use super::RmsApp;

impl RmsApp {
    pub(super) fn draw_transcript_panel(&mut self, ui: &mut egui::Ui) {
        let theme = Theme::from_ui(ui);
        panel_frame(&theme).show(ui, |ui| {
            ui.horizontal(|ui| {
                panel_header(
                    ui,
                    &theme,
                    "Transcript",
                    "Live transcript and final results in one place.",
                    18.0,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    count_badge(ui, &theme, self.transcripts.len());
                });
            });

            ui.add_space(16.0);

            let available_height = (ui.available_height() - 2.0).max(320.0);

            let previous_item_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.y = 0.0;

            egui::Frame::none()
                .stroke(egui::Stroke::new(1.0, theme.border))
                .rounding(0.0)
                .show(ui, |ui| {
                    content_frame(&theme, theme.field_bg).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = previous_item_spacing;
                        ui.spacing_mut().scroll.bar_inner_margin = 0.0;
                        ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                        ui.spacing_mut().scroll.floating_width = 4.0;

                        let mut clip_rect = ui.max_rect();
                        clip_rect.min.x -= 2.0;
                        clip_rect.max.x += 12.0;
                        clip_rect.max.y -= 2.0;
                        ui.set_clip_rect(clip_rect);

                        egui::ScrollArea::vertical()
                            .id_source("transcript_scroll")
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .max_height(available_height)
                            .show(ui, |ui| {
                                egui::Frame::none()
                                    .inner_margin(egui::Margin {
                                        left: 0.0,
                                        right: 12.0,
                                        top: 14.0,
                                        bottom: 14.0,
                                    })
                                    .show(ui, |ui| {
                                        if self.transcripts.is_empty()
                                            && self.interim_transcript.is_empty()
                                            && self.groq_result.is_empty()
                                            && self.assemblyai_transcripts.is_empty()
                                            && self.summary_result.is_empty()
                                        {
                                            empty_transcript(ui, &theme);
                                        }

                                        let transcript_editable =
                                            self.stage.allows_transcript_edit();
                                        let mut transcript_changed = false;
                                        for (index, text) in self.transcripts.iter_mut().enumerate()
                                        {
                                            transcript_changed |= transcript_block(
                                                ui,
                                                &theme,
                                                index + 1,
                                                text,
                                                transcript_editable,
                                            );
                                            ui.add_space(6.0);
                                        }
                                        if transcript_changed {
                                            self.mark_transcript_dirty();
                                        }

                                        if !self.interim_transcript.is_empty() {
                                            interim_block(ui, &theme, &self.interim_transcript);
                                            ui.add_space(6.0);
                                        }

                                        if !self.groq_result.is_empty() {
                                            groq_block(ui, &theme, &mut self.groq_result);
                                            ui.add_space(6.0);
                                        }

                                        for text in &mut self.assemblyai_transcripts {
                                            assemblyai_block(ui, &theme, text);
                                            ui.add_space(6.0);
                                        }

                                        if !self.summary_result.is_empty() {
                                            summary_block(ui, &theme, &mut self.summary_result);
                                        }
                                    });
                            });
                    });
                });
        });
    }

    pub(super) fn draw_log_panel(&self, ui: &mut egui::Ui) {
        let theme = Theme::from_ui(ui);
        panel_frame(&theme).show(ui, |ui| {
            ui.horizontal(|ui| {
                panel_header(ui, &theme, "Workflow log", "Technical status log.", 16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.stage != AppStage::Init {
                        stage_badge(ui, &theme, self.stage);
                    }
                });
            });
            ui.add_space(16.0);

            let available_height = (ui.available_height() - 2.0).max(260.0);

            let previous_item_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.y = 0.0;

            egui::Frame::none()
                .stroke(egui::Stroke::new(1.0, theme.border))
                .rounding(0.0)
                .show(ui, |ui| {
                    content_frame(&theme, theme.log_bg).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = previous_item_spacing;
                        ui.spacing_mut().scroll.bar_inner_margin = 0.0;
                        ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                        ui.spacing_mut().scroll.floating_width = 4.0;

                        let mut clip_rect = ui.max_rect();
                        clip_rect.min.x -= 2.0;
                        clip_rect.max.x += 12.0;
                        clip_rect.max.y -= 2.0;
                        ui.set_clip_rect(clip_rect);

                        egui::ScrollArea::vertical()
                            .id_source("workflow_log_scroll")
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .max_height(available_height)
                            .show(ui, |ui| {
                                egui::Frame::none()
                                    .inner_margin(egui::Margin {
                                        left: 0.0,
                                        right: 12.0,
                                        top: 14.0,
                                        bottom: 14.0,
                                    })
                                    .show(ui, |ui| {
                                        for line in &self.logs {
                                            let is_error = line.contains("ERROR");
                                            ui.label(
                                                egui::RichText::new(line)
                                                    .monospace()
                                                    .size(11.5)
                                                    .color(if is_error {
                                                        theme.danger_text
                                                    } else {
                                                        theme.muted
                                                    }),
                                            );
                                        }
                                    });
                            });
                    });
                });
        });
    }
}
