use log::error;
use lychee_lib::Request;
use lychee_lib::Result;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use tokio_stream::StreamExt;

use crate::verbosity::Verbosity;
use crate::ExitCode;

use super::CommandParams;

// Helper function to create an output writer.
//
// If the output file is not specified, it will use `stdout`.
//
// # Errors
//
// If the output file cannot be opened, an error is returned.
fn create_writer(output: Option<PathBuf>) -> Result<Box<dyn Write>> {
    let out = if let Some(output) = output {
        let out = fs::OpenOptions::new().append(true).open(output)?;
        Box::new(out) as Box<dyn Write>
    } else {
        let out = io::stdout();
        Box::new(out.lock()) as Box<dyn Write>
    };
    Ok(out)
}

/// Dump all detected links to stdout without checking them
pub(crate) async fn dump<S>(params: CommandParams<S>) -> Result<ExitCode>
where
    S: futures::Stream<Item = Result<Request>>,
{
    let requests = params.requests;
    tokio::pin!(requests);

    if let Some(out_file) = &params.cfg.output {
        fs::File::create(out_file)?;
    }

    let mut writer = create_writer(params.cfg.output)?;

    while let Some(request) = requests.next().await {
        let mut request = request?;

        // Apply URI remappings (if any)
        params.client.remap(&mut request.uri)?;

        // Avoid panic on broken pipe.
        // See https://github.com/rust-lang/rust/issues/46016
        // This can occur when piping the output of lychee
        // to another program like `grep`.

        let excluded = params.client.is_excluded(&request.uri);

        if excluded && params.cfg.verbose.log_level() < log::Level::Info {
            continue;
        }
        if let Err(e) = write(&mut writer, &request, &params.cfg.verbose, excluded) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                error!("{e}");
                return Ok(ExitCode::UnexpectedFailure);
            }
        }
    }

    Ok(ExitCode::Success)
}

/// Dump all input sources to stdout without detecting any links and checking
/// them.
pub(crate) async fn dump_inputs<S>(sources: S, output: Option<&PathBuf>) -> Result<ExitCode>
where
    S: futures::Stream<Item = Result<String>>,
{
    let sources = sources;
    tokio::pin!(sources);

    if let Some(out_file) = output {
        fs::File::create(out_file)?;
    }

    let mut writer = create_writer(output.cloned())?;

    while let Some(source) = sources.next().await {
        let source = source?;

        writeln!(writer, "{source}")?;
    }

    Ok(ExitCode::Success)
}

/// Dump request to stdout
fn write(
    writer: &mut Box<dyn Write>,
    request: &Request,
    verbosity: &Verbosity,
    excluded: bool,
) -> io::Result<()> {
    // Only print `data:` URIs if verbose mode is at least `info`.
    if request.uri.is_data() && verbosity.log_level() < log::Level::Info {
        return Ok(());
    }

    // Only print source if verbose mode is at least `info`. This way the normal
    // link output can be fed into another tool without data mangling.
    let request = if verbosity.log_level() >= log::Level::Info {
        request.to_string()
    } else {
        request.uri.to_string()
    };

    // Mark excluded links
    let out_str = if excluded {
        format!("{request} [excluded]")
    } else {
        request
    };

    write_out(writer, &out_str)
}

fn write_out(writer: &mut Box<dyn Write>, out_str: &str) -> io::Result<()> {
    writeln!(writer, "{out_str}")
}
