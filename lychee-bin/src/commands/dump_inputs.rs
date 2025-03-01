use lychee_lib::FileExtensions;
use lychee_lib::Result;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use tokio_stream::StreamExt;

use crate::ExitCode;

/// Dump all input sources to stdout without extracting any links and checking
/// them.
///
/// In the case of a file path like `.`, the path is expanded to all files
/// in that directory, which match the file extensions specified in the
/// configuration.
pub(crate) async fn dump_inputs<S>(
    sources: S,
    output: Option<&PathBuf>,
    excluded_paths: &[PathBuf],
    valid_extensions: &FileExtensions,
) -> Result<ExitCode>
where
    S: futures::Stream<Item = Result<String>>,
{
    if let Some(out_file) = output {
        fs::File::create(out_file)?;
    }

    let mut writer = super::create_writer(output.cloned())?;

    tokio::pin!(sources);
    while let Some(source) = sources.next().await {
        let source = source?;

        let excluded = excluded_paths
            .iter()
            .any(|path| source.starts_with(path.to_string_lossy().as_ref()));
        if excluded {
            continue;
        }

        // In case of a file path like `.`, expand it to all files in that
        // directory, which match the file extensions specified in the
        // configuration.
        // TODO: This needs to change and use the Inputs method.
        // Then we get proper file extensions and filter handling!
        if let Ok(entries) = fs::read_dir(&source) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let path = path.to_string_lossy();

                    // Check if the file matches extensions specified in the
                    // configuration or the default extensions.
                    if !path
                        .split('.')
                        .last()
                        .map(|ext| {
                            let ext = ext.to_lowercase();
                            valid_extensions.contains(&ext)
                        })
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    write_out(&mut writer, &path)?;
                }
            }
            continue;
        }

        writeln!(writer, "{source}")?;
    }

    Ok(ExitCode::Success)
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
        let result =
            dump_inputs(stream, Some(&output_path), &[], &FileExtensions::default()).await?;
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
        let result = dump_inputs(
            stream,
            Some(&output_path),
            &excluded,
            &FileExtensions::default(),
        )
        .await?;
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
        let result =
            dump_inputs(stream, Some(&output_path), &[], &FileExtensions::default()).await?;
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
            Err(io::Error::new(io::ErrorKind::Other, "test error").into()),
            Ok(String::from("test/path2")),
        ];
        let stream = stream::iter(inputs);

        let result = dump_inputs(stream, Some(&output_path), &[], &FileExtensions::default()).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_dump_inputs_to_stdout() -> Result<()> {
        // When output path is None, should write to stdout
        let inputs = vec![Ok(String::from("test/path1"))];
        let stream = stream::iter(inputs);

        let result = dump_inputs(stream, None, &[], &FileExtensions::default()).await?;
        assert_eq!(result, ExitCode::Success);
        Ok(())
    }
}
