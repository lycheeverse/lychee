use lychee_lib::Result;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

// Helper function to create an output writer.
//
// If the output file is not specified, it will use `stdout`.
//
// # Errors
//
// If the output file cannot be opened, an error is returned.
pub(crate) fn create_writer(output: Option<PathBuf>) -> Result<Box<dyn Write>> {
    let out = if let Some(output) = output {
        let out = fs::OpenOptions::new().append(true).open(output)?;
        Box::new(out) as Box<dyn Write>
    } else {
        let out = io::stdout();
        Box::new(out.lock()) as Box<dyn Write>
    };
    Ok(out)
}
