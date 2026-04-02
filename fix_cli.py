import re

with open('lychee-bin/tests/cli.rs', 'r') as f:
    text = f.read()

# For lychee-bin/tests/cli.rs we want BOTH our additions (test_cli_input_url_status_error) and the remote additions
pattern = r'<<<<<<< HEAD(.*?)\|\|\|\|\|\|\| f8a35def.*?=======(.*?)>>>>>>> [a-f0-9]+'
def replace(match):
    return match.group(1) + "\n" + match.group(2)

new_text = re.sub(pattern, replace, text, flags=re.DOTALL)
with open('lychee-bin/tests/cli.rs', 'w') as f:
    f.write(new_text)

