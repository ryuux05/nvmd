use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::document::model::{plain_text, Block, Document, Inline, ListItem, TableAlignment};
use crate::mermaid::renderer::MermaidRenderState;

pub fn parse_markdown(source: &str) -> Document {
    let mut builder = Builder::default();
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options);
    for event in parser {
        builder.handle(event);
    }
    let mut document = builder.finish();
    document.block_source_lines = top_level_block_lines(source, options);
    document
}

#[derive(Debug, Default)]
struct Builder {
    blocks: Vec<Block>,
    quote_stack: Vec<Vec<Block>>,
    mode: Mode,
    text: String,
    inlines: Vec<Inline>,
    inline_stack: Vec<InlineFrame>,
    list_items: Vec<ListItem>,
    list_ordered: bool,
    list_start: Option<u64>,
    current_item_checked: Option<bool>,
    table_alignments: Vec<TableAlignment>,
    table_header: Vec<Vec<Inline>>,
    table_rows: Vec<Vec<Vec<Inline>>>,
    table_row: Vec<Vec<Inline>>,
    table_in_header: bool,
}

#[derive(Debug, Default)]
enum Mode {
    #[default]
    None,
    Paragraph,
    Heading(u8),
    Code(Option<String>),
    HtmlBlock,
    List,
    Item,
    TableCell,
}

#[derive(Debug)]
struct InlineFrame {
    kind: InlineKind,
    children: Vec<Inline>,
}

#[derive(Debug)]
enum InlineKind {
    Emphasis,
    Strong,
    Strikethrough,
    Link {
        destination: String,
        title: Option<String>,
    },
    Image {
        destination: String,
        title: Option<String>,
    },
}

