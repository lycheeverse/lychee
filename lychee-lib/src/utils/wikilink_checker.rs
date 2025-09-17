use crate::{Base, ErrorKind, Uri};
use log::info;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
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
            filenames: Arc::new(Mutex::new(HashMap::new())),
            basedir: base,
        }
    }

    pub(crate) async fn index_files(&self) {
        //Skip the indexing step in case the filenames are already populated
        if !self.filenames.lock().await.is_empty() {
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

                    let mut filenameslock = self.filenames.lock().await;
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

    pub(crate) async fn check(&self, path: &Path, uri: &Uri) -> Result<PathBuf, ErrorKind> {
        match path.file_name() {
            None => Err(ErrorKind::InvalidFilePath(uri.clone())),
            Some(filename) => {
                let filenamelock = self.filenames.lock().await;
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
