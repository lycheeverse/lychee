//! Checker Module
//!
//! This module contains all checkers, which are responsible for checking the status of a URL.

use std::path::{Path, PathBuf};

pub(crate) mod file;
pub(crate) mod mail;
pub(crate) mod website;
pub(crate) mod wikilink;

/// Returns the candidates to try for `path`, in the order they should be tried.
///
/// The path itself comes first, then every extension appended to the full file
/// name, then every extension replacing an existing one. Appended candidates
/// are exhausted first because they preserve the name as written.
///
/// Replacement is skipped when the existing extension contains ASCII
/// whitespace. Replacing it could discard part of the intended file name and
/// resolve to an unrelated file.
pub(crate) fn fallback_candidates<'a>(
    path: &'a Path,
    extensions: &'a [String],
) -> impl Iterator<Item = PathBuf> + 'a {
    let has_replaceable_extension = path.extension().is_some_and(|extension| {
        !extension
            .as_encoded_bytes()
            .iter()
            .any(u8::is_ascii_whitespace)
    });

    let original = std::iter::once(path.to_path_buf());

    // Build from the file name rather than the whole path so that trailing
    // slashes normalize like `set_extension`: `/a/b/` gives `/a/b.html`.
    let appended = extensions.iter().filter_map(move |extension| {
        path.file_name().map(|file_name| {
            let mut file_name = file_name.to_os_string();
            file_name.push(".");
            file_name.push(extension);
            path.with_file_name(file_name)
        })
    });

    let replaceable = if has_replaceable_extension {
        extensions
    } else {
        &[]
    };

    let replaced = replaceable.iter().map(move |extension| {
        let mut candidate = path.to_path_buf();
        candidate.set_extension(extension);
        candidate
    });

    original.chain(appended).chain(replaced)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates(path: &str, extensions: &[&str]) -> Vec<String> {
        let extensions: Vec<String> = extensions.iter().map(ToString::to_string).collect();
        fallback_candidates(Path::new(path), &extensions)
            .map(|candidate| candidate.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn appending_is_exhausted_before_replacing() {
        assert_eq!(
            candidates("a.tar", &["gz", "md"]),
            ["a.tar", "a.tar.gz", "a.tar.md", "a.gz", "a.md"]
        );
    }

    #[test]
    fn a_path_without_an_extension_has_nothing_to_replace() {
        assert_eq!(candidates("a", &["gz", "md"]), ["a", "a.gz", "a.md"]);
    }

    #[test]
    fn an_extension_with_whitespace_is_not_replaced() {
        // Replacing here would drop " hi" and could resolve to an unrelated `e.md`.
        assert_eq!(candidates("e. hi", &["md"]), ["e. hi", "e. hi.md"]);
    }
}
