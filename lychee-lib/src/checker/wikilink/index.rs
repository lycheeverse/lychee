use log::info;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Mutex;
use std::{path::PathBuf, sync::Arc};
use walkdir::WalkDir;

/// Indexes a given directory mapping filenames to their corresponding path.
///
/// The `WikilinkIndex` recursively checks all subdirectories of the given
/// base directory mapping any found files to the path where they can be found.
/// Symlinks are ignored to prevent it from infinite loops.
#[derive(Clone, Debug)]
pub(crate) struct WikilinkIndex {
    filenames: Arc<Mutex<HashMap<OsString, PathBuf>>>,
    /// Local base directory
    local_base: PathBuf,
}

impl WikilinkIndex {
    pub(crate) fn new(local_base: PathBuf) -> Self {
        let index = Self {
            local_base,
            filenames: Arc::new(Mutex::new(HashMap::new())),
        };
        index.start_indexing();
        index
    }

    /// Populates the index of the `WikilinkIndex` on startup by walking
    /// the local base directory, mapping each filename to an absolute filepath.
    pub(crate) fn start_indexing(&self) {
        // Start file indexing only if the Base is valid and local
        info!(
            "Starting file indexing for wikilinks in {}",
            self.local_base.display()
        );

        for entry in WalkDir::new(&self.local_base)
            // actively ignore symlinks
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if let Some(filename) = entry.path().file_name() {
                self.filenames
                    .lock()
                    .unwrap()
                    .insert(filename.to_os_string(), entry.path().to_path_buf());
            }
        }
    }

    /// Checks the index for a filename. Returning the absolute path if the name is found,
    /// otherwise returning None
    pub(crate) fn contains_path(&self, path: &Path) -> Option<PathBuf> {
        self.filenames
            .lock()
            .unwrap()
            .get(path.file_name()?)
            .cloned()
    }
}
