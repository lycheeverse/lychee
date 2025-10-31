//! Directory builder to generate directories of local files for testing.
//!
//! This module provides [`DirBuilder`] which provides methods to easily
//! populate a given directory with files containing certain links. This
//! is intended to allow test fixtures to be defined within the test code.

use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::result::Result;

pub struct DirBuilder {
    path: PathBuf,
}

impl DirBuilder {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    fn make_path(&self, subpath: &str) -> Result<PathBuf, String> {
        let subpath = Path::new(subpath);
        if !subpath.is_relative() {
            return Err(format!("dir() subpath not relative: {subpath:?}"));
        }
        Ok(self.path.join(subpath))
    }

    fn append_bytes(&self, path: &Path, contents: &[u8]) -> Result<(), String> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(debug_to_string)?;

        let file = BufWriter::new(file);
        file.write_all(contents).map_err(debug_to_string)?;
        file.write_all(b"\n").map_err(debug_to_string)?;

        Ok(())
    }

    fn append_str(&self, path: &Path, s: &str) -> Result<(), String> {
        self.append_bytes(path, s.as_bytes())
    }

    pub fn dir(self, subpath: &str) -> Result<Self, String> {
        std::fs::create_dir_all(&self.make_path(subpath)?).map_err(debug_to_string)?;
        Ok(self)
    }

    pub fn raw(self, subpath: &str, contents: &[u8]) -> Result<Self, String> {
        self.append_bytes(&self.make_path(subpath)?, contents)?;
        Ok(self)
    }

    pub fn str(self, subpath: &str, contents: &str) -> Result<Self, String> {
        self.append_str(&self.make_path(subpath)?, contents)?;
        Ok(self)
    }

    pub fn html(self, subpath: &str, links: &[&str]) -> Result<Self, String> {
        let mut content = String::new();
        for link in links {
            content.push_str(&format!("<a href=\"{link}\">link</a>\n"));
        }
        self.str(subpath, &content)
    }

    pub fn html_anchors(self, subpath: &str, ids: &[&str]) -> Result<Self, String> {
        let mut content = String::new();
        for id in ids {
            content.push_str(&format!("<p id=\"{id}\">text</p>"));
        }
        self.str(subpath, &content)
    }

    pub fn md(self, subpath: &str, links: &[&str]) -> Result<Self, String> {
        let mut content = String::new();
        for link in links {
            content.push_str(&format!("[link]({link})\n"));
        }
        self.str(subpath, &content)
    }
}

// https://internals.rust-lang.org/t/to-debug-a-debug-counterpart-of-to-string/11228/3
fn debug_to_string<T: Debug>(t: T) -> String {
    use std::fmt::Write;
    let mut buf = String::new();
    buf.write_fmt(format_args!("{:?}", t))
        .expect("a Debug implementation returned an error unexpectedly");
    buf.shrink_to_fit();
    buf
}
