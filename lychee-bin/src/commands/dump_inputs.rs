use lychee_lib::{FileExtensions, Input, Result};
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
    
    // Create the path filter once outside the loop for better performance
    let excluded_path_filter = lychee_lib::filter::PathExcludes::new(excluded_paths)?;

    for input in inputs {
        let sources_stream = input.get_sources(
            valid_extensions.clone(),
            skip_hidden,
            skip_gitignored,
            excluded_path_filter.clone(),
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
