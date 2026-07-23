use eframe::egui;

use super::options::{
    primary_action_for, secondary_actions_for, AppStage, PrimaryAction, SecondaryAction,
};
use super::ui::*;
use super::{
    RmsApp, DIALOG_HEIGHT_RATIO, DIALOG_MAX_HEIGHT, DIALOG_MAX_WIDTH, DIALOG_MIN_HEIGHT,
    DIALOG_MIN_WIDTH, DIALOG_WIDTH_RATIO, OVERLAY_ALPHA,
};

impl RmsApp {
    pub(super) fn draw_top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let theme = Theme::from_ui(ui);

        ui.horizontal(|ui| {
            app_title(ui, &theme);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.draw_actions(ui, ctx);
            });
        });
    }

    fn draw_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let theme = Theme::from_ui(ui);
        let ai_after_review_enabled =
            self.options.auto_groq || self.options.enable_assemblyai || self.options.enable_summary;
        let has_review_wav = self
            .review_wav_path
            .as_ref()
            .map(|p| std::path::Path::new(p).exists())
            .unwrap_or(false);
        let primary_action = primary_action_for(
            self.stage,
            ai_after_review_enabled,
            has_review_wav,
            self.failed_stage,
        );
        let secondary_actions = secondary_actions_for(
            self.stage,
            ai_after_review_enabled,
            has_review_wav,
            self.failed_stage,
        );
        let can_reset = matches!(
            self.stage,
            AppStage::Review | AppStage::Done | AppStage::Error | AppStage::Init
        ) && (!self.transcripts.is_empty()
            || !self.groq_result.is_empty()
            || !self.summary_result.is_empty()
            || self.logs.len() > 1
            || matches!(self.stage, AppStage::Done | AppStage::Error));

        if ui
            .add(app_button(
                "Settings",
                theme.text,
                theme.surface_muted,
                theme.border,
            ))
            .clicked()
        {
            self.show_settings = !self.show_settings;
        }

        if ui
            .add(app_button(
                "Output",
                theme.text,
                theme.surface_muted,
                theme.border,
            ))
            .clicked()
        {
            self.open_output_folder();
        }

        if ui
            .add_enabled(
                can_reset,
                app_button("Reset", theme.muted, theme.surface_muted, theme.border),
            )
            .clicked()
        {
            self.reset_session();
        }

        for action in secondary_actions {
            let button = match action {
                SecondaryAction::Done => app_button(
                    action.label(),
                    theme.success_text,
                    theme.success_soft,
                    theme.success_border,
                ),
                SecondaryAction::SkipGroq
                | SecondaryAction::SkipAssemblyAi
                | SecondaryAction::SkipSummary => app_button(
                    action.label(),
                    theme.warning_text,
                    theme.warning_soft,
                    theme.warning_border,
                ),
            };

            if ui.add(button).clicked() {
                match action {
                    SecondaryAction::SkipGroq => {
                        if let Some(failed) = self.failed_stage {
                            self.skip_ai_step(failed, ctx.clone());
                        }
                    }
                    SecondaryAction::SkipAssemblyAi => {
                        if let Some(failed) = self.failed_stage {
                            self.skip_ai_step(failed, ctx.clone());
                        }
                    }
                    SecondaryAction::SkipSummary => {
                        if let Some(failed) = self.failed_stage {
                            self.skip_ai_step(failed, ctx.clone());
                        }
                    }
                    SecondaryAction::Done => self.mark_done(),
                }
            }
        }

        if let Some(action) = primary_action {
            let button = match action {
                PrimaryAction::Stop | PrimaryAction::LoadingStop => app_button(
                    action.label(),
                    theme.danger_text,
                    theme.danger_soft,
                    theme.danger_border,
                ),
                PrimaryAction::Start
                | PrimaryAction::RunAi
                | PrimaryAction::RetryGroq
                | PrimaryAction::RetryAssemblyAi
                | PrimaryAction::RetrySummary
                | PrimaryAction::LoadingRunAi => {
                    app_button(action.label(), theme.panel_bg, theme.accent, theme.accent)
                }
            };

            if ui.add_enabled(!action.is_loading(), button).clicked() {
                match action {
                    PrimaryAction::Start => self.start_workflow(ctx.clone()),
                    PrimaryAction::Stop => self.stop_workflow(),
                    PrimaryAction::RunAi => self.run_ai_after_review(ctx.clone()),
                    PrimaryAction::RetryGroq
                    | PrimaryAction::RetryAssemblyAi
                    | PrimaryAction::RetrySummary => self.retry_failed_step(ctx.clone()),
                    PrimaryAction::LoadingStop | PrimaryAction::LoadingRunAi => {}
                }
            }
        }
    }

    fn draw_settings_content(&mut self, ui: &mut egui::Ui) -> bool {
        let mut should_close = false;
        let theme = Theme::from_dark(self.dark_mode);
        let options_before = self.options.clone();

        egui::Frame::none()
            .inner_margin(egui::Margin {
                left: 16.0,
                right: 16.0,
                top: 15.0,
                bottom: 9.0,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(egui::RichText::new("Settings").color(theme.text).size(18.0));
                        ui.add_space(0.0);
                        ui.label(
                            egui::RichText::new(&self.settings_autosave_status)
                                .size(12.0)
                                .color(if self.settings_autosave_status.contains("failed") {
                                    theme.danger_text
                                } else if self.settings_dirty {
                                    theme.warning_text
                                } else {
                                    theme.muted
                                }),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = egui::Button::new(egui::RichText::new("✖").size(16.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .frame(false);

                        if ui.add(close_btn).clicked() {
                            should_close = true;
                        }
                    });
                });
            });

        ui.spacing_mut().item_spacing.y = 0.0;

        ui.add(egui::Separator::default().horizontal().spacing(0.0));

        egui::Frame::none()
            .inner_margin(egui::Margin {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            })
            .show(ui, |ui| {
                ui.spacing_mut().scroll.bar_inner_margin = 0.0;
                ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                ui.spacing_mut().scroll.floating_width = 4.0;
                ui.spacing_mut().scroll.bar_outer_margin = -1.0;
                let available_height = ui.available_height();

                egui::ScrollArea::vertical()
                    .id_source("settings_scroll")
                    .auto_shrink([false, false])
                    .max_height(available_height)
                    .show_viewport(ui, |ui, viewport| {
                        let parent_clip = ui.clip_rect();
                        let clip_min = ui.max_rect().min + viewport.min.to_vec2();
                        let mut clip_max = ui.max_rect().min + viewport.max.to_vec2();
                        clip_max.y -= 0.7;
                        ui.set_clip_rect(
                            egui::Rect::from_min_max(clip_min, clip_max).intersect(parent_clip),
                        );
                        ui.set_min_width(viewport.width());

                        ui.spacing_mut().item_spacing.y = 8.0;
                        ui.spacing_mut().item_spacing.x = 0.0;

                        let margin = egui::Margin {
                            left: 16.0,
                            right: 16.0,
                            top: 16.0,
                            bottom: 16.0,
                        };
                        egui::Frame::none().inner_margin(margin).show(ui, |ui| {
                            ui.set_min_width((viewport.width() - 32.0).max(0.0));

                            panel_header(
                                ui,
                                &theme,
                                "Appearance",
                                "Choose how the recorder interface should look.",
                                16.0,
                            );
                            ui.add_space(16.0);

                            let dark_mode_before = self.dark_mode;
                            option_row(
                                ui,
                                &theme,
                                &mut self.dark_mode,
                                "Dark mode",
                                "Use the darker interface theme for low-light recording sessions.",
                            );
                            if self.dark_mode != dark_mode_before {
                                configure_style(ui.ctx(), self.dark_mode);
                                self.mark_settings_dirty();
                            }

                            section_divider(ui);

                            panel_header(
                                ui,
                                &theme,
                                "Session setup",
                                "Choose live transcription, after-review WAV transcription, and summary output.",
                                16.0,
                            );
                            ui.add_space(16.0);

                            option_row(
                                ui,
                                &theme,
                                &mut self.options.enable_deepgram,
                                "Deepgram realtime",
                                "Stream live transcript while recording.",
                            );
                            ui.add_space(10.0);

                            option_row(
                                ui,
                                &theme,
                                &mut self.options.auto_groq,
                                "Groq after review",
                                "Create Groq final from the saved WAV when Run AI is clicked.",
                            );
                            ui.add_space(10.0);
                            option_row(
                                ui,
                                &theme,
                                &mut self.options.enable_assemblyai,
                                "AssemblyAI after review",
                                "Create AssemblyAI final from the saved WAV when Run AI is clicked.",
                            );
                            ui.add_space(10.0);

                            option_row(
                                ui,
                                &theme,
                                &mut self.options.enable_summary,
                                "AI Summary TXT",
                                "Create a summary from reviewed transcript and batch transcripts when available.",
                            );
                            if !self.options.enable_deepgram
                                && !self.options.auto_groq
                                && !self.options.enable_assemblyai
                                && !self.options.enable_summary
                            {
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Recording only: no live transcript, after-review WAV transcript, or summary will be generated.",
                                    )
                                    .size(12.5)
                                    .color(theme.muted),
                                );
                            }

                            section_divider(ui);

                            ui.horizontal(|ui| {
                                panel_header(
                                    ui,
                                    &theme,
                                    "Audio devices",
                                    if self.config_notice.starts_with("Device list refreshed:") {
                                        &self.config_notice
                                    } else {
                                        "Select at least one microphone or system audio source."
                                    },
                                    16.0,
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(app_button(
                                                "Refresh",
                                                theme.muted,
                                                theme.surface_muted,
                                                theme.border,
                                            ))
                                            .clicked()
                                        {
                                            self.refresh_devices();
                                        }
                                    },
                                );
                            });
                            ui.add_space(10.0);

                            ui.label("Input Devices (Microphones, optional, Default available):");
                            device_checkbox_list(
                                ui,
                                &theme,
                                "input_scroll",
                                &self.input_devices,
                                &mut self.options.input_device_names,
                                self.options.output_device_names.len(),
                            );

                            ui.add_space(8.0);

                            ui.label("Output Devices (System audio, optional, Default available):");
                            device_checkbox_list(
                                ui,
                                &theme,
                                "output_scroll",
                                &self.output_devices,
                                &mut self.options.output_device_names,
                                self.options.input_device_names.len(),
                            );

                            section_divider(ui);

                            panel_header(
                                ui,
                                &theme,
                                "API & output",
                                "Configure credentials, model names, language, and output files.",
                                16.0,
                            );
                            ui.add_space(16.0);

                            egui::Grid::new("config_fields")
                                .num_columns(1)
                                .spacing([0.0, 8.0])
                                .show(ui, |ui| {
                                     let deepgram_status = self.deepgram_check_status.clone();
                                     if password_check_row(
                                         ui,
                                         &theme,
                                         "Deepgram API key",
                                         &mut self.options.deepgram_api_key,
                                         &deepgram_status,
                                     ) {
                                         self.check_deepgram_credentials(ui.ctx().clone());
                                     }
                                     ui.end_row();
                                     let assemblyai_status = self.assemblyai_check_status.clone();
                                     if password_check_row(
                                         ui,
                                         &theme,
                                         "AssemblyAI API key",
                                         &mut self.options.assemblyai_api_key,
                                         &assemblyai_status,
                                     ) {
                                         self.check_assemblyai_credentials(ui.ctx().clone());
                                     }
                                     ui.end_row();
                                    let groq_status = self.groq_check_status.clone();
                                    if password_check_row(
                                        ui,
                                        &theme,
                                        "Groq API key",
                                        &mut self.options.groq_api_key,
                                        &groq_status,
                                    ) {
                                        self.check_groq_credentials(ui.ctx().clone());
                                    }
                                    ui.end_row();
                                    let summary_status = self.summary_check_status.clone();
                                    if password_check_row(
                                        ui,
                                        &theme,
                                        "OpenAI-compatible API key",
                                        &mut self.options.summary_api_key,
                                        &summary_status,
                                    ) {
                                        self.check_summary_credentials(ui.ctx().clone());
                                    }
                                    ui.end_row();
                                    labeled_text_field(
                                        ui,
                                        &theme,
                                        "OpenAI-compatible base URL",
                                        &mut self.options.summary_base_url,
                                    );
                                    ui.end_row();
                                    labeled_text_field(
                                        ui,
                                        &theme,
                                        "Deepgram model",
                                        &mut self.options.deepgram_model,
                                    );
                                    ui.end_row();
                                     labeled_text_field(
                                         ui,
                                         &theme,
                                         "Groq model",
                                         &mut self.options.groq_model,
                                     );
                                     ui.end_row();
                                     labeled_text_field(
                                         ui,
                                         &theme,
                                         "AssemblyAI model",
                                         &mut self.options.assemblyai_model,
                                     );
                                     ui.end_row();
                                    labeled_text_field(
                                        ui,
                                        &theme,
                                        "Summary model",
                                        &mut self.options.summary_model,
                                    );
                                    ui.end_row();
                                    labeled_text_field(
                                        ui,
                                        &theme,
                                        "Language",
                                        &mut self.options.language,
                                    );
                                    ui.end_row();
                                    labeled_text_field(
                                        ui,
                                        &theme,
                                        "Output folder",
                                        &mut self.options.output_dir,
                                    );
                                    ui.end_row();
                                    labeled_text_field(
                                        ui,
                                        &theme,
                                        "Output prefix",
                                        &mut self.options.output_prefix,
                                    );
                                    ui.end_row();
                                });

                            ui.add_space(12.0);
                            multiline_text_field(
                                ui,
                                &theme,
                                "Summary prompt",
                                "Prompt for AI summary. Auto-saved to settings.json.",
                                &mut self.options.summary_prompt,
                                7,
                            );

                            if !self.config_notice.is_empty()
                                && !self.config_notice.starts_with("Device list refreshed:")
                            {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new(&self.config_notice)
                                        .size(12.0)
                                        .color(theme.muted),
                                );
                            }
                        }); // close inner frame
                    });
            });

        if self.options != options_before {
            self.mark_settings_dirty();
        }

        should_close
    }

    fn compute_dialog_size(ctx: &egui::Context) -> egui::Vec2 {
        let screen = ctx.screen_rect();
        let available_w = screen.width();
        let available_h = screen.height();

        let w = (available_w * DIALOG_WIDTH_RATIO).clamp(DIALOG_MIN_WIDTH, DIALOG_MAX_WIDTH);
        let h = (available_h * DIALOG_HEIGHT_RATIO).clamp(DIALOG_MIN_HEIGHT, DIALOG_MAX_HEIGHT);

        egui::vec2(w, h)
    }

    pub(super) fn draw_settings_dialog(&mut self, ctx: &egui::Context) {
        let screen_rect = ctx.screen_rect();
        let dialog_size = Self::compute_dialog_size(ctx);
        let dialog_pos = egui::pos2(
            (screen_rect.width() - dialog_size.x) / 2.0 + screen_rect.left(),
            (screen_rect.height() - dialog_size.y) / 2.0 + screen_rect.top(),
        );
        let dialog_rect = egui::Rect::from_min_size(dialog_pos, dialog_size);

        let _ = egui::Area::new(egui::Id::new("settings_overlay"))
            .fixed_pos(screen_rect.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(screen_rect.size(), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(OVERLAY_ALPHA));
                if response.clicked() {
                    let clicked_outside_dialog = response
                        .interact_pointer_pos()
                        .is_none_or(|pos| !dialog_rect.contains(pos));

                    if clicked_outside_dialog {
                        self.show_settings = false;
                    }
                }
            });

        let mut close_dialog = false;

        let theme = Theme::from_dark(self.dark_mode);

        let dialog_frame = egui::Frame::none()
            .fill(theme.app_bg)
            .stroke(egui::Stroke::new(1.0, theme.border))
            .rounding(0.0)
            .inner_margin(0.0);

        egui::Area::new(egui::Id::new("settings_dialog"))
            .fixed_pos(dialog_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_min_size(dialog_size);
                ui.set_max_size(dialog_size);

                dialog_frame.show(ui, |ui| {
                    ui.set_min_size(dialog_size);
                    ui.set_max_size(dialog_size);
                    ui.set_clip_rect(ui.max_rect());
                    close_dialog = self.draw_settings_content(ui);
                });
            });

        if close_dialog {
            self.show_settings = false;
        }
    }
}
