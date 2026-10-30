import re
from pathlib import Path

root = Path(__file__).resolve().parents[1]
path = root / "src" / "image" / "image_player.rs"

original = path.read_text()

layout_old = """    fn layout(
        &self,
        _tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {"""
layout_new = """    fn layout(
        &mut self,
        _tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {"""
assert layout_old in original, "Could not find image_player layout method"
fixed = original.replace(layout_old, layout_new, 1)

start = fixed.find("    fn on_event(")
end = fixed.find("    fn mouse_interaction(", start)
assert start != -1 and end != -1, "Could not find image_player on_event method"

new_method = """    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let event = event.clone();

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(cursor_position) = cursor.position_over(bounds) else {
                    return;
                };

                match delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => {
                        let state = tree.state.downcast_mut::<State>();
                        let previous_scale = state.scale;

                        if y < 0.0 && previous_scale > self.min_scale
                            || y > 0.0 && previous_scale < self.max_scale
                        {
                            state.scale = (if y > 0.0 {
                                state.scale * (1.0 + self.scale_step)
                            } else {
                                state.scale / (1.0 + self.scale_step)
                            })
                            .clamp(self.min_scale, self.max_scale);

                            let image_size =
                                image_size(renderer, &self.handle, state, bounds.size());

                            let factor = state.scale / previous_scale - 1.0;

                            let cursor_to_center = cursor_position - bounds.center();

                            let adjustment =
                                cursor_to_center * factor + state.current_offset * factor;

                            state.current_offset = Vector::new(
                                if image_size.width > bounds.width {
                                    state.current_offset.x + adjustment.x
                                } else {
                                    0.0
                                },
                                if image_size.height > bounds.height {
                                    state.current_offset.y + adjustment.y
                                } else {
                                    0.0
                                },
                            );
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cursor_position) = cursor.position() else {
                    return;
                };

                let state = tree.state.downcast_mut::<State>();

                state.cursor_grabbed_at = Some(cursor_position);
                state.starting_offset = state.current_offset;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let state = tree.state.downcast_mut::<State>();

                if state.cursor_grabbed_at.is_some() {
                    state.cursor_grabbed_at = None;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let state = tree.state.downcast_mut::<State>();

                if let Some(origin) = state.cursor_grabbed_at {
                    let image_size = image_size(renderer, &self.handle, state, bounds.size());

                    let hidden_width = (image_size.width - bounds.width / 2.0).max(0.0).round();

                    let hidden_height = (image_size.height - bounds.height / 2.0).max(0.0).round();

                    let delta = position - origin;

                    let x = if bounds.width < image_size.width {
                        (state.starting_offset.x - delta.x).clamp(-hidden_width, hidden_width)
                    } else {
                        0.0
                    };

                    let y = if bounds.height < image_size.height {
                        (state.starting_offset.y - delta.y).clamp(-hidden_height, hidden_height)
                    } else {
                        0.0
                    };

                    state.current_offset = Vector::new(x, y);
                }
            }
            _ => {}
        }
    }

"""

fixed = fixed[:start] + new_method + fixed[end:]

if fixed != original:
    path.write_text(fixed)
    print(f"Fixed: {path}")
else:
    print(f"No changes: {path}")
