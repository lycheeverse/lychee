//! Input source resolution.
//!
//! Provides the `InputResolver` which handles resolution of various input sources
//! into concrete, processable sources by expanding glob patterns and applying filters.

use std::path::Path;

use super::input::Input;
use super::source::{InputSource, ResolvedInputSource};
use crate::Result;
use crate::filter::PathExcludes;
use crate::types::file::FileExtensions;
use async_stream::try_stream;
use futures::stream::Stream;
use glob::glob_with;
use ignore::{Walk, WalkBuilder};
use shellexpand::tilde;

/// Resolves input sources into concrete, processable sources.
///
/// Handles expansion of glob patterns and filtering based on exclusion rules.
#[derive(Copy, Clone, Debug)]
pub struct InputResolver;

impl InputResolver {
    /// Resolve an input into a stream of concrete input sources.
    ///
    /// This returns a stream of resolved input sources for the given input,
    /// taking into account the matching file extensions and respecting
    /// exclusions. Glob patterns are expanded into individual file paths.
    ///
    /// # Returns
    ///
    /// Returns a stream of `Result<ResolvedInputSource>` for all matching input
    /// sources. Glob patterns are expanded, so `FsGlob` never appears in the
    /// output.
    ///
    /// # Errors
    ///
    /// Will return errors for file system operations or glob pattern issues
    pub fn resolve<'a>(
        input: &'a Input,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_gitignored: bool,
        excluded_paths: &'a PathExcludes,
    ) -> impl Stream<Item = Result<ResolvedInputSource>> + 'a {
        Self::resolve_input(
            input,
            file_extensions,
            skip_hidden,
            skip_gitignored,
            excluded_paths,
        )
    }

    /// Create a [`Walk`] iterator for directory traversal
    ///
    /// # Errors
    ///
    /// Fails if [`FileExtensions`] cannot be converted
    pub(crate) fn walk(
        path: &Path,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_gitignored: bool,
    ) -> Result<Walk> {
        Ok(WalkBuilder::new(path)
            .git_ignore(skip_gitignored)
            .git_global(skip_gitignored)
            .git_exclude(skip_gitignored)
            .ignore(skip_gitignored)
            .parents(skip_gitignored)
            .hidden(skip_hidden)
            // Configure the file types filter to only include files with matching extensions
            .types(file_extensions.try_into()?)
            .build())
    }

    /// Internal method for resolving input sources.
    ///
    /// Takes an Input and returns a stream of `ResolvedInputSource` items,
    /// expanding glob patterns and applying filtering based on the provided
    /// configuration.
    fn resolve_input<'a>(
        input: &'a Input,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_gitignored: bool,
        excluded_paths: &'a PathExcludes,
    ) -> impl Stream<Item = Result<ResolvedInputSource>> + 'a {
        try_stream! {
            match &input.source {
                InputSource::RemoteUrl(url) => {
                    yield ResolvedInputSource::RemoteUrl(url.clone());
                },
                InputSource::FsGlob { pattern, ignore_case } => {
                    // For glob patterns, we expand the pattern and yield
                    // matching paths as ResolvedInputSource::FsPath items.
                    let glob_expanded = tilde(pattern).to_string();
                    let mut match_opts = glob::MatchOptions::new();
                    match_opts.case_sensitive = !ignore_case;

                    for entry in glob_with(&glob_expanded, match_opts)? {
                        match entry {
                            Ok(path) => {
                                // Skip directories or files that don't match
                                // extensions
                                if path.is_dir() {
                                    continue;
                                }
                                if excluded_paths.is_match(&path.to_string_lossy()) {
                                    continue;
                                }

                                // We do not filter by extensions here.
                                //
                                // Instead, we always check files captured by
                                // the glob pattern, as the user explicitly
                                // specified them.
                                yield ResolvedInputSource::FsPath(path);
                            }
                            Err(e) => {
                                eprintln!("Error in glob pattern: {e:?}");
                            }
                        }
                    }
                },
                InputSource::FsPath(path) => {
                    if path.is_dir() {
                        for entry in Self::walk(path, file_extensions, skip_hidden, skip_gitignored)? {
                            let entry = entry?;
                            if excluded_paths.is_match(&entry.path().to_string_lossy()) {
                                continue;
                            }

                            match entry.file_type() {
                                None => continue,
                                Some(file_type) => {
                                    if !file_type.is_file() {
                                        continue;
                                    }
                                }
                            }

                            yield ResolvedInputSource::FsPath(entry.path().to_path_buf());
                        }
                    } else {
                        // For individual files, yield if not excluded.
                        //
                        // We do not filter by extension here, as individual
                        // files should always be checked, no matter if their
                        // extension matches or not.
                        //
                        // This follows the principle of least surprise because
                        // the user explicitly specified the file, so they
                        // expect it to be checked.
                        if !excluded_paths.is_match(&path.to_string_lossy()) {
                            yield ResolvedInputSource::FsPath(path.clone());
                        }
                    }
                },
                InputSource::Stdin => {
                    yield ResolvedInputSource::Stdin;
                },
                InputSource::String(s) => {
                    yield ResolvedInputSource::String(s.clone());
                }
            }
        }
    }
}
