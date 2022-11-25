pub(crate) mod check;
pub(crate) mod dump;

pub(crate) use check::check;
pub(crate) use dump::dump;
use lychee_lib::ClientWrapper;

use std::sync::Arc;

use crate::cache::Cache;
use crate::formatters::response::ResponseFormatter;
use crate::options::Config;
use lychee_lib::Request;
use lychee_lib::Result;

/// Parameters passed to every command
pub(crate) struct CommandParams<S: futures::Stream<Item = Result<Request>>> {
    pub(crate) client: ClientWrapper,
    pub(crate) cache: Arc<Cache>,
    pub(crate) requests: S,
    pub(crate) formatter: Box<dyn ResponseFormatter>,
    pub(crate) cfg: Config,
}