impl Builder {
    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.push_text(&text),
            Event::Code(code) if matches!(self.mode, Mode::Code(_)) => self.push_text(&code),
            Event::Code(code) => self.push_inline(Inline::Code(code.into_string())),
            Event::Html(html) if matches!(self.mode, Mode::HtmlBlock) => self.push_text(&html),
            Event::Html(html) => self.push_inline(Inline::Html(html.into_string())),
            Event::InlineHtml(html) => self.push_inline(Inline::Html(html.into_string())),
            Event::SoftBreak => self.push_inline(Inline::SoftBreak),
            Event::HardBreak => self.push_inline(Inline::HardBreak),
            Event::Rule => self.push_block(Block::HorizontalRule),
            Event::TaskListMarker(checked) => self.current_item_checked = Some(checked),
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                if !matches!(self.mode, Mode::Item) {
                    self.mode = Mode::Paragraph;
                    self.clear_inline_state();
                }
            }
            Tag::Heading { level, .. } => {
                self.mode = Mode::Heading(heading_level(level));
                self.clear_inline_state();
            }
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let lang = lang.trim().to_owned();
                        (!lang.is_empty()).then_some(lang)
                    }
                    CodeBlockKind::Indented => None,
                };
                self.mode = Mode::Code(language);
                self.text.clear();
            }
            Tag::HtmlBlock => {
                self.mode = Mode::HtmlBlock;
                self.text.clear();
            }
            Tag::List(start) => {
                self.mode = Mode::List;
                self.list_items.clear();
                self.list_ordered = start.is_some();
                self.list_start = start;
            }
            Tag::Item => {
                self.mode = Mode::Item;
                self.current_item_checked = None;
                self.clear_inline_state();
            }
            Tag::BlockQuote => {
                self.quote_stack.push(Vec::new());
            }
            Tag::Table(alignments) => {
                self.table_alignments = alignments.into_iter().map(table_alignment).collect();
                self.table_header.clear();
                self.table_rows.clear();
                self.table_row.clear();
                self.table_in_header = false;
            }
            Tag::TableHead => {
                self.table_in_header = true;
                self.table_row.clear();
            }
            Tag::TableRow => {
                self.table_row.clear();
            }
            Tag::TableCell => {
                self.mode = Mode::TableCell;
                self.clear_inline_state();
            }
            Tag::Emphasis => self.start_inline(InlineKind::Emphasis),
            Tag::Strong => self.start_inline(InlineKind::Strong),
            Tag::Strikethrough => self.start_inline(InlineKind::Strikethrough),
            Tag::Link {
                dest_url, title, ..
            } => self.start_inline(InlineKind::Link {
                destination: dest_url.into_string(),
                title: (!title.is_empty()).then(|| title.into_string()),
            }),
            Tag::Image {
                dest_url, title, ..
            } => self.start_inline(InlineKind::Image {
                destination: dest_url.into_string(),
                title: (!title.is_empty()).then(|| title.into_string()),
            }),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph if matches!(self.mode, Mode::Item) => {}
            TagEnd::Paragraph => self.finish_text_block(TextKind::Paragraph),
            TagEnd::Heading(_) => self.finish_text_block(TextKind::Heading),
            TagEnd::CodeBlock => self.finish_code_block(),
            TagEnd::HtmlBlock => self.finish_html_block(),
            TagEnd::Item => {
                let inlines = self.take_inlines();
                if !inlines.is_empty() {
                    self.list_items.push(ListItem {
                        checked: self.current_item_checked.take(),
                        blocks: vec![Block::Paragraph { content: inlines }],
                    });
                }
                self.clear_inline_state();
                self.mode = Mode::List;
            }
            TagEnd::List(_) => {
                let items = std::mem::take(&mut self.list_items);
                let start = self.list_start.take();
                self.push_block(Block::List {
                    ordered: self.list_ordered,
                    start,
                    items,
                });
                self.mode = Mode::None;
            }
            TagEnd::TableCell => {
                let cell = self.take_inlines();
                self.table_row.push(cell);
                self.mode = Mode::None;
            }
            TagEnd::TableRow => {
                let row = std::mem::take(&mut self.table_row);
                if self.table_in_header {
                    self.table_header = row;
                } else {
                    self.table_rows.push(row);
                }
            }
            TagEnd::TableHead => {
                if !self.table_row.is_empty() {
                    self.table_header = std::mem::take(&mut self.table_row);
                }
                self.table_in_header = false;
            }
            TagEnd::Table => {
                let alignments = std::mem::take(&mut self.table_alignments);
                let header = std::mem::take(&mut self.table_header);
                let rows = std::mem::take(&mut self.table_rows);
                self.push_block(Block::Table {
                    alignments,
                    header,
                    rows,
                });
                self.mode = Mode::None;
            }
            TagEnd::BlockQuote => {
                if let Some(blocks) = self.quote_stack.pop() {
                    self.push_block(Block::Quote { blocks });
                }
            }
            TagEnd::Emphasis => self.finish_inline(),
            TagEnd::Strong => self.finish_inline(),
            TagEnd::Strikethrough => self.finish_inline(),
            TagEnd::Link => self.finish_inline(),
            TagEnd::Image => self.finish_inline(),
            _ => {}
        }
    }

    fn push_text(&mut self, value: &str) {
        match self.mode {
            Mode::Code(_) | Mode::HtmlBlock => {
                self.text.push_str(value);
            }
            Mode::Paragraph | Mode::Heading(_) | Mode::Item | Mode::TableCell => {
                self.push_inline(Inline::Text(value.to_owned()));
            }
            Mode::List | Mode::None => {}
        }
    }

    fn finish_text_block(&mut self, kind: TextKind) {
        let content = self.take_inlines();
        if content.is_empty() {
            self.mode = Mode::None;
            return;
        }

        let block = match (kind, &self.mode) {
            (TextKind::Heading, Mode::Heading(level)) => Block::Heading {
                level: *level,
                content,
                id: None,
            },
            _ => Block::Paragraph { content },
        };
        self.push_block(block);
        self.mode = Mode::None;
    }

    fn finish_html_block(&mut self) {
        let html = std::mem::take(&mut self.text);
        if !html.trim().is_empty() {
            self.push_block(Block::HtmlBlock { html });
        }
        self.mode = Mode::None;
    }

    fn finish_code_block(&mut self) {
        let code = std::mem::take(&mut self.text);
        let language = match &self.mode {
            Mode::Code(language) => language.clone(),
            _ => None,
        };

        let block = if language
            .as_deref()
            .map(|lang| lang.eq_ignore_ascii_case("mermaid"))
            .unwrap_or(false)
        {
            Block::Mermaid {
                source: code,
                render_state: MermaidRenderState::Pending,
            }
        } else {
            Block::CodeBlock { language, code }
        };

        self.push_block(block);
        self.mode = Mode::None;
    }

    fn push_block(&mut self, block: Block) {
        if let Some(quote_blocks) = self.quote_stack.last_mut() {
            quote_blocks.push(block);
        } else {
            self.blocks.push(block);
        }
    }

    fn clear_inline_state(&mut self) {
        self.inlines.clear();
        self.inline_stack.clear();
    }

    fn push_inline(&mut self, inline: Inline) {
        if let Some(frame) = self.inline_stack.last_mut() {
            frame.children.push(inline);
        } else {
            self.inlines.push(inline);
        }
    }

    fn start_inline(&mut self, kind: InlineKind) {
        if matches!(
            self.mode,
            Mode::Paragraph | Mode::Heading(_) | Mode::Item | Mode::TableCell
        ) {
            self.inline_stack.push(InlineFrame {
                kind,
                children: Vec::new(),
            });
        }
    }

    fn finish_inline(&mut self) {
        let Some(frame) = self.inline_stack.pop() else {
            return;
        };
        let inline = match frame.kind {
            InlineKind::Emphasis => Inline::Emphasis(frame.children),
            InlineKind::Strong => Inline::Strong(frame.children),
            InlineKind::Strikethrough => Inline::Strikethrough(frame.children),
            InlineKind::Link { destination, title } => Inline::Link {
                destination,
                title,
                children: frame.children,
            },
            InlineKind::Image { destination, title } => Inline::Image {
                destination,
                title,
                alt: frame.children,
            },
        };
        self.push_inline(inline);
    }

    fn take_inlines(&mut self) -> Vec<Inline> {
        while !self.inline_stack.is_empty() {
            self.finish_inline();
        }
        trim_inlines(std::mem::take(&mut self.inlines))
    }

    fn finish(self) -> Document {
        let title = self.blocks.iter().find_map(|block| match block {
            Block::Heading {
                level: 1, content, ..
            } => Some(plain_text(content)),
            _ => None,
        });

        Document {
            blocks: self.blocks,
            block_source_lines: Vec::new(),
            title,
            source_path: None,
            frontmatter: None,
        }
    }
}

