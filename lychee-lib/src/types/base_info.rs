//! Parses and resolves [`RawUri`] into into fully-qualified [`Uri`] by
//! applying base URL and root dir mappings.

use reqwest::Url;
use serde::Deserialize;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use url::ParseError;

use crate::ErrorKind;
use crate::Uri;
use crate::utils;
use crate::utils::url::is_root_relative_link;

/// Information used for resolving relative URLs within a particular
/// input source. There should be a 1:1 correspondence between each
/// `BaseInfo` and its originating `InputSource`. The main entry
/// point for constructing is [`BaseInfo::from_source_url`].
///
/// Once constructed, [`BaseInfo::parse_url_text`] can be used to
/// parse and resolve a (possibly relative) URL obtained from within
/// the associated `InputSource`.
///
/// A `BaseInfo` may be built from input sources which cannot resolve
/// relative links---for instance, stdin. It may also be built from input
/// sources which can resolve *locally*-relative links, but not *root*-relative
/// links.
#[derive(Debug, PartialEq, Eq, Clone, Deserialize, Default)]
#[serde(try_from = "String")]
pub enum BaseInfo {
    /// No base information is available. This is for sources with no base
    /// information, such as [`ResolvedInputSource::Stdin`], and for URLs which
    /// *cannot be a base*, such as `data:` and `tel:`. [`BaseInfo::None`]
    /// can resolve no relative links; only fully-qualified links will be
    /// parsed successfully.
    #[default]
    None,

    /// A base which cannot resolve root-relative links. This is for
    /// `file:` URLs where the root directory is not known. As such, you can
    /// traverse relative to the current URL (by traversing the filesystem),
    /// but you cannot jump to the "root".
    NoRoot(Url),

    /// A full base made up of `origin` and `path`. This can resolve
    /// all kinds of relative links.
    ///
    /// All non-`file:` URLs which *can be a base* fall into this case. For these,
    /// `origin` and `path` are obtained by dividing the source URL into its
    /// origin and path. When joined, `${origin}/${path}` should be equivalent
    /// to the source's original URL.
    ///
    /// This also represents `file:` URLs with a known root. The `origin` field
    /// records the `file:` URL which will be used to resolve root-relative links.
    /// The `path` field is the subpath to a particular input source within the
    /// root. This is retained to resolve locally-relative links.
    ///
    /// In all cases, the `path` field should be a (possibly-empty) locally- or
    /// root-relative link and should not be a full URL or a scheme-relative link.
    Full(Url, String),
}

impl BaseInfo {
    /// Constructs [`BaseInfo::None`].
    #[must_use]
    pub const fn none() -> Self {
        Self::None
    }

    /// Constructs [`BaseInfo::Full`] with the given fields.
    #[must_use]
    pub const fn full(origin: Url, path: String) -> Self {
        Self::Full(origin, path)
    }

    /// Constructs a [`BaseInfo`], with the variant being determined by the given URL.
    ///
    /// - A [`Url::cannot_be_a_base`] URL will yield [`BaseInfo::None`].
    /// - A `file:` URL will yield [`BaseInfo::NoRoot`].
    /// - For other URLs, a [`BaseInfo::Full`] will be constructed from the URL's
    ///   origin and path.
    ///
    /// Compared to [`BaseInfo::from_base_url`], this function is more lenient in
    /// what it accepts because this function should return *a* result for all
    /// input source URLs.
    #[must_use]
    pub fn from_source_url(url: &Url) -> Self {
        if url.scheme() == "file" {
            Self::NoRoot(url.clone())
        } else {
            match Self::split_url_origin_and_path(url) {
                Some((origin, path)) => Self::full(origin, path),
                None => Self::none(),
            }
        }
    }

    /// Split URL into its origin and path, if possible. Will fail and return
    /// `None` for URLs which *cannot be a base*.
    fn split_url_origin_and_path(url: &Url) -> Option<(Url, String)> {
        let origin = url.join("/").ok()?;
        let subpath = origin.make_relative(url)?;
        Some((origin, subpath))
    }

