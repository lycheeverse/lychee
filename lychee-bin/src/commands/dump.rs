use std::io::{self, Write};

use lychee_lib::Request;

use crate::ExitCode;

/// Dump all detected links to stdout without checking them
pub(crate) fn dump<'a>(links: impl Iterator<Item = &'a Request>) -> ExitCode {
    let mut stdout = io::stdout();
    for link in links {
        // Avoid panic on broken pipe.
        // See https://github.com/rust-lang/rust/issues/46016
        // This can occur when piping the output of lychee
        // to another program like `grep`.
        if let Err(e) = writeln!(stdout, "{}", &link) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("{}", e);
                return ExitCode::UnexpectedFailure;
            }
        }
    }
    ExitCode::Success
}
