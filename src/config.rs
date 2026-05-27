use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub markdown_path: PathBuf,
    _base_dir: PathBuf,
}

impl Config {
    pub fn new(path: PathBuf) -> Result<Self> {
        let markdown_path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .context("failed to resolve current directory")?
                .join(path)
        };
        let base_dir = markdown_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        Ok(Self {
            markdown_path,
            _base_dir: base_dir,
        })
    }

    pub fn file_name(&self) -> String {
        self.markdown_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Markdown")
            .to_owned()
    }

    pub fn fallback() -> Self {
        Self {
            markdown_path: PathBuf::from("<startup failed>"),
            _base_dir: PathBuf::from("."),
        }
    }
}
