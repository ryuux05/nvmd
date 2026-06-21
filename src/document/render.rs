use eframe::egui::{
    self, pos2,
    text::{LayoutJob, TextFormat},
    vec2, Align, Color32, FontFamily, FontId, Label, Rect, RichText, Stroke,
};

use crate::app::{ImageEntry, TocEntry};
use crate::document::model::{plain_text, Block, Document, Frontmatter, FrontmatterFormat, Inline};
use crate::input::NavigationState;
use crate::render::settings::MarkdownStyle;

/// Convert a heading's plain text to a GitHub-style anchor ID.
fn heading_to_id(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-')
        .map(|c| if c.is_whitespace() { '-' } else { c.to_lowercase().next().unwrap_or(c) })
        .collect()
}

/// Detect a GFM alert prefix like `[!NOTE]` in the first paragraph of a blockquote.
fn detect_alert(blocks: &[Block]) -> Option<(&'static str, &'static str)> {
    if let Some(Block::Paragraph { content }) = blocks.first() {
        let text = plain_text(content);
        let upper = text.trim().to_uppercase();
        let upper = upper.trim_start_matches("[!").trim_end_matches(']');
        return match upper {
            "NOTE"      => Some(("ℹ  Note",      "note")),
            "TIP"       => Some(("💡 Tip",        "tip")),
            "IMPORTANT" => Some(("★  Important",  "important")),
            "WARNING"   => Some(("⚠  Warning",    "warning")),
            "CAUTION"   => Some(("✖  Caution",    "caution")),
            _ => None,
        };
    }
    None
}

/// Convert MathML output to readable Unicode text by stripping XML tags and
/// decoding common entities. Used to display math without a full MathML renderer.
fn mml_to_text(mml: &str) -> String {
    let mut out = String::with_capacity(mml.len());
    let mut in_tag = false;
    for ch in mml.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
       .replace("&gt;", ">")
       .replace("&amp;", "&")
       .replace("&nbsp;", " ")
       .split_whitespace()
       .collect::<Vec<_>>()
       .join(" ")
}

/// Dispatch a link click: anchor fragments scroll within the document; external URLs open a browser.
fn handle_link_click(url: &str, toc_entries: &[TocEntry], navigation: &mut NavigationState) {
    if let Some(fragment) = url.strip_prefix('#') {
        let target = heading_to_id(fragment);
        if let Some(entry) = toc_entries.iter().find(|e| heading_to_id(&e.text) == target) {
            navigation.request_source_block(entry.block_index);
        }
    } else if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
        let _ = open::that(url);
    }
}