    /// Constructs a [`BaseInfo`] from the given URL, requiring that the given path be acceptable as a
    /// base URL. That is, it cannot be a special scheme like `data:`.
    ///
    /// # Errors
    ///
    /// Errors if the given URL cannot be a base.
    pub fn from_base_url(url: &Url) -> Result<BaseInfo, ErrorKind> {
        if url.cannot_be_a_base() {
            return Err(ErrorKind::InvalidBase(
                url.to_string(),
                "The given URL cannot be used as a base URL".to_string(),
            ));
        }

        Ok(Self::from_source_url(url))
    }

    /// Constructs a [`BaseInfo`] from the given filesystem path, requiring that
    /// the given path be absolute. Assumes that the given path represents a directory.
    ///
    /// This constructs a [`BaseInfo::Full`] where root-relative links will go to
    /// the given path.
    ///
    /// # Errors
    ///
    /// Errors if the given path is not an absolute path.
    pub fn from_path(path: &Path) -> Result<BaseInfo, ErrorKind> {
        let Ok(url) = Url::from_directory_path(path) else {
            return Err(ErrorKind::InvalidBase(
                path.to_string_lossy().to_string(),
                "Base must either be a full URL (with scheme) or an absolute local path"
                    .to_string(),
            ));
        };

        Self::from_base_url(&url).map(|x| x.use_fs_path_as_origin().into_owned())
    }

    /// If this is a [`BaseInfo::NoRoot`], promote it to a [`BaseInfo::Full`]
    /// by using the filesystem root as the "origin" for root-relative links.
    /// Root-relative links will go to the filesystem root.
    ///
    /// Generally, this function should be avoided in favour of a more explicit
    /// user-provided root directory. The filesystem root is rarely a good place
    /// to look for files.
    ///
    /// Makes no change to other [`BaseInfo`] variants.
    ///
    /// # Panics
    ///
    /// If unable to split a [`BaseInfo::NoRoot`] into origin and path.
    #[must_use]
    pub fn use_fs_root_as_origin(&self) -> Cow<'_, Self> {
        let Self::NoRoot(url) = self else {
            return Cow::Borrowed(self);
        };

        let (fs_root, subpath) = Self::split_url_origin_and_path(url)
            .expect("splitting up a NoRoot file:// URL should work");

