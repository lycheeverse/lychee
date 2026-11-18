#!/usr/bin/env bash

# Fails on `target.md` and passes every other file through.

case "$1" in
*/target.md)
    >&2 echo "Cannot preprocess target"
    exit 1
    ;;
*)
    exec cat "$1"
    ;;
esac
