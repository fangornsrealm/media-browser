import re
from pathlib import Path

root = Path(__file__).resolve().parents[1]


def fix_file(path):
    with open(path, "r") as f:
        s = f.read()
    original = s

    # 1. widget::column::new() -> widget::Column::new()
    #    (column is a free function, not a module; Column::new() is the correct API)
    s = s.replace("widget::column::new()", "widget::Column::new()")

    # 2. widget::column().  -> widget::Column::new().  (remaining no-arg calls)
    s = re.sub(r"widget::column\(\)\.", "widget::Column::new().", s)

    # 3. Size::INFINITY -> Size::new(f32::INFINITY, f32::INFINITY)
    s = s.replace("Size::INFINITY", "Size::new(f32::INFINITY, f32::INFINITY)")

    # 4. Space::with_width(x) -> widget::space::horizontal(x)
    s = re.sub(r"Space::with_width\(", "widget::space::horizontal(", s)

    # 5. Space::with_height(x) -> widget::space::vertical(x)
    s = re.sub(r"Space::with_height\(", "widget::space::vertical(", s)

    # 6. Remove push_constant_ranges: &[], (wgpu 28 removed this field)
    s = re.sub(r"^\s*push_constant_ranges:\s*&\[\],\s*\n", "", s, flags=re.MULTILINE)

    # 7. Key::Named(Named::Space) -> Key::Character(" ")
    #    (Named::Space was removed in iced 0.14; use Character instead)
    s = s.replace("Key::Named(Named::Space)", 'Key::Character(" ")')

    # 8. .run_with_id( -> .with_id( (Subscription API rename in iced 0.14)
    s = s.replace(".run_with_id(", ".with_id(")

    # 9. In rule::Style, width: -> thickness: (field renamed in iced 0.14)
    #    Match rule::Style { ... width: ... } patterns
    s = re.sub(
        r"(rule::Style\s*\{(?:[^}])*?)\bwidth:\s",
        r"\1thickness: ",
        s,
        flags=re.DOTALL,
    )

    # 10. Text struct: horizontal_alignment: -> align_x:, vertical_alignment: -> align_y:
    s = s.replace("horizontal_alignment:", "align_x:")
    s = s.replace("vertical_alignment:", "align_y:")

    # 11. cosmic_theme::Theme.background (private field) -> .background_color()
    #     Only replace when accessed as a field on a theme variable
    s = re.sub(r"\.background\b", ".background_color()", s)

    # 12. ContextDrawer::header_actions -> ContextDrawer::header
    s = s.replace(".header_actions(", ".header(")

    if s != original:
        with open(path, "w") as f:
            f.write(s)
        print(f"Fixed: {path}")
    else:
        print(f"No changes: {path}")


for dirpath in ["src"]:
    for path in (root / dirpath).rglob("*.rs"):
        fix_file(path)
