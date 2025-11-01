use std::{path::PathBuf, process::Command};

use serde::Deserialize;

use super::{ErrorKind, Result};

/// Preprocess files with the specified command.
/// So instead of reading the file contents directly,
/// lychee will read the output of the preprocessor command.
/// The specified command is invoked with one argument, the path to the input file.
///
/// For example using `cat` is equivalent to not specifying any preprocessor command.
/// To invoke programs with custom arguments,
/// create a shell script to specify it as preprocessor command.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Preprocessor(String);

impl From<String> for Preprocessor {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl Preprocessor {
    /// Try to invoke the preprocessor command with `path` as single argument
    /// and return the resulting stdout.
    pub(crate) fn process(&self, path: &PathBuf) -> Result<String> {
        let pre = &self.0;
        let output = Command::new(pre).arg(path).output().map_err(|e| {
            ErrorKind::PreprocessorError(pre.clone(), format!("could not start: {e}"))
        })?;

        if output.status.success() {
            from_utf8(output.stdout)
        } else {
            let mut stderr = from_utf8(output.stderr)?;

            if stderr.is_empty() {
                stderr = "<empty stderr>".to_owned();
            }

            Err(ErrorKind::PreprocessorError(
                pre.clone(),
                format!("exited with non-zero code: {stderr}"),
            ))
        }
    }
}

fn from_utf8(data: Vec<u8>) -> Result<String> {
    String::from_utf8(data).map_err(|e| ErrorKind::Utf8(e.utf8_error()))
}
