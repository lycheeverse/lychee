use crate::{Base, ErrorKind, Result};
use std::path::{Path, PathBuf};

// Returns the base if it is a valid `PathBuf`
fn get_base_dir(base: &Option<Base>) -> Option<PathBuf> {
    base.as_ref().and_then(|b| b.dir())
}

pub(crate) fn resolve(src: &Path, dst: &Path, base: &Option<Base>) -> Result<PathBuf> {
    if dst.is_relative() {
        // Find `dst` in the parent directory of `src`
        if let Some(parent) = src.parent() {
            let rel_path = parent.join(dst.to_path_buf());
            return Ok(rel_path);
        }
    }
    if dst.is_absolute() {
        // Absolute local links (leading slash) require the base_url to
        // define the document root.
        if let Some(base_dir) = get_base_dir(base) {
            let abs_path = join(base_dir, dst);
            return Ok(abs_path);
        }
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
    use std::fs::File;

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
    fn test_find_absolute() -> Result<()> {
        let dummy = PathBuf::new();
        let dir = tempfile::tempdir()?;
        let dst = dir.path().join("foo.html");
        File::create(&dst)?;
        assert_eq!(find(&dummy, &dst, &None)?, dst);
        Ok(())
    }

    // index.html
    // ./foo.html
    #[test]
    fn test_find_relative() -> Result<()> {
        let root = PathBuf::from("index.html");
        let dir = tempfile::tempdir()?;
        let dst = dir.path().join("./foo.html");
        File::create(&dst)?;
        assert_eq!(find(&root, &dst, &None)?, dst);
        Ok(())
    }

    // ./index.html
    // ./foo.html
    #[test]
    fn test_find_relative_index() -> Result<()> {
        let root = PathBuf::from("./index.html");
        let dir = tempfile::tempdir()?;
        let dst = dir.path().join("./foo.html");
        File::create(&dst)?;
        assert_eq!(find(&root, &dst, &None)?, dst);
        Ok(())
    }

    #[test]
    fn test_find_relative_nonexistent() -> Result<()> {
        let root = PathBuf::from("index.html");
        // This file does not exist
        let dst = PathBuf::from("./foo.html");
        assert!(find(&root, &dst, &None).is_err());
        Ok(())
    }

    // /path/to/index.html
    // ./foo.html
    #[test]
    fn test_find_relative_from_absolute() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("index.html");
        // We create the absolute path to foo.html,
        // but we address it under its relative path
        let dst = PathBuf::from("./foo.html");
        let dst_absolute = dir.path().join("./foo.html");
        File::create(&dst_absolute)?;
        assert_eq!(find(&root, &dst, &None)?, dst_absolute);
        Ok(())
    }

    // dummy
    // ./foo.html
    // valid base dir
    #[test]
    fn test_find_absolute_from_base_dir() -> Result<()> {
        let dummy = PathBuf::new();
        let dir = tempfile::tempdir()?;
        let dst = dir.path().join("foo.html");
        File::create(&dst)?;
        let base_dir = dir.path().to_path_buf();
        let dst_absolute = base_dir.join(dst.to_path_buf());
        assert_eq!(
            find(&dummy, &dst, &Some(Base::Local(base_dir)))?,
            dst_absolute
        );
        Ok(())
    }

    // /path/to/index.html
    // ./foo.html (non-existent)
    #[test]
    fn test_find_relative_from_absolute_nonexistent() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("index.html");
        // We create the absolute path to foo.html,
        // but we address it under its relative path
        let dst = PathBuf::from("./foo.html");
        assert!(find(&root, &dst, &None).is_err());
        Ok(())
    }

    // /path/to/index.html
    // /other/path/to/foo.html
    #[test]
    fn test_find_absolute_from_absolute() -> Result<()> {
        let root = PathBuf::from("/path/to/index.html");
        let dir = tempfile::tempdir()?;
        let dst = dir.path().join("foo.html");
        File::create(&dst)?;
        assert_eq!(find(&root, &dst, &None)?, dst);
        Ok(())
    }

    // /path/to
    // /other/path/to/foo.html
    #[test]
    fn test_root_is_dir() -> Result<()> {
        let root = PathBuf::from("/path/to/");
        let dir = tempfile::tempdir()?;
        let dst = dir.path().join("foo.html");
        File::create(&dst)?;
        assert_eq!(find(&root, &dst, &None)?, dst);
        Ok(())
    }
}
