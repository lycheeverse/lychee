//! Directory builder to generate directories of local files for testing.
//!
//! This module provides [`DirBuilder`] which provides methods to easily
//! populate a given directory with files containing certain links. This
//! is intended to allow test fixtures to be defined within the test code.

use std::result::Result;
use std::path::PathBuf;
use std::path::Path;

pub struct DirBuilder {
    path: PathBuf,
}

impl DirBuilder {

    pub fn new(path: &Path) -> Self {
        Self { path: path.to_path_buf() }
    }

    pub fn dir(self, subpath: &str) -> Result<Self, String> {
        let subpath = Path::new(subpath);
        if !subpath.is_relative() {
            return Err("dir() subpath not relative".to_string());
        }
        std::fs::create_dir_all(self.path.join(subpath))
            .map_err(|_| "dir() create_dir_all")?;
        Ok(self)
    }

    pub fn raw(self, subpath: &str, contents: &[u8]) -> Result<Self, String> {
        std::fs::write(self.path.join(subpath), contents).map_err(|_| "raw() write")?;
        Ok(self)
    }

}

