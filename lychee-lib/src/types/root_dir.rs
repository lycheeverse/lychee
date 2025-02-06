use crate::Result;
use std::{
    fmt::{Display, Formatter},
    ops::Deref,
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use super::{ErrorKind, Input, InputSource};

/// The canonical root directory during document processing.
///
/// This is used to resolve relative paths in a document,
/// similar to how a web server resolves relative URLs.
///
/// Similar mechanisms exist in:
/// - Apache's `DocumentRoot`
/// - Nginx's `root` directive
///
/// The root directory can be only be a local path.
/// Paths must be absolute or are expected to be relative to the current working
/// directory.
///
/// For resolving remote URLs instead, see [`Base`].
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct RootDir(PathBuf);

impl RootDir {
    /// Creates a new root directory
    ///
    /// Root directories must be absolute paths.
    /// If the given path is relative, it will be resolved relative to the current working directory
    /// and then canonicalized to resolve symbolic links.
    ///
    /// # Examples
    ///
    /// ```
    /// use lychee_lib::types::RootDir;
    ///
    /// let root_dir = RootDir::new("/path/to/root").unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the current working directory cannot be determined
    /// or the path can not be canonicalized.
    ///
    /// Amongst other reasons, this can happen if:
    /// * `path` does not exist.
    /// * A non-final component in path is not a directory.
    pub fn new<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let path = path.into();
        let root_dir = if path.is_relative() {
            // The root directory must be an absolute path
            // Canonicalize the path relative to the current working directory
            let root_dir = std::env::current_dir()?.join(&path);
            root_dir
                .canonicalize()
                .map_err(|_| ErrorKind::InvalidRootDir(path.display().to_string()))?;
            root_dir
        } else {
            path.clone()
        };

        println!("Set root directory to: {}", root_dir.display());

        Ok(Self(root_dir))
    }
}

impl Display for RootDir {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl Deref for RootDir {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<RootDir> for PathBuf {
    fn from(root_dir: RootDir) -> Self {
        root_dir.0
    }
}

impl TryFrom<Input> for RootDir {
    type Error = ErrorKind;

    fn try_from(input: Input) -> std::result::Result<Self, Self::Error> {
        let source = input.source;
        match source {
            InputSource::FsPath(path) => {
                if path.is_dir() {
                    Ok(Self::new(path)?)
                } else {
                    Err(ErrorKind::InvalidRootDir(path.display().to_string()))
                }
            }
            _ => Err(ErrorKind::InvalidRootDir(
                "Root directory must be a local path".to_string(),
            )),
        }
    }
}

impl TryFrom<&Input> for RootDir {
    type Error = ErrorKind;

    fn try_from(input: &Input) -> std::result::Result<Self, Self::Error> {
        RootDir::try_from(input.clone())
    }
}

impl TryFrom<&str> for RootDir {
    type Error = ErrorKind;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        RootDir::new(value)
    }
}
