use eframe::egui::{
    self, pos2,
    text::{LayoutJob, TextFormat},
    vec2, Align, Color32, FontFamily, FontId, Label, Rect, RichText, Stroke,
};

use crate::app::ImageEntry;
use crate::document::model::{plain_text, Block, Document, Frontmatter, FrontmatterFormat, Inline};
use crate::input::NavigationState;
use crate::render::settings::MarkdownStyle;

pub fn render_document(
    ui: &mut egui::Ui,
    document: &mut Document,
    render_mermaid: bool,
    style: &MarkdownStyle,
    navigation: &mut NavigationState,
    highlighter: Option<&crate::highlight::Highlighter>,
    image_cache: &mut std::collections::HashMap<String, crate::app::ImageEntry>,
    search_match: Option<usize>,
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
            );
            if *level <= 2 {
                ui.add_space(6.0);
                horizontal_rule(ui, style);
            }
        }
        Block::Paragraph { content } => {
            if content.iter().any(|i| matches!(i, Inline::Image { .. })) {
                render_paragraph_with_images(ui, content, style, image_cache);
            } else {
                inline_label(
                    ui,
                    content,
                    FontId::new(style.body_font_size, FontFamily::Proportional),
                    style.colors.text,
                    false,
                    style,
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
                            );
                        }
                    });
                });
            }
        }
        Block::Quote { blocks } => {
            let frame_response = egui::Frame::new()
                .fill(style.colors.quote_background)
                .stroke(Stroke::NONE)
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
                            highlighter,
                            image_cache,
                            false,
                        );
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
                    highlighter,
                    image_cache,
                    false,
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
                                highlighter,
                                image_cache,
                                false,
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
                None,
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
                                    let _ = open::that(url);
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
    alignments: &[crate::document::model::TableAlignment],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    style: &MarkdownStyle,
) {
    use crate::document::model::TableAlignment;

    let col_align = |col: usize| match alignments.get(col).copied().unwrap_or(TableAlignment::None) {
        TableAlignment::Center => egui::Align::Center,
        TableAlignment::Right => egui::Align::RIGHT,
        _ => egui::Align::LEFT,
    };

    ui.scope(|ui| {
        ui.visuals_mut().faint_bg_color = style.colors.table_stripe;
        egui::Grid::new(ui.next_auto_id())
            .striped(true)
            .spacing(style.table_spacing)
            .show(ui, |ui| {
                for (col, cell) in header.iter().enumerate() {
                    ui.with_layout(egui::Layout::left_to_right(col_align(col)), |ui| {
                        let layout = inline_layout(
                            cell,
                            FontId::new(style.body_font_size, FontFamily::Proportional),
                            style.colors.strong_text,
                            true,
                            ui.available_width().max(1.0),
                            style.body_font_size * style.line_height,
                            style,
                        );
                        let (pos, galley, _) = Label::new(layout.job).wrap().layout_in_ui(ui);
                        paint_inline_code_backgrounds(
                            ui, pos, &galley, &layout.code_sections,
                            style.colors.inline_code_background,
                        );
                        ui.painter().galley(pos, galley.clone(), style.colors.strong_text);
                    });
                }
                if !header.is_empty() {
                    ui.end_row();
                }
                for row in rows {
                    for (col, cell) in row.iter().enumerate() {
                        ui.with_layout(egui::Layout::left_to_right(col_align(col)), |ui| {
                            let layout = inline_layout(
                                cell,
                                FontId::new(style.body_font_size, FontFamily::Proportional),
                                style.colors.text,
                                false,
                                ui.available_width().max(1.0),
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
                            ui.painter().galley(pos, galley.clone(), style.colors.text);
                            if has_links && response.clicked() {
                                if let Some(cursor_pos) = ui.ctx().pointer_interact_pos() {
                                    let local = cursor_pos - pos.to_vec2();
                                    'hit: for row in &galley.rows {
                                        if local.y >= row.rect.min.y && local.y <= row.rect.max.y {
                                            for glyph in &row.glyphs {
                                                if local.x >= glyph.pos.x && local.x <= glyph.max_x() {
                                                    for (section_idx, url) in &layout.link_sections {
                                                        if glyph.section_index == *section_idx {
                                                            let _ = open::that(url);
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

    let copy_id = egui::Id::new(("code-copy", ui.next_auto_id()));
    ui.add_space(4.0);
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
            let btn_label = if is_copied { "✓" } else { "copy" };
            let btn = egui::Button::new(
                RichText::new(btn_label)
                    .size(11.0)
                    .color(style.colors.muted_text),
            )
            .fill(egui::Color32::TRANSPARENT);
            if ui.add(btn).clicked() && !is_copied {
                ui.ctx().copy_text(code.to_owned());
                ui.data_mut(|d| d.insert_temp(copy_id, std::time::Instant::now()));
            }
        });
    });
    ui.add_space(2.0);

    egui::Frame::new()
        .fill(style.colors.code_background)
        .stroke(Stroke::new(1.0, style.colors.page_border))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(style.code_margin))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
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
}

fn render_paragraph_with_images(
    ui: &mut egui::Ui,
    content: &[Inline],
    style: &MarkdownStyle,
    image_cache: &mut std::collections::HashMap<String, ImageEntry>,
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
