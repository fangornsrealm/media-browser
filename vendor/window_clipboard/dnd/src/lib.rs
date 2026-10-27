use std::{
    borrow::Cow,
    fmt::Debug,
    sync::{mpsc::SendError, Arc},
};

use bitflags::bitflags;
use raw_window_handle::HasWindowHandle;

#[cfg(all(
    unix,
    not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        target_os = "emscripten",
        target_os = "redox"
    ))
))]
#[path = "platform/linux.rs"]
pub mod platform;

bitflags! {
    // Attributes can be applied to flags types
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct DndAction: u32 {
        const Copy = 0b00000001;
        const Move = 0b00000010;
        const Ask = 0b00000100;
    }
}

#[derive(Debug, Clone)]
pub enum DndEvent<T> {
    /// Dnd Offer event with the corresponding destination rectangle ID.
    Offer(Option<u128>, OfferEvent<T>),
    /// Dnd Source event.
    Source(SourceEvent),
}

impl<T> PartialEq for DndEvent<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DndEvent::Offer(a, b), DndEvent::Offer(a2, b2)) => {
                a == a2 && b == b2
            }
            (DndEvent::Source(a), DndEvent::Source(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SourceEvent {
    /// DnD operation ended.
    Finished,
    /// DnD Cancelled.
    Cancelled,
    /// DnD action chosen by the compositor.
    Action(DndAction),
    /// Mime accepted by destination.
    /// If [`None`], no mime types are accepted.
    Mime(Option<String>),
    /// DnD Dropped. The operation is still ongoing until receiving a
    /// [`SourceEvent::Finished`] event.
    Dropped,
}

impl PartialEq for SourceEvent {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (SourceEvent::Finished, SourceEvent::Finished)
            | (SourceEvent::Cancelled, SourceEvent::Cancelled)
            | (SourceEvent::Dropped, SourceEvent::Dropped) => true,
            (SourceEvent::Action(a), SourceEvent::Action(b)) => a == b,
            (SourceEvent::Mime(a), SourceEvent::Mime(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum OfferEvent<T> {
    Enter {
        x: f64,
        y: f64,
        mime_types: Vec<String>,
        surface: T,
    },
    Motion {
        x: f64,
        y: f64,
    },
    /// The offer is no longer on a DnD destination.
    LeaveDestination,
    /// The offer has left the surface.
    Leave,
    /// An offer was dropped
    Drop,
    /// If the selected action is ASK, the user must be presented with a
    /// choice. [`Clipboard::set_action`] should then be called before data
    /// can be requested and th DnD operation can be finished.
    SelectedAction(DndAction),
    Data {
        data: Vec<u8>,
        mime_type: String,
    },
}

impl<T> PartialEq for OfferEvent<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                OfferEvent::Enter {
                    x,
                    y,
                    mime_types,
                    surface: _,
                },
                OfferEvent::Enter {
                    x: x2,
                    y: y2,
                    mime_types: mime_types2,
                    surface: _,
                },
            ) => x == x2 && y == y2 && mime_types == mime_types2,
            (
                OfferEvent::Motion { x, y },
                OfferEvent::Motion { x: x2, y: y2 },
            ) => x == x2 && y == y2,
            (OfferEvent::LeaveDestination, OfferEvent::LeaveDestination)
            | (OfferEvent::Leave, OfferEvent::Leave)
            | (OfferEvent::Drop, OfferEvent::Drop) => true,
            (OfferEvent::SelectedAction(a), OfferEvent::SelectedAction(b)) => {
                a == b
            }
            (
                OfferEvent::Data { data, mime_type },
                OfferEvent::Data {
                    data: data2,
                    mime_type: mime_type2,
                },
            ) => data == data2 && mime_type == mime_type2,
            _ => false,
        }
    }
}

/// A rectangle with a logical location and size relative to a [`DndSurface`]
#[derive(Debug, Default, Clone)]
pub struct Rectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub trait Sender<T> {
    /// Send an event in the channel
    fn send(&self, t: DndEvent<T>) -> Result<(), SendError<DndEvent<T>>>;
}

/// A rectangle with a logical location and size relative to a [`DndSurface`]
#[derive(Debug, Clone)]
pub struct DndDestinationRectangle {
    /// A unique ID
    pub id: u128,
    /// The rectangle representing this destination.
    pub rectangle: Rectangle,
    /// Accepted mime types in this rectangle
    pub mime_types: Vec<Cow<'static, str>>,
    /// Accepted actions in this rectangle
    pub actions: DndAction,
    /// Prefered action in this rectangle
    pub preferred: DndAction,
}

#[derive(Clone)]
pub enum Icon {
    Surface(DndSurface),
    /// Xrgb8888 or Argb8888 image data with premultiplied alpha
    Buffer {
        data: Arc<Vec<u8>>,
        width: u32,
        height: u32,
        transparent: bool,
    },
}

#[derive(Clone)]
pub struct DndSurface(pub Arc<dyn HasWindowHandle + 'static + Send + Sync>);

impl Debug for DndSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DndSurface").finish()
    }
}

#[derive(Clone)]
pub struct DataWrapper<T>(pub T);
