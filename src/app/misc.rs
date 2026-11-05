//! Message handlers for application configuration, theme, window lifecycle,
//! keyboard bindings, and zoom controls.
//!
//! Split out of `app.rs` (Phase 3 — modularization). Each of the original
//! arms of `App::update` is now an individual method on `App`, dispatched
//! from `mod.rs` with a single forwarding line:
//!
//! | Group   | `Message` arm                | Method                          |
//!|----------|------------------------------|---------------------------------|
//! Config   | `Message::Config`            | `App::config`                   |
//! Config   | `Message::SetShowDetails`    | `App::set_show_details`         |
//! Theme    | `Message::AppTheme`          | `App::app_theme`                |
//! Theme    | `Message::SystemThemeModeChange` | `App::system_theme_mode_change` |
//! Window   | `Message::MaybeExit`         | `App::maybe_exit`               |
//! Window   | `Message::WindowClose`       | `App::window_close`             |
//! Window   | `Message::WindowNew`         | `App::window_new`               |
//! Keybind  | `Message::Key`               | `App::key`                      |
//! Keybind  | `Message::Modifiers`         | `App::modifiers`                |
//! Zoom     | `Message::ZoomDefault`       | `App::zoom_default`             |
//! Zoom     | `Message::ZoomIn`            | `App::zoom_in`                  |
//! Zoom     | `Message::ZoomOut`           | `App::zoom_out`                 |
//!
//! Note: `app_theme` and `set_show_details` use the `config_set!` macro,
//! which must be defined at the top level of `app/mod.rs` (moved out of the
//! former inline definition inside `App::update`) so this sibling module can
//! access it. The zoom handlers dispatch `Message::TabConfig`, which is
//! handled by the tab module.

use cosmic::app::Task;
use cosmic::iced::keyboard::{Key, Modifiers};
use cosmic::iced::window;
use cosmic::widget::segmented_button::Entity;
use cosmic::Application;
use std::{env, num::NonZeroU16, process};

use super::{App, Message, Mode};
use crate::config::{AppTheme, Config};
use crate::tab::{self, Tab};

impl App {
    /// Handle `Message::Config`.
    ///
    /// Replaces the in-memory config with the given one, preserving the
    /// existing `show_details` value so a running instance keeps its
    /// show-details state, then persists and applies the config.
    pub fn config(&mut self, config: Config) -> Task<Message> {
        if config != self.config {
            log::info!("update config");
            // Show details is preserved for existing instances
            let show_details = self.config.show_details;
            self.config = config;
            self.config.show_details = show_details;
            return self.update_config();
        }

        Task::none()
    }

    /// Handle `Message::AppTheme`.
    ///
    /// Persists the chosen app theme and re-applies the config.
    pub fn app_theme(&mut self, app_theme: AppTheme) -> Task<Message> {
        config_set!(self, app_theme, app_theme);
        self.update_config()
    }

    /// Handle `Message::SetShowDetails`.
    ///
    /// Persists the show-details toggle and re-applies the config.
    pub fn set_show_details(&mut self, show_details: bool) -> Task<Message> {
        config_set!(self, show_details, show_details);
        self.update_config()
    }

    /// Handle `Message::SystemThemeModeChange`.
    ///
    /// A system theme mode change is simply re-applied through the config.
    pub fn system_theme_mode_change(
        &mut self,
        _theme_mode: cosmic::cosmic_theme::ThemeMode,
    ) -> Task<Message> {
        self.update_config()
    }

    /// Handle `Message::MaybeExit`.
    ///
    /// Exits the process once the last window is closed and there are no
    /// pending operations.
    pub fn maybe_exit(&mut self) -> Task<Message> {
        if self.window_id_opt.is_none() && self.pending_operations.is_empty() {
            // Exit if window is closed and there are no pending operations
            process::exit(0);
        }
        Task::none()
    }

    /// Handle `Message::WindowClose`.
    ///
    /// Closes the current window and then checks whether the process should
    /// exit.
    pub fn window_close(&mut self) -> Task<Message> {
        if let Some(window_id) = self.window_id_opt.take() {
            return Task::batch([
                window::close(window_id),
                cosmic::task::message(Message::MaybeExit),
            ]);
        }

        Task::none()
    }

    /// Handle `Message::WindowNew`.
    ///
    /// Spawns a new process for the current executable, opening it in a
    /// fresh window.
    pub fn window_new(&mut self) -> Task<Message> {
        match env::current_exe() {
            Ok(exe) => match process::Command::new(&exe).spawn() {
                Ok(_child) => {}
                Err(err) => {
                    log::error!("failed to execute {:?}: {}", exe, err);
                }
            },
            Err(err) => {
                log::error!("failed to get current executable path: {}", err);
            }
        }
        Task::none()
    }

    /// Handle `Message::Key`.
    ///
    /// Looks up the active tab's key bindings and, if a binding matches the
    /// pressed key and modifiers, dispatches its associated action.
    pub fn key(&mut self, modifiers: Modifiers, key: Key) -> Task<Message> {
        let entity = self.tab_model.active();
        for (key_bind, action) in self.key_binds.iter() {
            if key_bind.matches(modifiers, &key, None) {
                return self.update(action.message(Some(entity)));
            }
        }

        Task::none()
    }

    /// Handle `Message::Modifiers`.
    ///
    /// Remembers the current keyboard modifier state, used when dispatching
    /// tab messages.
    pub fn modifiers(&mut self, modifiers: Modifiers) -> Task<Message> {
        self.modifiers = modifiers;
        Task::none()
    }

    /// Handle `Message::ZoomDefault`.
    ///
    /// Resets the icon size of the active view (list or grid) back to its
    /// default of `100`.
    pub fn zoom_default(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
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

        Task::none()
    }

    /// Handle `Message::ZoomIn`.
    ///
    /// Increments the active view's icon size to the next 25-step value,
    /// clamped to the `50`–`500` range.
    pub fn zoom_in(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
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
        self.update(Message::TabConfig(config))
    }

    /// Handle `Message::ZoomOut`.
    ///
    /// Decrements the active view's icon size to the previous 25-step value,
    /// clamped to the `50`–`500` range.
    pub fn zoom_out(&mut self, entity_opt: Option<Entity>) -> Task<Message> {
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
        self.update(Message::TabConfig(config))
    }
}
