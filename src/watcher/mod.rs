use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use anyhow::{Context, Result};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<Event>>,
}

impl FileWatcher {
    pub fn watch(path: &Path) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(sender, Config::default())
            .context("failed to create file watcher")?;
        let watch_path = path.parent().unwrap_or_else(|| Path::new("."));
        watcher
            .watch(watch_path, RecursiveMode::NonRecursive)
            .with_context(|| format!("failed to watch {}", watch_path.display()))?;
        Ok(Self {
            _watcher: watcher,
            receiver,
        })
    }

    pub fn changed_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            if let Ok(event) = event {
                paths.extend(event.paths);
            }
        }
        paths
    }
}
