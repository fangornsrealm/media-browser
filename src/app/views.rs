//! Message handlers for view switching, media playback/transport controls,
//! and the per-view (image/audio/video) message dispatch.
//!
//! Split out of `app.rs` (Phase 3 — modularization). Each of the original
//! arms of `App::update` is now an individual method on `App`, dispatched
//! from `mod.rs` with a single forwarding line. The set covered here:
//!
//! | Group                          | `Message` arm                                              |
//!|--------------------------------|------------------------------------------------------------|
//! View switching                 | `Browser`, `Image`, `Video`, `Audio`                        |
//! Transport / playback           | `AudioMuteToggle`, `AudioCode`, `AudioVolume`, `TextCode`,  |
//!                                | `Fullscreen`, `Chapter`, `Seek`, `SeekRelative`,            |
//!                                | `EndOfStream`, `NewFrame`, `PlayPause`, `Previous`, `Next`, |
//!                                | `MouseScroll`, `SeekBackward`, `SeekForward`,               |
//!                                | `SkipToPosition`, `MissingPlugin`                           |
//! Per-view message dispatch      | `AudioMessage`, `ImageMessage`, `VideoMessage`              |
//!
//! Note: `update` and `view` are trait methods of `cosmic::Application`, so
//! this module imports that trait to call them. `App`'s private fields are
//! reachable here because this module is a child of `app`.

use cosmic::app::Task;
use cosmic::iced::window;
use cosmic::widget::segmented_button::Entity;
use cosmic::Application;

use super::{App, Message, Mode};
use crate::tab::{self, Tab};

impl App {
    /// Handle `Message::Browser`.
    ///
    /// Pauses any actively playing audio/video and switches the active view
    /// to the browser.
    pub fn browser_view(&mut self) -> Task<Message> {
        if self.active_view == Mode::Audio {
            if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                if !audio.paused() {
                    let _ = audio.set_paused(true);
                }
            }
        }
        if self.active_view == Mode::Video {
            if let Some(video) = self.video_view.video_opt.as_mut() {
                if !video.paused() {
                    let _ = video.set_paused(true);
                }
            }
        }

        self.active_view = Mode::Browser;

