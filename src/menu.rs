// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use cosmic::{
    Element,
    iced::{Alignment, Background, Border, Length},
    theme,
    widget::{
        self, Row, button, column, container, divider,
        menu::{self, ItemHeight, ItemWidth, MenuBar, key_bind::KeyBind},
        space, text,
    },
};
use mime_guess::Mime;
use std::collections::HashMap;

use crate::{
    app::{Action, Message},
    config::Config,
    fl,
    tab::{self, HeadingOptions, Location, LocationMenuAction, Tab},
};

macro_rules! menu_button {
    ($($x:expr),+ $(,)?) => (
        button::custom(
            Row::with_children(
                vec![$(Element::from($x)),+]
            )
            .height(Length::Fixed(24.0))
            .align_y(Alignment::Center)
        )
        .padding([theme::active().cosmic().spacing.space_xxxs, 16])
        .width(Length::Fill)
        .class(theme::Button::MenuItem)
    );
}

pub fn context_menu<'a>(
    tab: &Tab,
    key_binds: &HashMap<KeyBind, Action>,
) -> Element<'a, tab::Message> {
    let find_key = |action: &Action| -> String {
        for (key_bind, key_action) in key_binds.iter() {
            if action == key_action {
                return key_bind.to_string();
            }
        }
        String::new()
    };

    let menu_item = |label, action| {
        let key = find_key(&action);
        menu_button!(text::body(label), space::horizontal(), text::body(key))
            .on_press(tab::Message::ContextAction(action))
    };

    let (sort_name, sort_direction, _) = tab.sort_options();
    let sort_item = |label, variant| {
        menu_item(
            format!(
                "{} {}",
                label,
                match (sort_name == variant, sort_direction) {
                    (true, true) => "\u{2B07}",
                    (true, false) => "\u{2B06}",
                    _ => "",
                }
            ),
            Action::ToggleSort(variant),
        )
        .into()
    };

    let mut selected_dir = 0;
    let mut selected = 0;
    let mut selected_types: Vec<Mime> = vec![];
    tab.items_opt().map(|items| {
        for item in items.iter() {
            if item.selected {
                selected += 1;
                if item.metadata.is_dir() {
                    selected_dir += 1;
                }
                selected_types.push(item.mime.clone());
            }
        }
    });
    selected_types.sort_unstable();
    selected_types.dedup();

    let mut children: Vec<Element<_>> = Vec::new();
    match (&tab.mode, &tab.location) {
        (
            tab::Mode::Audio | tab::Mode::Image | tab::Mode::Video,
            Location::DBSearch(_)
            | Location::Tag(_)
            | Location::Collection(_)
            | Location::Path(_)
            | Location::Search(_, _, _, _)
            | Location::Recents,
        ) => {}
        (tab::Mode::App | tab::Mode::Desktop | tab::Mode::Browser, Location::DBSearch(_)) => {
            children.push(menu_item(fl!("search-context"), Action::SearchDB).into());
            children.push(divider::horizontal::light().into());
            children.push(menu_item(fl!("zoom-in"), Action::ZoomIn).into());
            children.push(menu_item(fl!("zoom-out"), Action::ZoomOut).into());
            children.push(menu_item(fl!("default-size"), Action::ZoomDefault).into());
            children.push(divider::horizontal::light().into());
            children.push(sort_item(
                fl!("media-browser"),
                HeadingOptions::MediaSpecific,
            ));
            children.push(sort_item(fl!("sort-by-name"), HeadingOptions::Name));
            children.push(sort_item(fl!("sort-by-modified"), HeadingOptions::Modified));
            children.push(sort_item(fl!("sort-by-size"), HeadingOptions::Size));
        }
        (tab::Mode::App | tab::Mode::Desktop | tab::Mode::Browser, Location::Tag(_)) => {}
        (
            tab::Mode::App | tab::Mode::Desktop | tab::Mode::Browser,
            Location::Path(_)
            | Location::Collection(_)
            | Location::Search(_, _, _, _)
            | Location::Recents,
        ) => {
            children.push(menu_item(fl!("search-context"), Action::SearchDB).into());
            if selected > 0 {
                if selected_dir == 1 && selected == 1 || selected_dir == 0 {
                    children.push(menu_item(fl!("open"), Action::Open).into());
                }
                if matches!(tab.location, Location::Search(..)) {
                    children.push(
                        menu_item(fl!("open-item-location"), Action::OpenItemLocation).into(),
                    );
                }
                // All selected items are directories
                if selected == selected_dir {
                    children
                        .push(menu_item(fl!("open-in-new-window"), Action::OpenInNewWindow).into());
                    children.push(
                        menu_item(
                            fl!("recursive-scan-directories"),
                            Action::RecursiveScanDirectories,
                        )
                        .into(),
                    );
                }
                children.push(divider::horizontal::light().into());
                children.push(menu_item(fl!("rename"), Action::Rename).into());
                children.push(menu_item(fl!("cut"), Action::Cut).into());
                children.push(menu_item(fl!("copy"), Action::Copy).into());
                children.push(menu_item(fl!("show-details"), Action::Preview).into());
                children.push(divider::horizontal::light().into());
                //TODO: Print?
                children.push(menu_item(fl!("add-to-sidebar"), Action::AddToSidebar).into());
                children.push(menu_item(fl!("add-new-tag"), Action::AddTagToSidebar).into());
                children.push(divider::horizontal::light().into());
                children.push(menu_item(fl!("move-to-trash"), Action::MoveToTrash).into());
            } else {
                //TODO: need better designs for menu with no selection
                //TODO: have things like properties but they apply to the folder?
                children.push(menu_item(fl!("new-folder"), Action::NewFolder).into());
                children.push(divider::horizontal::light().into());
                if tab.mode.multiple() {
                    children.push(menu_item(fl!("select-all"), Action::SelectAll).into());
                }
                children.push(menu_item(fl!("paste"), Action::Paste).into());
                children.push(divider::horizontal::light().into());
                // TODO: Nested menu
                children.push(sort_item(
                    fl!("media-browser"),
                    HeadingOptions::MediaSpecific,
                ));
                children.push(sort_item(fl!("sort-by-name"), HeadingOptions::Name));
                children.push(sort_item(fl!("sort-by-modified"), HeadingOptions::Modified));
                children.push(sort_item(fl!("sort-by-size"), HeadingOptions::Size));
            }
        }
        (
            tab::Mode::Dialog(dialog_kind),
            Location::DBSearch(_)
            | Location::Tag(_)
            | Location::Collection(_)
            | Location::Path(_)
            | Location::Search(_, _, _, _)
            | Location::Recents,
        ) => {
            if selected > 0 {
                if selected_dir == 1 && selected == 1 || selected_dir == 0 {
                    children.push(menu_item(fl!("open"), Action::Open).into());
                }
                if matches!(tab.location, Location::Search(_, _, _, _)) {
                    children.push(
                        menu_item(fl!("open-item-location"), Action::OpenItemLocation).into(),
                    );
                }
                children.push(divider::horizontal::light().into());
                children.push(menu_item(fl!("show-details"), Action::Preview).into());
            } else {
                if dialog_kind.save() {
                    children.push(menu_item(fl!("new-folder"), Action::NewFolder).into());
                }
                if tab.mode.multiple() {
                    children.push(menu_item(fl!("select-all"), Action::SelectAll).into());
                }
                if !children.is_empty() {
                    children.push(divider::horizontal::light().into());
                }
                children.push(sort_item(
                    fl!("media-browser"),
                    HeadingOptions::MediaSpecific,
                ));
                children.push(sort_item(fl!("sort-by-name"), HeadingOptions::Name));
                children.push(sort_item(fl!("sort-by-modified"), HeadingOptions::Modified));
                children.push(sort_item(fl!("sort-by-size"), HeadingOptions::Size));
            }
        }
        (_, Location::Network(_, _)) => {
            if selected > 0 {
                if selected_dir == 1 && selected == 1 || selected_dir == 0 {
                    children.push(menu_item(fl!("open"), Action::Open).into());
                }
            } else {
                if tab.mode.multiple() {
                    children.push(menu_item(fl!("select-all"), Action::SelectAll).into());
                }
                if !children.is_empty() {
                    children.push(divider::horizontal::light().into());
                }
                children.push(sort_item(
                    fl!("media-browser"),
                    HeadingOptions::MediaSpecific,
                ));
                children.push(sort_item(fl!("sort-by-name"), HeadingOptions::Name));
                children.push(sort_item(fl!("sort-by-modified"), HeadingOptions::Modified));
                children.push(sort_item(fl!("sort-by-size"), HeadingOptions::Size));
            }
        }
        (_, Location::Trash) => {
            if tab.mode.multiple() {
                children.push(menu_item(fl!("select-all"), Action::SelectAll).into());
            }
            if !children.is_empty() {
                children.push(divider::horizontal::light().into());
            }
            if selected > 0 {
                children.push(divider::horizontal::light().into());
                children
                    .push(menu_item(fl!("restore-from-trash"), Action::RestoreFromTrash).into());
            } else {
                // TODO: Nested menu
                children.push(sort_item(
                    fl!("media-browser"),
                    HeadingOptions::MediaSpecific,
                ));
                children.push(sort_item(fl!("sort-by-name"), HeadingOptions::Name));
                children.push(sort_item(fl!("sort-by-modified"), HeadingOptions::Modified));
                children.push(sort_item(fl!("sort-by-size"), HeadingOptions::Size));
            }
        }
    }

    container(column::with_children(children))
        .padding(1)
        //TODO: move style to libcosmic
        .class(theme::Container::custom(|theme| {
            let component = theme.current_container();
            container::Style {
                icon_color: Some(component.on.into()),
                text_color: Some(component.on.into()),
                background: Some(Background::Color(component.base.into())),
                border: Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: component.divider.into(),
                },
                ..Default::default()
            }
        }))
        .width(Length::Fixed(260.0))
        .into()
}

