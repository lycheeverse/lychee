use crate::{Base, Status, Uri};
use http::StatusCode;
use log::info;
use std::path::Path;
use std::sync::Mutex;
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use walkdir::WalkDir;

#[derive(Clone, Debug, Default)]
/// Indexes a given directory for filenames
pub(crate) struct WikilinkChecker {
    filenames: Arc<Mutex<HashSet<String>>>,
    basedir: Option<Base>,
}

impl WikilinkChecker {
    pub(crate) fn new(base: Option<Base>) -> Self {
        Self {
            filenames: Arc::new(Mutex::new(HashSet::new())),
            basedir: base,
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
                        .filter_map(|e| e.ok())
                    {
                        match entry.path().file_name() {
                            Some(filename) => {
                                filenameslock.insert(filename.to_string_lossy().to_string());
                            }
                            None => {}
                        }
                    }
                }
                // A remote base is of no use for the wikilink checker, silently skip over it
                Base::Remote(_remotebasename) => {}
            },
        }
    }

    pub(crate) fn check(&self, path: &Path, uri: &Uri) -> Status {
        match path.file_name() {
            None => Status::Error(crate::ErrorKind::InvalidFilePath(uri.clone())),
            Some(filename) => {
                if self
                    .filenames
                    .lock()
                    .unwrap()
                    .get(filename.to_str().unwrap())
                    .is_some()
                {
                    Status::Ok(StatusCode::OK)
                } else {
                    Status::Error(crate::ErrorKind::InvalidFilePath(uri.clone()))
                }
            }
        }
    }
}
