// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use cosmic::{
    cosmic_config::{self},
    cosmic_theme,
    iced::keyboard::{Key, Modifiers},
    //iced::Subscription,
    //widget::menu::action::MenuAction,
};
pub use gstreamer as gst;
//pub use gstreamer_app as gst_app;
use gstreamer::prelude::*;

//use iced_video_player::{
//    gst::{self, prelude::*},
//    gst_app, gst_pbutils, Video, VideoPlayer,
//};
use std::{
    ffi::{CStr, CString},
    time::{Duration, Instant},
};

use crate::config::Config;

static CONTROLS_TIMEOUT: Duration = Duration::new(2, 0);

const GST_PLAY_FLAG_VIDEO: i32 = 1 << 0;
const GST_PLAY_FLAG_AUDIO: i32 = 1 << 1;
const GST_PLAY_FLAG_TEXT: i32 = 1 << 2;

fn language_name(code: &str) -> Option<String> {
    let code_c = CString::new(code).ok()?;
    let name_c = unsafe {
        //TODO: export this in gstreamer_tag
        let name_ptr = gstreamer_tag::ffi::gst_tag_get_language_name(code_c.as_ptr());
        if name_ptr.is_null() {
            return None;
        }
        CStr::from_ptr(name_ptr)
    };
    let name = name_c.to_str().ok()?;
    Some(name.to_string())
}

/*
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    _FileClose,
    _FileOpen,
    _Fullscreen,
    _PlayPause,
    _SeekBackward,
    _SeekForward,
    _WindowClose,
}

impl MenuAction for Action {
    type Message = Message;

    fn message(&self) -> Message {
        match self {
            Self::_FileClose => Message::FileClose,
            Self::_FileOpen => Message::FileOpen,
            Self::_Fullscreen => Message::Fullscreen,
            Self::_PlayPause => Message::PlayPause,
            Self::_SeekBackward => Message::SeekRelative(-10.0),
            Self::_SeekForward => Message::SeekRelative(10.0),
            Self::_WindowClose => Message::WindowClose,
        }
    }
}
*/

#[derive(Clone)]
pub struct _Flags {
    config_handler: Option<cosmic_config::Config>,
    config: Config,
    url_opt: Option<url::Url>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropdownKind {
    Audio,
    Subtitle,
    Browser,
    Chapter,
}

/// Messages that are used specifically by our [`App`].
#[derive(Clone, Debug)]
pub enum Message {
    None,
    ToBrowser,
    ToImage,
    Toaudio,
    ToAudio,
    NextFile,
    PreviousFile,
    Open(String),
    Config(Config),
    DropdownToggle(DropdownKind),
    FileClose,
    FileLoad(url::Url),
    FileOpen,
    Fullscreen,
    Key(Modifiers, Key),
    AudioCode(usize),
    AudioToggle,
    AudioVolume(f64),
    TextCode(usize),
    PlayPause,
    Seek(f64),
    SeekRelative(f64),
    SeekRelease,
    EndOfStream,
    MissingPlugin,
    NewFrame,
    Reload,
    ShowControls,
    SystemThemeModeChange(cosmic_theme::ThemeMode),
    WindowClose,
}

/// The [`App`] stores application-specific state.
pub struct AudioView {
    pub audiopath_opt: Option<url::Url>,
    pub posterpath_opt: Option<String>,
    pub controls: bool,
    pub controls_time: Instant,
    pub dropdown_opt: Option<DropdownKind>,
    pub fullscreen: bool,
    pub audio_opt: Option<super::audio::Audio>,
    pub position: f64,
    pub duration: f64,
    pub dragging: bool,
    pub audio_codes: Vec<String>,
    pub current_audio: i32,
    pub text_codes: Vec<String>,
    pub current_text: i32,
    pub chapters: Vec<crate::sql::Chapter>,
    pub chapters_str: Vec<String>,
    pub current_chapter: usize,
}

impl AudioView {
    /// Creates the application, and optionally emits command on initialize.
    pub fn new() -> Self {
        let audio_view = AudioView {
            audiopath_opt: None,
            posterpath_opt: None,
            controls: true,
            controls_time: Instant::now(),
            dropdown_opt: None,
            fullscreen: false,
            audio_opt: None,
            position: 0.0,
            duration: 0.0,
            dragging: false,
            audio_codes: Vec::new(),
            current_audio: -1,
            text_codes: Vec::new(),
            current_text: -1,
            chapters: Vec::new(),
            chapters_str: Vec::new(),
            current_chapter: 0,
        };
        audio_view
    }

    pub fn close(&mut self) {
        //TODO: drop does not work well
        if let Some(mut audio) = self.audio_opt.take() {
            log::info!("pausing audio");
            audio.set_paused(true);
            log::info!("dropping audio");
            drop(audio);
            log::info!("dropped audio");
        }
        self.position = 0.0;
        self.duration = 0.0;
        self.dragging = false;
        self.audio_codes = Vec::new();
        self.current_audio = -1;
        self.text_codes = Vec::new();
        self.current_text = -1;
    }

