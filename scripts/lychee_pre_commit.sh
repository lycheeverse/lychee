#!/bin/bash
set -e

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

# Auto-install lychee if not found
if ! command -v lychee &> /dev/null; then
    echo "Installing lychee..."
    
    # Try cargo-binstall first (faster - uses pre-built binaries)
    if command -v cargo-binstall &> /dev/null; then
        echo "Using cargo-binstall for faster installation..."
        cargo binstall lychee --no-confirm
    else
        # Fall back to cargo install (slower - compiles from source)
        echo "Using cargo install (this may take a few minutes)..."
        cargo install lychee
    fi
fi

command -v lychee >/dev/null || {
    echo "lychee installation failed or binary not found in PATH" >&2
    exit 1
}

# Run lychee with all passed arguments
exec lychee "$@"