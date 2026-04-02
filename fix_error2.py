import re

with open('lychee-lib/src/types/error.rs', 'r') as f:
    text = f.read()

# Pattern for standard git conflict without diff3 base
pattern = r'<<<<<<< HEAD\n(.*?)\n=======\n(.*?)\n>>>>>>> [a-f0-9A-Z/]+'
def replace(match):
    return match.group(1)

new_text = re.sub(pattern, replace, text, flags=re.DOTALL)
with open('lychee-lib/src/types/error.rs', 'w') as f:
    f.write(new_text)

