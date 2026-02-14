#!/usr/bin/env python3

"""
Simply updates the `lychee --help` output in the README.
"""

import subprocess

def main():
    new_help = subprocess.check_output('cargo run -- --help'.split(), encoding='utf-8')
    version = subprocess.check_output('cargo run -- --version'.split(), encoding='utf-8').split()[-1]
    new_help = '\n'.join(
        line.replace(f'lychee/{version}', 'lychee/x.y.z').rstrip()
        for line in new_help.strip().split('\n')
    )

    begin = '\n```help-message\n'
    end = '\n```\n'

    with open('README.md', 'r+') as f:
        text = f.read()
        before, after = text.split(begin, 1)
        old_help, after = after.split(end, 1)

        if old_help.strip() == new_help.strip():
            print('readme already up to date, skipping.')
            return

        f.seek(0)

        f.write(before)
        f.write(begin)
        f.write(new_help)
        f.write(end)
        f.write(after)

        f.truncate()

if __name__ == "__main__":
    main()
