#![allow(dead_code)]

use std::fs;
use std::path::Path;

use tempfile::TempDir;

pub struct FixtureRepo {
    tempdir: TempDir,
}

impl FixtureRepo {
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            tempdir: tempfile::tempdir()?,
        })
    }

    pub fn path(&self) -> &Path {
        self.tempdir.path()
    }

    pub fn write(&self, rel: &str, contents: &str) -> Result<(), std::io::Error> {
        let path = self.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    }

    pub fn write_bytes(&self, rel: &str, contents: &[u8]) -> Result<(), std::io::Error> {
        let path = self.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    }

    pub fn symlink_file(&self, source_rel: &str, target_rel: &str) -> Result<(), std::io::Error> {
        let source = self.path().join(source_rel);
        let target = self.path().join(target_rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        create_symlink_file(&source, &target)
    }
}

#[cfg(unix)]
fn create_symlink_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_symlink_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    std::os::windows::fs::symlink_file(source, target)
}
