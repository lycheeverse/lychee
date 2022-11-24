use crate::{helpers, ErrorKind, Result};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::{fs, path::PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// `PathExcludes` is used to check if a given input
/// path should be excluded from being checked.
/// Path matching is always relative to the `root` directory.
///
/// We check against `.gitignore` file inside `root` and against a list of
/// user-provided excludes. If the `root` is not a git repository, we
/// only check against the user-provided excludes.
#[derive(Debug, Clone)]
pub struct PathExcludes {
    root: PathBuf,
    excluded_paths: Vec<PathBuf>,
    gitignore: ignore::gitignore::Gitignore,
}

// We need to implement PartialEq manually because ignore::gitignore::Gitignore
// does not implement PartialEq. Instead we compare the gitignore path to check
// for equality.
impl PartialEq for PathExcludes {
    fn eq(&self, other: &Self) -> bool {
        self.excluded_paths == other.excluded_paths && self.root == other.root
    }
}

impl Eq for PathExcludes {}

impl Hash for PathExcludes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.excluded_paths.hash(state);
        self.root.hash(state);
    }
}

impl PathExcludes {
    /// Create a new instance of `PathExcludes`
    ///
    /// This is responsible for parsing the given `excludes` and creating a
    /// `Gitignore` instance. Based on this, we can then check if a given input
    /// path should be excluded.
    ///
    /// # Errors
    ///
    /// Returns an error if the `.gitignore` file cannot be parsed
    pub fn new(root: PathBuf, excludes: Vec<PathBuf>, load_gitignore: bool) -> Result<Self> {
        let excluded_paths = parse_excludes(excludes);
        let gitignore = if load_gitignore {
            // It is common that paths don't contain a `.gitignore` file,
            // in which case we just return an empty Gitignore instance.
            parse_gitignore(&root).unwrap_or_else(|_| Gitignore::empty())
        } else {
            Gitignore::empty()
        };

        Ok(Self {
            root,
            excluded_paths,
            gitignore,
        })
    }

    /// Check if a given input path should be excluded
    /// from being checked.
    ///
    /// This checks if the given path is in the list of `excludes`
    /// and also checks if the given path is ignored by the `.gitignore` file
    /// (if any).
    ///
    /// Returns `false` if the path cannot be canonicalized (i.e. does not
    /// exist)
    #[must_use]
    pub fn is_excluded_path(&self, path: &PathBuf) -> bool {
        let path = match fs::canonicalize(path) {
            Ok(path) => path,
            Err(_) => return false,
        };
        is_excluded(&self.excluded_paths, &path)
            || is_in_gitingore(&path, path.is_dir(), &self.gitignore)
    }
}

/// Canonicalize excluded paths for faster lookup
// Standalone function for easier testing
fn parse_excludes(excludes: Vec<PathBuf>) -> Vec<PathBuf> {
    excludes
        .into_iter()
        .filter_map(|path| match fs::canonicalize(path) {
            Ok(path) => Some(path),
            Err(_e) => None,
        })
        .collect()
}

/// Parse the `.gitignore` file
///
/// The path given should be the path at which the globs for this
/// gitignore file should be matched. Note that paths are always
/// matched relative to the root path. Generally, the
/// root path should correspond to the directory containing a
/// `.gitignore` file.
///
// Standalone function for easier testing
fn parse_gitignore(root: &PathBuf) -> Result<Gitignore> {
    println!("Loading gitignore file from {:?}", root);
    let mut builder = GitignoreBuilder::new(root);
    println!("Loading gitignore file from {:?}", root.join(".gitignore"));
    if let Some(error) = builder.add(root.join(".gitignore")) {
        println!("Error loading gitignore file: {:?}", error);
        return Err(ErrorKind::GitignoreError(error.to_string()));
    };

    builder
        .build()
        .map_err(|error| ErrorKind::GitignoreError(error.to_string()))
}

/// Check if a given path is in the list of excluded paths
// Standalone function to make testing easier
fn is_excluded(excluded_paths: &[PathBuf], path: &Path) -> bool {
    excluded_paths
        .iter()
        .any(|excluded| helpers::path::contains(excluded, path))
}

