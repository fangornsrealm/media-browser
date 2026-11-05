//! Message handlers for mounters, network drives, drag-and-drop onto the
//! nav bar and tabs, and nav bar context-menu actions.
//!
//! Split out of `app.rs` (Phase 3 — modularization). The original arms of
//! `App::update` are now individual methods on `App`, dispatched from
//! `mod.rs` with a single forwarding line each:
//!
//! | `Message` arm                | Method                      |
//! |------------------------------|-----------------------------|
//! | `Message::MounterItems`      | `App::mounter_items`        |
//! | `Message::MountResult`       | `App::mount_result`         |
//! | `Message::NetworkAuth`       | `App::network_auth`         |
//! | `Message::NetworkDriveInput` | `App::network_drive_input`  |
//! | `Message::NetworkDriveSubmit`| `App::network_drive_submit` |
//! | `Message::NetworkResult`     | `App::network_result`       |
//! | `Message::DndEnterNav`       | `App::dnd_enter_nav`        |
//! | `Message::DndExitNav`        | `App::dnd_exit_nav`         |
//! | `Message::DndDropNav`        | `App::dnd_drop_nav`         |
//! | `Message::DndHoverLocTimeout`| `App::dnd_hover_loc_timeout`|
//! | `Message::DndEnterTab`       | `App::dnd_enter_tab`        |
//! | `Message::DndExitTab`        | `App::dnd_exit_tab`         |
//! | `Message::DndDropTab`        | `App::dnd_drop_tab`         |
//! | `Message::DndHoverTabTimeout`| `App::dnd_hover_tab_timeout`|
//! | `Message::NavBarClose`       | `App::nav_bar_close`        |
//! | `Message::NavBarContext`     | `App::nav_bar_context`      |
//! | `Message::NavMenuAction`     | `App::nav_menu_action`      |
//!
//! Note: `nav_menu_action` uses the `config_set!` macro, which must be
//! defined at the top level of `app/mod.rs` (moved out of the former inline
//! definition inside `App::update`) so this sibling module can access it.

use cosmic::action as message;
use cosmic::app::Task;
use cosmic::iced::clipboard::dnd::DndAction;
use cosmic::widget::segmented_button::Entity;
use cosmic::{Application, ApplicationExt};
use std::time::Instant;
use tokio::sync::mpsc;

use super::{
    App, ContextPage, DialogPage, FavoriteIndex, Message, MounterData, NavMenuAction, PreviewItem,
    PreviewKind,
};
use crate::clipboard::{ClipboardKind, ClipboardPaste};
use crate::config::IconSizes;
use crate::home_dir;
use crate::mounter::{MOUNTERS, MounterAuth, MounterItem, MounterItems, MounterKey};
use crate::operation::Operation;
use crate::tab::{HOVER_DURATION, Location, Tab};

