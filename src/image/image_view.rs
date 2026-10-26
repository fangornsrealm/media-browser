// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use cosmic::iced::Length;

#[derive(Debug, Clone)]
pub enum Message {
    ToBrowser,
    ToImage,
    ToVideo,
    ToAudio,
    Open(String),
    NextFile,
    PreviousFile,
    ZoomPlus,
    ZoomMinus,
    ZoomFit,
    Seek,
}

//single static page, is not supposed to do anything other than display a button that lets you move to the next page
pub struct ImageView {
    pub handle_opt: Option<super::image::Handle>,
    pub controls: bool,
    pub _controls_time: std::time::Instant,
    pub _fullscreen: bool,
    pub image_path: String,
    pub image_path_loaded: String,
    pub width: Length,
    pub height: Length,
    pub min_scale: f32,
    pub max_scale: f32,
    pub scale_step: f32,
}

impl ImageView {
    pub fn new() -> Self {
        ImageView {
            handle_opt: None,
            controls: true,
            _controls_time: std::time::Instant::now(),
            _fullscreen: false,
            image_path: "./examples/logo.png".to_string(),
            image_path_loaded: String::new(),
            width: Length::Fixed(4096.0),
            height: Length::Fixed(4096.0),
            min_scale: 0.1,
            max_scale: 10.0,
            scale_step: 0.2,
        }
    }
    //I don't know when this is called..'
    pub fn update(&mut self, message: Message) {
        match message {
            Message::ToBrowser => {}
            Message::ToImage => {}
            Message::ToVideo => {}
            Message::ToAudio => {}
            Message::Open(imagepath) => {
                self.image_path = imagepath.clone();
                self.handle_opt = Some(super::image::Handle::from_path(self.image_path.clone()));
                self.image_path_loaded = self.image_path.clone();
            }
            Message::NextFile => {}
            Message::PreviousFile => {}
            Message::ZoomPlus => {}
            Message::ZoomMinus => {}
            Message::ZoomFit => {}
            Message::Seek => {}
        }
    }
}