pub fn dialog_menu<'a>(
    tab: &Tab,
    key_binds: &HashMap<KeyBind, Action>,
    show_details: bool,
) -> Element<'static, Message> {
    let (sort_name, sort_direction, _) = tab.sort_options();
    let sort_item = |label, sort, dir| {
        menu::Item::CheckBox(
            label,
            None,
            sort_name == sort && sort_direction == dir,
            Action::SetSort(sort, dir),
        )
    };

    let in_trash = tab.location == Location::Trash;

    MenuBar::new(vec![
        menu::Tree::with_children(
            widget::RcElementWrapper::new(Element::from(
                widget::button::icon(widget::icon::from_name(match tab.config.view {
                    tab::View::Grid => "view-grid-symbolic",
                    tab::View::List => "view-list-symbolic",
                }))
                // This prevents the button from being shown as insensitive
                .on_press(Message::None)
                .padding(8),
            )),
            menu::items(
                key_binds,
                vec![
                    menu::Item::CheckBox(
                        fl!("grid-view"),
                        None,
                        matches!(tab.config.view, tab::View::Grid),
                        Action::TabViewGrid,
                    ),
                    menu::Item::CheckBox(
                        fl!("list-view"),
                        None,
                        matches!(tab.config.view, tab::View::List),
                        Action::TabViewList,
                    ),
                ],
            ),
        ),
        menu::Tree::with_children(
            widget::RcElementWrapper::new(Element::from(
                widget::button::icon(widget::icon::from_name(if sort_direction {
                    "view-sort-ascending-symbolic"
                } else {
                    "view-sort-descending-symbolic"
                }))
                // This prevents the button from being shown as insensitive
                .on_press(Message::None)
                .padding(8),
            )),
            menu::items(
                key_binds,
                vec![
                    sort_item(
                        fl!("media-browser"),
                        tab::HeadingOptions::MediaSpecific,
                        true,
                    ),
                    sort_item(fl!("sort-a-z"), tab::HeadingOptions::Name, true),
                    sort_item(fl!("sort-z-a"), tab::HeadingOptions::Name, false),
                    sort_item(
                        fl!("sort-newest-first"),
                        if in_trash {
                            tab::HeadingOptions::TrashedOn
                        } else {
                            tab::HeadingOptions::Modified
                        },
                        false,
                    ),
                    sort_item(
                        fl!("sort-oldest-first"),
                        if in_trash {
                            tab::HeadingOptions::TrashedOn
                        } else {
                            tab::HeadingOptions::Modified
                        },
                        true,
                    ),
                    sort_item(
                        fl!("sort-smallest-to-largest"),
                        tab::HeadingOptions::Size,
                        true,
                    ),
                    sort_item(
                        fl!("sort-largest-to-smallest"),
                        tab::HeadingOptions::Size,
                        false,
                    ),
                    //TODO: sort by type
                ],
            ),
        ),
        menu::Tree::with_children(
            widget::RcElementWrapper::new(Element::from(
                widget::button::icon(widget::icon::from_name("view-more-symbolic"))
                    // This prevents the button from being shown as insensitive
                    .on_press(Message::None)
                    .padding(8),
            )),
            menu::items(
                key_binds,
                vec![
                    menu::Item::Button(fl!("zoom-in"), None, Action::ZoomIn),
                    menu::Item::Button(fl!("default-size"), None, Action::ZoomDefault),
                    menu::Item::Button(fl!("zoom-out"), None, Action::ZoomOut),
                    menu::Item::Divider,
                    menu::Item::CheckBox(
                        fl!("show-hidden-files"),
                        None,
                        tab.config.show_hidden,
                        Action::ToggleShowHidden,
                    ),
                    menu::Item::CheckBox(
                        fl!("list-directories-first"),
                        None,
                        tab.config.folders_first,
                        Action::ToggleFoldersFirst,
                    ),
                    menu::Item::CheckBox(fl!("show-details"), None, show_details, Action::Preview),
                ],
            ),
        ),
    ])
    .item_height(ItemHeight::Dynamic(40))
    .item_width(ItemWidth::Uniform(240))
    .spacing(theme::active().cosmic().spacing.space_xxxs.into())
    .into()
}

