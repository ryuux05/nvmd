use std::sync::Arc;

use anyhow::{Context, Result};
use eframe::egui::ColorImage;
use tiny_skia::Pixmap;

use crate::mermaid::cache::MermaidCache;

const ER_NODE_SPACING: f32 = 168.0;
const ER_RANK_SPACING: f32 = 176.0;
const ER_TITLE_SCALE: f32 = 1.5;
const ER_HEADER_HEIGHT_SCALE: f32 = 3.0;
const ER_ENTITY_CLEARANCE_SCALE: f32 = 6.0;

#[derive(Debug, Clone)]
pub enum MermaidRenderState {
    Pending,
    Rendered { image: ColorImage },
    Failed { reason: String },
}

impl MermaidRenderState {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

#[derive(Debug, Clone)]
pub struct MermaidRenderer {
    cache: MermaidCache,
    fontdb: Arc<usvg::fontdb::Database>,
}

impl MermaidRenderer {
    pub fn new() -> Self {
        let mut fontdb = usvg::fontdb::Database::new();
        fontdb.load_system_fonts();
        fontdb.set_sans_serif_family("Arial Unicode MS");

        Self {
            cache: MermaidCache::new(),
            fontdb: Arc::new(fontdb),
        }
    }

    pub fn read_svg(&self, source: &str) -> Option<String> {
        self.cache.read_svg(source)
    }

    pub fn render_image(&self, source: &str) -> Result<ColorImage> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let svg = if let Some(svg) = self.cache.read_svg(source) {
                svg
            } else {
                let svg = render_diagram_svg(source)?;
                let _ = self.cache.write_svg(source, &svg);
                svg
            };

            rasterize_svg(&svg, self.fontdb.clone())
        }))
        .map_err(|payload| {
            anyhow::anyhow!(
                "native Mermaid renderer panicked: {}",
                panic_message(payload)
            )
        })?
    }
}

