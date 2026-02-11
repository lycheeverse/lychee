#!/usr/bin/env bash
set -eu

first_arg="${1:-}"
if [[ "$first_arg" == LYCHEE_VERSION=* ]]; then
    shift
    exec env "$first_arg" "$0" "$@"
fi

# Find and navigate to the pre-commit cache folder where lychee
# is checked out. something like: ~/.cache/pre-commit/repo7r00atq6/
pushd "$(dirname "$0")" >/dev/null
LYCHEE_DIR="$(git rev-parse --show-toplevel)"
popd >/dev/null
pushd "$LYCHEE_DIR" >/dev/null

# install tools into a subfolder of the pre-commit repo folder, so they can be cached
# but still remain independent from other cargo tools.
export CARGO_HOME="$LYCHEE_DIR/.cargo"
export CARGO_BIN="$CARGO_HOME/bin"
export PATH="$CARGO_BIN:$PATH"

LYCHEE="$CARGO_BIN/lychee"

# fast path if lychee has already been downloaded
if [[ -x "$LYCHEE" ]]; then
    popd >/dev/null
    exec "$LYCHEE" "$@"
fi

if [[ -z "${LYCHEE_VERSION:-}" ]]; then

    # pre-commit doesn't fetch tags by default, so we might need to grab them.
    if [[ -z "$(git tag)" ]]; then
        git fetch --tags &>/dev/null
    fi
    # this is safe to cache because if pre-commit `rev` is updated, it makes a
    # fresh cache folder and we'll re-fetch tags again.

    # get the tag of the current commit, matching the glob.
    tag="$(git describe --tags --exact-match --match 'lychee-*v*' 2>/dev/null || true)"
    version_regex='lychee-(lib-)?v([0-9.]+)'
    if [[ "$tag" =~ $version_regex ]]; then
        LYCHEE_VERSION="${BASH_REMATCH[2]}"
    else
        LYCHEE_VERSION=''
    fi
fi

if [[ -n "$LYCHEE_VERSION" ]]; then

    if ! command -v cargo-binstall &>/dev/null; then
        echo "Installing cargo-binstall..." >&2
        curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | BASH_XTRACEFD=7 bash 7>/dev/null
    fi
    # TODO: it's possible that a user has cargo on their path but not cargo-binstall,
    # so `cargo binstall` would succeed but `cargo-binstall` wouldn't. it would be nice
    # to support this case also.

    echo 'Installing lychee by cargo-binstall...' >&2
    cargo-binstall -y "lychee@$LYCHEE_VERSION"
else
    echo "lychee pre-commit requires 'rev' to be a versioned release tag," \
        "such as 'lychee-v0.XX.0'." \
        "please update the 'rev' field in your .pre-commit-config.yaml" >&2
    exit 100

    # echo 'Installing lychee by compiling from source (this may be slow)...' >&2
    # cargo install --path lychee-bin --locked
    # cargo clean
fi

# smoke test before we do destructive deletes of the local files
[[ -x "$LYCHEE" ]] || {
    echo 'lychee pre-commit binary not found after cargo-binstall.' \
        "this shouldn't happen and is probably a bug in lychee!" >&2
    exit 100
}

# clean up no-longer-needed files to save a little bit of space
rm -rf .git assets target build "$CARGO_BIN/cargo-binstall"

popd >/dev/null

# Run lychee with all passed arguments
exec "$LYCHEE" "$@"
