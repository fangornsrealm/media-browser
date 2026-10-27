use std::{borrow::Cow, ffi::c_void, sync::Arc};

use crate::{DataWrapper, DndAction, DndSurface, Icon};
use raw_window_handle::HasWindowHandle;
use smithay_clipboard::mime::{AllowedMimeTypes, AsMimeTypes, MimeType};

impl<
        T: mime::AllowedMimeTypes
            + std::convert::TryFrom<(std::vec::Vec<u8>, String)>,
    > AllowedMimeTypes for DataWrapper<T>
{
    fn allowed() -> Cow<'static, [MimeType]> {
        T::allowed()
            .into_iter()
            .map(|s| MimeType::from(Cow::Owned(s.to_string())))
            .collect()
    }
}

impl<T: TryFrom<(Vec<u8>, String)>> TryFrom<(Vec<u8>, MimeType)>
    for DataWrapper<T>
{
    type Error = T::Error;

    fn try_from(
        (data, mime): (Vec<u8>, MimeType),
    ) -> Result<Self, Self::Error> {
        T::try_from((data, mime.to_string())).map(|d| DataWrapper(d))
    }
}

impl<T: mime::AsMimeTypes> AsMimeTypes for DataWrapper<T> {
    fn available(&self) -> Cow<'static, [MimeType]> {
        self.0
            .available()
            .into_iter()
            .map(|m| MimeType::from(Cow::Owned(m.to_string())))
            .collect()
    }

    fn as_bytes(&self, mime_type: &MimeType) -> Option<Cow<'static, [u8]>> {
        self.0.as_bytes(mime_type.as_ref())
    }
}

impl smithay_clipboard::dnd::RawSurface for DndSurface {
    unsafe fn get_ptr(&mut self) -> *mut c_void {
        self.0.window_handle().unwrap().get_ptr()
    }
}

impl From<sctk::reexports::client::protocol::wl_data_device_manager::DndAction>
    for DndAction
{
    fn from(
        action: sctk::reexports::client::protocol::wl_data_device_manager::DndAction,
    ) -> Self {
        let mut a = DndAction::empty();
        if action.contains(sctk::reexports::client::protocol::wl_data_device_manager::DndAction::Copy) {
        a |= DndAction::Copy;
    }
        if action.contains(sctk::reexports::client::protocol::wl_data_device_manager::DndAction::Move) {
        a |= DndAction::Move;
    }
        if action.contains(sctk::reexports::client::protocol::wl_data_device_manager::DndAction::Ask) {
        a |= DndAction::Ask;
    }
        a
    }
}

impl From<DndAction>
    for sctk::reexports::client::protocol::wl_data_device_manager::DndAction
{
    fn from(action: DndAction) -> Self {
        let mut a = sctk::reexports::client::protocol::wl_data_device_manager::DndAction::empty();
        if action.contains(DndAction::Copy) {
            a |= sctk::reexports::client::protocol::wl_data_device_manager::DndAction::Copy;
        }
        if action.contains(DndAction::Move) {
            a |= sctk::reexports::client::protocol::wl_data_device_manager::DndAction::Move;
        }
        if action.contains(DndAction::Ask) {
            a |= sctk::reexports::client::protocol::wl_data_device_manager::DndAction::Ask;
        }
        a
    }
}

impl From<Icon> for smithay_clipboard::dnd::Icon<DndSurface> {
    fn from(icon: Icon) -> Self {
        match icon {
            Icon::Surface(surface) => {
                smithay_clipboard::dnd::Icon::Surface(surface)
            }
            Icon::Buffer {
                data,
                width,
                height,
                transparent,
            } => smithay_clipboard::dnd::Icon::Buf {
                data: Arc::try_unwrap(data)
                    .unwrap_or_else(|d| d.as_ref().clone()),
                width,
                height,
                transparent,
            },
        }
    }
}