fn top_level_block_lines(source: &str, options: Options) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut active_depth = 0_usize;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(tag) if active_depth == 0 && starts_top_level_block(&tag) => {
                lines.push(line_at_offset(source, range.start));
                active_depth = 1;
            }
            Event::Start(_) if active_depth > 0 => active_depth += 1,
            Event::End(_) if active_depth > 0 => active_depth -= 1,
            Event::Rule if active_depth == 0 => lines.push(line_at_offset(source, range.start)),
            _ => {}
        }
    }
    lines
}

fn starts_top_level_block(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::BlockQuote
            | Tag::Table(_)
    )
}

fn line_at_offset(source: &str, offset: usize) -> usize {
    source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

enum TextKind {
    Paragraph,
    Heading,
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn table_alignment(alignment: Alignment) -> TableAlignment {
    match alignment {
        Alignment::None => TableAlignment::None,
        Alignment::Left => TableAlignment::Left,
        Alignment::Center => TableAlignment::Center,
        Alignment::Right => TableAlignment::Right,
    }
}

fn trim_inlines(mut inlines: Vec<Inline>) -> Vec<Inline> {
    trim_inline_start(&mut inlines);
    trim_inline_end(&mut inlines);
    inlines
}

fn trim_inline_start(inlines: &mut Vec<Inline>) {
    if let Some(Inline::Text(text)) = inlines.first_mut() {
        let trimmed = text.trim_start().to_owned();
        *text = trimmed;
        if text.is_empty() {
            inlines.remove(0);
        }
    }
}

fn trim_inline_end(inlines: &mut Vec<Inline>) {
    if let Some(Inline::Text(text)) = inlines.last_mut() {
        let trimmed = text.trim_end().to_owned();
        *text = trimmed;
        if text.is_empty() {
            inlines.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_markdown;
    use crate::document::model::{plain_text, Block, Inline};

    #[test]
    fn parses_mermaid_fenced_code_as_mermaid_block() {
        let document = parse_markdown(
            r#"# Diagram

```mermaid
flowchart LR
  A --> B
```
"#,
        );

        assert!(document
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Mermaid { .. })));
        assert_eq!(document.block_source_lines, vec![1, 3]);
    }

    #[test]
    fn records_source_lines_for_top_level_rendered_blocks() {
        let document = parse_markdown(
            "# Title\n\ntext\n\n- first\n- second\n\n> quoted\n\n---\n\n```rust\nfn main() {}\n```\n",
        );

        assert_eq!(document.block_source_lines, vec![1, 3, 5, 8, 10, 12]);
        assert_eq!(document.block_index_for_line(1), Some(0));
        assert_eq!(document.block_index_for_line(7), Some(2));
        assert_eq!(document.block_index_for_line(13), Some(5));
    }

    #[test]
    fn parses_emphasis_and_strong_as_inline_nodes() {
        let document = parse_markdown("This is *italic* and **bold**.");

        let Block::Paragraph { content } = &document.blocks[0] else {
            panic!("expected paragraph");
        };

        assert!(content
            .iter()
            .any(|inline| matches!(inline, Inline::Emphasis(_))));
        assert!(content
            .iter()
            .any(|inline| matches!(inline, Inline::Strong(_))));
    }

    #[test]
    fn parses_github_table() {
        let document = parse_markdown(
            r#"| Name | Value |
| ---- | ----- |
| one  | two   |
"#,
        );

        let Block::Table { header, rows, .. } = &document.blocks[0] else {
            panic!("expected table");
        };

        assert_eq!(plain_text(&header[0]), "Name");
        assert_eq!(plain_text(&rows[0][1]), "two");
    }

    #[test]
    fn parses_task_list_marker() {
        let document = parse_markdown("- [x] done\n");

        let Block::List { items, .. } = &document.blocks[0] else {
            panic!("expected list");
        };

        assert_eq!(items[0].checked, Some(true));
    }
}
