#!/usr/bin/env bash
#
# Keep the `lychee-docker` pre-commit hook in sync with the workspace version.
#
# The `lychee` and `lychee-system` hooks derive their version dynamically (from
# the checked-out git tag or the system install), but `lychee-docker` has to
# pin a concrete image tag in `.pre-commit-hooks.yaml`. This script rewrites
# that pinned tag to match `[workspace.package] version` in `Cargo.toml`.
#
# Usage:
#   scripts/sync_docker_pre_commit_version.sh           # update the file in place
#   scripts/sync_docker_pre_commit_version.sh --check   # fail if out of sync (no writes)
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

HOOKS_FILE=".pre-commit-hooks.yaml"
CARGO_FILE="Cargo.toml"

# Extract the workspace version, i.e. the first `version = "x.y.z"` under
# `[workspace.package]`.
version="$(
    awk '
        /^\[workspace\.package\]/ { in_section = 1; next }
        /^\[/                     { in_section = 0 }
        in_section && /^version[[:space:]]*=/ {
            gsub(/[^0-9.]/, "", $0)
            print
            exit
        }
    ' "$CARGO_FILE"
)"

if [[ -z "$version" ]]; then
    echo "error: could not determine workspace version from $CARGO_FILE" >&2
    exit 1
fi

current="$(sed -n 's/.*entry:[[:space:]]*lycheeverse\/lychee:\([0-9.]*\).*/\1/p' "$HOOKS_FILE")"

if [[ -z "$current" ]]; then
    echo "error: could not find pinned 'lycheeverse/lychee:<version>' entry in $HOOKS_FILE" >&2
    exit 1
fi

if [[ "$current" == "$version" ]]; then
    echo "lychee-docker pre-commit version already in sync ($version)."
    exit 0
fi

if [[ "${1:-}" == "--check" ]]; then
    echo "error: lychee-docker pre-commit version is out of sync." >&2
    echo "  $HOOKS_FILE pins: $current" >&2
    echo "  $CARGO_FILE expects: $version" >&2
    echo "Run scripts/sync_docker_pre_commit_version.sh to fix." >&2
    exit 1
fi

sed -i.bak "s|\(entry:[[:space:]]*lycheeverse/lychee:\)[0-9.]*|\1${version}|" "$HOOKS_FILE"
rm -f "${HOOKS_FILE}.bak"

echo "Updated lychee-docker pre-commit version: $current -> $version"
