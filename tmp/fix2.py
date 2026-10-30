import re
from pathlib import Path

root = Path(__file__).resolve().parents[1]


def fix_file(path):
    with open(path, "r") as f:
        s = f.read()
    original = s

    # 1. Remove push_constant_ranges: &[], (wgpu 28 removed this field)
    s = re.sub(r"^\s*push_constant_ranges:\s*&\[\],\s*\n", "", s, flags=re.MULTILINE)

    # 2. mipmap_filter was removed in wgpu 28
    s = re.sub(
        r"^\s*mipmap_filter:\s*wgpu::FilterMode::\w+,\s*\n", "", s, flags=re.MULTILINE
    )

    # 3. KeyBind::matches now takes (modifiers, key) where key is Key, not &Key
    #    Change .matches(modifiers, &key) to .matches(modifiers, key)
    s = s.replace(".matches(modifiers, &key)", ".matches(modifiers, key)")

    # 4. widget::column::new() -> widget::Column::new()
    s = s.replace("widget::column::new()", "widget::Column::new()")

    # 5. widget::column(). (no-arg free function call) -> widget::Column::new().
    s = re.sub(r"widget::column\(\)\.", "widget::Column::new().", s)

    # 6. measure_image now returns Option<Size> - need .unwrap() or handle
    #    Pattern: let Size { width, height } = renderer.measure_image(handle);
    #    -> let Some(Size { width, height }) = renderer.measure_image(handle) else { ... };
    #    Actually, let's just add .unwrap() for now to make it compile:
    #    But we need to destructure. Let's do:
    #    let image_size = renderer.measure_image(&self.handle).unwrap();
    #    then use image_size.width, image_size.height
    #    This is complex, so let's handle specific patterns:

    # In image.rs: let Size { width, height } = renderer.measure_image(handle);
    # -> let Some(Size { width, height }) = renderer.measure_image(handle) else { unreachable!() };
    s = re.sub(
        r"let Size \{ width, height \} = renderer\.measure_image\(([^)]+)\);",
        r"let Some(Size { width, height }) = renderer.measure_image(\1) else { panic!(\"no image size\"); };",
        s,
    )

    # In image_player.rs: let image_size = renderer.measure_image(&self.handle);
    # -> let image_size = renderer.measure_image(&self.handle).unwrap();
    s = re.sub(
        r"let image_size = renderer\.measure_image\(&self\.handle\);",
        r"let image_size = renderer.measure_image(&self.handle).unwrap();",
        s,
    )

    # 7. Size::INFINITY -> Size::new(f32::INFINITY as u32, f32::INFINITY as u32)
    #    Actually Size::new takes f32. Let's use:
    #    Size::new(f32::INFINITY, f32::INFINITY)
    s = s.replace("Size::INFINITY", "Size::new(f32::INFINITY, f32::INFINITY)")

    # 8. Space::with_width(x) -> widget::space::horizontal(x)
    s = re.sub(r"widget::space::Space::with_width\(", "widget::space::horizontal(", s)
    # Also handle: Space::with_width(
    s = re.sub(r"\bSpace::with_width\(", "widget::space::horizontal(", s)

    # 9. Space::with_height(x) -> widget::space::vertical(x)
    s = re.sub(r"widget::space::Space::with_height\(", "widget::space::vertical(", s)
    s = re.sub(r"\bSpace::with_height\(", "widget::space::vertical(", s)

    # 10. .run_with_id( -> .with_id( (Subscription API rename in iced 0.14)
    s = s.replace(".run_with_id(", ".with_id(")

    # 11. Key::Named(Named::Space) -> Key::Character(" ")
    s = s.replace("Key::Named(Named::Space)", 'Key::Character(" ")')

    # 12. rule::Style { ... width: N, ... } -> remove width field (no longer exists)
    #     Match within rule::Style { ... } blocks: width: <expr>,
    s = re.sub(
        r"(rule::Style\s*\{[^}]*?)\bwidth:\s*[^,]+,\s*",
        r"\1",
        s,
        flags=re.DOTALL,
    )

    # 13. text_editor::Style { ... icon: ..., ... } -> remove icon field
    s = re.sub(
        r"(text_editor::Style\s*\{[^}]*?)\bicon:\s*[^,]+,\s*",
        r"\1",
        s,
        flags=re.DOTALL,
    )

    # 14. Text<&str> { horizontal_alignment: ..., vertical_alignment: ..., .. }
    #     These fields were removed from the Text struct literal in iced 0.14
    s = re.sub(r"^\s*horizontal_alignment:\s*[^,]+,\s*\n", "", s, flags=re.MULTILINE)
    s = re.sub(r"^\s*vertical_alignment:\s*[^,]+,\s*\n", "", s, flags=re.MULTILINE)

    # 15. cosmic_theme::Theme .background (now private) -> .background_color()
    #     Only when used as a field access on a theme value
    #     Pattern: theme.background or cosmic.background
    s = re.sub(r"(\w+)\.background\b(?!\w)", r"\1.background_color()", s)

    # 16. ContextDrawer::header_actions -> header
    s = s.replace(".header_actions(", ".header(")

    # 17. .context_menu(self.nav_context_menu(self.nav_bar_context_id))
    #     In iced 0.14, context_menu takes no args (entity comes from self.nav_bar_context_id)
    #     Change .context_menu(self.nav_context_menu(self.nav_bar_context_id)) to .context_menu()
    #     But actually the error says "takes 0 arguments but 1 argument was supplied"
    #     So the method is now .context_menu() with no args
    s = re.sub(
        r"\.context_menu\((self\.nav_context_menu\(self\.nav_bar_context_id\))\)",
        r".context_menu()",
        s,
    )

    # 18. desktop::apps() or similar returns an iterator, not a collection
    #     .iter() on an iterator doesn't work; remove .iter()
    #     Pattern: something.iter() where something is already an iterator
    #     This is hard to detect generically, skip

    # 19. mime_app.rs: desktop::DesktopEntryData apps - the API changed
    #     apps.iter() where apps is impl Iterator -> just use apps directly
    #     Pattern: .iter() after a call that returns an iterator
    #     Let's handle: for app in apps.iter() -> for app in apps
    #     But this is risky, skip for now

    if s != original:
        with open(path, "w") as f:
            f.write(s)
        print(f"Fixed: {path}")
    else:
        print(f"No changes: {path}")


for dirpath in ["src"]:
    for path in (root / dirpath).rglob("*.rs"):
        fix_file(path)
