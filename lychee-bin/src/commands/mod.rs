pub(crate) mod check;
pub(crate) mod dump;
pub(crate) mod dump_inputs;
pub(crate) mod generate;

pub(crate) use check::check;
pub(crate) use dump::dump;
pub(crate) use dump_inputs::dump_inputs;

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::cache::Cache;
use crate::options::Config;
use lychee_lib::RequestError;
use lychee_lib::{Client, Request};

/// Parameters passed to every command
pub(crate) struct CommandParams<S: futures::Stream<Item = Result<Request, RequestError>>> {
    pub(crate) client: Client,
    pub(crate) cache: Cache,
    pub(crate) requests: S,
    pub(crate) cfg: Config,
}

/// Creates a writer that outputs to a file or stdout.
///
/// # Errors
///
/// Returns an error if the output file cannot be opened.
fn create_writer(output: Option<PathBuf>) -> lychee_lib::Result<Box<dyn Write>> {
    Ok(match output {
        Some(path) => Box::new(fs::OpenOptions::new().append(true).open(path)?),
        None => Box::new(io::stdout().lock()),
    })
}
