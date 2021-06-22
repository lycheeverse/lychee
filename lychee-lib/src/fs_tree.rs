use crate::{Base, ErrorKind, Result};
use std::path::{Path, PathBuf};

pub(crate) fn find(src: &Path, dst: &Path, base: &Option<Base>) -> Result<PathBuf> {
    if dst.exists() {
        return Ok(dst.to_path_buf());
    }
    if dst.is_dir() {
        return Err(ErrorKind::FileNotFound(dst.into()));
    }
    if dst.is_absolute() {
        // Absolute local links (leading slash) require the base_url to
        // define the document root.
        if let Some(base_dir) = base.as_ref().and_then(|b| b.dir()) {
            let absolute = base_dir.join(dst.to_path_buf());
            if absolute.exists() {
                return Ok(absolute);
            }
        }
    }
    if dst.is_relative() {
        // Find `dst` in the `root` path
        if let Some(parent) = src.parent() {
            let relative = parent.join(dst.to_path_buf());
            if relative.exists() {
                return Ok(relative);
            }
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

    // /path/to/index.html
    // /other/path/to
    #[test]
    fn test_dst_is_dir() -> Result<()> {
        let root = PathBuf::from("/path/to/");
        let dir = tempfile::tempdir()?;
        File::create(&dir)?;
        assert!(find(&root, &dir.into_path(), &None).is_err());
        Ok(())
    }
}
