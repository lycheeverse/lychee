use crate::{Base, ErrorKind, Result};
use path_clean::PathClean;
use std::env;
use std::path::{Path, PathBuf};

// Returns the base if it is a valid `PathBuf`
fn get_base_dir(base: &Option<Base>) -> Option<PathBuf> {
    base.as_ref().and_then(Base::dir)
}

// https://stackoverflow.com/a/54817755/270334
pub(crate) fn absolute_path(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();

    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    }
    .clean();

    Ok(absolute_path)
}

// Get the parent directory of a given `Path`.
fn dirname(src: &Path) -> PathBuf {
    if src.is_file() {
        src.to_path_buf()
            .parent()
            .map_or(PathBuf::new(), Path::to_path_buf)
    } else {
        src.to_path_buf()
    }
}

// Resolve `dst` that was linked to from within `src`
// Returns Ok(None) in case of an absolute local link without a `base_url`
pub(crate) fn resolve(src: &Path, dst: &Path, base: &Option<Base>) -> Result<Option<PathBuf>> {
    let resolved = match dst {
        relative if dst.is_relative() => {
            // Find `dst` in the parent directory of `src`
            let parent = match src.parent() {
                Some(parent) => parent,
                None => return Err(ErrorKind::FileNotFound(relative.to_path_buf())),
            };
            parent.join(relative.to_path_buf())
        }
        absolute if dst.is_absolute() => {
            // Absolute local links (leading slash) require the `base_url` to
            // define the document root. Silently ignore the link in case we
            // `base_url` is not defined.
            let base = match get_base_dir(base) {
                Some(path) => path,
                None => return Ok(None),
            };
            join(dirname(&base), absolute)
        }
        _ => return Err(ErrorKind::FileNotFound(dst.to_path_buf())),
    };
    Ok(Some(absolute_path(&resolved)?))
}

// A cumbersome way to concatenate paths without checking their
// existence on disk. See https://github.com/rust-lang/rust/issues/16507
fn join(base: PathBuf, dst: &Path) -> PathBuf {
    let mut abs = base.into_os_string();
    let target_str = dst.as_os_str();
    abs.push(target_str);
    PathBuf::from(abs)
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
            Some(env::current_dir()?.join("foo.html"))
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
            Some(env::current_dir()?.join("foo.html"))
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
}
