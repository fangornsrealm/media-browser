use crate::ClipboardProvider;

use crate::dnd::DndProvider;
use dnd::{DndAction, DndDestinationRectangle, DndSurface, Icon};
use mime::{AllowedMimeTypes, AsMimeTypes};
use raw_window_handle::HasDisplayHandle;
use std::{borrow::Cow, error::Error};

pub fn connect<W: HasDisplayHandle>(
    _window: &W,
) -> Result<Clipboard, Box<dyn Error>> {
    Clipboard::new()
}

pub struct Clipboard;

impl Clipboard {
    pub fn new() -> Result<Clipboard, Box<dyn Error>> {
        Ok(Self)
    }
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum AndroidClipboardError {
    Unimplemented,
}

impl std::fmt::Display for AndroidClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unimplemented")
    }
}

impl Error for AndroidClipboardError {}

impl ClipboardProvider for Clipboard {
    fn read(&self) -> Result<String, Box<dyn Error>> {
        Err(Box::new(AndroidClipboardError::Unimplemented))
    }

    fn write(&mut self, _contents: String) -> Result<(), Box<dyn Error>> {
        Err(Box::new(AndroidClipboardError::Unimplemented))
    }
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
