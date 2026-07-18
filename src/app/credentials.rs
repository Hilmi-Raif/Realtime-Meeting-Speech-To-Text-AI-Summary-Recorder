use eframe::egui;

use super::messages::{emit, CredentialProvider, UiMessage};
use super::RmsApp;
use crate::services::credentials;

impl RmsApp {
    pub(super) fn check_deepgram_credentials(&mut self, ctx: egui::Context) {
        self.deepgram_check_status = "Checking...".to_string();
        let api_key = self.options.deepgram_api_key.clone();
        let model = self.options.deepgram_model.clone();
        let language = self.options.language.clone();
        let ui_tx = self.ui_tx.clone();
        self.rt.spawn(async move {
            let result = credentials::check_deepgram(api_key, model, language).await;
            emit(
                &ui_tx,
                &ctx,
                UiMessage::CredentialCheck {
                    provider: CredentialProvider::Deepgram,
                    result,
                },
            );
        });
    }

    pub(super) fn check_groq_credentials(&mut self, ctx: egui::Context) {
        self.groq_check_status = "Checking...".to_string();
        let api_key = self.options.groq_api_key.clone();
        let model = self.options.groq_model.clone();
        let ui_tx = self.ui_tx.clone();
        self.rt.spawn(async move {
            let result = credentials::check_groq(api_key, model).await;
            emit(
                &ui_tx,
                &ctx,
                UiMessage::CredentialCheck {
                    provider: CredentialProvider::Groq,
                    result,
                },
            );
        });
    }

    pub(super) fn check_summary_credentials(&mut self, ctx: egui::Context) {
        self.summary_check_status = "Checking...".to_string();
        let api_key = self.options.summary_api_key.clone();
        let base_url = self.options.summary_base_url.clone();
        let model = self.options.summary_model.clone();
        let ui_tx = self.ui_tx.clone();
        self.rt.spawn(async move {
            let result = credentials::check_openai_compatible(api_key, base_url, model).await;
            emit(
                &ui_tx,
                &ctx,
                UiMessage::CredentialCheck {
                    provider: CredentialProvider::Summary,
                    result,
                },
            );
        });
    }
}
