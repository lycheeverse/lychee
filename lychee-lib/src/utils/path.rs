use cached::proc_macro::cached;
use path_clean::PathClean;
use std::env;
use std::path::PathBuf;
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
