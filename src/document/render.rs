use eframe::egui::{
    self, pos2,
    text::{LayoutJob, TextFormat},
    vec2, Align, Color32, FontFamily, FontId, Label, Rect, RichText, Stroke,
};

use crate::document::model::{plain_text, Block, Document, Inline};
use crate::input::NavigationState;
use crate::render::settings::MarkdownStyle;

pub fn render_document(
    ui: &mut egui::Ui,
    document: &mut Document,
    render_mermaid: bool,
    style: &MarkdownStyle,
    navigation: &mut NavigationState,
) {
    let mut mermaid_index = 0;
    let mermaid_count = count_mermaid_blocks(&document.blocks);
    let source_block = navigation.take_source_block();
    for (index, block) in document.blocks.iter_mut().enumerate() {
        if index > 0 {
            ui.add_space(block_top_gap(block, style));
        }
        if source_block == Some(index) {
            ui.scroll_to_cursor(Some(Align::TOP));
            let selected_mermaid = matches!(block, Block::Mermaid { .. }).then_some(mermaid_index);
            navigation.apply_synced_block_mode(selected_mermaid);
        }
        render_block(
            ui,
            block,
            render_mermaid,
            style,
            &mut mermaid_index,
            mermaid_count,
            navigation,
        );
    }
}

fn render_block(
    ui: &mut egui::Ui,
    block: &mut Block,
    render_mermaid: bool,
    style: &MarkdownStyle,
    mermaid_index: &mut usize,
    mermaid_count: usize,
    navigation: &mut NavigationState,
) {
    match block {
        Block::Heading { level, content, .. } => {
            let size = style.heading_sizes[usize::from((*level).saturating_sub(1)).min(5)];
            inline_label(
                ui,
                content,
                FontId::new(size, FontFamily::Proportional),
                style.colors.strong_text,
                true,
                style,
            );
            if *level <= 2 {
                ui.add_space(6.0);
                horizontal_rule(ui, style);
            }
        }
        Block::Paragraph { content } => {
            inline_label(
                ui,
                content,
                FontId::new(style.body_font_size, FontFamily::Proportional),
                style.colors.text,
                false,
                style,
            );
        }
        Block::CodeBlock { language, code } => {
            code_block(ui, language.as_deref(), code, None, style);
        }
        Block::List {
            ordered,
            start,
            items,
        } => {
            for (index, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}.", start.unwrap_or(1) + index as u64)
                } else if item.checked == Some(true) {
                    "[x]".to_owned()
                } else if item.checked == Some(false) {
                    "[ ]".to_owned()
                } else {
                    "-".to_owned()
                };
                ui.horizontal_top(|ui| {
                    ui.set_min_height(style.list_item_min_height);
                    ui.add_sized(
                        vec2(style.list_marker_width, style.list_item_min_height),
                        Label::new(
                            RichText::new(marker)
                                .font(FontId::new(style.body_font_size, FontFamily::Monospace))
                                .color(style.colors.muted_text),
                        ),
                    );
                    let text_width = ui.available_width().max(1.0);
                    ui.vertical(|ui| {
                        ui.set_width(text_width);
                        for block in &item.blocks {
                            render_block(
                                ui,
                                &mut block.clone(),
                                render_mermaid,
                                style,
                                mermaid_index,
                                mermaid_count,
                                navigation,
                            );
                        }
                    });
                });
            }
        }
        Block::Quote { blocks } => {
            egui::Frame::new()
                .fill(style.colors.page_background)
                .stroke(Stroke::new(4.0, style.colors.quote_border))
                .inner_margin(egui::Margin::same(style.quote_margin))
                .show(ui, |ui| {
                    ui.visuals_mut().override_text_color = Some(style.colors.quote_text);
                    for block in blocks {
                        render_block(
                            ui,
                            block,
                            render_mermaid,
                            style,
                            mermaid_index,
                            mermaid_count,
                            navigation,
                        );
                        ui.add_space(style.paragraph_gap * 0.5);
                    }
                });
        }
        Block::HorizontalRule => {
            ui.add_space(8.0);
            horizontal_rule(ui, style);
            ui.add_space(8.0);
        }
        Block::HtmlBlock { html } => {
            code_block(ui, Some("html"), html, Some("Raw HTML block"), style);
        }
        Block::Table {
            alignments: _,
            header,
            rows,
        } => {
            table_block(ui, header, rows, style);
        }
        Block::FootnoteDefinition { label, blocks } => {
            ui.label(
                RichText::new(format!("[^{label}]"))
                    .font(FontId::new(style.code_font_size, FontFamily::Monospace))
                    .color(style.colors.muted_text),
            );
            for block in blocks {
                render_block(
                    ui,
                    block,
                    render_mermaid,
                    style,
                    mermaid_index,
                    mermaid_count,
                    navigation,
                );
            }
        }
        Block::DefinitionList { items } => {
            for item in items {
                ui.label(
                    RichText::new(plain_text(&item.term))
                        .size(style.body_font_size)
                        .strong()
                        .color(style.colors.strong_text),
                );
                for blocks in &item.definitions {
                    ui.indent("definition-list-item", |ui| {
                        for block in blocks {
                            render_block(
                                ui,
                                &mut block.clone(),
                                render_mermaid,
                                style,
                                mermaid_index,
                                mermaid_count,
                                navigation,
                            );
                        }
                    });
                }
            }
        }
        Block::MathBlock { expression } => {
            code_block(
                ui,
                Some("math"),
                expression,
                Some("Math rendering is not implemented"),
                style,
            );
        }
        Block::Mermaid {
            source,
            render_state,
        } => {
            if render_mermaid {
                let diagram_index = *mermaid_index;
                *mermaid_index += 1;
                crate::mermaid::widget::render_block(
                    ui,
                    diagram_index,
                    mermaid_count,
                    source,
                    render_state,
                    navigation,
                );
            } else {
                code_block(
                    ui,
                    Some("mermaid"),
                    source,
                    Some("Mermaid rendering disabled by --no-mermaid"),
                    style,
                );
            }
        }
    }
}

