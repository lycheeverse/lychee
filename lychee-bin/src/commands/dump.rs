use lychee_lib::{Client, Collector, Request};
use lychee_lib::{Input, Result};
use std::io::{self, Write};
use tokio_stream::StreamExt;

use crate::options::Config;
use crate::ExitCode;

/// Dump all detected links to stdout without checking them
pub(crate) async fn dump(client: Client, inputs: Vec<Input>, cfg: Config) -> Result<ExitCode> {
    let requests = Collector::new(cfg.base.clone(), cfg.skip_missing)
        .from_iter(inputs)
        .await;
    tokio::pin!(requests);

    while let Some(request) = requests.next().await {
        let request = request?;

        if client.is_excluded(&request.uri) {
            continue;
        }

        // Avoid panic on broken pipe.
        // See https://github.com/rust-lang/rust/issues/46016
        // This can occur when piping the output of lychee
        // to another program like `grep`.
        if let Err(e) = write(&request, cfg.verbose) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("{}", e);
                return Ok(ExitCode::UnexpectedFailure);
            }
        }
    }

    Ok(ExitCode::Success)
}

/// Dump request to stdout
/// Only print source in verbose mode. This way the normal link output
/// can be fed into another tool without data mangling.
fn write(request: &Request, verbose: bool) -> io::Result<()> {
    let output = if verbose {
        request.to_string()
    } else {
        request.uri.to_string()
    };
    writeln!(io::stdout(), "{}", output)
}
