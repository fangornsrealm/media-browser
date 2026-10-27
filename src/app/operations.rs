//! Message handlers for pending operations and operation state management.
//!
//! Split out of `app.rs` (Phase 3 — modularization). The original
//! `Message::PendingCancel`, `Message::PendingCancelAll`,
//! `Message::PendingComplete`, `Message::PendingDismiss`,
//! `Message::PendingError`, `Message::PendingPause`,
//! `Message::PendingPauseAll`, and `Message::Undo` arms of
//! `App::update` are now individual methods on `App`, dispatched from
//! `mod.rs` with a single forwarding line each.

use cosmic::app::Task;
use cosmic::widget::{self, toaster::ToastId};
use std::path::PathBuf;
use std::sync::Arc;

use super::{App, DialogPage, Message};
use crate::fl;
use crate::operation::{Operation, OperationSelection};

impl App {
    /// Handle `Message::PendingCancel`.
    ///
    /// Cancels the operation identified by `id` and removes it from the
    /// set of operations currently showing progress.
    pub fn pending_cancel(&mut self, id: u64) -> Task<Message> {
        if let Some((_, controller)) = self.pending_operations.get(&id) {
            controller.cancel();
            self.progress_operations.remove(&id);
        }
        Task::none()
    }

    /// Handle `Message::PendingCancelAll`.
    ///
    /// Cancels every pending operation and clears the progress set.
    pub fn pending_cancel_all(&mut self) -> Task<Message> {
        for (id, (_, controller)) in self.pending_operations.iter() {
            controller.cancel();
            self.progress_operations.remove(&id);
        }
        Task::none()
    }

    /// Handle `Message::PendingComplete`.
    ///
    /// Finalizes a completed operation: optionally shows an undo toast
    /// (for delete operations), records the operation, clears the progress
    /// set if no operations still need it, and kicks off post-completion
    /// rescans and notifications.
    pub fn pending_complete(&mut self, id: u64, op_sel: OperationSelection) -> Task<Message> {
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
                                    .action(fl!("undo"), move |tid: ToastId| {
                                        Message::UndoTrash(tid, paths.clone())
                                    }),
                            )
                            .map(cosmic::app::Message::App),
                    );
                }
            }
            self.complete_operations.insert(id, op);
        }

        // Close progress notification if all relevant operations are finished
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

        Task::batch(commands)
    }

    /// Handle `Message::PendingDismiss`.
    ///
    /// Clears the set of operations currently showing progress, used when
    /// the user dismisses the progress indicator.
    pub fn pending_dismiss(&mut self) -> Task<Message> {
        self.progress_operations.clear();
        Task::none()
    }

    /// Handle `Message::PendingError`.
    ///
    /// Records a failed operation. If the operation was not explicitly
    /// cancelled, a `DialogPage::FailedOperation` is pushed so the user
    /// can inspect the error.
    pub fn pending_error(&mut self, id: u64, err: String) -> Task<Message> {
        if let Some((op, controller)) = self.pending_operations.remove(&id) {
            // Only show dialog if not cancelled
            if !controller.is_cancelled() {
                self.dialog_pages.push_back(DialogPage::FailedOperation(id));
            }
            // Remove from progress
            self.progress_operations.remove(&id);
            self.failed_operations.insert(id, (op, controller, err));
        }

        // Close progress notification if all relevant operations are finished
        if !self
            .pending_operations
            .iter()
            .any(|(_id, (op, _))| op.show_progress_notification())
        {
            self.progress_operations.clear();
        }
        Task::none()
    }

    /// Handle `Message::PendingPause`.
    ///
    /// Pauses or unpauses the operation identified by `id`.
    pub fn pending_pause(&mut self, id: u64, pause: bool) -> Task<Message> {
        if let Some((_, controller)) = self.pending_operations.get(&id) {
            if pause {
                controller.pause();
            } else {
                controller.unpause();
            }
        }
        Task::none()
    }

    /// Handle `Message::PendingPauseAll`.
    ///
    /// Pauses or unpauses every pending operation.
    pub fn pending_pause_all(&mut self, pause: bool) -> Task<Message> {
        for (_id, (_, controller)) in self.pending_operations.iter() {
            if pause {
                controller.pause();
            } else {
                controller.unpause();
            }
        }
        Task::none()
    }

    /// Handle `Message::Undo`.
    ///
    /// Placeholder — undo is not yet implemented.
    pub fn undo(&mut self, _id: usize) -> Task<Message> {
        // TODO: undo
        Task::none()
    }
}
