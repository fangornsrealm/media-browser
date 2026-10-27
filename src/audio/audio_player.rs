// Copyright [iced-video-player]()
// SPDX-License-Identifier: MIT
// [MIT](http://opensource.org/licenses/MIT)
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use super::{audio::Audio, pipeline::VideoPrimitive};
use cosmic::iced::{
    advanced::{self, layout, widget, Widget},
    Element,
};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use std::marker::PhantomData;
use std::{sync::Arc, time::Instant};

pub use gstreamer as gst;
pub use gstreamer_pbutils as gst_pbutils;
//use gstreamer::prelude::*;

/// Video player widget which displays the current frame of a [`Video`](crate::Video).
pub struct AudioPlayer<'a, Message, Theme = cosmic::iced::Theme, Renderer = cosmic::iced::Renderer>
where
    Renderer: PrimitiveRenderer,
{
    audio: &'a Audio,
    content_fit: cosmic::iced::ContentFit,
    width: cosmic::iced::Length,
    height: cosmic::iced::Length,
    mouse_hidden: bool,
    on_end_of_stream: Option<Message>,
    _on_new_frame: Option<Message>,
    _on_subtitle_text: Option<Box<dyn Fn(Option<String>) -> Message + 'a>>,
    on_error: Option<Box<dyn Fn(gst::glib::Error) -> Message + 'a>>,
    on_missing_plugin: Option<Box<dyn Fn(gst::Message) -> Message + 'a>>,
    on_warning: Option<Box<dyn Fn(gst::glib::Error) -> Message + 'a>>,
    _phantom: PhantomData<(Theme, Renderer)>,
}

