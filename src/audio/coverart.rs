//! Cover-art decoding for the audio view.
//!
//! Decodes a poster image (or the bundled dummy) to a flat BGRA8 buffer
//! that the [`crate::pipeline`] blits through the wgpu renderer — the
//! same byte order as mpv's software-rendered video frames (the pipeline
//! probes the device once and compensates for it in the shader).

/// Decode `posterpath` (or the bundled `res/audio_dummy.jpg` fallback) into
/// a BGRA8 buffer: `(bytes, width, height)`.
///
/// `bytes.len() == width * height * 4`; stride is `width * 4`.
pub fn cover_from_image(posterpath: Option<String>) -> (Vec<u8>, u32, u32) {
    let mut v: Vec<u8> = Vec::new();
    let mut width: u32 = 0;
    let mut height: u32 = 0;

    let mut candidates: Vec<String> = Vec::new();
    if let Some(p) = posterpath {
        candidates.push(p);
    }
    candidates.push("res/audio_dummy.jpg".to_string());

    for candidate in &candidates {
        let Ok(reader) = image::ImageReader::open(candidate.as_str()) else {
            continue;
        };
        let Ok(reader) = reader.with_guessed_format() else {
            continue;
        };
        let Ok(img) = reader.decode() else {
            continue;
        };
        // BGRA, matching the pipeline's frame convention (mpv sw frames):
        // swap R and B of the decoded RGBA buffer.
        let rgba = img.to_rgba8();
        width = rgba.width();
        height = rgba.height();
        v = rgba.into_vec();
        for pixel in v.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        break;
    }

    if v.is_empty() {
        // Opaque black placeholder, 4:3.
        width = 640;
        height = 480;
        v = vec![0u8; (width as usize * height as usize * 4)];
    }

    (v, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the decoded buffer must be BGRA (the pipeline's frame
    /// convention, probed once per device). A pure-red pixel therefore
    /// decodes to `[0, 0, 255, 255]`, not the RGBA `[255, 0, 0, 255]` that
    /// used to come out red-blue swapped on screen.
    #[test]
    fn cover_decodes_to_bgra_byte_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("red.png");
        let red = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        red.save(&path).expect("save test image");

        let (bytes, width, height) =
            cover_from_image(Some(path.to_string_lossy().to_string()));
        assert_eq!((width, height), (1, 1));
        assert_eq!(
            bytes,
            vec![0u8, 0, 255, 255],
            "a pure red pixel must be stored as BGRA"
        );
    }
}
