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
use ignore::WalkBuilder;
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
    /// Will return errors for file system operations or glob pattern issues
    #[must_use]
    pub fn resolve<'a>(
        input: &'_ Input,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_gitignored: bool,
        excluded_paths: &'a PathExcludes,
    ) -> Pin<Box<dyn Stream<Item = Result<ResolvedInputSource>> + Send + 'a>> {
        Self::resolve_input(
            input,
            file_extensions,
            skip_hidden,
            skip_gitignored,
            excluded_paths,
        )
    }

    /// Create a `WalkBuilder` for directory traversal
    fn walk_entries(
        path: &Path,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_gitignored: bool,
    ) -> Result<ignore::Walk> {
        Ok(WalkBuilder::new(path)
            // Enable standard filters if `skip_gitignored `is true.
            // This will skip files ignored by `.gitignore` and other VCS ignore files.
            .standard_filters(skip_gitignored)
            // Override hidden file behavior to be controlled by the separate skip_hidden parameter
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
        input: &'_ Input,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_gitignored: bool,
        excluded_paths: &'a PathExcludes,
    ) -> Pin<Box<dyn Stream<Item = Result<ResolvedInputSource>> + Send + 'a>> {
        match &input.source {
            InputSource::RemoteUrl(url) => {
                let url = url.clone();
                Box::pin(try_stream! {
                    yield ResolvedInputSource::RemoteUrl(url);
                })
            }
            InputSource::FsGlob {
                pattern,
                ignore_case,
            } => {
                let glob_expanded = tilde(pattern).to_string();
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
                })
            }
            InputSource::FsPath(path) => {
                if path.is_dir() {
                    let walk = match Self::walk_entries(
                        path,
                        file_extensions,
                        skip_hidden,
                        skip_gitignored,
                    ) {
                        Ok(x) => x,
                        Err(e) => {
                            return Box::pin(once(async move { Err(e) }));
                        }
                    };

                    Box::pin(try_stream! {
                        for entry in walk {
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
                    if excluded_paths.is_match(&path.to_string_lossy()) {
                        Box::pin(futures::stream::empty())
                    } else {
                        let path = path.clone();
                        Box::pin(try_stream! {
                            yield ResolvedInputSource::FsPath(path);
                        })
                    }
                }
            }
            InputSource::Stdin => Box::pin(try_stream! {
                yield ResolvedInputSource::Stdin;
            }),
            InputSource::String(s) => {
                let s = s.clone();
                Box::pin(try_stream! {
                    yield ResolvedInputSource::String(s);
                })
            }
        }
    }
}
