use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ViewerSettings {
    pub preset: MarkdownPreset,
    pub window_width: f32,
    pub window_height: f32,
    pub enable_mermaid: bool,
    pub font_scale: f32,
    pub page_max_width: f32,
    pub page_inner_margin: f32,
    pub body_font_size: f32,
    pub code_font_size: f32,
    pub line_height: f32,
    pub paragraph_gap: f32,
    pub list_marker_width: f32,
    pub code_margin: f32,
    pub quote_margin: f32,
    pub table_spacing_x: f32,
    pub table_spacing_y: f32,
    pub heading_sizes: [f32; 6],
    pub keys: KeyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct KeyConfig {
    pub scroll_down: String,
    pub scroll_up: String,
    pub palette: String,
    pub quit: String,
    pub toc: String,
    pub search: String,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            scroll_down: "j".to_owned(),
            scroll_up: "k".to_owned(),
            palette: ":".to_owned(),
            quit: "q".to_owned(),
            toc: "t".to_owned(),
            search: "/".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarkdownPreset {
    Dark,
    Light,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone)]
pub struct MarkdownStyle {
    pub colors: MarkdownColors,
    pub is_dark: bool,
    pub body_font_size: f32,
    pub code_font_size: f32,
    pub small_font_size: f32,
    pub heading_sizes: [f32; 6],
    pub page_max_width: f32,
    pub page_inner_margin: f32,
    pub line_height: f32,
    pub paragraph_gap: f32,
    pub list_marker_width: f32,
    pub list_item_min_height: f32,
    pub code_margin: i8,
    pub quote_margin: i8,
    pub table_spacing: eframe::egui::Vec2,
}

#[derive(Debug, Clone)]
pub struct MarkdownColors {
    pub app_background: Color32,
    pub page_background: Color32,
    pub page_border: Color32,
    pub text: Color32,
    pub strong_text: Color32,
    pub muted_text: Color32,
    pub link: Color32,
    pub rule: Color32,
    pub code_text: Color32,
    pub code_background: Color32,
    pub inline_code_background: Color32,
    pub quote_text: Color32,
    pub quote_border: Color32,
    pub quote_background: Color32,
    pub table_stripe: Color32,
    pub warning_text: Color32,
    pub warning_background: Color32,
    pub warning_border: Color32,
    pub chrome_background: Color32,
    pub chrome_border: Color32,
    // GFM alert block colors
    pub alert_note_border: Color32,
    pub alert_note_bg: Color32,
    pub alert_note_text: Color32,
    pub alert_tip_border: Color32,
    pub alert_tip_bg: Color32,
    pub alert_tip_text: Color32,
    pub alert_important_border: Color32,
    pub alert_important_bg: Color32,
    pub alert_important_text: Color32,
    pub alert_warning_border: Color32,
    pub alert_warning_bg: Color32,
    pub alert_warning_text: Color32,
    pub alert_caution_border: Color32,
    pub alert_caution_bg: Color32,
    pub alert_caution_text: Color32,
}

impl Default for ViewerSettings {
    fn default() -> Self {
        let style = MarkdownStyle::dark();
        Self {
            preset: MarkdownPreset::Dark,
            window_width: 1280.0,
            window_height: 900.0,
            enable_mermaid: true,
            font_scale: 1.0,
            page_max_width: style.page_max_width,
            page_inner_margin: style.page_inner_margin,
            body_font_size: style.body_font_size,
            code_font_size: style.code_font_size,
            line_height: style.line_height,
            paragraph_gap: style.paragraph_gap,
            list_marker_width: style.list_marker_width,
            code_margin: f32::from(style.code_margin),
            quote_margin: f32::from(style.quote_margin),
            table_spacing_x: style.table_spacing.x,
            table_spacing_y: style.table_spacing.y,
            heading_sizes: style.heading_sizes,
            keys: KeyConfig::default(),
        }
    }
}

impl Default for MarkdownPreset {
    fn default() -> Self {
        Self::Dark
    }
}

impl ViewerSettings {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        // Migrate from old viewer.toml if config.toml doesn't exist yet
        if !path.exists() {
            if let Some(old) = legacy_viewer_path() {
                if old.exists() {
                    let _ = fs::rename(&old, &path);
                }
            }
        }
        let Ok(source) = fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&source).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = config_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let source = toml::to_string_pretty(self).unwrap_or_else(|_| String::new());
        fs::write(path, source)
    }

    pub fn style(&self) -> MarkdownStyle {
        let mut style = match self.preset {
            MarkdownPreset::Light => MarkdownStyle::light(),
            _ => MarkdownStyle::dark(),
        };
        style.page_max_width = self.page_max_width.clamp(360.0, 1600.0);
        style.page_inner_margin = self.page_inner_margin.clamp(0.0, 96.0);
        style.body_font_size = self.body_font_size.clamp(10.0, 40.0);
        style.code_font_size = self.code_font_size.clamp(10.0, 40.0);
        style.small_font_size = (style.code_font_size - 1.6).clamp(9.0, 24.0);
        style.line_height = self.line_height.clamp(1.0, 2.4);
        style.paragraph_gap = self.paragraph_gap.clamp(0.0, 64.0);
        style.list_marker_width = self.list_marker_width.clamp(0.0, 64.0);
        style.code_margin = clamped_margin(self.code_margin);
        style.quote_margin = clamped_margin(self.quote_margin);
        style.table_spacing = eframe::egui::vec2(
            self.table_spacing_x.clamp(0.0, 64.0),
            self.table_spacing_y.clamp(0.0, 64.0),
        );
        style.heading_sizes = self.heading_sizes.map(|size| size.clamp(10.0, 40.0));
        let scale = self.font_scale.clamp(0.5, 3.0);
        style.body_font_size *= scale;
        style.code_font_size *= scale;
        style.small_font_size *= scale;
        style.heading_sizes = style.heading_sizes.map(|s| s * scale);
        style
    }

    pub fn reset_to_default(&mut self) {
        *self = Self::default();
    }
}

