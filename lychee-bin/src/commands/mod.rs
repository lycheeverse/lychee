pub(crate) mod check;
pub(crate) mod dump;

pub(crate) use check::check;
pub(crate) use dump::dump;

use std::sync::Arc;

use crate::cache::Cache;
use crate::formatters::response::ResponseFormatter;
use crate::options::Config;
use lychee_lib::Result;
use lychee_lib::{Client, Request};

/// Parameters passed to every command
pub(crate) struct CommandParams<S: futures::Stream<Item = Result<Request>>> {
    pub(crate) client: Client,
    pub(crate) cache: Arc<Cache>,
    pub(crate) requests: S,
    pub(crate) formatter: Box<dyn ResponseFormatter>,
    pub(crate) cfg: Config,
}
