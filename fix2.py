with open('lychee-lib/src/types/error.rs', 'r') as f:
    content = f.read()

import re
content = re.sub(r'(ErrorKind::ReadFileInput.*?,\n)', r'\1            ErrorKind::ReadInputUrlStatusCode(_) => "".to_string(),\n', content)
content = re.sub(r'(\(Self::ReadStdinInput\(e1\), Self::ReadStdinInput\(e2\)\) => e1.kind\(\) == e2.kind\(\),\n)', r'(Self::ReadInputUrlStatusCode(e1), Self::ReadInputUrlStatusCode(e2)) => e1 == e2,\n            \1', content)
content = re.sub(r'(Self::ReadStdinInput\(e\) => e.kind\(\).hash\(state\),\n)', r'Self::ReadInputUrlStatusCode(c) => c.hash(state),\n            \1', content)

with open('lychee-lib/src/types/error.rs', 'w') as f:
    f.write(content)
