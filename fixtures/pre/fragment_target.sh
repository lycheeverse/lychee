#!/usr/bin/env bash

# Replaces the content of `target.md` and passes every other file through.
# If `CALLS_LOG` is set, the path of each processed file is appended to it.

if [ -n "$CALLS_LOG" ]; then
    echo "$1" >> "$CALLS_LOG"
fi

case "$1" in
*/target.md)
    printf '# Prepared\n\n## Other\n'
    ;;
*)
    exec cat "$1"
    ;;
esac
