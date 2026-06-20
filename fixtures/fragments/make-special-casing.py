#!/usr/bin/env python3
import re
import sys

"""
Pipe in the contents of https://www.unicode.org/Public/UNIDATA/SpecialCasing.txt

For example:

    curl https://www.unicode.org/Public/UNIDATA/SpecialCasing.txt | fixtures/fragments/make-special-casing.py > fixtures/fragments/special-casing.md

"""

a = sys.stdin.read()

# Exclude Language-Sensitive Mappings by deleting everything after that
# section heading
a = a.split('Language-Sensitive Mappings', 1)[0]

# Turn contiguous regions beginning with '#' into Markdown code blocks
a = re.sub(r'^#[^\n]*\n\n', lambda x: x[0].rstrip() + '\n```\n\n', a, flags=re.MULTILINE)
a = re.sub(r'\n\n#', '\n\n```\n#', a)

# Turn lines not beginning with backtick or # into Markdown headings.
a = re.sub(r'^[^`#\n]', lambda x: '# ' + x[0], a, flags=re.MULTILINE)

# Turn Unicode codepoints (4 hex digits) into HTML entities and make the
# codepoints adjacent to trigger special casing.
a = re.sub(r'[0-9A-F]{4}', lambda x: '&#x' + x[0] + ';', a)
a = re.sub(r'; &', ';&', a)

print('```')
print(a)
print('```')