fn render_diagram_svg(source: &str) -> Result<String> {
    let parsed = mermaid_rs_renderer::parse_mermaid(source)
        .map_err(|err| anyhow::anyhow!("native Mermaid parser failed: {err}"))?;
    let mut options = mermaid_rs_renderer::RenderOptions::default();
    let mut layout_graph = parsed.graph.clone();
    let er_self_edges = if parsed.graph.kind == mermaid_rs_renderer::DiagramKind::Er {
        configure_er_layout(&mut options);
        parsed
            .graph
            .edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| edge.from == edge.to)
            .map(|(index, edge)| (index, edge.clone()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if parsed.graph.kind == mermaid_rs_renderer::DiagramKind::Er {
        layout_graph.edges.retain(|edge| edge.from != edge.to);
        for edge in &mut layout_graph.edges {
            edge.label = None;
        }
    }
    let mut layout =
        mermaid_rs_renderer::compute_layout(&layout_graph, &options.theme, &options.layout);
    if parsed.graph.kind == mermaid_rs_renderer::DiagramKind::Er {
        restore_er_relationship_labels(&mut layout, &parsed.graph);
    }
    add_er_self_relationships(
        &mut layout,
        &parsed.graph,
        &er_self_edges,
        options.theme.font_size,
    );

    normalize_layout_text(&mut layout, options.theme.font_size);
    if matches!(
        layout.kind,
        mermaid_rs_renderer::DiagramKind::Sequence | mermaid_rs_renderer::DiagramKind::ZenUML
    ) {
        improve_sequence_layout(&mut layout, options.theme.font_size);
    } else {
        improve_general_layout(&mut layout, options.theme.font_size);
        if layout.kind == mermaid_rs_renderer::DiagramKind::Er {
            keep_er_layout_inside_bounds(&mut layout, options.theme.font_size);
        }
    }
    extend_layout_bounds(&mut layout, options.theme.font_size);

    if layout.kind == mermaid_rs_renderer::DiagramKind::Er {
        Ok(render_er_svg(&mut layout, &options.theme, &options.layout))
    } else {
        Ok(mermaid_rs_renderer::render_svg(
            &layout,
            &options.theme,
            &options.layout,
        ))
    }
}

fn configure_er_layout(options: &mut mermaid_rs_renderer::RenderOptions) {
    options.layout.max_label_width_chars = 256;
    options.layout.node_spacing = options.layout.node_spacing.max(ER_NODE_SPACING);
    options.layout.rank_spacing = options.layout.rank_spacing.max(ER_RANK_SPACING);
}

fn restore_er_relationship_labels(
    layout: &mut mermaid_rs_renderer::Layout,
    graph: &mermaid_rs_renderer::ir::Graph,
) {
    let source_edges = graph.edges.iter().filter(|edge| edge.from != edge.to);
    for (layout_edge, source_edge) in layout.edges.iter_mut().zip(source_edges) {
        layout_edge.label = er_relationship_label(source_edge);
    }
}

fn add_er_self_relationships(
    layout: &mut mermaid_rs_renderer::Layout,
    graph: &mermaid_rs_renderer::ir::Graph,
    edges: &[(usize, mermaid_rs_renderer::ir::Edge)],
    font_size: f32,
) {
    for (index, edge) in edges {
        let Some(node) = layout.nodes.get(&edge.from) else {
            continue;
        };
        let gap = (font_size * 2.5).max(32.0);
        let right = node.x + node.width;
        let top = node.y + node.height * 0.35;
        let bottom = node.y + node.height * 0.65;
        let override_style = graph
            .edge_styles
            .get(index)
            .cloned()
            .or_else(|| graph.edge_style_default.clone())
            .unwrap_or_default();

        layout.edges.push(mermaid_rs_renderer::layout::EdgeLayout {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label: er_relationship_label(edge),
            start_label: None,
            end_label: None,
            points: vec![
                (right, top),
                (right + gap, top),
                (right + gap, bottom),
                (right, bottom),
            ],
            directed: edge.directed,
            arrow_start: edge.arrow_start,
            arrow_end: edge.arrow_end,
            arrow_start_kind: edge.arrow_start_kind,
            arrow_end_kind: edge.arrow_end_kind,
            start_decoration: edge.start_decoration,
            end_decoration: edge.end_decoration,
            style: edge.style,
            override_style,
        });
    }
}

fn er_relationship_label(
    edge: &mermaid_rs_renderer::ir::Edge,
) -> Option<mermaid_rs_renderer::layout::TextBlock> {
    edge.label
        .as_ref()
        .map(|text| mermaid_rs_renderer::layout::TextBlock {
            lines: vec![text.clone()],
            width: 0.0,
            height: 0.0,
        })
}

fn normalize_layout_text(layout: &mut mermaid_rs_renderer::Layout, font_size: f32) {
    let padding = font_size * 1.2;
    let is_er = layout.kind == mermaid_rs_renderer::DiagramKind::Er;
    for node in layout.nodes.values_mut() {
        let center = node.x + node.width / 2.0;
        normalize_text_block(&mut node.label, font_size, false);
        let content_width = if is_er {
            er_table_content_width(&node.label, font_size).unwrap_or(node.label.width)
        } else {
            node.label.width
        };
        node.width = node.width.max(content_width + padding * 2.0);
        node.height = node.height.max(node.label.height + padding * 1.5);
        node.x = center - node.width / 2.0;
    }
    for footbox in &mut layout.sequence_footboxes {
        let center = footbox.x + footbox.width / 2.0;
        normalize_text_block(&mut footbox.label, font_size, false);
        footbox.width = footbox.width.max(footbox.label.width + padding * 2.0);
        footbox.height = footbox.height.max(footbox.label.height + padding * 1.5);
        footbox.x = center - footbox.width / 2.0;
    }
    for edge in &mut layout.edges {
        if let Some(label) = &mut edge.label {
            normalize_text_block(label, font_size, false);
        }
        if let Some(label) = &mut edge.start_label {
            normalize_text_block(label, font_size, false);
        }
        if let Some(label) = &mut edge.end_label {
            normalize_text_block(label, font_size, false);
        }
    }
    for subgraph in &mut layout.subgraphs {
        normalize_text_block(&mut subgraph.label_block, font_size, false);
        subgraph.width = subgraph
            .width
            .max(subgraph.label_block.width + padding * 2.0);
    }
    for note in &mut layout.sequence_notes {
        let center = note.x + note.width / 2.0;
        normalize_text_block(&mut note.label, font_size, false);
        note.width = note.width.max(note.label.width + padding * 2.0);
        note.height = note.height.max(note.label.height + padding * 1.5);
        note.x = center - note.width / 2.0;
    }
    for frame in &mut layout.sequence_frames {
        normalize_text_block(&mut frame.label.text, font_size, false);
        for label in &mut frame.section_labels {
            normalize_text_block(&mut label.text, font_size, false);
        }
        frame.label_box.2 = frame
            .label_box
            .2
            .max(frame.label.text.width + padding * 2.0);
        frame.label_box.0 = frame.label.x - frame.label_box.2 / 2.0;
        frame.width = frame.width.max(frame.label_box.2 + padding);
    }
    for note in &mut layout.state_notes {
        let center = note.x + note.width / 2.0;
        normalize_text_block(&mut note.label, font_size, false);
        note.width = note.width.max(note.label.width + padding * 2.0);
        note.height = note.height.max(note.label.height + padding * 1.5);
        note.x = center - note.width / 2.0;
    }
    for legend in &mut layout.pie_legend {
        normalize_text_block(&mut legend.label, font_size, false);
    }
    if let Some(title) = &mut layout.pie_title {
        normalize_text_block(&mut title.text, font_size, false);
    }
    if let Some(quadrant) = &mut layout.quadrant {
        for label in [
            &mut quadrant.title,
            &mut quadrant.x_axis_left,
            &mut quadrant.x_axis_right,
            &mut quadrant.y_axis_bottom,
            &mut quadrant.y_axis_top,
        ] {
            if let Some(label) = label {
                normalize_text_block(label, font_size, false);
            }
        }
        for label in &mut quadrant.quadrant_labels {
            if let Some(label) = label {
                normalize_text_block(label, font_size, false);
            }
        }
        for point in &mut quadrant.points {
            normalize_text_block(&mut point.label, font_size, false);
        }
    }
    if let Some(gantt) = &mut layout.gantt {
        if let Some(title) = &mut gantt.title {
            normalize_text_block(title, font_size, false);
        }
        for section in &mut gantt.sections {
            normalize_text_block(&mut section.label, font_size, false);
        }
        for task in &mut gantt.tasks {
            normalize_text_block(&mut task.label, font_size, false);
            task.width = task.width.max(task.label.width + padding * 2.0);
            task.height = task.height.max(task.label.height + padding);
        }
    }
    if let Some(chart) = &mut layout.xychart {
        for label in [
            &mut chart.title,
            &mut chart.x_axis_label,
            &mut chart.y_axis_label,
        ] {
            if let Some(label) = label {
                normalize_text_block(label, font_size, false);
            }
        }
    }
    if let Some(timeline) = &mut layout.timeline {
        if let Some(title) = &mut timeline.title {
            normalize_text_block(title, font_size, false);
        }
        for section in &mut timeline.sections {
            normalize_text_block(&mut section.label, font_size, false);
            section.width = section.width.max(section.label.width + padding * 2.0);
            section.height = section.height.max(section.label.height + padding);
        }
        for event in &mut timeline.events {
            normalize_text_block(&mut event.time, font_size, false);
            let mut content_width = event.time.width;
            for text in &mut event.events {
                normalize_text_block(text, font_size, false);
                content_width = content_width.max(text.width);
            }
            event.width = event.width.max(content_width + padding * 2.0);
        }
    }
}

#[derive(Debug, Clone)]
struct ErAttributeRow {
    data_type: String,
    name: String,
    key: String,
    comment: String,
}

#[derive(Debug)]
struct ErTable {
    node_x: f32,
    node_y: f32,
    node_width: f32,
    title: Option<String>,
    rows: Vec<ErAttributeRow>,
    type_width: f32,
    name_width: f32,
    key_width: f32,
}

fn er_attribute_rows(label: &mermaid_rs_renderer::layout::TextBlock) -> Vec<ErAttributeRow> {
    let Some(divider_index) = label.lines.iter().position(|line| line.trim() == "---") else {
        return Vec::new();
    };

    label
        .lines
        .iter()
        .skip(divider_index + 1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let data_type = fields.next()?.to_owned();
            let name = fields.next()?.to_owned();
            let remainder = fields.collect::<Vec<_>>();
            let comment_start = remainder
                .iter()
                .position(|field| field.starts_with('"'))
                .unwrap_or(remainder.len());

            Some(ErAttributeRow {
                data_type,
                name,
                key: remainder[..comment_start].join(" "),
                comment: remainder[comment_start..].join(" "),
            })
        })
        .collect()
}

fn er_table_title(label: &mermaid_rs_renderer::layout::TextBlock) -> Option<String> {
    let divider_index = label.lines.iter().position(|line| line.trim() == "---")?;
    label
        .lines
        .iter()
        .take(divider_index)
        .find(|line| !line.trim().is_empty())
        .cloned()
}

fn er_table_column_widths(rows: &[ErAttributeRow], font_size: f32) -> (f32, f32, f32, f32) {
    rows.iter().fold(
        (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32),
        |(data_type, name, key, comment), row| {
            (
                data_type.max(readable_text_width(&row.data_type, font_size)),
                name.max(readable_text_width(&row.name, font_size)),
                key.max(readable_text_width(&row.key, font_size)),
                comment.max(readable_text_width(&row.comment, font_size)),
            )
        },
    )
}

fn er_table_content_width(
    label: &mermaid_rs_renderer::layout::TextBlock,
    font_size: f32,
) -> Option<f32> {
    let rows = er_attribute_rows(label);
    if rows.is_empty() {
        return None;
    }
    let (data_type, name, key, comment) = er_table_column_widths(&rows, font_size);
    let populated_columns = [data_type, name, key, comment]
        .iter()
        .filter(|width| **width > 0.0)
        .count();
    let gaps = populated_columns.saturating_sub(1) as f32 * font_size;
    let table_width = data_type + name + key + comment + gaps;
    let title_width = er_table_title(label)
        .map(|title| readable_text_width(&title, font_size * ER_TITLE_SCALE))
        .unwrap_or_default();
    Some(table_width.max(title_width))
}

fn render_er_svg(
    layout: &mut mermaid_rs_renderer::Layout,
    theme: &mermaid_rs_renderer::Theme,
    config: &mermaid_rs_renderer::LayoutConfig,
) -> String {
    let mut tables = Vec::new();
    for node in layout.nodes.values_mut() {
        let rows = er_attribute_rows(&node.label);
        if rows.is_empty() {
            continue;
        }
        let title = er_table_title(&node.label);
        let (type_width, name_width, key_width, _) = er_table_column_widths(&rows, theme.font_size);
        for line in &mut node.label.lines {
            line.clear();
        }
        tables.push(ErTable {
            node_x: node.x,
            node_y: node.y,
            node_width: node.width,
            title,
            rows,
            type_width,
            name_width,
            key_width,
        });
    }

    let mut svg = mermaid_rs_renderer::render_svg(layout, theme, config);
    let mut table_svg = String::new();
    let line_height = theme.font_size * config.label_line_height;
    let gap = theme.font_size;
    let inset = config.node_padding_x.max(10.0);
    for table in tables {
        let header_bottom = table.node_y + theme.font_size * ER_HEADER_HEIGHT_SCALE;
        let title_y = table.node_y + theme.font_size * 2.0;
        let first_row_y = header_bottom + line_height * 0.8;
        let type_x = table.node_x + inset;
        let name_x = type_x + table.type_width + gap;
        let key_x = name_x + table.name_width + gap;
        let comment_x = key_x + table.key_width + if table.key_width > 0.0 { gap } else { 0.0 };
        if let Some(title) = &table.title {
            push_er_title(
                &mut table_svg,
                table.node_x + table.node_width / 2.0,
                title_y,
                title,
                theme,
            );
        }
        push_er_header_divider(
            &mut table_svg,
            table.node_x,
            table.node_width,
            header_bottom,
            theme,
        );
        for (index, row) in table.rows.iter().enumerate() {
            let y = first_row_y + index as f32 * line_height;
            if index > 0 {
                push_er_row_divider(
                    &mut table_svg,
                    table.node_x,
                    table.node_width,
                    y - line_height * 0.65,
                    theme,
                );
            }
            push_er_text(
                &mut table_svg,
                "er-attribute-type",
                type_x,
                y,
                &row.data_type,
                theme,
            );
            push_er_text(
                &mut table_svg,
                "er-attribute-name",
                name_x,
                y,
                &row.name,
                theme,
            );
            push_er_text(
                &mut table_svg,
                "er-attribute-key",
                key_x,
                y,
                &row.key,
                theme,
            );
            push_er_text(
                &mut table_svg,
                "er-attribute-comment",
                comment_x,
                y,
                &row.comment,
                theme,
            );
        }
    }
    if let Some(closing_tag) = svg.rfind("</svg>") {
        svg.insert_str(closing_tag, &table_svg);
    }
    svg
}

fn push_er_title(
    svg: &mut String,
    x: f32,
    y: f32,
    title: &str,
    theme: &mermaid_rs_renderer::Theme,
) {
    svg.push_str(&format!(
        "<text class=\"er-title\" x=\"{x:.2}\" y=\"{y:.2}\" text-anchor=\"middle\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"900\" fill=\"{}\">{}</text>",
        xml_escape(&theme.font_family),
        theme.font_size * ER_TITLE_SCALE,
        xml_escape(&theme.primary_text_color),
        xml_escape(title),
    ));
}

fn push_er_header_divider(
    svg: &mut String,
    node_x: f32,
    node_width: f32,
    y: f32,
    theme: &mermaid_rs_renderer::Theme,
) {
    let inset = 6.0;
    svg.push_str(&format!(
        "<line class=\"er-header-divider\" x1=\"{:.2}\" y1=\"{y:.2}\" x2=\"{:.2}\" y2=\"{y:.2}\" stroke=\"{}\" stroke-width=\"1.0\"/>",
        node_x + inset,
        node_x + node_width - inset,
        xml_escape(&theme.primary_border_color),
    ));
}

fn push_er_row_divider(
    svg: &mut String,
    node_x: f32,
    node_width: f32,
    y: f32,
    theme: &mermaid_rs_renderer::Theme,
) {
    let inset = 6.0;
    svg.push_str(&format!(
        "<line class=\"er-row-divider\" x1=\"{:.2}\" y1=\"{y:.2}\" x2=\"{:.2}\" y2=\"{y:.2}\" stroke=\"{}\" stroke-width=\"0.6\" stroke-opacity=\"0.32\"/>",
        node_x + inset,
        node_x + node_width - inset,
        xml_escape(&theme.primary_border_color),
    ));
}

fn push_er_text(
    svg: &mut String,
    class: &str,
    x: f32,
    y: f32,
    text: &str,
    theme: &mermaid_rs_renderer::Theme,
) {
    if text.is_empty() {
        return;
    }
    svg.push_str(&format!(
        "<text class=\"{class}\" x=\"{x:.2}\" y=\"{y:.2}\" text-anchor=\"start\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
        xml_escape(&theme.font_family),
        theme.font_size,
        xml_escape(&theme.primary_text_color),
        xml_escape(text),
    ));
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn improve_sequence_layout(layout: &mut mermaid_rs_renderer::Layout, font_size: f32) {
    let original_centers = layout
        .lifelines
        .iter()
        .map(|lifeline| (lifeline.id.clone(), lifeline.x))
        .collect::<Vec<_>>();
    if original_centers.is_empty() {
        return;
    }

    for edge in &mut layout.edges {
        if let Some(label) = &mut edge.label {
            normalize_text_block(label, font_size, true);
        }
    }

    let mut gaps = original_centers
        .windows(2)
        .map(|centers| centers[1].1 - centers[0].1)
        .collect::<Vec<_>>();
    for (index, gap) in gaps.iter_mut().enumerate() {
        let left_width = participant_width(layout, &original_centers[index].0);
        let right_width = participant_width(layout, &original_centers[index + 1].0);
        *gap = gap.max(left_width / 2.0 + right_width / 2.0 + font_size);
    }
    for edge in &layout.edges {
        let Some(label) = &edge.label else {
            continue;
        };
        let Some(from) = participant_index(&original_centers, &edge.from) else {
            continue;
        };
        let Some(to) = participant_index(&original_centers, &edge.to) else {
            continue;
        };
        let (left, right) = if from < to { (from, to) } else { (to, from) };
        if left == right {
            continue;
        }
        let required_span = label.width + font_size * 3.0;
        let current_span: f32 = gaps[left..right].iter().sum();
        if required_span > current_span {
            let extra_per_gap = (required_span - current_span) / (right - left) as f32;
            for gap in &mut gaps[left..right] {
                *gap += extra_per_gap;
            }
        }
    }
    for note in &layout.sequence_notes {
        let Some((left, right)) = participant_span(&original_centers, &note.participants) else {
            continue;
        };
        if left == right {
            continue;
        }
        expand_span(&mut gaps, left, right, note.width + font_size * 2.0);
    }

    let mut new_centers = vec![original_centers[0].1];
    for gap in gaps {
        let previous = *new_centers
            .last()
            .expect("sequence has a first participant");
        new_centers.push(previous + gap);
    }
    let remap = |x: f32| remap_sequence_x(x, &original_centers, &new_centers);

    for (id, node) in &mut layout.nodes {
        if let Some(index) = participant_index(&original_centers, id) {
            node.x += new_centers[index] - original_centers[index].1;
        }
    }
    for lifeline in &mut layout.lifelines {
        lifeline.x = remap(lifeline.x);
    }
    for footbox in &mut layout.sequence_footboxes {
        let center = footbox.x + footbox.width / 2.0;
        footbox.x = remap(center) - footbox.width / 2.0;
    }
    for edge in &mut layout.edges {
        for point in &mut edge.points {
            point.0 = remap(point.0);
        }
    }
    for note in &mut layout.sequence_notes {
        let center = note.x + note.width / 2.0;
        note.x = remap(center) - note.width / 2.0;
    }
    for frame in &mut layout.sequence_frames {
        let right = remap(frame.x + frame.width);
        frame.x = remap(frame.x);
        frame.width = right - frame.x;
        frame.label_box.0 = remap(frame.label_box.0);
        frame.label.x = remap(frame.label.x);
        for label in &mut frame.section_labels {
            label.x = remap(label.x);
        }
    }
    for sequence_box in &mut layout.sequence_boxes {
        let right = remap(sequence_box.x + sequence_box.width);
        sequence_box.x = remap(sequence_box.x);
        sequence_box.width = right - sequence_box.x;
    }
    for activation in &mut layout.sequence_activations {
        activation.x = remap(activation.x);
    }
    for number in &mut layout.sequence_numbers {
        number.x = remap(number.x);
    }
    expand_sequence_over_notes(layout, font_size);
    keep_sequence_layout_inside_left_bound(layout);
    add_sequence_vertical_clearance(layout, font_size);
    add_sequence_message_clearance(layout, font_size);

    let added_width = new_centers.last().unwrap_or(&0.0)
        - original_centers.last().map(|(_, x)| x).unwrap_or(&0.0);
    layout.width += added_width.max(0.0);
}

fn expand_sequence_over_notes(layout: &mut mermaid_rs_renderer::Layout, font_size: f32) {
    let note_outset = font_size;
    let spans = layout
        .sequence_notes
        .iter()
        .map(|note| {
            if note.position != mermaid_rs_renderer::ir::SequenceNotePosition::Over {
                return None;
            }

            let mut participant_bounds = note.participants.iter().filter_map(|participant| {
                let center = layout
                    .lifelines
                    .iter()
                    .find(|lifeline| lifeline.id == *participant)?
                    .x;
                let half_width = participant_width(layout, participant) / 2.0;
                Some((center - half_width, center + half_width))
            });
            let (mut left, mut right) = participant_bounds.next()?;
            for (participant_left, participant_right) in participant_bounds {
                left = left.min(participant_left);
                right = right.max(participant_right);
            }
            Some((left - note_outset, right + note_outset))
        })
        .collect::<Vec<_>>();

    for (note, span) in layout.sequence_notes.iter_mut().zip(spans) {
        let Some((left, right)) = span else {
            continue;
        };
        let center = (left + right) / 2.0;
        note.width = note.width.max(right - left);
        note.x = center - note.width / 2.0;
    }
}

fn keep_sequence_layout_inside_left_bound(layout: &mut mermaid_rs_renderer::Layout) {
    let mut min_x = 0.0_f32;
    for node in layout
        .nodes
        .values()
        .chain(layout.sequence_footboxes.iter())
    {
        min_x = min_x.min(node.x);
    }
    for note in &layout.sequence_notes {
        min_x = min_x.min(note.x);
    }
    for frame in &layout.sequence_frames {
        min_x = min_x.min(frame.x).min(frame.label_box.0);
    }
    for sequence_box in &layout.sequence_boxes {
        min_x = min_x.min(sequence_box.x);
    }
    for edge in &layout.edges {
        for point in &edge.points {
            min_x = min_x.min(point.0);
        }
    }
    if min_x >= 0.0 {
        return;
    }

    let offset = -min_x;
    for node in layout.nodes.values_mut() {
        node.x += offset;
    }
    for lifeline in &mut layout.lifelines {
        lifeline.x += offset;
    }
    for footbox in &mut layout.sequence_footboxes {
        footbox.x += offset;
    }
    for edge in &mut layout.edges {
        for point in &mut edge.points {
            point.0 += offset;
        }
    }
    for note in &mut layout.sequence_notes {
        note.x += offset;
    }
    for frame in &mut layout.sequence_frames {
        frame.x += offset;
        frame.label_box.0 += offset;
        frame.label.x += offset;
        for label in &mut frame.section_labels {
            label.x += offset;
        }
    }
    for sequence_box in &mut layout.sequence_boxes {
        sequence_box.x += offset;
    }
    for activation in &mut layout.sequence_activations {
        activation.x += offset;
    }
    for number in &mut layout.sequence_numbers {
        number.x += offset;
    }
    layout.width += offset;
}

fn add_sequence_vertical_clearance(layout: &mut mermaid_rs_renderer::Layout, font_size: f32) {
    let clearance = font_size;
    let mut note_order = (0..layout.sequence_notes.len()).collect::<Vec<_>>();
    note_order.sort_by(|left, right| {
        layout.sequence_notes[*left]
            .y
            .total_cmp(&layout.sequence_notes[*right].y)
    });

    for note_index in note_order {
        let note_y = layout.sequence_notes[note_index].y;
        let note_bottom =
            layout.sequence_notes[note_index].y + layout.sequence_notes[note_index].height;
        let mut next_top = f32::INFINITY;

        for (other_index, note) in layout.sequence_notes.iter().enumerate() {
            if other_index != note_index && note.y > note_y {
                next_top = next_top.min(note.y);
            }
        }
        for edge in &layout.edges {
            let Some((_, edge_y)) = edge.points.first().copied() else {
                continue;
            };
            if edge_y <= note_y {
                continue;
            }
            let label_height = edge
                .label
                .as_ref()
                .map(|label| label.height)
                .unwrap_or(font_size);
            next_top = next_top.min(edge_y - label_height - font_size * 1.5);
        }

        if !next_top.is_finite() {
            continue;
        }
        let delta = note_bottom + clearance - next_top;
        if delta > 0.0 {
            shift_sequence_content_after(layout, note_y, delta);
        }
    }
}

fn add_sequence_message_clearance(layout: &mut mermaid_rs_renderer::Layout, font_size: f32) {
    let clearance = font_size;
    for current_index in 1..layout.edges.len() {
        let previous_bottom = layout.edges[current_index - 1]
            .points
            .iter()
            .map(|point| point.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let Some(current_y) = layout.edges[current_index]
            .points
            .first()
            .map(|point| point.1)
        else {
            continue;
        };
        let current_top = if let Some(label) = &layout.edges[current_index].label {
            current_y - label.height - font_size * 1.5
        } else {
            current_y
        };
        let delta = previous_bottom + clearance - current_top;
        if delta > 0.0 {
            shift_sequence_rows_from(layout, current_index, current_y, delta);
        }
    }
}

fn shift_sequence_rows_from(
    layout: &mut mermaid_rs_renderer::Layout,
    first_edge_index: usize,
    cutoff_y: f32,
    delta: f32,
) {
    for edge in layout.edges.iter_mut().skip(first_edge_index) {
        for point in &mut edge.points {
            point.1 += delta;
        }
    }
    for note in &mut layout.sequence_notes {
        if note.index >= first_edge_index {
            note.y += delta;
        }
    }
    shift_sequence_decorations_after(layout, cutoff_y - f32::EPSILON, delta);
}

fn shift_sequence_content_after(
    layout: &mut mermaid_rs_renderer::Layout,
    cutoff_y: f32,
    delta: f32,
) {
    for edge in &mut layout.edges {
        for point in &mut edge.points {
            if point.1 > cutoff_y {
                point.1 += delta;
            }
        }
    }
    for note in &mut layout.sequence_notes {
        if note.y > cutoff_y {
            note.y += delta;
        }
    }
    shift_sequence_decorations_after(layout, cutoff_y, delta);
}

fn shift_sequence_decorations_after(
    layout: &mut mermaid_rs_renderer::Layout,
    cutoff_y: f32,
    delta: f32,
) {
    for frame in &mut layout.sequence_frames {
        let bottom = frame.y + frame.height;
        if frame.y > cutoff_y {
            frame.y += delta;
            frame.label_box.1 += delta;
            frame.label.y += delta;
            for divider in &mut frame.dividers {
                *divider += delta;
            }
            for label in &mut frame.section_labels {
                label.y += delta;
            }
        } else if bottom > cutoff_y {
            frame.height += delta;
            for divider in &mut frame.dividers {
                if *divider > cutoff_y {
                    *divider += delta;
                }
            }
            for label in &mut frame.section_labels {
                if label.y > cutoff_y {
                    label.y += delta;
                }
            }
        }
    }
    for activation in &mut layout.sequence_activations {
        let bottom = activation.y + activation.height;
        if activation.y > cutoff_y {
            activation.y += delta;
        } else if bottom > cutoff_y {
            activation.height += delta;
        }
    }
    for number in &mut layout.sequence_numbers {
        if number.y > cutoff_y {
            number.y += delta;
        }
    }
    for lifeline in &mut layout.lifelines {
        if lifeline.y2 > cutoff_y {
            lifeline.y2 += delta;
        }
    }
    for footbox in &mut layout.sequence_footboxes {
        if footbox.y > cutoff_y {
            footbox.y += delta;
        }
    }
    for sequence_box in &mut layout.sequence_boxes {
        if sequence_box.y + sequence_box.height > cutoff_y {
            sequence_box.height += delta;
        }
    }
    layout.height += delta;
}

fn improve_general_layout(layout: &mut mermaid_rs_renderer::Layout, font_size: f32) {
    let clearance = if layout.kind == mermaid_rs_renderer::DiagramKind::Er {
        font_size * ER_ENTITY_CLEARANCE_SCALE
    } else {
        font_size
    };
    let nodes = layout
        .nodes
        .values()
        .map(|node| {
            (
                node.id.clone(),
                node.x + node.width / 2.0,
                node.y + node.height / 2.0,
                node.width,
                node.height,
            )
        })
        .collect::<Vec<_>>();
    if nodes.len() < 2 {
        return;
    }

    let mut scale_x = 1.0_f32;
    let mut scale_y = 1.0_f32;
    for (index, left) in nodes.iter().enumerate() {
        for right in nodes.iter().skip(index + 1) {
            let dx = (right.1 - left.1).abs();
            let dy = (right.2 - left.2).abs();
            if dx >= dy && dx > f32::EPSILON {
                let required = left.3 / 2.0 + right.3 / 2.0 + clearance;
                scale_x = scale_x.max(required / dx);
            } else if dy > f32::EPSILON {
                let required = left.4 / 2.0 + right.4 / 2.0 + clearance;
                scale_y = scale_y.max(required / dy);
            }
        }
    }
    for edge in &layout.edges {
        let Some(label) = &edge.label else {
            continue;
        };
        let Some(from) = layout.nodes.get(&edge.from) else {
            continue;
        };
        let Some(to) = layout.nodes.get(&edge.to) else {
            continue;
        };
        let dx = ((to.x + to.width / 2.0) - (from.x + from.width / 2.0)).abs();
        let dy = ((to.y + to.height / 2.0) - (from.y + from.height / 2.0)).abs();
        if dx >= dy && dx > f32::EPSILON {
            let required = from.width / 2.0 + label.width + to.width / 2.0 + clearance;
            scale_x = scale_x.max(required / dx);
        } else if dy > f32::EPSILON {
            let required = from.height / 2.0 + label.height + to.height / 2.0 + clearance;
            scale_y = scale_y.max(required / dy);
        }
    }

    let origin_x = nodes
        .iter()
        .map(|node| node.1)
        .fold(f32::INFINITY, f32::min);
    let origin_y = nodes
        .iter()
        .map(|node| node.2)
        .fold(f32::INFINITY, f32::min);
    let remap_x = |x: f32| origin_x + (x - origin_x) * scale_x;
    let remap_y = |y: f32| origin_y + (y - origin_y) * scale_y;
    for node in layout.nodes.values_mut() {
        let center_x = node.x + node.width / 2.0;
        let center_y = node.y + node.height / 2.0;
        node.x = remap_x(center_x) - node.width / 2.0;
        node.y = remap_y(center_y) - node.height / 2.0;
    }
    for edge in &mut layout.edges {
        for point in &mut edge.points {
            point.0 = remap_x(point.0);
            point.1 = remap_y(point.1);
        }
    }
    for subgraph in &mut layout.subgraphs {
        let left = remap_x(subgraph.x);
        let right = remap_x(subgraph.x + subgraph.width);
        let top = remap_y(subgraph.y);
        let bottom = remap_y(subgraph.y + subgraph.height);
        subgraph.x = left;
        subgraph.width = right - left;
        subgraph.y = top;
        subgraph.height = bottom - top;
    }
    for note in &mut layout.state_notes {
        let center_x = note.x + note.width / 2.0;
        let center_y = note.y + note.height / 2.0;
        note.x = remap_x(center_x) - note.width / 2.0;
        note.y = remap_y(center_y) - note.height / 2.0;
    }
}

fn keep_er_layout_inside_bounds(layout: &mut mermaid_rs_renderer::Layout, font_size: f32) {
    let padding = font_size * 2.0;
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;

    for node in layout.nodes.values() {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
    }
    for edge in &layout.edges {
        for &(x, y) in &edge.points {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() {
        return;
    }

    let offset_x = (padding - min_x).max(0.0);
    let offset_y = (padding - min_y).max(0.0);
    if offset_x == 0.0 && offset_y == 0.0 {
        return;
    }
    for node in layout.nodes.values_mut() {
        node.x += offset_x;
        node.y += offset_y;
    }
    for edge in &mut layout.edges {
        for point in &mut edge.points {
            point.0 += offset_x;
            point.1 += offset_y;
        }
    }
    layout.width += offset_x;
    layout.height += offset_y;
}

fn participant_index(centers: &[(String, f32)], id: &str) -> Option<usize> {
    centers
        .iter()
        .position(|(participant, _)| participant == id)
}

fn participant_width(layout: &mermaid_rs_renderer::Layout, id: &str) -> f32 {
    let header_width = layout.nodes.get(id).map(|node| node.width).unwrap_or(0.0);
    let footer_width = layout
        .sequence_footboxes
        .iter()
        .find(|node| node.id == id)
        .map(|node| node.width)
        .unwrap_or(0.0);
    header_width.max(footer_width)
}

fn participant_span(centers: &[(String, f32)], participants: &[String]) -> Option<(usize, usize)> {
    let mut indexes = participants
        .iter()
        .filter_map(|participant| participant_index(centers, participant));
    let first = indexes.next()?;
    let mut left = first;
    let mut right = first;
    for index in indexes {
        left = left.min(index);
        right = right.max(index);
    }
    Some((left, right))
}

fn expand_span(gaps: &mut [f32], left: usize, right: usize, required_span: f32) {
    let current_span: f32 = gaps[left..right].iter().sum();
    if required_span <= current_span {
        return;
    }
    let extra_per_gap = (required_span - current_span) / (right - left) as f32;
    for gap in &mut gaps[left..right] {
        *gap += extra_per_gap;
    }
}

fn normalize_text_block(
    block: &mut mermaid_rs_renderer::layout::TextBlock,
    font_size: f32,
    join_lines: bool,
) {
    if join_lines && block.lines.len() > 1 {
        block.lines = vec![block.lines.join(" ")];
    }
    block.width = block
        .lines
        .iter()
        .map(|line| readable_text_width(line, font_size))
        .fold(0.0, f32::max);
    block.height = block.lines.len().max(1) as f32 * font_size * 1.5;
}

fn readable_text_width(text: &str, font_size: f32) -> f32 {
    text.chars()
        .map(|character| {
            if character.is_ascii() {
                if character.is_whitespace() {
                    0.35
                } else {
                    0.62
                }
            } else {
                1.0
            }
        })
        .sum::<f32>()
        * font_size.max(16.0)
}

fn remap_sequence_x(x: f32, original: &[(String, f32)], expanded: &[f32]) -> f32 {
    if x <= original[0].1 {
        return x + expanded[0] - original[0].1;
    }
    for (index, pair) in original.windows(2).enumerate() {
        if x <= pair[1].1 {
            let old_gap = pair[1].1 - pair[0].1;
            let position = (x - pair[0].1) / old_gap;
            return expanded[index] + position * (expanded[index + 1] - expanded[index]);
        }
    }
    x + expanded[expanded.len() - 1] - original[original.len() - 1].1
}

fn extend_layout_bounds(layout: &mut mermaid_rs_renderer::Layout, font_size: f32) {
    let padding = font_size * 2.0;
    let mut max_x = layout.width - padding;
    let mut max_y = layout.height - padding;
    for node in layout
        .nodes
        .values()
        .chain(layout.sequence_footboxes.iter())
    {
        max_x = max_x.max(node.x + node.width);
        max_y = max_y.max(node.y + node.height);
    }
    for subgraph in &layout.subgraphs {
        max_x = max_x.max(subgraph.x + subgraph.width);
        max_y = max_y.max(subgraph.y + subgraph.height);
    }
    for note in &layout.sequence_notes {
        max_x = max_x.max(note.x + note.width);
        max_y = max_y.max(note.y + note.height);
    }
    for note in &layout.state_notes {
        max_x = max_x.max(note.x + note.width);
        max_y = max_y.max(note.y + note.height);
    }
    for edge in &layout.edges {
        for point in &edge.points {
            max_x = max_x.max(point.0);
            max_y = max_y.max(point.1);
        }
    }
    for frame in &layout.sequence_frames {
        max_x = max_x.max(frame.x + frame.width);
        max_y = max_y.max(frame.y + frame.height);
    }
    for sequence_box in &layout.sequence_boxes {
        max_x = max_x.max(sequence_box.x + sequence_box.width);
        max_y = max_y.max(sequence_box.y + sequence_box.height);
    }
    for legend in &layout.pie_legend {
        max_x = max_x.max(legend.x + legend.marker_size + padding + legend.label.width);
        max_y = max_y.max(legend.y + legend.label.height);
    }
    if let Some(gantt) = &layout.gantt {
        for task in &gantt.tasks {
            max_x = max_x.max(task.x + task.width);
            max_y = max_y.max(task.y + task.height);
        }
    }
    if let Some(timeline) = &layout.timeline {
        for event in &timeline.events {
            max_x = max_x.max(event.x + event.width);
            max_y = max_y.max(event.y + event.height);
        }
        for section in &timeline.sections {
            max_x = max_x.max(section.x + section.width);
            max_y = max_y.max(section.y + section.height);
        }
    }
    layout.width = layout.width.max(max_x + padding);
    layout.height = layout.height.max(max_y + padding);
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "unknown panic".to_owned()
    }
}

fn rasterize_svg(svg: &str, fontdb: Arc<usvg::fontdb::Database>) -> Result<ColorImage> {
    let mut options = usvg::Options::default();
    options.font_family = "sans-serif".to_owned();
    options.fontdb = fontdb;
    let tree = usvg::Tree::from_str(svg, &options).context("failed to parse Mermaid SVG")?;
    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height())
        .context("failed to allocate Mermaid image buffer")?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    Ok(ColorImage::from_rgba_unmultiplied(
        [size.width() as usize, size.height() as usize],
        pixmap.data(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        configure_er_layout, improve_general_layout, improve_sequence_layout,
        keep_er_layout_inside_bounds, normalize_layout_text, rasterize_svg, render_diagram_svg,
        MermaidRenderer, ER_NODE_SPACING, ER_RANK_SPACING,
    };

    fn refined_layout(source: &str) -> (mermaid_rs_renderer::Layout, f32) {
        let parsed = mermaid_rs_renderer::parse_mermaid(source).expect("diagram should parse");
        let options = mermaid_rs_renderer::RenderOptions::default();
        let mut layout =
            mermaid_rs_renderer::compute_layout(&parsed.graph, &options.theme, &options.layout);
        normalize_layout_text(&mut layout, options.theme.font_size);
        if matches!(
            layout.kind,
            mermaid_rs_renderer::DiagramKind::Sequence | mermaid_rs_renderer::DiagramKind::ZenUML
        ) {
            improve_sequence_layout(&mut layout, options.theme.font_size);
        } else {
            improve_general_layout(&mut layout, options.theme.font_size);
        }
        (layout, options.theme.font_size)
    }

    #[test]
    fn renders_flowchart_to_image() {
        let image = MermaidRenderer::new()
            .render_image(
                r#"flowchart LR
  A[Neovim] --> B[nvmd]
  B --> C[egui window]
"#,
            )
            .expect("Mermaid flowchart should render");

        assert!(image.size[0] > 1);
        assert!(image.size[1] > 1);
        assert!(!image.pixels.is_empty());
    }

    #[test]
    fn renders_sequence_diagram_with_japanese_text() {
        let image = MermaidRenderer::new()
            .render_image(
                r#"sequenceDiagram
  participant 管理者
  participant サーバー
  管理者->>サーバー: 承認を実行
  Note over 管理者,サーバー: 状態を更新
"#,
            )
            .expect("Mermaid Japanese sequence text should render");

        assert!(image.size[0] > 1);
        assert!(image.size[1] > 1);
        assert!(!image.pixels.is_empty());
    }

    #[test]
    fn renders_er_self_relationship_without_dagre_panic() {
        let svg = render_diagram_svg(
            r#"erDiagram
  departments {
    UUID id PK
  }
  projects {
    UUID id PK
  }
  departments ||--o{ departments : parent_of
  departments ||--o{ projects : owns
"#,
        )
        .expect("ER self-relationships should render");

        assert!(svg.contains("parent_of"));
        assert!(svg.contains("owns"));
    }

    #[test]
    fn renders_er_attributes_as_aligned_columns_on_single_rows() {
        let svg = render_diagram_svg(
            r#"erDiagram
  catalog_items {
    UUID id PK
    VARCHAR sku "UNIQUE when present"
    UUID category_id FK
  }
"#,
        )
        .expect("ER attributes should render");

        assert!(svg.contains("class=\"er-attribute-type\""));
        assert!(svg.contains("class=\"er-attribute-name\""));
        assert!(svg.contains("class=\"er-attribute-key\""));
        assert!(svg.contains("class=\"er-attribute-comment\""));
        assert!(svg.contains("class=\"er-title\""));
        assert!(svg.contains("font-weight=\"900\""));
        assert!(svg.contains(">catalog_items</text>"));
        assert!(svg.contains("class=\"er-header-divider\""));
        assert!(svg.contains("class=\"er-row-divider\""));
        assert!(svg.contains("stroke-width=\"0.6\""));
        assert!(svg.contains(">sku</text>"));
        assert!(svg.contains(">PK</text>"));
        assert!(svg.contains(">FK</text>"));
        assert!(svg.contains("&quot;UNIQUE when present&quot;"));
        assert!(!svg.contains(">VARCHAR license_no"));
    }

    #[test]
    fn gives_er_relationship_routes_room_between_entities() {
        let mut options = mermaid_rs_renderer::RenderOptions::default();
        let default_node_spacing = options.layout.node_spacing;
        let default_rank_spacing = options.layout.rank_spacing;

        configure_er_layout(&mut options);

        assert!(options.layout.node_spacing > default_node_spacing);
        assert!(options.layout.rank_spacing > default_rank_spacing);
        assert_eq!(options.layout.node_spacing, ER_NODE_SPACING);
        assert_eq!(options.layout.rank_spacing, ER_RANK_SPACING);
    }

    #[test]
    fn keeps_resized_er_entities_inside_the_svg_left_edge() {
        let source = r#"erDiagram
  pipeline_inputs {
    UUID id PK
    JSONB source_options
    JSONB processing_summary
  }
  pipeline_outputs {
    UUID id PK
    VARCHAR status "pending | processed | rejected | expired"
    VARCHAR result_label
  }
  pipeline_inputs ||--o{ pipeline_outputs : produces
"#;
        let parsed = mermaid_rs_renderer::parse_mermaid(source).expect("ER should parse");
        let mut options = mermaid_rs_renderer::RenderOptions::default();
        configure_er_layout(&mut options);
        let mut graph = parsed.graph.clone();
        for edge in &mut graph.edges {
            edge.label = None;
        }
        let mut layout =
            mermaid_rs_renderer::compute_layout(&graph, &options.theme, &options.layout);
        normalize_layout_text(&mut layout, options.theme.font_size);
        improve_general_layout(&mut layout, options.theme.font_size);
        keep_er_layout_inside_bounds(&mut layout, options.theme.font_size);

        let padding = options.theme.font_size * 2.0;
        assert!(layout
            .nodes
            .values()
            .all(|node| node.x + 0.1 >= padding && node.y + 0.1 >= padding));
    }

    #[test]
    fn expands_sequence_columns_for_long_message_labels() {
        let short =
            render_diagram_svg("sequenceDiagram\n  participant A\n  participant B\n  A->>B: OK\n")
                .expect("short sequence diagram should render");
        let long = render_diagram_svg(
            "sequenceDiagram\n  participant A\n  participant B\n  A->>B: POST /api/v1/tasks/search { state: pending_review }\n",
        )
        .expect("long sequence diagram should render");

        let short_tree = usvg::Tree::from_str(&short, &usvg::Options::default())
            .expect("short SVG should parse");
        let long_tree =
            usvg::Tree::from_str(&long, &usvg::Options::default()).expect("long SVG should parse");

        assert!(long_tree.size().width() > short_tree.size().width());
        assert!(!long.contains("POST\n"));
    }

    #[test]
    fn sequence_notes_expand_participant_span() {
        let (layout, font_size) = refined_layout(
            "sequenceDiagram\n  participant A\n  participant middle-participant\n  participant C\n  Note over A,C: 状態を更新する注意書き\n  A->>C: OK\n",
        );
        let note = layout
            .sequence_notes
            .first()
            .expect("note should be laid out");
        let left = layout.nodes.get("A").expect("A participant");
        let middle = layout
            .nodes
            .get("middle-participant")
            .expect("middle participant");
        let right = layout.nodes.get("C").expect("C participant");

        assert!(
            note.x <= left.x - font_size + 0.1,
            "note left {} does not cover actor edge {} with outset {}",
            note.x,
            left.x,
            font_size
        );
        assert!(
            note.x + note.width + 0.1 >= right.x + right.width + font_size,
            "note right {} does not cover actor edge {} with outset {}",
            note.x + note.width,
            right.x + right.width,
            font_size
        );
        assert!(
            note.x >= -0.1,
            "expanded note must remain inside SVG bounds"
        );
        assert!(note.x <= middle.x);
        assert!(note.x + note.width >= middle.x + middle.width);
    }

    #[test]
    fn sequence_note_over_one_participant_covers_actor_box() {
        let (layout, font_size) = refined_layout(
            "sequenceDiagram\n  participant long-component-backend\n  Note over long-component-backend: 状態\n",
        );
        let note = layout
            .sequence_notes
            .first()
            .expect("note should be laid out");
        let actor = layout
            .nodes
            .get("long-component-backend")
            .expect("participant should be laid out");

        assert!(
            note.x <= actor.x - font_size + 0.1,
            "note left {} does not cover actor edge {} with outset {}",
            note.x,
            actor.x,
            font_size
        );
        assert!(
            note.x + note.width + 0.1 >= actor.x + actor.width + font_size,
            "note right {} does not cover actor edge {} with outset {}",
            note.x + note.width,
            actor.x + actor.width,
            font_size
        );
        assert!(
            note.x >= -0.1,
            "expanded note must remain inside SVG bounds"
        );
    }

    #[test]
    fn sequence_notes_reserve_rows_before_following_messages() {
        let (layout, font_size) = refined_layout(
            "sequenceDiagram\n  participant A\n  participant B\n  Note over A,B: first line<br/>second line<br/>third line\n  A->>B: next action\n",
        );
        let note = layout
            .sequence_notes
            .first()
            .expect("note should be laid out");
        let edge = layout.edges.first().expect("message should be laid out");
        let message_y = edge.points.first().expect("edge has point").1;
        let message_height = edge.label.as_ref().expect("message has text").height;
        let message_top = message_y - message_height - font_size * 1.5;

        assert!(message_top + 0.1 >= note.y + note.height + font_size);
    }

    #[test]
    fn sequence_messages_leave_room_after_self_message_loops() {
        let (layout, font_size) = refined_layout(
            "sequenceDiagram\n  participant client\n  participant backend\n  backend->>backend: status: processing -> completed\n  backend->>client: 200 OK\n",
        );
        let self_message = &layout.edges[0];
        let following_message = &layout.edges[1];
        let self_bottom = self_message
            .points
            .iter()
            .map(|point| point.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let following_y = following_message.points.first().expect("message point").1;
        let following_label_height = following_message
            .label
            .as_ref()
            .expect("message label")
            .height;
        let following_top = following_y - following_label_height - font_size * 1.5;

        assert!(following_top + 0.1 >= self_bottom + font_size);
    }

    #[test]
    fn generic_long_node_boxes_leave_clearance() {
        let (layout, font_size) = refined_layout(
            "flowchart LR\n  A[very long source element label requiring calculated spacing] --> B[very long destination element label requiring calculated spacing]\n",
        );
        let from = layout.nodes.get("A").expect("source node");
        let to = layout.nodes.get("B").expect("target node");

        assert!(to.x + 0.1 >= from.x + from.width + font_size);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rasterizes_japanese_svg_glyphs_with_system_fonts() {
        let renderer = MermaidRenderer::new();
        let image = rasterize_svg(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="40">
  <text x="2" y="28" font-size="24" font-family="sans-serif">承認実行</text>
</svg>"#,
            renderer.fontdb,
        )
        .expect("Japanese SVG text should rasterize");

        assert!(image.pixels.iter().any(|pixel| pixel.a() > 0));
    }
}
