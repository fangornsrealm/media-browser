//! Message handlers for similarity scans (images and videos).
//!
//! The scan runs on a background `std::thread` (duplo-rs is synchronous and
//! its stores are not safe for concurrent mutation). Progress is bridged to
//! the UI through a `tokio::sync::mpsc` channel that is drained into
//! `Message::SimilarProgress` messages. This follows the
//! `Message::RecursiveScanDirectories` pattern: the scan lives outside the
//! `Operation` system, with its own cancel flag and progress channel.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cosmic::{
    app::Task,
    iced::{futures::channel::mpsc as futures_mpsc, futures::StreamExt},
    task,
    widget::segmented_button::Entity,
};

use super::{App, Message};
use crate::operation::Operation;
use crate::similar::{self, SimilarKind, SimilarProgress};
use crate::tab::Tab;

/// UI state of the currently running similarity scan.
#[derive(Debug, Clone)]
pub struct SimilarScanState {
    pub kind: SimilarKind,
    pub done: usize,
    pub total: usize,
    /// The scan's output directory (`similar_images/` or `similar_videos/`).
    pub out_dir: PathBuf,
}

impl App {
    /// Handle `Message::SimilarImageScan`.
    pub fn similar_image_scan(
        &mut self,
        entity_opt: Option<Entity>,
    ) -> Task<Message> {
        self.start_similar_scan(SimilarKind::Image, entity_opt)
    }

    /// Handle `Message::SimilarVideoScan`.
    pub fn similar_video_scan(
        &mut self,
        entity_opt: Option<Entity>,
    ) -> Task<Message> {
        self.start_similar_scan(SimilarKind::Video, entity_opt)
    }

    fn start_similar_scan(
        &mut self,
        kind: SimilarKind,
        entity_opt: Option<Entity>,
    ) -> Task<Message> {
        // Only one scan at a time.
        if self.similar_scan.is_some() {
            return Task::none();
        }
        let entity = entity_opt.unwrap_or_else(|| self.tab_model.active());
        let Some(tab) = self.tab_model.data::<Tab>(entity) else {
            return Task::none();
        };
        let Some(path) = tab.location.path_opt() else {
            return Task::none();
        };
        let path = path.to_path_buf();
        self.start_similar_scan_path(kind, &path)
    }

    /// Handle a nav-bar menu item: scan the nav location's directory.
    pub fn nav_similar_scan(
        &mut self,
        kind: SimilarKind,
        entity: Entity,
    ) -> Task<Message> {
        // Only one scan at a time.
        if self.similar_scan.is_some() {
            return Task::none();
        }
        let Some(location) = self.nav_model.data::<crate::tab::Location>(entity) else {
            return Task::none();
        };
        let Some(path) = location.path_opt() else {
            return Task::none();
        };
        let path = path.to_path_buf();
        self.start_similar_scan_path(kind, &path)
    }

    fn start_similar_scan_path(
        &mut self,
        kind: SimilarKind,
        path: &std::path::Path,
    ) -> Task<Message> {
        if !path.is_dir() {
            log::warn!(
                "similar scan: refusing to scan non-directory {}",
                path.display()
            );
            return Task::none();
        }

        if kind == SimilarKind::Video && !similar::ffmpeg_available() {
            return task::batch([self.toasts.push(
                cosmic::widget::toaster::Toast::new(crate::fl!("similar-ffmpeg-missing"))
                    .duration(cosmic::widget::toaster::Duration::Long),
            )]);
        }

        let config = self.config.similar.clone();
        // Unbounded: progress reports are small and short-lived; the scan
        // thread must never block on the UI.
        let (tx, rx) =
            futures_mpsc::unbounded::<SimilarProgress>();
        let cancel = Arc::new(AtomicBool::new(false));
        self.similar_cancel_flag = Some(cancel.clone());
        self.similar_scan = Some(SimilarScanState {
            kind,
            done: 0,
            total: 0,
            out_dir: config
                .output_dir
                .clone()
                .map(|dir| dir.join(kind.output_dir_name()))
                .unwrap_or_else(|| path.join(kind.output_dir_name())),
        });

        let scan_path = path.to_path_buf();
        std::thread::spawn(move || {
            let result = match kind {
                SimilarKind::Image => similar::run_image_scan(
                    &config,
                    &scan_path,
                    &tx,
                    cancel.clone(),
                )
                .map(|r| r.pairs_found),
                SimilarKind::Video => similar::run_video_scan(
                    &config,
                    &scan_path,
                    &tx,
                    cancel.clone(),
                )
                .map(|r| r.html_files),
            };
            match result {
                Ok(count) => {
                    let _ = tx.unbounded_send(SimilarProgress::Complete {
                        pairs_found: count,
                        html_files: count,
                    });
                }
                Err(err) => {
                    if !matches!(err, similar::SimilarError::Cancelled) {
                        let _ = tx.unbounded_send(SimilarProgress::Warning {
                            message: err.to_string(),
                        });
                    }
                }
            }
        });

        // Drain the progress channel into app messages; the stream ends
        // when the scan thread drops the sender.
        task::stream(rx.map(Message::SimilarProgress))
    }

