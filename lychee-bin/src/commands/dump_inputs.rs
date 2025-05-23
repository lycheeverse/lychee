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
    excluded_paths: &[PathBuf],
    valid_extensions: &FileExtensions,
    skip_hidden: bool,
    skip_gitignored: bool,
) -> Result<ExitCode> {
    if let Some(out_file) = output {
        fs::File::create(out_file)?;
    }

    let mut writer = super::create_writer(output.cloned())?;

    for input in inputs {
        let paths_stream =
            input.get_file_paths(valid_extensions.clone(), skip_hidden, skip_gitignored);
        tokio::pin!(paths_stream);

        while let Some(path_result) = paths_stream.next().await {
            let path = path_result?;

            // Skip excluded paths
            if excluded_paths
                .iter()
                .any(|excluded| path.starts_with(excluded))
            {
                continue;
            }

            write_out(&mut writer, &path.to_string_lossy())?;
        }
    }

    Ok(ExitCode::Success)
}

fn write_out(writer: &mut Box<dyn Write>, out_str: &str) -> io::Result<()> {
    writeln!(writer, "{out_str}")
}
