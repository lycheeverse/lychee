#!/usr/bin/env bash
#
# Adapted from
# https://github.com/marionebl/svg-term-cli
# https://github.com/sharkdp/fd/blob/master/doc/screencast.sh
#
# Designed to be executed via termsvg from the lychee root directory
set -e
set -u

PROMPT="❯"

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

    enter "lychee --verbose README.md"
    
    enter "lychee https://lychee.cli.rs"

    enter "lychee --verbose --format=json fixtures/TEST.html | jq"

    enter "lychee --no-progress --mode emoji --format detailed https://example.com"

    enter "lychee --dump --include github -- './**/*.md'"
    
    prompt

    sleep 3

    echo ""

    unset IFS
}

main