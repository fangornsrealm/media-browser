use std::borrow::Cow;

use ::dnd::{DndAction, DndDestinationRectangle, Sender};
use dnd::{DndSurface, Icon};
use mime::{AllowedMimeTypes, AsMimeTypes};

pub trait DndProvider {
    /// Set up DnD operations for the Clipboard
    fn init_dnd(
        &self,
        _tx: Box<dyn dnd::Sender<DndSurface> + Send + Sync + 'static>,
    ) {
    }

    /// Start a DnD operation on the given surface with some data
    fn start_dnd<D: AsMimeTypes + Send + 'static>(
        &self,
        _internal: bool,
        _source_surface: DndSurface,
        _icon_surface: Option<Icon>,
        _content: D,
        _actions: DndAction,
    ) {
    }

    /// End the current DnD operation, if there is one
    fn end_dnd(&self) {}

    /// Register a surface for receiving DnD offers
    /// Rectangles should be provided in order of decreasing priority.
    /// This method can be called multiple time for a single surface if the
    /// rectangles change.
    fn register_dnd_destination(
        &self,
        _surface: DndSurface,
        _rectangles: Vec<DndDestinationRectangle>,
    ) {
    }

    /// Set the final action after presenting the user with a choice
    fn set_action(&self, _action: DndAction) {}

    /// Peek at the contents of a DnD offer
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

impl<C: DndProvider> DndProvider for crate::PlatformClipboard<C> {
    fn init_dnd(
        &self,
        tx: Box<dyn Sender<DndSurface> + Send + Sync + 'static>,
    ) {
        self.raw.init_dnd(tx);
    }

    fn start_dnd<D: AsMimeTypes + Send + 'static>(
        &self,
        internal: bool,
        source_surface: DndSurface,
        icon_surface: Option<Icon>,
        content: D,
        actions: DndAction,
    ) {
        self.raw.start_dnd(
            internal,
            source_surface,
            icon_surface,
            content,
            actions,
        );
    }

    fn end_dnd(&self) {
        self.raw.end_dnd();
    }

    fn register_dnd_destination(
        &self,
        surface: DndSurface,
        rectangles: Vec<DndDestinationRectangle>,
    ) {
        self.raw.register_dnd_destination(surface, rectangles);
    }

    fn set_action(&self, action: DndAction) {
        self.raw.set_action(action);
    }

    fn peek_offer<D: AllowedMimeTypes + 'static>(
        &self,
        mime_type: Option<Cow<'static, str>>,
    ) -> std::io::Result<D> {
        self.raw.peek_offer::<D>(mime_type)
    }
}
