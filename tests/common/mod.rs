#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// A `fodelta` command with deterministic output settings for tests.
pub fn fodelta_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fodelta"));
    cmd.env("NO_COLOR", "1").env("TERM", "dumb");
    cmd
}

/// A temporary directory plus helpers to write fixtures and run fodelta in it.
pub struct TestRepo {
    dir: tempfile::TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        Self {
            dir: tempfile::TempDir::new().expect("create temp dir"),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn command(&self) -> Command {
        let mut cmd = fodelta_cmd();
        cmd.current_dir(self.path());
        cmd
    }

    pub fn write(&self, relative: &str, contents: impl AsRef<[u8]>) {
        let path = self.path().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&path, contents).expect("write fixture");
    }

    pub fn read(&self, relative: &str) -> Vec<u8> {
        std::fs::read(self.path().join(relative)).expect("read fixture")
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.path().join(relative).exists()
    }

    pub fn path_of(&self, relative: &str) -> PathBuf {
        self.path().join(relative)
    }
}