fn count_mermaid_blocks(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|block| match block {
            Block::Mermaid { .. } => 1,
            Block::Quote { blocks } | Block::FootnoteDefinition { blocks, .. } => {
                count_mermaid_blocks(blocks)
            }
            Block::List { items, .. } => items
                .iter()
                .map(|item| count_mermaid_blocks(&item.blocks))
                .sum(),
            Block::DefinitionList { items } => items
                .iter()
                .flat_map(|item| item.definitions.iter())
                .map(|blocks| count_mermaid_blocks(blocks))
                .sum(),
            _ => 0,
        })
        .sum()
}

#[derive(Clone, Copy)]
struct InlineStyle {
    strong: bool,
    italics: bool,
    strikethrough: bool,
    code: bool,
}

struct InlineLayout {
    job: LayoutJob,
    code_sections: Vec<u32>,
}

fn inline_label(
    ui: &mut egui::Ui,
    inlines: &[Inline],
    font_id: FontId,
    color: Color32,
    strong: bool,
    markdown: &MarkdownStyle,
) {
    let line_height = font_id.size * markdown.line_height;
    let layout = inline_layout(
        inlines,
        font_id,
        color,
        strong,
        ui.available_width(),
        line_height,
        markdown,
    );
    let (position, galley, _) = Label::new(layout.job).wrap().layout_in_ui(ui);
    paint_inline_code_backgrounds(
        ui,
        position,
        &galley,
        &layout.code_sections,
        markdown.colors.inline_code_background,
    );
    ui.painter().galley(position, galley, color);
}

fn inline_layout(
    inlines: &[Inline],
    font_id: FontId,
    color: Color32,
    strong: bool,
    wrap_width: f32,
    line_height: f32,
    markdown: &MarkdownStyle,
) -> InlineLayout {
    let mut job = LayoutJob::default();
    let mut code_sections = Vec::new();
    job.wrap.max_width = wrap_width.max(1.0);
    job.break_on_newline = true;
    append_inlines(
        &mut job,
        &mut code_sections,
        inlines,
        font_id,
        color,
        InlineStyle {
            strong,
            italics: false,
            strikethrough: false,
            code: false,
        },
        line_height,
        markdown,
    );
    InlineLayout { job, code_sections }
}

