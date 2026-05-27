use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub fn load_markdown(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .with_context(|| format!("failed to read Markdown file: {}", path.display()))
}
