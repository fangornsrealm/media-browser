//! Cover-art decoding for the audio view.
//!
//! Decodes a poster image (or the bundled dummy) to a flat BGRA8 buffer
//! that the [`crate::pipeline`] blits through the wgpu renderer — the
//! same byte order as mpv's software-rendered video frames (the pipeline
//! probes the device once and compensates for it in the shader).

/// Resolve `audio_path` (a plain path or a `file://` URL) to a path on
/// disk. `None` for non-file URLs that cannot be mapped to the filesystem.
pub fn audio_file_path(audio_path: &str) -> Option<std::path::PathBuf> {
    match url::Url::parse(audio_path) {
        Ok(url) if url.scheme() == "file" => url.to_file_path().ok(),
        _ => Some(std::path::PathBuf::from(audio_path)),
    }
}

/// The path in the same directory as `audio_path`, sharing its stem up to
/// the last dot and carrying `ext`: `track.v2.mp3` + `"lrc"` ->
/// `track.v2.lrc`. (Not `Path::with_extension`, which on current rustc
/// treats everything after the *first* dot as the extension and would
/// mangle dotted stems.) `None` if the file has no extension or no parent.
/// `audio_path` may be a plain path or a `file://` URL.
pub fn same_stem_with_ext(audio_path: &str, ext: &str) -> Option<std::path::PathBuf> {
    let audio = audio_file_path(audio_path)?;
    let name = audio.file_name()?.to_string_lossy().into_owned();
    let (stem, _) = name.rsplit_once('.')?;
    let parent = audio
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    Some(parent.join(format!("{stem}.{ext}")))
}

/// Find a cover image next to the audio file sharing its stem —
/// `track.mp3` -> `track.png`, else `track.jpg`, else `track.jpeg` (the
/// first existing file wins). `audio_path` may be a plain path or a
/// `file://` URL.
pub fn cover_next_to(audio_path: &str) -> Option<String> {
    for ext in ["png", "jpg", "jpeg"] {
        let Some(candidate) = same_stem_with_ext(audio_path, ext) else {
            continue;
        };
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

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

        let (bytes, width, height) = cover_from_image(Some(path.to_string_lossy().to_string()));
        assert_eq!((width, height), (1, 1));
        assert_eq!(
            bytes,
            vec![0u8, 0, 255, 255],
            "a pure red pixel must be stored as BGRA"
        );
    }

    #[test]
    fn cover_next_to_finds_stem_image() {
        let dir = tempfile::tempdir().expect("tempdir");
        let audio = dir.path().join("track.mp3");
        std::fs::write(&audio, b"fake").expect("write audio");
        let img = dir.path().join("track.png");
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
            .save(&img)
            .expect("save png");

        let expected = img.to_string_lossy().to_string();
        // Plain path.
        assert_eq!(
            cover_next_to(&audio.to_string_lossy()),
            Some(expected.clone())
        );
        // `file://` URL (what the audio view passes).
        let url = url::Url::from_file_path(&audio).expect("file url");
        assert_eq!(cover_next_to(&url.to_string()), Some(expected));
        // No adjacent image.
        let other = dir.path().join("other.mp3");
        std::fs::write(&other, b"fake").expect("write audio");
        assert!(cover_next_to(&other.to_string_lossy()).is_none());
    }

    #[test]
    fn cover_next_to_jpg_fallback_and_dotted_stem() {
        let dir = tempfile::tempdir().expect("tempdir");
        let audio = dir.path().join("track.v2.mp3");
        std::fs::write(&audio, b"fake").expect("write audio");
        let img = dir.path().join("track.v2.jpg");
        image::RgbImage::from_pixel(1, 1, image::Rgb([0, 255, 0]))
            .save(&img)
            .expect("save jpg");

        // Must not match `track.v2.mp3.jpg`-style suffixes; the stem keeps
        // its inner dot.
        assert_eq!(
            cover_next_to(&audio.to_string_lossy()),
            Some(img.to_string_lossy().to_string())
        );
    }
}
