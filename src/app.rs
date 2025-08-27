// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

#[cfg(feature = "wayland")]
use cosmic::iced::{
    event::wayland::{Event as WaylandEvent, OutputEvent, OverlapNotifyEvent},
    platform_specific::runtime::wayland::layer_surface::{
        IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
    },
    platform_specific::shell::wayland::commands::layer_surface::{
        destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity, Layer,
    },
    Limits,
};
#[cfg(feature = "wayland")]
use cosmic::iced_winit::commands::overlap_notify::overlap_notify;
use cosmic::{
    app::{self, context_drawer, message, Core, Task},
    cosmic_config, cosmic_theme, executor, font,
    iced::{
        clipboard::dnd::DndAction,
        event,
        futures::{self, SinkExt},
        keyboard::{Event as KeyEvent, Key, Modifiers},
        stream,
        window::{self, Event as WindowEvent, Id as WindowId},
        Alignment, Event, Length, Rectangle, Size, Subscription,
    },
    iced_runtime::clipboard,
    style, theme,
    widget::{
        self,
        dnd_destination::DragId,
        horizontal_space,
        menu::{action::MenuAction, key_bind::KeyBind},
        segmented_button::{self, Entity},
        vertical_space, Container, Slider,
    },
    Application, ApplicationExt, Element,
};
use notify_debouncer_full::{
    new_debouncer,
    notify::{self, RecommendedWatcher, Watcher},
    DebouncedEvent, Debouncer, FileIdMap,
};
use slotmap::Key as SlotMapKey;
use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    env, fmt, fs,
    num::NonZeroU16,
    path::PathBuf,
    process,
    sync::{Arc, Mutex},
    time::{self, Instant},
};
/*
use iced_video_player::{
    gst::{self, prelude::*},
    gst_app, gst_pbutils, Video, VideoPlayer,
};
*/
pub use crate::audio::audio::Audio;
pub use crate::audio::audio_player::AudioPlayer;
pub use crate::video::video::Video;
pub use crate::video::video_player::VideoPlayer;
pub use gstreamer as gst;
use gstreamer::prelude::*;
pub use gstreamer_app as gst_app;
pub use gstreamer_pbutils as gst_pbutils;

use tokio::sync::mpsc;
use trash::TrashItem;
#[cfg(feature = "wayland")]
use wayland_client::{protocol::wl_output::WlOutput, Proxy};

use crate::{
    clipboard::{ClipboardCopy, ClipboardKind, ClipboardPaste},
    config::{AppTheme, Config, IconSizes, MediaFavorite as Favorite, MediaTabConfig as TabConfig},
    fl, home_dir,
    key_bind::key_binds,
    localize::LANGUAGE_SORTER,
    menu, mime_app, mime_icon,
    mounter::{MounterAuth, MounterItem, MounterItems, MounterKey, MounterMessage, MOUNTERS},
    operation::{Controller, Operation, OperationSelection, ReplaceResult},
    spawn_detached::spawn_detached,
    tab::{self, HeadingOptions, ItemMetadata, Location, Tab, HOVER_DURATION},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    App,
    Desktop,
    Browser,
    Image,
    Video,
    Audio,
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: Config,
    pub mode: Mode,
    pub locations: Vec<Location>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    OpenBrowser,
    OpenImage,
    OpenVideo,
    OpenAudio,
    About,
    AddToSidebar,
    AddTagToSidebar,
    AudioMuteToggle,
    Copy,
    Cut,
    CosmicSettingsAppearance,
    CosmicSettingsDisplays,
    CosmicSettingsWallpaper,
    EditHistory,
    EditLocation,
    EmptyTrash,
    Fullscreen,
    HistoryNext,
    HistoryPrevious,
    ItemDown,
    ItemLeft,
    ItemRight,
    ItemUp,
    LocationUp,
    MoveToTrash,
    NewFolder,
    Next,
    Open,
    OpenInNewWindow,
    OpenItemLocation,
    Paste,
    PlayPause,
    PlayFromBeginning,
    //PlaySkip,
    Preview,
    Previous,
    RecursiveScanDirectories,
    Rename,
    RestoreFromTrash,
    SearchActivate,
    SearchDB,
    SeekBackward,
    SeekForward,
    SelectAll,
    SetSort(HeadingOptions, bool),
    Settings,
    //SubtitleToggle,
    TabClose,
    TabNew,
    TabNext,
    TabPrev,
    TabViewGrid,
    TabViewList,
    ToggleFoldersFirst,
    ToggleShowHidden,
    ToggleSort(HeadingOptions),
    WindowClose,
    WindowNew,
    ZoomDefault,
    ZoomIn,
    ZoomOut,
    Recents,
}

impl Action {
    fn message(&self, entity_opt: Option<Entity>) -> Message {
        match self {
            Action::About => Message::ToggleContextPage(ContextPage::About),
            Action::AddToSidebar => Message::AddToSidebar(entity_opt),
            Action::AddTagToSidebar => Message::AddTagToSidebar(entity_opt),
            Action::AudioMuteToggle => Message::AudioMuteToggle,
            Action::OpenBrowser => Message::Browser,
            Action::OpenImage => Message::Image(crate::image::image_view::Message::ToImage),
            Action::OpenVideo => Message::Video(crate::video::video_view::Message::ToVideo),
            Action::OpenAudio => Message::Audio(crate::audio::audio_view::Message::ToAudio),
            Action::Copy => Message::Copy(entity_opt),
            Action::Cut => Message::Cut(entity_opt),
            Action::CosmicSettingsAppearance => Message::CosmicSettings("appearance"),
            Action::CosmicSettingsDisplays => Message::CosmicSettings("displays"),
            Action::CosmicSettingsWallpaper => Message::CosmicSettings("wallpaper"),
            Action::EditHistory => Message::ToggleContextPage(ContextPage::EditHistory),
            Action::EditLocation => {
                Message::TabMessage(entity_opt, tab::Message::EditLocationEnable)
            }
            Action::EmptyTrash => Message::TabMessage(None, tab::Message::EmptyTrash),
            Action::Fullscreen => Message::Fullscreen,
            Action::HistoryNext => Message::TabMessage(entity_opt, tab::Message::GoNext),
            Action::HistoryPrevious => Message::TabMessage(entity_opt, tab::Message::GoPrevious),
            Action::ItemDown => Message::TabMessage(entity_opt, tab::Message::ItemDown),
            Action::ItemLeft => Message::TabMessage(entity_opt, tab::Message::ItemLeft),
            Action::ItemRight => Message::TabMessage(entity_opt, tab::Message::ItemRight),
            Action::ItemUp => Message::TabMessage(entity_opt, tab::Message::ItemUp),
            Action::LocationUp => Message::TabMessage(entity_opt, tab::Message::LocationUp),
            Action::MoveToTrash => Message::MoveToTrash(entity_opt),
            Action::NewFolder => Message::NewItem(entity_opt, true),
            Action::Next => Message::Next(entity_opt),
            Action::Open => {
                Message::Open(entity_opt, dirs::home_dir().unwrap().display().to_string())
            }
            Action::OpenInNewWindow => Message::OpenInNewWindow(entity_opt),
            Action::OpenItemLocation => Message::OpenItemLocation(entity_opt),
            Action::Paste => Message::Paste(entity_opt),
            Action::PlayFromBeginning => Message::Seek(0.0),
            Action::PlayPause => Message::PlayPause,
            Action::Preview => Message::Preview(entity_opt),
            Action::Previous => Message::Previous(entity_opt),
            Action::RecursiveScanDirectories => Message::RecursiveScanDirectories(entity_opt),
            Action::Rename => Message::Rename(entity_opt),
            Action::RestoreFromTrash => Message::RestoreFromTrash(entity_opt),
            Action::SearchActivate => Message::SearchActivate,
            Action::SearchDB => Message::SearchStart,
            Action::SelectAll => Message::TabMessage(entity_opt, tab::Message::SelectAll),
            Action::SetSort(sort, dir) => {
                Message::TabMessage(entity_opt, tab::Message::SetSort(*sort, *dir))
            }
            Action::Settings => Message::ToggleContextPage(ContextPage::Settings),
            Action::SeekBackward => Message::SeekBackward,
            Action::SeekForward => Message::SeekForward,
            //Action::SubtitleToggle => Message::SubtitleToggle,
            Action::TabClose => Message::TabClose(entity_opt),
            Action::TabNew => Message::TabNew,
            Action::TabNext => Message::TabNext,
            Action::TabPrev => Message::TabPrev,
            Action::TabViewGrid => Message::TabView(entity_opt, tab::View::Grid),
            Action::TabViewList => Message::TabView(entity_opt, tab::View::List),
            Action::ToggleFoldersFirst => Message::ToggleFoldersFirst,
            Action::ToggleShowHidden => {
                Message::TabMessage(entity_opt, tab::Message::ToggleShowHidden)
            }
            Action::ToggleSort(sort) => {
                Message::TabMessage(entity_opt, tab::Message::ToggleSort(*sort))
            }
            Action::WindowClose => Message::WindowClose,
            Action::WindowNew => Message::WindowNew,
            Action::ZoomDefault => Message::ZoomDefault(entity_opt),
            Action::ZoomIn => Message::ZoomIn(entity_opt),
            Action::ZoomOut => Message::ZoomOut(entity_opt),
            Action::Recents => Message::Recents,
        }
    }
}

impl MenuAction for Action {
    type Message = Message;

    fn message(&self) -> Message {
        self.message(None)
    }
}

#[derive(Clone, Debug)]
pub struct PreviewItem(pub tab::Item);

impl PartialEq for PreviewItem {
    fn eq(&self, other: &Self) -> bool {
        self.0.location_opt == other.0.location_opt
    }
}

impl Eq for PreviewItem {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewKind {
    Custom(PreviewItem),
    Location(Location),
    Selected,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum NavMenuAction {
    Open(segmented_button::Entity),
    OpenTag(segmented_button::Entity),
    Preview(segmented_button::Entity),
    RecursiveScanDirectories(segmented_button::Entity),
    RemoveFromSidebar(segmented_button::Entity),
    RemoveTagFromSidebar(segmented_button::Entity),
    EmptyTrash,
}

impl MenuAction for NavMenuAction {
    type Message = cosmic::app::Message<Message>;

    fn message(&self) -> Self::Message {
        cosmic::app::Message::App(Message::NavMenuAction(*self))
    }
}

/// Messages that are used specifically by our [`App`].
#[derive(Clone, Debug)]
pub enum Message {
    Browser,
    Image(crate::image::image_view::Message),
    Video(crate::video::video_view::Message),
    Audio(crate::audio::audio_view::Message),
    AddToSidebar(Option<Entity>),
    AddTagToContents(crate::sql::Tag, ClipboardPaste),
    AddTagToSidebar(Option<Entity>),
    AppTheme(AppTheme),
    AudioMessage(crate::audio::audio_view::Message),
    AudioMuteToggle,
    AudioCode(usize),
    AudioVolume(f64),
    //SubtitleToggle,
    TextCode(usize),
    MissingPlugin(gstreamer::Message),
    Fullscreen,
    Seek(f64),
    SeekRelative(f64),
    EndOfStream,
    NewFrame,
    PlayPause,
    Chapter(usize),
    CloseToast(widget::ToastId),
    Config(Config),
    Copy(Option<Entity>),
    CosmicSettings(&'static str),
    Cut(Option<Entity>),
    DialogCancel,
    DialogComplete,
    DialogPush(DialogPage),
    DialogUpdate(DialogPage),
    DialogUpdateComplete(DialogPage),
    ImageMessage(crate::image::image_view::Message),
    Key(Modifiers, Key),
    LaunchUrl(String),
    LaunchSearch(crate::sql::SearchType, String),
    MaybeExit,
    MetadataDelete,
    Modifiers(Modifiers),
    MoveToTrash(Option<Entity>),
    MounterItems(MounterKey, MounterItems),
    MountResult(MounterKey, MounterItem, Result<bool, String>),
    MouseScroll(cosmic::iced_core::mouse::ScrollDelta),
    NavBarClose(Entity),
    NavBarContext(Entity),
    NavMenuAction(NavMenuAction),
    NetworkAuth(MounterKey, String, MounterAuth, mpsc::Sender<MounterAuth>),
    NetworkDriveInput(String),
    NetworkDriveSubmit,
    NetworkResult(MounterKey, String, Result<bool, String>),
    NewItem(Option<Entity>, bool),
    Next(Option<Entity>),
    Previous(Option<Entity>),
    #[cfg(feature = "notify")]
    Notification(Arc<Mutex<notify_rust::NotificationHandle>>),
    NotifyEvents(Vec<DebouncedEvent>),
    NotifyWatcher(WatcherWrapper),
    Open(Option<Entity>, String),
    OpenInNewWindow(Option<Entity>),
    OpenItemLocation(Option<Entity>),
    Paste(Option<Entity>),
    PasteContents(PathBuf, ClipboardPaste),
    PendingCancel(u64),
    PendingCancelAll,
    PendingComplete(u64, OperationSelection),
    PendingDismiss,
    PendingError(u64, String),
    PendingPause(u64, bool),
    PendingPauseAll(bool),
    Preview(Option<Entity>),
    RecursiveScanDirectories(Option<Entity>),
    RescanTrash,
    Rename(Option<Entity>),
    RenameWithPattern(Option<Entity>, String, i32, i32),
    ReplaceResult(ReplaceResult),
    RestoreFromTrash(Option<Entity>),
    SearchActivate,
    SearchClear,
    SearchInput(String),
    SearchStart,
    SearchPreviousPick(usize),
    SearchPreviousSelect,
    SearchPreviousDelete,
    SearchImages(bool),
    SearchVideos(bool),
    SearchAudios(bool),
    SearchSearchString(String),
    SearchSearchStringSubmit,
    SearchSearchFromString(String),
    SearchSearchFromStringSubmit,
    SearchSearchToString(String),
    SearchSearchToStringSubmit,
    SearchSearchFromValue(String),
    SearchSearchFromValueSubmit,
    SearchSearchToValue(String),
    SearchSearchToValueSubmit,
    SearchFilepath(bool),
    SearchTitle(bool),
    SearchTag(bool),
    SearchDescription(bool),
    SearchActor(bool),
    SearchDirector(bool),
    SearchArtist(bool),
    SearchAlbumartist(bool),
    SearchAlbum(bool),
    SearchComposer(bool),
    SearchGenre(bool),
    SearchDuration(bool),
    SearchCreationDate(bool),
    SearchModificationDate(bool),
    SearchReleaseDate(bool),
    SearchLenseModel(bool),
    SearchFocalLength(bool),
    SearchExposureTime(bool),
    SearchFNumber(bool),
    SearchGpsLatitude(bool),
    SearchGpsLongitude(bool),
    SearchGpsAltitude(bool),
    SearchCommit,
    SeekBackward,
    SeekForward,
    SetShowDetails(bool),
    SkipToPosition(f64),
    SystemThemeModeChange(cosmic_theme::ThemeMode),
    Size(Size),
    TabActivate(Entity),
    TabNext,
    TabPrev,
    TabClose(Option<Entity>),
    TabConfig(TabConfig),
    TabMessage(Option<Entity>, tab::Message),
    TabNew,
    TabRescan(
        Entity,
        Location,
        Option<tab::Item>,
        Vec<tab::Item>,
        Option<Vec<PathBuf>>,
    ),
    TabView(Option<Entity>, tab::View),
    ToggleContextPage(ContextPage),
    ToggleFoldersFirst,
    Undo(usize),
    UndoTrash(widget::ToastId, Arc<[PathBuf]>),
    UndoTrashStart(Vec<TrashItem>),
    VideoMessage(crate::video::video_view::Message),
    WindowClose,
    WindowNew,
    ZoomDefault(Option<Entity>),
    ZoomIn(Option<Entity>),
    ZoomOut(Option<Entity>),
    DndHoverLocTimeout(Location),
    DndHoverTabTimeout(Entity),
    DndEnterNav(Entity),
    DndExitNav,
    DndEnterTab(Entity),
    DndExitTab,
    DndDropTab(Entity, Option<ClipboardPaste>, DndAction),
    DndDropNav(Entity, Option<ClipboardPaste>, DndAction),
    Recents,
    #[cfg(feature = "wayland")]
    OutputEvent(OutputEvent, WlOutput),
    Cosmic(app::cosmic::Message),
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextPage {
    About,
    EditHistory,
    NetworkDrive,
    Preview(Option<Entity>, PreviewKind),
    Settings,
    Search,
}

#[derive(Clone, Debug)]
pub enum DialogPage {
    EmptyTrash,
    FailedOperation(u64),
    MountError {
        mounter_key: MounterKey,
        item: MounterItem,
        error: String,
    },
    NetworkAuth {
        mounter_key: MounterKey,
        uri: String,
        auth: MounterAuth,
        auth_tx: mpsc::Sender<MounterAuth>,
    },
    NetworkError {
        mounter_key: MounterKey,
        uri: String,
        error: String,
    },
    NewItem {
        parent: PathBuf,
        name: String,
        dir: bool,
    },
    NewTag {
        tag: String,
    },
    OpenWith {
        path: PathBuf,
        mime: mime_guess::Mime,
        apps: Vec<mime_app::MimeApp>,
        selected: usize,
        store_opt: Option<mime_app::MimeApp>,
    },
    RenameItem {
        from: PathBuf,
        parent: PathBuf,
        name: String,
        dir: bool,
    },
    Replace {
        from: tab::Item,
        to: tab::Item,
        multiple: bool,
        apply_to_all: bool,
        tx: mpsc::Sender<ReplaceResult>,
    },
}

pub struct FavoriteIndex(usize);

pub struct MounterData(MounterKey, MounterItem);

#[derive(Clone, Debug)]
pub enum WindowKind {
    Desktop(Entity),
    Preview(Option<Entity>, PreviewKind),
}

pub struct WatcherWrapper {
    watcher_opt: Option<Debouncer<RecommendedWatcher, FileIdMap>>,
}

impl Clone for WatcherWrapper {
    fn clone(&self) -> Self {
        Self { watcher_opt: None }
    }
}

impl fmt::Debug for WatcherWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WatcherWrapper").finish()
    }
}

impl PartialEq for WatcherWrapper {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

/// The [`App`] stores application-specific state.
pub struct App {
    image_view: crate::image::image_view::ImageView,
    video_view: crate::video::video_view::VideoView,
    audio_view: crate::audio::audio_view::AudioView,
    active_view: Mode,
    core: Core,
    nav_bar_context_id: segmented_button::Entity,
    nav_model: segmented_button::SingleSelectModel,
    tab_model: segmented_button::Model<segmented_button::SingleSelect>,
    tab_model_id: Entity,
    config_handler: Option<cosmic_config::Config>,
    config: Config,
    mode: Mode,
    app_themes: Vec<String>,
    context_page: ContextPage,
    dialog_pages: VecDeque<DialogPage>,
    dialog_text_input: widget::Id,
    key_binds: HashMap<KeyBind, Action>,
    margin: HashMap<window::Id, (f32, f32, f32, f32)>,
    modifiers: Modifiers,
    mounter_items: HashMap<MounterKey, MounterItems>,
    network_drive_connecting: Option<(MounterKey, String)>,
    network_drive_input: String,
    #[cfg(feature = "notify")]
    notification_opt: Option<Arc<Mutex<notify_rust::NotificationHandle>>>,
    _overlap: HashMap<String, (window::Id, Rectangle)>,
    pending_operation_id: u64,
    pending_operations: BTreeMap<u64, (Operation, Controller)>,
    progress_operations: BTreeSet<u64>,
    complete_operations: BTreeMap<u64, Operation>,
    failed_operations: BTreeMap<u64, (Operation, Controller, String)>,
    search_id: widget::Id,
    search: crate::sql::SearchData,
    search_previous: Vec<crate::sql::SearchData>,
    search_previous_str: Vec<String>,
    search_previous_pos: usize,
    search_from_string: widget::Id,
    search_to_string: widget::Id,
    size: Option<Size>,
    #[cfg(feature = "wayland")]
    surface_ids: HashMap<WlOutput, WindowId>,
    #[cfg(feature = "wayland")]
    surface_names: HashMap<WindowId, String>,
    toasts: widget::toaster::Toasts<Message>,
    watcher_opt: Option<(Debouncer<RecommendedWatcher, FileIdMap>, HashSet<PathBuf>)>,
    window_id_opt: Option<window::Id>,
    windows: HashMap<window::Id, WindowKind>,
    nav_dnd_hover: Option<(Location, Instant)>,
    tab_dnd_hover: Option<(Entity, Instant)>,
    nav_drag_id: DragId,
    tab_drag_id: DragId,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl App {
    fn open_tab(
        &mut self,
        location: Location,
        activate: bool,
        selection_paths: Option<Vec<PathBuf>>,
    ) -> Task<Message> {
        self.open_tab_entity(location, activate, selection_paths).1
    }

    fn open_tab_entity(
        &mut self,
        location: Location,
        activate: bool,
        selection_paths: Option<Vec<PathBuf>>,
    ) -> (Entity, Task<Message>) {
        let mut tab = Tab::new(location.clone(), self.config.tab);
        tab.mode = match self.mode {
            Mode::App => tab::Mode::App,
            Mode::Desktop => {
                tab.config.view = tab::View::Grid;
                tab::Mode::Desktop
            }
            Mode::Browser => tab::Mode::App,
            Mode::Image => tab::Mode::App,
            Mode::Video => tab::Mode::App,
            Mode::Audio => tab::Mode::App,
        };
        let entity = self
            .tab_model
            .insert()
            .text(tab.title())
            .data(tab)
            .closable();

        let entity = if activate {
            entity.activate().id()
        } else {
            entity.id()
        };

        (
            entity,
            Task::batch([
                self.update_title(),
                self.update_watcher(),
                self.rescan_tab(entity, location, selection_paths),
            ]),
        )
    }

    fn operation(&mut self, operation: Operation) {
        let id = self.pending_operation_id;
        self.pending_operation_id += 1;
        if operation.show_progress_notification() {
            self.progress_operations.insert(id);
        }
        self.pending_operations
            .insert(id, (operation, Controller::new()));
    }

    fn rescan_operation_selection(&mut self, op_sel: OperationSelection) -> Task<Message> {
        log::info!("rescan_operation_selection {:?}", op_sel);
        let entity = self.tab_model.active();
        let Some(tab) = self.tab_model.data::<Tab>(entity) else {
            return Task::none();
        };
        let Some(ref items) = tab.items_opt() else {
            return Task::none();
        };
        for item in items.iter() {
            if item.selected {
                if let Some(path) = item.path_opt() {
                    if op_sel.selected.contains(path) || op_sel.ignored.contains(path) {
                        // Ignore if path in selected or ignored paths
                        continue;
                    }
                }

                // Return if there is a previous selection not matching
                return Task::none();
            }
        }
        self.rescan_tab(entity, tab.location.clone(), Some(op_sel.selected))
    }

    fn rescan_tab(
        &mut self,
        entity: Entity,
        location: Location,
        selection_paths: Option<Vec<PathBuf>>,
    ) -> Task<Message> {
        log::info!("rescan_tab {entity:?} {location:?} {selection_paths:?}");
        let icon_sizes = self.config.tab.icon_sizes;
        let sql_connection = self.sql_connection.clone();
        Task::perform(
            async move {
                let location2 = location.clone();
                match tokio::task::spawn_blocking(move || {
                    location2.scan(sql_connection, icon_sizes)
                })
                .await
                {
                    Ok((parent_item_opt, items)) => message::app(Message::TabRescan(
                        entity,
                        location,
                        parent_item_opt,
                        items,
                        selection_paths,
                    )),
                    Err(err) => {
                        log::warn!("failed to rescan: {}", err);
                        message::none()
                    }
                }
            },
            |x| x,
        )
    }

    fn rescan_trash(&mut self) -> Task<Message> {
        let mut needs_reload = Vec::new();
        for entity in self.tab_model.iter() {
            if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                if let Location::Trash = &tab.location {
                    needs_reload.push((entity, Location::Trash));
                }
            }
        }

        let mut commands = Vec::with_capacity(needs_reload.len());
        for (entity, location) in needs_reload {
            commands.push(self.rescan_tab(entity, location, None));
        }
        Task::batch(commands)
    }

