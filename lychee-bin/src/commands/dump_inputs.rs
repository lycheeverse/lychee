use lychee_lib::{FileExtensions, Input, Result};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use tokio_stream::StreamExt;

use crate::ExitCode;

/// Dump all input sources to stdout without extracting any links and checking
/// them.
///
/// Uses the Input handler to properly handle different input sources and respect
/// file extension filtering.
pub(crate) async fn dump_inputs(
    inputs: Vec<Input>,
    output: Option<&PathBuf>,
    excluded_paths: &[String],
    valid_extensions: &FileExtensions,
    skip_hidden: bool,
    skip_gitignored: bool,
) -> Result<ExitCode> {
    if let Some(out_file) = output {
        fs::File::create(out_file)?;
    }

    let mut writer = super::create_writer(output.cloned())?;

    for input in inputs {
        let excluded_path_filter = lychee_lib::filter::PathExcludes::new(excluded_paths)?;
        let sources_stream = input.get_sources(
            valid_extensions.clone(),
            skip_hidden,
            skip_gitignored,
            excluded_path_filter,
        );
        tokio::pin!(sources_stream);

        while let Some(source_result) = sources_stream.next().await {
            let source = source_result?;
            write_out(&mut writer, &source)?;
        }
    }

    Ok(ExitCode::Success)
}

fn write_out(writer: &mut Box<dyn Write>, out_str: &str) -> io::Result<()> {
    writeln!(writer, "{out_str}")
}
