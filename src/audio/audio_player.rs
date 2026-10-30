//! Audio player widget (iced 0.14 / libcosmic 1.0).
//!
//! Displays cover art for an [`Audio`](crate::audio::audio::Audio) by
//! submitting a static [`MediaPrimitive`] to the wgpu renderer. While
//! playback is active the widget publishes `on_new_frame` at a throttled
//! rate so the position UI keeps moving (audio has no video frames).

use crate::pipeline::MediaPrimitive;
use crate::audio::audio::Audio;
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
use std::time::{Duration, Instant};

/// How often `on_new_frame` is published while playing.
const NEW_FRAME_PERIOD: Duration = Duration::from_millis(200);

/// Audio player widget which displays the cover art of an [`Audio`].
pub struct AudioPlayer<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: PrimitiveRenderer,
{
    audio: &'a Audio,
    content_fit: iced::ContentFit,
    width: iced::Length,
    height: iced::Length,
    mouse_hidden: bool,
    on_end_of_stream: Option<Message>,
    on_new_frame: Option<Message>,
    /// Last time `on_new_frame` was published.
    last_new_frame: Option<Instant>,
    _phantom: PhantomData<(Theme, Renderer)>,
}

impl<'a, Message, Theme, Renderer> AudioPlayer<'a, Message, Theme, Renderer>
where
    Renderer: PrimitiveRenderer,
{
    /// Creates a new audio player widget for a given audio file.
    pub fn new(audio: &'a Audio) -> Self {
        AudioPlayer {
            audio,
            content_fit: iced::ContentFit::Contain,
            width: iced::Length::Shrink,
            height: iced::Length::Shrink,
            mouse_hidden: false,
            on_end_of_stream: None,
            on_new_frame: None,
            last_new_frame: None,
            _phantom: PhantomData::default(),
        }
    }

    /// Sets the width of the `AudioPlayer` boundaries.
    pub fn width(self, width: impl Into<iced::Length>) -> Self {
        AudioPlayer {
            width: width.into(),
            ..self
        }
    }

    /// Sets the height of the `AudioPlayer` boundaries.
    pub fn height(self, height: impl Into<iced::Length>) -> Self {
        AudioPlayer {
            height: height.into(),
            ..self
        }
    }

    /// Sets the `ContentFit` of the `AudioPlayer`.
    pub fn content_fit(self, content_fit: iced::ContentFit) -> Self {
        AudioPlayer {
            content_fit,
            ..self
        }
    }

    /// Whether the cursor should be hidden while hovering.
    pub fn mouse_hidden(self, mouse_hidden: bool) -> Self {
        AudioPlayer {
            mouse_hidden,
            ..self
        }
    }

    /// Message to send when the audio reaches the end of stream.
    pub fn on_end_of_stream(self, on_end_of_stream: Message) -> Self {
        AudioPlayer {
            on_end_of_stream: Some(on_end_of_stream),
            ..self
        }
    }

    /// Message to send periodically while playing (position updates).
    pub fn on_new_frame(self, on_new_frame: Message) -> Self {
        AudioPlayer {
            on_new_frame: Some(on_new_frame),
            ..self
        }
    }
}

impl<'a, Message, Theme, Renderer> widget::Widget<Message, Theme, Renderer>
    for AudioPlayer<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + PrimitiveRenderer,
{
    fn size(&self) -> Size<iced::Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, _tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
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
        let cover = self.audio.cover();
        // Fit the (typically square) cover art into the widget bounds.
        let size = self.content_fit.fit(
            Size::new(cover.width as f32, cover.height as f32),
            bounds.size(),
        );
        let drawing_bounds = Rectangle {
            x: bounds.x + (bounds.width - size.width) / 2.0,
            y: bounds.y + (bounds.height - size.height) / 2.0,
            width: size.width,
            height: size.height,
        };

        renderer.draw_primitive(
            drawing_bounds,
            MediaPrimitive {
                id: self.audio.id(),
                alive: self.audio.alive(),
                frame: std::sync::Arc::clone(&cover.bytes),
                width: cover.width,
                height: cover.height,
                stride: cover.stride,
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
                let audio = self.audio;

                // Drain mpv events; maintains EOS / looping state.
                audio.handle_events();

                // Report end-of-file exactly once.
                if audio.take_eos_event() {
                    if !audio.looping() {
                        let _ = audio.set_paused(true);
                        if let Some(msg) = self.on_end_of_stream.clone() {
                            shell.publish(msg);
                        }
                    }
                }

                // Periodic position updates while playing.
                if !audio.paused() && !audio.eos() {
                    let now = Instant::now();
                    if self
                        .last_new_frame
                        .map(|t| now.duration_since(t) >= NEW_FRAME_PERIOD)
                        .unwrap_or(true)
                    {
                        self.last_new_frame = Some(now);
                        if let Some(msg) = self.on_new_frame.clone() {
                            shell.publish(msg);
                        }
                    }
                    shell.request_redraw_at(RedrawRequest::At(
                        now + NEW_FRAME_PERIOD,
                    ));
                } else {
                    shell.request_redraw_at(RedrawRequest::At(
                        Instant::now() + Duration::from_millis(100),
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

impl<'a, Message, Theme, Renderer> From<AudioPlayer<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + PrimitiveRenderer,
{
    fn from(audio_player: AudioPlayer<'a, Message, Theme, Renderer>) -> Self {
        Self::new(audio_player)
    }
}
