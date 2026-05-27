use std::{fs, path::Path, sync::Arc};

use eframe::egui::{
    style::ScrollAnimation, Color32, FontData, FontDefinitions, FontFamily, FontId, Rangef,
    TextStyle, Visuals,
};

use crate::render::settings::MarkdownStyle;

const JAPANESE_FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
];

pub fn configure(ctx: &eframe::egui::Context, markdown: &MarkdownStyle) {
    configure_fonts(ctx);

    let mut style = (*ctx.style()).clone();
    style.visuals = Visuals::dark();
    style.scroll_animation = ScrollAnimation::new(720.0, Rangef::new(0.16, 0.42));
    style.spacing.item_spacing = eframe::egui::vec2(8.0, 6.0);
    style.spacing.button_padding = eframe::egui::vec2(10.0, 6.0);
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(markdown.heading_sizes[0], FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(markdown.body_font_size, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(markdown.code_font_size, FontFamily::Monospace),
    );
    style.visuals.panel_fill = markdown.colors.app_background;
    style.visuals.window_fill = markdown.colors.page_background;
    style.visuals.extreme_bg_color = markdown.colors.code_background;
    style.visuals.widgets.noninteractive.bg_fill = markdown.colors.page_background;
    style.visuals.widgets.inactive.bg_fill = markdown.colors.code_background;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(33, 38, 45);
    ctx.set_style(style);
}

fn configure_fonts(ctx: &eframe::egui::Context) {
    let mut fonts = FontDefinitions::default();
    insert_first_existing_font(
        &mut fonts,
        "system-ui",
        &[
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/SFNSDisplay.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ],
        FontFamily::Proportional,
    );
    insert_first_existing_font(
        &mut fonts,
        "system-japanese-ui",
        JAPANESE_FONT_PATHS,
        FontFamily::Proportional,
    );
    insert_first_existing_font(
        &mut fonts,
        "system-monospace",
        &[
            "/System/Library/Fonts/SFNSMono.ttf",
            "/System/Library/Fonts/Menlo.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        ],
        FontFamily::Monospace,
    );
    insert_first_existing_fallback_font(
        &mut fonts,
        "system-japanese",
        JAPANESE_FONT_PATHS,
        &[FontFamily::Monospace],
    );
    ctx.set_fonts(fonts);
}

fn insert_first_existing_font(
    fonts: &mut FontDefinitions,
    name: &str,
    paths: &[&str],
    family: FontFamily,
) {
    let Some(bytes) = paths.iter().find_map(|path| read_font(path)) else {
        return;
    };
    fonts
        .font_data
        .insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
    if let Some(family_fonts) = fonts.families.get_mut(&family) {
        family_fonts.insert(0, name.to_owned());
    }
}

fn insert_first_existing_fallback_font(
    fonts: &mut FontDefinitions,
    name: &str,
    paths: &[&str],
    families: &[FontFamily],
) {
    let Some(bytes) = paths.iter().find_map(|path| read_font(path)) else {
        return;
    };
    fonts
        .font_data
        .insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
    for family in families {
        if let Some(family_fonts) = fonts.families.get_mut(family) {
            family_fonts.push(name.to_owned());
        }
    }
}

fn read_font(path: &str) -> Option<Vec<u8>> {
    let path = Path::new(path);
    path.is_file().then(|| fs::read(path).ok()).flatten()
}