        Task::none()
    }

    /// Handle `Message::Image`.
    ///
    /// Routes image-view navigation messages (ToBrowser/ToVideo/ToAudio) to
    /// switch the active view; any other image message is forwarded to the
    /// image view itself.
    pub fn image_msg(&mut self, msg: crate::image::image_view::Message) -> Task<Message> {
        match msg {
            crate::image::image_view::Message::ToBrowser => {
                self.active_view = Mode::Browser;
                self.view();
            }
            crate::image::image_view::Message::ToVideo => {
                self.active_view = Mode::Video;
                self.view();
            }
            crate::image::image_view::Message::ToAudio => {
                self.active_view = Mode::Audio;
                self.view();
            }
            //and here we decide that if it has more messages, we will let it handle them itself.
            _ => {
                //and give it back it's own message
                self.image_view.update(msg);
            }
        }

        Task::none()
    }

    /// Handle `Message::Video`.
    ///
    /// Routes video-view navigation messages (ToBrowser/ToImage/ToAudio) to
    /// switch the active view; any other video message is forwarded to the
    /// video view itself.
    pub fn video_msg(&mut self, msg: crate::video::video_view::Message) -> Task<Message> {
        match msg {
            crate::video::video_view::Message::ToBrowser => {
                self.active_view = Mode::Browser;
                self.view();
            }
            crate::video::video_view::Message::ToImage => {
                self.active_view = Mode::Image;
                self.view();
            }
            crate::video::video_view::Message::ToAudio => {
                self.active_view = Mode::Audio;
                self.view();
            }
            //and here we decide that if it has more messages, we will let it handle them itself.
            _ => {
                //and give it back it's own message
                self.video_view.update(msg);
            }
        }

        Task::none()
    }

    /// Handle `Message::Audio`.
    ///
    /// Routes audio-view navigation messages (ToBrowser/ToImage) to switch
    /// the active view; any other audio message is forwarded to the audio
    /// view itself.
    pub fn audio_msg(&mut self, msg: crate::audio::audio_view::Message) -> Task<Message> {
        match msg {
            crate::audio::audio_view::Message::ToBrowser => {
                self.active_view = Mode::Browser;
                self.view();
            }
            crate::audio::audio_view::Message::ToImage => {
                self.active_view = Mode::Image;
                self.view();
            }
            //and here we decide that if it has more messages, we will let it handle them itself.
            _ => {
                //and give it back it's own message
                self.audio_view.update(msg);
            }
        }

        Task::none()
    }

    /// Handle `Message::AudioMuteToggle`.
    ///
    /// Toggles mute on whichever of the video or audio views is active.
    pub fn audio_mute_toggle(&mut self) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::AudioToggle,
            ));
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::AudioToggle,
            ));
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::AudioCode`.
    ///
    /// Selects the given audio track on the active video/audio view.
    pub fn audio_code(&mut self, val: usize) -> Task<Message> {
        if self.active_view == Mode::Video {
            self.video_view.current_audio = val as i32;
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::AudioCode(val),
            ));
        } else if self.active_view == Mode::Audio {
            self.audio_view.current_audio = val as i32;
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::AudioCode(val),
            ));
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::AudioVolume`.
    ///
    /// Sets the volume on the active video/audio view.
    pub fn audio_volume(&mut self, val: f64) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::AudioVolume(val),
            ));
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::AudioVolume(val),
            ));
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::TextCode`.
    ///
    /// Selects the given text (subtitle) track on the active video/audio view.
    pub fn text_code(&mut self, val: usize) -> Task<Message> {
        if self.active_view == Mode::Video {
            self.video_view.current_text = val as i32;
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::TextCode(val),
            ));
        } else if self.active_view == Mode::Audio {
            self.audio_view.current_text = val as i32;
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::TextCode(val),
            ));
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::Fullscreen`.
    ///
    /// Toggles fullscreen on the active video/audio view and switches the
    /// window mode accordingly (also hiding the header bar when fullscreen).
    pub fn fullscreen(&mut self) -> Task<Message> {
        if self.active_view == Mode::Video {
            self.video_view.dropdown_opt = None;
            self.video_view.fullscreen = !self.video_view.fullscreen;
            self.core.window.show_headerbar = !self.video_view.fullscreen;
            return window::set_mode(
                window::Id::RESERVED,
                if self.video_view.fullscreen {
                    window::Mode::Fullscreen
                } else {
                    window::Mode::Windowed
                },
            );
        } else if self.active_view == Mode::Audio {
            self.audio_view.dropdown_opt = None;

            self.audio_view.fullscreen = !self.audio_view.fullscreen;
            self.core.window.show_headerbar = !self.audio_view.fullscreen;
            return window::set_mode(
                window::Id::RESERVED,
                if self.audio_view.fullscreen {
                    window::Mode::Fullscreen
                } else {
                    window::Mode::Windowed
                },
            );
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::Chapter`.
    ///
    /// Seeks to the start of the given chapter on the active video/audio view
    /// and unpauses playback.
    pub fn chapter(&mut self, val: usize) -> Task<Message> {
        if self.active_view == Mode::Video {
            self.video_view.current_chapter = val;
            let new_pos = self.video_view.chapters[val].start as f64;
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::Seek(new_pos),
            ));
            if let Some(video) = self.video_view.video_opt.as_mut() {
                if video.paused() {
                    let _ = video.set_paused(false);
                }
            }
        } else if self.active_view == Mode::Audio {
            self.audio_view.current_chapter = val;
            let new_pos = self.audio_view.chapters[val].start as f64;
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::Seek(new_pos),
            ));
            if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                if audio.paused() {
                    let _ = audio.set_paused(false);
                }
            }
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::Seek`.
    ///
    /// Seeks to the given position on the active video/audio view and
    /// unpauses playback.
    pub fn seek(&mut self, val: f64) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::Seek(val),
            ));
            if let Some(video) = self.video_view.video_opt.as_mut() {
                if video.paused() {
                    let _ = video.set_paused(false);
                }
            }
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::Seek(val),
            ));
            if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                if audio.paused() {
                    let _ = audio.set_paused(false);
                }
            }
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::SeekRelative`.
    ///
    /// Seeks by a relative offset on the active video/audio view and unpauses
    /// playback.
    pub fn seek_relative(&mut self, val: f64) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::SeekRelative(val),
            ));
            if let Some(video) = self.video_view.video_opt.as_mut() {
                if video.paused() {
                    let _ = video.set_paused(false);
                }
            }
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::SeekRelative(val),
            ));
            if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                if audio.paused() {
                    let _ = audio.set_paused(false);
                }
            }
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::EndOfStream`.
    ///
    /// Forwards end-of-stream to the active video/audio view.
    pub fn end_of_stream(&mut self) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::EndOfStream,
            ));
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::EndOfStream,
            ));
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::NewFrame`.
    ///
    /// Forwards a new-frame event to the active video/audio view.
    pub fn new_frame(&mut self) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::NewFrame,
            ));
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::NewFrame,
            ));
            if let Some(offset) = self.audio_view.lyrics_scroll_target() {
                return cosmic::iced::widget::scrollable::scroll_to(
                    self.audio_view.lyrics_scroll_id.clone(),
                    cosmic::iced::widget::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(offset),
                    },
                );
            }
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::PlayPause`.
    ///
    /// Toggles play/pause on the active video/audio view.
    pub fn play_pause(&mut self) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::PlayPause,
            ));
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::PlayPause,
            ));
        } else {
            // no audio active
        }

        Task::none()
    }

    /// Handle `Message::Previous`.
    ///
    /// Steps to the previous item: previous file for video/audio, previous
    /// history entry for the browser, or previous file for the image view.
    pub fn previous(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::PreviousFile,
            ));
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::PreviousFile,
            ));
        } else if self.active_view == Mode::Browser {
            let _ = self.update(Message::TabMessage(
                entity_opt,
                crate::tab::Message::GoPrevious,
            ));
        } else {
            let _ = self.update(Message::ImageMessage(
                crate::image::image_view::Message::PreviousFile,
            ));
        }

        Task::none()
    }

    /// Handle `Message::Next`.
    ///
    /// Steps to the next item: next file for video/audio, next history entry
    /// for the browser, or next file for the image view.
    pub fn next(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        if self.active_view == Mode::Video {
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::NextFile,
            ));
        } else if self.active_view == Mode::Audio {
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::NextFile,
            ));
        } else if self.active_view == Mode::Browser {
            let _ = self.update(Message::TabMessage(
                entity_opt,
                crate::tab::Message::GoPrevious,
            ));
        } else {
            let _ = self.update(Message::ImageMessage(
                crate::image::image_view::Message::NextFile,
            ));
        }

        Task::none()
    }

    /// Handle `Message::MouseScroll`.
    ///
    /// Converts a mouse wheel delta into a ~10s seek relative to the current
    /// position on the active video/audio view and unpauses playback.
    pub fn mouse_scroll(&mut self, delta: cosmic::iced::core::mouse::ScrollDelta) -> Task<Message> {
        let delta_y = match delta {
            cosmic::iced::core::mouse::ScrollDelta::Lines { y, .. } => y,
            cosmic::iced::core::mouse::ScrollDelta::Pixels { y, .. } => y,
        };
        if self.active_view == Mode::Video {
            let seconds = self.video_view.position + delta_y as f64 * 10.0;
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::Seek(seconds),
            ));
            if let Some(video) = self.video_view.video_opt.as_mut() {
                if video.paused() {
                    let _ = video.set_paused(false);
                }
            }
        } else if self.active_view == Mode::Audio {
            let seconds = self.audio_view.position + delta_y as f64 * 10.0;
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::Seek(seconds),
            ));
            if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                if audio.paused() {
                    let _ = audio.set_paused(false);
                }
            }
        }

        Task::none()
    }

    /// Handle `Message::SeekBackward`.
    ///
    /// Seeks ~10s back (clamped to 0) on the active video/audio view.
    pub fn seek_backward(&mut self) -> Task<Message> {
        if self.active_view == Mode::Video {
            let position = self.video_view.position;
            let adjustment;
            if position < 10.0 {
                adjustment = 0.0;
            } else {
                adjustment = position - 10.0;
            }
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::Seek(adjustment),
            ));
        } else if self.active_view == Mode::Audio {
            let position = self.audio_view.position;
            let adjustment;
            if position < 10.0 {
                adjustment = 0.0;
            } else {
                adjustment = position - 10.0;
            }
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::Seek(adjustment),
            ));
        }

        Task::none()
    }

    /// Handle `Message::SeekForward`.
    ///
    /// Seeks ~10s forward (clamped to the duration) on the active
    /// video/audio view.
    pub fn seek_forward(&mut self) -> Task<Message> {
        if self.active_view == Mode::Video {
            let position = self.video_view.position;
            let adjustment;
            if position + 10.0 > self.video_view.duration {
                adjustment = 0.0;
            } else {
                adjustment = position + 10.0;
            }
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::Seek(adjustment),
            ));
        } else if self.active_view == Mode::Audio {
            let position = self.audio_view.position;
            let adjustment;
            if position + 10.0 > self.audio_view.duration {
                adjustment = 0.0;
            } else {
                adjustment = position + 10.0;
            }
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::Seek(adjustment),
            ));
        }

        Task::none()
    }

    /// Handle `Message::SkipToPosition`.
    ///
    /// Seeks so that the current position is at the given `seconds` on the
    /// active video/audio view.
    pub fn skip_to_position(&mut self, seconds: f64) -> Task<Message> {
        if self.active_view == Mode::Video {
            let position = self.video_view.position;
            let adjustment = seconds - position;
            let _ = self.update(Message::VideoMessage(
                crate::video::video_view::Message::Seek(adjustment),
            ));
        } else if self.active_view == Mode::Audio {
            let position = self.audio_view.position;
            let adjustment = seconds - position;
            let _ = self.update(Message::AudioMessage(
                crate::audio::audio_view::Message::Seek(adjustment),
            ));
        }

        Task::none()
    }

    /// Handle `Message::AudioMessage`.
    ///
    /// Detailed dispatch of audio-view messages: opening/switching files,
    /// transport, dropdowns, fullscreen, and track/volume controls. Unknown
    /// messages are forwarded to the audio view.
    pub fn audio_message(
        &mut self,
        audio_message: crate::audio::audio_view::Message,
    ) -> Task<Message> {
        match audio_message {
            crate::audio::audio_view::Message::ToBrowser => {
                self.active_view = Mode::Browser;
                self.view();
            }
            crate::audio::audio_view::Message::Open(audiopath) => {
                match url::Url::from_file_path(std::path::PathBuf::from(&audiopath)) {
                    Ok(url) => {
                        self.audio_view.audiopath_opt = Some(url);
                        // Cover art from the database record (embedded art
                        // extracted at scan time); `load()` falls back to a
                        // same-stem image next to the file.
                        let mut poster = None;
                        if let Some(tab) = self.tab_model.data::<Tab>(self.tab_model_id) {
                            if let Some(items) = tab.items_opt() {
                                for item in items.iter() {
                                    if let Some(audio) = item.audio_opt.as_ref() {
                                        if std::path::PathBuf::from(&audio.path)
                                            == std::path::PathBuf::from(&audiopath)
                                        {
                                            if poster.is_none() && !audio.poster.is_empty() {
                                                poster = Some(audio.poster.clone());
                                            } else if poster.is_none() && !audio.thumb.is_empty() {
                                                poster = Some(audio.thumb.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        self.audio_view.posterpath_opt = poster;
                        self.audio_view.load();
                        if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                            let v = tab.selected_file_paths();
                            for path in v {
                                if let Some(items) = tab.items_opt() {
                                    for item in items.iter() {
                                        if let Some(audio) = item.audio_opt.as_ref() {
                                            if std::path::PathBuf::from(&audio.path) == path {
                                                let (v, s) = crate::sql::fill_chapters(
                                                    audio.chapters.clone(),
                                                    audio.duration,
                                                );
                                                if v.len() > 0 {
                                                    self.audio_view.chapters.clear();
                                                    self.audio_view.chapters_str.clear();
                                                    self.audio_view.chapters.extend(v);
                                                    self.audio_view.chapters_str.extend(s);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        self.active_view = Mode::Audio;
                        self.view();
                    }
                    _ => {}
                }
            }
            crate::audio::audio_view::Message::NextFile => {
                // open next file in the sorted list if possible
                let id = self.tab_model.active();
                if id != self.tab_model_id {
                    self.tab_model_id = id;
                }
                if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                    let _ret = tab.update(tab::Message::ItemRight, self.modifiers);
                    if let Some(path) = tab.selected_file_paths().into_iter().next() {
                        return match self.open_path(path) {
                            Some(command) => command,
                            _ => Task::none(),
                        };
                    }
                }
            }
            crate::audio::audio_view::Message::PreviousFile => {
                // open next file in the sorted list if possible
                let id = self.tab_model.active();
                if id != self.tab_model_id {
                    self.tab_model_id = id;
                }
                if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                    let _ret = tab.update(tab::Message::ItemLeft, self.modifiers);
                    if let Some(path) = tab.selected_file_paths().into_iter().next() {
                        return match self.open_path(path) {
                            Some(command) => command,
                            _ => Task::none(),
                        };
                    }
                }
            }
            crate::audio::audio_view::Message::FileClose => {
                self.audio_view.close();
            }
            crate::audio::audio_view::Message::FileLoad(_url) => {
                self.audio_view.load();
            }
            crate::audio::audio_view::Message::FileOpen => {
                //TODO: embed cosmic-files dialog (after libcosmic rebase works)
            }
            crate::audio::audio_view::Message::DropdownToggle(menu_kind) => {
                if self.audio_view.dropdown_opt.take() != Some(menu_kind) {
                    self.audio_view.dropdown_opt = Some(menu_kind);
                }
            }
            crate::audio::audio_view::Message::Fullscreen => {
                //TODO: cleanest way to close dropdowns
                self.audio_view.dropdown_opt = None;

                self.audio_view.fullscreen = !self.audio_view.fullscreen;
                self.core.window.show_headerbar = !self.audio_view.fullscreen;
                return window::set_mode(
                    window::Id::RESERVED,
                    if self.audio_view.fullscreen {
                        window::Mode::Fullscreen
                    } else {
                        window::Mode::Windowed
                    },
                );
            }
            crate::audio::audio_view::Message::AudioCode(code) => {
                if let Some(audio) = &self.audio_view.audio_opt {
                    let idx = code as usize;
                    if let Err(e) = audio.select_audio(idx) {
                        log::error!("failed to select audio track {idx}: {e}");
                    } else {
                        self.audio_view.current_audio = code as i32;
                        self.audio_view.dropdown_opt = None;
                    }
                }
            }
            crate::audio::audio_view::Message::AudioToggle => {
                if let Some(audio) = &mut self.audio_view.audio_opt {
                    let _ = audio.set_muted(!audio.muted());
                    self.audio_view.update_controls(true);
                }
            }
            crate::audio::audio_view::Message::AudioVolume(volume) => {
                if let Some(audio) = &mut self.audio_view.audio_opt {
                    let _ = audio.set_volume(volume);
                    self.audio_view.update_controls(true);
                }
            }
            crate::audio::audio_view::Message::TextCode(_code) => {
                // Audio files in practice never expose subtitle tracks;
                // the model has no subtitle selection.
            }
            crate::audio::audio_view::Message::ShowControls => {
                self.audio_view.update_controls(true);
            }
            crate::audio::audio_view::Message::LyricSeek(idx) => {
                self.audio_view
                    .update(crate::audio::audio_view::Message::LyricSeek(idx));
                return cosmic::iced::widget::scrollable::scroll_to(
                    self.audio_view.lyrics_scroll_id.clone(),
                    cosmic::iced::widget::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(self.audio_view.lyrics_scroll_sent),
                    },
                );
            }
            crate::audio::audio_view::Message::LyricsScroll(offset) => {
                self.audio_view
                    .update(crate::audio::audio_view::Message::LyricsScroll(offset));
            }
            _ => self.audio_view.update(audio_message),
        }

        Task::none()
    }

    /// Handle `Message::ImageMessage`.
    ///
    /// Detailed dispatch of image-view messages: switching views, opening a
    /// specific image, and stepping through the file list. Unknown messages
    /// are forwarded to the image view.
    pub fn image_message(
        &mut self,
        image_message: crate::image::image_view::Message,
    ) -> Task<Message> {
        match image_message {
            crate::image::image_view::Message::ToBrowser => {
                self.active_view = Mode::Browser;
                self.view();
            }
            crate::image::image_view::Message::ToImage => {}
            crate::image::image_view::Message::ToVideo => {}
            crate::image::image_view::Message::ToAudio => {}
            crate::image::image_view::Message::Open(imagepath) => {
                self.image_view.image_path = imagepath;
                self.image_view.handle_opt = Some(crate::image::image::Handle::from_path(
                    self.image_view.image_path.clone(),
                ));
                self.image_view.image_path_loaded = self.image_view.image_path.clone();
                self.active_view = Mode::Image;
                self.view();
            }
            crate::image::image_view::Message::NextFile => {
                // open next file in the sorted list if possible
                let id = self.tab_model.active();
                if id != self.tab_model_id {
                    self.tab_model_id = id;
                }
                if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                    let _ret = tab.update(tab::Message::ItemRight, self.modifiers);
                    if let Some(path) = tab.selected_file_paths().into_iter().next() {
                        return match self.open_path(path) {
                            Some(command) => command,
                            _ => Task::none(),
                        };
                    }
                }
            }
            crate::image::image_view::Message::PreviousFile => {
                // open previous file in the sorted list if possible
                let id = self.tab_model.active();
                if id != self.tab_model_id {
                    self.tab_model_id = id;
                }
                if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                    let _ret = tab.update(tab::Message::ItemLeft, self.modifiers);
                    if let Some(path) = tab.selected_file_paths().into_iter().next() {
                        return match self.open_path(path) {
                            Some(command) => command,
                            _ => Task::none(),
                        };
                    }
                }
            }
            _ => {
                self.image_view.update(image_message);
            }
        }

        Task::none()
    }

    /// Handle `Message::VideoMessage`.
    ///
    /// Detailed dispatch of video-view messages: switching views, opening a
    /// video, stepping through the file list, transport, dropdowns,
    /// fullscreen, and track/volume controls. Unknown messages are forwarded
    /// to the video view.
    pub fn video_message(
        &mut self,
        video_message: crate::video::video_view::Message,
    ) -> Task<Message> {
        match video_message {
            crate::video::video_view::Message::ToBrowser => {
                self.active_view = Mode::Browser;
                self.view();
            }
            crate::video::video_view::Message::Open(videopath) => {
                match url::Url::from_file_path(std::path::PathBuf::from(&videopath)) {
                    Ok(url) => {
                        self.video_view.videopath_opt = Some(url);
                        self.video_view.load();
                        self.active_view = Mode::Video;
                        self.view();
                    }
                    _ => {}
                }
            }
            crate::video::video_view::Message::NextFile => {
                // open next file in the sorted list if possible
                let id = self.tab_model.active();
                if id != self.tab_model_id {
                    self.tab_model_id = id;
                }
                if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                    let _ret = tab.update(tab::Message::ItemRight, self.modifiers);
                    if let Some(path) = tab.selected_file_paths().into_iter().next() {
                        return match self.open_path(path) {
                            Some(command) => command,
                            _ => Task::none(),
                        };
                    }
                }
            }
            crate::video::video_view::Message::PreviousFile => {
                // open next file in the sorted list if possible
                let id = self.tab_model.active();
                if id != self.tab_model_id {
                    self.tab_model_id = id;
                }
                if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                    let _ret = tab.update(tab::Message::ItemLeft, self.modifiers);
                    if let Some(path) = tab.selected_file_paths().into_iter().next() {
                        return match self.open_path(path) {
                            Some(command) => command,
                            _ => Task::none(),
                        };
                    }
                }
            }
            crate::video::video_view::Message::FileClose => {
                self.video_view.close();
            }
            crate::video::video_view::Message::FileLoad(url) => {
                self.video_view.videopath_opt = Some(url);
                self.video_view.load();
            }
            crate::video::video_view::Message::FileOpen => {
                //TODO: embed cosmic-files dialog (after libcosmic rebase works)
            }
            crate::video::video_view::Message::DropdownToggle(menu_kind) => {
                if self.video_view.dropdown_opt.take() != Some(menu_kind) {
                    self.video_view.dropdown_opt = Some(menu_kind);
                }
            }
            crate::video::video_view::Message::Fullscreen => {
                //TODO: cleanest way to close dropdowns
                self.video_view.dropdown_opt = None;

                self.video_view.fullscreen = !self.video_view.fullscreen;
                self.core.window.show_headerbar = !self.video_view.fullscreen;
                return window::set_mode(
                    window::Id::RESERVED,
                    if self.video_view.fullscreen {
                        window::Mode::Fullscreen
                    } else {
                        window::Mode::Windowed
                    },
                );
            }
            crate::video::video_view::Message::AudioCode(code) => {
                if let Some(video) = &self.video_view.video_opt {
                    let idx = code as usize;
                    if let Err(e) = video.select_audio(idx) {
                        log::error!("failed to select audio track {idx}: {e}");
                    } else {
                        self.video_view.current_audio = code as i32;
                        self.video_view.dropdown_opt = None;
                    }
                }
            }
            crate::video::video_view::Message::AudioToggle => {
                if let Some(video) = &mut self.video_view.video_opt {
                    let _ = video.set_muted(!video.muted());
                    self.video_view.update_controls(true);
                }
            }
            crate::video::video_view::Message::AudioVolume(volume) => {
                if let Some(video) = &mut self.video_view.video_opt {
                    let _ = video.set_volume(volume);
                    self.video_view.update_controls(true);
                }
            }
            crate::video::video_view::Message::TextCode(code) => {
                if let Some(video) = &self.video_view.video_opt {
                    let idx = code as usize;
                    if let Err(e) = video.select_subtitle(idx) {
                        log::error!("failed to select subtitle track {idx}: {e}");
                    } else {
                        self.video_view.current_text = code as i32;
                        self.video_view.dropdown_opt = None;
                    }
                }
            }
            crate::video::video_view::Message::ShowControls => {
                self.video_view.update_controls(true);
            }
            _ => self.video_view.update(video_message),
        }

        Task::none()
    }
}

#[cfg(test)]
mod lyrics_integration {
    //! End-to-end check of the lyrics column flow (see
    //! `docs/plan-lyrics-view.md`): open an audio file that has a same-name
    //! `.lrc` through the real `App` message path, verify discovery, the
    //! headless `view()` build of the audio view with the lyrics column,
    //! and the seek / follow / manual-scroll message flow.
    //!
    //! The audio fixture is generated with `ffmpeg` (the same soft
    //! dependency the `audio::audio` tests use); without `ffmpeg` the test
    //! skips, like the other fixture-dependent tests.
    use super::*;
    use crate::app::Flags;
    use crate::config::Config;

    /// `(audio with .lrc, .lrc path, audio without .lrc)` in a shared
    /// tempdir, or `None` when no fixture can be produced.
    fn fixture() -> Option<(String, String, String)> {
        let dir =
            std::env::temp_dir().join(format!("mb_lyrics_integration_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok()?;
        let mp3 = dir.join("track.mp3");
        let lrc = dir.join("track.lrc");
        if !mp3.is_file() {
            let ok = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=2",
                    "-c:a",
                    "libmp3lame",
                    "-q:a",
                    "9",
                    mp3.to_str()?,
                ])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                return None;
            }
            std::fs::write(
                &lrc,
                "[00:00.00]First line\n[00:01.00]Second line\n[00:01.50]\n[00:02.00]Third line",
            )
            .ok()?;
            let cover = dir.join("track.png");
            image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
                .save(&cover)
                .ok()?;
            let plain = dir.join("plain.mp3");
            std::fs::copy(&mp3, &plain).ok()?;
        }
        Some((
            mp3.to_str()?.to_string(),
            lrc.to_str()?.to_string(),
            dir.join("plain.mp3").to_str()?.to_string(),
        ))
    }

    fn test_app(location: &std::path::Path) -> App {
        let (mut app, _task) = App::init(
            cosmic::app::Core::default(),
            Flags {
                config_handler: None,
                config: Config::default(),
                mode: Mode::Browser,
                locations: vec![crate::tab::Location::Path(location.to_path_buf())],
            },
        );
        app
    }

    #[test]
    fn lyrics_column_flow() {
        let Some((audio, lrc, plain)) = fixture() else {
            eprintln!("skipped: no audio fixture (ffmpeg missing?)");
            return;
        };

        // The fixture itself must be usable.
        let lyrics =
            crate::audio::lyrics::from_audio_file(&audio).expect("fixture .lrc must parse");
        assert!(std::path::Path::new(&lrc).is_file());

        let mut app = test_app(std::path::Path::new(&audio).parent().unwrap());

        // Open the audio file that has a same-name `.lrc` next to it.
        let _ = app.update(Message::AudioMessage(
            crate::audio::audio_view::Message::Open(audio.clone()),
        ));

        assert!(app.audio_view.audio_opt.is_some(), "audio must load");
        // Cover art: `track.png` next to the audio file (no DB row here, so
        // this exercises the same-stem fallback in `load()`).
        let cover = std::path::Path::new(&audio)
            .with_extension("png")
            .to_string_lossy()
            .to_string();
        assert!(std::path::Path::new(&cover).is_file());
        assert_eq!(
            app.audio_view.posterpath_opt.as_deref(),
            Some(cover.as_str())
        );
        assert!(
            !app.audio_view.lyrics.is_empty(),
            "lyrics must be discovered from track.lrc"
        );
        assert_eq!(
            app.audio_view.lyrics, lyrics,
            "view lyrics must match the parsed fixture"
        );
        let lines = app.audio_view.lyrics.clone();

        // The full view tree (audio view + lyrics column) must build.
        {
            let _el = app.view();
        }

        // LyricSeek: seeks to the line and (re-)arms follow mode.
        let target_idx = 3;
        let _ = app.update(Message::AudioMessage(
            crate::audio::audio_view::Message::LyricSeek(target_idx),
        ));
        assert!(
            (app.audio_view.position - lines[target_idx].time).abs() < 1e-6,
            "position must be at the clicked line"
        );
        assert_eq!(
            app.audio_view.lyrics_scroll_sent,
            crate::audio::lyrics::scroll_target(&lines, target_idx)
        );
        assert!(app.audio_view.lyrics_follow);

        // Manual scrolling disables follow and records the offset.
        let _ = app.update(Message::AudioMessage(
            crate::audio::audio_view::Message::LyricsScroll(123.0),
        ));
        assert!(!app.audio_view.lyrics_follow);
        assert_eq!(app.audio_view.lyrics_scroll_sent, 123.0);
        // Same active line: no new scroll target while the user scrolls.
        app.audio_view.position = lines[target_idx].time;
        assert_eq!(app.audio_view.lyrics_scroll_target(), None);

        // Playback reaching a different line resumes follow with the
        // right target, then settles (no repeated commands).
        let other_idx = 1;
        app.audio_view.position = lines[other_idx].time;
        assert_eq!(
            app.audio_view.lyrics_scroll_target(),
            Some(crate::audio::lyrics::scroll_target(&lines, other_idx))
        );
        assert!(app.audio_view.lyrics_follow);
        assert_eq!(app.audio_view.lyrics_scroll_target(), None);

        // A file without an `.lrc` must load with no lyrics, and without
        // an adjacent image it must not inherit the previous cover.
        let _ = app.update(Message::AudioMessage(
            crate::audio::audio_view::Message::Open(plain),
        ));
        assert!(app.audio_view.audio_opt.is_some(), "plain audio must load");
        assert!(app.audio_view.lyrics.is_empty());
        assert!(app.audio_view.posterpath_opt.is_none());
        {
            let _el = app.view();
        }
    }
}
