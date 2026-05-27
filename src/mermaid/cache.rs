use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use sha2::{Digest, Sha256};

const THEME: &str = "default";
const RENDERER_VERSION: &str = "mermaid-rs-renderer-0.1.2-collision-spacing-v13";

#[derive(Debug, Clone)]
pub struct MermaidCache {
    dir: Option<PathBuf>,
}

impl MermaidCache {
    pub fn new() -> Self {
        let dir =
            ProjectDirs::from("", "", "nvmd").map(|project| project.cache_dir().join("mermaid"));
        Self { dir }
    }

    pub fn key(source: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(source.as_bytes());
        hasher.update(THEME.as_bytes());
        hasher.update(RENDERER_VERSION.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn read_svg(&self, source: &str) -> Option<String> {
        let path = self.path_for(source)?;
        fs::read_to_string(path).ok()
    }

    pub fn write_svg(&self, source: &str, svg: &str) -> Result<()> {
        let Some(path) = self.path_for(source) else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&path, svg)
            .with_context(|| format!("failed to write Mermaid cache {}", path.display()))
    }

    fn path_for(&self, source: &str) -> Option<PathBuf> {
        self.dir
            .as_ref()
            .map(|dir| dir.join(format!("{}.svg", Self::key(source))))
    }
}