impl<'a, Message, Theme, Renderer> AudioPlayer<'a, Message, Theme, Renderer>
where
    Renderer: PrimitiveRenderer,
{
    /// Creates a new video player widget for a given video.
    pub fn new(audio: &'a Audio) -> Self {
        AudioPlayer {
            audio,
            content_fit: cosmic::iced::ContentFit::Contain,
            width: cosmic::iced::Length::Shrink,
            height: cosmic::iced::Length::Shrink,
            mouse_hidden: false,
            on_end_of_stream: None,
            _on_new_frame: None,
            _on_subtitle_text: None,
            on_error: None,
            on_missing_plugin: None,
            on_warning: None,
            _phantom: Default::default(),
        }
    }

    /// Sets the width of the `AudioPlayer` boundaries.
    pub fn width(self, width: impl Into<cosmic::iced::Length>) -> Self {
        AudioPlayer {
            width: width.into(),
            ..self
        }
    }

    /// Sets the height of the `AudioPlayer` boundaries.
    pub fn height(self, height: impl Into<cosmic::iced::Length>) -> Self {
        AudioPlayer {
            height: height.into(),
            ..self
        }
    }

    /// Sets the `ContentFit` of the `AudioPlayer`.
    pub fn content_fit(self, content_fit: cosmic::iced::ContentFit) -> Self {
        AudioPlayer {
            content_fit,
            ..self
        }
    }

    pub fn mouse_hidden(self, mouse_hidden: bool) -> Self {
        AudioPlayer {
            mouse_hidden,
            ..self
        }
    }

    /// Message to send when the video reaches the end of stream (i.e., the video ends).
    pub fn on_end_of_stream(self, on_end_of_stream: Message) -> Self {
        AudioPlayer {
            on_end_of_stream: Some(on_end_of_stream),
            ..self
        }
    }

    /// Message to send when the video receives a new frame.
    pub fn on_new_frame(self, on_new_frame: Message) -> Self {
        AudioPlayer {
            _on_new_frame: Some(on_new_frame),
            ..self
        }
    }

    /// Message to send when the subtitle receives a new frame.
    pub fn on_subtitle_text<F>(self, on_subtitle_text: F) -> Self
    where
        F: 'a + Fn(Option<String>) -> Message,
    {
        AudioPlayer {
            _on_subtitle_text: Some(Box::new(on_subtitle_text)),
            ..self
        }
    }

    /// Message to send when the video playback encounters an error.
    pub fn on_error<F>(self, on_error: F) -> Self
    where
        F: 'a + Fn(gst::glib::Error) -> Message,
    {
        AudioPlayer {
            on_error: Some(Box::new(on_error)),
            ..self
        }
    }

    pub fn on_missing_plugin<F>(self, on_missing_plugin: F) -> Self
    where
        F: 'a + Fn(gst::Message) -> Message,
    {
        AudioPlayer {
            on_missing_plugin: Some(Box::new(on_missing_plugin)),
            ..self
        }
    }

    pub fn on_warning<F>(self, on_warning: F) -> Self
    where
        F: 'a + Fn(gst::glib::Error) -> Message,
    {
        AudioPlayer {
            on_warning: Some(Box::new(on_warning)),
            ..self
        }
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for AudioPlayer<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: PrimitiveRenderer,
{
    fn size(&self) -> cosmic::iced::Size<cosmic::iced::Length> {
        cosmic::iced::Size {
            width: cosmic::iced::Length::Shrink,
            height: cosmic::iced::Length::Shrink,
        }
    }

    fn layout(
        &self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // based on `Image::layout`
        let (width, height) = self.audio.size();
        let image_size = cosmic::iced::Size::new(width as f32, height as f32);
        let raw_size = limits.resolve(self.width, self.height, image_size);
        let full_size = self.content_fit.fit(image_size, raw_size);
        let final_size = cosmic::iced::Size {
            width: match self.width {
                cosmic::iced::Length::Shrink => f32::min(raw_size.width, full_size.width),
                _ => raw_size.width,
            },
            height: match self.height {
                cosmic::iced::Length::Shrink => f32::min(raw_size.height, full_size.height),
                _ => raw_size.height,
            },
        };

        layout::Node::new(final_size)
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &advanced::renderer::Style,
        layout: advanced::Layout<'_>,
        _cursor: advanced::mouse::Cursor,
        _viewport: &cosmic::iced::Rectangle,
    ) {
        let inner = self.audio.read();

        // bounds based on `Image::draw`
        let image_size = cosmic::iced::Size::new(inner.width as f32, inner.height as f32);
        let bounds = layout.bounds();
        let adjusted_fit = self.content_fit.fit(image_size, bounds.size());
        let scale = cosmic::iced::Vector::new(
            adjusted_fit.width / image_size.width,
            adjusted_fit.height / image_size.height,
        );
        let final_size =
            cosmic::iced::Size::new(image_size.width * scale.x, image_size.height * scale.y);

        let position = match self.content_fit {
            cosmic::iced::ContentFit::None => cosmic::iced::Point::new(
                bounds.x + (image_size.width - adjusted_fit.width) / 2.0,
                bounds.y + (image_size.height - adjusted_fit.height) / 2.0,
            ),
            _ => cosmic::iced::Point::new(
                bounds.center_x() - final_size.width / 2.0,
                bounds.center_y() - final_size.height / 2.0,
            ),
        };

        let drawing_bounds = cosmic::iced::Rectangle::new(position, final_size);

        renderer.draw_primitive(
            drawing_bounds,
            VideoPrimitive::new(
                inner.id,
                Arc::clone(&inner.alive),
                Arc::clone(&inner.frame),
                (inner.width as _, inner.height as _),
                true,
            ),
        );
    }

    fn on_event(
        &mut self,
        _state: &mut cosmic::iced::core::widget::Tree,
        event: cosmic::iced::Event,
        _layout: advanced::Layout<'_>,
        _cursor: advanced::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn advanced::Clipboard,
        shell: &mut advanced::Shell<'_, Message>,
        _viewport: &cosmic::iced::Rectangle,
    ) -> cosmic::iced::event::Status {
        let mut inner = self.audio.write();

        if let cosmic::iced::Event::Window(cosmic::iced::window::Event::RedrawRequested(_)) = event
        {
            if inner.restart_stream || (!inner.is_eos && !inner.paused()) {
                let mut restart_stream = false;
                if inner.restart_stream {
                    restart_stream = true;
                    // Set flag to false to avoid potentially multiple seeks
                    inner.restart_stream = false;
                }
                let mut eos_pause = false;

                while let Some(msg) = inner
                    .bus
                    .pop_filtered(&[gst::MessageType::Error, gst::MessageType::Eos])
                {
                    match msg.view() {
                        gst::MessageView::Error(err) => {
                            log::error!("bus returned an error: {err}");
                            if let Some(ref on_error) = self.on_error {
                                shell.publish(on_error(err.error()))
                            };
                        }
                        gst::MessageView::Element(element) => {
                            if gst_pbutils::MissingPluginMessage::is(&element) {
                                if let Some(ref on_missing_plugin) = self.on_missing_plugin {
                                    shell.publish(on_missing_plugin(element.copy()));
                                }
                            }
                        }
                        gst::MessageView::Eos(_eos) => {
                            if let Some(on_end_of_stream) = self.on_end_of_stream.clone() {
                                shell.publish(on_end_of_stream);
                            }
                            if inner.looping {
                                restart_stream = true;
                            } else {
                                eos_pause = true;
                            }
                        }
                        gst::MessageView::Warning(warn) => {
                            log::warn!("bus returned a warning: {warn}");
                            if let Some(ref on_warning) = self.on_warning {
                                shell.publish(on_warning(warn.error()));
                            }
                        }
                        _ => {}
                    }
                }

                // Don't run eos_pause if restart_stream is true; fixes "pausing" after restarting a stream
                if restart_stream {
                    if let Err(err) = inner.restart_stream() {
                        log::error!("cannot restart stream (can't seek): {err:#?}");
                    }
                } else if eos_pause {
                    inner.is_eos = true;
                    inner.set_paused(true);
                }
                shell.request_redraw(cosmic::iced::window::RedrawRequest::NextFrame);
            } else {
                shell.request_redraw(cosmic::iced::window::RedrawRequest::At(
                    Instant::now() + core::time::Duration::from_millis(32),
                ));
            }
            cosmic::iced::event::Status::Captured
        } else {
            cosmic::iced::event::Status::Ignored
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &cosmic::iced::core::widget::Tree,
        _layout: advanced::Layout<'_>,
        _cursor_position: cosmic::iced::mouse::Cursor,
        _viewport: &cosmic::iced::Rectangle,
        _renderer: &Renderer,
    ) -> cosmic::iced::mouse::Interaction {
        if self.mouse_hidden {
            cosmic::iced::mouse::Interaction::Idle
        } else {
            cosmic::iced::mouse::Interaction::default()
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
    fn from(video_player: AudioPlayer<'a, Message, Theme, Renderer>) -> Self {
        Self::new(video_player)
    }
}