pub fn menu_bar<'a>(
    tab_opt: Option<&Tab>,
    config: &Config,
    key_binds: &HashMap<KeyBind, Action>,
) -> Element<'a, Message> {
    let sort_options = tab_opt.map(|tab| tab.sort_options());
    let sort_item = |label, sort, dir| {
        menu::Item::CheckBox(
            label,
            None,
            sort_options.map_or(false, |(sort_name, sort_direction, _)| {
                sort_name == sort && sort_direction == dir
            }),
            Action::SetSort(sort, dir),
        )
    };

    let mut selected_dir = 0;
    let mut selected = 0;
    let mut selected_gallery = 0;
    tab_opt.and_then(|tab| tab.items_opt()).map(|items| {
        for item in items.iter() {
            if item.selected {
                selected += 1;
                if item.metadata.is_dir() {
                    selected_dir += 1;
                }
                if item.can_gallery() {
                    selected_gallery += 1;
                }
            }
        }
    });

    MenuBar::new(vec![
        menu::Tree::with_children(
            widget::RcElementWrapper::new(Element::from(menu::root(fl!("file")))),
            menu::items(
                key_binds,
                vec![
                    menu::Item::Button(fl!("new-folder"), None, Action::NewFolder),
                    menu::Item::Button(fl!("open"), None, Action::Open),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("rename"), None, Action::Rename),
                    menu::Item::Divider,
                    menu::Item::Button(
                        fl!("recursive-scan-directories"),
                        None,
                        Action::RecursiveScanDirectories,
                    ),
                    menu::Item::Button(fl!("search-context"), None, Action::SearchDB),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("add-to-sidebar"), None, Action::AddToSidebar),
                    menu::Item::Button(fl!("add-new-tag"), None, Action::AddTagToSidebar),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("move-to-trash"), None, Action::MoveToTrash),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("quit"), None, Action::WindowClose),
                ],
            ),
        ),
        menu::Tree::with_children(
            widget::RcElementWrapper::new(Element::from(menu::root(fl!("edit")))),
            menu::items(
                key_binds,
                vec![
                    menu::Item::Button(fl!("cut"), None, Action::Cut),
                    menu::Item::Button(fl!("copy"), None, Action::Copy),
                    menu::Item::Button(fl!("paste"), None, Action::Paste),
                    menu::Item::Button(fl!("select-all"), None, Action::SelectAll),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("search-context"), None, Action::SearchDB),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("history"), None, Action::EditHistory),
                ],
            ),
        ),
        menu::Tree::with_children(
            widget::RcElementWrapper::new(Element::from(menu::root(fl!("view")))),
            menu::items(
                key_binds,
                vec![
                    menu::Item::Button(fl!("zoom-in"), None, Action::ZoomIn),
                    menu::Item::Button(fl!("default-size"), None, Action::ZoomDefault),
                    menu::Item::Button(fl!("zoom-out"), None, Action::ZoomOut),
                    menu::Item::Divider,
                    menu::Item::CheckBox(
                        fl!("grid-view"),
                        None,
                        tab_opt.map_or(false, |tab| matches!(tab.config.view, tab::View::Grid)),
                        Action::TabViewGrid,
                    ),
                    menu::Item::CheckBox(
                        fl!("list-view"),
                        None,
                        tab_opt.map_or(false, |tab| matches!(tab.config.view, tab::View::List)),
                        Action::TabViewList,
                    ),
                    menu::Item::Divider,
                    menu::Item::CheckBox(
                        fl!("show-hidden-files"),
                        None,
                        tab_opt.map_or(false, |tab| tab.config.show_hidden),
                        Action::ToggleShowHidden,
                    ),
                    menu::Item::CheckBox(
                        fl!("list-directories-first"),
                        None,
                        tab_opt.map_or(false, |tab| tab.config.folders_first),
                        Action::ToggleFoldersFirst,
                    ),
                    menu::Item::CheckBox(
                        fl!("show-details"),
                        None,
                        config.show_details,
                        Action::Preview,
                    ),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("menu-settings"), None, Action::Settings),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("menu-about"), None, Action::About),
                ],
            ),
        ),
        menu::Tree::with_children(
            widget::RcElementWrapper::new(Element::from(menu::root(fl!("sort")))),
            menu::items(
                key_binds,
                vec![
                    sort_item(
                        fl!("media-browser"),
                        tab::HeadingOptions::MediaSpecific,
                        true,
                    ),
                    sort_item(fl!("sort-a-z"), tab::HeadingOptions::Name, true),
                    sort_item(fl!("sort-z-a"), tab::HeadingOptions::Name, false),
                    sort_item(
                        fl!("sort-newest-first"),
                        tab::HeadingOptions::Modified,
                        false,
                    ),
                    sort_item(
                        fl!("sort-oldest-first"),
                        tab::HeadingOptions::Modified,
                        true,
                    ),
                    sort_item(
                        fl!("sort-smallest-to-largest"),
                        tab::HeadingOptions::Size,
                        true,
                    ),
                    sort_item(
                        fl!("sort-largest-to-smallest"),
                        tab::HeadingOptions::Size,
                        false,
                    ),
                    //TODO: sort by type
                ],
            ),
        ),
    ])
    .item_height(ItemHeight::Dynamic(40))
    .item_width(ItemWidth::Uniform(240))
    .spacing(theme::active().cosmic().spacing.space_xxxs.into())
    .into()
}

pub fn location_context_menu<'a>(ancestor_index: usize) -> Element<'a, tab::Message> {
    let children = vec![
        menu_button!(text::body(fl!("open-in-new-window")))
            .on_press(tab::Message::LocationMenuAction(
                LocationMenuAction::OpenInNewWindow(ancestor_index),
            ))
            .into(),
    ];

    container(column::with_children(children))
        .padding(1)
        .class(theme::Container::custom(|theme| {
            let component = theme.current_container();
            container::Style {
                icon_color: Some(component.on.into()),
                text_color: Some(component.on.into()),
                background: Some(Background::Color(component.base.into())),
                border: Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: component.divider.into(),
                },
                ..Default::default()
            }
        }))
        .width(Length::Fixed(240.0))
        .into()
}
