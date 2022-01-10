use anyhow::Result;
use dashmap::DashMap;
use lychee_lib::{CacheStatus, Uri};
use serde::Deserialize;
use std::path::Path;

pub(crate) type Cache = DashMap<Uri, CacheStatus>;

pub(crate) trait StoreExt {
    fn store<T: AsRef<Path>>(&self, path: T) -> Result<()>;
    fn load<T: AsRef<Path>>(path: T) -> Result<Cache>;
}

#[derive(Debug, Deserialize)]
struct Record {
    uri: Uri,
    status: CacheStatus,
}

impl StoreExt for Cache {
    fn store<T: AsRef<Path>>(&self, path: T) -> Result<()> {
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_path(path)?;
        for result in self {
            wtr.serialize((result.key(), result.value()))?
        }
        Ok(())
    }

    fn load<T: AsRef<Path>>(path: T) -> Result<Cache> {
        let map = DashMap::new();
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_path(path)?;
        for result in rdr.deserialize() {
            let (uri, status): (Uri, CacheStatus) = result?;
            map.insert(uri, status);
        }
        Ok(map)
    }
}
