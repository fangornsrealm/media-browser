//! Message handlers for the dialog stack and sidebar configuration.
//!
//! Split out of `app.rs` (Phase 3 — modularization). The original
//! `Message::AddToSidebar`, `Message::AddTagToSidebar`,
//! `Message::AddTagToContents`, `Message::DialogCancel`,
//! `Message::DialogComplete`, `Message::DialogPush`,
//! `Message::DialogUpdate`, and `Message::DialogUpdateComplete` arms of
//! `App::update` are now individual methods on `App`, dispatched from
//! `mod.rs` with a single forwarding line each.
//!
//! Note: `add_to_sidebar` and `dialog_complete` use the `config_set!`
//! macro, which must be defined at the top level of `app/mod.rs` (moved
//! out of the former inline definition inside `App::update`) so this
//! sibling module can access it.

use cosmic::app::{message, Task};
use cosmic::widget::{self, segmented_button::Entity};
use cosmic::Application;

use super::{App, DialogPage, Message};
use crate::clipboard::ClipboardPaste;
use crate::config::MediaFavorite as Favorite;
use crate::mounter::MOUNTERS;
use crate::operation::Operation;
use crate::spawn_detached::spawn_detached;
use crate::sql::Tag;

impl App {
    /// Handle `Message::AddToSidebar`.
    ///
    /// Adds the selected paths in the given (or active) tab to the
    /// favorites list in the sidebar, persisting the config change.
    pub fn add_to_sidebar(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        let mut favorites = self.config.favorites.clone();
        for path in self.selected_paths(entity_opt) {
            let favorite = Favorite::from_path(path);
            if !favorites.iter().any(|f| f == &favorite) {
                favorites.push(favorite);
            }
        }
        config_set!(self, favorites, favorites);
        self.update_config()
    }

    /// Handle `Message::AddTagToSidebar`.
    ///
    /// Pushes a `DialogPage::NewTag` for creating a new tag in the
    /// sidebar and focuses the dialog's text input.
    pub fn add_tag_to_sidebar(&mut self, _entity_opt: Option<Entity>) -> Task<Message> {
        self.dialog_pages.push_back(DialogPage::NewTag {
            tag: "unnamed".to_string(),
        });
        widget::text_input::focus(self.dialog_text_input.clone())
    }

    /// Handle `Message::AddTagToContents`.
    ///
    /// Inserts the given tag as a media tag for every file in the
    /// clipboard contents, after looking up each file's metadata row.
    pub fn add_tag_to_contents(&mut self, to: Tag, contents: ClipboardPaste) -> Task<Message> {
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
        Task::none()
    }

    /// Handle `Message::DialogCancel`.
    ///
    /// Dismisses the front dialog without performing its action.
    pub fn dialog_cancel(&mut self) -> Task<Message> {
        self.dialog_pages.pop_front();
        Task::none()
    }

    /// Handle `Message::DialogComplete`.
    ///
    /// Pops the front dialog and performs its associated action:
    ///
    /// | Variant           | Action                                                        |
    /// |-------------------|---------------------------------------------------------------|
    /// | `EmptyTrash`      | Queues `Operation::EmptyTrash`                                 |
    /// | `FailedOperation` | Logs a retry TODO                                              |
    /// | `MountError`      | Re-mounts via the mounter                                      |
    /// | `NetworkAuth`     | Sends the auth credentials to the mounter                      |
    /// | `NetworkError`    | Re-submits the network drive connection                        |
    /// | `NewItem`         | Queues `Operation::NewFolder`                                  |
    /// | `NewTag`          | Inserts the tag and adds it to the sidebar, persisting config  |
    /// | `OpenWith`        | Launches the selected app with the path                        |
    /// | `RenameItem`      | Queues `Operation::Rename`                                     |
    /// | `Replace`         | Warns that it should be completed with a replace result        |
    pub fn dialog_complete(&mut self) -> Task<Message> {
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
                        crate::sql::insert_tag(self.sql_connection.clone(), 0, tag.clone()) as u32;
                    let t = crate::sql::Tag {
                        tag_id,
                        tag: tag.clone(),
                    };
                    let mut tags = self.config.tags.clone();
                    //let taglocation = Location::Tag(t);
                    if !tags.iter().any(|f| f == &t) {
                        tags.push(t);
                    }
                    config_set!(self, tags, tags);
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

        Task::none()
    }

    /// Handle `Message::DialogPush`.
    ///
    /// Pushes a new dialog page onto the back of the dialog stack.
    pub fn dialog_push(&mut self, dialog_page: DialogPage) -> Task<Message> {
        self.dialog_pages.push_back(dialog_page);
        Task::none()
    }

    /// Handle `Message::DialogUpdate`.
    ///
    /// Replaces the front dialog page in the stack with the updated one,
    /// leaving the rest of the stack intact.
    pub fn dialog_update(&mut self, dialog_page: DialogPage) -> Task<Message> {
        if !self.dialog_pages.is_empty() {
            self.dialog_pages[0] = dialog_page;
        }
        Task::none()
    }

    /// Handle `Message::DialogUpdateComplete`.
    ///
    /// Applies the update to the front dialog and immediately completes
    /// it, batching both resulting tasks.
    pub fn dialog_update_complete(&mut self, dialog_page: DialogPage) -> Task<Message> {
        Task::batch([
            self.update(Message::DialogUpdate(dialog_page)),
            self.update(Message::DialogComplete),
        ])
    }
}
