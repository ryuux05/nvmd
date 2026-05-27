use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ViewerSettings {
    pub preset: MarkdownPreset,
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarkdownPreset {
    GitHub,
}

#[derive(Debug, Clone)]
pub struct MarkdownStyle {
    pub colors: MarkdownColors,
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
    pub table_stripe: Color32,
    pub warning_text: Color32,
    pub warning_background: Color32,
    pub warning_border: Color32,
    pub chrome_background: Color32,
    pub chrome_border: Color32,
}

impl Default for ViewerSettings {
    fn default() -> Self {
        let style = MarkdownStyle::github();
        Self {
            preset: MarkdownPreset::GitHub,
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
        }
    }
}

impl Default for MarkdownPreset {
    fn default() -> Self {
        Self::GitHub
    }
}

impl ViewerSettings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        let Ok(source) = fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&source).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = settings_path() else {
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
            MarkdownPreset::GitHub => MarkdownStyle::github(),
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
    pub fn github() -> Self {
        Self {
            colors: MarkdownColors {
                app_background: Color32::from_rgb(13, 17, 23),
                page_background: Color32::from_rgb(13, 17, 23),
                page_border: Color32::from_rgb(48, 54, 61),
                text: Color32::from_rgb(230, 237, 243),
                strong_text: Color32::from_rgb(230, 237, 243),
                muted_text: Color32::from_rgb(125, 133, 144),
                link: Color32::from_rgb(47, 129, 247),
                rule: Color32::from_rgb(48, 54, 61),
                code_text: Color32::from_rgb(230, 237, 243),
                code_background: Color32::from_rgb(22, 27, 34),
                inline_code_background: Color32::from_rgba_unmultiplied(110, 118, 129, 102),
                quote_text: Color32::from_rgb(125, 133, 144),
                quote_border: Color32::from_rgb(48, 54, 61),
                table_stripe: Color32::from_rgb(22, 27, 34),
                warning_text: Color32::from_rgb(210, 168, 255),
                warning_background: Color32::from_rgb(22, 27, 34),
                warning_border: Color32::from_rgb(48, 54, 61),
                chrome_background: Color32::from_rgb(1, 4, 9),
                chrome_border: Color32::from_rgb(48, 54, 61),
            },
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

pub fn settings_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "nvmd").map(|project| project.config_dir().join("viewer.toml"))
}

#[cfg(test)]
mod tests {
    use super::{MarkdownPreset, ViewerSettings};

    #[test]
    fn defaults_to_github_preset() {
        assert_eq!(ViewerSettings::default().preset, MarkdownPreset::GitHub);
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
