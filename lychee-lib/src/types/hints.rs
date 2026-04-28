//! Provide the means to display practical user-friendly messages,
//! which are collected during runtime.

use std::{fmt::Display, sync::Mutex};

/// Hints are accumulated during the whole program invocation.
static HINTS: Mutex<Vec<Hint>> = Mutex::new(vec![]);

/// An informative and friendly message created during the invocation of the program
/// to be displayed before termination, to improve user experience.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hint(String);

impl Display for Hint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Hint {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Hint {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// Collect a [`Hint`] to optionally be shown to users
/// at a later point in time by using [`get_hints`].
///
///
/// # Panics
///
/// Panics if the mutex is poisoned
pub fn add_hint(hint: Hint) {
    HINTS.lock().unwrap().push(hint);
}

/// Format and collect a [`Hint`].
/// Helper macro for [`add_hint`].
#[macro_export]
macro_rules! hint {
    ($($arg:tt)*) => {{
        $crate::add_hint(format!($($arg)*).into());
    }};
}

/// Get [`Hint`]s to report to users
///
/// # Panics
///
/// Panics if the mutex is poisoned
pub fn get_hints() -> Vec<Hint> {
    let mut hints = HINTS.lock().unwrap().clone();
    hints.sort(); // for reproducible reporting
    hints
}
