use crate::{Base, ErrorKind, Result};
use std::path::{Component, Path, PathBuf};

// Returns the base if it is a valid `PathBuf`
fn get_base_dir(base: &Option<Base>) -> Option<PathBuf> {
    base.as_ref().and_then(|b| b.dir())
}

/// Normalize a path, removing things like `.` and `..`.
///
/// CAUTION: This does not resolve symlinks (unlike
/// [`std::fs::canonicalize`]). This may cause incorrect or surprising
/// behavior at times. This should be used carefully. Unfortunately,
/// [`std::fs::canonicalize`] can be hard to use correctly, since it can often
/// fail, or on Windows returns annoying device paths. This is a problem Cargo
/// needs to improve on.
///
/// Taken from https://github.com/rust-lang/cargo/blob/fede83ccf973457de319ba6fa0e36ead454d2e20/src/cargo/util/paths.rs#L61
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

pub(crate) fn resolve(src: &Path, dst: &Path, base: &Option<Base>) -> Result<PathBuf> {
    if dst.is_relative() {
        // Find `dst` in the parent directory of `src`
        if let Some(parent) = src.parent() {
            let rel_path = parent.join(dst.to_path_buf());
            return Ok(normalize(&rel_path));
        }
    }
    if dst.is_absolute() {
        // Absolute local links (leading slash) require the base_url to
        // define the document root.
        let base_dir = get_base_dir(base).unwrap_or(
            src.to_path_buf()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or(PathBuf::new()),
        );
        let abs_path = join(base_dir, dst);
        return Ok(normalize(&abs_path));
    }
    Err(ErrorKind::FileNotFound(dst.to_path_buf()))
}

// A cumbersome way to concatenate paths without checking their
// existence on disk. See https://github.com/rust-lang/rust/issues/16507
fn join(base: PathBuf, dst: &Path) -> PathBuf {
    let mut abs = base.into_os_string();
    let target_str = dst.as_os_str();
    abs.push(target_str);
    PathBuf::from(abs)
}

/// A little helper function to remove the get parameters from a URL link.
/// The link is not a URL but a String as that link may not have a base domain.
pub(crate) fn sanitize(link: String) -> String {
    let path = match link.split_once('?') {
        Some((path, _params)) => path,
        None => link.as_str(),
    };
    path.to_string()
}

#[cfg(test)]
mod test_fs_tree {
    use super::*;
    use crate::Result;

    #[test]
    fn test_sanitize() {
        assert_eq!(sanitize("/".to_string()), "/".to_string());
        assert_eq!(
            sanitize("index.html?foo=bar".to_string()),
            "index.html".to_string()
        );
        assert_eq!(
            sanitize("/index.html?foo=bar".to_string()),
            "/index.html".to_string()
        );
        assert_eq!(
            sanitize("/index.html?foo=bar&baz=zorx?bla=blub".to_string()),
            "/index.html".to_string()
        );
        assert_eq!(
            sanitize("https://example.org/index.html?foo=bar".to_string()),
            "https://example.org/index.html".to_string()
        );
        assert_eq!(
            sanitize("test.png?foo=bar".to_string()),
            "test.png".to_string()
        );
    }

    // dummy root
    // /path/to/foo.html
    #[test]
    fn test_resolve_absolute() -> Result<()> {
        let dummy = PathBuf::new();
        let abs_path = PathBuf::from("/absolute/path/to/foo.html");
        assert_eq!(resolve(&dummy, &abs_path, &None)?, abs_path);
        Ok(())
    }

    // index.html
    // ./foo.html
    #[test]
    fn test_resolve_relative() -> Result<()> {
        let dummy = PathBuf::from("index.html");
        let abs_path = PathBuf::from("./foo.html");
        assert_eq!(
            resolve(&dummy, &abs_path, &None)?,
            PathBuf::from("foo.html")
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
            PathBuf::from("foo.html")
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
            PathBuf::from("/path/to/foo.html")
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
            PathBuf::from("/some/absolute/base/dir/foo.html")
        );
        Ok(())
    }

    // /path/to/index.html
    // /other/path/to/foo.html
    #[test]
    fn test_resolve_absolute_from_absolute() -> Result<()> {
        let abs_index = PathBuf::from("/path/to/index.html");
        let abs_path = PathBuf::from("/other/path/to/foo.html");
        assert_eq!(
            resolve(&abs_index, &abs_path, &None)?,
            PathBuf::from("/path/to/other/path/to/foo.html")
        );
        Ok(())
    }
}
