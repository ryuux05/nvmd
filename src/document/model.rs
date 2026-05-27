#![allow(dead_code)]

use std::path::PathBuf;

use crate::mermaid::renderer::MermaidRenderState;

#[derive(Debug, Clone)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub block_source_lines: Vec<usize>,
    pub title: Option<String>,
    pub source_path: Option<PathBuf>,
    pub frontmatter: Option<Frontmatter>,
}

impl Document {
    pub fn block_index_for_line(&self, line: usize) -> Option<usize> {
        self.block_source_lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, start_line)| **start_line <= line)
            .map(|(index, _)| index)
            .or_else(|| (!self.blocks.is_empty()).then_some(0))
    }
}

/// Block-level Markdown elements.
#[derive(Debug, Clone)]
pub enum Block {
    Heading {
        level: u8,
        content: Vec<Inline>,
        id: Option<String>,
    },
    Paragraph {
        content: Vec<Inline>,
    },
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Quote {
        blocks: Vec<Block>,
    },
    HorizontalRule,
    HtmlBlock {
        html: String,
    },
    Table {
        alignments: Vec<TableAlignment>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    FootnoteDefinition {
        label: String,
        blocks: Vec<Block>,
    },
    DefinitionList {
        items: Vec<DefinitionListItem>,
    },
    MathBlock {
        expression: String,
    },
    Mermaid {
        source: String,
        render_state: MermaidRenderState,
    },
}

/// Inline Markdown elements used inside paragraph-like blocks.
#[derive(Debug, Clone)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Code(String),
    Link {
        destination: String,
        title: Option<String>,
        children: Vec<Inline>,
    },
    Image {
        destination: String,
        title: Option<String>,
        alt: Vec<Inline>,
    },
    Html(String),
    Math(String),
    SoftBreak,
    HardBreak,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy)]
pub enum TableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct DefinitionListItem {
    pub term: Vec<Inline>,
    pub definitions: Vec<Vec<Block>>,
}

#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub format: FrontmatterFormat,
    pub raw: String,
}

#[derive(Debug, Clone, Copy)]
pub enum FrontmatterFormat {
    Yaml,
    Toml,
    Json,
}

pub fn plain_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    push_plain_text(inlines, &mut text);
    text
}

fn push_plain_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(value)
            | Inline::Code(value)
            | Inline::Html(value)
            | Inline::Math(value) => out.push_str(value),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => push_plain_text(children, out),
            Inline::Link { children, .. } => push_plain_text(children, out),
            Inline::Image { alt, .. } => push_plain_text(alt, out),
            Inline::SoftBreak | Inline::HardBreak => out.push('\n'),
        }
    }
}