        Cow::Owned(Self::full(fs_root, subpath))
    }

    /// If this is a [`BaseInfo::NoRoot`], promote it to a [`BaseInfo::Full`]
    /// by using the entire filesystem path as the "origin" for root-relative links.
    /// Root-relative links will go to the URL that was previously within `NoRoot`.
    ///
    /// Generally, this function should be avoided in favour of a more explicit
    /// user-provided root directory.
    ///
    /// Makes no change to other [`BaseInfo`] variants.
    #[must_use]
    pub fn use_fs_path_as_origin(&self) -> Cow<'_, Self> {
        let Self::NoRoot(url) = self else {
            return Cow::Borrowed(self);
        };

        Cow::Owned(Self::full(url.clone(), String::new()))
    }

    /// Returns the URL for the current [`BaseInfo`], joining the origin and path
    /// if needed.
    #[must_use]
    pub fn url(&self) -> Option<Url> {
        match self {
            Self::None => None,
            Self::NoRoot(url) => Some(url.clone()),
            Self::Full(url, path) => url.join(path).ok(),
        }
    }

    /// Returns the filesystem path for the current [`BaseInfo`] if the underlying
    /// URL is a `file:` URL.
    #[must_use]
    pub fn to_file_path(&self) -> Option<PathBuf> {
        self.url()
            .filter(|url| url.scheme() == "file")
            .and_then(|x| x.to_file_path().ok())
    }

    /// Returns the scheme of the underlying URL.
    #[must_use]
    pub fn scheme(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::NoRoot(url) | Self::Full(url, _) => Some(url.scheme()),
        }
    }

    /// Returns whether this value is [`BaseInfo::None`].
    #[must_use]
    pub const fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns whether this [`BaseInfo`] variant supports resolving root-relative links.
    ///
    /// If true, implies [`BaseInfo::supports_locally_relative`].
    #[must_use]
    pub const fn supports_root_relative(&self) -> bool {
        matches!(self, Self::Full(_, _))
    }

    /// Returns whether this [`BaseInfo`] variant supports resolving locally-relative links.
    #[must_use]
    pub const fn supports_locally_relative(&self) -> bool {
        !self.is_none()
    }

    /// Returns the [`BaseInfo`] which has _more information_
    /// between `self` and the given `fallback`.
    ///
    /// [`BaseInfo::Full`] is preferred over [`BaseInfo::NoRoot`]
    /// which is preferred over [`BaseInfo::None`]. If both `self`
    /// and `fallback` are the same variant, then `self` will be preferred.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn or_fallback<'a>(&'a self, fallback: &'a Self) -> &'a Self {
        match (self, fallback) {
            (x @ Self::Full(_, _), _) => x,
            (_, x @ Self::Full(_, _)) => x,
            (x @ Self::NoRoot(_), _) => x,
            (_, x @ Self::NoRoot(_)) => x,
            (x @ Self::None, Self::None) => x,
        }
    }

    /// Parses the given URL text into a fully-qualified URL, including
    /// resolving relative links if supported by the current [`BaseInfo`].
    ///
    /// To parse and resolve relative links, this uses [`Url::join`] with
    /// the current [`BaseInfo`]'s URL as a base, as applicable.
    ///
    /// # Errors
    ///
    /// Returns an error if the text is an invalid URL, or if the text is a
    /// relative link and this [`BaseInfo`] variant cannot resolve
    /// the relative link.
    pub fn parse_url_text(&self, text: &str) -> Result<Url, ErrorKind> {
        use ParseError::RelativeUrlWithoutBase;

        match Uri::try_from(text) {
            Ok(Uri { url }) => Ok(url),

            Err(ErrorKind::ParseUrl(RelativeUrlWithoutBase, _))
                if !self.supports_root_relative() && is_root_relative_link(text) =>
            {
                Err(ErrorKind::RootRelativeLinkWithoutRoot(text.to_string()))
            }

            Err(ErrorKind::ParseUrl(RelativeUrlWithoutBase, _)) => match self {
                // Cannot resolve any relative links
                Self::None => Err(RelativeUrlWithoutBase),

                // Resolve locally-relative link using NoRoot
                Self::NoRoot(base) => base.join(text),

                // Resolve root-relative link with `file:` base by changing it to
                // a subpath of the origin.
                Self::Full(origin, _)
                    if is_root_relative_link(text) && origin.scheme() == "file" =>
                {
                    let locally_relative = format!(".{}", text.trim_ascii_start());
                    origin.join(&locally_relative)
                }

                // Resolve all other relative links, including root-relative links
                // of non-file bases.
                Self::Full(origin, subpath) => origin.join(subpath).and_then(|x| x.join(text)),
            }
            .map_err(|e| ErrorKind::ParseUrl(e, text.to_string())),

            Err(e) => Err(e),
        }
    }

    /// Parses the given URL text into a fully-qualified URL, including
    /// resolving relative links if supported by the current [`BaseInfo`]
    /// and applying the given root-dir if necessary.
    ///
    /// The root-dir is applied if the current `BaseInfo` is [`BaseInfo::None`]
    /// or has a `file:` URL and if the given text is a root-relative link.
    /// In these cases, the given `root_dir` will *override* the original
    /// `BaseInfo`.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`BaseInfo::parse_url_text`].
    pub fn parse_url_text_with_root_dir(
        &self,
        text: &str,
        root_dir: Option<&Url>,
    ) -> Result<Url, ErrorKind> {
        // HACK: if root-dir is specified, apply it by fudging around with
        // file:// URLs. eventually, someone up the stack should construct
        // the BaseInfo::Full for root-dir and this function should be deleted.

        // NOTE: also apply root-dir for BaseInfo::None :)
        let fake_base_info = match (self.scheme(), root_dir) {
            (Some("file") | None, Some(root_dir)) if is_root_relative_link(text) => {
                Cow::Owned(Self::full(root_dir.clone(), String::new()))
            }
            _ => Cow::Borrowed(self),
        };

        fake_base_info.parse_url_text(text)
    }
}

impl TryFrom<&str> for BaseInfo {
    type Error = ErrorKind;

