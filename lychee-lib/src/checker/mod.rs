//! Checker Module
//!
//! This module contains all checkers, which are responsible for checking the status of a URL.
//! Each checker implements [Handler](crate::chain::Handler).

pub(crate) mod file;
pub(crate) mod website;