fn append_inlines(
    job: &mut LayoutJob,
    code_sections: &mut Vec<u32>,
    inlines: &[Inline],
    font_id: FontId,
    color: Color32,
    style: InlineStyle,
    line_height: f32,
    markdown: &MarkdownStyle,
) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => append_inline_text(
                job,
                code_sections,
                text,
                &font_id,
                color,
                style,
                line_height,
                markdown,
            ),
            Inline::Emphasis(children) => append_inlines(
                job,
                code_sections,
                children,
                font_id.clone(),
                color,
                InlineStyle {
                    italics: true,
                    ..style
                },
                line_height,
                markdown,
            ),
            Inline::Strong(children) => append_inlines(
                job,
                code_sections,
                children,
                font_id.clone(),
                color,
                InlineStyle {
                    strong: true,
                    ..style
                },
                line_height,
                markdown,
            ),
            Inline::Strikethrough(children) => append_inlines(
                job,
                code_sections,
                children,
                font_id.clone(),
                color,
                InlineStyle {
                    strikethrough: true,
                    ..style
                },
                line_height,
                markdown,
            ),
            Inline::Code(code) => append_inline_text(
                job,
                code_sections,
                code,
                &FontId::new(markdown.code_font_size, FontFamily::Monospace),
                markdown.colors.code_text,
                InlineStyle {
                    code: true,
                    ..style
                },
                line_height,
                markdown,
            ),
            Inline::Link { children, .. } => append_inlines(
                job,
                code_sections,
                children,
                font_id.clone(),
                markdown.colors.link,
                style,
                line_height,
                markdown,
            ),
            Inline::Image { alt, .. } => {
                append_inline_text(
                    job,
                    code_sections,
                    "![",
                    &font_id,
                    color,
                    style,
                    line_height,
                    markdown,
                );
                append_inlines(
                    job,
                    code_sections,
                    alt,
                    font_id.clone(),
                    color,
                    style,
                    line_height,
                    markdown,
                );
                append_inline_text(
                    job,
                    code_sections,
                    "]",
                    &font_id,
                    color,
                    style,
                    line_height,
                    markdown,
                );
            }
            Inline::Html(html) | Inline::Math(html) => {
                append_inline_text(
                    job,
                    code_sections,
                    html,
                    &font_id,
                    color,
                    style,
                    line_height,
                    markdown,
                );
            }
            Inline::SoftBreak => append_inline_text(
                job,
                code_sections,
                " ",
                &font_id,
                color,
                style,
                line_height,
                markdown,
            ),
            Inline::HardBreak => append_inline_text(
                job,
                code_sections,
                "\n",
                &font_id,
                color,
                style,
                line_height,
                markdown,
            ),
        }
    }
}

fn append_inline_text(
    job: &mut LayoutJob,
    code_sections: &mut Vec<u32>,
    text: &str,
    font_id: &FontId,
    color: Color32,
    style: InlineStyle,
    line_height: f32,
    markdown: &MarkdownStyle,
) {
    let line_color = if style.strong {
        markdown.colors.strong_text
    } else {
        color
    };
    let mut format = TextFormat::simple(font_id.clone(), line_color);
    format.italics = style.italics;
    if style.strikethrough {
        format.strikethrough = Stroke::new(1.0, line_color);
    }
    if style.code {
        format.valign = Align::Min;
        code_sections.push(job.sections.len() as u32);
    } else {
        format.line_height = Some(line_height);
    }
    job.append(text, 0.0, format);
}

fn paint_inline_code_backgrounds(
    ui: &egui::Ui,
    position: egui::Pos2,
    galley: &egui::Galley,
    code_sections: &[u32],
    color: Color32,
) {
    const PAD_X: f32 = 4.0;
    const PAD_Y: f32 = 2.0;
    const RADIUS: f32 = 4.0;

    for row in &galley.rows {
        let mut run: Option<Rect> = None;
        for glyph in &row.glyphs {
            if code_sections.contains(&glyph.section_index) {
                let top = glyph.pos.y - glyph.font_ascent - PAD_Y;
                let bottom = top + glyph.font_height + PAD_Y * 2.0;
                let glyph_rect =
                    Rect::from_min_max(pos2(glyph.pos.x, top), pos2(glyph.max_x(), bottom));
                run = Some(match run {
                    Some(rect) => rect.union(glyph_rect),
                    None => glyph_rect,
                });
            } else if let Some(rect) = run.take() {
                ui.painter().rect_filled(
                    rect.expand2(vec2(PAD_X, 0.0)).translate(position.to_vec2()),
                    RADIUS,
                    color,
                );
            }
        }
        if let Some(rect) = run {
            ui.painter().rect_filled(
                rect.expand2(vec2(PAD_X, 0.0)).translate(position.to_vec2()),
                RADIUS,
                color,
            );
        }
    }
}

