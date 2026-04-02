with open('lychee-lib/src/types/error.rs', 'r') as f:
    content = f.read()

import re
content = re.sub(r'(ErrorKind::ReadFileInput\(e, path\) => match e.kind\(\) \{.*?\},\n)', r'\1            ErrorKind::ReadInputUrlStatusCode(_) => "".to_string(),\n', content, flags=re.DOTALL)

with open('lychee-lib/src/types/error.rs', 'w') as f:
    f.write(content)
