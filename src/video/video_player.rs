//! Video player widget (iced 0.14 / libcosmic 1.0).
//!
//! Displays the current frame of a [`Video`](crate::video::video::Video) by
//! submitting a [`MediaPrimitive`] to the wgpu renderer. The frame itself is
//! produced by the mpv software-renderer worker thread (see
//! [`crate::mpv::render`]); this widget only snapshots it.
//!
//! Every `RedrawRequested` the widget drains mpv events (EOS / looping
//! bookkeeping) and publishes the `on_new_frame` / `on_end_of_stream`
//! messages while playback is active.

use crate::pipeline::MediaPrimitive;
use crate::video::video::Video;
use cosmic::iced::{
    self, core, Element,
    core::{
        event, layout,
        mouse::{Cursor, Interaction},
        widget::{self, tree::Tree},
        Rectangle, Size,
    },
    window::RedrawRequest,
};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use std::marker::PhantomData;
use std::time::Duration;

/// Video player widget which displays the current frame of a [`Video`].
pub struct VideoPlayer<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: PrimitiveRenderer,
{
    video: &'a Video,
    content_fit: iced::ContentFit,
    width: iced::Length,
    height: iced::Length,
    mouse_hidden: bool,
    on_end_of_stream: Option<Message>,
    on_new_frame: Option<Message>,
    _phantom: PhantomData<(Theme, Renderer)>,
}

impl<'a, Message, Theme, Renderer> VideoPlayer<'a, Message, Theme, Renderer>
where
    Renderer: PrimitiveRenderer,
{
    /// Creates a new video player widget for a given video.
    pub fn new(video: &'a Video) -> Self {
        VideoPlayer {
            video,
            content_fit: iced::ContentFit::Contain,
            width: iced::Length::Shrink,
            height: iced::Length::Shrink,
            mouse_hidden: false,
            on_end_of_stream: None,
            on_new_frame: None,
            _phantom: PhantomData::default(),
        }
    }

    /// Sets the width of the `VideoPlayer` boundaries.
    pub fn width(self, width: impl Into<iced::Length>) -> Self {
        VideoPlayer {
            width: width.into(),
            ..self
        }
    }

    /// Sets the height of the `VideoPlayer` boundaries.
    pub fn height(self, height: impl Into<iced::Length>) -> Self {
        VideoPlayer {
            height: height.into(),
            ..self
        }
    }

    /// Sets the `ContentFit` of the `VideoPlayer`.
    pub fn content_fit(self, content_fit: iced::ContentFit) -> Self {
        VideoPlayer {
            content_fit,
            ..self
        }
    }

    /// Whether the cursor should be hidden while hovering.
    pub fn mouse_hidden(self, mouse_hidden: bool) -> Self {
        VideoPlayer {
            mouse_hidden,
            ..self
        }
    }

    /// Message to send when the video reaches the end of stream (i.e., the
    /// video ends).
    pub fn on_end_of_stream(self, on_end_of_stream: Message) -> Self {
        VideoPlayer {
            on_end_of_stream: Some(on_end_of_stream),
            ..self
        }
    }

    /// Message to send when a new frame becomes available.
    pub fn on_new_frame(self, on_new_frame: Message) -> Self {
        VideoPlayer {
            on_new_frame: Some(on_new_frame),
            ..self
        }
    }
}

/// Computes the letterboxed bounds of a `frame_size` inside `bounds` for
/// `ContentFit::Contain`.
fn contain_bounds(bounds: Rectangle, frame_size: (u32, u32)) -> Rectangle {
    let (fw, fh) = frame_size;
    if fw == 0 || fh == 0 {
        return bounds;
    }
    let fw = fw as f32;
    let fh = fh as f32;
    let bw = bounds.width;
    let bh = bounds.height;
    let scale = (bw / fw).min(bh / fh);
    let w = fw * scale;
    let h = fh * scale;
    Rectangle {
        x: bounds.x + (bw - w) / 2.0,
        y: bounds.y + (bh - h) / 2.0,
        width: w,
        height: h,
    }
}