fn clamped_margin(value: f32) -> i8 {
    value.clamp(0.0, 96.0).round() as i8
}

impl MarkdownStyle {
    pub fn dark() -> Self {
        Self {
            colors: MarkdownColors {
                app_background: Color32::from_rgb(10, 14, 22),
                page_background: Color32::from_rgb(13, 18, 32),
                page_border: Color32::from_rgb(28, 40, 70),
                text: Color32::from_rgb(194, 208, 232),
                strong_text: Color32::from_rgb(222, 234, 255),
                muted_text: Color32::from_rgb(84, 104, 136),
                link: Color32::from_rgb(86, 162, 255),
                rule: Color32::from_rgb(24, 36, 62),
                code_text: Color32::from_rgb(194, 208, 232),
                code_background: Color32::from_rgb(8, 11, 20),
                inline_code_background: Color32::from_rgba_unmultiplied(70, 108, 180, 52),
                quote_text: Color32::from_rgb(140, 164, 200),
                quote_border: Color32::from_rgb(68, 104, 200),
                quote_background: Color32::from_rgb(13, 20, 38),
                table_stripe: Color32::from_rgb(11, 16, 28),
                warning_text: Color32::from_rgb(176, 132, 255),
                warning_background: Color32::from_rgb(16, 12, 30),
                warning_border: Color32::from_rgb(52, 36, 90),
                chrome_background: Color32::from_rgb(8, 9, 15),
                chrome_border: Color32::from_rgb(22, 30, 50),
                alert_note_border: Color32::from_rgb(56, 139, 253),
                alert_note_bg: Color32::from_rgb(10, 18, 38),
                alert_note_text: Color32::from_rgb(121, 174, 255),
                alert_tip_border: Color32::from_rgb(46, 160, 67),
                alert_tip_bg: Color32::from_rgb(10, 24, 14),
                alert_tip_text: Color32::from_rgb(80, 200, 100),
                alert_important_border: Color32::from_rgb(137, 87, 229),
                alert_important_bg: Color32::from_rgb(18, 12, 34),
                alert_important_text: Color32::from_rgb(180, 140, 255),
                alert_warning_border: Color32::from_rgb(210, 153, 34),
                alert_warning_bg: Color32::from_rgb(26, 22, 8),
                alert_warning_text: Color32::from_rgb(230, 186, 80),
                alert_caution_border: Color32::from_rgb(218, 54, 51),
                alert_caution_bg: Color32::from_rgb(28, 10, 10),
                alert_caution_text: Color32::from_rgb(240, 100, 90),
            },
            is_dark: true,
            body_font_size: 16.0,
            code_font_size: 13.6,
            small_font_size: 12.0,
            heading_sizes: [32.0, 24.0, 20.0, 16.0, 14.0, 13.6],
            page_max_width: 1012.0,
            page_inner_margin: 32.0,
            line_height: 1.5,
            paragraph_gap: 16.0,
            list_marker_width: 32.0,
            list_item_min_height: 24.0,
            code_margin: 16,
            quote_margin: 12,
            table_spacing: eframe::egui::vec2(18.0, 10.0),
        }
    }

