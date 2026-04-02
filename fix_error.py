import re

with open('lychee-lib/src/types/error.rs', 'r') as f:
    text = f.read()

# For error.rs we want to keep our Option<String> and None, so basically we keep HEAD
pattern = r'<<<<<<< HEAD(.*?)\|\|\|\|\|\|\| f8a35def.*?=======(.*?)>>>>>>> [a-f0-9]+'
def replace(match):
    return match.group(1)

new_text = re.sub(pattern, replace, text, flags=re.DOTALL)
with open('lychee-lib/src/types/error.rs', 'w') as f:
    f.write(new_text)

