import re
from pathlib import Path

root = Path(__file__).resolve().parents[1]


def fix_file(path):
    with open(path, "r") as f:
        s = f.read()
    original = s

    # Match .on_input(Message::VariantName) where the argument is a bare Message variant
    # (not already a closure like |value| ...)
    # Replace with .on_input(|value| Message::VariantName(value))
    pattern = re.compile(r"\.on_input\(Message::(\w+)\)")

    s = pattern.sub(r".on_input(|value| Message::\1(value))", s)

    if s != original:
        with open(path, "w") as f:
            f.write(s)
        print(f"Fixed: {path}")
    else:
        print(f"No changes: {path}")


for dirpath in ["src"]:
    for path in (root / dirpath).rglob("*.rs"):
        fix_file(path)
