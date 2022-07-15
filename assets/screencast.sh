#!/usr/bin/env bash
#
# Adapted from
# https://github.com/marionebl/svg-term-cli
# https://github.com/sharkdp/fd/blob/master/doc/screencast.sh
#
# Designed to be executed via svg-term from the lychee root directory:
# svg-term --command="bash assets/screencast.sh" --out assets/screencast.svg --padding=10
# Then run this (workaround for https://github.com/sharkdp/fd/issues/1003):
# sed -i '' 's/<text/<text font-size="1.67"/g' assets/screencast.svg
set -e
set -u

PROMPT="❯"

# Always use latest version of lychee for screencast
alias lychee="cargo run --"

enter() {
    INPUT=$1
    DELAY=1

    prompt
    sleep "$DELAY"
    type "$INPUT"
    sleep 0.5
    printf '%b' "\\n"
    eval "$INPUT"
    type "\\n"
}

prompt() {
    printf '%b ' "$PROMPT" | pv -q
}

type() {
    printf '%b' "$1" | pv -qL $((10+(-2 + RANDOM%5)))
}

main() {
    IFS='%'

    enter "lychee README.md"

    enter "lychee --verbose --format=json fixtures/test.html"

    enter "lychee --no-progress --format detailed https://example.com"

    enter "lychee --dump --include github -- './**/*.md'"

    prompt

    sleep 3

    echo ""

    unset IFS
}

main