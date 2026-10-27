// Copyright 2017 Avraham Weinstock
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    borrow::Cow,
    error::Error,
    ffi::c_void,
    sync::{mpsc::SendError, Arc, Mutex},
};

use dnd::{
    DataWrapper, DndAction, DndDestinationRectangle, DndSurface, Sender,
};
use mime::ClipboardData;
use smithay_clipboard::dnd::{Icon, Rectangle};
pub use smithay_clipboard::mime::{AllowedMimeTypes, AsMimeTypes, MimeType};

#[derive(Clone)]
pub struct DndSender(pub Arc<dyn Sender<DndSurface> + 'static + Send + Sync>);

impl smithay_clipboard::dnd::Sender<DndSurface> for DndSender {
    fn send(
        &self,
        event: smithay_clipboard::dnd::DndEvent<DndSurface>,
    ) -> Result<(), SendError<smithay_clipboard::dnd::DndEvent<DndSurface>>>
    {
        _ = self.0.send(match event {
            smithay_clipboard::dnd::DndEvent::Offer(id, e) => dnd::DndEvent::Offer(
                id,
                match e {
                    smithay_clipboard::dnd::OfferEvent::Enter {
                        x,
                        y,
                        mime_types,
                        surface,
                    } => dnd::OfferEvent::Enter {
                        x,
                        y,
                        mime_types: mime_types
                            .into_iter()
                            .map(|m| m.to_string())
                            .collect(),
                        surface,
                    },
                    smithay_clipboard::dnd::OfferEvent::Motion { x, y } => {
                        dnd::OfferEvent::Motion { x, y }
                    }
                    smithay_clipboard::dnd::OfferEvent::LeaveDestination => {
                        dnd::OfferEvent::LeaveDestination
                    }
                    smithay_clipboard::dnd::OfferEvent::Leave => {
                        dnd::OfferEvent::Leave
                    }
                    smithay_clipboard::dnd::OfferEvent::Drop => {
                        dnd::OfferEvent::Drop
                    }
                    smithay_clipboard::dnd::OfferEvent::SelectedAction(
                        action,
                    ) => dnd::OfferEvent::SelectedAction(action.into()),
                    smithay_clipboard::dnd::OfferEvent::Data {
                        data,
                        mime_type,
                    } => dnd::OfferEvent::Data {
                        data,
                        mime_type: mime_type.to_string(),
                    },
                },
            ),
            smithay_clipboard::dnd::DndEvent::Source(e) => match e {
                smithay_clipboard::dnd::SourceEvent::Finished => {
                    dnd::DndEvent::Source(dnd::SourceEvent::Finished)
                }
                smithay_clipboard::dnd::SourceEvent::Cancelled => {
                    dnd::DndEvent::Source(dnd::SourceEvent::Cancelled)
                }
                smithay_clipboard::dnd::SourceEvent::Action(action) => {
                    dnd::DndEvent::Source(dnd::SourceEvent::Action(
                        action.into(),
                    ))
                }
                smithay_clipboard::dnd::SourceEvent::Mime(mime) => {
                    dnd::DndEvent::Source(dnd::SourceEvent::Mime(
                        mime.map(|m| m.to_string()),
                    ))
                }
                smithay_clipboard::dnd::SourceEvent::Dropped => {
                    dnd::DndEvent::Source(dnd::SourceEvent::Dropped)
                }
            },
        });
        Ok(())
    }
}

pub struct Clipboard {
    context: Arc<Mutex<smithay_clipboard::Clipboard<DndSurface>>>,
}

impl Clipboard {
    pub unsafe fn connect(display: *mut c_void) -> Clipboard {
        let context = Arc::new(Mutex::new(smithay_clipboard::Clipboard::new(
            display as *mut _,
        )));

        Clipboard { context }
    }

    pub fn read(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.context.lock().unwrap().load_text()?)
    }

    pub fn read_primary(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.context.lock().unwrap().load_primary_text()?)
    }

    pub fn write(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        self.context.lock().unwrap().store_text(data);

        Ok(())
    }

    pub fn write_primary(
        &mut self,
        data: String,
    ) -> Result<(), Box<dyn Error>> {
        self.context.lock().unwrap().store_primary_text(data);

        Ok(())
    }

    pub fn write_data<T: AsMimeTypes + Send + Sync + 'static>(
        &mut self,
        data: T,
    ) -> Result<(), Box<dyn Error>> {
        self.context.lock().unwrap().store(data);

        Ok(())
    }

    pub fn write_primary_data<T: AsMimeTypes + Send + Sync + 'static>(
        &mut self,
        data: T,
    ) -> Result<(), Box<dyn Error>> {
        self.context.lock().unwrap().store_primary(data);

        Ok(())
    }

    pub fn read_data<T: AllowedMimeTypes + 'static>(
        &self,
    ) -> Result<T, Box<dyn Error>> {
        Ok(self.context.lock().unwrap().load()?)
    }

    pub fn read_primary_data<T: AllowedMimeTypes + 'static>(
        &self,
    ) -> Result<T, Box<dyn Error>> {
        Ok(self.context.lock().unwrap().load_primary()?)
    }

    pub fn read_primary_raw(
        &self,
        allowed: Vec<String>,
    ) -> Result<(Vec<u8>, String), Box<dyn Error>> {
        Ok(self
            .context
            .lock()
            .unwrap()
            .load_primary_mime::<DataWrapper<ClipboardData>>(
                allowed
                    .into_iter()
                    .map(|s| MimeType::from(Cow::Owned(s)))
                    .collect::<Vec<_>>(),
            )
            .map(|d| (d.0 .0, d.0 .1.to_string()))?)
    }

    pub fn read_raw(
        &self,
        allowed: Vec<String>,
    ) -> Result<(Vec<u8>, String), Box<dyn Error>> {
        Ok(self
            .context
            .lock()
            .unwrap()
            .load_mime::<DataWrapper<ClipboardData>>(
                allowed
                    .into_iter()
                    .map(|s| MimeType::from(Cow::Owned(s)))
                    .collect::<Vec<_>>(),
            )
            .map(|d| (d.0 .0, d.0 .1))?)
    }

    pub fn init_dnd(&self, tx: DndSender) {
        _ = self.context.lock().unwrap().init_dnd(Box::new(tx));
    }

    /// Start a DnD operation on the given surface with some data
    pub fn start_dnd<D: mime::AsMimeTypes + Send + 'static>(
        &self,
        internal: bool,
        source_surface: DndSurface,
        icon_surface: Option<dnd::Icon>,
        content: D,
        actions: DndAction,
    ) {
        _ = self.context.lock().unwrap().start_dnd(
            internal,
            source_surface,
            icon_surface.map(|i| Icon::<DndSurface>::from(i)),
            DataWrapper(content),
            actions.into(),
        );
    }

    /// End the current DnD operation, if there is one
    pub fn end_dnd(&self) {
        _ = self.context.lock().unwrap().end_dnd();
    }

    /// Register a surface for receiving DnD offers
    /// Rectangles should be provided in order of decreasing priority.
    /// This method can be called multiple time for a single surface if the
    /// rectangles change.
    pub fn register_dnd_destination(
        &self,
        surface: DndSurface,
        rectangles: Vec<DndDestinationRectangle>,
    ) {
        _ = self.context.lock().unwrap().register_dnd_destination(
            surface,
            rectangles
                .into_iter()
                .map(|r| RectangleWrapper(r).into())
                .collect(),
        );
    }

    /// Set the final action after presenting the user with a choice
    pub fn set_action(&self, action: DndAction) {
        self.context.lock().unwrap().set_action(action.into());
    }

    /// Peek at the contents of a DnD offer
    pub fn peek_offer<D: mime::AllowedMimeTypes + 'static>(
        &self,
        mime_type: Option<Cow<'static, str>>,
    ) -> std::io::Result<D> {
        let d = self
            .context
            .lock()
            .unwrap()
            .peek_offer::<DataWrapper<D>>(mime_type.map(MimeType::from));
        d.map(|d| d.0)
    }
}

pub struct RectangleWrapper(pub DndDestinationRectangle);

impl From<RectangleWrapper>
    for smithay_clipboard::dnd::DndDestinationRectangle
{
    fn from(RectangleWrapper(d): RectangleWrapper) -> Self {
        smithay_clipboard::dnd::DndDestinationRectangle {
            id: d.id,
            rectangle: Rectangle {
                x: d.rectangle.x,
                y: d.rectangle.y,
                width: d.rectangle.width,
                height: d.rectangle.height,
            },
            mime_types: d.mime_types.into_iter().map(MimeType::from).collect(),
            actions: d.actions.into(),
            preferred: d.preferred.into(),
        }
    }
}