    fn search(&mut self) -> Task<Message> {
        if let Some(term) = self.search_get() {
            self.search_set(Some(term.to_string()))
        } else {
            Task::none()
        }
    }

    fn search_get(&self) -> Option<&str> {
        let entity = self.tab_model.active();
        let tab = self.tab_model.data::<Tab>(entity)?;
        match &tab.location {
            Location::Search(_, term, ..) => Some(term),
            _ => None,
        }
    }

    fn search_set(&mut self, term_opt: Option<String>) -> Task<Message> {
        let entity = self.tab_model.active();
        let mut title_location_opt = None;
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            let location_opt = match term_opt {
                Some(term) => match &tab.location {
                    Location::Path(path) | Location::Search(path, ..) => Some((
                        Location::Search(
                            path.to_path_buf(),
                            term,
                            tab.config.show_hidden,
                            Instant::now(),
                        ),
                        true,
                    )),
                    _ => None,
                },
                None => match &tab.location {
                    Location::Search(path, ..) => Some((Location::Path(path.to_path_buf()), false)),
                    _ => None,
                },
            };
            if let Some((location, focus_search)) = location_opt {
                tab.change_location(&location, None);
                title_location_opt = Some((tab.title(), tab.location.clone(), focus_search));
            }
        }
        if let Some((title, location, focus_search)) = title_location_opt {
            self.tab_model.text_set(entity, title);
            return Task::batch([
                self.update_title(),
                self.update_watcher(),
                self.rescan_tab(entity, location, None),
                if focus_search {
                    widget::text_input::focus(self.search_id.clone())
                } else {
                    Task::none()
                },
            ]);
        }
        Task::none()
    }

    fn selected_paths(&self, entity_opt: Option<Entity>) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
        if let Some(tab) = self.tab_model.data::<Tab>(entity) {
            for location in tab.selected_locations() {
                if let Some(path) = location.path_opt() {
                    paths.push(path.to_path_buf());
                }
            }
        }
        paths
    }

    fn update_config(&mut self) -> Task<Message> {
        self.update_nav_model();
        // Tabs are collected first to placate the borrowck
        let tabs: Vec<_> = self.tab_model.iter().collect();
        // Update main conf and each tab with the new config
        let commands: Vec<_> = std::iter::once(cosmic::app::command::set_theme(
            self.config.app_theme.theme(),
        ))
        .chain(tabs.into_iter().map(|entity| {
            self.update(Message::TabMessage(
                Some(entity),
                tab::Message::Config(self.config.tab),
            ))
        }))
        .collect();
        Task::batch(commands)
    }

    fn activate_nav_model_location(&mut self, location: &Location) {
        let nav_bar_id = self.nav_model.iter().find(|&id| {
            self.nav_model
                .data::<Location>(id)
                .map(|l| l == location)
                .unwrap_or_default()
        });

        if let Some(id) = nav_bar_id {
            self.nav_model.activate(id);
        } else {
            let active = self.nav_model.active();
            segmented_button::Selectable::deactivate(&mut self.nav_model, active);
        }
    }

    fn update_nav_model(&mut self) {
        let mut nav_model = segmented_button::ModelBuilder::default();

        nav_model = nav_model.insert(|b| {
            b.text(fl!("recents"))
                .icon(widget::icon::from_name("document-open-recent-symbolic"))
                .data(Location::Recents)
        });

        for (favorite_i, favorite) in self.config.favorites.iter().enumerate() {
            if let Some(path) = favorite.path_opt() {
                let name = if matches!(favorite, Favorite::Home) {
                    fl!("home")
                } else if let Some(file_name) = path.file_name().and_then(|x| x.to_str()) {
                    file_name.to_string()
                } else {
                    fl!("filesystem")
                };
                nav_model = nav_model.insert(move |b| {
                    b.text(name.clone())
                        .icon(
                            widget::icon::icon(if path.is_dir() {
                                tab::folder_icon_symbolic(&path, 16)
                            } else {
                                widget::icon::from_name("text-x-generic-symbolic")
                                    .size(16)
                                    .handle()
                            })
                            .size(16),
                        )
                        .data(Location::Path(path.clone()))
                        .data(FavoriteIndex(favorite_i))
                });
            }
        }

        nav_model = nav_model.insert(|b| {
            b.text(fl!("trash"))
                .icon(widget::icon::icon(tab::trash_icon_symbolic(16)))
                .data(Location::Trash)
                .divider_above()
        });

        if !MOUNTERS.is_empty() {
            nav_model = nav_model.insert(|b| {
                b.text(fl!("networks"))
                    .icon(widget::icon::icon(
                        widget::icon::from_name("network-workgroup-symbolic")
                            .size(16)
                            .handle(),
                    ))
                    .data(Location::Network(
                        "network:///".to_string(),
                        fl!("networks"),
                    ))
                    .divider_above()
            });
        }

        // Collect all mounter items
        let mut nav_items = Vec::new();
        for (key, items) in self.mounter_items.iter() {
            for item in items.iter() {
                nav_items.push((*key, item));
            }
        }
        // Sort by name lexically
        nav_items.sort_by(|a, b| LANGUAGE_SORTER.compare(&a.1.name(), &b.1.name()));
        // Add items to nav model
        for (i, (key, item)) in nav_items.into_iter().enumerate() {
            nav_model = nav_model.insert(|mut b| {
                b = b.text(item.name()).data(MounterData(key, item.clone()));
                if let Some(path) = item.path() {
                    b = b.data(Location::Path(path.clone()));
                }
                if let Some(icon) = item.icon(true) {
                    b = b.icon(widget::icon::icon(icon).size(16));
                }
                if item.is_mounted() {
                    b = b.closable();
                }
                if i == 0 {
                    b = b.divider_above();
                }
                b
            });
        }
        for (tag_i, tag) in self.config.tags.iter().enumerate() {
            if tag.tag.len() > 0 {
                let name = tag.tag.clone();
                nav_model = nav_model.insert(move |b| {
                    b.text(name.clone())
                        .data(Location::Tag(tag.to_owned()))
                        .data(FavoriteIndex(tag_i))
                });
            }
        }

        self.nav_model = nav_model.build();

        let tab_entity = self.tab_model.active();
        self.tab_model_id = tab_entity.clone();
        if let Some(tab) = self.tab_model.data::<Tab>(tab_entity) {
            self.activate_nav_model_location(&tab.location.clone());
        }
    }

