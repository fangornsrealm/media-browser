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
use crate::video::video::Video;

use std::{
    time::{Duration, Instant},
};

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
    ToVideo,
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
pub struct VideoView {
    pub videopath_opt: Option<url::Url>,
    pub controls: bool,
    pub controls_time: Instant,
    pub dropdown_opt: Option<DropdownKind>,
    pub fullscreen: bool,
    pub video_opt: Option<Video>,
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

impl VideoView {
    /// Creates the application, and optionally emits command on initialize.
    pub fn new() -> Self {
        let video_view = VideoView {
            videopath_opt: None,
            controls: true,
            controls_time: Instant::now(),
            dropdown_opt: None,
            fullscreen: false,
            video_opt: None,
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
        video_view
    }

    pub fn close(&mut self) {
        //TODO: drop does not work well
        if let Some(mut video) = self.video_opt.take() {
            log::info!("pausing video");
            video.set_paused(true);
            log::info!("dropping video");
            drop(video);
            log::info!("dropped video");
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
        let videopath;
        let mut subtitlepath = None;
        if let Some(videopathstr) = &self.videopath_opt {
            videopath = videopathstr.to_owned();
        } else {
            return;
        }
        self.close();
        log::info!("Loading {}", videopath);
        if let Some(v) = self.video_opt.as_ref() {
            if let Some(subpathstr) = v.subtitle_url() {
                subtitlepath = Some(subpathstr);
            }
        }
        let video = match super::video::Video::new(&videopath, subtitlepath) {
            Ok(ok) => ok,
            Err(error) => {
                log::error!("Failed to open Video file {}: {}", videopath, error);
                return;
            }
        };

        self.duration = video.duration().as_secs_f64();
        self.video_opt = Some(video);

        // Populate the track lists from mpv's `track-list`.
        let Some(video) = self.video_opt.as_ref() else {
            return;
        };
        if let Ok(tracks) = video.tracks() {
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

            let mut idx = 0usize;
            self.text_codes = tracks
                .iter()
                .filter(|t| t.kind == crate::mpv::TrackKind::Sub)
                .map(|t| {
                    let name = t.display_name("Subtitle", idx);
                    idx += 1;
                    name
                })
                .collect();

            self.current_audio = video.current_audio();
            self.current_text = video.current_subtitle();
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
            Message::ToVideo => {}
            Message::Open(path) => {
                match url::Url::from_file_path(std::path::PathBuf::from(&path)) {
                    Ok(url) => {
                        self.videopath_opt = Some(url);
                        self.load();
                    }
                    _ => {}
                }
            }
            Message::PlayPause => {
                //TODO: cleanest way to close dropdowns
                self.dropdown_opt = None;

                if let Some(video) = &self.video_opt {
                    if let Err(e) = video.set_paused(!video.paused()) {
                        log::error!("failed to toggle pause: {e}");
                    }
                    self.update_controls(true);
                }
            }
            Message::AudioToggle => {
                if let Some(video) = &self.video_opt {
                    video.toggle_muted();
                    self.update_controls(true);
                }
            }
            Message::AudioVolume(val) => {
                if let Some(video) = &self.video_opt {
                    if let Err(e) = video.set_volume(val) {
                        log::error!("failed to set volume: {e}");
                    }
                }
            }
            Message::AudioCode(val) => {
                if let Some(video) = &self.video_opt {
                    if let Err(e) = video.select_audio(val) {
                        log::error!("failed to select audio track {val}: {e}");
                    } else {
                        self.current_audio = val as i32;
                        self.dropdown_opt = None;
                    }
                }
            }
            Message::TextCode(val) => {
                if let Some(video) = &self.video_opt {
                    if let Err(e) = video.select_subtitle(val) {
                        log::error!("failed to select subtitle track {val}: {e}");
                    } else {
                        self.current_text = val as i32;
                        self.dropdown_opt = None;
                    }
                }
            }
            Message::Seek(secs) => {
                //TODO: cleanest way to close dropdowns
                self.dropdown_opt = None;

                if let Some(video) = &self.video_opt {
                    self.dragging = true;
                    self.position = secs;
                    let _ = video.set_paused(true);
                    let duration = Duration::try_from_secs_f64(self.position).unwrap_or_default();
                    if let Err(e) = video.seek(duration, true) {
                        log::error!("seek failed: {e}");
                    }
                    let _ = video.set_paused(false);
                    self.dragging = false;
                }
                self.update_controls(true);
            }
            Message::SeekRelative(secs) => {
                if let Some(video) = &self.video_opt {
                    self.position = video.position().as_secs_f64();
                    let duration =
                        Duration::try_from_secs_f64(self.position + secs).unwrap_or_default();
                    if let Err(e) = video.seek(duration, true) {
                        log::error!("seek failed: {e}");
                    }
                }
                self.update_controls(true);
            }
            Message::SeekRelease => {
                //TODO: cleanest way to close dropdowns
                self.dropdown_opt = None;

                if let Some(video) = &self.video_opt {
                    self.dragging = false;
                    let duration = Duration::try_from_secs_f64(self.position).unwrap_or_default();
                    if let Err(e) = video.seek(duration, true) {
                        log::error!("seek failed: {e}");
                    }
                    let _ = video.set_paused(false);
                    self.update_controls(true);
                }
            }
            Message::EndOfStream => {
                println!("end of stream");
            }
            Message::NewFrame => {
                if let Some(video) = &self.video_opt {
                    if !self.dragging {
                        self.position = video.position().as_secs_f64();
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

#[cfg(test)]
mod tests {
    use crate::mpv::{Track, TrackKind};

    fn track(title: Option<&str>, language: Option<&str>) -> Track {
        Track {
            id: 1,
            title: title.map(str::to_string),
            language: language.map(str::to_string),
            kind: TrackKind::Audio,
            selected: true,
        }
    }

    #[test]
    fn display_name_prefers_title_then_language() {
        assert_eq!(
            track(Some("Commentary"), Some("eng")).display_name("Audio", 0),
            "Commentary"
        );
        assert_eq!(track(None, Some("deu")).display_name("Audio", 0), "German");
        assert_eq!(track(None, Some("zz")).display_name("Audio", 2), "zz");
        assert_eq!(track(None, None).display_name("Subtitle", 1), "Subtitle #1");
        // Empty title falls through to language.
        assert_eq!(
            track(Some(""), Some("fra")).display_name("Audio", 0),
            "French"
        );
    }

    #[test]
    fn language_name_maps_common_codes() {
        assert_eq!(crate::mpv::language_name("eng"), Some("English"));
        assert_eq!(crate::mpv::language_name("DE"), Some("German"));
        assert_eq!(crate::mpv::language_name("jpn"), Some("Japanese"));
        assert_eq!(crate::mpv::language_name("xx"), None);
    }
}
