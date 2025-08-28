use std::{collections::HashSet, path::PathBuf, sync::Arc};

use std::sync::Mutex;
use walkdir::WalkDir;

use crate::Base;

#[derive(Clone, Debug, Default)]
/// Indexes a given directory for filenames
pub(crate) struct WikilinkChecker {
    filesnames: Arc<Mutex<HashSet<String>>>,
    basedir: Option<Base>,
}

impl WikilinkChecker {
    pub(crate) fn new(base: Option<Base>) -> Self {
        Self {
            filesnames: Arc::new(Mutex::new(HashSet::with_capacity(100000000))),
            basedir: base,
        }
    }

    pub(crate) fn index_files(&self) {
        match self.basedir {
            None => {}
            Some(ref basetype) => match basetype {
                Base::Local(localbasename) => {
                    //Start file indexing only if the Base is valid and local

                    let mut filenameslock = self.filesnames.lock().unwrap();
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
                // A remote base is of no use for the wikilink checker
                Base::Remote(_remotebasename) => {}
            },
        }
    }
}