fn table_block(
    ui: &mut egui::Ui,
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    style: &MarkdownStyle,
) {
    ui.scope(|ui| {
        ui.visuals_mut().faint_bg_color = style.colors.table_stripe;
        egui::Grid::new(ui.next_auto_id())
            .striped(true)
            .spacing(style.table_spacing)
            .show(ui, |ui| {
                for cell in header {
                    ui.label(
                        RichText::new(plain_text(cell))
                            .size(style.body_font_size)
                            .strong()
                            .color(style.colors.strong_text),
                    );
                }
                if !header.is_empty() {
                    ui.end_row();
                }
                for row in rows {
                    for cell in row {
                        ui.label(
                            RichText::new(plain_text(cell))
                                .size(style.body_font_size)
                                .color(style.colors.text),
                        );
                    }
                    ui.end_row();
                }
            });
    });
}

fn horizontal_rule(ui: &mut egui::Ui, style: &MarkdownStyle) {
    let available_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(available_width, 1.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, style.colors.rule),
    );
}

fn code_block(
    ui: &mut egui::Ui,
    language: Option<&str>,
    code: &str,
    message: Option<&str>,
    style: &MarkdownStyle,
) {
    if let Some(message) = message {
        ui.add(
            Label::new(
                RichText::new(message)
                    .size(style.body_font_size)
                    .color(style.colors.warning_text),
            )
            .wrap(),
        );
    }
    if let Some(language) = language {
        ui.add_space(2.0);
        ui.label(
            RichText::new(language.to_ascii_uppercase())
                .font(FontId::new(style.small_font_size, FontFamily::Monospace))
                .color(style.colors.muted_text),
        );
    }
    egui::Frame::new()
        .fill(style.colors.code_background)
        .stroke(Stroke::NONE)
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(style.code_margin))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(code)
                                .font(FontId::new(style.code_font_size, FontFamily::Monospace))
                                .color(style.colors.code_text),
                        )
                        .selectable(true),
                    );
                });
        });
}

fn block_top_gap(block: &Block, style: &MarkdownStyle) -> f32 {
    match block {
        Block::Heading { level: 1, .. } => 24.0,
        Block::Heading { level: 2, .. } => 24.0,
        Block::Heading { .. } => style.paragraph_gap,
        Block::HorizontalRule => 24.0,
        _ => style.paragraph_gap,
    }
}

#[cfg(test)]
mod tests {
    use super::inline_layout;
    use crate::document::model::Inline;
    use crate::render::settings::ViewerSettings;
    use eframe::egui::{Align, Color32, FontFamily, FontId};

    #[test]
    fn inline_code_uses_custom_rounded_background_section() {
        let style = ViewerSettings::default().style();
        let layout = inline_layout(
            &[
                Inline::Text("before ".to_owned()),
                Inline::Code("inline_value".to_owned()),
                Inline::Text(" after".to_owned()),
            ],
            FontId::new(style.body_font_size, FontFamily::Proportional),
            style.colors.text,
            false,
            400.0,
            style.body_font_size * style.line_height,
            &style,
        );
        let code_section = layout.code_sections[0] as usize;

        assert_eq!(layout.code_sections, vec![1]);
        assert_eq!(
            layout.job.sections[0].format.line_height,
            Some(style.body_font_size * style.line_height)
        );
        assert_eq!(
            layout.job.sections[code_section].format.background,
            Color32::TRANSPARENT
        );
        assert_eq!(layout.job.sections[code_section].format.valign, Align::Min);
        assert_eq!(layout.job.sections[code_section].format.line_height, None);
    }

    #[test]
    fn heading_inline_layout_uses_heading_line_height() {
        let style = ViewerSettings::default().style();
        let font_size = style.heading_sizes[0];
        let line_height = font_size * style.line_height;
        let layout = inline_layout(
            &[Inline::Text("heading".to_owned())],
            FontId::new(font_size, FontFamily::Proportional),
            style.colors.strong_text,
            true,
            400.0,
            line_height,
            &style,
        );

        assert_eq!(layout.job.sections[0].format.line_height, Some(line_height));
    }
}
