import os
import re


def fix_file(path):
    with open(path, "r") as f:
        s = f.read()
    original = s

    # Fix widget::checkbox(label, is_checked) -> widget::checkbox(is_checked).label(label)
    # Handles both single-line and multi-line forms
    pattern = re.compile(r"widget::checkbox\(\s*([^,]+),\s*([^)]+)\)", re.DOTALL)

    def checkbox_repl(m):
        label = m.group(1).strip()
        is_checked = m.group(2).strip()
        return f"widget::checkbox({is_checked}).label({label})"

    s = pattern.sub(checkbox_repl, s)

    # Fix widget::column(). -> widget::column::new().
    s = s.replace("widget::column().", "widget::column::new().")

    if s != original:
        with open(path, "w") as f:
            f.write(s)
        print(f"Fixed: {path}")
    else:
        print(f"No changes: {path}")


# Process all .rs files in src/
src_dir = "src"
for root, dirs, files in os.walk(src_dir):
    for f in files:
        if f.endswith(".rs"):
            fix_file(os.path.join(root, f))
