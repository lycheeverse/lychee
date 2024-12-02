use crate::{ErrorKind, Result};
use cached::proc_macro::cached;
use once_cell::sync::Lazy;
use path_clean::PathClean;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

static CURRENT_DIR: Lazy<PathBuf> =
    Lazy::new(|| env::current_dir().expect("cannot get current dir from environment"));

/// Create an absolute path out of a `PathBuf`.
///
/// The `clean` method is relatively expensive
/// Therefore we cache this call to reduce allocs and wall time
/// https://stackoverflow.com/a/54817755/270334
#[cached]
pub(crate) fn absolute_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        CURRENT_DIR.join(path)
    }
    .clean()
}

/// Resolve `dst` that was linked to from within `src`
///
/// Returns Ok(None) in case of an absolute local link without a `base_url`
pub(crate) fn resolve(
    src: &Path,
    dst: &PathBuf,
    ignore_absolute_local_links: bool,
) -> Result<Option<PathBuf>> {
    let resolved = match dst {
        relative if dst.is_relative() => {
            // Find `dst` in the parent directory of `src`
            let Some(parent) = src.parent() else {
                return Err(ErrorKind::InvalidFile(relative.to_path_buf()));
            };
            parent.join(relative)
        }
        absolute if dst.is_absolute() => {
            if ignore_absolute_local_links {
                return Ok(None);
            }
            PathBuf::from(absolute)
        }
        _ => return Err(ErrorKind::InvalidFile(dst.to_path_buf())),
    };
    Ok(Some(absolute_path(resolved)))
}

/// Check if `child` is a subdirectory/file inside `parent`
///
/// Note that `contains(parent, parent)` will return `true`
///
/// See <https://stackoverflow.com/questions/30511331>
/// See <https://stackoverflow.com/questions/62939265>
///
/// # Errors
///
/// Returns an error if the `path` does not exist
/// or a non-final component in path is not a directory.
//
// Unfortunately requires real files for `fs::canonicalize`.
pub(crate) fn contains(parent: &PathBuf, child: &PathBuf) -> Result<bool> {
    let parent = fs::canonicalize(parent)?;
    let child = fs::canonicalize(child)?;

    Ok(child.starts_with(parent))
}

#[cfg(test)]
mod test_path {
    use super::*;
    use crate::Result;

    // index.html
    // ./foo.html
    #[test]
    fn test_resolve_relative() -> Result<()> {
        let dummy = PathBuf::from("index.html");
        let abs_path = PathBuf::from("./foo.html");
        assert_eq!(
            resolve(&dummy, &abs_path, true)?,
            Some(env::current_dir().unwrap().join("foo.html"))
        );
        Ok(())
    }

    // ./index.html
    // ./foo.html
    #[test]
    fn test_resolve_relative_index() -> Result<()> {
        let dummy = PathBuf::from("./index.html");
        let abs_path = PathBuf::from("./foo.html");
        assert_eq!(
            resolve(&dummy, &abs_path, true)?,
            Some(env::current_dir().unwrap().join("foo.html"))
        );
        Ok(())
    }

    // /path/to/index.html
    // ./foo.html
    #[test]
    fn test_resolve_from_absolute() -> Result<()> {
        let abs_index = PathBuf::from("/path/to/index.html");
        let abs_path = PathBuf::from("./foo.html");
        assert_eq!(
            resolve(&abs_index, &abs_path, true)?,
            Some(PathBuf::from("/path/to/foo.html"))
        );
        Ok(())
    }

    #[test]
    fn test_contains() {
        let parent_dir = tempfile::tempdir().unwrap();
        let parent = parent_dir.path();
        let child_dir = tempfile::tempdir_in(parent).unwrap();
        let child = child_dir.path();

        assert_eq!(contains(&parent.to_owned(), &child.to_owned()), Ok(true));
    }

    #[test]
    fn test_contains_not() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();

        assert_eq!(
            contains(&dir1.path().to_owned(), &dir2.path().to_owned()),
            Ok(false)
        );
    }

    #[test]
    fn test_contains_one_dir_does_not_exist() {
        let dir1 = tempfile::tempdir().unwrap();

        assert!(matches!(
            contains(&dir1.path().to_owned(), &PathBuf::from("/does/not/exist")),
            Err(crate::ErrorKind::ReadStdinInput(_))
        ));
    }

    // Relative paths are supported, e.g.
    // parent: `/path/to/parent`
    // child:  `/path/to/parent/child/..`
    #[test]
    fn test_contains_one_dir_relative_path() {
        let parent_dir = tempfile::tempdir().unwrap();
        let parent = parent_dir.path();
        let child_dir = tempfile::tempdir_in(parent).unwrap();
        let child = child_dir.path().join("..");

        assert_eq!(contains(&parent.to_owned(), &child), Ok(true));
    }
}
