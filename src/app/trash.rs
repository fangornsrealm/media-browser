//! Message handlers for trash-related actions: moving items to the trash,
//! restoring them, rescanning the trash view, and the undo-toast flow.
//!
//! Split out of `app.rs` (Phase 3 — modularization). The original
//! `Message::MoveToTrash`, `Message::RestoreFromTrash`, `Message::RescanTrash`,
//! and `Message::UndoTrash` arms of `App::update` are now individual methods
//! on `App`, dispatched from `mod.rs` with a single forwarding line each.

use cosmic::{
    app::Task,
    widget::{self, segmented_button::Entity},
};
use std::{path::PathBuf, sync::Arc};

use super::{App, Message};
use crate::operation::Operation;
use crate::tab::{ItemMetadata, Location, Tab};

impl App {
    /// Handle `Message::MoveToTrash`.
    ///
    /// Queues a delete operation for every selected path in the given (or
    /// active) tab.
    pub fn move_to_trash(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
        let paths = self.selected_paths(entity_opt);
        if !paths.is_empty() {
            self.operation(Operation::Delete { paths });
        }

        Task::none()
    }

    /// Handle `Message::RestoreFromTrash`.
    ///
    /// Collects the trash entries for every selected item in the given (or
    /// active) tab and queues a restore operation for them.
    pub fn restore_from_trash(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
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

        Task::none()
    }

    /// Handle `Message::RescanTrash`.
    ///
    /// Refreshes the trash icon in the nav bar (so it reflects whether the
    /// trash is empty or full) and re-scans the trash location.
    pub fn rescan_trash_message(&mut self) -> Task<Message> {
        // Update trash icon if empty/full
        let maybe_entity = self.nav_model.iter().find(|&entity| {
            self.nav_model
                .data::<Location>(entity)
                .map(|loc| matches!(loc, Location::Trash))
                .unwrap_or_default()
        });
        if let Some(entity) = maybe_entity {
            self.nav_model.icon_set(
                entity,
                widget::icon::icon(crate::tab::trash_icon_symbolic(16)),
            );
        }

        Task::batch([self.rescan_trash()])
    }

    /// Handle `Message::UndoTrash`.
    ///
    /// Dismisses the undo toast, re-scans the trash on a blocking task, and
    /// returns a `Message::UndoTrashStart` for every trashed item whose
    /// original path appears in `recently_trashed`.
    pub fn undo_trash(
        &mut self,
        id: widget::ToastId,
        recently_trashed: Arc<[PathBuf]>,
    ) -> Task<Message> {
        self.toasts.remove(id);

        let mut paths = Vec::with_capacity(recently_trashed.len());
        let icon_sizes = self.config.tab.icon_sizes;
        let sql_connection = self.sql_connection.clone();

        cosmic::task::future(async move {
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
        })
    }
}
