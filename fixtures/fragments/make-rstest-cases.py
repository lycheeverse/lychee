#!/usr/bin/env python3

"""
Pipe in the GitHub HTML content of special-casing.md

For example:

    curl -L -H "Accept: application/vnd.github.html+json" -H "X-GitHub-Api-Version: 2026-03-10" https://api.github.com/repos/lycheeverse/lychee/contents/fixtures/fragments/special-casing.md?ref=1e9f3ac51108cd19e669e4243ec604903d47db2b

"""

import sys

github_slugified = {}
for line in sys.stdin.buffer.read().splitlines():
    if b'class="markdown-heading"' in line:
        title = line.split(b'</h1>', 1)[0].rsplit(b'>', 1)[-1]
        fragment = line.split(b'id="user-content-', 1)[-1].split(b'"', 1)[0]
        github_slugified[title] = fragment

# print(github_slugified)

def to_rust_byte_literal(bytes):
    bytes = ascii(bytes).replace("'", '"')
    return f'str::from_utf8({bytes}).unwrap()'

for title, fragment in github_slugified.items():
    print(
        '#[case(',
        to_rust_byte_literal(title),
        ', ',
        to_rust_byte_literal(fragment),
        ')]',
        sep=''
    )



