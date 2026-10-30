// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use cosmic::{
    iced::core::keyboard::key::Named,
    iced::keyboard::Key,
    widget::menu::key_bind::{KeyBind, Modifier},
};
use std::collections::HashMap;

use crate::{app::Action, tab};

//TODO: load from config
pub fn key_binds(mode: &tab::Mode) -> HashMap<KeyBind, Action> {
    let mut key_binds = HashMap::new();

    macro_rules! bind {
        ([$($modifier:ident),* $(,)?], $key:expr, $action:ident) => {{
            key_binds.insert(
                KeyBind {
                    modifiers: vec![$(Modifier::$modifier),*],
                    key: $key,
                },
                Action::$action,
            );
        }};
    }

    // Common keys
    bind!([], Key::Named(Named::ArrowDown), ItemDown);
    bind!([], Key::Named(Named::ArrowLeft), ItemLeft);
    bind!([], Key::Named(Named::ArrowRight), ItemRight);
    bind!([], Key::Named(Named::ArrowUp), ItemUp);
    bind!([Shift], Key::Named(Named::ArrowDown), ItemDown);
    bind!([Shift], Key::Named(Named::ArrowLeft), ItemLeft);
    bind!([Shift], Key::Named(Named::ArrowRight), ItemRight);
    bind!([Shift], Key::Named(Named::ArrowUp), ItemUp);
    bind!([], Key::Named(Named::PageUp), Previous);
    bind!([], Key::Named(Named::PageDown), Next);
    bind!([], Key::Named(Named::Escape), OpenBrowser);
    bind!([Ctrl, Shift], Key::Character("n".into()), NewFolder);
    bind!([], Key::Named(Named::Enter), Open);
    bind!([Ctrl], Key::Character(" ".into()), Preview);
    bind!([Ctrl], Key::Character("h".into()), ToggleShowHidden);
    bind!([Ctrl], Key::Character("a".into()), SelectAll);
    bind!([Ctrl], Key::Character("=".into()), ZoomIn);
    bind!([Ctrl], Key::Character("+".into()), ZoomIn);
    bind!([Ctrl], Key::Character("0".into()), ZoomDefault);
    bind!([Ctrl], Key::Character("-".into()), ZoomOut);

    // App-only keys
    if matches!(mode, tab::Mode::App) {
        bind!([Ctrl], Key::Character("d".into()), AddToSidebar);
        bind!([Ctrl], Key::Character("t".into()), AddTagToSidebar);
        bind!([Ctrl], Key::Character(",".into()), Settings);
        bind!([Ctrl], Key::Named(Named::Tab), TabNext);
        bind!([Ctrl, Shift], Key::Named(Named::Tab), TabPrev);
        bind!([Ctrl], Key::Character("q".into()), WindowClose);
    }

    // App and desktop only keys
    if matches!(mode, tab::Mode::App | tab::Mode::Desktop) {
        bind!([Ctrl], Key::Character("c".into()), Copy);
        bind!([Ctrl], Key::Character("x".into()), Cut);
        bind!([], Key::Named(Named::Delete), MoveToTrash);
        bind!([Shift], Key::Named(Named::Enter), OpenInNewWindow);
        bind!([Ctrl], Key::Character("v".into()), Paste);
        bind!([], Key::Named(Named::F2), Rename);
    }

    // App and dialog only keys
    if matches!(mode, tab::Mode::App | tab::Mode::Dialog(_)) {
        bind!([Ctrl], Key::Character("l".into()), EditLocation);
        bind!([Alt], Key::Named(Named::ArrowRight), HistoryNext);
        bind!([Alt], Key::Named(Named::ArrowLeft), HistoryPrevious);
        bind!([], Key::Named(Named::Backspace), HistoryPrevious);
        bind!([Alt], Key::Named(Named::ArrowUp), LocationUp);
        bind!([Ctrl], Key::Character("f".into()), SearchActivate);
    }

    if matches!(mode, tab::Mode::Image) {
        bind!([Ctrl], Key::Character("l".into()), EditLocation);
        bind!([], Key::Named(Named::ArrowRight), ItemRight);
        bind!([], Key::Named(Named::ArrowLeft), ItemLeft);
        bind!([], Key::Named(Named::Backspace), HistoryPrevious);
        bind!([], Key::Named(Named::ArrowUp), LocationUp);
        bind!([], Key::Named(Named::PageUp), Previous);
        bind!([], Key::Named(Named::PageDown), Next);
        bind!([], Key::Named(Named::Escape), OpenBrowser);
    }

    if matches!(mode, tab::Mode::Video | tab::Mode::Audio) {
        bind!([], Key::Named(Named::ArrowRight), SeekForward);
        bind!([], Key::Named(Named::ArrowLeft), SeekBackward);
        bind!([], Key::Character(" ".into()), PlayPause);
        bind!([], Key::Character("m".into()), AudioMuteToggle);
        //bind!([], Key::Character("s".into()), SubtitleToggle);
        bind!([], Key::Character("f".into()), SeekForward);
        bind!([], Key::Character("b".into()), SeekBackward);
        bind!([], Key::Named(Named::Home), PlayFromBeginning);
        bind!([], Key::Named(Named::End), ItemRight);
    }

    key_binds
}
