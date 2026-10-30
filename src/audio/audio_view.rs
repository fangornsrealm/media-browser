// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use cosmic::{
    //cosmic_config::{self},
    cosmic_theme,
    iced::keyboard::{Key, Modifiers},
    //widget::menu::action::MenuAction,
};

use std::time::{Duration, Instant};

use crate::config::Config;

static CONTROLS_TIMEOUT: Duration = Duration::new(2, 0);

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
        if let Some(audio) = self.audio_opt.take() {
            log::info!("pausing audio");
            let _ = audio.set_paused(true);
            log::info!("dropping audio");
            drop(audio);
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
        let audio = match crate::audio::audio::Audio::new(&audiopath, self.posterpath_opt.clone())
        {
            Ok(ok) => ok,
            Err(error) => {
                log::error!("Failed to open audio file {}: {}", audiopath, error);
                return;
            }
        };

        self.duration = audio.duration().as_secs_f64();
        self.audio_opt = Some(audio);

        // Populate the track lists from mpv's `track-list`.
        let Some(audio) = self.audio_opt.as_ref() else {
            return;
        };
        if let Ok(tracks) = audio.tracks() {
            let mut idx = 0usize;
            self.audio_codes = tracks
                .iter()
                .filter(|t| t.kind == crate::mpv::TrackKind::Audio)
                .map(|t| {
                    let name = t.display_name("Audio", idx);
                    idx += 1;
                    name
                })
                .collect();
            self.current_audio = audio.current_audio();
        }
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

                if let Some(audio) = &self.audio_opt {
                    if let Err(e) = audio.set_paused(!audio.paused()) {
                        log::error!("failed to toggle pause: {e}");
                    }
                    self.update_controls(true);
                }
            }
            Message::AudioToggle => {
                if let Some(audio) = &self.audio_opt {
                    audio.toggle_muted();
                    self.update_controls(true);
                }
            }
            Message::AudioVolume(val) => {
                if let Some(audio) = &self.audio_opt {
                    if let Err(e) = audio.set_volume(val) {
                        log::error!("failed to set volume: {e}");
                    }
                }
            }
            Message::AudioCode(val) => {
                if let Some(audio) = &self.audio_opt {
                    if let Err(e) = audio.select_audio(val) {
                        log::error!("failed to select audio track {val}: {e}");
                    } else {
                        self.current_audio = val as i32;
                        self.dropdown_opt = None;
                    }
                }
            }
            Message::Seek(secs) => {
                //TODO: cleanest way to close dropdowns
                self.dropdown_opt = None;

                if let Some(audio) = &self.audio_opt {
                    self.dragging = true;
                    self.position = secs;
                    let _ = audio.set_paused(true);
                    let duration = Duration::try_from_secs_f64(self.position).unwrap_or_default();
                    if let Err(e) = audio.seek(duration, true) {
                        log::error!("seek failed: {e}");
                    }
                    let _ = audio.set_paused(false);
                    self.dragging = false;
                }
                self.update_controls(true);
            }
            Message::SeekRelative(secs) => {
                if let Some(audio) = &self.audio_opt {
                    self.position = audio.position().as_secs_f64();
                    let duration =
                        Duration::try_from_secs_f64(self.position + secs).unwrap_or_default();
                    if let Err(e) = audio.seek(duration, true) {
                        log::error!("seek failed: {e}");
                    }
                }
                self.update_controls(true);
            }
            Message::SeekRelease => {
                //TODO: cleanest way to close dropdowns
                self.dropdown_opt = None;

                if let Some(audio) = &self.audio_opt {
                    self.dragging = false;
                    let duration = Duration::try_from_secs_f64(self.position).unwrap_or_default();
                    if let Err(e) = audio.seek(duration, true) {
                        log::error!("seek failed: {e}");
                    }
                    let _ = audio.set_paused(false);
                    self.update_controls(true);
                }
            }
            Message::EndOfStream => {
                println!("end of stream");
            }
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
