with open('lychee-bin/tests/cli.rs', 'r') as f:
    content = f.read()

import re
content = content.replace('.arg("https://example.com/cargo_exclude_test_str")\n            .arg("--dump")', '.arg("https://example.com/cargo_exclude_test_str")')

with open('lychee-bin/tests/cli.rs', 'w') as f:
    f.write(content)