    pub fn light() -> Self {
        Self {
            colors: MarkdownColors {
                app_background: Color32::from_rgb(246, 248, 250),
                page_background: Color32::from_rgb(255, 255, 255),
                page_border: Color32::from_rgb(208, 215, 222),
                text: Color32::from_rgb(36, 41, 47),
                strong_text: Color32::from_rgb(24, 28, 33),
                muted_text: Color32::from_rgb(101, 109, 118),
                link: Color32::from_rgb(9, 105, 218),
                rule: Color32::from_rgb(208, 215, 222),
                code_text: Color32::from_rgb(36, 41, 47),
                code_background: Color32::from_rgb(240, 242, 244),
                inline_code_background: Color32::from_rgba_unmultiplied(175, 184, 193, 60),
                quote_text: Color32::from_rgb(87, 96, 106),
                quote_border: Color32::from_rgb(9, 105, 218),
                quote_background: Color32::from_rgb(237, 246, 255),
                table_stripe: Color32::from_rgb(240, 242, 244),
                warning_text: Color32::from_rgb(104, 60, 180),
                warning_background: Color32::from_rgb(246, 240, 255),
                warning_border: Color32::from_rgb(200, 170, 240),
                chrome_background: Color32::from_rgb(240, 242, 244),
                chrome_border: Color32::from_rgb(208, 215, 222),
                alert_note_border: Color32::from_rgb(9, 105, 218),
                alert_note_bg: Color32::from_rgb(230, 242, 255),
                alert_note_text: Color32::from_rgb(9, 71, 148),
                alert_tip_border: Color32::from_rgb(26, 127, 55),
                alert_tip_bg: Color32::from_rgb(220, 245, 226),
                alert_tip_text: Color32::from_rgb(20, 100, 40),
                alert_important_border: Color32::from_rgb(130, 80, 215),
                alert_important_bg: Color32::from_rgb(240, 232, 255),
                alert_important_text: Color32::from_rgb(90, 50, 160),
                alert_warning_border: Color32::from_rgb(180, 130, 20),
                alert_warning_bg: Color32::from_rgb(255, 248, 220),
                alert_warning_text: Color32::from_rgb(130, 90, 10),
                alert_caution_border: Color32::from_rgb(200, 50, 47),
                alert_caution_bg: Color32::from_rgb(255, 232, 230),
                alert_caution_text: Color32::from_rgb(160, 30, 28),
            },
            is_dark: false,
            body_font_size: 16.0,
            code_font_size: 13.6,
            small_font_size: 12.0,
            heading_sizes: [32.0, 24.0, 20.0, 16.0, 14.0, 13.6],
            page_max_width: 1012.0,
            page_inner_margin: 32.0,
            line_height: 1.5,
            paragraph_gap: 16.0,
            list_marker_width: 32.0,
            list_item_min_height: 24.0,
            code_margin: 16,
            quote_margin: 12,
            table_spacing: eframe::egui::vec2(18.0, 10.0),
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "nvmd").map(|p| p.config_dir().join("config.toml"))
}

fn legacy_viewer_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "nvmd").map(|p| p.config_dir().join("viewer.toml"))
}


#[cfg(test)]
mod tests {
    use super::{MarkdownPreset, ViewerSettings};

    #[test]
    fn defaults_to_dark_preset() {
        assert_eq!(ViewerSettings::default().preset, MarkdownPreset::Dark);
    }

    #[test]
    fn github_preset_uses_expected_page_width() {
        let style = ViewerSettings::default().style();
        assert_eq!(style.page_max_width, 1012.0);
    }

    #[test]
    fn custom_settings_round_trip_through_toml() {
        let mut settings = ViewerSettings::default();
        settings.page_inner_margin = 48.0;
        settings.body_font_size = 18.0;

        let source = toml::to_string(&settings).expect("settings should serialize");
        let parsed: ViewerSettings = toml::from_str(&source).expect("settings should deserialize");

        assert_eq!(parsed.page_inner_margin, 48.0);
        assert_eq!(parsed.body_font_size, 18.0);
    }

    #[test]
    fn style_clamps_values_to_readable_bounds() {
        let mut settings = ViewerSettings::default();
        settings.page_max_width = 100.0;
        settings.page_inner_margin = 500.0;
        settings.body_font_size = 2.0;

        let style = settings.style();

        assert_eq!(style.page_max_width, 360.0);
        assert_eq!(style.page_inner_margin, 96.0);
        assert_eq!(style.body_font_size, 10.0);
    }
}