pub fn render_document(
    ui: &mut egui::Ui,
    document: &mut Document,
    render_mermaid: bool,
    style: &MarkdownStyle,
    navigation: &mut NavigationState,
    highlighter: Option<&crate::highlight::Highlighter>,
    image_cache: &mut std::collections::HashMap<String, crate::app::ImageEntry>,
    search_match: Option<usize>,
    visible_heading_block: &mut Option<usize>,
    toc_entries: &[TocEntry],
    word_wrap: bool,
) {
    if let Some(fm) = &document.frontmatter {
        render_frontmatter(ui, fm, style);
        ui.add_space(style.paragraph_gap);
    }
    let mut mermaid_index = 0;
    let mermaid_count = count_mermaid_blocks(&document.blocks);
    let source_block = navigation.take_source_block();
    for (index, block) in document.blocks.iter_mut().enumerate() {
        if index > 0 {
            ui.add_space(block_top_gap(block, style));
        }
        if matches!(block, Block::Heading { .. }) {
            let pos_y = ui.next_widget_position().y;
            if pos_y <= ui.clip_rect().center().y {
                *visible_heading_block = Some(index);
            }
        }
        if source_block == Some(index) {
            ui.scroll_to_cursor(Some(Align::TOP));
            let selected_mermaid = matches!(block, Block::Mermaid { .. }).then_some(mermaid_index);
            navigation.apply_synced_block_mode(selected_mermaid);
        }
        let is_search_match = search_match == Some(index);
        render_block(
            ui,
            block,
            render_mermaid,
            style,
            &mut mermaid_index,
            mermaid_count,
            navigation,
            highlighter,
            image_cache,
            is_search_match,
            toc_entries,
            word_wrap,
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
    highlighter: Option<&crate::highlight::Highlighter>,
    image_cache: &mut std::collections::HashMap<String, crate::app::ImageEntry>,
    is_search_match: bool,
    toc_entries: &[TocEntry],
    word_wrap: bool,
) {
    if is_search_match {
        let rect = ui.available_rect_before_wrap();
        let highlight_color = if style.is_dark {
            egui::Color32::from_rgba_unmultiplied(86, 162, 255, 20)
        } else {
            egui::Color32::from_rgba_unmultiplied(9, 105, 218, 15)
        };
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.min.x - 8.0, rect.min.y - 4.0),
                egui::pos2(rect.max.x + 8.0, rect.min.y + 200.0),
            ),
            4.0,
            highlight_color,
        );
        let bar_color = style.colors.link;
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.min.x - 8.0, rect.min.y - 4.0),
                egui::pos2(rect.min.x - 5.0, rect.min.y + 200.0),
            ),
            0.0,
            bar_color,
        );
    }
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
            toc_entries,
            navigation,
                word_wrap,
            );
            if *level <= 2 {
                ui.add_space(6.0);
                horizontal_rule(ui, style);
            }
        }
        Block::Paragraph { content } => {
            if content.iter().any(|i| matches!(i, Inline::Image { .. })) {
                render_paragraph_with_images(ui, content, style, image_cache, toc_entries, navigation, word_wrap);
            } else {
                inline_label(
                    ui,
                    content,
                    FontId::new(style.body_font_size, FontFamily::Proportional),
                    style.colors.text,
                    false,
                    style,
                toc_entries,
                navigation,
                word_wrap,
                );
            }
        }
        Block::CodeBlock { language, code } => {
            code_block(ui, language.as_deref(), code, None, style, highlighter);
        }
        Block::List {
            ordered,
            start,
            items,
        } => {
            for (index, item) in items.iter().enumerate() {
                let (marker, marker_color) = if *ordered {
                    (
                        format!("{}.", start.unwrap_or(1) + index as u64),
                        style.colors.muted_text,
                    )
                } else if item.checked == Some(true) {
                    ("✓".to_owned(), style.colors.link)
                } else if item.checked == Some(false) {
                    ("○".to_owned(), style.colors.muted_text)
                } else {
                    ("·".to_owned(), style.colors.muted_text)
                };
                let marker_font = if *ordered {
                    FontFamily::Monospace
                } else {
                    FontFamily::Proportional
                };
                ui.horizontal_top(|ui| {
                    ui.set_min_height(style.list_item_min_height);
                    ui.add_sized(
                        vec2(style.list_marker_width, style.list_item_min_height),
                        Label::new(
                            RichText::new(marker)
                                .font(FontId::new(style.body_font_size, marker_font))
                                .color(marker_color),
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
                                highlighter,
                                image_cache,
                                false,
                                toc_entries,
                                word_wrap,
                            );

                        }
                    });
                });
            }
        }
        Block::Quote { blocks } => {
            if let Some((label, kind)) = detect_alert(blocks) {
                let (border, bg, text_color) = match kind {
                    "note"      => (style.colors.alert_note_border,      style.colors.alert_note_bg,      style.colors.alert_note_text),
                    "tip"       => (style.colors.alert_tip_border,       style.colors.alert_tip_bg,       style.colors.alert_tip_text),
                    "important" => (style.colors.alert_important_border, style.colors.alert_important_bg, style.colors.alert_important_text),
                    "warning"   => (style.colors.alert_warning_border,   style.colors.alert_warning_bg,   style.colors.alert_warning_text),
                    _           => (style.colors.alert_caution_border,   style.colors.alert_caution_bg,   style.colors.alert_caution_text),
                };
                let frame_response = egui::Frame::new()
                    .fill(bg)
                    .stroke(Stroke::NONE)
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin { left: 14, right: 12, top: 10, bottom: 10 })
                    .show(ui, |ui| {
                        ui.label(RichText::new(label).size(style.small_font_size).strong().color(border));
                        ui.add_space(4.0);
                        ui.visuals_mut().override_text_color = Some(text_color);
                        for block in blocks.iter().skip(1) {
                            render_block(ui, &mut block.clone(), render_mermaid, style, mermaid_index, mermaid_count, navigation, highlighter, image_cache, false, toc_entries, word_wrap);
                            ui.add_space(style.paragraph_gap * 0.5);
                        }
                    });
                let rect = frame_response.response.rect;
                ui.painter().rect_filled(
                    Rect::from_min_max(rect.min, pos2(rect.min.x + 3.0, rect.max.y)),
                    0.0,
                    border,
                );
            } else {
                let frame_response = egui::Frame::new()
                    .fill(style.colors.quote_background)
                    .stroke(Stroke::NONE)
                    .inner_margin(egui::Margin::same(style.quote_margin))
                    .show(ui, |ui| {
                        ui.visuals_mut().override_text_color = Some(style.colors.quote_text);
                        for block in blocks {
                            render_block(ui, block, render_mermaid, style, mermaid_index, mermaid_count, navigation, highlighter, image_cache, false, toc_entries, word_wrap);
                            ui.add_space(style.paragraph_gap * 0.5);
                        }
                    });
                let rect = frame_response.response.rect;
                ui.painter().rect_filled(
                    Rect::from_min_max(rect.min, pos2(rect.min.x + 3.0, rect.max.y)),
                    0.0,
                    style.colors.quote_border,
                );
            }
        }
        Block::HorizontalRule => {
            ui.add_space(8.0);
            horizontal_rule(ui, style);
            ui.add_space(8.0);
        }
        Block::HtmlBlock { html } => {
            code_block(ui, Some("html"), html, Some("Raw HTML block"), style, None);
        }
        Block::Table {
            alignments,
            header,
            rows,
        } => {
            table_block(ui, alignments, header, rows, style);
        }
        Block::FootnoteDefinition { label, blocks } => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("[{label}]"))
                        .font(FontId::new(style.small_font_size, FontFamily::Proportional))
                        .color(style.colors.link),
                );
                ui.visuals_mut().override_text_color = Some(style.colors.muted_text);
                for block in blocks {
                    render_block(
                        ui,
                        &mut block.clone(),
                        render_mermaid,
                        style,
                        mermaid_index,
                        mermaid_count,
                        navigation,
                        highlighter,
                        image_cache,
                        false,
                        toc_entries,
                        word_wrap,
                    );

                }
            });
        }
        Block::DefinitionList { items } => {
            for item in items {
                inline_label(
                    ui,
                    &item.term,
                    FontId::new(style.body_font_size, FontFamily::Proportional),
                    style.colors.strong_text,
                    true,
                    style,
                toc_entries,
                navigation,
                word_wrap,
                );
                for blocks in &item.definitions {
                    ui.indent("definition-list-item", |ui| {
                        ui.visuals_mut().override_text_color = Some(style.colors.muted_text);
                        for block in blocks {
                            render_block(
                                ui,
                                &mut block.clone(),
                                render_mermaid,
                                style,
                                mermaid_index,
                                mermaid_count,
                                navigation,
                                highlighter,
                                image_cache,
                                false,
                                toc_entries,
                                word_wrap,
                            );

                        }
                    });
                }
                ui.add_space(style.paragraph_gap * 0.5);
            }
        }
        Block::MathBlock { expression } => {
            let rendered = latex2mathml::latex_to_mathml(expression, latex2mathml::DisplayStyle::Block)
                .map(|mml| mml_to_text(&mml))
                .unwrap_or_else(|_| expression.clone());
            egui::Frame::new()
                .fill(style.colors.code_background)
                .stroke(Stroke::new(1.0, style.colors.page_border))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::same(style.code_margin))
                .show(ui, |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(&rendered)
                                .font(FontId::new(style.body_font_size * 1.1, FontFamily::Proportional))
                                .color(style.colors.strong_text)
                                .italics(),
                        )
                        .wrap(),
                    );
                });
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
                    None,
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
    link_sections: Vec<(u32, String)>,
}

