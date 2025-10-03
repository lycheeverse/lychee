use crate::{Base, ErrorKind, Uri};
use log::info;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Mutex;
use std::{path::PathBuf, sync::Arc};
use walkdir::WalkDir;

#[derive(Clone, Debug, Default)]
// Indexes a given directory for filenames and the corresponding path
pub(crate) struct WikilinkChecker {
    filenames: Arc<Mutex<HashMap<OsString, PathBuf>>>,
    basedir: Option<Base>,
}

impl WikilinkChecker {
    pub(crate) fn new(base: Option<Base>) -> Self {
        Self {
            basedir: base,
            ..default::Default()
        }
    }

    pub(crate) fn index_files(&self) {
        //Skip the indexing step in case the filenames are already populated
        if !self.filenames.lock().unwrap().is_empty() {
            return;
        }
        match self.basedir {
            None => {
                info!("File indexing for Wikilinks aborted as no base directory is specified");
            }
            Some(ref basetype) => match basetype {
                Base::Local(localbasename) => {
                    //Start file indexing only if the Base is valid and local
                    info!(
                        "Starting file indexing for wikilinks in {}",
                        localbasename.display()
                    );

                    let mut filenameslock = self.filenames.lock().unwrap();
                    for entry in WalkDir::new::<PathBuf>(localbasename.into())
                        //actively ignore symlinks
                        .follow_links(false)
                        .into_iter()
                        .filter_map(std::result::Result::ok)
                    {
                        if let Some(filename) = entry.path().file_name() {
                            filenameslock
                                .insert(filename.to_ascii_lowercase(), entry.path().to_path_buf());
                        }
                    }
                }
                // A remote base is of no use for the wikilink checker, silently skip over it
                Base::Remote(_remotebasename) => {}
            },
        }
    }

    pub(crate) fn check(&self, path: &Path, uri: &Uri) -> Result<PathBuf, ErrorKind> {
        match path.file_name() {
            None => Err(ErrorKind::InvalidFilePath(uri.clone())),
            Some(filename) => {
                let filenamelock = self.filenames.lock().unwrap();
                if filenamelock.contains_key(&filename.to_ascii_lowercase()) {
                    Ok(filenamelock
                        .get(&filename.to_ascii_lowercase())
                        .expect("Could not retrieve inserted Path for discovered Wikilink-Path"))
                    .cloned()
                } else {
                    Err(ErrorKind::InvalidFilePath(uri.clone()))
                }
            }
        }
    }
}
