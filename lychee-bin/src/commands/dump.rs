use log::error;
use lychee_lib::Request;
use lychee_lib::Result;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use tokio_stream::StreamExt;

use crate::ExitCode;
use crate::verbosity::Verbosity;

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

        let excluded = params.client.is_excluded(&request.uri);

        if excluded && params.cfg.verbose.log_level() < log::Level::Info {
            continue;
        }

        if let Err(e) = write(&mut writer, &request, &params.cfg.verbose, excluded) {
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

/// Dump all input sources to stdout without extracting any links and checking
/// them.
pub(crate) async fn dump_inputs<S>(
    sources: S,
    output: Option<&PathBuf>,
    excluded_paths: &[PathBuf],
) -> Result<ExitCode>
where
    S: futures::Stream<Item = Result<String>>,
{
    if let Some(out_file) = output {
        fs::File::create(out_file)?;
    }

    let mut writer = create_writer(output.cloned())?;

    tokio::pin!(sources);
    while let Some(source) = sources.next().await {
        let source = source?;

        let excluded = excluded_paths
            .iter()
            .any(|path| source.starts_with(path.to_string_lossy().as_ref()));
        if excluded {
            continue;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_dump_inputs_basic() -> Result<()> {
        // Create temp file for output
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        // Create test input stream
        let inputs = vec![
            Ok(String::from("test/path1")),
            Ok(String::from("test/path2")),
            Ok(String::from("test/path3")),
        ];
        let stream = stream::iter(inputs);

        // Run dump_inputs
        let result = dump_inputs(stream, Some(&output_path), &[]).await?;
        assert_eq!(result, ExitCode::Success);

        // Verify output
        let contents = fs::read_to_string(&output_path)?;
        assert_eq!(contents, "test/path1\ntest/path2\ntest/path3\n");
        Ok(())
    }

    #[tokio::test]
    async fn test_dump_inputs_with_excluded_paths() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let inputs = vec![
            Ok(String::from("test/path1")),
            Ok(String::from("excluded/path")),
            Ok(String::from("test/path2")),
        ];
        let stream = stream::iter(inputs);

        let excluded = vec![PathBuf::from("excluded")];
        let result = dump_inputs(stream, Some(&output_path), &excluded).await?;
        assert_eq!(result, ExitCode::Success);

        let contents = fs::read_to_string(&output_path)?;
        assert_eq!(contents, "test/path1\ntest/path2\n");
        Ok(())
    }

    #[tokio::test]
    async fn test_dump_inputs_empty_stream() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let stream = stream::iter::<Vec<Result<String>>>(vec![]);
        let result = dump_inputs(stream, Some(&output_path), &[]).await?;
        assert_eq!(result, ExitCode::Success);

        let contents = fs::read_to_string(&output_path)?;
        assert_eq!(contents, "");
        Ok(())
    }

    #[tokio::test]
    async fn test_dump_inputs_error_in_stream() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let inputs: Vec<Result<String>> = vec![
            Ok(String::from("test/path1")),
            Err(io::Error::other("test error").into()),
            Ok(String::from("test/path2")),
        ];
        let stream = stream::iter(inputs);

        let result = dump_inputs(stream, Some(&output_path), &[]).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_dump_inputs_to_stdout() -> Result<()> {
        // When output path is None, should write to stdout
        let inputs = vec![Ok(String::from("test/path1"))];
        let stream = stream::iter(inputs);

        let result = dump_inputs(stream, None, &[]).await?;
        assert_eq!(result, ExitCode::Success);
        Ok(())
    }
}
