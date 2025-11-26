use crate::Base;
use log::{info, warn};
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
    /// local base directory
    base: Base,
}

impl WikilinkIndex {
    pub(crate) fn new(local_base: Base) -> Self {
        let index = Self {
            base: local_base,
            filenames: Arc::new(Mutex::new(HashMap::new())),
        };
        index.start_indexing();
        index
    }

    /// Populates the index of the `WikilinkIndex` on startup
    ///
    /// Recursively walks the local base directory, mapping each filename to an absolute filepath.
    /// The Index stays empty if no base directory is supplied or if the base directory is remote
    pub(crate) fn start_indexing(&self) {
        match &self.base {
            Base::Local(local_base_name) => {
                // Start file indexing only if the Base is valid and local
                info!(
                    "Starting file indexing for wikilinks in {}",
                    local_base_name.display()
                );

                for entry in WalkDir::new::<PathBuf>(local_base_name.into())
                    // actively ignore symlinks
                    .follow_links(false)
                    .into_iter()
                    .filter_map(Result::ok)
                {
                    if let Some(filename) = entry.path().file_name() {
                        let mut lock = self.filenames.lock().unwrap();
                        lock.insert(filename.to_ascii_lowercase(), entry.path().to_path_buf());
                    }
                }
            }

            // A remote base is of no use for the wikilink checker, return an error to the user
            Base::Remote(remote_base_name) => {
                warn!("Error using remote base url for checking wililinks: {remote_base_name}");
            }
        }
    }

    /// Checks the index for a filename. Returning the absolute path if the name is found,
    /// otherwise returning None
    pub(crate) fn contains_path(&self, path: &Path) -> Option<PathBuf> {
        match path.file_name() {
            None => None,
            Some(filename) => {
                let filename_lock = self.filenames.lock().unwrap();
                if filename_lock.contains_key(&filename.to_ascii_lowercase()) {
                    filename_lock.get(&filename.to_ascii_lowercase()).cloned()
                } else {
                    None
                }
            }
        }
    }
}