/// Check if a given path is ignored by the `.gitignore` file
// Standalone function to make testing easier
fn is_in_gitingore(path: &PathBuf, is_dir: bool, gitignore: &Gitignore) -> bool {
    let ign = gitignore.matched(path, is_dir).is_ignore();
    println!("{}: {}", path.display(), ign);
    ign
}

#[cfg(test)]
mod tests {
    use std::env;

    use ignore::gitignore::Gitignore;

    use crate::{
        types::path_excludes::{is_excluded, is_in_gitingore, parse_gitignore},
        PathExcludes,
    };

    impl PathExcludes {
        /// Create an empty instance of `PathExcludes` which never matches any path
        ///
        /// # Panics
        ///
        /// Panics if the current working directory cannot be determined.
        #[must_use]
        pub fn empty() -> Self {
            Self {
                // Get current working directory
                root: env::current_dir().unwrap(),
                excluded_paths: Vec::new(),
                gitignore: Gitignore::empty(),
            }
        }
    }

    #[test]
    fn test_no_exclusions() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_excluded(&[], dir.path()));
    }

    #[test]
    fn test_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        assert!(is_excluded(&[path.clone()], &path));
    }

    #[test]
    fn test_excluded_subdir() {
        let parent_dir = tempfile::tempdir().unwrap();
        let parent = parent_dir.path();
        let child_dir = tempfile::tempdir_in(parent).unwrap();
        let child = child_dir.path();
        assert!(is_excluded(&[parent.to_path_buf()], child));
    }

    #[test]
    fn test_path_excludes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo");
        std::fs::write(&path, "foo").unwrap();
        let excludes = vec![path.clone()];
        let path_excludes = PathExcludes::new(env::current_dir().unwrap(), excludes, true).unwrap();
        assert!(path_excludes.is_excluded_path(&path));
    }

    #[test]
    fn test_path_excludes_gitignore_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo");
        std::fs::write(&path, "foo").unwrap();
        let gitignore_path = dir.path().join(".gitignore");
        std::fs::write(&gitignore_path, "foo").unwrap();
        let excludes = vec![];
        let path_excludes = PathExcludes::new(env::current_dir().unwrap(), excludes, true).unwrap();
        assert!(path_excludes.is_excluded_path(&path));
    }

    #[test]
    fn test_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo");
        std::fs::write(&path, "foo").unwrap();
        let gitignore_path = dir.path().join(".gitignore");
        std::fs::write(&gitignore_path, "foo").unwrap();
        let gitignore = parse_gitignore(&dir.path().to_path_buf()).unwrap();
        assert!(is_in_gitingore(&path, path.is_dir(), &gitignore));
    }

    #[test]
    fn test_gitignore_subdir() {
        let parent_dir = tempfile::tempdir().unwrap();
        let parent = parent_dir.path();
        let child_dir = tempfile::tempdir_in(parent).unwrap();
        let child = child_dir.path();
        let gitignore_path = parent.join(".gitignore");
        std::fs::write(&gitignore_path, "foo").unwrap();
        let gitignore = parse_gitignore(&parent_dir.path().to_path_buf()).unwrap();
        assert!(is_in_gitingore(&child.join("foo"), false, &gitignore));
    }

    #[test]
    fn test_gitignore_subdir_with_parent() {
        let parent_dir = tempfile::tempdir().unwrap();
        let parent = parent_dir.path();
        let child_dir = tempfile::tempdir_in(parent).unwrap();
        let child = child_dir.path();
        let gitignore_path = parent.join(".gitignore");
        std::fs::write(&gitignore_path, "foo").unwrap();
        let gitignore = parse_gitignore(&parent_dir.path().to_path_buf()).unwrap();
        assert!(is_in_gitingore(&child.join("foo"), false, &gitignore));
    }

    #[test]
    fn test_gitignore_not_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo");
        std::fs::write(&path, "foo").unwrap();
        let gitignore_path = dir.path().join(".gitignore");
        std::fs::write(&gitignore_path, "bar").unwrap();
        let gitignore = parse_gitignore(&dir.path().to_path_buf()).unwrap();
        assert!(!is_in_gitingore(&path, path.is_dir(), &gitignore));
    }
}