impl App {
    /// Handle `Message::MounterItems`.
    ///
    /// Records the latest list of mounter items, sends any tabs that were
    /// unmounted back to the home directory, and refreshes the nav model.
    pub fn mounter_items(
        &mut self,
        mounter_key: MounterKey,
        mounter_items: MounterItems,
    ) -> Task<Message> {
        // Check for unmounted folders
        let mut unmounted = Vec::new();
        if let Some(old_items) = self.mounter_items.get(&mounter_key) {
            for old_item in old_items.iter() {
                if let Some(old_path) = old_item.path() {
                    if old_item.is_mounted() {
                        let mut still_mounted = false;
                        for item in mounter_items.iter() {
                            if let Some(path) = item.path() {
                                if path == old_path {
                                    if item.is_mounted() {
                                        still_mounted = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if !still_mounted {
                            unmounted.push(Location::Path(old_path));
                        }
                    }
                }
            }
        }

        // Go back to home in any tabs that were unmounted
        let mut commands = Vec::new();
        {
            let home_location = Location::Path(home_dir());
            let entities: Vec<_> = self.tab_model.iter().collect();
            for entity in entities {
                let title_opt = match self.tab_model.data_mut::<Tab>(entity) {
                    Some(tab) => {
                        if unmounted.contains(&tab.location) {
                            tab.change_location(&home_location, None);
                            Some(tab.title())
                        } else {
                            None
                        }
                    }
                    None => None,
                };
                if let Some(title) = title_opt {
                    self.tab_model.text_set(entity, title);
                    commands.push(self.rescan_tab(entity, home_location.clone(), None));
                }
            }
            if !commands.is_empty() {
                commands.push(self.update_title());
                commands.push(self.update_watcher());
            }
        }

        // Insert new items
        self.mounter_items.insert(mounter_key, mounter_items);

        // Update nav bar
        //TODO: this could change favorites IDs while they are in use
        self.update_nav_model();

        Task::batch(commands)
    }

    /// Handle `Message::MountResult`.
    ///
    /// Logs the result of a mount operation, pushing a
    /// `DialogPage::MountError` on failure.
    pub fn mount_result(
        &mut self,
        mounter_key: MounterKey,
        item: MounterItem,
        res: Result<bool, String>,
    ) -> Task<Message> {
        match res {
            Ok(true) => {
                log::info!("connected to {:?}", item);
            }
            Ok(false) => {
                log::info!("cancelled connection to {:?}", item);
            }
            Err(error) => {
                log::warn!("failed to connect to {:?}: {}", item, error);
                self.dialog_pages.push_back(DialogPage::MountError {
                    mounter_key,
                    item,
                    error,
                });
            }
        }
        Task::none()
    }

    /// Handle `Message::NetworkAuth`.
    ///
    /// Pushes a `DialogPage::NetworkAuth` for the given mounter and URI.
    pub fn network_auth(
        &mut self,
        mounter_key: MounterKey,
        uri: String,
        auth: MounterAuth,
        auth_tx: mpsc::Sender<MounterAuth>,
    ) -> Task<Message> {
        self.dialog_pages.push_back(DialogPage::NetworkAuth {
            mounter_key,
            uri,
            auth,
            auth_tx,
        });
        Task::none()
    }

    /// Handle `Message::NetworkDriveInput`.
    ///
    /// Stores the currently-typed network drive URI.
    pub fn network_drive_input(&mut self, input: String) -> Task<Message> {
        self.network_drive_input = input;
        Task::none()
    }

    /// Handle `Message::NetworkDriveSubmit`.
    ///
    /// Submits the currently-typed network drive URI to the first available
    /// mounter.
    pub fn network_drive_submit(&mut self) -> Task<Message> {
        //TODO: know which mounter to use for network drives
        if let Some((mounter_key, mounter)) = MOUNTERS.iter().next() {
            self.network_drive_connecting = Some((*mounter_key, self.network_drive_input.clone()));
            return mounter
                .network_drive(self.network_drive_input.clone())
                .map(|_| message::none());
        }
        log::warn!(
            "no mounter found for connecting to {:?}",
            self.network_drive_input
        );
        Task::none()
    }

    /// Handle `Message::NetworkResult`.
    ///
    /// Logs the result of a network drive connection attempt, dismissing the
    /// network drive context page on success.
    pub fn network_result(
        &mut self,
        mounter_key: MounterKey,
        uri: String,
        res: Result<bool, String>,
    ) -> Task<Message> {
        if self.network_drive_connecting == Some((mounter_key, uri.clone())) {
            self.network_drive_connecting = None;
        }
        match res {
            Ok(true) => {
                log::info!("connected to {:?}", uri);
                if matches!(self.context_page, ContextPage::NetworkDrive) {
                    self.set_show_context(false);
                }
            }
            Ok(false) => {
                log::info!("cancelled connection to {:?}", uri);
            }
            Err(error) => {
                log::warn!("failed to connect to {:?}: {}", uri, error);
                self.dialog_pages.push_back(DialogPage::NetworkError {
                    mounter_key,
                    uri,
                    error,
                });
            }
        }
        Task::none()
    }

    /// Handle `Message::DndEnterNav`.
    ///
    /// Starts the nav-bar drag-and-drop hover timer for the given entity.
    pub fn dnd_enter_nav(&mut self, entity: Entity) -> Task<Message> {
        if let Some(location) = self.nav_model.data::<Location>(entity) {
            self.nav_dnd_hover = Some((location.clone(), Instant::now()));
            let location = location.clone();
            return Task::perform(tokio::time::sleep(HOVER_DURATION), move |_| {
                cosmic::action::app(Message::DndHoverLocTimeout(location.clone()))
            });
        }
        Task::none()
    }

    /// Handle `Message::DndExitNav`.
    ///
    /// Cancels the nav-bar drag-and-drop hover state.
    pub fn dnd_exit_nav(&mut self) -> Task<Message> {
        self.nav_dnd_hover = None;
        Task::none()
    }

    /// Handle `Message::DndDropNav`.
    ///
    /// Applies a drag-and-drop onto a nav bar location: pastes to a
    /// `Location::Path`, tags to a `Location::Tag`, deletes to
    /// `Location::Trash` (only for moves), and warns otherwise.
    pub fn dnd_drop_nav(
        &mut self,
        entity: Entity,
        data: Option<ClipboardPaste>,
        action: DndAction,
    ) -> Task<Message> {
        self.nav_dnd_hover = None;
        if let Some((location, data)) = self.nav_model.data::<Location>(entity).zip(data) {
            let kind = match action {
                DndAction::Move => ClipboardKind::Cut,
                _ => ClipboardKind::Copy,
            };
            let ret = match location {
                Location::Path(p) => self.update(Message::PasteContents(
                    p.clone(),
                    ClipboardPaste {
                        kind,
                        paths: data.paths,
                    },
                )),
                Location::Tag(t) => self.update(Message::AddTagToContents(
                    t.clone(),
                    ClipboardPaste {
                        kind: ClipboardKind::Copy,
                        paths: data.paths,
                    },
                )),
                Location::Trash if matches!(action, DndAction::Move) => {
                    self.operation(Operation::Delete { paths: data.paths });
                    Task::none()
                }
                _ => {
                    log::warn!("Copy to trash is not supported.");
                    Task::none()
                }
            };
            return ret;
        }
        Task::none()
    }

    /// Handle `Message::DndHoverLocTimeout`.
    ///
    /// If the nav-bar hover has persisted for at least `HOVER_DURATION`,
    /// changes the active tab to the hovered location and rescans it.
    pub fn dnd_hover_loc_timeout(&mut self, location: Location) -> Task<Message> {
        if self
            .nav_dnd_hover
            .as_ref()
            .is_some_and(|(loc, i)| *loc == location && i.elapsed() >= HOVER_DURATION)
        {
            self.nav_dnd_hover = None;
            let entity = self.tab_model.active();
            let title_opt = match self.tab_model.data_mut::<Tab>(entity) {
                Some(tab) => {
                    tab.change_location(&location, None);
                    Some(tab.title())
                }
                None => None,
            };
            if let Some(title) = title_opt {
                self.tab_model.text_set(entity, title);
                return Task::batch([
                    self.update_title(),
                    self.update_watcher(),
                    self.rescan_tab(entity, location, None),
                ]);
            }
        }

        Task::none()
    }

    /// Handle `Message::DndEnterTab`.
    ///
    /// Starts the tab drag-and-drop hover timer for the given entity.
    pub fn dnd_enter_tab(&mut self, entity: Entity) -> Task<Message> {
        self.tab_dnd_hover = Some((entity, Instant::now()));
        Task::perform(tokio::time::sleep(HOVER_DURATION), move |_| {
            cosmic::action::app(Message::DndHoverTabTimeout(entity))
        })
    }

    /// Handle `Message::DndExitTab`.
    ///
    /// Cancels the nav-bar drag-and-drop hover state (this preserves the
    /// original behavior in `app.rs`, which clears the nav hover rather than
    /// the tab hover).
    pub fn dnd_exit_tab(&mut self) -> Task<Message> {
        self.nav_dnd_hover = None;
        Task::none()
    }

    /// Handle `Message::DndDropTab`.
    ///
    /// Applies a drag-and-drop onto a tab: pastes to a `Location::Path` tab,
    /// deletes to a `Location::Trash` tab (only for moves), and warns
    /// otherwise.
    pub fn dnd_drop_tab(
        &mut self,
        entity: Entity,
        data: Option<ClipboardPaste>,
        action: DndAction,
    ) -> Task<Message> {
        self.nav_dnd_hover = None;
        if let Some((tab, data)) = self.tab_model.data::<Tab>(entity).zip(data) {
            let kind = match action {
                DndAction::Move => ClipboardKind::Cut,
                _ => ClipboardKind::Copy,
            };
            let ret = match &tab.location {
                Location::Path(p) => self.update(Message::PasteContents(
                    p.clone(),
                    ClipboardPaste {
                        kind,
                        paths: data.paths,
                    },
                )),
                Location::Trash if matches!(action, DndAction::Move) => {
                    self.operation(Operation::Delete { paths: data.paths });
                    Task::none()
                }
                _ => {
                    log::warn!("Copy to trash is not supported.");
                    Task::none()
                }
            };
            return ret;
        }
        Task::none()
    }

    /// Handle `Message::DndHoverTabTimeout`.
    ///
    /// If the tab hover has persisted for at least `HOVER_DURATION`, activates
    /// the hovered tab.
    pub fn dnd_hover_tab_timeout(&mut self, entity: Entity) -> Task<Message> {
        if self
            .tab_dnd_hover
            .as_ref()
            .is_some_and(|(e, i)| *e == entity && i.elapsed() >= HOVER_DURATION)
        {
            self.tab_dnd_hover = None;
            return self.update(Message::TabActivate(entity));
        }

        Task::none()
    }

    /// Handle `Message::NavBarClose`.
    ///
    /// Unmounts the mounter item represented by the given nav bar entity, if
    /// any.
    pub fn nav_bar_close(&mut self, entity: Entity) -> Task<Message> {
        if let Some(data) = self.nav_model.data::<MounterData>(entity) {
            if let Some(mounter) = MOUNTERS.get(&data.0) {
                return mounter.unmount(data.1.clone()).map(|_| message::none());
            }
        }
        Task::none()
    }

    /// Handle `Message::NavBarContext`.
    ///
    /// Records which nav bar entity should show a context menu and closes the
    /// active tab's location-editing state.
    pub fn nav_bar_context(&mut self, entity: Entity) -> Task<Message> {
        // Close location editing if enabled
        let tab_entity = self.tab_model.active();
        if let Some(tab) = self.tab_model.data_mut::<Tab>(tab_entity) {
            tab.edit_location = None;
        }

        self.nav_bar_context_id = entity;
        Task::none()
    }

    /// Handle `Message::NavMenuAction`.
    ///
    /// Dispatches nav bar context-menu actions: opening a location or tag,
    /// recursively scanning a directory, removing favorites or tags from the
    /// sidebar, emptying the trash, or previewing an item.
    pub fn nav_menu_action(&mut self, action: NavMenuAction) -> Task<Message> {
        match action {
            NavMenuAction::Open(entity) => {
                if let Some(location) = self.nav_model.data::<Location>(entity) {
                    match location {
                        Location::Path(path) => {
                            let _ = self.update(Message::Open(
                                Some(entity),
                                crate::parsers::osstr_to_string(path.clone().into_os_string()),
                            ));
                        }
                        Location::Tag(t) => {
                            let _ = self.update(Message::LaunchSearch(
                                crate::sql::SearchType::Tag,
                                t.tag.clone(),
                            ));
                        }
                        _ => {}
                    }
                }
            }
            NavMenuAction::OpenTag(entity) => {
                if let Some(location) = self.nav_model.data::<Location>(entity) {
                    match location {
                        Location::Tag(t) => {
                            let _ = self.update(Message::LaunchSearch(
                                crate::sql::SearchType::Tag,
                                t.tag.clone(),
                            ));
                        }
                        _ => {}
                    }
                }
            }
            NavMenuAction::RecursiveScanDirectories(entity) => {
                if let Some(location) = self.nav_model.data::<Location>(entity) {
                    match location {
                        Location::Path(p) => {
                            let pathbuf = p.to_path_buf();
                            let sql_connection = self.sql_connection.clone();
                            let _joinhandle = std::thread::spawn(move || {
                                crate::tab::scan_path_recursive(sql_connection, pathbuf)
                            });
                        }
                        _ => {}
                    }
                }
            }
            NavMenuAction::SimilarImageScan(entity) => {
                return self.nav_similar_scan(
                    crate::similar::SimilarKind::Image,
                    entity,
                );
            }
            NavMenuAction::SimilarVideoScan(entity) => {
                return self.nav_similar_scan(
                    crate::similar::SimilarKind::Video,
                    entity,
                );
            }
            NavMenuAction::RemoveFromSidebar(entity) => {
                if let Some(FavoriteIndex(favorite_i)) =
                    self.nav_model.data::<FavoriteIndex>(entity)
                {
                    let mut favorites = self.config.favorites.clone();
                    favorites.remove(*favorite_i);
                    config_set!(self, favorites, favorites);
                    return self.update_config();
                }
            }
            NavMenuAction::RemoveTagFromSidebar(entity) => {
                if let Some(FavoriteIndex(tag_i)) = self.nav_model.data::<FavoriteIndex>(entity) {
                    let mut tags = self.config.tags.clone();
                    crate::sql::delete_tag(self.sql_connection.clone(), tags[*tag_i].tag.clone());
                    tags.remove(*tag_i);
                    config_set!(self, tags, tags);
                    return self.update_config();
                }
            }

            NavMenuAction::EmptyTrash => {
                self.dialog_pages.push_front(DialogPage::EmptyTrash);
            }

            NavMenuAction::Preview(entity) => {
                if let Some(path) = self
                    .nav_model
                    .data::<Location>(entity)
                    .and_then(|location| location.path_opt())
                {
                    match crate::parsers::item_from_path(path, IconSizes::default()) {
                        Ok(item) => {
                            self.context_page =
                                ContextPage::Preview(None, PreviewKind::Custom(PreviewItem(item)));
                            self.set_show_context(true);
                        }
                        Err(err) => {
                            log::warn!("failed to get item from path {:?}: {}", path, err);
                        }
                    }
                }
            }
        }

        Task::none()
    }
}
