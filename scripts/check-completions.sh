#!/usr/bin/env bash

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

LYCHEE="${TARGET_DIR:-target}/release/lychee"
[[ -e "$LYCHEE" ]] || LYCHEE="${LYCHEE/%\/release\/lychee/\/debug\/lychee}"

if [[ ! -e "$LYCHEE" ]]; then
    echo -e "${RED}Error: lychee binary not found at $LYCHEE${NC}" >&2
    exit 1
fi

echo "Testing completions for: $LYCHEE"
echo

# Get all options from help
HELP_OPTIONS=$(
    "$LYCHEE" --help |
    grep -E '^\s+--?[a-z0-9]' |
    grep -oE -- '--[a-z0-9-]+' |
    sort -u
)

# Track if any check failed
FAILED=false

check_completion() {
    local shell=$1
    local file=$2

    echo -n "Checking $shell completion... "

    if [[ ! -f "$file" ]]; then
        echo -e "${RED}FAIL${NC} - File not found"
        FAILED=true
        return
    fi

    if [[ ! -s "$file" ]]; then
        echo -e "${RED}FAIL${NC} - File is empty"
        FAILED=true
        return
    fi

    # Check for missing options
    local missing_opts=()
    while IFS= read -r opt; do
        local opt_name="${opt#--}"

        # Fish uses -l option_name format, others use --option-name
        if [[ "$shell" == "Fish" ]]; then
            if ! grep -qE -- "-l ${opt_name}\\b" "$file"; then
                missing_opts+=("$opt")
            fi
        else
            if ! grep -qF -- "$opt" "$file"; then
                missing_opts+=("$opt")
            fi
        fi
    done <<< "$HELP_OPTIONS"

    if ((${#missing_opts[@]} > 0)); then
        echo -e "${RED}FAIL${NC} - Missing ${#missing_opts[@]} option(s)"
        printf '    %s\n' "${missing_opts[@]}" | head -5
        if ((${#missing_opts[@]} > 5)); then
            echo "    ..."
        fi
        FAILED=true
    else
        echo -e "${GREEN}OK${NC}"
    fi
}

# Check each completion file
check_completion "Bash" "lychee-bin/complete/lychee.bash"
check_completion "Elvish" "lychee-bin/complete/lychee.elv"
check_completion "Fish" "lychee-bin/complete/lychee.fish"
check_completion "PowerShell" "lychee-bin/complete/_lychee.ps1"
check_completion "Zsh" "lychee-bin/complete/_lychee"

echo

if $FAILED; then
    echo -e "${RED}FAILED${NC}"
    echo
    echo "Completions are out of sync with --help options."
    echo "To update all completion files, run:"
    echo
    echo -e "  ${GREEN}make completions${NC}"
    echo
    exit 1
else
    echo -e "${GREEN}All checks passed${NC}"
    exit 0
fi
