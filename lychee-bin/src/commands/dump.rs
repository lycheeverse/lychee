use lychee_lib::Request;
use lychee_lib::Result;
use std::fs;
use std::io::{self, Write};
use tokio_stream::StreamExt;

use crate::ExitCode;

use super::CommandParams;

/// Dump all detected links to stdout without checking them
pub(crate) async fn dump<S>(params: CommandParams<S>) -> Result<ExitCode>
where
    S: futures::Stream<Item = Result<Request>>,
{
    let requests = params.requests;
    tokio::pin!(requests);

    if let Some(outfile) = &params.cfg.output {
        fs::File::create(outfile)?;
    }

    let mut writer = if let Some(output) = &params.cfg.output {
        let out = fs::OpenOptions::new().append(true).open(output)?;
        Box::new(out) as Box<dyn Write>
    } else {
        let out = io::stdout();
        Box::new(out.lock()) as Box<dyn Write>
    };

    while let Some(request) = requests.next().await {
        let mut request = request?;

        // Apply URI remappings (if any)
        request.uri = params.client.remap(request.uri)?;

        // Avoid panic on broken pipe.
        // See https://github.com/rust-lang/rust/issues/46016
        // This can occur when piping the output of lychee
        // to another program like `grep`.

        let excluded = params.client.is_excluded(&request.uri);
        let verbose = params.cfg.verbose;

        if excluded && !verbose {
            continue;
        }
        if let Err(e) = write(&mut writer, &request, verbose, excluded) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("{e}");
                return Ok(ExitCode::UnexpectedFailure);
            }
        }
    }

    Ok(ExitCode::Success)
}

/// Dump request to stdout
fn write(
    mut writer: &mut Box<dyn Write>,
    request: &Request,
    verbose: bool,
    excluded: bool,
) -> io::Result<()> {
    let request = if verbose {
        // Only print source in verbose mode. This way the normal link output
        // can be fed into another tool without data mangling.
        request.to_string()
    } else {
        request.uri.to_string()
    };

    let out_str = if excluded {
        format!("{request} [excluded]")
    } else {
        format!("{request}")
    };

    write_out(&mut writer, out_str)
}

fn write_out(writer: &mut Box<dyn Write>, out_str: String) -> io::Result<()> {
    writeln!(writer, "{}", out_str)
}
