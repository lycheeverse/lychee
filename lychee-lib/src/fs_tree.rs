use crate::{ErrorKind, Result};
use std::path::{Path, PathBuf};

pub(crate) fn find(root: &Path, dst: &Path) -> Result<PathBuf> {
    if dst.exists() {
        return Ok(dst.to_path_buf());
    }
    if dst.is_dir() {
        return Err(ErrorKind::FileNotFound(dst.into()));
    }
    // Find `dst` in the `root` path
    if let Some(parent) = root.parent() {
        let rel = parent.join(dst.to_path_buf());
        if rel.exists() {
            return Ok(rel);
        }
    }
    Err(ErrorKind::FileNotFound(dst.to_path_buf()))
}

#[cfg(test)]
mod test_fs_tree {
    use std::fs::File;

    use super::*;
    use crate::Result;

    // dummy root
    // /path/to/foo.html
    #[test]
    fn test_find_absolute() -> Result<()> {
        let dummy = PathBuf::new();
        let dir = tempfile::tempdir()?;
        let dst = dir.path().join("foo.html");
        File::create(&dst)?;
        assert_eq!(find(&dummy, &dst)?, dst);
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
        assert_eq!(find(&root, &dst)?, dst);
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
        assert_eq!(find(&root, &dst)?, dst);
        Ok(())
    }

    #[test]
    fn test_find_relative_nonexistent() -> Result<()> {
        let root = PathBuf::from("index.html");
        // This file does not exist
        let dst = PathBuf::from("./foo.html");
        assert!(find(&root, &dst).is_err());
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
        assert_eq!(find(&root, &dst)?, dst_absolute);
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
        assert!(find(&root, &dst).is_err());
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
        assert_eq!(find(&root, &dst)?, dst);
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
        assert_eq!(find(&root, &dst)?, dst);
        Ok(())
    }

    // /path/to/index.html
    // /other/path/to
    #[test]
    fn test_dst_is_dir() -> Result<()> {
        let root = PathBuf::from("/path/to/");
        let dir = tempfile::tempdir()?;
        File::create(&dir)?;
        assert!(find(&root, &dir.into_path()).is_err());
        Ok(())
    }
}
