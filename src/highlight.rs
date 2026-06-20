use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Highlighter {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        })
    }

    pub fn highlight(&self, code: &str, language: &str, dark: bool) -> Vec<(eframe::egui::Color32, String)> {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(language)
            .or_else(|| self.syntax_set.find_syntax_by_extension(language))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme_name = if dark { "base16-ocean.dark" } else { "InspiredGitHub" };
        let theme = match self.theme_set.themes.get(theme_name) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut out = Vec::new();

        for line in LinesWithEndings::from(code) {
            let ranges: Vec<(Style, &str)> = match highlighter.highlight_line(line, &self.syntax_set) {
                Ok(r) => r,
                Err(_) => return Vec::new(),
            };
            for (style, text) in ranges {
                if text.is_empty() {
                    continue;
                }
                let fg = style.foreground;
                let color = eframe::egui::Color32::from_rgb(fg.r, fg.g, fg.b);
                if let Some(last) = out.last_mut() {
                    let (last_color, last_text): &mut (eframe::egui::Color32, String) = last;
                    if *last_color == color {
                        last_text.push_str(text);
                        continue;
                    }
                }
                out.push((color, text.to_owned()));
            }
        }

        out
    }
}
