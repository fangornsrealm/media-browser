//! Message handlers for recursive scanning, metadata deletion, and file
//! watcher integration.
//!
//! Split out of `app.rs` (Phase 3 — modularization). The original
//! `Message::RecursiveScanDirectories`, `Message::MetadataDelete`,
//! `Message::NotifyEvents`, and `Message::NotifyWatcher` arms of
//! `App::update` are now individual methods on `App`, dispatched from
//! `mod.rs` with a single forwarding line each.
//!
//! Note: `notify_watcher` reads `WatcherWrapper::watcher_opt`, which is
//! defined in `types.rs`; that field must be at least `pub(super)` for
//! this sibling module to access it.

use cosmic::{app::Task, widget::segmented_button::Entity};
use notify_debouncer_full::{notify, DebouncedEvent};
use std::{collections::HashSet, fs};

use super::{App, Message, WatcherWrapper};
use crate::operation::Operation;
use crate::tab::{ItemMetadata, Location, Tab};

impl App {
    /// Handle `Message::RecursiveScanDirectories`.
    ///
    /// Spawns a background metadata scan for every selected directory in
    /// the given (or active) tab.
    pub fn recursive_scan_directories(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
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

        Task::none()
    }

    /// Handle `Message::MetadataDelete`.
    ///
    /// Removes the stored metadata database so the next scan rebuilds it.
    pub fn delete_metadata(&mut self) -> Task<Message> {
        let mut metadata_path;
        match dirs::data_local_dir() {
            Some(pb) => {
                metadata_path = pb;
                if !metadata_path.exists() {
                    let ret = std::fs::create_dir_all(metadata_path.clone());
                    if ret.is_err() {
                        log::warn!("Failed to create directory {}", metadata_path.display());
                        metadata_path = crate::sql::home_dir_or_root();
                    }
                }
            }
            None => {
                metadata_path = crate::sql::home_dir_or_root();
            }
        }
        let paths = vec![metadata_path];
        if !paths.is_empty() {
            self.operation(Operation::Delete { paths });
        }

        Task::none()
    }

    /// Handle `Message::NotifyEvents`.
    ///
    /// Applies debounced filesystem events to the matching tabs: metadata
    /// and data changes update the existing items in place, anything else
    /// triggers a full tab rescan.
    pub fn notify_events(&mut self, events: Vec<DebouncedEvent>) -> Task<Message> {
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
                                    | notify::EventKind::Modify(notify::event::ModifyKind::Data(
                                        _,
                                    )) => {
                                        // If metadata or data changed, find the matching item and reload it
                                        //TODO: this could be further optimized by looking at what exactly changed
                                        if let Some(items) = &mut tab.items_opt {
                                            for item in items.iter_mut() {
                                                if item.path_opt() == Some(event_path) {
                                                    //TODO: reload more, like mime types?
                                                    match fs::metadata(&event_path) {
                                                        Ok(new_metadata) => {
                                                            match &mut item.metadata {
                                                                ItemMetadata::Path {
                                                                    metadata,
                                                                    ..
                                                                } => *metadata = new_metadata,
                                                                _ => {}
                                                            }
                                                        }
                                                        Err(err) => {
                                                            log::warn!(
                                                                "failed to reload metadata for {:?}: {}",
                                                                path,
                                                                err
                                                            );
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
        Task::batch(commands)
    }

    /// Handle `Message::NotifyWatcher`.
    ///
    /// Stores the (re)started debouncer and refreshes the watched paths.
    pub fn notify_watcher(&mut self, mut watcher_wrapper: WatcherWrapper) -> Task<Message> {
        match watcher_wrapper.watcher_opt.take() {
            Some(watcher) => {
                self.watcher_opt = Some((watcher, HashSet::new()));
                return self.update_watcher();
            }
            None => {
                log::warn!("message did not contain notify watcher");
            }
        }

        Task::none()
    }
}
