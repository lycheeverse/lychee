use crate::{ErrorKind, Result};
use cached::proc_macro::cached;
use path_clean::PathClean;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static CURRENT_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| env::current_dir().expect("cannot get current dir from environment"));

/// Create an absolute path out of a `PathBuf`.
///
/// The `clean` method is relatively expensive
/// Therefore we cache this call to reduce allocs and wall time
/// <https://stackoverflow.com/a/54817755/270334>
#[cached]
pub(crate) fn absolute_path(path: PathBuf) -> PathBuf {
    let absolute = if path.is_absolute() {
        path
    } else {
        CURRENT_DIR.join(path)
    };
    absolute.clean()
}

/// Resolve `dst` that was linked to from within `src`
///
/// Returns Ok(None) in case of an absolute local link without a `base_url`
pub(crate) fn resolve(
    src: &Path,
    dst: &Path,
    ignore_absolute_local_links: bool,
) -> Result<Option<PathBuf>> {
    let resolved = match dst {
        relative if dst.is_relative() => {
            // Find `dst` in the parent directory of `src`
            let Some(parent) = src.parent() else {
                return Err(ErrorKind::InvalidFile(relative.to_owned()));
            };
            parent.join(relative)
        }
        absolute if dst.is_absolute() => {
            if ignore_absolute_local_links {
                return Ok(None);
            }
            PathBuf::from(absolute)
        }
        _ => return Err(ErrorKind::InvalidFile(dst.to_owned())),
    };
    Ok(Some(absolute_path(resolved)))
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
}
