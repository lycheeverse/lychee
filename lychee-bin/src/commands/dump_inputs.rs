use crate::commands::helpers::create_writer;
use lychee_lib::InputSource;
use lychee_lib::Request;
use lychee_lib::Result;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use tokio_stream::StreamExt;

use crate::ExitCode;

use super::CommandParams;

/// Dumps all detected inputs to stdout without checking them
/// This is useful for debugging purposes.
///
/// The implementation is suboptimal at the moment.
/// - The collected inputs get read from disk before being written to stdout.
/// - If inputs don't contain any links, they are ignored.
///
/// It would be better to just stream the inputs and not load them into memory.
/// This would require some refactoring in the `lychee-lib` crate,
/// so it is left as a TODO for now.
pub(crate) async fn dump_inputs<S>(params: CommandParams<S>) -> Result<ExitCode>
where
    S: futures::Stream<Item = Result<Request>>,
{
    let requests = params.requests;

    if let Some(outfile) = &params.cfg.output {
        fs::File::create(outfile)?;
    }
    let mut writer = create_writer(params.cfg.output)?;

    // A cache to avoid printing duplicate inputs
    let mut cache = HashSet::new();

    tokio::pin!(requests);
    while let Some(request) = requests.next().await {
        let request = request?;
        let source = request.source;

        if cache.contains(&source) {
            continue;
        }
        cache.insert(source.clone());

        if let Err(e) = write(&mut writer, &source) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("{e}");
                return Ok(ExitCode::UnexpectedFailure);
            }
        }
    }

    Ok(ExitCode::Success)
}

/// Dump request to stdout
fn write(writer: &mut Box<dyn Write>, source: &InputSource) -> io::Result<()> {
    writeln!(writer, "{}", source)
}
