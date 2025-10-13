use crate::{Base, ErrorKind, Result};
use log::{info, warn};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Mutex;
use std::{path::PathBuf, sync::Arc};
use walkdir::WalkDir;

/// Indexes a given directory mapping filenames to their corresponding path.
///
/// The `WikilinkChecker` Recursively checks all subdirectories of the given
/// base directory mapping any found files to the path where they can be found.
/// Symlinks are ignored to prevent it from infinite loops.
#[derive(Clone, Debug, Default)]
pub(crate) struct WikilinkChecker {
    filenames: Arc<Mutex<HashMap<OsString, PathBuf>>>,
    basedir: Option<Base>,
}

impl WikilinkChecker {
    pub(crate) fn new(base: Option<Base>) -> Option<Self> {
        if base.is_none() {
            None
        } else {
            warn!(
                "The Wikilink Checker could not be initialized because the base directory is missing"
            );
            Some(Self {
                basedir: base,
                ..Default::default()
            })
        }
    }

    /// Populates the index of the `WikilinkChecker` unless it is already populated.
    ///
    /// Recursively walks the base directory mapping each filename to an absolute filepath.
    /// Errors if no base directory is given or if it is recognized as remote
    pub(crate) fn setup_wikilinks_index(&self) -> Result<()> {
        // Skip the indexing step in case the filenames are already populated
        if !self.filenames.lock().unwrap().is_empty() {
            return Ok(());
        }
        match self.basedir {
            None => {
                warn!("File indexing for Wikilinks aborted as no base directory is specified");
                Ok(())
            }
            Some(ref base_type) => match base_type {
                Base::Local(local_base_name) => {
                    // Start file indexing only if the Base is valid and local
                    info!(
                        "Starting file indexing for wikilinks in {}",
                        local_base_name.display()
                    );

                    let mut lock = self
                        .filenames
                        .lock()
                        .map_err(|_| ErrorKind::MutexPoisoned)?;
                    for entry in WalkDir::new::<PathBuf>(local_base_name.into())
                        // actively ignore symlinks
                        .follow_links(false)
                        .into_iter()
                        .filter_map(std::result::Result::ok)
                    {
                        if let Some(filename) = entry.path().file_name() {
                            lock.insert(filename.to_ascii_lowercase(), entry.path().to_path_buf());
                        }
                    }
                    Ok(())
                }

                // A remote base is of no use for the wikilink checker, silently skip over it
                Base::Remote(remote_base_name) => {
                    warn!("Error using remote base url for checking wililinks: {remote_base_name}");
                    Err(ErrorKind::WikilinkCheckerInit(
                        "Remote Base Directory found, only local directories are allowed"
                            .to_string(),
                    ))
                }
            },
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
                    Some(
                        filename_lock.get(&filename.to_ascii_lowercase()).expect(
                            "Could not retrieve inserted Path for discovered Wikilink-Path",
                        ),
                    )
                    .cloned()
                } else {
                    None
                }
            }
        }
    }
}