impl<'a, Message, Theme, Renderer> widget::Widget<Message, Theme, Renderer>
    for VideoPlayer<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + PrimitiveRenderer,
{
    fn size(&self) -> Size<iced::Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, _tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        // The app sets explicit widths/heights; letterboxing of the actual
        // frame happens in `draw` (contain_bounds).
        layout::sized(limits, self.width, self.height, |_| Size::ZERO)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &core::renderer::Style,
        layout: cosmic::iced::core::Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(frame) = self.video.frame_slot().snapshot() else {
            // No frame yet (or the render worker is not publishing).
            log::debug!(
                "[video] draw: no frame in slot (video at {:?})",
                self.video.position()
            );
            return;
        };
        let drawing_bounds = match self.content_fit {
            iced::ContentFit::Contain => contain_bounds(bounds, (frame.width, frame.height)),
            _ => bounds,
        };
        log::debug!(
            "[video] draw: bounds={bounds:?} frame={}x{} -> {drawing_bounds:?}",
            frame.width,
            frame.height
        );

        renderer.draw_primitive(
            drawing_bounds,
            MediaPrimitive {
                id: self.video.id(),
                alive: self.video.alive(),
                frame: frame.bytes,
                width: frame.width,
                height: frame.height,
                stride: frame.stride,
                new_frame: true,
            },
        );
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &event::Event,
        _layout: cosmic::iced::core::Layout<'_>,
        _cursor: Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn core::Clipboard,
        shell: &mut core::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        match *event {
            event::Event::Window(cosmic::iced::window::Event::RedrawRequested(_)) => {
                let video = self.video;

                // Drain mpv events; maintains EOS / looping state.
                video.handle_events();

                // Report end-of-file exactly once.
                if video.take_eos_event() {
                    if !video.looping() {
                        let _ = video.set_paused(true);
                        if let Some(msg) = self.on_end_of_stream.clone() {
                            shell.publish(msg);
                        }
                    }
                }

                // Publish a new-frame notification when one arrived.
                if video.frame_slot().take_signal() {
                    if let Some(msg) = self.on_new_frame.clone() {
                        shell.publish(msg);
                    }
                }

                if !video.paused() && !video.is_eos() {
                    shell.request_redraw();
                } else {
                    // Low-frequency poll for seeks / external changes.
                    shell.request_redraw_at(RedrawRequest::At(
                        std::time::Instant::now() + Duration::from_millis(32),
                    ));
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: cosmic::iced::core::Layout<'_>,
        cursor_position: Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> Interaction {
        let bounds = layout.bounds();
        let is_mouse_over = cursor_position.is_over(bounds);

        if is_mouse_over {
            Interaction::Grab
        } else if self.mouse_hidden {
            Interaction::Idle
        } else {
            Interaction::default()
        }
    }
}

impl<'a, Message, Theme, Renderer> From<VideoPlayer<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + PrimitiveRenderer,
{
    fn from(video_player: VideoPlayer<'a, Message, Theme, Renderer>) -> Self {
        Self::new(video_player)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contain_bounds_fits_frame_into_bounds() {
        let bounds = cosmic::iced::core::Rectangle::new(
            cosmic::iced::core::Point::new(0.0, 0.0),
            cosmic::iced::core::Size::new(800.0, 450.0),
        );
        // 16:9 frame in a 16:9 area fills it.
        let b = contain_bounds(bounds, (1920, 1080));
        assert_eq!(b.width, 800.0);
        assert_eq!(b.height, 450.0);
        // 4:3 frame in a 16:9 area: limited vertically (480 -> 450),
        // width scales to 600 and is centered.
        let b = contain_bounds(bounds, (640, 480));
        assert!((b.height - 450.0).abs() < 1e-3);
        assert!((b.width - 600.0).abs() < 1e-3);
        assert!((b.x - 100.0).abs() < 1e-3);
        // Portrait frame in a wide area: limited horizontally.
        let b = contain_bounds(bounds, (480, 640));
        assert!((b.width - 337.5).abs() < 1e-2);
        assert!((b.height - 450.0).abs() < 1e-3);
        // Zero-sized frame falls back to bounds.
        assert_eq!(contain_bounds(bounds, (0, 0)), bounds);
    }
}