    /// Handle `Message::SimilarCancel`.
    pub fn similar_cancel(&mut self) -> Task<Message> {
        if let Some(flag) = self.similar_cancel_flag.as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
        Task::none()
    }

    /// Handle `Message::SimilarProgress`.
    pub fn similar_progress(&mut self, progress: SimilarProgress) -> Task<Message> {
        match progress {
            SimilarProgress::FileProgress {
                done,
                total,
                last_match,
            } => {
                if let Some(state) = self.similar_scan.as_mut() {
                    state.done = done;
                    state.total = total;
                }
                if let Some(last_match) = last_match {
                    log::info!("similar scan: matched {last_match}");
                }
                Task::none()
            }
            SimilarProgress::PairCreated { path } => {
                log::info!("similar scan: results written to {}", path.display());
                Task::none()
            }
            SimilarProgress::Warning { message } => {
                task::batch([self.toasts.push(
                    cosmic::widget::toaster::Toast::new(message)
                        .duration(cosmic::widget::toaster::Duration::Long),
                )])
            }
            SimilarProgress::Cancelled => {
                self.similar_scan = None;
                self.similar_cancel_flag = None;
                task::batch([self.toasts.push(
                    cosmic::widget::toaster::Toast::new(crate::fl!(
                        "similar-scan-cancelled"
                    )),
                )])
            }
            SimilarProgress::Complete { pairs_found, html_files } => {
                let state = self.similar_scan.take();
                self.similar_cancel_flag = None;
                let kind = state.as_ref().map(|state| state.kind);
                let out_dir = state.map(|state| state.out_dir);
                let complete_msg = match kind {
                    Some(SimilarKind::Video) => crate::fl!(
                        "similar-video-scan-complete",
                        count = html_files
                    ),
                    _ => crate::fl!(
                        "similar-image-scan-complete",
                        count = pairs_found
                    ),
                };
                task::batch([self.toasts.push(
                    cosmic::widget::toaster::Toast::new(complete_msg)
                        .duration(cosmic::widget::toaster::Duration::Long)
                        .action(
                            crate::fl!("similar-open-results"),
                            move |_| {
                                Message::SimilarOpenResults(
                                    out_dir.clone().unwrap_or_default(),
                                )
                            },
                        ),
                )])
            }
        }
    }

    /// Handle `Message::SimilarOpenResults`.
    pub fn similar_open_results(&mut self, path: PathBuf) -> Task<Message> {
        if !path.is_dir() {
            log::warn!(
                "similar scan: results directory {} no longer exists",
                path.display()
            );
            return Task::none();
        }
        self.open_tab(crate::tab::Location::Path(path), true, None)
    }

    /// Handle `Message::SimilarDeleteStore`.
    ///
    /// Moves the persistent store to the trash (recoverable); the next scan
    /// starts fresh.
    pub fn similar_delete_store(&mut self, kind: SimilarKind) -> Task<Message> {
        let store_dir = match similar::data_dir() {
            Ok(dir) => dir,
            Err(err) => {
                log::error!("similar scan: cannot open data directory: {err}");
                return Task::none();
            }
        };
        let path = store_dir.join(kind.store_file_name());
        if !path.exists() {
            log::info!(
                "similar scan: no {} store to delete at {}",
                kind.media_name(),
                path.display()
            );
            return Task::none();
        }
        self.operation(Operation::Delete {
            paths: vec![path],
        });
        Task::none()
    }

    /// Handle `Message::SimilarConfig`.
    pub fn similar_config(&mut self, new_config: crate::similar::SimilarConfig) -> Task<Message> {
        config_set!(self, similar, new_config);
        self.update_config()
    }
}
