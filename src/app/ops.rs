//! Message handlers for clipboard operations, external launching, and item
//! preview.
//!
//! Split out of `app.rs` (Phase 3 — modularization). The original arms of
//! `App::update` are now individual methods on `App`, dispatched from
//! `mod.rs` with a single forwarding line each:
//!
//! | `Message` arm           | Method                                   |
//! |-------------------------|------------------------------------------|
//! | `Message::Copy`         | `App::copy`                              |
//! | `Message::Cut`          | `App::cut`                               |
//! | `Message::CloseToast`   | `App::close_toast`                       |
//! | `Message::CosmicSettings` | `App::cosmic_settings`                 |
//! | `Message::LaunchUrl`    | `App::launch_url`                        |
//! | `Message::NewItem`      | `App::new_item`                          |
//! | `Message::Paste`        | `App::paste`                             |
//! | `Message::PasteContents`| `App::paste_contents`                    |
//! | `Message::Preview`      | `App::preview_message`                   |
//! | `Message::Rename`       | `App::rename`                            |
//! | `Message::RenameWithPattern` | `App::rename_with_pattern`         |
//! | `Message::ReplaceResult`| `App::replace_result`                    |
//!
//! Note: `App::preview_message` is named with a `_message` suffix because
//! `App::preview` is already taken by the preview view builder. It also uses
//! the `config_set!` macro, which must be defined at the top level of
//! `app/mod.rs` (moved out of the former inline definition inside
//! `App::update`) so this sibling module can access it.

use cosmic::app::{message, Task};
use cosmic::iced::{window, Size};
use cosmic::iced_runtime::clipboard;
use cosmic::widget::{self, segmented_button::Entity, ToastId};
use std::path::PathBuf;
use std::process;

use super::{App, ContextPage, DialogPage, Message, Mode, PreviewKind};
use crate::clipboard::{ClipboardCopy, ClipboardKind, ClipboardPaste};
use crate::operation::{Operation, ReplaceResult};
use crate::spawn_detached::spawn_detached;
use crate::tab::{Location, Tab};

impl App {
    /// Handle `Message::Copy`.
    ///
    /// Copies the selected paths in the given (or active) tab to the
    /// clipboard.
    pub fn copy(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        let paths = self.selected_paths(entity_opt);
        let contents = ClipboardCopy::new(ClipboardKind::Copy, &paths);
        clipboard::write_data(contents)
    }

    /// Handle `Message::Cut`.
    ///
    /// Cuts the selected paths in the given (or active) tab to the
    /// clipboard.
    pub fn cut(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        let paths = self.selected_paths(entity_opt);
        let contents = ClipboardCopy::new(ClipboardKind::Cut, &paths);
        clipboard::write_data(contents)
    }

    /// Handle `Message::CloseToast`.
    ///
    /// Removes the given toast.
    pub fn close_toast(&mut self, id: ToastId) -> Task<Message> {
        self.toasts.remove(id);
        Task::none()
    }

    /// Handle `Message::CosmicSettings`.
    ///
    /// Launches `cosmic-settings` with the given argument.
    pub fn cosmic_settings(arg: &'static str) -> Task<Message> {
        //TODO: use special settings URL scheme instead?
        let mut command = process::Command::new("cosmic-settings");
        command.arg(arg);
        match spawn_detached(&mut command) {
            Ok(()) => {}
            Err(err) => {
                log::warn!("failed to run cosmic-settings {}: {}", arg, err)
            }
        }
        Task::none()
    }

    /// Handle `Message::LaunchUrl`.
    ///
    /// Opens the given URL with the default application.
    pub fn launch_url(url: String) -> Task<Message> {
        match open::that_detached(&url) {
            Ok(()) => {}
            Err(err) => {
                log::warn!("failed to open {:?}: {}", url, err);
            }
        }
        Task::none()
    }

    /// Handle `Message::NewItem`.
    ///
    /// Pushes a `DialogPage::NewItem` for the location of the given (or
    /// active) tab and focuses the dialog's text input.
    pub fn new_item(&mut self, entity_opt: Option<Entity>, dir: bool) -> Task<Message> {
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            if let Some(path) = &tab.location.path_opt() {
                self.dialog_pages.push_back(DialogPage::NewItem {
                    parent: path.to_path_buf(),
                    name: String::new(),
                    dir,
                });
                return widget::text_input::focus(self.dialog_text_input.clone());
            }
        }

