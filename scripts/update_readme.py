#!/usr/bin/env python3

"""
Simply updates the `lychee --help` output in the README.
"""

import subprocess

def main():
    new_help = subprocess.check_output('cargo run -- --help'.split(), encoding='utf-8')
    version = subprocess.check_output('cargo run -- --version'.split(), encoding='utf-8').split()[-1]

    lines = new_help.strip().splitlines()
    new_help = '\n'.join(line.rstrip() for line in lines)
    new_help = new_help.replace(f'lychee/{version}', 'lychee/x.y.z')

    begin = '\n```help-message\n'
    end = '\n```\n'

    with open('README.md', 'r+') as f:
        text = f.read()
        before, after = text.split(begin, 1)
        _, after = after.split(end, 1)

        f.seek(0)

        f.write(before)
        f.write(begin)
        f.write(new_help)
        f.write(end)
        f.write(after)

        f.truncate()

if __name__ == "__main__":
    main()
