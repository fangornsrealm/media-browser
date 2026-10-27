//! Message handlers for the tab bar lifecycle (open/activate/switch/close),
//! the `TabMessage` command dispatch, and context-page toggling.
//!
//! Split out of `app.rs` (Phase 3 — modularization). Each of the original
//! arms of `App::update` is now an individual method on `App`, dispatched
//! from `mod.rs` with a single forwarding line:
//!
//! | Group           | `Message` arm             | Method                     |
//!|------------------|---------------------------|----------------------------|
//! Tab lifecycle    | `Message::Open`           | `App::open_message`        |
//! Tab lifecycle    | `Message::TabActivate`    | `App::tab_activate`        |
//! Tab lifecycle    | `Message::TabNext`        | `App::tab_next`            |
//! Tab lifecycle    | `Message::TabPrev`        | `App::tab_prev`            |
//! Tab lifecycle    | `Message::TabClose`       | `App::tab_close`           |
//! Tab lifecycle    | `Message::TabConfig`      | `App::tab_config`          |
//! Tab lifecycle    | `Message::ToggleFoldersFirst` | `App::toggle_folders_first` |
//! Tab lifecycle    | `Message::TabNew`         | `App::tab_new`             |
//! Tab lifecycle    | `Message::TabRescan`      | `App::tab_rescan`          |
//! Tab lifecycle    | `Message::TabView`        | `App::tab_view`            |
//! Command dispatch | `Message::TabMessage`     | `App::tab_message`         |
//! Context page     | `Message::ToggleContextPage` | `App::toggle_context_page` |
//!
//! Note: `tab_config` and `tab_message` use the `config_set!` macro, which
//! must be defined at the top level of `app/mod.rs` (moved out of the former
//! inline definition inside `App::update`) so this sibling module can access
//! it. `update`, `view`, and `APP_ID` come from the `cosmic::Application`
//! trait, which is imported for that reason.

use cosmic::app::{message, Task};
use cosmic::iced::window;
use cosmic::widget::segmented_button::Entity;
use cosmic::{Application, ApplicationExt};
use std::{env, path::PathBuf, process};

use super::{App, ContextPage, DialogPage, Message};
use crate::config::MediaFavorite as Favorite;
use crate::home_dir;
use crate::mime_app;
use crate::spawn_detached::spawn_detached;
use crate::tab::{self, Location, Tab};

impl App {
    /// Handle `Message::Open`.
    ///
    /// Opens the selected file of the active tab (if there is one) via the
    /// shared `open_path` helper.
    pub fn open_message(&mut self, _entity_opt: Option<Entity>, filepath: String) -> Task<Message> {
        if filepath.len() > 0 {}
        if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
            if let Some(path) = tab.selected_file_paths().into_iter().next() {
                return match self.open_path(path) {
                    Some(command) => command,
                    _ => Task::none(),
                };
            }
        }

