use lychee_lib::Request;
use lychee_lib::Result;
use std::io::{self, Write};
use tokio_stream::StreamExt;

use crate::ExitCode;

use super::CommandParams;

/// Dump all detected links to stdout without checking them
pub(crate) async fn dump<'a, S>(params: CommandParams<S>) -> Result<ExitCode>
where
    S: futures::Stream<Item = Result<Request>>,
{
    let requests = params.requests;
    tokio::pin!(requests);

    while let Some(request) = requests.next().await {
        let request = request?;

        // Avoid panic on broken pipe.
        // See https://github.com/rust-lang/rust/issues/46016
        // This can occur when piping the output of lychee
        // to another program like `grep`.

        let excluded = params.client.is_excluded(&request.uri);
        let verbose = params.cfg.verbose;

        if excluded && !verbose {
            continue;
        }
        if let Err(e) = write(&request, verbose, excluded) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("{e}");
                return Ok(ExitCode::UnexpectedFailure);
            }
        }
    }

    Ok(ExitCode::Success)
}

/// Dump request to stdout
fn write(request: &Request, verbose: bool, excluded: bool) -> io::Result<()> {
    let request = if verbose {
        // Only print source in verbose mode. This way the normal link output
        // can be fed into another tool without data mangling.
        request.to_string()
    } else {
        request.uri.to_string()
    };
    if excluded {
        writeln!(io::stdout(), "{} [excluded]", request)
    } else {
        writeln!(io::stdout(), "{}", request)
    }
}
