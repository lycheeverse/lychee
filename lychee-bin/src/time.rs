use std::time::SystemTime;

pub(crate) type Timestamp = u64;

/// Get the current UNIX timestamp
///
/// # Panics
///
/// Panics when the system clock is incorrectly configured
pub(crate) fn timestamp() -> Timestamp {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!")
        .as_secs()
}
