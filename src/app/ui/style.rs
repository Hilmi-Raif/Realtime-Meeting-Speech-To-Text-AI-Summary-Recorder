use eframe::egui;

#[derive(Clone, Copy)]
pub(crate) struct Theme {
    pub(crate) app_bg: egui::Color32,
    pub(crate) panel_bg: egui::Color32,
    pub(crate) surface_muted: egui::Color32,
    pub(crate) field_bg: egui::Color32,
    pub(crate) log_bg: egui::Color32,
    pub(crate) text: egui::Color32,
    pub(crate) muted: egui::Color32,
    pub(crate) border: egui::Color32,
    pub(crate) accent: egui::Color32,
    pub(crate) accent_soft: egui::Color32,
    pub(crate) accent_border: egui::Color32,
    pub(crate) success_soft: egui::Color32,
    pub(crate) success_text: egui::Color32,
    pub(crate) success_border: egui::Color32,
    pub(crate) warning_soft: egui::Color32,
    pub(crate) warning_text: egui::Color32,
    pub(crate) warning_border: egui::Color32,
    pub(crate) danger_soft: egui::Color32,
    pub(crate) danger_text: egui::Color32,
    pub(crate) danger_border: egui::Color32,
}

impl Theme {
    pub(crate) fn from_ctx(ctx: &egui::Context) -> Self {
        Self::from_dark(ctx.style().visuals.dark_mode)
    }

    pub(crate) fn from_ui(ui: &egui::Ui) -> Self {
        Self::from_dark(ui.visuals().dark_mode)
    }

    pub(crate) fn from_dark(dark: bool) -> Self {
        if dark {
            Self {
                app_bg: egui::Color32::from_rgb(15, 20, 28),
                panel_bg: egui::Color32::from_rgb(22, 29, 40),
                surface_muted: egui::Color32::from_rgb(30, 39, 53),
                field_bg: egui::Color32::from_rgb(18, 25, 35),
                log_bg: egui::Color32::from_rgb(18, 25, 35),
                text: egui::Color32::from_rgb(232, 238, 247),
                muted: egui::Color32::from_rgb(148, 163, 184),
                border: egui::Color32::from_rgb(51, 65, 85),
                accent: egui::Color32::from_rgb(110, 154, 255),
                accent_soft: egui::Color32::from_rgb(30, 47, 83),
                accent_border: egui::Color32::from_rgb(55, 84, 137),
                success_soft: egui::Color32::from_rgb(25, 64, 48),
                success_text: egui::Color32::from_rgb(134, 239, 172),
                success_border: egui::Color32::from_rgb(42, 94, 70),
                warning_soft: egui::Color32::from_rgb(74, 55, 24),
                warning_text: egui::Color32::from_rgb(251, 191, 36),
                warning_border: egui::Color32::from_rgb(120, 87, 32),
                danger_soft: egui::Color32::from_rgb(77, 34, 41),
                danger_text: egui::Color32::from_rgb(252, 165, 165),
                danger_border: egui::Color32::from_rgb(127, 49, 58),
            }
        } else {
            Self {
                app_bg: egui::Color32::from_rgb(246, 248, 252),
                panel_bg: egui::Color32::from_rgb(252, 253, 255),
                surface_muted: egui::Color32::from_rgb(238, 243, 250),
                field_bg: egui::Color32::from_rgb(248, 250, 254),
                log_bg: egui::Color32::from_rgb(243, 246, 251),
                text: egui::Color32::from_rgb(31, 41, 55),
                muted: egui::Color32::from_rgb(100, 116, 139),
                border: egui::Color32::from_rgb(222, 229, 239),
                accent: egui::Color32::from_rgb(52, 102, 213),
                accent_soft: egui::Color32::from_rgb(229, 237, 255),
                accent_border: egui::Color32::from_rgb(192, 210, 255),
                success_soft: egui::Color32::from_rgb(226, 246, 235),
                success_text: egui::Color32::from_rgb(22, 101, 52),
                success_border: egui::Color32::from_rgb(186, 230, 207),
                warning_soft: egui::Color32::from_rgb(255, 244, 214),
                warning_text: egui::Color32::from_rgb(146, 92, 16),
                warning_border: egui::Color32::from_rgb(245, 214, 146),
                danger_soft: egui::Color32::from_rgb(255, 232, 232),
                danger_text: egui::Color32::from_rgb(185, 28, 28),
                danger_border: egui::Color32::from_rgb(252, 190, 190),
            }
        }
    }
}

pub(crate) fn configure_style(ctx: &egui::Context, dark_mode: bool) {
    let mut style = (*ctx.style()).clone();
    style.visuals = if dark_mode {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let theme = Theme::from_dark(dark_mode);
    style.visuals.window_fill = theme.app_bg;
    style.visuals.panel_fill = theme.app_bg;
    style.visuals.faint_bg_color = theme.surface_muted;
    style.visuals.extreme_bg_color = theme.panel_bg;
    style.visuals.code_bg_color = theme.log_bg;

    style.visuals.window_shadow = egui::epaint::Shadow::NONE;
    style.visuals.window_stroke = egui::Stroke::NONE;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;

    style.visuals.override_text_color = Some(theme.text);
    style.visuals.hyperlink_color = theme.accent;
    style.visuals.warn_fg_color = theme.warning_text;
    style.visuals.error_fg_color = theme.danger_text;
    if dark_mode {
        style.visuals.selection.bg_fill = theme.accent_soft;
        style.visuals.selection.stroke = egui::Stroke::new(1.0, theme.text);
    } else {
        style.visuals.selection.bg_fill = theme.accent;
        style.visuals.selection.stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(248, 250, 254));
    }

    style.visuals.widgets.noninteractive.bg_fill = theme.panel_bg;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, theme.border);
    style.visuals.widgets.inactive.bg_fill = theme.field_bg;
    style.visuals.widgets.inactive.weak_bg_fill = theme.surface_muted;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, theme.border);
    style.visuals.widgets.hovered.bg_fill = theme.surface_muted;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme.accent_border);
    style.visuals.widgets.active.bg_fill = theme.accent_soft;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, theme.accent);
    style.visuals.widgets.open.bg_fill = theme.surface_muted;
    style.visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, theme.border);

    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(8.0);
    style.spacing.window_margin = egui::Margin::same(12.0);

    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(24.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(12.0, egui::FontFamily::Monospace),
        ),
    ]
    .into();

    ctx.set_style(style);
}
