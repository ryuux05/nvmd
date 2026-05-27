use std::{fs, path::PathBuf};

#[derive(Debug, Default)]
pub struct CursorSync {
    path: Option<PathBuf>,
    last_line: Option<usize>,
}

impl CursorSync {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            last_line: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.path.is_some()
    }

    pub fn take_changed_line(&mut self) -> Option<usize> {
        let path = self.path.as_ref()?;
        let line = fs::read_to_string(path)
            .ok()?
            .trim()
            .parse::<usize>()
            .ok()?;
        if line == 0 || self.last_line == Some(line) {
            return None;
        }
        self.last_line = Some(line);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::CursorSync;

    #[test]
    fn emits_only_changed_valid_lines() {
        let path = std::env::temp_dir().join(format!(
            "nvmd-cursor-sync-{}-{}.txt",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut sync = CursorSync::new(Some(path.clone()));

        std::fs::write(&path, "12\n").expect("cursor state should write");
        assert_eq!(sync.take_changed_line(), Some(12));
        assert_eq!(sync.take_changed_line(), None);

        std::fs::write(&path, "18\n").expect("cursor state should update");
        assert_eq!(sync.take_changed_line(), Some(18));
        let _ = std::fs::remove_file(path);
    }
}