        Task::none()
    }

    /// Handle `Message::TabActivate`.
    ///
    /// Activates the given tab, keeps the nav bar in sync with its
    /// location, and refreshes the window title.
    pub fn tab_activate(&mut self, entity: Entity) -> Task<Message> {
        self.tab_model.activate(entity);

        if let Some(tab) = self.tab_model.data::<Tab>(entity) {
            self.activate_nav_model_location(&tab.location.clone());
        }
        self.update_title()
    }

    /// Handle `Message::TabNext`.
    ///
    /// Activates the tab after the active one, wrapping around to the first
    /// tab if this was the last.
    pub fn tab_next(&mut self) -> Task<Message> {
        let len = self.tab_model.iter().count();
        let pos = self
            .tab_model
            .position(self.tab_model.active())
            // Wraparound to 0 if i + 1 > num of tabs
            .map(|i| (i as usize + 1) % len)
            .expect("should always be at least one tab open");

        let entity = self.tab_model.iter().nth(pos);
        if let Some(entity) = entity {
            return self.update(Message::TabActivate(entity));
        }

        Task::none()
    }

    /// Handle `Message::TabPrev`.
    ///
    /// Activates the tab before the active one, wrapping around to the last
    /// tab if this was the first.
    pub fn tab_prev(&mut self) -> Task<Message> {
        let pos = self
            .tab_model
            .position(self.tab_model.active())
            .and_then(|i| (i as usize).checked_sub(1))
            // Subtraction underflow => last tab; i.e. it wraps around
            .unwrap_or_else(|| {
                self.tab_model
                    .iter()
                    .count()
                    .checked_sub(1)
                    .unwrap_or_default()
            });

        let entity = self.tab_model.iter().nth(pos);
        if let Some(entity) = entity {
            return self.update(Message::TabActivate(entity));
        }

        Task::none()
    }

    /// Handle `Message::TabClose`.
    ///
    /// Closes the given (or active) tab, activates the closest remaining
    /// tab, and closes the window if this was the last tab. Also refreshes
    /// the window title and the file watcher.
    pub fn tab_close(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());

        // Activate closest item
        if let Some(position) = self.tab_model.position(entity) {
            let new_position = if position > 0 {
                position - 1
            } else {
                position + 1
            };

            if self.tab_model.activate_position(new_position) {
                if let Some(new_entity) = self.tab_model.entity_at(new_position) {
                    if let Some(tab) = self.tab_model.data::<Tab>(new_entity) {
                        self.activate_nav_model_location(&tab.location.clone());
                    }
                }
            }
        }

        // Remove item
        self.tab_model.remove(entity);

        // If that was the last tab, close window
        if self.tab_model.iter().next().is_none() {
            if let Some(window_id) = &self.window_id_opt {
                return window::close(*window_id);
            }
        }

        Task::batch([self.update_title(), self.update_watcher()])
    }

    /// Handle `Message::TabConfig`.
    ///
    /// Persists the tab configuration (view, icon sizes, sort, ...) if it
    /// changed, then re-applies the config.
    pub fn tab_config(&mut self, config: crate::config::MediaTabConfig) -> Task<Message> {
        if config != self.config.tab {
            config_set!(self, tab, config);
            return self.update_config();
        }

        Task::none()
    }

    /// Handle `Message::ToggleFoldersFirst`.
    ///
    /// Toggles the folders-first flag in the tab configuration.
    pub fn toggle_folders_first(&mut self) -> Task<Message> {
        let mut config = self.config.tab;
        config.folders_first = !config.folders_first;
        self.update(Message::TabConfig(config))
    }

    /// Handle `Message::TabMessage`.
    ///
    /// Forwards a tab message to the given (or active) tab, then executes
    /// the resulting `tab::Command`s in the application context: actions
    /// are re-dispatched as application messages, location changes update
    /// the nav model and rescan the tab, and so on.
    pub fn tab_message(
        &mut self,
        entity_opt: Option<Entity>,
        tab_message: tab::Message,
    ) -> Task<Message> {
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());

        //TODO: move to Task?
        //if let tab::Message::ContextMenu(_point_opt) = tab_message {
        // Disable side context page
        //    self.set_show_context(false);
        //}

        let tab_commands = match self.tab_model.data_mut::<Tab>(entity) {
            Some(tab) => tab.update(tab_message, self.modifiers),
            _ => Vec::new(),
        };

        let mut commands = Vec::new();
        for tab_command in tab_commands {
            match tab_command {
                tab::Command::Action(action) => {
                    commands.push(self.update(action.message(Some(entity))));
                }
                tab::Command::AddNetworkDrive => {
                    self.context_page = ContextPage::NetworkDrive;
                    self.set_show_context(true);
                }
                tab::Command::AddToSidebar(path) => {
                    let mut favorites = self.config.favorites.clone();
                    let favorite = Favorite::from_path(path);
                    if !favorites.iter().any(|f| f == &favorite) {
                        favorites.push(favorite);
                    }
                    config_set!(self, favorites, favorites);
                    commands.push(self.update_config());
                }
                tab::Command::ChangeLocation(tab_title, tab_path, selection_paths) => {
                    self.activate_nav_model_location(&tab_path);

                    self.tab_model.text_set(entity, tab_title);
                    commands.push(Task::batch([
                        self.update_title(),
                        self.update_watcher(),
                        self.rescan_tab(entity, tab_path, selection_paths),
                    ]));
                }
                tab::Command::DropFiles(to, from) => {
                    commands.push(self.update(Message::PasteContents(to, from)));
                }
                tab::Command::EmptyTrash => {
                    self.dialog_pages.push_back(DialogPage::EmptyTrash);
                }
                tab::Command::Iced(iced_command) => {
                    commands.push(iced_command.0.map(move |tab_message| {
                        message::app(Message::TabMessage(Some(entity), tab_message))
                    }));
                }
                tab::Command::MoveToTrash(paths) => {
                    self.operation(crate::operation::Operation::Delete { paths });
                }
                tab::Command::Open(filepath) => {
                    return match self.open_path(filepath) {
                        Some(command) => command,
                        _ => Task::none(),
                    };
                }
                tab::Command::OpenInExternalApp(path) => {
                    let mut found_desktop_exec = false;
                    if crate::mime_icon::mime_for_path(&path) == "application/x-desktop" {
                        match freedesktop_entry_parser::parse_entry(&path) {
                            Ok(entry) => match entry.section("Desktop Entry").attr("Exec") {
                                Some(exec) => match mime_app::exec_to_command(exec, None) {
                                    Some(mut command) => match spawn_detached(&mut command) {
                                        Ok(()) => {
                                            found_desktop_exec = true;
                                        }
                                        Err(err) => {
                                            log::warn!("failed to execute {:?}: {}", path, err);
                                        }
                                    },
                                    None => {
                                        log::warn!(
                                            "failed to parse {:?}: invalid Desktop Entry/Exec",
                                            path
                                        );
                                    }
                                },
                                None => {
                                    log::warn!(
                                        "failed to parse {:?}: missing Desktop Entry/Exec",
                                        path
                                    );
                                }
                            },
                            Err(err) => {
                                log::warn!("failed to parse {:?}: {}", path, err);
                            }
                        };
                    }
                    if !found_desktop_exec {
                        match open::that_detached(&path) {
                            Ok(()) => {
                                let _ = recently_used_xbel::update_recently_used(
                                    &path,
                                    App::APP_ID.to_string(),
                                    "cosmic-media-Tab".to_string(),
                                    None,
                                );
                            }
                            Err(err) => {
                                log::warn!("failed to open {:?}: {}", path, err);
                            }
                        }
                    }
                }
                tab::Command::OpenInNewWindow(path) => match env::current_exe() {
                    Ok(exe) => match process::Command::new(&exe).arg(path).spawn() {
                        Ok(_child) => {}
                        Err(err) => {
                            log::error!("failed to execute {:?}: {}", exe, err);
                        }
                    },
                    Err(err) => {
                        log::error!("failed to get current executable path: {}", err);
                    }
                },
                tab::Command::OpenTrash => {
                    //TODO: use handler for x-scheme-handler/trash and open trash:///
                    let mut command = process::Command::new("cosmic-files");
                    command.arg("--trash");
                    match spawn_detached(&mut command) {
                        Ok(()) => {}
                        Err(err) => {
                            log::warn!("failed to run cosmic-files --trash: {}", err)
                        }
                    }
                }
                tab::Command::Preview(kind) => {
                    self.context_page = ContextPage::Preview(Some(entity), kind);
                    self.set_show_context(true);
                }
                tab::Command::WindowDrag => {
                    if let Some(window_id) = &self.window_id_opt {
                        commands.push(window::drag(*window_id));
                    }
                }
                tab::Command::WindowToggleMaximize => {
                    if let Some(window_id) = &self.window_id_opt {
                        commands.push(window::toggle_maximize(*window_id));
                    }
                }
            }
        }
        return Task::batch(commands);
    }

    /// Handle `Message::TabNew`.
    ///
    /// Opens a new tab starting at the active tab's location (or the home
    /// directory if there is none).
    pub fn tab_new(&mut self) -> Task<Message> {
        let active = self.tab_model.active();
        let location = match self.tab_model.data::<Tab>(active) {
            Some(tab) => tab.location.clone(),
            None => Location::Path(home_dir()),
        };
        let _ = self.open_tab_entity(location, true, None);

        Task::none()
    }

    /// Handle `Message::TabRescan`.
    ///
    /// Applies freshly scanned items to the given tab, but only if the tab
    /// is still at the location the scan was started from (to avoid clobbering
    /// a tab the user has navigated away since).
    pub fn tab_rescan(
        &mut self,
        entity: Entity,
        location: Location,
        parent_item_opt: Option<tab::Item>,
        items: Vec<tab::Item>,
        selection_paths: Option<Vec<PathBuf>>,
    ) -> Task<Message> {
        match self.tab_model.data_mut::<Tab>(entity) {
            Some(tab) => {
                if location == tab.location {
                    tab.parent_item_opt = parent_item_opt;
                    tab.set_items(items);
                    if let Some(selection_paths) = selection_paths {
                        tab.select_paths(selection_paths);
                    }
                }
            }
            _ => (),
        }

        Task::none()
    }

    /// Handle `Message::TabView`.
    ///
    /// Switches the given (or active) tab to the given view (list/grid) and
    /// persists the view in the tab configuration.
    pub fn tab_view(&mut self, entity_opt: Option<Entity>, view: tab::View) -> Task<Message> {
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            tab.config.view = view;
        }
        let mut config = self.config.tab;
        config.view = view;
        self.update(Message::TabConfig(config))
    }

    /// Handle `Message::ToggleContextPage`.
    ///
    /// Toggles the side context drawer: if the given page is already the
    /// current one, the drawer is shown/hidden; otherwise the drawer is
    /// shown and the page is switched.
    pub fn toggle_context_page(&mut self, context_page: ContextPage) -> Task<Message> {
        //TODO: ensure context menus are closed
        if self.context_page == context_page {
            self.set_show_context(!self.core.window.show_context);
        } else {
            self.set_show_context(true);
        }
        self.context_page = context_page;
        Task::none()
    }
}
