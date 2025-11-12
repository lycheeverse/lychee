use lychee_lib::{FileExtensions, Input, Result};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use tokio_stream::StreamExt;

use crate::ExitCode;

/// Print all input sources to stdout, without extracting or checking links.
///
/// This command outputs the resolved input sources that would be processed
/// by lychee, including file paths, URLs, and special sources like stdin.
/// It respects file extension filtering and path exclusions.
pub(crate) async fn dump_inputs(
    inputs: HashSet<Input>,
    output: Option<&PathBuf>,
    excluded_paths: &[String],
    file_extensions: &FileExtensions,
    skip_hidden: bool,
    skip_ignored: bool,
) -> Result<ExitCode> {
    if let Some(out_file) = output {
        fs::File::create(out_file)?;
    }

    let mut writer = super::create_writer(output.cloned())?;

    // Create the path filter once outside the loop for better performance
    let excluded_path_filter = lychee_lib::filter::PathExcludes::new(excluded_paths)?;

    // Collect all sources with deduplication
    let mut seen_sources = HashSet::new();

    for input in inputs {
        let sources_stream = input.get_sources(
            file_extensions.clone(),
            skip_hidden,
            skip_ignored,
            &excluded_path_filter,
        );
        tokio::pin!(sources_stream);

        while let Some(source_result) = sources_stream.next().await {
            let source = source_result?;
            // Only print if we haven't seen this source before
            if seen_sources.insert(source.clone()) {
                write_out(&mut writer, &source)?;
            }
        }
    }

    Ok(ExitCode::Success)
}

fn write_out(writer: &mut Box<dyn Write>, out_str: &str) -> io::Result<()> {
    writeln!(writer, "{out_str}")
}