        Task::none()
    }

    /// Handle `Message::Paste`.
    ///
    /// Reads the clipboard asynchronously and, if it contains valid
    /// clipboard contents, re-dispatches them as `Message::PasteContents`
    /// targeted at the location of the given (or active) tab.
    pub fn paste(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            if let Some(path) = tab.location.path_opt() {
                let to = path.clone();
                return clipboard::read_data::<ClipboardPaste>().map(move |contents_opt| {
                    match contents_opt {
                        Some(contents) => {
                            message::app(Message::PasteContents(to.clone(), contents))
                        }
                        None => message::none(),
                    }
                });
            }
        }

        Task::none()
    }

    /// Handle `Message::PasteContents`.
    ///
    /// Executes the clipboard contents (copy or cut) as an operation
    /// targeting `to`, after dropping any paths that would paste onto
    /// themselves.
    pub fn paste_contents(&mut self, to: PathBuf, mut contents: ClipboardPaste) -> Task<Message> {
        contents.paths.retain(|p| p != &to);
        if !contents.paths.is_empty() {
            match contents.kind {
                ClipboardKind::Copy => {
                    self.operation(Operation::Copy {
                        paths: contents.paths,
                        to,
                    });
                }
                ClipboardKind::Cut => {
                    self.operation(Operation::Move {
                        paths: contents.paths,
                        to,
                    });
                }
            }
        }
        Task::none()
    }

    /// Handle `Message::Preview`.
    ///
    /// Toggles the preview context page (or opens a preview window in
    /// desktop mode), preserving the show-details config either way.
    pub fn preview_message(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        match self.mode {
            Mode::App => {
                let show_details = !self.config.show_details;
                //TODO: move to update_config?
                self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                self.core.window.show_context = show_details;
                config_set!(self, show_details, show_details);
                return self.update_config();
            }
            Mode::Desktop => {
                let selected_paths = self.selected_paths(entity_opt);
                let mut commands = Vec::with_capacity(selected_paths.len());
                for _path in selected_paths {
                    let mut settings = window::Settings::default();
                    settings.decorations = true;
                    settings.min_size = Some(Size::new(360.0, 180.0));
                    settings.resizable = true;
                    settings.size = Size::new(480.0, 600.0);
                    settings.transparent = true;

                    #[cfg(target_os = "linux")]
                    {
                        // Use the dialog ID to make it float
                        settings.platform_specific.application_id =
                            "eu.fangornsrealm.MediaBrowserDialog".to_string();
                    }

                    let (_id, command) = window::open(settings);
                    commands.push(command.map(|_id| message::none()));
                }
                return Task::batch(commands);
            }
            Mode::Browser => {
                let show_details = !self.config.show_details;
                //TODO: move to update_config?
                self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                self.core.window.show_context = show_details;
                config_set!(self, show_details, show_details);
                return self.update_config();
            }
            Mode::Image => {
                let show_details = !self.config.show_details;
                //TODO: move to update_config?
                self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                self.core.window.show_context = show_details;
                config_set!(self, show_details, show_details);
                return self.update_config();
            }
            Mode::Video => {
                let show_details = !self.config.show_details;
                //TODO: move to update_config?
                self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                self.core.window.show_context = show_details;
                config_set!(self, show_details, show_details);
                return self.update_config();
            }
            Mode::Audio => {
                let show_details = !self.config.show_details;
                //TODO: move to update_config?
                self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                self.core.window.show_context = show_details;
                config_set!(self, show_details, show_details);
                return self.update_config();
            }
        }
    }

    /// Handle `Message::Rename`.
    ///
    /// Pushes a `DialogPage::RenameItem` for every selected item in the
    /// given (or active) tab and focuses the dialog's text input.
    pub fn rename(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            if let Some(parent) = tab.location.path_opt() {
                if let Some(items) = tab.items_opt() {
                    let mut selected = Vec::new();
                    for item in items.iter() {
                        if item.selected {
                            if let Some(path) = item.path_opt() {
                                selected.push(path.to_path_buf());
                            }
                        }
                    }
                    if !selected.is_empty() {
                        //TODO: batch rename
                        for path in selected {
                            let name = match path.file_name().and_then(|x| x.to_str()) {
                                Some(some) => some.to_string(),
                                None => continue,
                            };
                            let dir = path.is_dir();
                            self.dialog_pages.push_back(DialogPage::RenameItem {
                                from: path,
                                parent: parent.clone(),
                                name,
                                dir,
                            });
                        }
                        return widget::text_input::focus(self.dialog_text_input.clone());
                    }
                }
            }
        }

        Task::none()
    }

    /// Handle `Message::RenameWithPattern`.
    ///
    /// Spawns a background rename-with-pattern operation for every selected
    /// non-video item in the given (or active) tab.
    pub fn rename_with_pattern(
        &mut self,
        entity_opt: Option<Entity>,
        pattern: String,
        start_val: i32,
        numdigits: i32,
    ) -> Task<Message> {
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            if let Location::Path(_parent) = &tab.location {
                if let Some(items) = tab.items_opt() {
                    let mut selected = Vec::new();
                    for item in items.iter() {
                        if item.selected {
                            if item.video_opt.is_some() {
                                continue;
                            }
                            if let Some(Location::Path(path)) = &item.location_opt {
                                selected.push(path.clone());
                            }
                        }
                    }
                    let _joinhandle = std::thread::spawn(move || {
                        crate::cmd::rename_with_pattern(selected, pattern, start_val, numdigits)
                    });
                }
            }
        }
        Task::none()
    }

    /// Handle `Message::ReplaceResult`.
    ///
    /// Sends the `ReplaceResult` to the front `DialogPage::Replace`'s
    /// receiver, or re-pushes it if the front dialog is something else.
    pub fn replace_result(&mut self, replace_result: ReplaceResult) -> Task<Message> {
        if let Some(dialog_page) = self.dialog_pages.pop_front() {
            match dialog_page {
                DialogPage::Replace { tx, .. } => {
                    return Task::perform(
                        async move {
                            let _ = tx.send(replace_result).await;
                            message::none()
                        },
                        |x| x,
                    );
                }
                other => {
                    log::warn!("tried to send replace result to the wrong dialog");
                    self.dialog_pages.push_front(other);
                }
            }
        }

        Task::none()
    }
}