    pub fn load(&mut self) {
        let audiopath;
        if let Some(audiopathstr) = &self.audiopath_opt {
            audiopath = audiopathstr.to_string();
        } else {
            return;
        }
        self.close();
        log::info!("Loading {}", audiopath);
        //TODO: this code came from iced_video_player::audio::new and has been modified to stop the pipeline on error
        //TODO: remove unwraps and enable playback of files with only audio.
        //if let Ok(audio_url) = url::Url::from_file_path(std::path::PathBuf::from(&audiopath)) {
        let audio = match crate::audio::audio::Audio::new(&audiopath, self.posterpath_opt.clone()) {
            Ok(ok) => ok,
            Err(error) => {
                log::error!("Failed to open audio file {}: {}", audiopath, error);
                return;
            }
        };

        self.duration = audio.duration().as_secs_f64();
        let pipeline = audio.pipeline();
        self.audio_opt = Some(audio);

        let n_audio = pipeline.property::<i32>("n-audio");
        self.audio_codes = Vec::with_capacity(n_audio as usize);
        for i in 0..n_audio {
            let tags: gst::TagList = pipeline.emit_by_name("get-audio-tags", &[&i]);
            log::info!("audio stream {i}: {tags:?}");
            self.audio_codes
                .push(if let Some(title) = tags.get::<gst::tags::Title>() {
                    title.get().to_string()
                } else if let Some(language_code) = tags.get::<gst::tags::LanguageCode>() {
                    let language_code = language_code.get();
                    language_name(language_code).unwrap_or_else(|| language_code.to_string())
                } else {
                    format!("Audio #{i}")
                });
        }
        self.current_audio = pipeline.property::<i32>("current-audio");

        let n_text = pipeline.property::<i32>("n-text");
        self.text_codes = Vec::with_capacity(n_text as usize);
        for i in 0..n_text {
            let tags: gst::TagList = pipeline.emit_by_name("get-text-tags", &[&i]);
            log::info!("text stream {i}: {tags:?}");
            self.text_codes
                .push(if let Some(title) = tags.get::<gst::tags::Title>() {
                    title.get().to_string()
                } else if let Some(language_code) = tags.get::<gst::tags::LanguageCode>() {
                    let language_code = language_code.get();
                    language_name(language_code).unwrap_or_else(|| language_code.to_string())
                } else {
                    format!("Subtitle #{i}")
                });
        }

        self.current_text = pipeline.property::<i32>("current-text");

        //TODO: Flags can be used to enable/disable subtitles
        let flags_value = pipeline.property_value("flags");
        println!("original flags {:?}", flags_value);
        match flags_value.transform::<i32>() {
            Ok(flags_transform) => match flags_transform.get::<i32>() {
                Ok(mut flags) => {
                    flags |= GST_PLAY_FLAG_VIDEO | GST_PLAY_FLAG_AUDIO | GST_PLAY_FLAG_TEXT;
                    match gst::glib::Value::from(flags).transform_with_type(flags_value.type_()) {
                        Ok(value) => pipeline.set_property("flags", value),
                        Err(err) => {
                            log::warn!("failed to transform int to flags: {err}");
                        }
                    }
                }
                Err(err) => {
                    log::warn!("failed to get flags as int: {err}");
                }
            },
            Err(err) => {
                log::warn!("failed to transform flags to int: {err}");
            }
        }
        println!("updated flags {:?}", pipeline.property_value("flags"));
    }

    pub fn update_controls(&mut self, in_use: bool) {
        if in_use {
            self.controls = true;
            self.controls_time = Instant::now();
        } else if self.controls && self.controls_time.elapsed() > CONTROLS_TIMEOUT {
            self.controls = false;
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::None => {}
            Message::ToBrowser => {}
            Message::ToImage => {}
            Message::ToAudio => {}
            Message::Toaudio => {}
            Message::Open(path) => {
                let pathbuf = std::path::PathBuf::from(&path);
                if let Ok(url) = url::Url::from_file_path(pathbuf) {
                    self.audiopath_opt = Some(url);
                    let mut p = path.clone();
                    p.push_str(".png");
                    let poster = std::path::PathBuf::from(&p);
                    if poster.is_file() {
                        self.posterpath_opt = Some(p);
                    } else {
                        self.posterpath_opt = None;
                    }
                    self.load();
                }
            }
            Message::PlayPause => {
                //TODO: cleanest way to close dropdowns
                self.dropdown_opt = None;

                if let Some(audio) = &mut self.audio_opt {
                    audio.set_paused(!audio.paused());
                    self.update_controls(true);
                }
            }
            Message::Seek(secs) => {
                //TODO: cleanest way to close dropdowns
                self.dropdown_opt = None;

                if let Some(audio) = &mut self.audio_opt {
                    self.dragging = true;
                    self.position = secs;
                    audio.set_paused(true);
                    let duration = Duration::try_from_secs_f64(self.position).unwrap_or_default();
                    audio.seek(duration, true).expect("seek");
                    audio.set_paused(false);
                    self.dragging = false;
                }
                self.update_controls(true);
            }
            Message::SeekRelative(secs) => {
                if let Some(audio) = &mut self.audio_opt {
                    self.position = audio.position().as_secs_f64();
                    let duration =
                        Duration::try_from_secs_f64(self.position + secs).unwrap_or_default();
                    audio.seek(duration, true).expect("seek");
                }
            }
            Message::SeekRelease => {
                //TODO: cleanest way to close dropdowns
                self.dropdown_opt = None;

                if let Some(audio) = &mut self.audio_opt {
                    self.dragging = false;
                    let duration = Duration::try_from_secs_f64(self.position).unwrap_or_default();
                    audio.seek(duration, true).expect("seek");
                    audio.set_paused(false);
                    self.update_controls(true);
                }
            }
            Message::EndOfStream => {
                println!("end of stream");
            }
            Message::MissingPlugin => {}
            Message::NewFrame => {
                if let Some(audio) = &self.audio_opt {
                    if !self.dragging {
                        self.position = audio.position().as_secs_f64();
                        self.update_controls(self.dropdown_opt.is_some());
                    }
                }
            }
            Message::Reload => {
                self.load();
            }
            _ => {}
        }
    }
}
