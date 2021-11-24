use std::io::{self, Write};

use lychee_lib::Request;

use crate::ExitCode;

/// Dump all detected links to stdout without checking them
pub(crate) fn dump<'a>(links: impl Iterator<Item = &'a Request>, verbose: bool) -> ExitCode {
    let mut stdout = io::stdout();
    for link in links {
        // Only print source in verbose mode. This way the normal link output
        // can be fed into another tool without data mangling.
        let output = if verbose {
            link.to_string()
        } else {
            link.uri.to_string()
        };

        // Avoid panic on broken pipe.
        // See https://github.com/rust-lang/rust/issues/46016
        // This can occur when piping the output of lychee
        // to another program like `grep`.
        if let Err(e) = writeln!(stdout, "{}", output) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("{}", e);
                return ExitCode::UnexpectedFailure;
            }
        }
    }
    ExitCode::Success
}
