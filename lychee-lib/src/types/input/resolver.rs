//! Input source resolution.
//!
//! Provides the `InputResolver` which handles resolution of various input sources
//! into concrete, processable sources by expanding glob patterns and applying filters.

use super::input::Input;
use super::source::{InputSource, ResolvedInputSource};
use crate::Result;
use crate::filter::PathExcludes;
use crate::types::file::FileExtensions;
use async_stream::try_stream;
use futures::stream::Stream;
use futures::stream::once;
use glob::glob_with;
use ignore::{Walk, WalkBuilder};
use shellexpand::tilde;
use std::path::Path;
use std::pin::Pin;

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
    /// Returns an error (within the stream) if:
    /// - The glob pattern is invalid or expansion encounters I/O errors
    /// - Directory traversal fails, including:
    ///   - Permission denied when accessing directories or files
    ///   - I/O errors while reading directory contents
    ///   - Filesystem errors (disk errors, network filesystem issues, etc.)
    ///   - Invalid file paths or symbolic link resolution failures
    /// - Errors when reading or evaluating `.gitignore` or `.ignore` files
    /// - Errors occur during file extension or path exclusion evaluation
    ///
    /// Once an error is returned, resolution of that input source halts
    /// and no further `Ok(ResolvedInputSource)` will be produced.
    #[must_use]
    pub fn resolve<'a>(
        input: &Input,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_ignored: bool,
        excluded_paths: &'a PathExcludes,
    ) -> Pin<Box<dyn Stream<Item = Result<ResolvedInputSource>> + Send + 'a>> {
        Self::resolve_input(
            input,
            file_extensions,
            skip_hidden,
            skip_ignored,
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
        skip_ignored: bool,
    ) -> Result<Walk> {
        Ok(WalkBuilder::new(path)
            // Skip over files which are ignored by git or `.ignore` if necessary
            .git_ignore(skip_ignored)
            .git_global(skip_ignored)
            .git_exclude(skip_ignored)
            .ignore(skip_ignored)
            .parents(skip_ignored)
            // Ignore hidden files if necessary
            .hidden(skip_hidden)
            // Configure the file types filter to only include files with matching extensions
            .types(file_extensions.build(skip_hidden)?)
            .build())
    }

    /// Internal method for resolving input sources.
    ///
    /// Takes an Input and returns a stream of `ResolvedInputSource` items,
    /// expanding glob patterns and applying filtering based on the provided
    /// configuration.
    fn resolve_input<'a>(
        input: &Input,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_ignored: bool,
        excluded_paths: &'a PathExcludes,
    ) -> Pin<Box<dyn Stream<Item = Result<ResolvedInputSource>> + Send + 'a>> {
        match &input.source {
            InputSource::RemoteUrl(url) => {
                let url = url.clone();
                Box::pin(once(async move { Ok(ResolvedInputSource::RemoteUrl(url)) }))
            }
            InputSource::FsGlob {
                pattern,
                ignore_case,
            } => {
                // NOTE: we convert the glob::Pattern back to str because
                // `glob_with` only takes string arguments.
                let glob_expanded = tilde(pattern.as_str()).to_string();
                let mut match_opts = glob::MatchOptions::new();
                match_opts.case_sensitive = !ignore_case;

                Box::pin(try_stream! {
                    // For glob patterns, we expand the pattern and yield
                    // matching paths as ResolvedInputSource::FsPath items.
                    for entry in glob_with(&glob_expanded, match_opts)? {
                        match entry {
                            Ok(path) => {
                                // Skip directories or files that don't match
                                // extensions
                                if path.is_dir() {
                                    continue;
                                }
                                if Self::is_excluded_path(&path, excluded_paths) {
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
                })
            }
            InputSource::FsPath(path) => {
                if path.is_dir() {
                    let walk = match Self::walk(path, file_extensions, skip_hidden, skip_ignored) {
                        Ok(x) => x,
                        Err(e) => {
                            return Box::pin(once(async move { Err(e) }));
                        }
                    };

                    Box::pin(try_stream! {
                        for entry in walk {
                            let entry = entry?;
                            if Self::is_excluded_path(entry.path(), excluded_paths)
                            {
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

                            yield ResolvedInputSource::FsPath(
                                entry.path().to_path_buf()
                            );
                        }
                    })
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
                    if Self::is_excluded_path(path, excluded_paths) {
                        Box::pin(futures::stream::empty())
                    } else {
                        let path = path.clone();
                        Box::pin(once(async move { Ok(ResolvedInputSource::FsPath(path)) }))
                    }
                }
            }
            InputSource::Stdin => Box::pin(once(async move { Ok(ResolvedInputSource::Stdin) })),
            InputSource::String(s) => {
                let s = s.clone();
                Box::pin(once(async move { Ok(ResolvedInputSource::String(s)) }))
            }
        }
    }

    /// Check if the given path was excluded from link checking
    fn is_excluded_path(path: &Path, excluded_paths: &PathExcludes) -> bool {
        excluded_paths.is_match(&path.to_string_lossy())
    }
}
