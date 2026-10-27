use crate::ClipboardProvider;

use crate::dnd::DndProvider;
use clipboard_win::{get_clipboard_string, set_clipboard_string};
use dnd::{DndAction, DndDestinationRectangle, DndSurface, Icon};
use mime::{AllowedMimeTypes, AsMimeTypes};
use raw_window_handle::HasDisplayHandle;
use std::{borrow::Cow, error::Error};

pub fn connect<W: HasDisplayHandle + ?Sized>(
    _window: &W,
) -> Result<Clipboard, Box<dyn Error>> {
    Ok(Clipboard)
}
impl DndProvider for Clipboard {
    fn init_dnd(
        &self,
        _tx: Box<dyn dnd::Sender<DndSurface> + Send + Sync + 'static>,
    ) {
    }

    fn start_dnd<D: AsMimeTypes + Send + 'static>(
        &self,
        _internal: bool,
        _source_surface: DndSurface,
        _icon_surface: Option<Icon>,
        _content: D,
        _actions: DndAction,
    ) {
    }

    fn end_dnd(&self) {}

    fn register_dnd_destination(
        &self,
        _surface: DndSurface,
        _rectangles: Vec<DndDestinationRectangle>,
    ) {
    }

    fn set_action(&self, _action: DndAction) {}

    fn peek_offer<D: AllowedMimeTypes + 'static>(
        &self,
        _mime_type: Option<Cow<'static, str>>,
    ) -> std::io::Result<D> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "DnD not supported",
        ))
    }
}

pub struct Clipboard;

impl ClipboardProvider for Clipboard {
    fn read(&self) -> Result<String, Box<dyn Error>> {
        Ok(get_clipboard_string()?)
    }

    fn write(&mut self, contents: String) -> Result<(), Box<dyn Error>> {
        Ok(set_clipboard_string(&contents)?)
    }
}
