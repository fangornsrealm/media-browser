//! Application-level types, enums, and messages.
//!
//! Split out of `app.rs` (Phase 3 — modularization). Re-exported from the
//! parent via `pub use types::*;` so existing `crate::app::` call-sites
//! (e.g. `crate::app::Message`) keep working unchanged.

use crate::clipboard::ClipboardPaste;
use crate::config::{AppTheme, Config, MediaTabConfig as TabConfig};
use crate::mime_app;
use crate::mounter::{MounterAuth, MounterItem, MounterItems, MounterKey};
use crate::operation::{OperationSelection, ReplaceResult};
use crate::tab::{self, HeadingOptions, Location};
use cosmic::app;
use cosmic::cosmic_config;
use cosmic::cosmic_theme;
#[cfg(feature = "wayland")]
use cosmic::iced::event::wayland::OutputEvent;
use cosmic::iced::{
    clipboard::dnd::DndAction,
    keyboard::{Key, Modifiers},
    Size,
};
use cosmic::widget::{self, menu::action::MenuAction, segmented_button, segmented_button::Entity};
use notify_debouncer_full::notify::RecommendedWatcher;
use notify_debouncer_full::{DebouncedEvent, Debouncer, FileIdMap};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use trash::TrashItem;
#[cfg(feature = "wayland")]
use wayland_client::protocol::wl_output::WlOutput;

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
    pub fn message(&self, entity_opt: Option<Entity>) -> Message {
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
            Action::Open => Message::Open(
                entity_opt,
                crate::sql::home_dir_or_root().display().to_string(),
            ),
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

pub struct FavoriteIndex(pub(super) usize);

pub struct MounterData(pub(super) MounterKey, pub(super) MounterItem);

#[derive(Clone, Debug)]
pub enum WindowKind {
    Desktop(Entity),
    Preview(Option<Entity>, PreviewKind),
}

pub struct WatcherWrapper {
    pub(super) watcher_opt: Option<Debouncer<RecommendedWatcher, FileIdMap>>,
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
