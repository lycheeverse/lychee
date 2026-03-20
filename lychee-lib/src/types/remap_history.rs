use crate::Status;
use serde::Serialize;
use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, Mutex},
};
use url::Url;

/// Represents a single [`Url`] remapping
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Remapping {
    pub original: Url,
    pub new: Url,
}

impl Display for Remapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} --> {}", self.original, self.new)?;
        Ok(())
    }
}

/// Keep track of remapped [`Url`]s for reporting
#[derive(Debug, Clone, Default)]
pub(crate) struct RemapHistory(Arc<Mutex<HashMap<Url, Url>>>);

impl RemapHistory {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Records a [`Remapping`]
    pub(crate) fn record_remap(&self, remapping: Remapping) {
        let mut map = self.0.lock().unwrap();
        map.insert(remapping.new, remapping.original);
    }

    /// Wrap the given [`Status`] in [`Status::Remapped`]
    /// if the given [`Url`] was remapped.
    pub(crate) fn handle_remapped(&self, new: &Url, status: Status) -> Status {
        if let Some(original) = self.get(new) {
            Status::Remapped(
                Box::new(status),
                Remapping {
                    original,
                    new: new.clone(),
                },
            )
        } else {
            status
        }
    }

    fn get(&self, original: &Url) -> Option<Url> {
        self.0.lock().ok()?.get(original).cloned()
    }
}