    /// Attempts to parse a base from the given string which may be
    /// a URL or a filesystem path. In both cases, the string must
    /// represent a valid base (i.e., not resulting in [`BaseInfo::None`]).
    /// Otherwise, an error will be returned.
    ///
    /// Note that this makes a distinction between filesystem paths as paths
    /// and filesystem paths as URLs. When specified as a path, they will
    /// become [`BaseInfo::Full`] but when specified as a URL, they will
    /// become [`BaseInfo::NoRoot`].
    ///
    /// Additionally, the empty string is accepted and will be parsed to
    /// [`BaseInfo::None`].
    fn try_from(value: &str) -> Result<Self, ErrorKind> {
        if value.is_empty() {
            return Ok(BaseInfo::none());
        }
        match utils::url::parse_url_or_path(value) {
            Ok(url) => BaseInfo::from_base_url(&url),
            Err(path) => BaseInfo::from_path(&PathBuf::from(path)),
        }
    }
}

impl TryFrom<String> for BaseInfo {
    type Error = ErrorKind;
    fn try_from(value: String) -> Result<Self, ErrorKind> {
        BaseInfo::try_from(value.as_ref())
    }
}

#[cfg(test)]
mod tests {

    use super::BaseInfo;
    use reqwest::Url;
    use rstest::rstest;

    #[test]
    fn test_base_info_construction() {
        assert_eq!(
            BaseInfo::try_from("https://a.com/b/?q#x").unwrap(),
            BaseInfo::Full(Url::parse("https://a.com").unwrap(), "b/?q#x".to_string())
        );
        assert_eq!(
            BaseInfo::try_from("file:///file-path").unwrap(),
            BaseInfo::NoRoot(Url::parse("file:///file-path").unwrap())
        );
        assert_eq!(
            BaseInfo::try_from("/file-path").unwrap(),
            BaseInfo::Full(Url::parse("file:///file-path/").unwrap(), String::new())
        );

        let urls = ["https://a.com/b/?q#x", "file:///a.com/b/?q#x"];
        // .url() of base-info should return the original URL
        for url_str in urls {
            let url = Url::parse(url_str).unwrap();
            assert_eq!(BaseInfo::try_from(url_str).unwrap().url(), Some(url));
        }
    }

    #[test]
    fn test_base_info_with_http_base() {
        let base = BaseInfo::try_from("https://a.com/c/u/").unwrap();
        let root_dir = Url::parse("file:///root/").unwrap();

        // shouldn't trigger the root URL
        assert_eq!(
            base.parse_url_text_with_root_dir("/a", Some(&root_dir)),
            Ok(Url::parse("https://a.com/a").unwrap())
        );

        assert_eq!(
            base.parse_url_text_with_root_dir("..", Some(&root_dir)),
            Ok(Url::parse("https://a.com/c/").unwrap())
        );

        // not many tests here because it's covered by join_rooted tests
    }

    #[test]
    fn test_base_info_parse_with_root_dir() {
        let base = BaseInfo::try_from("/file-path").unwrap();
        let root_dir = Url::parse("file:///root/").unwrap();

        // first, links which shouldn't trigger the root URL
        assert_eq!(
            base.parse_url_text_with_root_dir("a", Some(&root_dir)),
            Ok(Url::parse("file:///file-path/a").unwrap())
        );
        assert_eq!(
            base.parse_url_text_with_root_dir("./a", Some(&root_dir)),
            Ok(Url::parse("file:///file-path/a").unwrap())
        );
        assert_eq!(
            base.parse_url_text_with_root_dir("///scheme-relative", Some(&root_dir)),
            Ok(Url::parse("file:///scheme-relative").unwrap())
        );
        assert_eq!(
            base.parse_url_text_with_root_dir("https://a.com/b?q", Some(&root_dir)),
            Ok(Url::parse("https://a.com/b?q").unwrap())
        );
        assert_eq!(
            base.parse_url_text_with_root_dir("file:///a/", Some(&root_dir)),
            Ok(Url::parse("file:///a/").unwrap())
        );

        // basic root dir use
        assert_eq!(
            base.parse_url_text_with_root_dir("/a", Some(&root_dir)),
            Ok(Url::parse("file:///root/a").unwrap())
        );

        // root-dir can be traversed out of
        assert_eq!(
            base.parse_url_text_with_root_dir("/../../", Some(&root_dir)),
            Ok(Url::parse("file:///").unwrap())
        );
    }

