import re

with open('lychee-lib/src/types/error.rs', 'r') as f:
    text = f.read()

# Pattern for git conflict with diff3 base - robust regex
pattern = r'<<<<<<< HEAD(.*?)\|\|\|\|\|\|\| [a-f0-9A-Z\n]+?=======(.*?)>>>>>>> origin/master\n'
def replace(match):
    return match.group(1)

new_text = re.sub(pattern, replace, text, flags=re.DOTALL)
with open('lychee-lib/src/types/error.rs', 'w') as f:
    f.write(new_text)

