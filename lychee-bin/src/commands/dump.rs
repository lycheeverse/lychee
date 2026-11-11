use log::debug;
use log::error;
use log::warn;
use lychee_lib::Request;
use lychee_lib::RequestError;
use serde::Serialize;
use std::fs;
use std::io::{self, Write};
use tokio_stream::StreamExt;

use crate::ExitCode;
use crate::config::StatsFormat;
use crate::verbosity::Verbosity;

use super::CommandParams;

/// Dump all detected links to stdout without checking them
pub(crate) async fn dump<S>(params: CommandParams<S>) -> lychee_lib::Result<ExitCode>
where
    S: futures::Stream<Item = Result<Request, RequestError>>,
{
    let requests = params.requests;
    tokio::pin!(requests);

    if let Some(out_file) = &params.cfg.output {
        fs::File::create(out_file)?;
    }

    let verbosity = params.cfg.verbose();
    let json = params.cfg.format() == StatsFormat::Json;
    let mut writer = super::create_writer(params.cfg.output)?;

    while let Some(request) = requests.next().await {
        if let Err(e @ RequestError::UserInputContent { .. }) = request {
            return Err(e.into_error());
        }

        let mut request = match request {
            Ok(x) => x,
            Err(e) => {
                warn!("{e}");
                continue;
            }
        };

        // Apply URI remaps (if any)
        params
            .client
            .remap(&mut request.uri)?
            .inspect(|r| debug!("Remapping {r}"));

        let excluded = params.client.is_excluded(&request.uri);

        if excluded && verbosity.log_level() < log::Level::Info {
            continue;
        }

        if let Err(e) = write(&mut writer, &request, &verbosity, excluded, json) {
            // Avoid panic on broken pipe.
            // See https://github.com/rust-lang/rust/issues/46016
            // This can occur when piping the output of lychee
            // to another program like `grep`.
            if e.kind() != io::ErrorKind::BrokenPipe {
                error!("{e}");
                return Ok(ExitCode::UnexpectedFailure);
            }
        }
    }

    Ok(ExitCode::Success)
}

/// A dumped link, as printed with `--format json`
#[derive(Serialize)]
struct DumpedLink<'a> {
    url: &'a str,
    source: String,
    excluded: bool,
}

/// Dump request to stdout
fn write(
    writer: &mut Box<dyn Write>,
    request: &Request,
    verbosity: &Verbosity,
    excluded: bool,
    json: bool,
) -> io::Result<()> {
    // Only print `data:` URIs if verbose mode is at least `info`.
    if request.uri.is_data() && verbosity.log_level() < log::Level::Info {
        return Ok(());
    }

    let out_str = if json {
        // One JSON object per line, so output can be streamed and fed into tools like `jq`
        serde_json::to_string(&DumpedLink {
            url: request.uri.as_str(),
            source: request.source.to_string(),
            excluded,
        })?
    } else {
        // Only print source if verbose mode is at least `info`. This way the normal
        // link output can be fed into another tool without data mangling.
        let request = if verbosity.log_level() >= log::Level::Info {
            request.to_string()
        } else {
            request.uri.to_string()
        };

        // Mark excluded links
        if excluded {
            format!("{request} [excluded]")
        } else {
            request
        }
    };

    write_out(writer, &out_str)
}

fn write_out(writer: &mut Box<dyn Write>, out_str: &str) -> io::Result<()> {
    writeln!(writer, "{out_str}")
}