    #[rstest]
    // normal HTTP traversal and parsing absolute links
    #[case("https://a.com/b", "x/", "d", "https://a.com/x/d")]
    #[case("https://a.com/b/", "x/", "d", "https://a.com/b/x/d")]
    #[case("https://a.com/b/", "", "https://new.com", "https://new.com/")]
    // parsing absolute file://
    #[case("https://a.com/b/", "", "file:///a", "file:///a")]
    #[case("https://a.com/b/", "", "file:///a/", "file:///a/")]
    #[case("https://a.com/b/", "", "file:///a/b/", "file:///a/b/")]
    // file traversal
    #[case("file:///a/b/", "", "/x/y", "file:///a/b/x/y")]
    #[case("file:///a/b/", "", "a/", "file:///a/b/a/")]
    #[case("file:///a/b/", "a/", "../..", "file:///a/")]
    #[case("file:///a/b/", "a/", "/", "file:///a/b/")]
    #[case("file:///a/b/", "", "/..", "file:///a/")]
    #[case("file:///a/b/", "", "/../../", "file:///")]
    #[case("file:///a/b/", "", "?", "file:///a/b/?")]
    #[case("file:///a/b/", ".", "?", "file:///a/b/?")]
    // HTTP relative links
    #[case("https://a.com/x", "", "#", "https://a.com/x#")]
    #[case("https://a.com/x", "", "../../..", "https://a.com/")]
    #[case("https://a.com/x", "?q", "#x", "https://a.com/x?q#x")]
    #[case("https://a.com/x", ".", "?a", "https://a.com/?a")]
    #[case("https://a.com/x/", "", "/", "https://a.com/")]
    #[case("https://a.com/x?q#anchor", "", "?q", "https://a.com/x?q")]
    #[case("https://a.com/x#anchor", "", "?x", "https://a.com/x?x")]
    // scheme relative link - can traverse outside of root
    #[case("file:///root/", "", "///new-root", "file:///new-root")]
    #[case("file:///root/", "", "//a.com/boop", "file://a.com/boop")]
    #[case("https://root/", "", "//a.com/boop", "https://a.com/boop")]
    fn test_join_rooted(
        #[case] origin: &str,
        #[case] path: &str,
        #[case] text: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(
            BaseInfo::full(Url::parse(origin).unwrap(), path.to_string())
                .parse_url_text(text)
                .unwrap()
                .to_string(),
            expected,
            "origin={origin}, path={path:?}, text={text:?}, expected={expected}"
        );
    }

    #[rstest]
    // file URLs without trailing / are kinda weird.
    #[case("file:///a/b/c", "", "/../../x", "file:///x")]
    #[case("file:///a/b/c", "", "/", "file:///a/b/")]
    #[case("file:///a/b/c", "", ".?qq", "file:///a/b/?qq")]
    #[case("file:///a/b/c", "", "#x", "file:///a/b/c#x")]
    #[case("file:///a/b/c", "", "./", "file:///a/b/")]
    #[case("file:///a/b/c", "", "c", "file:///a/b/c")]
    // joining with d
    #[case("file:///a/b/c", "d", "/../../x", "file:///x")]
    #[case("file:///a/b/c", "d", "/", "file:///a/b/")]
    #[case("file:///a/b/c", "d", ".", "file:///a/b/")]
    #[case("file:///a/b/c", "d", "./", "file:///a/b/")]
    // joining with d/
    #[case("file:///a/b/c", "d/", "/", "file:///a/b/")]
    #[case("file:///a/b/c", "d/", ".", "file:///a/b/d/")]
    #[case("file:///a/b/c", "d/", "./", "file:///a/b/d/")]
    fn test_join_rooted_with_trailing_filename(
        #[case] origin: &str,
        #[case] path: &str,
        #[case] text: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(
            BaseInfo::full(Url::parse(origin).unwrap(), path.to_string())
                .parse_url_text(text)
                .unwrap()
                .to_string(),
            expected,
            "origin={origin}, path={path:?}, text={text:?}, expected={expected}"
        );
    }
}