    fn update_notification(&mut self) -> Task<Message> {
        // Handle closing notification if there are no operations
        if self.pending_operations.is_empty() {
            #[cfg(feature = "notify")]
            if let Some(notification_arc) = self.notification_opt.take() {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            //TODO: this is nasty
                            let notification_mutex = Arc::try_unwrap(notification_arc).unwrap();
                            let notification = notification_mutex.into_inner().unwrap();
                            notification.close();
                        })
                        .await
                        .unwrap();
                        message::app(Message::MaybeExit)
                    },
                    |x| x,
                );
            }
        }

        Task::none()
    }

    fn update_title(&mut self) -> Task<Message> {
        let window_title = match self.tab_model.text(self.tab_model.active()) {
            Some(tab_title) => format!("{tab_title} — {}", fl!("media-browser")),
            None => fl!("media-browser"),
        };
        if let Some(window_id) = &self.window_id_opt {
            self.set_window_title(window_title, *window_id)
        } else {
            Task::none()
        }
    }

    fn update_watcher(&mut self) -> Task<Message> {
        if let Some((mut watcher, old_paths)) = self.watcher_opt.take() {
            let mut new_paths = HashSet::new();
            for entity in self.tab_model.iter() {
                if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                    if let Some(path) = tab.location.path_opt() {
                        new_paths.insert(path.to_path_buf());
                    }
                }
            }

            // Unwatch paths no longer used
            for path in old_paths.iter() {
                if !new_paths.contains(path) {
                    match watcher.watcher().unwatch(path) {
                        Ok(()) => {
                            log::debug!("unwatching {:?}", path);
                        }
                        Err(err) => {
                            log::debug!("failed to unwatch {:?}: {}", path, err);
                        }
                    }
                }
            }

            // Watch new paths
            for path in new_paths.iter() {
                if !old_paths.contains(path) {
                    //TODO: should this be recursive?
                    match watcher
                        .watcher()
                        .watch(path, notify::RecursiveMode::NonRecursive)
                    {
                        Ok(()) => {
                            log::debug!("watching {:?}", path);
                        }
                        Err(err) => {
                            log::debug!("failed to watch {:?}: {}", path, err);
                        }
                    }
                }
            }

            self.watcher_opt = Some((watcher, new_paths));
        }

        //TODO: should any of this run in a command?
        Task::none()
    }

    fn about(&self) -> Element<'_, Message> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
        let repository = "https://github.com/fangornsrealm/media-browser";
        let hash = env!("VERGEN_GIT_SHA");
        let short_hash: String = hash.chars().take(7).collect();
        let date = env!("VERGEN_GIT_COMMIT_DATE");
        widget::column::with_children(vec![
            widget::svg(widget::svg::Handle::from_memory(
                &include_bytes!(
                    "../res/icons/hicolor/128x128/apps/eu.fangornsrealm.MediaBrowser.svg"
                )[..],
            ))
            .into(),
            widget::text::title3(fl!("media-browser")).into(),
            widget::button::link(repository)
                .on_press(Message::LaunchUrl(repository.to_string()))
                .padding(0)
                .into(),
            widget::button::link(fl!(
                "git-description",
                hash = short_hash.as_str(),
                date = date
            ))
            .on_press(Message::LaunchUrl(format!(
                "{}/commits/{}",
                repository, hash
            )))
            .padding(0)
            .into(),
        ])
        .align_x(Alignment::Center)
        .spacing(space_xxs)
        .into()
    }

    fn network_drive(&self) -> Element<'_, Message> {
        let cosmic_theme::Spacing {
            space_xxs, space_m, ..
        } = theme::active().cosmic().spacing;
        let mut table = widget::column::with_capacity(8);
        for (i, line) in fl!("network-drive-schemes").lines().enumerate() {
            let mut row = widget::row::with_capacity(2);
            for part in line.split(',') {
                row = row.push(
                    widget::container(if i == 0 {
                        widget::text::heading(part.to_string())
                    } else {
                        widget::text::body(part.to_string())
                    })
                    .width(Length::Fill)
                    .padding(space_xxs),
                );
            }
            table = table.push(row);
            if i == 0 {
                table = table.push(widget::divider::horizontal::light());
            }
        }
        widget::column::with_children(vec![
            widget::text::body(fl!("network-drive-description")).into(),
            table.into(),
        ])
        .spacing(space_m)
        .into()
    }

    fn _open_with(&self) -> Element<'_, Message> {
        let children = Vec::new();
        let entity = self.tab_model.active();
        if let Some(tab) = self.tab_model.data::<Tab>(entity) {
            if let Some(items) = tab.items_opt() {
                for item in items.iter() {
                    if item.selected {
                        //children.push(item.open(tab.config.icon_sizes));
                        // Only show one property view to avoid issues like hangs when generating
                        // preview images on thousands of files
                        break;
                    }
                }
            }
        }
        widget::settings::view_column(children).into()
    }

    fn edit_history(&self) -> Element<'_, Message> {
        let cosmic_theme::Spacing { space_m, .. } = theme::active().cosmic().spacing;

        let mut children = Vec::new();

        //TODO: get height from theme?
        let progress_bar_height = Length::Fixed(4.0);

        if !self.pending_operations.is_empty() {
            let mut section = widget::settings::section().title(fl!("pending"));
            for (id, (op, controller)) in self.pending_operations.iter().rev() {
                let progress = controller.progress();
                section = section.add(widget::column::with_children(vec![
                    widget::row::with_children(vec![
                        widget::progress_bar(0.0..=1.0, progress)
                            .height(progress_bar_height)
                            .into(),
                        if controller.is_paused() {
                            widget::tooltip(
                                widget::button::icon(widget::icon::from_name(
                                    "media-playback-start-symbolic",
                                ))
                                .on_press(Message::PendingPause(*id, false))
                                .padding(8),
                                widget::text::body("resume".to_string()),
                                widget::tooltip::Position::Top,
                            )
                            .into()
                        } else {
                            widget::tooltip(
                                widget::button::icon(widget::icon::from_name(
                                    "media-playback-pause-symbolic",
                                ))
                                .on_press(Message::PendingPause(*id, true))
                                .padding(8),
                                widget::text::body("pause".to_string()),
                                widget::tooltip::Position::Top,
                            )
                            .into()
                        },
                        widget::tooltip(
                            widget::button::icon(widget::icon::from_name("window-close-symbolic"))
                                .on_press(Message::PendingCancel(*id))
                                .padding(8),
                            widget::text::body(fl!("cancel")),
                            widget::tooltip::Position::Top,
                        )
                        .into(),
                    ])
                    .align_y(Alignment::Center)
                    .into(),
                    widget::text::body(op.pending_text(progress, controller.state())).into(),
                ]));
            }
            children.push(section.into());
        }

        if !self.failed_operations.is_empty() {
            let mut section = widget::settings::section().title(fl!("failed"));
            for (_id, (op, controller, error)) in self.failed_operations.iter().rev() {
                let progress = controller.progress();
                section = section.add(widget::column::with_children(vec![
                    widget::text::body(op.pending_text(progress, controller.state())).into(),
                    widget::text::body(error).into(),
                ]));
            }
            children.push(section.into());
        }

        if !self.complete_operations.is_empty() {
            let mut section = widget::settings::section().title(fl!("complete"));
            for (_id, op) in self.complete_operations.iter().rev() {
                section = section.add(widget::text::body(op.completed_text()));
            }
            children.push(section.into());
        }

        if children.is_empty() {
            children.push(widget::text::body(fl!("no-history")).into());
        }

        widget::column::with_children(children)
            .spacing(space_m)
            .into()
    }

    fn preview<'a>(
        &'a self,
        entity_opt: &Option<Entity>,
        kind: &'a PreviewKind,
        context_drawer: bool,
    ) -> Element<'a, Message> {
        let cosmic_theme::Spacing { space_l, .. } = theme::active().cosmic().spacing;

        let mut children = Vec::with_capacity(1);
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
        match kind {
            PreviewKind::Custom(PreviewItem(item)) => {
                children.push(item.preview_view(IconSizes::default()));
            }
            PreviewKind::Location(location) => {
                if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                    if let Some(items) = tab.items_opt() {
                        for item in items.iter() {
                            if item.location_opt.as_ref() == Some(location) {
                                children.push(item.preview_view(tab.config.icon_sizes));
                                // Only show one property view to avoid issues like hangs when generating
                                // preview images on thousands of files
                                break;
                            }
                        }
                    }
                }
            }
            PreviewKind::Selected => {
                if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                    if let Some(items) = tab.items_opt() {
                        for item in items.iter() {
                            if item.selected {
                                children.push(item.preview_view(tab.config.icon_sizes));
                                // Only show one property view to avoid issues like hangs when generating
                                // preview images on thousands of files
                                break;
                            }
                        }
                        if children.is_empty() {
                            if let Some(item) = &tab.parent_item_opt {
                                children.push(item.preview_view(tab.config.icon_sizes));
                            }
                        }
                    }
                }
            }
        }
        widget::column::with_children(children)
            .padding(if context_drawer {
                [0, 0, 0, 0]
            } else {
                [0, space_l, space_l, space_l]
            })
            .into()
    }

    fn search_database(&self) -> Element<'_, Message> {
        let cosmic_theme::Spacing { space_m, .. } = theme::active().cosmic().spacing;

        let mut column = widget::column().spacing(space_m);
        column = column.push(widget::text::heading(fl!("search-previous")));
        column = column.push(widget::row::with_children(vec![
            widget::dropdown(
                &self.search_previous_str,
                usize::try_from(self.search_previous_pos).ok(),
                Message::SearchPreviousPick,
            )
            .into(),
            widget::horizontal_space().into(),
            widget::tooltip(
                widget::button::icon(widget::icon::from_name("checkbox-checked-symbolic").size(16))
                    .on_press(Message::SearchPreviousSelect),
                widget::text::body(fl!("search-select")),
                widget::tooltip::Position::Top,
            )
            .into(),
            widget::tooltip(
                widget::button::icon(widget::icon::from_name("edit-delete-symbolic").size(16))
                    .on_press(Message::SearchPreviousDelete),
                widget::text::body(fl!("search-delete")),
                widget::tooltip::Position::Top,
            )
            .into(),
        ]));

        column = column.push(widget::text::heading(fl!("search-mediatypes")));
        column = column.push(widget::row::with_children(vec![
            widget::checkbox(fl!("search-images"), self.search.image)
                .on_toggle(|b| Message::SearchImages(b))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-videos"), self.search.video)
                .on_toggle(|b| Message::SearchVideos(b))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-audios"), self.search.audio)
                .on_toggle(|b| Message::SearchAudios(b))
                .into(),
        ]));
        column = column.push(widget::text::heading(fl!("search-textentry")));
        column = column.push(widget::row::with_children(vec![
            widget::text::heading(fl!("search-text-from")).into(),
            widget::horizontal_space().into(),
            widget::text::heading(fl!("search-text-to")).into(),
        ]));
        column = column.push(widget::row::with_children(vec![
            widget::tooltip(
                widget::text_input("".to_string(), self.search.from_string.as_str())
                    .id(self.search_from_string.clone())
                    .on_input(Message::SearchSearchFromString)
                    .on_submit(Message::SearchSearchFromStringSubmit),
                widget::text::body(fl!("search-tooltip-date")),
                widget::tooltip::Position::Top,
            )
            .into(),
            widget::tooltip(
                widget::text_input("".to_string(), self.search.to_string.as_str())
                    .id(self.search_to_string.clone())
                    .on_input(Message::SearchSearchToString)
                    .on_submit(Message::SearchSearchToStringSubmit),
                widget::text::body(fl!("search-tooltip-date")),
                widget::tooltip::Position::Top,
            )
            .into(),
        ]));
        column = column.push(widget::row::with_children(vec![
            widget::checkbox(fl!("search-filepath"), self.search.filepath)
                .on_toggle(move |value| Message::SearchFilepath(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-title"), self.search.title)
                .on_toggle(move |value| Message::SearchTitle(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-tag"), self.search.tags)
                .on_toggle(move |value| Message::SearchTag(value))
                .into(),
        ]));
        column = column.push(widget::row::with_children(vec![
            widget::checkbox(fl!("search-description"), self.search.description)
                .on_toggle(move |value| Message::SearchDescription(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-actor"), self.search.actor)
                .on_toggle(move |value| Message::SearchActor(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-director"), self.search.director)
                .on_toggle(move |value| Message::SearchDirector(value))
                .into(),
        ]));
        column = column.push(widget::row::with_children(vec![
            widget::checkbox(fl!("search-album"), self.search.album)
                .on_toggle(move |value| Message::SearchAlbum(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-composer"), self.search.composer)
                .on_toggle(move |value| Message::SearchComposer(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-genre"), self.search.genre)
                .on_toggle(move |value| Message::SearchGenre(value))
                .into(),
        ]));
        column = column.push(widget::row::with_children(vec![
            widget::checkbox(fl!("search-artist"), self.search.artist)
                .on_toggle(move |value| Message::SearchArtist(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-album_artist"), self.search.album_artist)
                .on_toggle(move |value| Message::SearchAlbumartist(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-duration"), self.search.duration)
                .on_toggle(move |value| Message::SearchDuration(value))
                .into(),
        ]));
        column = column.push(widget::row::with_children(vec![
            widget::checkbox(fl!("search-creation_date"), self.search.creation_date)
                .on_toggle(move |value| Message::SearchCreationDate(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(
                fl!("search-modification_date"),
                self.search.modification_date,
            )
            .on_toggle(move |value| Message::SearchModificationDate(value))
            .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-release_date"), self.search.release_date)
                .on_toggle(move |value| Message::SearchReleaseDate(value))
                .into(),
        ]));
        column = column.push(widget::row::with_children(vec![
            widget::checkbox(fl!("search-lense_model"), self.search.lense_model)
                .on_toggle(move |value| Message::SearchLenseModel(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-focal_length"), self.search.focal_length)
                .on_toggle(move |value| Message::SearchFocalLength(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-exposure_time"), self.search.exposure_time)
                .on_toggle(move |value| Message::SearchExposureTime(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-fnumber"), self.search.fnumber)
                .on_toggle(move |value| Message::SearchFNumber(value))
                .into(),
        ]));
        column = column.push(widget::row::with_children(vec![
            widget::checkbox(fl!("search-gps_latitude"), self.search.gps_latitude)
                .on_toggle(move |value| Message::SearchGpsLatitude(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-gps_longitude"), self.search.gps_longitude)
                .on_toggle(move |value| Message::SearchGpsLongitude(value))
                .into(),
            widget::horizontal_space().into(),
            widget::checkbox(fl!("search-gps_altitude"), self.search.gps_altitude)
                .on_toggle(move |value| Message::SearchGpsAltitude(value))
                .into(),
        ]));
        column = column.push(widget::tooltip(
            widget::button::icon(widget::icon::from_name("media-playback-start-symbolic"))
                .on_press(Message::SearchCommit)
                .padding(8),
            widget::text::body(fl!("search-commit")),
            widget::tooltip::Position::Top,
        ));
        widget::column::with_children(vec![
            widget::text::body(fl!("search-context")).into(),
            column.into(),
        ])
        .spacing(space_m)
        .into()
    }

    fn settings(&self) -> Element<'_, Message> {
        // TODO: Should dialog be updated here too?
        let app_theme_selected = match self.config.app_theme {
            AppTheme::Dark => 1,
            AppTheme::Light => 2,
            AppTheme::System => 0,
        };
        let mut metadata_path;
        match dirs::data_local_dir() {
            Some(pb) => {
                metadata_path = pb;
                if !metadata_path.exists() {
                    let ret = std::fs::create_dir_all(metadata_path.clone());
                    if ret.is_err() {
                        log::warn!("Failed to create directory {}", metadata_path.display());
                        metadata_path = dirs::home_dir().unwrap();
                    }
                }
            }
            None => {
                metadata_path = dirs::home_dir().unwrap();
            }
        }
        let appearance_section = widget::settings::section().title(fl!("appearance")).add(
            widget::settings::item::builder(fl!("theme")).control(widget::dropdown(
                &self.app_themes,
                Some(app_theme_selected),
                move |index| {
                    Message::AppTheme(match index {
                        1 => AppTheme::Dark,
                        2 => AppTheme::Light,
                        _ => AppTheme::System,
                    })
                },
            )),
        );

        if let Ok(metadata_item) =
            crate::parsers::item_from_path(metadata_path, IconSizes::default())
        {
            match &metadata_item.metadata {
                ItemMetadata::Path { metadata, children } => {
                    if metadata.is_dir() {
                        if let Some(path) = metadata_item.path_opt() {
                            let metadata_location =
                                crate::parsers::osstr_to_string(path.clone().into_os_string());
                            let metadata_items = children.to_owned();
                            let metadata_size = match &metadata_item.dir_size {
                                crate::tab::DirSize::Calculating(_) => fl!("calculating"),
                                crate::tab::DirSize::Directory(size) => {
                                    crate::tab::format_size(*size)
                                }
                                crate::tab::DirSize::NotDirectory => String::new(),
                                crate::tab::DirSize::Error(err) => err.clone(),
                            };
                            let metadata_section = widget::settings::section()
                                .title(fl!("metadata"))
                                .add(widget::text::body(fl!(
                                    "metadata-details",
                                    items = metadata_items,
                                    size = metadata_size,
                                    location = metadata_location
                                )))
                                .add(
                                    widget::settings::item::builder(fl!("metadata-delete"))
                                        .control(
                                            widget::button::custom(
                                                widget::icon::from_name("user-trash-symbolic")
                                                    .size(16),
                                            )
                                            .on_press(Message::MetadataDelete),
                                        ),
                                );
                            widget::column::with_children(vec![
                                appearance_section.into(),
                                metadata_section.into(),
                            ])
                            .into()
                        } else {
                            widget::column::with_children(vec![appearance_section.into()]).into()
                        }
                    } else {
                        widget::column::with_children(vec![appearance_section.into()]).into()
                    }
                }
                _ => widget::column::with_children(vec![appearance_section.into()]).into(),
            }
        } else {
            widget::column::with_children(vec![appearance_section.into()]).into()
        }
    }

    fn view_image_view(&self) -> Element<'_, <App as cosmic::Application>::Message> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

        if self.image_view.handle_opt == None {
            // construct the image first
            return self.view_browser_view();
        }
        let image_viewer = Container::new(
            cosmic::iced::widget::image::Viewer::new(crate::image::image::create_handle(
                self.image_view.image_path.clone(),
            ))
            .width(self.image_view.width)
            .height(self.image_view.height)
            .min_scale(self.image_view.min_scale)
            .max_scale(self.image_view.max_scale)
            .scale_step(self.image_view.scale_step)
            .padding(5.0),
        )
        .class(style::Container::Background)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding([0, space_s]);
        // draw Image GUI
        let mouse_area = widget::mouse_area(image_viewer)
            .on_press(Message::PlayPause)
            .on_double_press(Message::Fullscreen);

        let mut popover = widget::popover(mouse_area).position(widget::popover::Position::Bottom);

        let mut popup_items = Vec::<Element<_>>::with_capacity(2);
        if self.image_view.controls {
            let mut browser = None;
            let entity = self.tab_model.active();
            match self.tab_model.data::<Tab>(entity) {
                Some(tab) => {
                    let tab_view = tab
                        .view(&self.key_binds)
                        .map(move |message| Message::TabMessage(Some(entity), message));
                    browser = Some(tab_view);
                }
                None => {
                    //TODO
                }
            }
            if let Some(view) = browser {
                popup_items.push(
                    widget::container(view)
                        .padding(1)
                        //TODO: move style to libcosmic
                        .class(theme::Container::custom(|theme| {
                            let cosmic = theme.cosmic();
                            let component = &cosmic.background.component;
                            widget::container::Style {
                                icon_color: Some(component.on.into()),
                                text_color: Some(component.on.into()),
                                background: Some(cosmic::iced::Background::Color(
                                    component.base.into(),
                                )),
                                border: cosmic::iced::Border {
                                    radius: 8.0.into(),
                                    width: 1.0,
                                    color: component.divider.into(),
                                },
                                ..Default::default()
                            }
                        }))
                        .height(Length::Fixed(250.0))
                        .width(Length::Fill)
                        .align_x(Alignment::Start)
                        .into(),
                );
            }
            popup_items.push(
                widget::container(
                    widget::row::with_capacity(10)
                        .align_y(Alignment::Center)
                        .spacing(space_xxs)
                        .push(
                            widget::button::icon(
                                widget::icon::from_name("go-up-symbolic").size(16),
                            )
                            .on_press(Message::ImageMessage(
                                crate::image::image_view::Message::ToBrowser,
                            )),
                        )
                        .push(
                            widget::button::icon(
                                widget::icon::from_name("go-previous-symbolic").size(16),
                            )
                            .on_press(Message::Previous(Some(entity))),
                        )
                        .push(
                            widget::button::icon(
                                widget::icon::from_name("go-next-symbolic").size(16),
                            )
                            .on_press(Message::Next(Some(entity))),
                        ), /*
                           .push(
                               widget::button::icon(
                                       widget::icon::from_name("zoom-in-symbolic")
                                           .size(16)
                               ).on_press(Message::ImageMessage(
                                           crate::image::image_view::Message::ZoomPlus))
                           )
                           .push(
                               widget::button::icon(
                                       widget::icon::from_name("zoom-out-symbolic")
                                           .size(16)
                               ).on_press(Message::ImageMessage(
                                           crate::image::image_view::Message::ZoomMinus))
                           )
                           .push(
                               widget::button::icon(
                                       widget::icon::from_name("zoom-fit-best-symbolic")
                                           .size(16)
                               ).on_press(Message::ImageMessage(
                                           crate::image::image_view::Message::ZoomFit))
                           )
                           */
                )
                //TODO: move style to libcosmic
                .class(theme::Container::custom(|theme| {
                    let cosmic = theme.cosmic();
                    let component = &cosmic.background.component;
                    widget::container::Style {
                        icon_color: Some(component.on.into()),
                        text_color: Some(component.on.into()),
                        background: Some(cosmic::iced::Background::Color(component.base.into())),
                        border: cosmic::iced::Border {
                            radius: 8.0.into(),
                            width: 1.0,
                            color: component.divider.into(),
                        },
                        ..Default::default()
                    }
                }))
                .padding([space_xxs, space_xs])
                .height(Length::Shrink)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .max_height(280)
                .into(),
            );
        }

        if !popup_items.is_empty() {
            popover = popover.popup(widget::column::with_children(popup_items));
        }

        widget::container(popover)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(theme::Container::Custom(Box::new(|_theme| {
                widget::container::Style::default().background(cosmic::iced::Color::BLACK)
            })))
            .into()
    }

    fn view_video_view(&self) -> Element<'_, <App as cosmic::Application>::Message> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_m,
            ..
        } = theme::active().cosmic().spacing;

        let format_time = |time_float: f64| -> String {
            let time = time_float.floor() as i64;
            let seconds = time % 60;
            let minutes = (time / 60) % 60;
            let hours = (time / 60) / 60;
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        };

        let Some(mut video) = self.video_view.video_opt.as_ref() else {
            //TODO: open button if no video?
            return widget::container(widget::text("No video open"))
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .class(theme::Container::WindowBackground)
                .into();
        };

        let muted = video.muted();
        let volume = video.volume();

        let video_player = VideoPlayer::new(&mut video)
            .mouse_hidden(!self.video_view.controls)
            .on_end_of_stream(Message::EndOfStream)
            .on_missing_plugin(Message::MissingPlugin)
            .on_new_frame(Message::NewFrame)
            .width(3840.0)
            .height(2160.0);

        let mouse_area = widget::mouse_area(video_player)
            .on_press(Message::PlayPause)
            .on_scroll(|delta| Message::MouseScroll(delta))
            .on_double_press(Message::Fullscreen);

        let mut popover = widget::popover(mouse_area).position(widget::popover::Position::Bottom);

        let mut popup_items = Vec::<Element<_>>::with_capacity(2);
        if let Some(dropdown) = self.video_view.dropdown_opt {
            let mut items_right = Vec::<Element<_>>::new();
            //let mut items_left = Vec::<Element<_>>::new();
            match dropdown {
                crate::video::video_view::DropdownKind::Audio => {
                    items_right.push(
                        widget::row::with_children(vec![
                            widget::button::icon(
                                widget::icon::from_name({
                                    if muted {
                                        "audio-volume-muted-symbolic"
                                    } else {
                                        if volume >= (2.0 / 3.0) {
                                            "audio-volume-high-symbolic"
                                        } else if volume >= (1.0 / 3.0) {
                                            "audio-volume-medium-symbolic"
                                        } else {
                                            "audio-volume-low-symbolic"
                                        }
                                    }
                                })
                                .size(16),
                            )
                            .on_press(Message::VideoMessage(
                                crate::video::video_view::Message::AudioToggle,
                            ))
                            .into(),
                            //TODO: disable slider when muted?
                            Slider::new(0.0..=1.0, volume, Message::AudioVolume)
                                .step(0.01)
                                .into(),
                        ])
                        .align_y(Alignment::Center)
                        .into(),
                    );
                }
                crate::video::video_view::DropdownKind::Subtitle => {
                    if !self.video_view.audio_codes.is_empty() {
                        items_right.push(widget::text::heading(fl!("audio")).into());
                        items_right.push(
                            widget::dropdown(
                                &self.video_view.audio_codes,
                                usize::try_from(self.video_view.current_audio).ok(),
                                Message::AudioCode,
                            )
                            .into(),
                        );
                    }
                    if !self.video_view.text_codes.is_empty() {
                        //TODO: allow toggling subtitles
                        items_right.push(widget::text::heading(fl!("subtitles")).into());
                        items_right.push(
                            widget::dropdown(
                                &self.video_view.text_codes,
                                usize::try_from(self.video_view.current_text).ok(),
                                Message::TextCode,
                            )
                            .into(),
                        );
                    }
                }
                crate::video::video_view::DropdownKind::Browser => {
                    let mut browser = None;
                    let entity = self.tab_model.active();
                    match self.tab_model.data::<Tab>(entity) {
                        Some(tab) => {
                            let tab_view = tab
                                .view(&self.key_binds)
                                .map(move |message| Message::TabMessage(Some(entity), message));
                            browser = Some(tab_view);
                        }
                        None => {
                            //TODO
                        }
                    }
                    if let Some(view) = browser {
                        popup_items.push(
                            widget::container(view)
                                .padding(1)
                                //TODO: move style to libcosmic
                                .class(theme::Container::custom(|theme| {
                                    let cosmic = theme.cosmic();
                                    let component = &cosmic.background.component;
                                    widget::container::Style {
                                        icon_color: Some(component.on.into()),
                                        text_color: Some(component.on.into()),
                                        background: Some(cosmic::iced::Background::Color(
                                            component.base.into(),
                                        )),
                                        border: cosmic::iced::Border {
                                            radius: 8.0.into(),
                                            width: 1.0,
                                            color: component.divider.into(),
                                        },
                                        ..Default::default()
                                    }
                                }))
                                .height(Length::Fixed(250.0))
                                .width(Length::Fill)
                                .align_x(Alignment::Start)
                                .into(),
                        );
                    }
                }
                crate::video::video_view::DropdownKind::Chapter => {
                    if !self.video_view.chapters.is_empty() {
                        items_right.push(widget::text::heading(fl!("chapters")).into());
                        items_right.push(
                            widget::dropdown(
                                &self.video_view.chapters_str,
                                Some(self.video_view.current_chapter),
                                Message::Chapter,
                            )
                            .into(),
                        );
                    }
                }
            }

            let mut column = widget::column::with_capacity(items_right.len());
            for item in items_right {
                column = column.push(widget::container(item).padding([space_xxs, space_m]));
            }

            popup_items.push(
                widget::row::with_children(vec![
                    widget::horizontal_space().into(),
                    widget::container(column)
                        .padding(1)
                        //TODO: move style to libcosmic
                        .class(theme::Container::custom(|theme| {
                            let cosmic = theme.cosmic();
                            let component = &cosmic.background.component;
                            widget::container::Style {
                                icon_color: Some(component.on.into()),
                                text_color: Some(component.on.into()),
                                background: Some(cosmic::iced::Background::Color(
                                    component.base.into(),
                                )),
                                border: cosmic::iced::Border {
                                    radius: 8.0.into(),
                                    width: 1.0,
                                    color: component.divider.into(),
                                },
                                ..Default::default()
                            }
                        }))
                        .width(Length::Fixed(240.0))
                        .into(),
                ])
                .into(),
            );
        }

        if self.video_view.controls {
            popup_items.push(
                widget::container(
                    widget::row::with_capacity(10)
                        .align_y(Alignment::Center)
                        .spacing(space_xxs)
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("go-up-symbolic").size(16),
                            )
                            .on_press(Message::Browser),
                            widget::text::body(fl!("descripttion-back")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("go-previous-symbolic").size(16),
                            )
                            .on_press(Message::VideoMessage(
                                crate::video::video_view::Message::PreviousFile,
                            )),
                            widget::text::body(fl!("description-previous-element")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("go-next-symbolic").size(16),
                            )
                            .on_press(Message::VideoMessage(
                                crate::video::video_view::Message::NextFile,
                            )),
                            widget::text::body(fl!("description-next-element")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(
                            if self
                                .audio_view
                                .audio_opt
                                .as_ref()
                                .map_or(true, |video| video.paused())
                            {
                                widget::tooltip(
                                    widget::button::icon(
                                        widget::icon::from_name("media-playback-start-symbolic")
                                            .size(16),
                                    )
                                    .on_press(
                                        Message::VideoMessage(
                                            crate::video::video_view::Message::PlayPause,
                                        ),
                                    ),
                                    widget::text::body(fl!("description-play")),
                                    widget::tooltip::Position::Top,
                                )
                            } else {
                                widget::tooltip(
                                    widget::button::icon(
                                        widget::icon::from_name("media-playback-pause-symbolic")
                                            .size(16),
                                    )
                                    .on_press(
                                        Message::VideoMessage(
                                            crate::video::video_view::Message::PlayPause,
                                        ),
                                    ),
                                    widget::text::body(fl!("description-pause")),
                                    widget::tooltip::Position::Top,
                                )
                            },
                        )
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("view-more-symbolic").size(16),
                            )
                            .on_press(Message::VideoMessage(
                                crate::video::video_view::Message::DropdownToggle(
                                    crate::video::video_view::DropdownKind::Browser,
                                ),
                            )),
                            widget::text::body(fl!("description-browser")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("media-skip-backward-symbolic").size(16),
                            )
                            .on_press(Message::SeekBackward),
                            widget::text::body(fl!("description-seek-backward")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("media-skip-forward-symbolic").size(16),
                            )
                            .on_press(Message::SeekForward),
                            widget::text::body(fl!("description-seek-forward")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(
                            widget::text(format_time(self.video_view.position)).font(font::mono()),
                        )
                        .push(
                            Slider::new(
                                0.0..=self.video_view.duration,
                                self.video_view.position,
                                Message::Seek,
                            )
                            .step(0.1)
                            .on_release(Message::VideoMessage(
                                crate::video::video_view::Message::SeekRelease,
                            )),
                        )
                        .push(
                            widget::text(format_time(
                                self.video_view.duration - self.video_view.position,
                            ))
                            .font(font::mono()),
                        )
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("open-menu-symbolic").size(16),
                            )
                            .on_press(Message::VideoMessage(
                                crate::video::video_view::Message::DropdownToggle(
                                    crate::video::video_view::DropdownKind::Chapter,
                                ),
                            )),
                            widget::text::body(fl!("description-chapters")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("media-view-subtitles-symbolic").size(16),
                            )
                            .on_press(Message::VideoMessage(
                                crate::video::video_view::Message::DropdownToggle(
                                    crate::video::video_view::DropdownKind::Subtitle,
                                ),
                            )),
                            widget::text::body(fl!("description-streams")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(
                            widget::button::icon(
                                widget::icon::from_name("view-fullscreen-symbolic").size(16),
                            )
                            .on_press(Message::Fullscreen),
                        )
                        .push(
                            //TODO: scroll up/down on icon to change volume
                            widget::button::icon(
                                widget::icon::from_name({
                                    if muted {
                                        "audio-volume-muted-symbolic"
                                    } else {
                                        if volume >= (2.0 / 3.0) {
                                            "audio-volume-high-symbolic"
                                        } else if volume >= (1.0 / 3.0) {
                                            "audio-volume-medium-symbolic"
                                        } else {
                                            "audio-volume-low-symbolic"
                                        }
                                    }
                                })
                                .size(16),
                            )
                            .on_press(Message::VideoMessage(
                                crate::video::video_view::Message::DropdownToggle(
                                    crate::video::video_view::DropdownKind::Audio,
                                ),
                            )),
                        ),
                )
                .padding([space_xxs, space_xs])
                .class(theme::Container::WindowBackground)
                .into(),
            );
        }
        if !popup_items.is_empty() {
            popover = popover.popup(widget::column::with_children(popup_items));
        }

        widget::container(popover)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(theme::Container::Custom(Box::new(|_theme| {
                widget::container::Style::default().background(cosmic::iced::Color::BLACK)
            })))
            .into()
    }

    fn view_audio_view(&self) -> Element<'_, <App as cosmic::Application>::Message> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_m,
            ..
        } = theme::active().cosmic().spacing;

        let format_time = |time_float: f64| -> String {
            let time = time_float.floor() as i64;
            let seconds = time % 60;
            let minutes = (time / 60) % 60;
            let hours = (time / 60) / 60;
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        };

        let Some(audio) = &self.audio_view.audio_opt else {
            //TODO: open button if no video?
            return widget::container(widget::text("No audio open"))
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .class(theme::Container::WindowBackground)
                .into();
        };

        let muted = audio.muted();
        let volume = audio.volume();

        let audio_player = AudioPlayer::new(audio)
            .mouse_hidden(!self.audio_view.controls)
            .on_end_of_stream(Message::EndOfStream)
            .on_missing_plugin(Message::MissingPlugin)
            .on_new_frame(Message::NewFrame)
            .width(1920.0)
            .height(1080.0);

        let mouse_area = widget::mouse_area(audio_player)
            .on_press(Message::AudioMessage(
                crate::audio::audio_view::Message::PlayPause,
            ))
            .on_scroll(|delta| Message::MouseScroll(delta))
            .on_double_press(Message::Fullscreen);

        let mut popover = widget::popover(mouse_area).position(widget::popover::Position::Bottom);
        let mut popup_items = Vec::<Element<_>>::with_capacity(2);
        if let Some(dropdown) = self.audio_view.dropdown_opt {
            let mut items_right = Vec::<Element<_>>::new();
            //let mut items_left = Vec::<Element<_>>::new();
            match dropdown {
                crate::audio::audio_view::DropdownKind::Audio => {
                    items_right.push(
                        widget::row::with_children(vec![
                            widget::button::icon(
                                widget::icon::from_name({
                                    if muted {
                                        "audio-volume-muted-symbolic"
                                    } else {
                                        if volume >= (2.0 / 3.0) {
                                            "audio-volume-high-symbolic"
                                        } else if volume >= (1.0 / 3.0) {
                                            "audio-volume-medium-symbolic"
                                        } else {
                                            "audio-volume-low-symbolic"
                                        }
                                    }
                                })
                                .size(16),
                            )
                            .on_press(Message::AudioMessage(
                                crate::audio::audio_view::Message::AudioToggle,
                            ))
                            .into(),
                            //TODO: disable slider when muted?
                            Slider::new(0.0..=1.0, volume, Message::AudioVolume)
                                .step(0.01)
                                .into(),
                        ])
                        .align_y(Alignment::Center)
                        .into(),
                    );
                }
                crate::audio::audio_view::DropdownKind::Subtitle => {
                    if !self.audio_view.audio_codes.is_empty() {
                        items_right.push(widget::text::heading(fl!("audio")).into());
                        items_right.push(
                            widget::dropdown(
                                &self.audio_view.audio_codes,
                                usize::try_from(self.audio_view.current_audio).ok(),
                                Message::AudioCode,
                            )
                            .into(),
                        );
                    }
                    if !self.audio_view.text_codes.is_empty() {
                        //TODO: allow toggling subtitles
                        items_right.push(widget::text::heading(fl!("subtitles")).into());
                        items_right.push(
                            widget::dropdown(
                                &self.audio_view.text_codes,
                                usize::try_from(self.audio_view.current_text).ok(),
                                Message::TextCode,
                            )
                            .into(),
                        );
                    }
                }
                crate::audio::audio_view::DropdownKind::Browser => {
                    let mut browser = None;
                    let entity = self.tab_model.active();
                    match self.tab_model.data::<Tab>(entity) {
                        Some(tab) => {
                            let tab_view = tab
                                .view(&self.key_binds)
                                .map(move |message| Message::TabMessage(Some(entity), message));
                            browser = Some(tab_view);
                        }
                        None => {
                            //TODO
                        }
                    }
                    if let Some(view) = browser {
                        popup_items.push(
                            widget::container(view)
                                .padding(1)
                                //TODO: move style to libcosmic
                                .class(theme::Container::custom(|theme| {
                                    let cosmic = theme.cosmic();
                                    let component = &cosmic.background.component;
                                    widget::container::Style {
                                        icon_color: Some(component.on.into()),
                                        text_color: Some(component.on.into()),
                                        background: Some(cosmic::iced::Background::Color(
                                            component.base.into(),
                                        )),
                                        border: cosmic::iced::Border {
                                            radius: 8.0.into(),
                                            width: 1.0,
                                            color: component.divider.into(),
                                        },
                                        ..Default::default()
                                    }
                                }))
                                .height(Length::Fixed(250.0))
                                .width(Length::Fill)
                                .align_x(Alignment::Start)
                                .into(),
                        );
                    }
                }
                crate::audio::audio_view::DropdownKind::Chapter => {
                    if !self.audio_view.chapters.is_empty() {
                        items_right.push(widget::text::heading(fl!("chapters")).into());
                        items_right.push(
                            widget::dropdown(
                                &self.audio_view.chapters_str,
                                Some(self.audio_view.current_chapter),
                                Message::Chapter,
                            )
                            .into(),
                        );
                    }
                }
            }

            let mut column = widget::column::with_capacity(items_right.len());
            for item in items_right {
                column = column.push(widget::container(item).padding([space_xxs, space_m]));
            }

            popup_items.push(
                widget::row::with_children(vec![
                    widget::horizontal_space().into(),
                    widget::container(column)
                        .padding(1)
                        //TODO: move style to libcosmic
                        .class(theme::Container::custom(|theme| {
                            let cosmic = theme.cosmic();
                            let component = &cosmic.background.component;
                            widget::container::Style {
                                icon_color: Some(component.on.into()),
                                text_color: Some(component.on.into()),
                                background: Some(cosmic::iced::Background::Color(
                                    component.base.into(),
                                )),
                                border: cosmic::iced::Border {
                                    radius: 8.0.into(),
                                    width: 1.0,
                                    color: component.divider.into(),
                                },
                                ..Default::default()
                            }
                        }))
                        .width(Length::Fixed(240.0))
                        .into(),
                ])
                .into(),
            );
        }

        if self.audio_view.controls {
            popup_items.push(
                widget::container(
                    widget::row::with_capacity(7)
                        .align_y(Alignment::Center)
                        .spacing(space_xxs)
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("go-up-symbolic").size(16),
                            )
                            .on_press(Message::Browser),
                            widget::text::body(fl!("descripttion-back")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("go-previous-symbolic").size(16),
                            )
                            .on_press(Message::AudioMessage(
                                crate::audio::audio_view::Message::PreviousFile,
                            )),
                            widget::text::body(fl!("description-previous-element")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("go-next-symbolic").size(16),
                            )
                            .on_press(Message::AudioMessage(
                                crate::audio::audio_view::Message::NextFile,
                            )),
                            widget::text::body(fl!("description-next-element")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(
                            if self
                                .audio_view
                                .audio_opt
                                .as_ref()
                                .map_or(true, |video| video.paused())
                            {
                                widget::tooltip(
                                    widget::button::icon(
                                        widget::icon::from_name("media-playback-start-symbolic")
                                            .size(16),
                                    )
                                    .on_press(
                                        Message::AudioMessage(
                                            crate::audio::audio_view::Message::PlayPause,
                                        ),
                                    ),
                                    widget::text::body(fl!("description-play")),
                                    widget::tooltip::Position::Top,
                                )
                            } else {
                                widget::tooltip(
                                    widget::button::icon(
                                        widget::icon::from_name("media-playback-pause-symbolic")
                                            .size(16),
                                    )
                                    .on_press(
                                        Message::AudioMessage(
                                            crate::audio::audio_view::Message::PlayPause,
                                        ),
                                    ),
                                    widget::text::body(fl!("description-pause")),
                                    widget::tooltip::Position::Top,
                                )
                            },
                        )
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("view-more-symbolic").size(16),
                            )
                            .on_press(Message::AudioMessage(
                                crate::audio::audio_view::Message::DropdownToggle(
                                    crate::audio::audio_view::DropdownKind::Browser,
                                ),
                            )),
                            widget::text::body(fl!("description-browser")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("media-skip-backward-symbolic").size(16),
                            )
                            .on_press(Message::SeekBackward),
                            widget::text::body(fl!("description-seek-backward")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("media-skip-forward-symbolic").size(16),
                            )
                            .on_press(Message::SeekForward),
                            widget::text::body(fl!("description-seek-forward")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(
                            widget::text(format_time(self.audio_view.position)).font(font::mono()),
                        )
                        .push(
                            Slider::new(
                                0.0..=self.audio_view.duration,
                                self.audio_view.position,
                                Message::Seek,
                            )
                            .step(0.1)
                            .on_release(Message::AudioMessage(
                                crate::audio::audio_view::Message::SeekRelease,
                            )),
                        )
                        .push(
                            widget::text(format_time(
                                self.audio_view.duration - self.audio_view.position,
                            ))
                            .font(font::mono()),
                        )
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("open-menu-symbolic").size(16),
                            )
                            .on_press(Message::AudioMessage(
                                crate::audio::audio_view::Message::DropdownToggle(
                                    crate::audio::audio_view::DropdownKind::Chapter,
                                ),
                            )),
                            widget::text::body(fl!("description-chapters")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(widget::tooltip(
                            widget::button::icon(
                                widget::icon::from_name("media-view-subtitles-symbolic").size(16),
                            )
                            .on_press(Message::AudioMessage(
                                crate::audio::audio_view::Message::DropdownToggle(
                                    crate::audio::audio_view::DropdownKind::Subtitle,
                                ),
                            )),
                            widget::text::body(fl!("description-streams")),
                            widget::tooltip::Position::Top,
                        ))
                        .push(
                            widget::button::icon(
                                widget::icon::from_name("view-fullscreen-symbolic").size(16),
                            )
                            .on_press(Message::AudioMessage(
                                crate::audio::audio_view::Message::Fullscreen,
                            )),
                        )
                        .push(
                            //TODO: scroll up/down on icon to change volume
                            widget::button::icon(
                                widget::icon::from_name({
                                    if muted {
                                        "audio-volume-muted-symbolic"
                                    } else {
                                        if volume >= (2.0 / 3.0) {
                                            "audio-volume-high-symbolic"
                                        } else if volume >= (1.0 / 3.0) {
                                            "audio-volume-medium-symbolic"
                                        } else {
                                            "audio-volume-low-symbolic"
                                        }
                                    }
                                })
                                .size(16),
                            )
                            .on_press(Message::AudioMessage(
                                crate::audio::audio_view::Message::DropdownToggle(
                                    crate::audio::audio_view::DropdownKind::Audio,
                                ),
                            )),
                        ),
                )
                .padding([space_xxs, space_xs])
                .class(theme::Container::WindowBackground)
                .into(),
            );
        }
        if !popup_items.is_empty() {
            popover = popover.popup(widget::column::with_children(popup_items));
        }

        widget::container(popover)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(theme::Container::Custom(Box::new(|_theme| {
                widget::container::Style::default().background(cosmic::iced::Color::BLACK)
            })))
            .into()
    }

    fn view_browser_view(&self) -> Element<'_, <App as cosmic::Application>::Message> {
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;
        let mut tab_column = widget::column::with_capacity(3);

        if self.tab_model.iter().count() > 1 {
            tab_column = tab_column.push(
                widget::container(
                    widget::tab_bar::horizontal(&self.tab_model)
                        .button_height(32)
                        .button_spacing(space_xxs)
                        .on_activate(Message::TabActivate)
                        .on_dnd_enter(|entity, _| Message::DndEnterTab(entity))
                        .on_dnd_leave(|_| Message::DndExitTab)
                        .on_dnd_drop(|entity, data, action| {
                            Message::DndDropTab(entity, data, action)
                        })
                        .drag_id(self.tab_drag_id),
                )
                .class(style::Container::Background)
                .width(Length::Fill)
                .padding([0, space_s]),
            );
        }

        let entity = self.tab_model.active();
        match self.tab_model.data::<Tab>(entity) {
            Some(tab) => {
                let tab_view = tab
                    .view(&self.key_binds)
                    .map(move |message| Message::TabMessage(Some(entity), message));
                tab_column = tab_column.push(tab_view);
            }
            None => {
                //TODO
            }
        }

        // The toaster is added on top of an empty element to ensure that it does not override context menus
        tab_column = tab_column.push(widget::toaster(&self.toasts, widget::horizontal_space()));

        let content: Element<_> = tab_column.into();

        // Uncomment to debug layout:
        //content.explain(cosmic::iced::Color::WHITE)
        content
    }

    fn open_path(&mut self, path: PathBuf) -> Option<Task<crate::app::Message>> {
        if self.active_view == Mode::Audio {
            if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                if !audio.paused() {
                    audio.set_paused(true);
                }
            }
        }
        if self.active_view == Mode::Video {
            if let Some(video) = self.video_view.video_opt.as_mut() {
                if !video.paused() {
                    video.set_paused(true);
                }
            }
        }

        if path.is_dir() {
            // change directory
            self.core.nav_bar_set_toggled(false);
            if let Some(location_ref) = self.tab_model.data::<Location>(self.tab_model_id) {
                let location = location_ref.to_owned();
                if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                    let _ret = tab.update(tab::Message::Location(location), self.modifiers);
                }
                return None;
            }
        } else {
            // if the file is a supported media file, open it
            // else use mimetype to open in an external app
            let ret = file_format::FileFormat::from_file(path.clone());
            if ret.is_err() {
                return Some(Task::none());
            }
            let fmt = ret.unwrap();
            let filepath = path.display().to_string();
            self.core.nav_bar_set_toggled(false);
            match fmt.kind() {
                file_format::Kind::Image => {
                    self.image_view
                        .update(crate::image::image_view::Message::Open(filepath.clone()));
                    self.core.window.content_container = true;
                    self.core.window.show_window_menu = true;
                    self.core.window.show_headerbar = true;
                    self.active_view = Mode::Image;
                    self.view();
                }
                file_format::Kind::Video => {
                    self.video_view
                        .update(crate::video::video_view::Message::Open(filepath.clone()));
                    if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                        let v = tab.selected_file_paths();
                        for path in v {
                            if let Some(items) = tab.items_opt() {
                                for item in items.iter() {
                                    if let Some(video) = item.video_opt.as_ref() {
                                        if PathBuf::from(&video.path) == path {
                                            let (v, s) = crate::sql::fill_chapters(
                                                video.chapters.clone(),
                                                video.duration,
                                            );
                                            if v.len() > 0 {
                                                self.video_view.chapters.clear();
                                                self.video_view.chapters_str.clear();
                                                self.video_view.chapters.extend(v);
                                                self.video_view.chapters_str.extend(s);
                                            }
                                            self.video_view.audio_codes.clear();
                                            self.video_view
                                                .audio_codes
                                                .extend(video.audiolangs.clone());
                                            self.video_view.text_codes.clear();
                                            self.video_view
                                                .text_codes
                                                .extend(video.sublangs.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    self.core.window.content_container = true;
                    self.core.window.show_window_menu = true;
                    self.core.window.show_headerbar = true;
                    self.active_view = Mode::Video;
                    self.view();
                }
                file_format::Kind::Audio => {
                    self.audio_view
                        .update(crate::audio::audio_view::Message::Open(filepath.clone()));
                    if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                        let v = tab.selected_file_paths();
                        for path in v {
                            if let Some(items) = tab.items_opt() {
                                for item in items.iter() {
                                    if let Some(audio) = item.audio_opt.as_ref() {
                                        if PathBuf::from(&audio.path) == path {
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
                    self.core.window.content_container = true;
                    self.core.window.show_window_menu = true;
                    self.core.window.show_headerbar = true;
                    self.active_view = Mode::Audio;
                    self.view();
                }
                _ => {
                    if let Some(_tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                        let _ = self.update(Message::TabMessage(
                            Some(self.tab_model_id),
                            tab::Message::OpenInExternalApp(Some(path)),
                        ));
                    }
                }
            }
        }
        return Some(Task::none());
    }
}

/// Implement [`Application`] to integrate with COSMIC.
impl Application for App {
    /// Default async executor to use with the app.
    type Executor = executor::Default;

    /// Argument received
    type Flags = Flags;

    /// Message type specific to our [`App`].
    type Message = Message;

    /// The unique application ID to supply to the window manager.
    const APP_ID: &'static str = "eu.fangornsrealm.MediaBrowser";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    /// Creates the application, and optionally emits command on initialize.
    fn init(mut core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        core.window.context_is_overlay = false;
        match flags.mode {
            Mode::App => {
                core.window.show_context = flags.config.show_details;
            }
            Mode::Desktop => {
                core.window.content_container = false;
                core.window.show_window_menu = false;
                core.window.show_headerbar = false;
                core.window.sharp_corners = false;
                core.window.show_maximize = false;
                core.window.show_minimize = false;
                core.window.use_template = true;
            }
            Mode::Browser => {}
            Mode::Image => {
                core.window.content_container = true;
                core.window.show_window_menu = true;
                core.window.show_headerbar = true;
            }
            Mode::Audio => {
                core.window.content_container = true;
                core.window.show_window_menu = true;
                core.window.show_headerbar = true;
            }
            Mode::Video => {
                core.window.content_container = true;
                core.window.show_window_menu = true;
                core.window.show_headerbar = true;
            }
        }

        let app_themes = vec![fl!("match-desktop"), fl!("dark"), fl!("light")];

        let key_binds = key_binds(&match flags.mode {
            Mode::App => tab::Mode::App,
            Mode::Desktop => tab::Mode::Desktop,
            Mode::Browser => tab::Mode::App,
            Mode::Image => tab::Mode::App,
            Mode::Video => tab::Mode::App,
            Mode::Audio => tab::Mode::App,
        });

        let window_id_opt = core.main_window_id();

        let sql_conmnection: rusqlite::Connection = match crate::sql::connect() {
            Ok(conn) => conn,
            Err(error) => {
                log::error!("Failed to open Database! {}", error);
                panic!("Ending the program as a database is required!");
            }
        };

        let mut app = App {
            image_view: crate::image::image_view::ImageView::new(),
            video_view: crate::video::video_view::VideoView::new(),
            audio_view: crate::audio::audio_view::AudioView::new(),
            active_view: Mode::Browser,
            core,
            nav_bar_context_id: segmented_button::Entity::null(),
            nav_model: segmented_button::ModelBuilder::default().build(),
            tab_model: segmented_button::ModelBuilder::default().build(),
            tab_model_id: Entity::default(),
            config_handler: flags.config_handler,
            config: flags.config,
            mode: flags.mode,
            app_themes,
            context_page: ContextPage::Preview(None, PreviewKind::Selected),
            dialog_pages: VecDeque::new(),
            dialog_text_input: widget::Id::unique(),
            key_binds,
            margin: HashMap::new(),
            modifiers: Modifiers::empty(),
            mounter_items: HashMap::new(),
            network_drive_connecting: None,
            network_drive_input: String::new(),
            #[cfg(feature = "notify")]
            notification_opt: None,
            _overlap: HashMap::new(),
            pending_operation_id: 0,
            pending_operations: BTreeMap::new(),
            progress_operations: BTreeSet::new(),
            complete_operations: BTreeMap::new(),
            failed_operations: BTreeMap::new(),
            search_id: widget::Id::unique(),
            search: crate::sql::SearchData {
                ..Default::default()
            },
            search_previous: Vec::new(),
            search_previous_str: Vec::new(),
            search_previous_pos: 0,
            search_from_string: widget::Id::unique(),
            search_to_string: widget::Id::unique(),
            size: None,
            #[cfg(feature = "wayland")]
            surface_ids: HashMap::new(),
            #[cfg(feature = "wayland")]
            surface_names: HashMap::new(),
            toasts: widget::toaster::Toasts::new(Message::CloseToast),
            watcher_opt: None,
            window_id_opt,
            windows: HashMap::new(),
            nav_dnd_hover: None,
            tab_dnd_hover: None,
            nav_drag_id: DragId::new(),
            tab_drag_id: DragId::new(),
            sql_connection: std::sync::Arc::new(std::sync::Mutex::new(sql_conmnection)),
        };
        app.tab_model_id = app.tab_model.active();
        let mut commands = vec![app.update_config()];

        for location in flags.locations {
            commands.push(app.open_tab(location, true, None));
        }

        if app.tab_model.iter().next().is_none() {
            if let Ok(current_dir) = env::current_dir() {
                commands.push(app.open_tab(Location::Path(current_dir), true, None));
            } else {
                commands.push(app.open_tab(Location::Path(home_dir()), true, None));
            }
        }
        app.core.nav_bar_set_toggled(false);
        (app, Task::batch(commands))
    }

    fn nav_bar(&self) -> Option<Element<'_, message::Message<Self::Message>>> {
        if !self.core().nav_bar_active() {
            return None;
        }

        let nav_model = self.nav_model()?;

        let mut nav = cosmic::widget::nav_bar(nav_model, |entity| {
            cosmic::app::Message::Cosmic(cosmic::app::cosmic::Message::NavBar(entity))
        })
        .drag_id(self.nav_drag_id)
        .on_dnd_enter(|entity, _| cosmic::app::Message::App(Message::DndEnterNav(entity)))
        .on_dnd_leave(|_| cosmic::app::Message::App(Message::DndExitNav))
        .on_dnd_drop(|entity, data, action| {
            cosmic::app::Message::App(Message::DndDropNav(entity, data, action))
        })
        .on_context(|entity| cosmic::app::Message::App(Message::NavBarContext(entity)))
        .on_close(|entity| cosmic::app::Message::App(Message::NavBarClose(entity)))
        .on_middle_press(|entity| {
            cosmic::app::Message::App(Message::NavMenuAction(NavMenuAction::Open(entity)))
        })
        .context_menu(self.nav_context_menu(self.nav_bar_context_id))
        .close_icon(
            widget::icon::from_name("media-eject-symbolic")
                .size(16)
                .icon(),
        )
        .into_container();

        if !self.core().is_condensed() {
            nav = nav.max_width(280);
        }

        Some(Element::from(
            // XXX both must be shrink to avoid flex layout from ignoring it
            nav.width(Length::Shrink).height(Length::Shrink),
        ))
    }

    fn nav_context_menu(
        &self,
        entity: widget::nav_bar::Id,
    ) -> Option<Vec<widget::menu::Tree<'_, cosmic::app::Message<Self::Message>>>> {
        let favorite_index_opt = self.nav_model.data::<FavoriteIndex>(entity);
        let location_opt = self.nav_model.data::<Location>(entity);

        let mut items = Vec::new();

        if let Some(location) = location_opt {
            match location {
                Location::Path(path) => {
                    if path.is_file() {
                        items.push(cosmic::widget::menu::Item::Button(
                            fl!("open"),
                            None,
                            NavMenuAction::Open(entity),
                        ));
                        items.push(cosmic::widget::menu::Item::Button(
                            fl!("open-with"),
                            None,
                            NavMenuAction::Open(entity),
                        ));
                    } else {
                        items.push(cosmic::widget::menu::Item::Button(
                            fl!("open"),
                            None,
                            NavMenuAction::Open(entity),
                        ));
                        items.push(cosmic::widget::menu::Item::Button(
                            fl!("recursive-scan-directories"),
                            None,
                            NavMenuAction::RecursiveScanDirectories(entity),
                        ));
                    }
                    if favorite_index_opt.is_some() {
                        items.push(cosmic::widget::menu::Item::Button(
                            fl!("remove-from-sidebar"),
                            None,
                            NavMenuAction::RemoveFromSidebar(entity),
                        ));
                    }
                }
                Location::Tag(_t) => {
                    items.push(cosmic::widget::menu::Item::Button(
                        fl!("open-in-new-tab"),
                        None,
                        NavMenuAction::OpenTag(entity),
                    ));
                    items.push(cosmic::widget::menu::Item::Divider);
                    if favorite_index_opt.is_some() {
                        items.push(cosmic::widget::menu::Item::Button(
                            fl!("remove-tag-from-sidebar"),
                            None,
                            NavMenuAction::RemoveTagFromSidebar(entity),
                        ));
                    }
                }
                _ => {}
            }
        }
        items.push(cosmic::widget::menu::Item::Divider);
        items.push(cosmic::widget::menu::Item::Button(
            fl!("show-details"),
            None,
            NavMenuAction::Preview(entity),
        ));
        if matches!(location_opt, Some(Location::Trash)) {
            items.push(cosmic::widget::menu::Item::Button(
                fl!("empty-trash"),
                None,
                NavMenuAction::EmptyTrash,
            ));
        }

        Some(cosmic::widget::menu::items(&HashMap::new(), items))
    }

    fn nav_model(&self) -> Option<&segmented_button::SingleSelectModel> {
        match self.mode {
            Mode::App => Some(&self.nav_model),
            Mode::Desktop => None,
            Mode::Browser => Some(&self.nav_model),
            Mode::Image => Some(&self.nav_model),
            Mode::Audio => Some(&self.nav_model),
            Mode::Video => Some(&self.nav_model),
        }
    }

    fn on_nav_select(&mut self, entity: Entity) -> Task<Self::Message> {
        self.nav_model.activate(entity);
        if let Some(location) = self.nav_model.data::<Location>(entity) {
            let message = Message::TabMessage(None, tab::Message::Location(location.clone()));
            return self.update(message);
        }

        if let Some(data) = self.nav_model.data::<MounterData>(entity).clone() {
            if let Some(mounter) = MOUNTERS.get(&data.0) {
                return mounter.mount(data.1.clone()).map(|_| message::none());
            }
        }
        Task::none()
    }

    fn on_app_exit(&mut self) -> Option<Message> {
        Some(Message::WindowClose)
    }

    fn on_context_drawer(&mut self) -> Task<Self::Message> {
        match self.context_page {
            ContextPage::Preview(..) => {
                // Persist state of preview page
                if self.core.window.show_context != self.config.show_details {
                    return self.update(Message::Preview(None));
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn on_escape(&mut self) -> Task<Self::Message> {
        let entity = self.tab_model.active();

        // Close dialog if open
        if self.dialog_pages.pop_front().is_some() {
            return Task::none();
        }

        // Close gallery mode if open
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            if tab.gallery {
                tab.gallery = false;
                return Task::none();
            }
        }

        // Close menus and context panes in order per message
        // Why: It'd be weird to close everything all at once
        // Usually, the Escape key (for example) closes menus and panes one by one instead
        // of closing everything on one press
        if self.core.window.show_context {
            self.set_show_context(false);
            return cosmic::task::message(app::Message::App(Message::SetShowDetails(false)));
        }
        if self.search_get().is_some() {
            // Close search if open
            return self.search_set(None);
        }
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            if tab.context_menu.is_some() {
                tab.context_menu = None;
                return Task::none();
            }

            if tab.edit_location.is_some() {
                tab.edit_location = None;
                return Task::none();
            }

            let had_focused_button = tab.select_focus_id().is_some();
            if tab.select_none() {
                if had_focused_button {
                    // Unfocus if there was a focused button
                    return widget::button::focus(widget::Id::unique());
                }
                return Task::none();
            }
        }

        Task::none()
    }

    /// Handle application events here.
    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        // Helper for updating config values efficiently
        macro_rules! config_set {
            ($name: ident, $value: expr) => {
                match &self.config_handler {
                    Some(config_handler) => {
                        match paste::paste! { self.config.[<set_ $name>](config_handler, $value) } {
                            Ok(_) => {}
                            Err(err) => {
                                log::warn!(
                                    "failed to save config {:?}: {}",
                                    stringify!($name),
                                    err
                                );
                            }
                        }
                    }
                    None => {
                        self.config.$name = $value;
                        log::warn!(
                            "failed to save config {:?}: no config handler",
                            stringify!($name)
                        );
                    }
                }
            };
        }

        match message {
            Message::Open(_entity_opt, filepath) => {
                if filepath.len() > 0 {}
                if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                    let v = tab.selected_file_paths();
                    for path in v {
                        return match self.open_path(path) {
                            Some(command) => command,
                            _ => Task::none(),
                        };
                    }
                }
            }
            Message::Browser => {
                if self.active_view == Mode::Audio {
                    if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                        if !audio.paused() {
                            audio.set_paused(true);
                        }
                    }
                }
                if self.active_view == Mode::Video {
                    if let Some(video) = self.video_view.video_opt.as_mut() {
                        if !video.paused() {
                            video.set_paused(true);
                        }
                    }
                }

                self.active_view = Mode::Browser;
            }
            Message::Image(msg) => {
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
                };
            }
            Message::Video(msg) => {
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
                };
            }
            Message::Audio(msg) => {
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
                };
            }
            Message::AddToSidebar(entity_opt) => {
                let mut favorites = self.config.favorites.clone();
                for path in self.selected_paths(entity_opt) {
                    let favorite = Favorite::from_path(path);
                    if !favorites.iter().any(|f| f == &favorite) {
                        favorites.push(favorite);
                    }
                }
                config_set!(favorites, favorites);
                return self.update_config();
            }
            Message::AddTagToContents(to, contents) => {
                for p in contents.paths {
                    let file = crate::sql::file(
                        self.sql_connection.clone(),
                        &crate::parsers::osstr_to_string(p.clone().into_os_string()),
                    );
                    crate::sql::insert_media_tag(
                        self.sql_connection.clone(),
                        file.metadata_id as u32,
                        to.tag_id,
                    );
                }
            }
            Message::AddTagToSidebar(_entity_opt) => {
                self.dialog_pages.push_back(DialogPage::NewTag {
                    tag: "unnamed".to_string(),
                });
                return widget::text_input::focus(self.dialog_text_input.clone());
            }
            Message::AppTheme(app_theme) => {
                config_set!(app_theme, app_theme);
                return self.update_config();
            }
            Message::AudioMuteToggle => {
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
            }
            Message::AudioCode(val) => {
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
            }
            Message::AudioVolume(val) => {
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
            }
            Message::TextCode(val) => {
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
            }
            Message::Fullscreen => {
                if self.active_view == Mode::Video {
                    self.video_view.dropdown_opt = None;
                    self.video_view.fullscreen = !self.video_view.fullscreen;
                    self.core.window.show_headerbar = !self.video_view.fullscreen;
                    return window::change_mode(
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
                    return window::change_mode(
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
            }
            Message::Chapter(val) => {
                if self.active_view == Mode::Video {
                    self.video_view.current_chapter = val;
                    let new_pos = self.video_view.chapters[val].start as f64;
                    let _ = self.update(Message::VideoMessage(
                        crate::video::video_view::Message::Seek(new_pos),
                    ));
                    if let Some(video) = self.video_view.video_opt.as_mut() {
                        if video.paused() {
                            video.set_paused(false);
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
                            audio.set_paused(false);
                        }
                    }
                } else {
                    // no audio active
                }
            }
            Message::Seek(val) => {
                if self.active_view == Mode::Video {
                    let _ = self.update(Message::VideoMessage(
                        crate::video::video_view::Message::Seek(val),
                    ));
                    if let Some(video) = self.video_view.video_opt.as_mut() {
                        if video.paused() {
                            video.set_paused(false);
                        }
                    }
                } else if self.active_view == Mode::Audio {
                    let _ = self.update(Message::AudioMessage(
                        crate::audio::audio_view::Message::Seek(val),
                    ));
                    if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                        if audio.paused() {
                            audio.set_paused(false);
                        }
                    }
                } else {
                    // no audio active
                }
            }
            Message::SeekRelative(val) => {
                if self.active_view == Mode::Video {
                    let _ = self.update(Message::VideoMessage(
                        crate::video::video_view::Message::SeekRelative(val),
                    ));
                    if let Some(video) = self.video_view.video_opt.as_mut() {
                        if video.paused() {
                            video.set_paused(false);
                        }
                    }
                } else if self.active_view == Mode::Audio {
                    let _ = self.update(Message::AudioMessage(
                        crate::audio::audio_view::Message::SeekRelative(val),
                    ));
                    if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                        if audio.paused() {
                            audio.set_paused(false);
                        }
                    }
                } else {
                    // no audio active
                }
            }
            Message::EndOfStream => {
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
            }
            Message::NewFrame => {
                if self.active_view == Mode::Video {
                    let _ = self.update(Message::VideoMessage(
                        crate::video::video_view::Message::NewFrame,
                    ));
                } else if self.active_view == Mode::Audio {
                    let _ = self.update(Message::AudioMessage(
                        crate::audio::audio_view::Message::NewFrame,
                    ));
                } else {
                    // no audio active
                }
            }
            Message::PlayPause => {
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
            }
            Message::Previous(entity_opt) => {
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
            }
            Message::Next(entity_opt) => {
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
            }
            Message::RecursiveScanDirectories(entity_opt) => {
                let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
                if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
                    if let Location::Path(_parent) = &tab.location {
                        if let Some(items) = tab.items_opt() {
                            for item in items.iter() {
                                if item.selected {
                                    if let Some(Location::Path(path)) = &item.location_opt {
                                        let pathbuf = path.to_path_buf();
                                        let sql_connection = self.sql_connection.clone();
                                        let _joinhandle = std::thread::spawn(move || {
                                            crate::tab::scan_path_recursive(sql_connection, pathbuf)
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Message::MissingPlugin(element) => {
                if self.active_view == Mode::Video {
                    if let Some(video) = &mut self.video_view.video_opt {
                        video.set_paused(true);
                    }
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                match gst_pbutils::MissingPluginMessage::parse(&element) {
                                    Ok(missing_plugin) => {
                                        let mut install_ctx =
                                            gst_pbutils::InstallPluginsContext::new();
                                        install_ctx
                                            .set_desktop_id(&format!("{}.desktop", Self::APP_ID));
                                        let install_detail = missing_plugin.installer_detail();
                                        println!("installing plugins: {}", install_detail);
                                        let status =
                                            gst_pbutils::missing_plugins::install_plugins_sync(
                                                &[&install_detail],
                                                Some(&install_ctx),
                                            );
                                        log::info!("plugin install status: {}", status);
                                        log::info!(
                                            "gstreamer registry update: {:?}",
                                            gst::Registry::update()
                                        );
                                    }
                                    Err(err) => {
                                        log::warn!("failed to parse missing plugin message: {err}");
                                    }
                                }
                                message::app(Message::AudioMessage(
                                    crate::audio::audio_view::Message::Reload,
                                ))
                            })
                            .await
                            .unwrap()
                        },
                        |x| x,
                    );
                } else if self.active_view == Mode::Audio {
                    if let Some(video) = &mut self.audio_view.audio_opt {
                        video.set_paused(true);
                    }
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                match gst_pbutils::MissingPluginMessage::parse(&element) {
                                    Ok(missing_plugin) => {
                                        let mut install_ctx =
                                            gst_pbutils::InstallPluginsContext::new();
                                        install_ctx
                                            .set_desktop_id(&format!("{}.desktop", Self::APP_ID));
                                        let install_detail = missing_plugin.installer_detail();
                                        println!("installing plugins: {}", install_detail);
                                        let status =
                                            gst_pbutils::missing_plugins::install_plugins_sync(
                                                &[&install_detail],
                                                Some(&install_ctx),
                                            );
                                        log::info!("plugin install status: {}", status);
                                        log::info!(
                                            "gstreamer registry update: {:?}",
                                            gst::Registry::update()
                                        );
                                    }
                                    Err(err) => {
                                        log::warn!("failed to parse missing plugin message: {err}");
                                    }
                                }
                                message::app(Message::AudioMessage(
                                    crate::audio::audio_view::Message::Reload,
                                ))
                            })
                            .await
                            .unwrap()
                        },
                        |x| x,
                    );
                }
            }
            Message::AudioMessage(audio_message) => match audio_message {
                crate::audio::audio_view::Message::ToBrowser => {
                    self.active_view = Mode::Browser;
                    self.view();
                }
                crate::audio::audio_view::Message::Open(audiopath) => {
                    match url::Url::from_file_path(std::path::PathBuf::from(&audiopath)) {
                        Ok(url) => {
                            self.audio_view.audiopath_opt = Some(url);
                            self.audio_view.load();
                            if let Some(tab) = self.tab_model.data_mut::<Tab>(self.tab_model_id) {
                                let v = tab.selected_file_paths();
                                for path in v {
                                    if let Some(items) = tab.items_opt() {
                                        for item in items.iter() {
                                            if let Some(audio) = item.audio_opt.as_ref() {
                                                if PathBuf::from(&audio.path) == path {
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
                        let v = tab.selected_file_paths();
                        for path in v {
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
                        let v = tab.selected_file_paths();
                        for path in v {
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
                    return window::change_mode(
                        window::Id::RESERVED,
                        if self.audio_view.fullscreen {
                            window::Mode::Fullscreen
                        } else {
                            window::Mode::Windowed
                        },
                    );
                }
                crate::audio::audio_view::Message::AudioCode(code) => {
                    if let Ok(code) = i32::try_from(code) {
                        if let Some(audio) = &self.audio_view.audio_opt {
                            let pipeline = audio.pipeline();
                            pipeline.set_property("current-audio", code);
                            self.audio_view.current_audio = pipeline.property("current-audio");
                        }
                    }
                }
                crate::audio::audio_view::Message::AudioToggle => {
                    if let Some(audio) = &mut self.audio_view.audio_opt {
                        audio.set_muted(!audio.muted());
                        self.audio_view.update_controls(true);
                    }
                }
                crate::audio::audio_view::Message::AudioVolume(volume) => {
                    if let Some(audio) = &mut self.audio_view.audio_opt {
                        audio.set_volume(volume);
                        self.audio_view.update_controls(true);
                    }
                }
                crate::audio::audio_view::Message::TextCode(code) => {
                    if let Ok(code) = i32::try_from(code) {
                        if let Some(audio) = &self.audio_view.audio_opt {
                            let pipeline = audio.pipeline();
                            pipeline.set_property("current-text", code);
                            self.audio_view.current_text = pipeline.property("current-text");
                        }
                    }
                }
                crate::audio::audio_view::Message::ShowControls => {
                    self.audio_view.update_controls(true);
                }
                _ => self.audio_view.update(audio_message),
            },
            Message::Config(config) => {
                if config != self.config {
                    log::info!("update config");
                    // Show details is preserved for existing instances
                    let show_details = self.config.show_details;
                    self.config = config;
                    self.config.show_details = show_details;
                    return self.update_config();
                }
            }
            Message::Copy(entity_opt) => {
                let paths = self.selected_paths(entity_opt);
                let contents = ClipboardCopy::new(ClipboardKind::Copy, &paths);
                return clipboard::write_data(contents);
            }
            Message::Cut(entity_opt) => {
                let paths = self.selected_paths(entity_opt);
                let contents = ClipboardCopy::new(ClipboardKind::Cut, &paths);
                return clipboard::write_data(contents);
            }
            Message::CloseToast(id) => {
                self.toasts.remove(id);
            }
            Message::CosmicSettings(arg) => {
                //TODO: use special settings URL scheme instead?
                let mut command = process::Command::new("cosmic-settings");
                command.arg(arg);
                match spawn_detached(&mut command) {
                    Ok(()) => {}
                    Err(err) => {
                        log::warn!("failed to run cosmic-settings {}: {}", arg, err)
                    }
                }
            }
            Message::DialogCancel => {
                self.dialog_pages.pop_front();
            }
            Message::DialogComplete => {
                if let Some(dialog_page) = self.dialog_pages.pop_front() {
                    match dialog_page {
                        DialogPage::EmptyTrash => {
                            self.operation(Operation::EmptyTrash);
                        }
                        DialogPage::FailedOperation(id) => {
                            log::warn!("TODO: retry operation {}", id);
                        }
                        DialogPage::MountError {
                            mounter_key,
                            item,
                            error: _,
                        } => {
                            if let Some(mounter) = MOUNTERS.get(&mounter_key) {
                                return mounter.mount(item).map(|_| message::none());
                            }
                        }
                        DialogPage::NetworkAuth {
                            mounter_key: _,
                            uri: _,
                            auth,
                            auth_tx,
                        } => {
                            return Task::perform(
                                async move {
                                    auth_tx.send(auth).await.unwrap();
                                    message::none()
                                },
                                |x| x,
                            );
                        }
                        DialogPage::NetworkError {
                            mounter_key: _,
                            uri,
                            error: _,
                        } => {
                            //TODO: re-use mounter_key?
                            return Task::batch([
                                self.update(Message::NetworkDriveInput(uri)),
                                self.update(Message::NetworkDriveSubmit),
                            ]);
                        }
                        DialogPage::NewItem { parent, name, dir } => {
                            let path = parent.join(name);
                            self.operation(if dir {
                                Operation::NewFolder { path }
                            } else {
                                Operation::NewFolder { path }
                            });
                        }
                        DialogPage::NewTag { tag } => {
                            let tag_id =
                                crate::sql::insert_tag(self.sql_connection.clone(), 0, tag.clone())
                                    as u32;
                            let t = crate::sql::Tag {
                                tag_id,
                                tag: tag.clone(),
                            };
                            let mut tags = self.config.tags.clone();
                            //let taglocation = Location::Tag(t);
                            if !tags.iter().any(|f| f == &t) {
                                tags.push(t);
                            }
                            config_set!(tags, tags);
                            return self.update_config();
                        }
                        DialogPage::OpenWith {
                            path,
                            apps,
                            selected,
                            ..
                        } => {
                            if let Some(app) = apps.get(selected) {
                                if let Some(mut command) = app.command(Some(path.clone().into())) {
                                    match spawn_detached(&mut command) {
                                        Ok(()) => {
                                            let _ = recently_used_xbel::update_recently_used(
                                                &path,
                                                App::APP_ID.to_string(),
                                                "media-browser".to_string(),
                                                None,
                                            );
                                        }
                                        Err(err) => {
                                            log::warn!(
                                                "failed to open {:?} with {:?}: {}",
                                                path,
                                                app.id,
                                                err
                                            )
                                        }
                                    }
                                } else {
                                    log::warn!(
                                        "failed to open {:?} with {:?}: failed to get command",
                                        path,
                                        app.id
                                    );
                                }
                            }
                        }
                        DialogPage::RenameItem {
                            from, parent, name, ..
                        } => {
                            let to = parent.join(name);
                            self.operation(Operation::Rename { from, to });
                        }
                        DialogPage::Replace { .. } => {
                            log::warn!("replace dialog should be completed with replace result");
                        }
                    }
                }
            }
            Message::DialogPush(dialog_page) => {
                self.dialog_pages.push_back(dialog_page);
            }
            Message::DialogUpdate(dialog_page) => {
                if !self.dialog_pages.is_empty() {
                    self.dialog_pages[0] = dialog_page;
                }
            }
            Message::DialogUpdateComplete(dialog_page) => {
                return Task::batch([
                    self.update(Message::DialogUpdate(dialog_page)),
                    self.update(Message::DialogComplete),
                ]);
            }
            Message::ImageMessage(image_message) => match image_message {
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
                        let v = tab.selected_file_paths();
                        for path in v {
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
                        let v = tab.selected_file_paths();
                        for path in v {
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
            },
            Message::Key(modifiers, key) => {
                let entity = self.tab_model.active();
                for (key_bind, action) in self.key_binds.iter() {
                    if key_bind.matches(modifiers, &key) {
                        return self.update(action.message(Some(entity)));
                    }
                }
            }
            Message::MaybeExit => {
                if self.window_id_opt.is_none() && self.pending_operations.is_empty() {
                    // Exit if window is closed and there are no pending operations
                    process::exit(0);
                }
            }
            Message::MetadataDelete => {
                let mut metadata_path;
                match dirs::data_local_dir() {
                    Some(pb) => {
                        metadata_path = pb;
                        if !metadata_path.exists() {
                            let ret = std::fs::create_dir_all(metadata_path.clone());
                            if ret.is_err() {
                                log::warn!(
                                    "Failed to create directory {}",
                                    metadata_path.display()
                                );
                                metadata_path = dirs::home_dir().unwrap();
                            }
                        }
                    }
                    None => {
                        metadata_path = dirs::home_dir().unwrap();
                    }
                }
                let paths = vec![metadata_path];
                if !paths.is_empty() {
                    self.operation(Operation::Delete { paths });
                }
            }
            Message::LaunchUrl(url) => match open::that_detached(&url) {
                Ok(()) => {}
                Err(err) => {
                    log::warn!("failed to open {:?}: {}", url, err);
                }
            },
            Message::LaunchSearch(search_type, search_term) => {
                use crate::sql::SearchType as ST;
                self.search = crate::sql::SearchData {
                    ..Default::default()
                };
                self.search_previous.clear();
                self.search_previous
                    .extend(crate::sql::searches(self.sql_connection.clone()));
                match search_type {
                    ST::Director => {
                        self.search.director = true;
                        self.search.video = true;
                        self.search.from_string = search_term;
                    }
                    ST::Actor => {
                        self.search.actor = true;
                        self.search.video = true;
                        self.search.from_string = search_term;
                    }
                    ST::Artist => {
                        self.search.artist = true;
                        self.search.audio = true;
                        self.search.from_string = search_term;
                    }
                    ST::AlbumArtist => {
                        self.search.album_artist = true;
                        self.search.audio = true;
                        self.search.from_string = search_term;
                    }
                    ST::Album => {
                        self.search.album = true;
                        self.search.audio = true;
                        self.search.from_string = search_term;
                    }
                    ST::Composer => {
                        self.search.composer = true;
                        self.search.audio = true;
                        self.search.from_string = search_term;
                    }
                    ST::Genre => {
                        self.search.genre = true;
                        self.search.audio = true;
                        self.search.from_string = search_term;
                    }
                    ST::Tag => {
                        self.search.tags = true;
                        self.search.audio = true;
                        self.search.video = true;
                        self.search.image = true;
                        self.search.from_string = search_term;
                    }

                    _ => return Task::none(),
                }
                return self.update(Message::SearchCommit);
            }
            Message::Modifiers(modifiers) => {
                self.modifiers = modifiers;
            }
            Message::MoveToTrash(entity_opt) => {
                let paths = self.selected_paths(entity_opt);
                if !paths.is_empty() {
                    self.operation(Operation::Delete { paths });
                }
            }
            Message::MounterItems(mounter_key, mounter_items) => {
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

                return Task::batch(commands);
            }
            Message::MountResult(mounter_key, item, res) => match res {
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
            },
            Message::MouseScroll(delta) => {
                let delta_y = match delta {
                    cosmic::iced_core::mouse::ScrollDelta::Lines { y, .. } => y,
                    cosmic::iced_core::mouse::ScrollDelta::Pixels { y, .. } => y,
                };
                if self.active_view == Mode::Video {
                    let seconds = self.video_view.position + delta_y as f64 * 10.0;
                    let _ = self.update(Message::VideoMessage(
                        crate::video::video_view::Message::Seek(seconds),
                    ));
                    if let Some(video) = self.video_view.video_opt.as_mut() {
                        if video.paused() {
                            video.set_paused(false);
                        }
                    }
                } else if self.active_view == Mode::Audio {
                    let seconds = self.audio_view.position + delta_y as f64 * 10.0;
                    let _ = self.update(Message::AudioMessage(
                        crate::audio::audio_view::Message::Seek(seconds),
                    ));
                    if let Some(audio) = self.audio_view.audio_opt.as_mut() {
                        if audio.paused() {
                            audio.set_paused(false);
                        }
                    }
                }

                return Task::none();
            }
            Message::NetworkAuth(mounter_key, uri, auth, auth_tx) => {
                self.dialog_pages.push_back(DialogPage::NetworkAuth {
                    mounter_key,
                    uri,
                    auth,
                    auth_tx,
                });
            }
            Message::NetworkDriveInput(input) => {
                self.network_drive_input = input;
            }
            Message::NetworkDriveSubmit => {
                //TODO: know which mounter to use for network drives
                for (mounter_key, mounter) in MOUNTERS.iter() {
                    self.network_drive_connecting =
                        Some((*mounter_key, self.network_drive_input.clone()));
                    return mounter
                        .network_drive(self.network_drive_input.clone())
                        .map(|_| message::none());
                }
                log::warn!(
                    "no mounter found for connecting to {:?}",
                    self.network_drive_input
                );
            }
            Message::NetworkResult(mounter_key, uri, res) => {
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
            }
            Message::NewItem(entity_opt, dir) => {
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
            }
            #[cfg(feature = "notify")]
            Message::Notification(notification) => {
                self.notification_opt = Some(notification);
            }
            Message::NotifyEvents(events) => {
                log::debug!("{:?}", events);

                let mut needs_reload = Vec::new();
                let entities: Vec<_> = self.tab_model.iter().collect();
                for entity in entities {
                    if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
                        if let Some(path) = &tab.location.path_opt() {
                            let mut contains_change = false;
                            for event in events.iter() {
                                for event_path in event.paths.iter() {
                                    if event_path.starts_with(&path) {
                                        match event.kind {
                                            notify::EventKind::Modify(
                                                notify::event::ModifyKind::Metadata(_),
                                            )
                                            | notify::EventKind::Modify(
                                                notify::event::ModifyKind::Data(_),
                                            ) => {
                                                // If metadata or data changed, find the matching item and reload it
                                                //TODO: this could be further optimized by looking at what exactly changed
                                                if let Some(items) = &mut tab.items_opt {
                                                    for item in items.iter_mut() {
                                                        if item.path_opt() == Some(event_path) {
                                                            //TODO: reload more, like mime types?
                                                            match fs::metadata(&event_path) {
                                                                Ok(new_metadata) => match &mut item
                                                                    .metadata
                                                                {
                                                                    ItemMetadata::Path {
                                                                        metadata,
                                                                        ..
                                                                    } => *metadata = new_metadata,
                                                                    _ => {}
                                                                },
                                                                Err(err) => {
                                                                    log::warn!("failed to reload metadata for {:?}: {}", path, err);
                                                                }
                                                            }
                                                            //TODO item.thumbnail_opt =
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {
                                                // Any other events reload the whole tab
                                                contains_change = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            if contains_change {
                                needs_reload.push((entity, tab.location.clone()));
                            }
                        }
                    }
                }

                let mut commands = Vec::with_capacity(needs_reload.len());
                for (entity, location) in needs_reload {
                    commands.push(self.rescan_tab(entity, location, None));
                }
                return Task::batch(commands);
            }
            Message::NotifyWatcher(mut watcher_wrapper) => match watcher_wrapper.watcher_opt.take()
            {
                Some(watcher) => {
                    self.watcher_opt = Some((watcher, HashSet::new()));
                    return self.update_watcher();
                }
                None => {
                    log::warn!("message did not contain notify watcher");
                }
            },
            Message::OpenInNewWindow(entity_opt) => match env::current_exe() {
                Ok(exe) => self
                    .selected_paths(entity_opt)
                    .into_iter()
                    .filter(|p| p.is_dir())
                    .for_each(|path| match process::Command::new(&exe).arg(path).spawn() {
                        Ok(_child) => {}
                        Err(err) => {
                            log::error!("failed to execute {:?}: {}", exe, err);
                        }
                    }),
                Err(err) => {
                    log::error!("failed to get current executable path: {}", err);
                }
            },
            Message::OpenItemLocation(entity_opt) => {
                log::warn!("OpenItemLocation");
                return Task::batch(self.selected_paths(entity_opt).into_iter().filter_map(
                    |path| {
                        if let Some(parent) = path.parent() {
                            Some(self.open_tab(
                                Location::Path(parent.to_path_buf()),
                                true,
                                Some(vec![path]),
                            ))
                        } else {
                            None
                        }
                    },
                ));
            }
            Message::Paste(entity_opt) => {
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
            }
            Message::PasteContents(to, mut contents) => {
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
            }
            Message::PendingCancel(id) => {
                if let Some((_, controller)) = self.pending_operations.get(&id) {
                    controller.cancel();
                    self.progress_operations.remove(&id);
                }
            }
            Message::PendingCancelAll => {
                for (id, (_, controller)) in self.pending_operations.iter() {
                    controller.cancel();
                    self.progress_operations.remove(&id);
                }
            }
            Message::PendingComplete(id, op_sel) => {
                let mut commands = Vec::with_capacity(4);
                // Show toast for some operations
                if let Some((op, _)) = self.pending_operations.remove(&id) {
                    if let Some(description) = op.toast() {
                        if let Operation::Delete { ref paths } = op {
                            let paths: Arc<[PathBuf]> = Arc::from(paths.as_slice());
                            commands.push(
                                self.toasts
                                    .push(
                                        widget::toaster::Toast::new(description)
                                            .action(fl!("undo"), move |tid| {
                                                Message::UndoTrash(tid, paths.clone())
                                            }),
                                    )
                                    .map(cosmic::app::Message::App),
                            );
                        }
                    }
                    self.complete_operations.insert(id, op);
                }
                // Close progress notification if all relavent operations are finished
                if !self
                    .pending_operations
                    .iter()
                    .any(|(_id, (op, _))| op.show_progress_notification())
                {
                    self.progress_operations.clear();
                }
                // Potentially show a notification
                commands.push(self.update_notification());
                // Rescan and select based on operation
                commands.push(self.rescan_operation_selection(op_sel));
                // Manually rescan any trash tabs after any operation is completed
                commands.push(self.rescan_trash());
                // if search is active, update "search" tab view
                commands.push(self.search());
                return Task::batch(commands);
            }
            Message::PendingDismiss => {
                self.progress_operations.clear();
            }
            Message::PendingError(id, err) => {
                if let Some((op, controller)) = self.pending_operations.remove(&id) {
                    // Only show dialog if not cancelled
                    if !controller.is_cancelled() {
                        self.dialog_pages.push_back(DialogPage::FailedOperation(id));
                    }
                    // Remove from progress
                    self.progress_operations.remove(&id);
                    self.failed_operations.insert(id, (op, controller, err));
                }
                // Close progress notification if all relavent operations are finished
                if !self
                    .pending_operations
                    .iter()
                    .any(|(_id, (op, _))| op.show_progress_notification())
                {
                    self.progress_operations.clear();
                }
                // Manually rescan any trash tabs after any operation is completed
                return self.rescan_trash();
            }
            Message::PendingPause(id, pause) => {
                if let Some((_, controller)) = self.pending_operations.get(&id) {
                    if pause {
                        controller.pause();
                    } else {
                        controller.unpause();
                    }
                }
            }
            Message::PendingPauseAll(pause) => {
                for (_id, (_, controller)) in self.pending_operations.iter() {
                    if pause {
                        controller.pause();
                    } else {
                        controller.unpause();
                    }
                }
            }
            Message::Preview(entity_opt) => {
                match self.mode {
                    Mode::App => {
                        let show_details = !self.config.show_details;
                        //TODO: move to update_config?
                        self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                        self.core.window.show_context = show_details;
                        config_set!(show_details, show_details);
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
                        config_set!(show_details, show_details);
                        return self.update_config();
                    }
                    Mode::Image => {
                        let show_details = !self.config.show_details;
                        //TODO: move to update_config?
                        self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                        self.core.window.show_context = show_details;
                        config_set!(show_details, show_details);
                        return self.update_config();
                    }
                    Mode::Video => {
                        let show_details = !self.config.show_details;
                        //TODO: move to update_config?
                        self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                        self.core.window.show_context = show_details;
                        config_set!(show_details, show_details);
                        return self.update_config();
                    }
                    Mode::Audio => {
                        let show_details = !self.config.show_details;
                        //TODO: move to update_config?
                        self.context_page = ContextPage::Preview(None, PreviewKind::Selected);
                        self.core.window.show_context = show_details;
                        config_set!(show_details, show_details);
                        return self.update_config();
                    }
                }
            }
            Message::RescanTrash => {
                // Update trash icon if empty/full
                let maybe_entity = self.nav_model.iter().find(|&entity| {
                    self.nav_model
                        .data::<Location>(entity)
                        .map(|loc| matches!(loc, Location::Trash))
                        .unwrap_or_default()
                });
                if let Some(entity) = maybe_entity {
                    self.nav_model
                        .icon_set(entity, widget::icon::icon(tab::trash_icon_symbolic(16)));
                }

                return Task::batch([self.rescan_trash()]);
            }

            Message::Rename(entity_opt) => {
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
            }
            Message::RenameWithPattern(entity_opt, pattern, start_val, numdigits) => {
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
                                crate::cmd::rename_with_pattern(
                                    selected, pattern, start_val, numdigits,
                                )
                            });
                        }
                    }
                }
            }
            Message::ReplaceResult(replace_result) => {
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
            }
            Message::RestoreFromTrash(entity_opt) => {
                let mut trash_items = Vec::new();
                let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
                if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
                    if let Some(items) = tab.items_opt() {
                        for item in items.iter() {
                            if item.selected {
                                match &item.metadata {
                                    ItemMetadata::Trash { entry, .. } => {
                                        trash_items.push(entry.clone());
                                    }
                                    _ => {
                                        //TODO: error on trying to restore non-trash file?
                                    }
                                }
                            }
                        }
                    }
                }
                if !trash_items.is_empty() {
                    self.operation(Operation::Restore { items: trash_items });
                }
            }
            Message::SearchActivate => {
                return if self.search_get().is_none() {
                    self.search_set(Some(String::new()))
                } else {
                    widget::text_input::focus(self.search_id.clone())
                };
            }
            Message::SearchClear => {
                return self.search_set(None);
            }
            Message::SearchInput(input) => {
                return self.search_set(Some(input));
            }
            Message::SearchStart => {
                self.search = crate::sql::SearchData {
                    ..Default::default()
                };
                self.search_previous.clear();
                self.search_previous
                    .extend(crate::sql::searches(self.sql_connection.clone()));
                for s in self.search_previous.iter() {
                    self.search_previous_str.push(s.display());
                }
                self.context_page = ContextPage::Search;
                self.core.window.show_context = true;
            }
            Message::SearchPreviousPick(pos) => {
                self.search_previous_pos = pos;
            }
            Message::SearchPreviousSelect => {
                let search = self.search_previous[self.search_previous_pos].clone();
                self.search = search.clone();
            }
            Message::SearchPreviousDelete => {
                let search = self.search_previous[self.search_previous_pos].clone();
                self.search_previous_str.remove(self.search_previous_pos);
                self.search_previous.remove(self.search_previous_pos);
                crate::sql::delete_search(self.sql_connection.clone(), search);
            }
            Message::SearchImages(is_checked) => {
                self.search.search_id = 0;
                self.search.image = is_checked;
            }
            Message::SearchVideos(is_checked) => {
                self.search.search_id = 0;
                self.search.video = is_checked;
            }
            Message::SearchAudios(is_checked) => {
                self.search.search_id = 0;
                self.search.audio = is_checked;
            }
            Message::SearchSearchString(input) => {
                self.search.search_id = 0;
                self.search.search_string = input.clone();
            }
            Message::SearchSearchStringSubmit => {
                self.search.search_id = 0;
                log::warn!("{}", self.search.search_string);
            }
            Message::SearchSearchFromString(input) => {
                self.search.search_id = 0;
                self.search.from_string = input.clone();
            }
            Message::SearchSearchFromStringSubmit => {
                self.search.search_id = 0;
                log::warn!("{}", self.search.from_string);
                let lt = crate::sql::string_to_linux_time(&self.search.from_string);
                if lt > 0 {
                    self.search.from_date = lt as i64;
                }
                match self.search.from_string.parse::<f32>() {
                    Ok(float) => self.search.from_value = (float * 1000000.0) as u64,
                    Err(_) => {}
                }
            }
            Message::SearchSearchToString(input) => {
                self.search.search_id = 0;
                self.search.to_string = input.clone();
            }
            Message::SearchSearchToStringSubmit => {
                self.search.search_id = 0;
                log::warn!("{}", self.search.to_string);
                let lt = crate::sql::string_to_linux_time(&self.search.to_string);
                if lt > 0 {
                    self.search.to_date = lt as i64;
                }
                match self.search.to_string.parse::<f32>() {
                    Ok(float) => self.search.to_value = (float * 1000000.0) as u64,
                    Err(_) => {}
                }
            }
            Message::SearchSearchFromValue(input) => {
                self.search.search_id = 0;
                self.search.from_value_string = input.clone();
                let float = crate::parsers::string_to_float(&self.search.from_value_string);
                self.search.from_value = (float * 1000000.0) as u64;
            }
            Message::SearchSearchFromValueSubmit => {
                self.search.search_id = 0;
                log::warn!("{}", self.search.search_string);
            }
            Message::SearchSearchToValue(input) => {
                self.search.search_id = 0;
                self.search.to_value_string = input.clone();
                let float = crate::parsers::string_to_float(&self.search.to_value_string);
                self.search.to_value = (float * 1000000.0) as u64;
            }
            Message::SearchSearchToValueSubmit => {
                self.search.search_id = 0;
                log::warn!("{}", self.search.to_value);
            }
            Message::SearchFilepath(is_checked) => {
                self.search.search_id = 0;
                self.search.filepath = is_checked;
            }
            Message::SearchTitle(is_checked) => {
                self.search.search_id = 0;
                self.search.title = is_checked;
            }
            Message::SearchTag(is_checked) => {
                self.search.search_id = 0;
                self.search.tags = is_checked;
            }
            Message::SearchDescription(is_checked) => {
                self.search.search_id = 0;
                self.search.description = is_checked;
                if !self.search.video {
                    self.search.video = true;
                }
            }
            Message::SearchActor(is_checked) => {
                self.search.search_id = 0;
                self.search.actor = is_checked;
                if !self.search.video {
                    self.search.video = true;
                }
            }
            Message::SearchDirector(is_checked) => {
                self.search.search_id = 0;
                self.search.director = is_checked;
                if !self.search.video {
                    self.search.video = true;
                }
            }
            Message::SearchArtist(is_checked) => {
                self.search.search_id = 0;
                self.search.artist = is_checked;
                if !self.search.audio {
                    self.search.audio = true;
                }
            }
            Message::SearchAlbumartist(is_checked) => {
                self.search.search_id = 0;
                self.search.album_artist = is_checked;
                if !self.search.audio {
                    self.search.audio = true;
                }
            }
            Message::SearchAlbum(is_checked) => {
                self.search.search_id = 0;
                self.search.album = is_checked;
                if !self.search.audio {
                    self.search.audio = true;
                }
            }
            Message::SearchComposer(is_checked) => {
                self.search.search_id = 0;
                self.search.composer = is_checked;
                if !self.search.audio {
                    self.search.audio = true;
                }
            }
            Message::SearchGenre(is_checked) => {
                self.search.search_id = 0;
                self.search.genre = is_checked;
                if !self.search.audio {
                    self.search.audio = true;
                }
            }
            Message::SearchDuration(is_checked) => {
                self.search.search_id = 0;
                self.search.duration = is_checked;
            }
            Message::SearchCreationDate(is_checked) => {
                self.search.search_id = 0;
                self.search.creation_date = is_checked;
            }
            Message::SearchModificationDate(is_checked) => {
                self.search.search_id = 0;
                self.search.modification_date = is_checked;
            }
            Message::SearchReleaseDate(is_checked) => {
                self.search.search_id = 0;
                self.search.release_date = is_checked;
            }
            Message::SearchLenseModel(is_checked) => {
                self.search.search_id = 0;
                self.search.lense_model = is_checked;
                if !self.search.image {
                    self.search.image = true;
                }
            }
            Message::SearchFocalLength(is_checked) => {
                self.search.search_id = 0;
                self.search.focal_length = is_checked;
                if !self.search.image {
                    self.search.image = true;
                }
            }
            Message::SearchExposureTime(is_checked) => {
                self.search.search_id = 0;
                self.search.exposure_time = is_checked;
                if !self.search.image {
                    self.search.image = true;
                }
            }
            Message::SearchFNumber(is_checked) => {
                self.search.search_id = 0;
                self.search.fnumber = is_checked;
                if !self.search.image {
                    self.search.image = true;
                }
            }
            Message::SearchGpsLatitude(is_checked) => {
                self.search.search_id = 0;
                self.search.gps_latitude = is_checked;
                if !self.search.image {
                    self.search.image = true;
                }
            }
            Message::SearchGpsLongitude(is_checked) => {
                self.search.search_id = 0;
                self.search.gps_longitude = is_checked;
                if !self.search.image {
                    self.search.image = true;
                }
            }
            Message::SearchGpsAltitude(is_checked) => {
                self.search.search_id = 0;
                self.search.gps_altitude = is_checked;
                if !self.search.image {
                    self.search.image = true;
                }
            }
            Message::SearchCommit => {
                let mut s = self.search.clone();
                for s2 in self.search_previous.iter() {
                    if &s == s2 {
                        s.search_id = s2.search_id;
                    }
                }
                if s.search_id == 0 {
                    s.store(self.sql_connection.clone());
                    self.search_previous.push(s.clone());
                }
                self.search = s.clone();
                let location = Location::DBSearch(s);
                let (parent_item_opt, items) =
                    location.scan(self.sql_connection.clone(), IconSizes::default());
                let (entity, command) = self.open_tab_entity(location, true, None);
                if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
                    tab.parent_item_opt = parent_item_opt;
                    tab.set_items(items);
                }
                return command;
            }
            Message::SeekBackward => {
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
            }
            Message::SeekForward => {
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
            }
            Message::SkipToPosition(seconds) => {
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
            }
            Message::SetShowDetails(show_details) => {
                config_set!(show_details, show_details);
                return self.update_config();
            }
            Message::SystemThemeModeChange(_theme_mode) => {
                return self.update_config();
            }
            Message::TabActivate(entity) => {
                self.tab_model.activate(entity);

                if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                    self.activate_nav_model_location(&tab.location.clone());
                }
                return self.update_title();
            }
            Message::TabNext => {
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
            }
            Message::TabPrev => {
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
            }
            Message::TabClose(entity_opt) => {
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

                return Task::batch([self.update_title(), self.update_watcher()]);
            }
            Message::TabConfig(config) => {
                if config != self.config.tab {
                    config_set!(tab, config);
                    return self.update_config();
                }
            }
            Message::ToggleFoldersFirst => {
                let mut config = self.config.tab;
                config.folders_first = !config.folders_first;
                return self.update(Message::TabConfig(config));
            }
            Message::TabMessage(entity_opt, tab_message) => {
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
                            config_set!(favorites, favorites);
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
                            self.operation(Operation::Delete { paths });
                        }
                        tab::Command::Open(filepath) => {
                            return match self.open_path(filepath) {
                                Some(command) => command,
                                _ => Task::none(),
                            };
                        }
                        tab::Command::OpenInExternalApp(path) => {
                            let mut found_desktop_exec = false;
                            if mime_icon::mime_for_path(&path) == "application/x-desktop" {
                                match freedesktop_entry_parser::parse_entry(&path) {
                                    Ok(entry) => {
                                        match entry.section("Desktop Entry").attr("Exec") {
                                            Some(exec) => {
                                                match mime_app::exec_to_command(exec, None) {
                                                    Some(mut command) => {
                                                        match spawn_detached(&mut command) {
                                                            Ok(()) => {
                                                                found_desktop_exec = true;
                                                            }
                                                            Err(err) => {
                                                                log::warn!(
                                                                    "failed to execute {:?}: {}",
                                                                    path,
                                                                    err
                                                                );
                                                            }
                                                        }
                                                    }
                                                    None => {
                                                        log::warn!("failed to parse {:?}: invalid Desktop Entry/Exec", path);
                                                    }
                                                }
                                            }
                                            None => {
                                                log::warn!("failed to parse {:?}: missing Desktop Entry/Exec", path);
                                            }
                                        }
                                    }
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
            Message::TabNew => {
                let active = self.tab_model.active();
                let location = match self.tab_model.data::<Tab>(active) {
                    Some(tab) => tab.location.clone(),
                    None => Location::Path(home_dir()),
                };
                let _ = self.open_tab_entity(location, true, None);
            }
            Message::TabRescan(entity, location, parent_item_opt, items, selection_paths) => {
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
            }
            Message::TabView(entity_opt, view) => {
                let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
                if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
                    tab.config.view = view;
                }
                let mut config = self.config.tab;
                config.view = view;
                return self.update(Message::TabConfig(config));
            }
            Message::ToggleContextPage(context_page) => {
                //TODO: ensure context menus are closed
                if self.context_page == context_page {
                    self.set_show_context(!self.core.window.show_context);
                } else {
                    self.set_show_context(true);
                }
                self.context_page = context_page;
            }
            Message::Undo(_id) => {
                // TODO: undo
            }
            Message::UndoTrash(id, recently_trashed) => {
                self.toasts.remove(id);

                let mut paths = Vec::with_capacity(recently_trashed.len());
                let icon_sizes = self.config.tab.icon_sizes;
                let sql_connection = self.sql_connection.clone();

                return cosmic::task::future(async move {
                    match tokio::task::spawn_blocking(move || {
                        Location::Trash.scan(sql_connection, icon_sizes)
                    })
                    .await
                    {
                        Ok((_parent_item_opt, items)) => {
                            for path in &*recently_trashed {
                                for item in &items {
                                    if let ItemMetadata::Trash { ref entry, .. } = item.metadata {
                                        let original_path = entry.original_path();
                                        if &original_path == path {
                                            paths.push(entry.clone());
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            log::warn!("failed to rescan: {}", err);
                        }
                    }

                    Message::UndoTrashStart(paths)
                });
            }
            Message::UndoTrashStart(items) => {
                self.operation(Operation::Restore { items });
            }
            Message::VideoMessage(video_message) => match video_message {
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
                        let v = tab.selected_file_paths();
                        for path in v {
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
                        let v = tab.selected_file_paths();
                        for path in v {
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
                    return window::change_mode(
                        window::Id::RESERVED,
                        if self.video_view.fullscreen {
                            window::Mode::Fullscreen
                        } else {
                            window::Mode::Windowed
                        },
                    );
                }
                crate::video::video_view::Message::AudioCode(code) => {
                    if let Ok(code) = i32::try_from(code) {
                        if let Some(video) = &self.video_view.video_opt {
                            let pipeline = video.pipeline();
                            pipeline.set_property("current-audio", code);
                            self.video_view.current_audio = pipeline.property("current-audio");
                        }
                    }
                }
                crate::video::video_view::Message::AudioToggle => {
                    if let Some(video) = &mut self.video_view.video_opt {
                        video.set_muted(!video.muted());
                        self.video_view.update_controls(true);
                    }
                }
                crate::video::video_view::Message::AudioVolume(volume) => {
                    if let Some(video) = &mut self.video_view.video_opt {
                        video.set_volume(volume);
                        self.video_view.update_controls(true);
                    }
                }
                crate::video::video_view::Message::TextCode(code) => {
                    if let Ok(code) = i32::try_from(code) {
                        if let Some(video) = &self.video_view.video_opt {
                            let pipeline = video.pipeline();
                            pipeline.set_property("current-text", code);
                            self.video_view.current_text = pipeline.property("current-text");
                        }
                    }
                }
                crate::video::video_view::Message::ShowControls => {
                    self.video_view.update_controls(true);
                }
                _ => self.video_view.update(video_message),
            },
            Message::WindowClose => {
                if let Some(window_id) = self.window_id_opt.take() {
                    return Task::batch([
                        window::close(window_id),
                        Task::perform(async move { message::app(Message::MaybeExit) }, |x| x),
                    ]);
                }
            }
            Message::WindowNew => match env::current_exe() {
                Ok(exe) => match process::Command::new(&exe).spawn() {
                    Ok(_child) => {}
                    Err(err) => {
                        log::error!("failed to execute {:?}: {}", exe, err);
                    }
                },
                Err(err) => {
                    log::error!("failed to get current executable path: {}", err);
                }
            },
            Message::ZoomDefault(entity_opt) => {
                if self.active_view == Mode::Browser || self.active_view == Mode::App {
                    let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
                    let mut config = self.config.tab;
                    if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                        match tab.config.view {
                            tab::View::List => config.icon_sizes.list = 100.try_into().unwrap(),
                            tab::View::Grid => config.icon_sizes.grid = 100.try_into().unwrap(),
                        }
                    }
                    return self.update(Message::TabConfig(config));
                } else if self.active_view == Mode::Image {
                }
            }
            Message::ZoomIn(entity_opt) => {
                let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
                let zoom_in = |size: &mut NonZeroU16, min: u16, max: u16| {
                    let mut step = min;
                    while step <= max {
                        if size.get() < step {
                            *size = step.try_into().unwrap();
                            break;
                        }
                        step += 25;
                    }
                    if size.get() > step {
                        *size = step.try_into().unwrap();
                    }
                };
                let mut config = self.config.tab;
                if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                    match tab.config.view {
                        tab::View::List => zoom_in(&mut config.icon_sizes.list, 50, 500),
                        tab::View::Grid => zoom_in(&mut config.icon_sizes.grid, 50, 500),
                    }
                }
                return self.update(Message::TabConfig(config));
            }
            Message::ZoomOut(entity_opt) => {
                let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
                let zoom_out = |size: &mut NonZeroU16, min: u16, max: u16| {
                    let mut step = max;
                    while step >= min {
                        if size.get() > step {
                            *size = step.try_into().unwrap();
                            break;
                        }
                        step -= 25;
                    }
                    if size.get() < step {
                        *size = step.try_into().unwrap();
                    }
                };
                let mut config = self.config.tab;
                if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                    match tab.config.view {
                        tab::View::List => zoom_out(&mut config.icon_sizes.list, 50, 500),
                        tab::View::Grid => zoom_out(&mut config.icon_sizes.grid, 50, 500),
                    }
                }
                return self.update(Message::TabConfig(config));
            }
            Message::DndEnterNav(entity) => {
                if let Some(location) = self.nav_model.data::<Location>(entity) {
                    self.nav_dnd_hover = Some((location.clone(), Instant::now()));
                    let location = location.clone();
                    return Task::perform(tokio::time::sleep(HOVER_DURATION), move |_| {
                        cosmic::app::Message::App(Message::DndHoverLocTimeout(location.clone()))
                    });
                }
            }
            Message::DndExitNav => {
                self.nav_dnd_hover = None;
            }
            Message::DndDropNav(entity, data, action) => {
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
            }
            Message::DndHoverLocTimeout(location) => {
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
            }
            Message::DndEnterTab(entity) => {
                self.tab_dnd_hover = Some((entity, Instant::now()));
                return Task::perform(tokio::time::sleep(HOVER_DURATION), move |_| {
                    cosmic::app::Message::App(Message::DndHoverTabTimeout(entity))
                });
            }
            Message::DndExitTab => {
                self.nav_dnd_hover = None;
            }
            Message::DndDropTab(entity, data, action) => {
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
            }
            Message::DndHoverTabTimeout(entity) => {
                if self
                    .tab_dnd_hover
                    .as_ref()
                    .is_some_and(|(e, i)| *e == entity && i.elapsed() >= HOVER_DURATION)
                {
                    self.tab_dnd_hover = None;
                    return self.update(Message::TabActivate(entity));
                }
            }

            Message::NavBarClose(entity) => {
                if let Some(data) = self.nav_model.data::<MounterData>(entity) {
                    if let Some(mounter) = MOUNTERS.get(&data.0) {
                        return mounter.unmount(data.1.clone()).map(|_| message::none());
                    }
                }
            }

            // Tracks which nav bar item to show a context menu for.
            Message::NavBarContext(entity) => {
                // Close location editing if enabled
                let tab_entity = self.tab_model.active();
                if let Some(tab) = self.tab_model.data_mut::<Tab>(tab_entity) {
                    tab.edit_location = None;
                }

                self.nav_bar_context_id = entity;
            }

            // Applies selected nav bar context menu operation.
            Message::NavMenuAction(action) => match action {
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
                NavMenuAction::RemoveFromSidebar(entity) => {
                    if let Some(FavoriteIndex(favorite_i)) =
                        self.nav_model.data::<FavoriteIndex>(entity)
                    {
                        let mut favorites = self.config.favorites.clone();
                        favorites.remove(*favorite_i);
                        config_set!(favorites, favorites);
                        return self.update_config();
                    }
                }
                NavMenuAction::RemoveTagFromSidebar(entity) => {
                    if let Some(FavoriteIndex(tag_i)) = self.nav_model.data::<FavoriteIndex>(entity)
                    {
                        let mut tags = self.config.tags.clone();
                        crate::sql::delete_tag(
                            self.sql_connection.clone(),
                            tags[*tag_i].tag.clone(),
                        );
                        tags.remove(*tag_i);
                        config_set!(tags, tags);
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
                        match super::parsers::item_from_path(path, IconSizes::default()) {
                            Ok(item) => {
                                self.context_page = ContextPage::Preview(
                                    None,
                                    PreviewKind::Custom(PreviewItem(item)),
                                );
                                self.set_show_context(true);
                            }
                            Err(err) => {
                                log::warn!("failed to get item from path {:?}: {}", path, err);
                            }
                        }
                    }
                }
            },
            Message::Recents => {}
            #[cfg(feature = "wayland")]
            Message::OutputEvent(output_event, output) => {
                match output_event {
                    OutputEvent::Created(output_info_opt) => {
                        log::info!("output {}: created", output.id());

                        let surface_id = WindowId::unique();
                        match self.surface_ids.insert(output.clone(), surface_id) {
                            Some(old_surface_id) => {
                                //TODO: remove old surface?
                                log::warn!(
                                    "output {}: already had surface ID {:?}",
                                    output.id(),
                                    old_surface_id
                                );
                            }
                            None => {}
                        }

                        let display = match output_info_opt {
                            Some(output_info) => match output_info.name {
                                Some(output_name) => {
                                    self.surface_names.insert(surface_id, output_name.clone());
                                    output_name
                                }
                                None => {
                                    log::warn!("output {}: no output name", output.id());
                                    String::new()
                                }
                            },
                            None => {
                                log::warn!("output {}: no output info", output.id());
                                String::new()
                            }
                        };

                        let (entity, command) = self.open_tab_entity(
                            Location::Desktop(crate::desktop_dir(), display, self.config.desktop),
                            false,
                            None,
                        );
                        self.windows.insert(surface_id, WindowKind::Desktop(entity));
                        return Task::batch([
                            command,
                            get_layer_surface(SctkLayerSurfaceSettings {
                                id: surface_id,
                                layer: Layer::Bottom,
                                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                                pointer_interactivity: true,
                                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                                output: IcedOutput::Output(output),
                                namespace: "cosmic-files-applet".into(),
                                size: Some((None, None)),
                                margin: IcedMargin {
                                    top: 0,
                                    bottom: 0,
                                    left: 0,
                                    right: 0,
                                },
                                exclusive_zone: 0,
                                size_limits: Limits::NONE.min_width(1.0).min_height(1.0),
                            }),
                            #[cfg(feature = "wayland")]
                            overlap_notify(surface_id, true),
                        ]);
                    }
                    OutputEvent::Removed => {
                        log::info!("output {}: removed", output.id());
                        match self.surface_ids.remove(&output) {
                            Some(surface_id) => {
                                self.remove_window(&surface_id);
                                self.surface_names.remove(&surface_id);
                                return destroy_layer_surface(surface_id);
                            }
                            None => {
                                log::warn!("output {}: no surface found", output.id());
                            }
                        }
                    }
                    OutputEvent::InfoUpdate(_output_info) => {
                        log::info!("output {}: info update", output.id());
                    }
                }
            }
            Message::Cosmic(cosmic) => {
                // Forward cosmic messages
                return Task::perform(async move { cosmic }, |cosmic| message::cosmic(cosmic));
            }
            Message::None => {}
            Message::Size(size) => {
                self.size = Some(size);
            }
        }

        Task::none()
    }

    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Message>> {
        if !self.core.window.show_context {
            return None;
        }

        Some(match &self.context_page {
            ContextPage::About => context_drawer::context_drawer(
                self.about(),
                Message::ToggleContextPage(ContextPage::About),
            ),
            ContextPage::EditHistory => context_drawer::context_drawer(
                self.edit_history(),
                Message::ToggleContextPage(ContextPage::EditHistory),
            )
            .title(fl!("edit-history")),
            ContextPage::NetworkDrive => {
                let mut text_input =
                    widget::text_input(fl!("enter-server-address"), &self.network_drive_input);
                let button = if self.network_drive_connecting.is_some() {
                    widget::button::standard(fl!("connecting"))
                } else {
                    text_input = text_input
                        .on_input(Message::NetworkDriveInput)
                        .on_submit(Message::NetworkDriveSubmit);
                    widget::button::standard(fl!("connect")).on_press(Message::NetworkDriveSubmit)
                };
                context_drawer::context_drawer(
                    self.network_drive(),
                    Message::ToggleContextPage(ContextPage::NetworkDrive),
                )
                .title(fl!("add-network-drive"))
                .header(text_input)
                .footer(widget::row::with_children(vec![
                    widget::horizontal_space().into(),
                    button.into(),
                ]))
            }
            ContextPage::Preview(entity_opt, kind) => {
                let mut actions = Vec::with_capacity(3);
                let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
                if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                    if let Some(items) = tab.items_opt() {
                        for item in items.iter() {
                            if item.selected {
                                actions.extend(item.preview_header())
                            }
                        }
                    }
                };
                context_drawer::context_drawer(
                    self.preview(entity_opt, kind, true),
                    Message::ToggleContextPage(ContextPage::Preview(
                        entity_opt.clone(),
                        kind.clone(),
                    )),
                )
                .header_actions(actions)
            }
            ContextPage::Settings => context_drawer::context_drawer(
                self.settings(),
                Message::ToggleContextPage(ContextPage::Settings),
            )
            .title(fl!("settings")),
            ContextPage::Search => context_drawer::context_drawer(
                self.search_database(),
                Message::ToggleContextPage(ContextPage::Search),
            )
            .title(fl!("search-context")),
        })
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        //TODO: should gallery view just be a dialog?
        let entity = self.tab_model.active();

        let dialog_page = match self.dialog_pages.front() {
            Some(some) => some,
            None => return None,
        };

        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let dialog = match dialog_page {
            DialogPage::EmptyTrash => widget::dialog()
                .title(fl!("empty-trash"))
                .body(fl!("empty-trash-warning"))
                .primary_action(
                    widget::button::suggested(fl!("empty-trash")).on_press(Message::DialogComplete),
                )
                .secondary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                ),
            DialogPage::FailedOperation(id) => {
                //TODO: try next dialog page (making sure index is used by Dialog messages)?
                let (operation, _, err) = self.failed_operations.get(id)?;

                //TODO: nice description of error
                widget::dialog()
                    .title("Failed operation")
                    .body(format!("{:#?}\n{}", operation, err))
                    .icon(widget::icon::from_name("dialog-error").size(64))
                    //TODO: retry action
                    .primary_action(
                        widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                    )
            }
            DialogPage::MountError {
                mounter_key: _,
                item: _,
                error,
            } => widget::dialog()
                .title("mount-error".to_string())
                .body(error)
                .icon(widget::icon::from_name("dialog-error").size(64))
                .primary_action(
                    widget::button::standard(fl!("try-again")).on_press(Message::DialogComplete),
                )
                .secondary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                ),
            DialogPage::NetworkAuth {
                mounter_key,
                uri,
                auth,
                auth_tx,
            } => {
                //TODO: use URI!
                let mut controls = Vec::with_capacity(4);
                if let Some(username) = &auth.username_opt {
                    //TODO: what should submit do?
                    controls.push(
                        widget::text_input(fl!("username"), username)
                            .on_input(move |value| {
                                Message::DialogUpdate(DialogPage::NetworkAuth {
                                    mounter_key: *mounter_key,
                                    uri: uri.clone(),
                                    auth: MounterAuth {
                                        username_opt: Some(value),
                                        ..auth.clone()
                                    },
                                    auth_tx: auth_tx.clone(),
                                })
                            })
                            .into(),
                    );
                }
                if let Some(domain) = &auth.domain_opt {
                    //TODO: what should submit do?
                    controls.push(
                        widget::text_input(fl!("domain"), domain)
                            .on_input(move |value| {
                                Message::DialogUpdate(DialogPage::NetworkAuth {
                                    mounter_key: *mounter_key,
                                    uri: uri.clone(),
                                    auth: MounterAuth {
                                        domain_opt: Some(value),
                                        ..auth.clone()
                                    },
                                    auth_tx: auth_tx.clone(),
                                })
                            })
                            .into(),
                    );
                }
                if let Some(password) = &auth.password_opt {
                    //TODO: what should submit do?
                    //TODO: button for showing password
                    controls.push(
                        widget::secure_input(fl!("password"), password, None, true)
                            .on_input(move |value| {
                                Message::DialogUpdate(DialogPage::NetworkAuth {
                                    mounter_key: *mounter_key,
                                    uri: uri.clone(),
                                    auth: MounterAuth {
                                        password_opt: Some(value),
                                        ..auth.clone()
                                    },
                                    auth_tx: auth_tx.clone(),
                                })
                            })
                            .into(),
                    );
                }
                if let Some(remember) = &auth.remember_opt {
                    //TODO: what should submit do?
                    //TODO: button for showing password
                    controls.push(
                        widget::checkbox(fl!("remember-password"), *remember)
                            .on_toggle(move |value| {
                                Message::DialogUpdate(DialogPage::NetworkAuth {
                                    mounter_key: *mounter_key,
                                    uri: uri.clone(),
                                    auth: MounterAuth {
                                        remember_opt: Some(value),
                                        ..auth.clone()
                                    },
                                    auth_tx: auth_tx.clone(),
                                })
                            })
                            .into(),
                    );
                }

                let mut parts = auth.message.splitn(2, '\n');
                let title = parts.next().unwrap_or_default();
                let body = parts.next().unwrap_or_default();
                widget::dialog()
                    .title(title)
                    .body(body)
                    .control(widget::column::with_children(controls).spacing(space_s))
                    .primary_action(
                        widget::button::suggested(fl!("connect")).on_press(Message::DialogComplete),
                    )
                    .secondary_action(
                        widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                    )
                    .tertiary_action(widget::button::text(fl!("connect-anonymously")).on_press(
                        Message::DialogUpdateComplete(DialogPage::NetworkAuth {
                            mounter_key: *mounter_key,
                            uri: uri.clone(),
                            auth: MounterAuth {
                                anonymous_opt: Some(true),
                                ..auth.clone()
                            },
                            auth_tx: auth_tx.clone(),
                        }),
                    ))
            }
            DialogPage::NetworkError {
                mounter_key: _,
                uri: _,
                error,
            } => widget::dialog()
                .title(fl!("network-drive-error"))
                .body(error)
                .icon(widget::icon::from_name("dialog-error").size(64))
                .primary_action(
                    widget::button::standard(fl!("try-again")).on_press(Message::DialogComplete),
                )
                .secondary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                ),
            DialogPage::NewItem { parent, name, dir } => {
                let mut dialog = widget::dialog().title(if *dir {
                    fl!("create-new-folder")
                } else {
                    fl!("create-new-file")
                });

                let complete_maybe = if name.is_empty() {
                    None
                } else if name == "." || name == ".." {
                    dialog = dialog.tertiary_action(widget::text::body(fl!(
                        "name-invalid",
                        filename = name.as_str()
                    )));
                    None
                } else if name.contains('/') {
                    dialog = dialog.tertiary_action(widget::text::body(fl!("name-no-slashes")));
                    None
                } else {
                    let path = parent.join(name);
                    if path.exists() {
                        if path.is_dir() {
                            dialog = dialog
                                .tertiary_action(widget::text::body(fl!("folder-already-exists")));
                        } else {
                            dialog = dialog
                                .tertiary_action(widget::text::body(fl!("file-already-exists")));
                        }
                        None
                    } else {
                        if name.starts_with('.') {
                            dialog = dialog.tertiary_action(widget::text::body(fl!("name-hidden")));
                        }
                        Some(Message::DialogComplete)
                    }
                };

                dialog
                    .primary_action(
                        widget::button::suggested(fl!("save"))
                            .on_press_maybe(complete_maybe.clone()),
                    )
                    .secondary_action(
                        widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                    )
                    .control(
                        widget::column::with_children(vec![
                            widget::text::body(if *dir {
                                fl!("folder-name")
                            } else {
                                fl!("file-name")
                            })
                            .into(),
                            widget::text_input("", name.as_str())
                                .id(self.dialog_text_input.clone())
                                .on_input(move |name| {
                                    Message::DialogUpdate(DialogPage::NewItem {
                                        parent: parent.clone(),
                                        name,
                                        dir: *dir,
                                    })
                                })
                                .on_submit_maybe(complete_maybe)
                                .into(),
                        ])
                        .spacing(space_xxs),
                    )
            }
            DialogPage::NewTag { tag } => {
                let mut dialog = widget::dialog().title(fl!("create-new-tag"));

                let complete_maybe = if tag.is_empty() {
                    None
                } else if tag.starts_with(".") {
                    dialog = dialog.tertiary_action(widget::text::body(fl!(
                        "name-invalid",
                        filename = tag.as_str()
                    )));
                    None
                } else if tag.contains('/') || tag.contains('.') {
                    dialog = dialog.tertiary_action(widget::text::body(fl!("name-no-slashes")));
                    None
                } else {
                    Some(Message::DialogComplete)
                };

                dialog
                    .primary_action(
                        widget::button::suggested(fl!("save"))
                            .on_press_maybe(complete_maybe.clone()),
                    )
                    .secondary_action(
                        widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                    )
                    .control(
                        widget::column::with_children(vec![
                            widget::text::body(fl!("tag-name")).into(),
                            widget::text_input("", tag.as_str())
                                .id(self.dialog_text_input.clone())
                                .on_input(move |tag| {
                                    Message::DialogUpdate(DialogPage::NewTag {
                                        tag: tag.to_string(),
                                    })
                                })
                                .on_submit_maybe(complete_maybe)
                                .into(),
                        ])
                        .spacing(space_xxs),
                    )
            }
            DialogPage::OpenWith {
                path,
                apps,
                selected,
                ..
            } => {
                let mut column = widget::list_column();
                for (i, app) in apps.iter().enumerate() {
                    column = column.add(
                        widget::button::custom(
                            widget::row::with_children(vec![
                                widget::icon(app.icon.clone()).size(32).into(),
                                if app.is_default {
                                    widget::text::body(fl!(
                                        "default-app",
                                        name = Some(app.name.as_str())
                                    ))
                                    .into()
                                } else {
                                    widget::text::body(app.name.to_string()).into()
                                },
                                widget::horizontal_space().into(),
                                if *selected == i {
                                    widget::icon::from_name("checkbox-checked-symbolic")
                                        .size(16)
                                        .into()
                                } else {
                                    widget::Space::with_width(Length::Fixed(16.0)).into()
                                },
                            ])
                            .spacing(space_s)
                            .height(Length::Fixed(32.0))
                            .align_y(Alignment::Center),
                        )
                        .width(Length::Fill)
                        .class(theme::Button::MenuItem)
                        .on_press(Message::Open(
                            Some(entity),
                            crate::parsers::osstr_to_string(path.clone().into_os_string()),
                        )),
                    );
                }

                let dialog = widget::dialog()
                    .title("Open-with".to_string())
                    .primary_action(
                        widget::button::suggested(fl!("open")).on_press(Message::DialogComplete),
                    )
                    .secondary_action(
                        widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                    )
                    .control(column);

                dialog
            }
            DialogPage::RenameItem {
                from,
                parent,
                name,
                dir,
            } => {
                //TODO: combine logic with NewItem
                let mut dialog = widget::dialog().title(if *dir {
                    fl!("rename-folder")
                } else {
                    fl!("rename-file")
                });

                let complete_maybe = if name.is_empty() {
                    None
                } else if name == "." || name == ".." {
                    dialog = dialog.tertiary_action(widget::text::body(fl!(
                        "name-invalid",
                        filename = name.as_str()
                    )));
                    None
                } else if name.contains('/') {
                    dialog = dialog.tertiary_action(widget::text::body(fl!("name-no-slashes")));
                    None
                } else {
                    let path = parent.join(name);
                    if path.exists() {
                        if path.is_dir() {
                            dialog = dialog
                                .tertiary_action(widget::text::body(fl!("folder-already-exists")));
                        } else {
                            dialog = dialog
                                .tertiary_action(widget::text::body(fl!("file-already-exists")));
                        }
                        None
                    } else {
                        if name.starts_with('.') {
                            dialog = dialog.tertiary_action(widget::text::body(fl!("name-hidden")));
                        }
                        Some(Message::DialogComplete)
                    }
                };

                dialog
                    .primary_action(
                        widget::button::suggested(fl!("rename"))
                            .on_press_maybe(complete_maybe.clone()),
                    )
                    .secondary_action(
                        widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                    )
                    .control(
                        widget::column::with_children(vec![
                            widget::text::body(if *dir {
                                fl!("folder-name")
                            } else {
                                fl!("file-name")
                            })
                            .into(),
                            widget::text_input("", name.as_str())
                                .id(self.dialog_text_input.clone())
                                .on_input(move |name| {
                                    Message::DialogUpdate(DialogPage::RenameItem {
                                        from: from.clone(),
                                        parent: parent.clone(),
                                        name,
                                        dir: *dir,
                                    })
                                })
                                .on_submit_maybe(complete_maybe)
                                .into(),
                        ])
                        .spacing(space_xxs),
                    )
            }
            DialogPage::Replace {
                from,
                to,
                multiple,
                apply_to_all,
                tx,
            } => {
                let dialog = widget::dialog()
                    .title(fl!("replace-title", filename = to.name.as_str()))
                    .body(fl!("replace-warning-operation"))
                    .control(to.replace_view(fl!("original-file"), IconSizes::default()))
                    .control(from.replace_view(fl!("replace-with"), IconSizes::default()))
                    .primary_action(widget::button::suggested(fl!("replace")).on_press(
                        Message::ReplaceResult(ReplaceResult::Replace(*apply_to_all)),
                    ));
                if *multiple {
                    dialog
                        .control(
                            widget::checkbox(fl!("apply-to-all"), *apply_to_all).on_toggle(
                                |apply_to_all| {
                                    Message::DialogUpdate(DialogPage::Replace {
                                        from: from.clone(),
                                        to: to.clone(),
                                        multiple: *multiple,
                                        apply_to_all,
                                        tx: tx.clone(),
                                    })
                                },
                            ),
                        )
                        .secondary_action(
                            widget::button::standard(fl!("skip")).on_press(Message::ReplaceResult(
                                ReplaceResult::Skip(*apply_to_all),
                            )),
                        )
                        .tertiary_action(
                            widget::button::text(fl!("cancel"))
                                .on_press(Message::ReplaceResult(ReplaceResult::Cancel)),
                        )
                } else {
                    dialog
                        .secondary_action(
                            widget::button::standard(fl!("cancel"))
                                .on_press(Message::ReplaceResult(ReplaceResult::Cancel)),
                        )
                        .tertiary_action(
                            widget::button::text(fl!("keep-both"))
                                .on_press(Message::ReplaceResult(ReplaceResult::KeepBoth)),
                        )
                }
            }
        };

        Some(dialog.into())
    }

    fn footer(&self) -> Option<Element<'_, Message>> {
        if self.progress_operations.is_empty() {
            return None;
        }

        let cosmic_theme::Spacing {
            space_xs, space_s, ..
        } = theme::active().cosmic().spacing;

        let mut title = String::new();
        let mut total_progress = 0.0;
        let mut count = 0;
        let mut all_paused = true;
        for (_id, (op, controller)) in self.pending_operations.iter() {
            if !controller.is_paused() {
                all_paused = false;
            }
            if op.show_progress_notification() {
                let progress = controller.progress();
                if title.is_empty() {
                    title = op.pending_text(progress, controller.state());
                }
                total_progress += progress;
                count += 1;
            }
        }
        let running = count;
        // Adjust the progress bar so it does not jump around when operations finish
        for id in self.progress_operations.iter() {
            if self.complete_operations.contains_key(&id) {
                total_progress += 1.0;
                count += 1;
            }
        }
        let finished = count - running;
        total_progress /= count as f32;
        if running > 1 {
            if finished > 0 {
                title = fl!(
                    "operations-running-finished",
                    running = running,
                    finished = finished,
                    percent = (total_progress as i32)
                );
            } else {
                title = fl!(
                    "operations-running",
                    running = running,
                    percent = (total_progress as i32)
                );
            }
        }

        //TODO: get height from theme?
        let progress_bar_height = Length::Fixed(4.0);
        let progress_bar =
            widget::progress_bar(0.0..=1.0, total_progress).height(progress_bar_height);

        let container = widget::layer_container(widget::column::with_children(vec![
            widget::row::with_children(vec![
                progress_bar.into(),
                if all_paused {
                    widget::tooltip(
                        widget::button::icon(widget::icon::from_name(
                            "media-playback-start-symbolic",
                        ))
                        .on_press(Message::PendingPauseAll(false))
                        .padding(8),
                        widget::text::body("Resume".to_string()),
                        widget::tooltip::Position::Top,
                    )
                    .into()
                } else {
                    widget::tooltip(
                        widget::button::icon(widget::icon::from_name(
                            "media-playback-pause-symbolic",
                        ))
                        .on_press(Message::PendingPauseAll(true))
                        .padding(8),
                        widget::text::body("Pause".to_string()),
                        widget::tooltip::Position::Top,
                    )
                    .into()
                },
                widget::tooltip(
                    widget::button::icon(widget::icon::from_name("window-close-symbolic"))
                        .on_press(Message::PendingCancelAll)
                        .padding(8),
                    widget::text::body(fl!("cancel")),
                    widget::tooltip::Position::Top,
                )
                .into(),
            ])
            .align_y(Alignment::Center)
            .into(),
            widget::text::body(title).into(),
            widget::Space::with_height(space_s).into(),
            widget::row::with_children(vec![
                widget::button::link("details".to_string())
                    .on_press(Message::ToggleContextPage(ContextPage::EditHistory))
                    .padding(0)
                    .trailing_icon(true)
                    .into(),
                widget::horizontal_space().into(),
                widget::button::standard("dismiss".to_string())
                    .on_press(Message::PendingDismiss)
                    .into(),
            ])
            .align_y(Alignment::Center)
            .into(),
        ]))
        .padding([8, space_xs])
        .layer(cosmic_theme::Layer::Primary);

        Some(container.into())
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![menu::menu_bar(
            self.tab_model.active_data::<Tab>(),
            &self.config,
            &self.key_binds,
        )
        .into()]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        let mut elements = Vec::with_capacity(2);

        if let Some(term) = self.search_get() {
            if self.core.is_condensed() {
                elements.push(
                    //TODO: selected state is not appearing different
                    widget::button::icon(widget::icon::from_name("system-search-symbolic"))
                        .on_press(Message::SearchClear)
                        .padding(8)
                        .selected(true)
                        .into(),
                );
            } else {
                elements.push(
                    widget::text_input::search_input("", term)
                        .width(Length::Fixed(240.0))
                        .id(self.search_id.clone())
                        .on_clear(Message::SearchClear)
                        .on_input(Message::SearchInput)
                        .into(),
                );
            }
        } else {
            elements.push(
                widget::button::icon(widget::icon::from_name("system-search-symbolic"))
                    .on_press(Message::SearchActivate)
                    .padding(8)
                    .into(),
            );
        }

        elements
    }

    /// Creates a view after each update.
    fn view(&self) -> Element<'_, Self::Message> {
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let mut tab_column = widget::column::with_capacity(4);

        if self.active_view == Mode::Image {
            return self.view_image_view();
        } else if self.active_view == Mode::Video {
            return self.view_video_view();
        } else if self.active_view == Mode::Audio {
            return self.view_audio_view();
        } else {
            if self.core.is_condensed() {
                if let Some(term) = self.search_get() {
                    tab_column = tab_column.push(
                        widget::container(
                            widget::text_input::search_input("", term)
                                .width(Length::Fill)
                                .id(self.search_id.clone())
                                .on_clear(Message::SearchClear)
                                .on_input(Message::SearchInput),
                        )
                        .padding(space_xxs),
                    )
                }
            }

            if self.tab_model.iter().count() > 1 {
                tab_column = tab_column.push(
                    widget::container(
                        widget::tab_bar::horizontal(&self.tab_model)
                            .button_height(32)
                            .button_spacing(space_xxs)
                            .on_activate(Message::TabActivate)
                            .on_close(|entity| Message::TabClose(Some(entity)))
                            .on_dnd_enter(|entity, _| Message::DndEnterTab(entity))
                            .on_dnd_leave(|_| Message::DndExitTab)
                            .on_dnd_drop(|entity, data, action| {
                                Message::DndDropTab(entity, data, action)
                            })
                            .drag_id(self.tab_drag_id),
                    )
                    .class(style::Container::Background)
                    .width(Length::Fill)
                    .padding([0, space_s]),
                );
            }

            let entity = self.tab_model.active();
            match self.tab_model.data::<Tab>(entity) {
                Some(tab) => {
                    let tab_view = tab
                        .view(&self.key_binds)
                        .map(move |message| Message::TabMessage(Some(entity), message));
                    tab_column = tab_column.push(tab_view);
                }
                None => {
                    //TODO
                }
            }

            // The toaster is added on top of an empty element to ensure that it does not override context menus
            tab_column = tab_column.push(widget::toaster(&self.toasts, widget::horizontal_space()));

            let content: Element<_> = tab_column.into();

            // Uncomment to debug layout:
            //content.explain(cosmic::iced::Color::WHITE)
            content
        }
    }

    fn view_window(&self, id: WindowId) -> Element<'_, Self::Message> {
        let content = match self.windows.get(&id) {
            Some(WindowKind::Desktop(entity)) => {
                let mut tab_column = widget::column::with_capacity(3);

                let tab_view = match self.tab_model.data::<Tab>(*entity) {
                    Some(tab) => tab
                        .view(&self.key_binds)
                        .map(move |message| Message::TabMessage(Some(*entity), message)),
                    None => widget::vertical_space().into(),
                };

                let mut popover = widget::popover(tab_view);
                if let Some(dialog) = self.dialog() {
                    popover = popover.popup(dialog);
                }
                tab_column = tab_column.push(popover);

                // The toaster is added on top of an empty element to ensure that it does not override context menus
                tab_column =
                    tab_column.push(widget::toaster(&self.toasts, widget::horizontal_space()));
                return if let Some(margin) = self.margin.get(&id) {
                    if margin.0 >= 0. || margin.2 >= 0. {
                        tab_column = widget::column::with_children(vec![
                            vertical_space().height(margin.0 as f32).into(),
                            tab_column.into(),
                            vertical_space().height(margin.2 as f32).into(),
                        ])
                    }
                    if margin.1 >= 0. || margin.3 >= 0. {
                        Element::from(widget::row::with_children(vec![
                            horizontal_space().width(margin.1 as f32).into(),
                            tab_column.into(),
                            horizontal_space().width(margin.3 as f32).into(),
                        ]))
                    } else {
                        tab_column.into()
                    }
                } else {
                    tab_column.into()
                };
            }
            Some(WindowKind::Preview(entity_opt, kind)) => self.preview(&entity_opt, &kind, false),
            None => {
                //TODO: distinct views per monitor in desktop mode
                return self.view_main().map(|message| match message {
                    app::Message::App(app) => app,
                    app::Message::Cosmic(cosmic) => Message::Cosmic(cosmic),
                    app::Message::None => Message::None,
                });
            }
        };

        widget::container(widget::scrollable(content))
            .width(Length::Fill)
            .height(Length::Fill)
            .class(theme::Container::WindowBackground)
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        struct ThemeSubscription;
        struct WatcherSubscription;
        struct TrashWatcherSubscription;

        let mut subscriptions = vec![
            event::listen_with(|event, status, _| match event {
                Event::Keyboard(KeyEvent::KeyPressed { key, modifiers, .. }) => match status {
                    event::Status::Ignored => Some(Message::Key(modifiers, key)),
                    event::Status::Captured => None,
                },
                Event::Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
                    Some(Message::Modifiers(modifiers))
                }
                Event::Window(WindowEvent::CloseRequested) => Some(Message::WindowClose),
                Event::Window(WindowEvent::Opened { position: _, size }) => {
                    Some(Message::Size(size))
                }
                Event::Window(WindowEvent::Resized(s)) => Some(Message::Size(s)),
                #[cfg(feature = "wayland")]
                Event::PlatformSpecific(event::PlatformSpecific::Wayland(wayland_event)) => {
                    match wayland_event {
                        WaylandEvent::Output(output_event, output) => {
                            Some(Message::OutputEvent(output_event, output))
                        }
                        _ => None,
                    }
                }
                _ => None,
            }),
            Config::subscription().map(|update| {
                if !update.errors.is_empty() {
                    log::info!(
                        "errors loading config {:?}: {:?}",
                        update.keys,
                        update.errors
                    );
                }
                Message::Config(update.config)
            }),
            cosmic_config::config_subscription::<_, cosmic_theme::ThemeMode>(
                TypeId::of::<ThemeSubscription>(),
                cosmic_theme::THEME_MODE_ID.into(),
                cosmic_theme::ThemeMode::version(),
            )
            .map(|update| {
                if !update.errors.is_empty() {
                    log::info!(
                        "errors loading theme mode {:?}: {:?}",
                        update.keys,
                        update.errors
                    );
                }
                Message::SystemThemeModeChange(update.config)
            }),
            Subscription::run_with_id(
                TypeId::of::<WatcherSubscription>(),
                stream::channel(100, |mut output| async move {
                    let watcher_res = {
                        let mut output = output.clone();
                        new_debouncer(
                            time::Duration::from_millis(250),
                            Some(time::Duration::from_millis(250)),
                            move |events_res: notify_debouncer_full::DebounceEventResult| {
                                match events_res {
                                    Ok(mut events) => {
                                        log::debug!("{:?}", events);

                                        events.retain(|event| {
                                            match &event.kind {
                                                notify::EventKind::Access(_) => {
                                                    // Data not mutated
                                                    false
                                                }
                                                notify::EventKind::Modify(
                                                    notify::event::ModifyKind::Metadata(e),
                                                ) if (*e != notify::event::MetadataKind::Any
                                                    && *e
                                                        != notify::event::MetadataKind::WriteTime) =>
                                                {
                                                    // Data not mutated nor modify time changed
                                                    false
                                                }
                                                _ => true
                                            }
                                        });

                                        if !events.is_empty() {
                                            match futures::executor::block_on(async {
                                                output.send(Message::NotifyEvents(events)).await
                                            }) {
                                                Ok(()) => {}
                                                Err(err) => {
                                                    log::warn!(
                                                        "failed to send notify events: {:?}",
                                                        err
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        log::warn!("failed to watch files: {:?}", err);
                                    }
                                }
                            },
                        )
                    };

                    match watcher_res {
                        Ok(watcher) => {
                            match output
                                .send(Message::NotifyWatcher(WatcherWrapper {
                                    watcher_opt: Some(watcher),
                                }))
                                .await
                            {
                                Ok(()) => {}
                                Err(err) => {
                                    log::warn!("failed to send notify watcher: {:?}", err);
                                }
                            }
                        }
                        Err(err) => {
                            log::warn!("failed to create file watcher: {:?}", err);
                        }
                    }

                    std::future::pending().await
                }),
            ),
            Subscription::run_with_id(
                TypeId::of::<TrashWatcherSubscription>(),
                stream::channel(25, |mut output| async move {
                    let watcher_res = new_debouncer(
                        time::Duration::from_millis(250),
                        Some(time::Duration::from_millis(250)),
                        move |event_res: notify_debouncer_full::DebounceEventResult| match event_res
                        {
                            Ok(mut events) => {
                                events.retain(|event| {
                                    matches!(
                                        event.kind,
                                        notify::EventKind::Create(_) | notify::EventKind::Remove(_)
                                    )
                                });

                                if !events.is_empty() {
                                    if let Err(e) = futures::executor::block_on(async {
                                        output.send(Message::RescanTrash).await
                                    }) {
                                        log::warn!("trash needs to be rescanned but sending message failed: {e:?}");
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("failed to watch trash bin for changes: {e:?}")
                            }
                        },
                    );

                    // TODO: Trash watching support for Windows, macOS, and other OSes
                    #[cfg(all(
                        unix,
                        not(target_os = "macos"),
                        not(target_os = "ios"),
                        not(target_os = "android")
                    ))]
                    match (watcher_res, trash::os_limited::trash_folders()) {
                        (Ok(mut watcher), Ok(trash_bins)) => {
                            for path in trash_bins {
                                if let Err(e) = watcher
                                    .watcher()
                                    .watch(&path, notify::RecursiveMode::Recursive)
                                {
                                    log::warn!(
                                        "failed to add trash bin `{}` to watcher: {e:?}",
                                        path.display()
                                    );
                                }
                            }

                            // Don't drop the watcher
                            std::future::pending().await
                        }
                        (Err(e), _) => {
                            log::warn!("failed to create new watcher for trash bin: {e:?}")
                        }
                        (_, Err(e)) => {
                            log::warn!("could not find any valid trash bins to watch: {e:?}")
                        }
                    }

                    std::future::pending().await
                }),
            ),
        ];

        for (key, mounter) in MOUNTERS.iter() {
            subscriptions.push(
                mounter.subscription().with(*key).map(
                    |(key, mounter_message)| match mounter_message {
                        MounterMessage::Items(items) => Message::MounterItems(key, items),
                        MounterMessage::MountResult(item, res) => {
                            Message::MountResult(key, item, res)
                        }
                        MounterMessage::NetworkAuth(uri, auth, auth_tx) => {
                            Message::NetworkAuth(key, uri, auth, auth_tx)
                        }
                        MounterMessage::NetworkResult(uri, res) => {
                            Message::NetworkResult(key, uri, res)
                        }
                    },
                ),
            );
        }

        if !self.pending_operations.is_empty() {
            //TODO: inhibit suspend/shutdown?

            if self.window_id_opt.is_some() {
                // Refresh progress when window is open and operations are in progress
                subscriptions.push(window::frames().map(|_| Message::None));
            } else {
                // Handle notification when window is closed and operations are in progress
                #[cfg(feature = "notify")]
                {
                    struct NotificationSubscription;
                    subscriptions.push(Subscription::run_with_id(
                        TypeId::of::<NotificationSubscription>(),
                        stream::channel(1, move |msg_tx| async move {
                            let msg_tx = Arc::new(tokio::sync::Mutex::new(msg_tx));
                            tokio::task::spawn_blocking(move || {
                                match notify_rust::Notification::new()
                                    .summary(&fl!("notification-in-progress"))
                                    .timeout(notify_rust::Timeout::Never)
                                    .show()
                                {
                                    Ok(notification) => {
                                        let _ = futures::executor::block_on(async {
                                            msg_tx
                                                .lock()
                                                .await
                                                .send(Message::Notification(Arc::new(Mutex::new(
                                                    notification,
                                                ))))
                                                .await
                                        });
                                    }
                                    Err(err) => {
                                        log::warn!("failed to create notification: {}", err);
                                    }
                                }
                            })
                            .await
                            .unwrap();

                            std::future::pending().await
                        }),
                    ));
                }
            }
        }

        for (id, (pending_operation, controller)) in self.pending_operations.iter() {
            //TODO: use recipe?
            let id = *id;
            let pending_operation = pending_operation.clone();
            let controller = controller.clone();
            subscriptions.push(Subscription::run_with_id(
                id,
                stream::channel(16, move |msg_tx| async move {
                    let msg_tx = Arc::new(tokio::sync::Mutex::new(msg_tx));
                    match pending_operation.perform(&msg_tx, controller).await {
                        Ok(result_paths) => {
                            let _ = msg_tx
                                .lock()
                                .await
                                .send(Message::PendingComplete(id, result_paths))
                                .await;
                        }
                        Err(err) => {
                            let _ = msg_tx
                                .lock()
                                .await
                                .send(Message::PendingError(id, err.to_string()))
                                .await;
                        }
                    }

                    std::future::pending().await
                }),
            ));
        }

        let mut selected_preview = None;
        if self.core.window.show_context {
            if let ContextPage::Preview(entity_opt, PreviewKind::Selected) = self.context_page {
                selected_preview = Some(entity_opt.unwrap_or_else(|| self.tab_model.active()));
            }
        }
        for entity in self.tab_model.iter() {
            if let Some(tab) = self.tab_model.data::<Tab>(entity) {
                subscriptions.push(
                    tab.subscription(selected_preview == Some(entity))
                        .with(entity)
                        .map(|(entity, tab_msg)| Message::TabMessage(Some(entity), tab_msg)),
                );
            }
        }
        subscriptions.push(
            cosmic::iced::time::every(cosmic::iced::time::Duration::from_millis(1000)).map(
                |_time| -> Message {
                    return Message::NewFrame;
                },
            ),
        );

        Subscription::batch(subscriptions)
    }
}

// Utilities to build a temporary file hierarchy for tests.
//
// Ideally, tests would use the cap-std crate which limits path traversal.
#[cfg(test)]
pub(crate) mod test_utils {
    use std::{
        cmp::Ordering,
        fs::File,
        io::{self, Write},
        iter,
        path::Path,
    };

    use log::{debug, trace};
    use tempfile::{tempdir, TempDir};

    use crate::{
        config::{IconSizes, MediaTabConfig as TabConfig},
        //mounter::MounterMap,
        tab::Item,
    };

    use super::*;

    // Default number of files, directories, and nested directories for test file system
    pub const NUM_FILES: usize = 2;
    pub const NUM_HIDDEN: usize = 1;
    pub const NUM_DIRS: usize = 2;
    pub const NUM_NESTED: usize = 1;
    pub const NAME_LEN: usize = 5;

    /// Add `n` temporary files in `dir`
    ///
    /// Each file is assigned a numeric name from [0, n) with a prefix.
    pub fn file_flat_hier<D: AsRef<Path>>(dir: D, n: usize, prefix: &str) -> io::Result<Vec<File>> {
        let dir = dir.as_ref();
        (0..n)
            .map(|i| -> io::Result<File> {
                let name = format!("{prefix}{i}");
                let path = dir.join(&name);

                let mut file = File::create(path)?;
                file.write_all(name.as_bytes())?;

                Ok(file)
            })
            .collect()
    }

    // Random alphanumeric String of length `len`
    fn rand_string(len: usize) -> String {
        (0..len).map(|_| fastrand::alphanumeric()).collect()
    }

    /// Create a small, temporary file hierarchy.
    ///
    /// # Arguments
    ///
    /// * `files` - Number of files to create in temp directories
    /// * `hidden` - Number of hidden files to create
    /// * `dirs` - Number of directories to create
    /// * `nested` - Number of nested directories to create in new dirs
    /// * `name_len` - Length of randomized directory names
    pub fn simple_fs(
        files: usize,
        hidden: usize,
        dirs: usize,
        nested: usize,
        name_len: usize,
    ) -> io::Result<TempDir> {
        // Files created inside of a TempDir are deleted with the directory
        // TempDir won't leak resources as long as the destructor runs
        let root = tempdir()?;
        debug!("Root temp directory: {}", root.as_ref().display());
        trace!("Creating {files} files and {hidden} hidden files in {dirs} temp dirs with {nested} nested temp dirs");

        // All paths for directories and nested directories
        let paths = (0..dirs).flat_map(|_| {
            let root = root.as_ref();
            let current = rand_string(name_len);

            iter::once(root.join(&current)).chain(
                (0..nested).map(move |_| root.join(format!("{current}/{}", rand_string(name_len)))),
            )
        });

        // Create directories from `paths` and add a few files
        for path in paths {
            fs::create_dir_all(&path)?;

            // Normal files
            file_flat_hier(&path, files, "")?;
            // Hidden files
            file_flat_hier(&path, hidden, ".")?;

            for entry in path.read_dir()? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    trace!("Created file: {}", entry.path().display());
                }
            }
        }

        Ok(root)
    }

    /// Empty file hierarchy
    pub fn empty_fs() -> io::Result<TempDir> {
        tempdir()
    }

    /// Sort files.
    ///
    /// Directories are placed before files.
    /// Files are lexically sorted.
    /// This is more or less copied right from the [Tab] code
    pub fn sort_files(a: &Path, b: &Path) -> Ordering {
        match (a.is_dir(), b.is_dir()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => LANGUAGE_SORTER.compare(
                a.file_name()
                    .expect("temp entries should have names")
                    .to_str()
                    .expect("temp entries should be valid UTF-8"),
                b.file_name()
                    .expect("temp entries should have names")
                    .to_str()
                    .expect("temp entries should be valid UTF-8"),
            ),
        }
    }

    /// Read directory entries from `path` and sort.
    pub fn read_dir_sorted(path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries: Vec<_> = path
            .read_dir()?
            .map(|maybe_entry| maybe_entry.map(|entry| entry.path()))
            .collect::<io::Result<_>>()?;
        entries.sort_by(|a, b| sort_files(a, b));

        Ok(entries)
    }

    /// Filter `path` for directories
    pub fn filter_dirs(path: &Path) -> io::Result<impl Iterator<Item = PathBuf>> {
        Ok(path.read_dir()?.filter_map(|entry| {
            entry.ok().and_then(|entry| {
                let path = entry.path();
                if path.is_dir() {
                    Some(path)
                } else {
                    None
                }
            })
        }))
    }

    // Filter `path` for files
    pub fn filter_files(path: &Path) -> io::Result<impl Iterator<Item = PathBuf>> {
        Ok(path.read_dir()?.filter_map(|entry| {
            entry.ok().and_then(|entry| {
                let path = entry.path();
                path.is_file().then_some(path)
            })
        }))
    }

    /// Boiler plate for Tab tests
    pub fn tab_click_new(
        files: usize,
        hidden: usize,
        dirs: usize,
        nested: usize,
        name_len: usize,
        sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    ) -> io::Result<(TempDir, Tab)> {
        let fs = simple_fs(files, hidden, dirs, nested, name_len)?;
        let path = fs.path();

        // New tab with items
        let location = Location::Path(path.to_owned());
        let (parent_item_opt, items) = location.scan(sql_connection.clone(), IconSizes::default());
        let mut tab = Tab::new(location, TabConfig::default());
        tab.parent_item_opt = parent_item_opt;
        tab.set_items(items);

        // Ensure correct number of directories as a sanity check
        let items = tab.items_opt().expect("tab should be populated with Items");
        assert_eq!(NUM_DIRS, items.len());

        Ok((fs, tab))
    }

    /// Equality for [Path] and [Item].
    pub fn eq_path_item(path: &Path, item: &Item) -> bool {
        let name = path
            .file_name()
            .expect("temp entries should have names")
            .to_str()
            .expect("temp entries should be valid UTF-8");
        let is_dir = path.is_dir();

        // NOTE: I don't want to change `tab::hidden_attribute` to `pub(crate)` for
        // tests without asking
        #[cfg(not(target_os = "windows"))]
        let is_hidden = name.starts_with('.');

        #[cfg(target_os = "windows")]
        let is_hidden = {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_HIDDEN: u32 = 2;
            let metadata = path.metadata().expect("fetching file metadata");
            metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN == FILE_ATTRIBUTE_HIDDEN
        };

        name == item.name
            && is_dir == item.metadata.is_dir()
            && path == item.path_opt().expect("item should have path")
            && is_hidden == item.hidden
    }

    /// Asserts `tab`'s location changed to `path`
    pub fn assert_eq_tab_path(tab: &Tab, path: &Path) {
        // Paths should be the same
        let Some(tab_path) = tab.location.path_opt() else {
            panic!("Expected tab's location to be a path");
        };

        assert_eq!(
            path,
            tab_path,
            "Tab's path is {} instead of being updated to {}",
            tab_path.display(),
            path.display()
        );
    }

    /// Assert that tab's items are equal to a path's entries.
    pub fn _assert_eq_tab_path_contents(tab: &Tab, path: &Path) {
        let Some(tab_path) = tab.location.path_opt() else {
            panic!("Expected tab's location to be a path");
        };

        // Tab items are sorted so paths from read_dir must be too
        let entries = read_dir_sorted(path).expect("should be able to read paths from temp dir");

        // Check lengths.
        // `items_opt` is optional and the directory at `path` may have zero entries
        // Therefore, this doesn't panic if `items_opt` is None
        let items_len = tab.items_opt().map(|items| items.len()).unwrap_or_default();
        assert_eq!(entries.len(), items_len);

        let empty = Vec::new();
        assert!(
            entries
                .into_iter()
                .zip(tab.items_opt().clone().unwrap_or(&empty))
                .all(|(a, b)| eq_path_item(&a, &b)),
            "Path ({}) and Tab path ({}) don't have equal contents",
            path.display(),
            tab_path.display()
        );
    }
}
