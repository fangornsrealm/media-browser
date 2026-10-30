import os
import re


def fix_file(path):
    with open(path, "r") as f:
        s = f.read()
    original = s

    # Match widget::text_input(fl!("..."), ...) and add & to the fl! placeholder
    # Only match when not already prefixed with &
    pattern = re.compile(
        r"widget::text_input\(\s*(?<!&)fl!\(\"([^\"]*)\"\)", re.MULTILINE
    )

    def repl(m):
        return f'widget::text_input(&fl!("{m.group(1)}")'

    s = pattern.sub(repl, s)

    if s != original:
        with open(path, "w") as f:
            f.write(s)
        print(f"Fixed: {path}")
    else:
        print(f"No changes: {path}")


src_dir = "src"
for root, dirs, files in os.walk(src_dir):
    for f in files:
        if f.endswith(".rs"):
            fix_file(os.path.join(root, f))
