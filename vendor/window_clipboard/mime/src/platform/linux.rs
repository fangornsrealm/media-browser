use smithay_clipboard::mime::{AllowedMimeTypes, AsMimeTypes, MimeType};

use crate::{ClipboardLoadData, ClipboardStoreData};

impl<T: crate::AsMimeTypes> AsMimeTypes for ClipboardStoreData<T> {
    fn available(&self) -> std::borrow::Cow<'static, [MimeType]> {
        self.0
            .available()
            .into_iter()
            .map(|m| MimeType::Other(m.clone().into()))
            .collect()
    }

    fn as_bytes(
        &self,
        mime_type: &MimeType,
    ) -> Option<std::borrow::Cow<'static, [u8]>> {
        self.0.as_bytes(mime_type.as_ref())
    }
}

impl<T: crate::AllowedMimeTypes> AllowedMimeTypes for ClipboardLoadData<T> {
    // TODO select text variants if string matches...
    fn allowed() -> std::borrow::Cow<'static, [MimeType]> {
        T::allowed()
            .into_iter()
            .map(|s| MimeType::Other(s.clone().into()))
            .collect()
    }
}

impl<T> TryFrom<(Vec<u8>, MimeType)> for ClipboardLoadData<T>
where
    T: for<'b> TryFrom<(Vec<u8>, String)>,
    T: 'static,
{
    type Error = crate::Error;

    fn try_from(
        (value, mime): (Vec<u8>, MimeType),
    ) -> Result<Self, Self::Error> {
        let mime = mime.to_string();
        Ok(ClipboardLoadData(
            T::try_from((value, mime)).map_err(|_| crate::Error)?,
        ))
    }
}
