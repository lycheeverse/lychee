with open('lychee-lib/src/types/error.rs', 'r') as f:
    content = f.read()

insert = """
    /// Error while reading an input URL
    #[error("Cannot read input content from URL: status code {0}. To check links in error pages, download and check locally instead.")]
    ReadInputUrlStatusCode(StatusCode),
"""

import re
content = re.sub(r'(\s+/// Error while reading stdin as input)', insert + r'\1', content)

with open('lychee-lib/src/types/error.rs', 'w') as f:
    f.write(content)
