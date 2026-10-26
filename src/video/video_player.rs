// Copyright [iced-video-player]()
// SPDX-License-Identifier: MIT
// [MIT](http://opensource.org/licenses/MIT)
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use crate::video::{gst, gst_pbutils, video::Video};
use cosmic::iced::{
    self,
    advanced::{self, graphics::core::event::Status, layout, widget, Widget},
    mouse, Element,
};
use log::error;
use std::{marker::PhantomData, sync::atomic::Ordering, time::Duration};
use std::{sync::Arc, time::Instant};

use crate::video::pipeline::VideoPrimitive;
use cosmic::iced_wgpu::primitive::Renderer as PrimitiveRenderer;

/// Video player widget which displays the current frame of a [`Video`](crate::Video).
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
    on_subtitle_text: Option<Box<dyn Fn(Option<String>) -> Message + 'a>>,
    on_error: Option<Box<dyn Fn(gst::glib::Error) -> Message + 'a>>,
    on_missing_plugin: Option<Box<dyn Fn(gst::Message) -> Message + 'a>>,
    on_warning: Option<Box<dyn Fn(gst::glib::Error) -> Message + 'a>>,
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
            on_subtitle_text: None,
            on_error: None,
            on_missing_plugin: None,
            on_warning: None,
            _phantom: Default::default(),
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

    pub fn mouse_hidden(self, mouse_hidden: bool) -> Self {
        VideoPlayer {
            mouse_hidden,
            ..self
        }
    }

    /// Message to send when the video reaches the end of stream (i.e., the video ends).
    pub fn on_end_of_stream(self, on_end_of_stream: Message) -> Self {
        VideoPlayer {
            on_end_of_stream: Some(on_end_of_stream),
            ..self
        }
    }

    /// Message to send when the video receives a new frame.
    pub fn on_new_frame(self, on_new_frame: Message) -> Self {
        VideoPlayer {
            on_new_frame: Some(on_new_frame),
            ..self
        }
    }

    /// Message to send when the video receives a new frame.
    pub fn on_subtitle_text<F>(self, on_subtitle_text: F) -> Self
    where
        F: 'a + Fn(Option<String>) -> Message,
    {
        VideoPlayer {
            on_subtitle_text: Some(Box::new(on_subtitle_text)),
            ..self
        }
    }

    /// Message to send when the video playback encounters an error.
    pub fn on_error<F>(self, on_error: F) -> Self
    where
        F: 'a + Fn(gst::glib::Error) -> Message,
    {
        VideoPlayer {
            on_error: Some(Box::new(on_error)),
            ..self
        }
    }

    pub fn on_missing_plugin<F>(self, on_missing_plugin: F) -> Self
    where
        F: 'a + Fn(gst::Message) -> Message,
    {
        VideoPlayer {
            on_missing_plugin: Some(Box::new(on_missing_plugin)),
            ..self
        }
    }

    pub fn on_warning<F>(self, on_warning: F) -> Self
    where
        F: 'a + Fn(gst::glib::Error) -> Message,
    {
        VideoPlayer {
            on_warning: Some(Box::new(on_warning)),
            ..self
        }
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for VideoPlayer<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: PrimitiveRenderer,
{
    fn size(&self) -> iced::Size<iced::Length> {
        iced::Size {
            width: iced::Length::Shrink,
            height: iced::Length::Shrink,
        }
    }

    fn layout(
        &self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let (video_width, video_height) = self.video.size();

        // based on `Image::layout`
        let image_size = iced::Size::new(video_width as f32, video_height as f32);
        let raw_size = limits.resolve(self.width, self.height, image_size);
        let full_size = self.content_fit.fit(image_size, raw_size);
        let final_size = iced::Size {
            width: match self.width {
                iced::Length::Shrink => f32::min(raw_size.width, full_size.width),
                _ => raw_size.width,
            },
            height: match self.height {
                iced::Length::Shrink => f32::min(raw_size.height, full_size.height),
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
        _viewport: &iced::Rectangle,
    ) {
        let mut inner = self.video.write();

        // bounds based on `Image::draw`
        let image_size = iced::Size::new(inner.width as f32, inner.height as f32);
        let bounds = layout.bounds();
        let adjusted_fit = self.content_fit.fit(image_size, bounds.size());
        let scale = iced::Vector::new(
            adjusted_fit.width / image_size.width,
            adjusted_fit.height / image_size.height,
        );
        let final_size = iced::Size::new(image_size.width * scale.x, image_size.height * scale.y);

        let position = match self.content_fit {
            iced::ContentFit::None => iced::Point::new(
                bounds.x + (image_size.width - adjusted_fit.width) / 2.0,
                bounds.y + (image_size.height - adjusted_fit.height) / 2.0,
            ),
            _ => iced::Point::new(
                bounds.center_x() - final_size.width / 2.0,
                bounds.center_y() - final_size.height / 2.0,
            ),
        };

        let drawing_bounds = iced::Rectangle::new(position, final_size);

        let upload_frame = inner.upload_frame.swap(false, Ordering::SeqCst);

        if upload_frame {
            let last_frame_time = inner
                .last_frame_time
                .lock()
                .map(|time| *time)
                .unwrap_or_else(|_| Instant::now());
            inner.set_av_offset(Instant::now() - last_frame_time);
        }

        #[cfg(feature = "wgpu")]
        renderer.draw_primitive(
            drawing_bounds,
            VideoPrimitive::new(
                inner.id,
                Arc::clone(&inner.alive),
                Arc::clone(&inner.frame),
                (inner.width as _, inner.height as _),
                upload_frame,
            ),
        );

        #[cfg(not(feature = "wgpu"))]
        {
            if upload_frame {
                let yuv_data_opt = match inner.frame.lock() {
                    Ok(frame) => Some(frame.clone()),
                    Err(_err) => None,
                };
                inner.handle_opt = if let Some(yuv_data) = yuv_data_opt {
                    //TODO: convert on worker thread?
                    let rgba_data = yuv_to_rgba(&yuv_data, inner.width as _, inner.height as _, 1);
                    Some(advanced::image::Handle::from_pixels(
                        inner.width as _,
                        inner.height as _,
                        rgba_data,
                    ))
                } else {
                    None
                };
            }
            if let Some(handle) = &inner.handle_opt {
                renderer.draw(
                    handle.clone(),
                    advanced::image::FilterMethod::Nearest,
                    drawing_bounds,
                    [0.0; 4],
                );
            }
        }
    }

    fn on_event(
        &mut self,
        _tree: &mut widget::Tree,
        event: iced::Event,
        _layout: advanced::Layout<'_>,
        _cursor: advanced::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn advanced::Clipboard,
        shell: &mut advanced::Shell<'_, Message>,
        _viewport: &iced::Rectangle,
    ) -> Status {
        //let bounds = layout.bounds();

        match event {
            iced::Event::Window(iced::window::Event::RedrawRequested(_)) => {
                let mut inner = self.video.write();
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
                                error!("bus returned an error: {err}");
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
                            error!("cannot restart stream (can't seek): {err:#?}");
                        }
                    } else if eos_pause {
                        inner.is_eos = true;
                        inner.set_paused(true);
                    }

                    if inner.upload_frame.load(Ordering::SeqCst) {
                        if let Some(on_new_frame) = self.on_new_frame.clone() {
                            shell.publish(on_new_frame);
                        }
                    }

                    if let Some(on_subtitle_text) = &self.on_subtitle_text {
                        if inner.upload_text.swap(false, Ordering::SeqCst) {
                            if let Ok(text) = inner.subtitle_text.try_lock() {
                                shell.publish(on_subtitle_text(text.clone()));
                            }
                        }
                    }

                    shell.request_redraw(iced::window::RedrawRequest::NextFrame);
                } else {
                    shell.request_redraw(iced::window::RedrawRequest::At(
                        Instant::now() + Duration::from_millis(32),
                    ));
                }
                Status::Captured
            }
            _ => Status::Ignored,
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &widget::Tree,
        layout: advanced::Layout<'_>,
        cursor_position: mouse::Cursor,
        _viewport: &iced::Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        // let state = tree.state.downcast_ref::<cosmic::iced_core::widget::tree::State>();
        let bounds = layout.bounds();
        let is_mouse_over = cursor_position.is_over(bounds);

        if is_mouse_over {
            mouse::Interaction::Grab
        } else if self.mouse_hidden {
            mouse::Interaction::Idle
        } else {
            mouse::Interaction::default()
        }
        /*
                if self.mouse_hidden {
                    mouse::Interaction::Idle
                } else {
                    mouse::Interaction::default()
                }
        */
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