fn inline_label(
    ui: &mut egui::Ui,
    inlines: &[Inline],
    font_id: FontId,
    color: Color32,
    strong: bool,
    markdown: &MarkdownStyle,
    toc_entries: &[TocEntry],
    navigation: &mut NavigationState,
    word_wrap: bool,
) {
    let line_height = font_id.size * markdown.line_height;
    let mut layout = inline_layout(
        inlines,
        font_id,
        color,
        strong,
        ui.available_width(),
        line_height,
        markdown,
    );
    if !word_wrap {
        layout.job.wrap.max_width = f32::INFINITY;
    }
    let has_links = !layout.link_sections.is_empty();
    let sense = if has_links {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (position, galley, response) = Label::new(layout.job).sense(sense).wrap().layout_in_ui(ui);
    paint_inline_code_backgrounds(
        ui,
        position,
        &galley,
        &layout.code_sections,
        markdown.colors.inline_code_background,
    );
    ui.painter().galley(position, galley.clone(), color);

    if has_links && response.clicked() {
        if let Some(cursor_pos) = ui.ctx().pointer_interact_pos() {
            let local = cursor_pos - position.to_vec2();
            'hit: for row in &galley.rows {
                if local.y >= row.rect.min.y && local.y <= row.rect.max.y {
                    for glyph in &row.glyphs {
                        if local.x >= glyph.pos.x && local.x <= glyph.max_x() {
                            for (section_idx, url) in &layout.link_sections {
                                if glyph.section_index == *section_idx {
                                    handle_link_click(url, toc_entries, navigation);
                                    break 'hit;
                                }
                            }
                            break 'hit;
                        }
                    }
                }
            }
        }
    }
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
    let mut link_sections = Vec::new();
    job.wrap.max_width = wrap_width.max(1.0);
    job.break_on_newline = true;
    append_inlines(
        &mut job,
        &mut code_sections,
        &mut link_sections,
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
    InlineLayout { job, code_sections, link_sections }
}

fn append_inlines(
    job: &mut LayoutJob,
    code_sections: &mut Vec<u32>,
    link_sections: &mut Vec<(u32, String)>,
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
                link_sections,
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
                link_sections,
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
                link_sections,
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
            Inline::Link { destination, children, .. } => {
                let section_before = job.sections.len() as u32;
                append_inlines(
                    job,
                    code_sections,
                    link_sections,
                    children,
                    font_id.clone(),
                    markdown.colors.link,
                    style,
                    line_height,
                    markdown,
                );
                for i in section_before..job.sections.len() as u32 {
                    link_sections.push((i, destination.clone()));
                }
            }
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
                    link_sections,
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
            Inline::Html(html) => {
                append_inline_text(job, code_sections, html, &font_id, color, style, line_height, markdown);
            }
            Inline::Math(expr) => {
                let text = latex2mathml::latex_to_mathml(expr, latex2mathml::DisplayStyle::Inline)
                    .map(|mml| mml_to_text(&mml))
                    .unwrap_or_else(|_| expr.clone());
                let math_fmt = TextFormat {
                    font_id: font_id.clone(),
                    color: markdown.colors.strong_text,
                    italics: true,
                    ..Default::default()
                };
                job.append(&text, 0.0, math_fmt);
            }
            Inline::FootnoteRef(label) => {
                let sup_font = FontId::new(
                    (font_id.size * 0.72).max(8.0),
                    font_id.family.clone(),
                );
                let text = format!("[{label}]");
                let mut fmt = TextFormat::simple(sup_font, markdown.colors.link);
                fmt.valign = Align::Min;
                job.append(&text, 0.0, fmt);
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
    alignments: &[crate::document::model::TableAlignment],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    style: &MarkdownStyle,
) {
    use crate::document::model::TableAlignment;

    let num_cols = header.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if num_cols == 0 {
        return;
    }

    // Estimate each column's natural width from the longest plain-text cell in that column.
    // We use character count as a proxy for rendered width, then distribute the available
    // width proportionally so narrow columns (like key badges) don't over-consume space.
    let min_chars: usize = 6;
    let col_weights: Vec<usize> = (0..num_cols)
        .map(|col| {
            let h = header.get(col).map(|c| plain_text(c).chars().count()).unwrap_or(0);
            let r = rows
                .iter()
                .map(|row| row.get(col).map(|c| plain_text(c).chars().count()).unwrap_or(0))
                .max()
                .unwrap_or(0);
            h.max(r).max(min_chars)
        })
        .collect();
    let total_weight: usize = col_weights.iter().sum();
    // Dynamic column gap: 5% of available width, clamped between 16px and 72px.
    let col_gap = (ui.available_width() * 0.05).clamp(16.0, 72.0);
    let h_gap = col_gap * (num_cols as f32 - 1.0);
    let usable = (ui.available_width() - h_gap).max(1.0);
    let col_widths: Vec<f32> = col_weights
        .iter()
        .map(|&w| (w as f32 / total_weight as f32 * usable).max(40.0))
        .collect();

    let col_align = |col: usize| match alignments.get(col).copied().unwrap_or(TableAlignment::None) {
        TableAlignment::Center => egui::Align::Center,
        TableAlignment::Right => egui::Align::RIGHT,
        _ => egui::Align::LEFT,
    };

    let render_cell = |ui: &mut egui::Ui, col: usize, inlines: &[Inline], strong: bool| {
        let w = col_widths.get(col).copied().unwrap_or(80.0);
        ui.set_min_width(w);
        let color = if strong { style.colors.strong_text } else { style.colors.text };
        let layout = inline_layout(
            inlines,
            FontId::new(style.body_font_size, FontFamily::Proportional),
            color,
            strong,
            w,
            style.body_font_size * style.line_height,
            style,
        );
        let has_links = !layout.link_sections.is_empty();
        let sense = if has_links { egui::Sense::click() } else { egui::Sense::hover() };
        let (pos, galley, response) = Label::new(layout.job).sense(sense).wrap().layout_in_ui(ui);
        paint_inline_code_backgrounds(
            ui, pos, &galley, &layout.code_sections,
            style.colors.inline_code_background,
        );
        ui.painter().galley(pos, galley.clone(), color);
        if has_links && response.clicked() {
            if let Some(cursor_pos) = ui.ctx().pointer_interact_pos() {
                let local = cursor_pos - pos.to_vec2();
                'hit: for row in &galley.rows {
                    if local.y >= row.rect.min.y && local.y <= row.rect.max.y {
                        for glyph in &row.glyphs {
                            if local.x >= glyph.pos.x && local.x <= glyph.max_x() {
                                for (section_idx, url) in &layout.link_sections {
                                    if glyph.section_index == *section_idx {
                                        if url.starts_with("http://")
                                            || url.starts_with("https://")
                                            || url.starts_with("mailto:")
                                        {
                                            let _ = open::that(url);
                                        }
                                        break 'hit;
                                    }
                                }
                                break 'hit;
                            }
                        }
                    }
                }
            }
        }
    };

    ui.scope(|ui| {
        ui.visuals_mut().faint_bg_color = style.colors.table_stripe;
        egui::Grid::new(ui.next_auto_id())
            .striped(true)
            .spacing(egui::vec2(col_gap, style.table_spacing.y))
            .show(ui, |ui| {
                for (col, cell) in header.iter().enumerate() {
                    ui.with_layout(egui::Layout::left_to_right(col_align(col)), |ui| {
                        render_cell(ui, col, cell, true);
                    });
                }
                if !header.is_empty() {
                    ui.end_row();
                }
                for row in rows {
                    for (col, cell) in row.iter().enumerate() {
                        ui.with_layout(egui::Layout::left_to_right(col_align(col)), |ui| {
                            render_cell(ui, col, cell, false);
                        });
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
    highlighter: Option<&crate::highlight::Highlighter>,
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

    let block_id = ui.next_auto_id();
    let copy_id = egui::Id::new(("code-copy", block_id));
    let alpha_id = egui::Id::new(("code-alpha", block_id));

    // Animated opacity: 0.0 = fully hidden, 1.0 = fully visible.
    let hover_alpha: f32 = ui.data(|d| d.get_temp(alpha_id).unwrap_or(0.0_f32));

    ui.add_space(4.0);

    let frame_resp = egui::Frame::new()
        .fill(style.colors.code_background)
        .stroke(Stroke::new(1.0, style.colors.page_border))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(style.code_margin))
        .show(ui, |ui| {
            // Header row is always rendered at fixed height — no layout shift.
            // The icon color's alpha is animated so it fades in/out smoothly.
            ui.horizontal(|ui| {
                if let Some(lang) = language {
                    ui.label(
                        RichText::new(lang)
                            .font(FontId::new(style.small_font_size, FontFamily::Monospace))
                            .color(style.colors.muted_text),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let copied_at: Option<std::time::Instant> =
                        ui.data(|d| d.get_temp::<std::time::Instant>(copy_id));
                    let is_copied = copied_at
                        .map(|t| t.elapsed() < std::time::Duration::from_millis(1400))
                        .unwrap_or(false);
                    let icon = if is_copied { "✓" } else { "⧉" };
                    let base = if is_copied { style.colors.link } else { style.colors.muted_text };
                    let icon_color = egui::Color32::from_rgba_unmultiplied(
                        base.r(), base.g(), base.b(),
                        (hover_alpha * base.a() as f32).round() as u8,
                    );
                    let btn = egui::Button::new(
                        RichText::new(icon).size(14.0).color(icon_color),
                    )
                    .fill(egui::Color32::TRANSPARENT);
                    if ui.add(btn).clicked() && hover_alpha > 0.1 && !is_copied {
                        ui.ctx().copy_text(code.to_owned());
                        ui.data_mut(|d| d.insert_temp(copy_id, std::time::Instant::now()));
                    }
                });
            });
            ui.add_space(6.0);
            // id_salt ensures each code block's ScrollArea has a unique ID.
            egui::ScrollArea::horizontal()
                .id_salt(block_id)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if let (Some(lang), Some(hl)) = (language, highlighter) {
                        let spans = hl.highlight(code, lang, style.is_dark);
                        if !spans.is_empty() {
                            let line_height = style.code_font_size * style.line_height;
                            let font_id =
                                FontId::new(style.code_font_size, FontFamily::Monospace);
                            let mut job = eframe::egui::text::LayoutJob::default();
                            job.wrap.max_width = f32::INFINITY;
                            for (color, text) in spans {
                                let mut fmt = eframe::egui::text::TextFormat::simple(
                                    font_id.clone(),
                                    color,
                                );
                                fmt.line_height = Some(line_height);
                                job.append(&text, 0.0, fmt);
                            }
                            ui.add(Label::new(job).selectable(true));
                            return;
                        }
                    }
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

    // Lerp alpha toward target using actual frame delta time for smooth, rate-independent easing.
    let is_hovered = ui.rect_contains_pointer(frame_resp.response.rect);
    let target = if is_hovered { 1.0_f32 } else { 0.0_f32 };
    let dt = ui.ctx().input(|i| i.unstable_dt).min(0.1);
    let new_alpha = (hover_alpha + (target - hover_alpha) * (1.0 - (-8.0 * dt).exp())).clamp(0.0, 1.0);
    ui.data_mut(|d| d.insert_temp(alpha_id, new_alpha));
    if (new_alpha - target).abs() > 0.002 {
        ui.ctx().request_repaint();
    }
}

fn render_paragraph_with_images(
    ui: &mut egui::Ui,
    content: &[Inline],
    style: &MarkdownStyle,
    image_cache: &mut std::collections::HashMap<String, ImageEntry>,
    toc_entries: &[TocEntry],
    navigation: &mut NavigationState,
    word_wrap: bool,
) {
    for inline in content {
        match inline {
            Inline::Image { destination, alt, .. } => {
                match image_cache.get(destination) {
                    Some(ImageEntry::Loaded(img)) => {
                        let texture = ui.ctx().load_texture(
                            destination.as_str(),
                            img.clone(),
                            egui::TextureOptions::LINEAR,
                        );
                        let max_width = ui.available_width();
                        let orig = texture.size_vec2();
                        let display_size = if orig.x > max_width {
                            egui::vec2(max_width, orig.y * (max_width / orig.x))
                        } else {
                            orig
                        };
                        ui.image((texture.id(), display_size));
                        let alt_text = plain_text(alt);
                        if !alt_text.is_empty() {
                            ui.label(
                                RichText::new(alt_text)
                                    .size(style.small_font_size)
                                    .color(style.colors.muted_text)
                                    .italics(),
                            );
                        }
                    }
                    Some(ImageEntry::Failed(err)) => {
                        let alt_text = plain_text(alt);
                        let display = if alt_text.is_empty() {
                            format!("[image failed: {err}]")
                        } else {
                            format!("[{alt_text}]")
                        };
                        ui.label(
                            RichText::new(display)
                                .size(style.body_font_size)
                                .color(style.colors.warning_text),
                        );
                    }
                    Some(ImageEntry::Loading) => {
                        let alt_text = plain_text(alt);
                        let display = if alt_text.is_empty() {
                            "[loading image…]".to_owned()
                        } else {
                            format!("[loading: {alt_text}]")
                        };
                        ui.label(
                            RichText::new(display)
                                .size(style.body_font_size)
                                .color(style.colors.muted_text),
                        );
                    }
                    None => {
                        image_cache.insert(destination.clone(), ImageEntry::Loading);
                    }
                }
            }
            _ => {
                inline_label(
                    ui,
                    std::slice::from_ref(inline),
                    FontId::new(style.body_font_size, FontFamily::Proportional),
                    style.colors.text,
                    false,
                    style,
                toc_entries,
                navigation,
                word_wrap,
                );
            }
        }
    }
}

fn render_frontmatter(ui: &mut egui::Ui, fm: &Frontmatter, style: &MarkdownStyle) {
    let lang = match fm.format {
        FrontmatterFormat::Yaml => "yaml",
        FrontmatterFormat::Toml => "toml",
        FrontmatterFormat::Json => "json",
    };
    egui::Frame::new()
        .fill(style.colors.code_background)
        .stroke(Stroke::new(1.0, style.colors.page_border))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(lang)
                        .font(FontId::new(style.small_font_size, FontFamily::Monospace))
                        .color(style.colors.muted_text),
                );
                ui.label(
                    RichText::new("frontmatter")
                        .font(FontId::new(style.small_font_size, FontFamily::Monospace))
                        .color(style.colors.muted_text),
                );
            });
            ui.add_space(4.0);
            ui.add(
                Label::new(
                    RichText::new(fm.raw.trim())
                        .font(FontId::new(style.code_font_size, FontFamily::Monospace))
                        .color(style.colors.muted_text),
                )
                .selectable(true),
            );
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
