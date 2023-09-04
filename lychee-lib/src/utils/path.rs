use crate::{Base, ErrorKind, Result};
use cached::proc_macro::cached;
use once_cell::sync::Lazy;
use path_clean::PathClean;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

static CURRENT_DIR: Lazy<PathBuf> =
    Lazy::new(|| env::current_dir().expect("cannot get current dir from environment"));

/// Returns the base if it is a valid `PathBuf`
fn get_base_dir(base: &Option<Base>) -> Option<PathBuf> {
    base.as_ref().and_then(Base::dir)
}

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

/// Get the directory name of a given `Path`.
fn dirname(src: &'_ Path) -> Option<&'_ Path> {
    if src.is_file() {
        return src.parent();
    }
    Some(src)
}

/// Resolve `dst` that was linked to from within `src`
///
/// Returns Ok(None) in case of an absolute local link without a `base_url`
pub(crate) fn resolve(src: &Path, dst: &Path, base: &Option<Base>) -> Result<Option<PathBuf>> {
    let resolved = match dst {
        relative if dst.is_relative() => {
            // Find `dst` in the parent directory of `src`
            let Some(parent) = src.parent() else {
                return Err(ErrorKind::InvalidFile(relative.to_path_buf()));
            };
            parent.join(relative)
        }
        absolute if dst.is_absolute() => {
            // Absolute local links (leading slash) require the `base_url` to
            // define the document root. Silently ignore the link in case the
            // `base_url` is not defined.
            let Some(base) = get_base_dir(base) else {
                return Ok(None);
            };
            let Some(dir) = dirname(&base) else {
                return Err(ErrorKind::InvalidBase(
                    base.display().to_string(),
                    "The given directory cannot be a base".to_string(),
                ));
            };
            join(dir.to_path_buf(), absolute)
        }
        _ => return Err(ErrorKind::InvalidFile(dst.to_path_buf())),
    };
    Ok(Some(absolute_path(resolved)))
}

/// A cumbersome way to concatenate paths without checking their
/// existence on disk. See <https://github.com/rust-lang/rust/issues/16507>
fn join(base: PathBuf, dst: &Path) -> PathBuf {
    let mut abs = base.into_os_string();
    let target_str = dst.as_os_str();
    abs.push(target_str);
    PathBuf::from(abs)
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
            resolve(&dummy, &abs_path, &None)?,
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
            resolve(&dummy, &abs_path, &None)?,
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
            resolve(&abs_index, &abs_path, &None)?,
            Some(PathBuf::from("/path/to/foo.html"))
        );
        Ok(())
    }

    // dummy
    // foo.html
    // valid base dir
    #[test]
    fn test_resolve_absolute_from_base_dir() -> Result<()> {
        let dummy = PathBuf::new();
        let abs_path = PathBuf::from("/foo.html");
        let base = Some(Base::Local(PathBuf::from("/some/absolute/base/dir")));
        assert_eq!(
            resolve(&dummy, &abs_path, &base)?,
            Some(PathBuf::from("/some/absolute/base/dir/foo.html"))
        );
        Ok(())
    }

    // /path/to/index.html
    // /other/path/to/foo.html
    #[test]
    fn test_resolve_absolute_from_absolute() -> Result<()> {
        let abs_index = PathBuf::from("/path/to/index.html");
        let abs_path = PathBuf::from("/other/path/to/foo.html");
        let base = Some(Base::Local(PathBuf::from("/some/absolute/base/dir")));
        assert_eq!(
            resolve(&abs_index, &abs_path, &base)?,
            Some(PathBuf::from(
                "/some/absolute/base/dir/other/path/to/foo.html"
            ))
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
