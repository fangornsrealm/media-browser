use rand::Rng;
use regex::Regex;
use std::ffi::OsStr;
use std::fs;
use std::iter;
use std::path::PathBuf;
use walkdir::WalkDir;

const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

fn generate_random_string(len: usize) -> String {
    let mut rng = rand::thread_rng();
    let one_char = || CHARSET[rng.gen_range(0..CHARSET.len())] as char;
    iter::repeat_with(one_char).take(len).collect()
}

pub fn osstring_to_string(input: &OsStr) -> String {
    let ret = input.to_str();
    if ret.is_none() {
        return String::new();
    }
    let s = ret.unwrap();
    s.to_string()
}

pub fn walk_dir_images(dirpath: &str) -> Vec<PathBuf> {
    let mut p = Vec::new();
    let ret = fs::read_dir(dirpath);
    if ret.is_err() {
        return p;
    }
    let dir = ret.unwrap();
    for f in dir {
        if f.is_ok() {
            let file = f.unwrap();
            let filepathopt = file.path().canonicalize();
            if filepathopt.is_err() {
                log::error!(
                    "Failed to make an absolute path for {}",
                    file.path().display()
                );
                return p;
            }
            let filepath = filepathopt.unwrap();
            if filepath.is_file() {
                let e = filepath.extension();
                if e.is_some() {
                    let ext = e.unwrap();
                    let extstring = osstring_to_string(ext).to_ascii_lowercase();
                    if extstring == "png"
                        || extstring == "jpg"
                        || extstring == "jpeg"
                        || extstring == "bmp"
                        || extstring == "gif"
                        || extstring == "webp"
                        || extstring == "tif"
                        || extstring == "tiff"
                    {
                        p.push(filepath);
                    }
                }
            }
        }
    }
    p
}

pub fn walk_tree_images(dirpath: &str) -> Vec<PathBuf> {
    let mut p = Vec::new();
    for entry in WalkDir::new(dirpath)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let filepathopt = entry.path().canonicalize();
        if filepathopt.is_err() {
            log::error!(
                "Failed to make an absolute path for {}",
                entry.path().display()
            );
            return p;
        }
        let filepath = filepathopt.unwrap();
        if filepath.is_file() {
            let e = filepath.extension();
            if e.is_some() {
                let ext = e.unwrap();
                let extstring = osstring_to_string(ext).to_ascii_lowercase();
                if extstring == "png"
                    || extstring == "jpg"
                    || extstring == "jpeg"
                    || extstring == "bmp"
                    || extstring == "gif"
                    || extstring == "webp"
                    || extstring == "tif"
                    || extstring == "tiff"
                {
                    p.push(filepath.into());
                }
            }
        }
    }
    p
}

pub fn walk_dir_videos(dirpath: &str) -> Vec<PathBuf> {
    let mut p = Vec::new();
    let ret = fs::read_dir(dirpath);
    if ret.is_err() {
        return p;
    }
    let dir = ret.unwrap();
    for f in dir {
        if f.is_ok() {
            let file = f.unwrap();
            let filepathopt = file.path().canonicalize();
            if filepathopt.is_err() {
                log::error!(
                    "Failed to make an absolute path for {}",
                    file.path().display()
                );
                return p;
            }
            let filepath = filepathopt.unwrap();
            if filepath.is_file() {
                let e = filepath.extension();
                if e.is_some() {
                    let ext = e.unwrap();
                    let extstring = osstring_to_string(ext).to_ascii_lowercase();
                    if extstring == "mkv"
                        || extstring == "mp4"
                        || extstring == "avi"
                        || extstring == "mov"
                        || extstring == "webm"
                    {
                        p.push(filepath);
                    }
                }
            }
        }
    }
    p
}

pub fn walk_tree_videos(dirpath: &str) -> Vec<PathBuf> {
    let mut p = Vec::new();
    for entry in WalkDir::new(dirpath)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let filepathopt = entry.path().canonicalize();
        if filepathopt.is_err() {
            log::error!(
                "Failed to make an absolute path for {}",
                entry.path().display()
            );
            return p;
        }
        let filepath = filepathopt.unwrap();
        if filepath.is_file() {
            let e = filepath.extension();
            if e.is_some() {
                let ext = e.unwrap();
                let extstring = osstring_to_string(ext).to_ascii_lowercase();
                if extstring == "mkv"
                    || extstring == "mp4"
                    || extstring == "avi"
                    || extstring == "mov"
                    || extstring == "webm"
                {
                    p.push(filepath.into());
                }
            }
        }
    }
    p
}

/// reads an image, creates a hash and compares it with the existing hashes.
/// Delivers existing Matches and the hash back to the calling program.
pub fn process_image(p: &PathBuf) -> crate::hash::Hash {
    let res = image::ImageReader::open(p);
    if res.is_ok() {
        let dynimg = res.unwrap().decode();
        if dynimg.is_err() {
            return crate::hash::Hash::new();
        }
        let img = dynimg.unwrap();
        let (hash, _smallimg) = crate::hash::create_hash(&img.into());
        return hash;
    }
    crate::hash::Hash::new()
}

pub fn find_similar_images(
    store: &crate::store::Store,
    id: &str,
    hash: &crate::hash::Hash,
) -> (crate::matches::Matches, String, crate::hash::Hash) {
    let matches = store.query(&hash);
    (matches, id.to_string(), hash.clone())
}

/// presents the two files together renamed to have the same prefix
/// The file containing KEEP is a hard link. You can delete it without removing the original.
/// The file containing REMOVE is the original file. If you delete it, it will be gone!
pub fn present_pairs(
    destination_dir: &std::path::PathBuf,
    remove_candidate: &str,
    keep_candidate: &str,
) {
    if !destination_dir.is_dir() {
        let ret = std::fs::create_dir(destination_dir.as_path());
        if ret.is_err() {
            log::error!(
                "Unable to create the duplicates directory in {}!",
                destination_dir.display()
            );
            return;
        }
    }
    let prefix = generate_random_string(5);
    let removefile_opt = std::path::Path::new(remove_candidate).file_name();
    let keepfile_opt = std::path::Path::new(keep_candidate).file_name();
    if removefile_opt.is_none() {
        log::error!(
            "Path {} does not contain a valid file name!",
            remove_candidate
        );
        return;
    }
    if keepfile_opt.is_none() {
        log::error!(
            "Path {} does not contain a valid file name!",
            keep_candidate
        );
        return;
    }
    let removefile_opt = removefile_opt.unwrap().to_str();
    let keepfile_opt = keepfile_opt.unwrap().to_str();
    if removefile_opt.is_none() {
        log::error!("Path {} does contain illegal characters!", remove_candidate);
        return;
    }
    if keepfile_opt.is_none() {
        log::error!("Path {} does contain illegal characters!", keep_candidate);
        return;
    }
    let removefile = removefile_opt.unwrap();
    let keepfile = keepfile_opt.unwrap();
    let remove_path = destination_dir.join(format!("{}_{}_{}", prefix, "REMOVE", removefile));
    let keep_path = destination_dir.join(format!("{}_{}_{}", prefix, "KEEP", keepfile));
    let ret = std::fs::hard_link(keep_candidate, keep_path.clone());
    if ret.is_err() {
        log::error!(
            "Failed to create a hard link from {} to {}!",
            keep_candidate,
            keep_path.display()
        );
        return;
    }
    let ret = std::fs::rename(remove_candidate, remove_path.clone());
    if ret.is_err() {
        log::error!(
            "Failed to move a file from {} to {}!",
            remove_candidate,
            remove_path.display()
        );
        return;
    }
}

fn prepare_video_table(compare: &Vec<crate::videocandidate::VideoCandidate>) -> String {
    let mut v =
        String::from("<table><thead>\n<tr><th>Video</th><th>Info</th></tr>\n</thead><tbody>\n");
    for i in 0..compare.len() {
        let filepath = std::path::Path::new(&compare[i].id);
        let fileformat;
        let e = filepath.extension();
        if e.is_some() {
            let ext = e.unwrap();
            let extstring = osstring_to_string(ext).to_ascii_lowercase();
            if extstring == "mkv" || extstring == "webm" {
                v = format!(
                    r##"{}<tr><td><video controls width="320" src="{}"</video></td>"##,
                    v, compare[i].id
                );
            } else if extstring == "mp4" || extstring == "mpv" {
                fileformat = "mp4".to_string();
                v = format!(
                    r##"{}<tr><td><video controls width="320"><source src="{}" type="video/{}" />/td>"##,
                    v, compare[i].id, fileformat
                );
            } else {
                v = format!(
                    r##"{}<tr><td><video controls width="320" src="{}"</video></td>"##,
                    v, compare[i].id
                );
            }

            v = format!(
                r##"{}<td><a href="{}">{}</a><p>Resolution: {}x{}</p><p>Duration: {}</p></td></tr>"##,
                v,
                compare[i].id,
                compare[i].id,
                compare[i].width,
                compare[i].height,
                compare[i].runtime
            );
        }
        v = format!("{}\n", v);
    }
    v = format!("{}\n</tbody></table>", v);
    v
}

pub fn present_video_matches(
    destination_dir: &std::path::PathBuf,
    compare: &Vec<crate::videocandidate::VideoCandidate>,
) {
    use build_html::*;
    use std::io::Write;
    if compare.len() < 2 {
        return;
    }
    let basefile_opt = std::path::Path::new(&compare[0].id).file_stem();
    if basefile_opt.is_none() {
        return;
    }
    if !destination_dir.is_dir() {
        let ret = std::fs::create_dir(destination_dir.as_path());
        if ret.is_err() {
            log::error!(
                "Unable to create the video matches directory in {}!",
                destination_dir.display()
            );
            return;
        }
    }

    let basefile = osstring_to_string(&basefile_opt.unwrap());
    let title = format!("Comparing similar videos for {}", basefile);
    let table = prepare_video_table(compare);
    let html = build_html::HtmlPage::new()
        .with_title(&title)
        .with_header(1, &title)
        .with_html(
            HtmlElement::new(HtmlTag::Table)
                .with_attribute("id", "Matches")
                .with_paragraph(&table),
        )
        .to_html_string();
    let storefile = destination_dir.join(format!("{}.html", basefile));
    let path = std::path::Path::new(&storefile);
    let display = path.display();
    // Open a file in write-only mode, returns `io::Result<File>`
    let write_file = match std::fs::File::create(&path) {
        Err(why) => {
            log::error!("couldn't create {}: {}", display, why);
            return;
        }
        Ok(f) => f,
    };
    let mut write_file = write_file;
    let ret = write_file.write(&html.as_bytes());
    if ret.is_err() {
        log::error!("failed to write html data to file {}!", storefile.display())
    }
    let ret = write_file.flush();
    if ret.is_err() {
        log::error!(
            "failed to empty the write buffer for html data to file {}!",
            &storefile.display()
        )
    }
}

struct VideoMetadata {
    duration: u32,
    width: u32,
    height: u32,
    framerate: f32,
}

impl Default for VideoMetadata {
    fn default() -> VideoMetadata {
        VideoMetadata {
            duration: 0,
            width: 0,
            height: 0,
            framerate: 0.0,
        }
    }
}

pub fn string_to_uint(mystring: &str) -> u32 {
    let u = 0;
    if mystring.trim().len() == 0 {
        return u;
    }
    match u32::from_str_radix(mystring, 10) {
        Ok(ret) => return ret,
        Err(_) => {
            log::warn!("Parsing of {} into Integer failed\n", mystring)
        }
    }
    u
}

pub fn string_to_float(mystring: &str) -> f32 {
    let f = 0.0;
    if mystring.trim().len() == 0 {
        return f;
    }
    match mystring.parse::<f32>() {
        Ok(ret) => return ret,
        Err(_) => {
            log::warn!("Parsing of {} into flaot failed\n", mystring)
        }
    }
    f
}

/// Probe a video file for duration, resolution, and frame rate using ffmpeg's
/// stderr output. Returns an error only if ffmpeg itself cannot be spawned
/// (e.g. binary missing); a failed probe (corrupt file, etc.) yields
/// `Default` metadata.
fn video_metadata(filepath: &str) -> Result<VideoMetadata, Box<dyn std::error::Error>> {
    let mut meta = VideoMetadata {
        ..Default::default()
    };
    let ffmpeg_output = std::process::Command::new("ffmpeg")
        .args(["-i", filepath])
        .output()
        .map_err(|e| format!("failed to execute ffmpeg (is it installed?): {}", e))?;
    let stderr = String::from_utf8_lossy(&ffmpeg_output.stderr).to_string();
    if let Ok(re_duration) =
        Regex::new(r"(?i)\s*Duration:\s+(?P<hours>\d+):(?P<minutes>\d+):(?P<seconds>\d+).\d+\s*,")
    {
        if let Some(caps) = re_duration.captures(&stderr) {
            let hours = string_to_uint(&caps["hours"]);
            let minutes = string_to_uint(&caps["minutes"]);
            let seconds = string_to_uint(&caps["seconds"]);
            meta.duration = hours * 3600 + minutes * 60 + seconds;
        }
    }
    if let Ok(re_video) =
        Regex::new(r"(?i), (?P<width>\d+)x(?P<height>\d+).*, (?P<fps>\d+(?:\.\d+)?) fps")
    {
        if let Some(caps) = re_video.captures(&stderr) {
            meta.width = string_to_uint(&caps["width"]);
            meta.height = string_to_uint(&caps["height"]);
            meta.framerate = string_to_float(&caps["fps"]);
        }
    }
    Ok(meta)
}

/// Per-frame fallback: extract one frame at each timecode using separate
/// `ffmpeg -ss` invocations.  This is the older approach that handles
/// codecs/containers the single-pass `fps` filter cannot (e.g. certain
/// HEVC/MKV combinations on ffmpeg builds without a matching decoder).
///
/// Returns the paths of successfully extracted JPEGs, in timecode order.
/// Stops early if a frame fails to extract (the stream most likely ended
/// before the announced runtime, or the codec is truly unsupported).
fn extract_per_frame(
    filepath: &str,
    tmp_dir: &std::path::Path,
    meta: &VideoMetadata,
    num_seconds_between_screenshots: u32,
) -> Vec<std::path::PathBuf> {
    if num_seconds_between_screenshots == 0 {
        return Vec::new();
    }

    // Derive the upper bound from the probed duration.  If the probe failed
    // (duration == 0, e.g. corrupt metadata) use a generous default so we
    // still sample the stream.
    let duration = if meta.duration > 0 {
        meta.duration
    } else {
        600 // 10 minutes — safe upper bound for unknown-length streams
    };

    let mut frames: Vec<std::path::PathBuf> = Vec::new();
    let mut timecode = num_seconds_between_screenshots;
    let mut frame_num: u32 = 1;

    while timecode < duration {
        let output = format!(
            "{}/shot_{:05}.jpeg",
            tmp_dir.to_str().unwrap_or_default(),
            frame_num
        );

        let result = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-ss",
                &timecode.to_string(),
                "-i",
                filepath,
                "-frames:v",
                "1",
                "-vf",
                "scale=128:-2",
                "-q:v",
                "2",
                &output,
            ])
            .output();

        match result {
            Ok(ref o) if o.status.success() => {
                let path = std::path::PathBuf::from(&output);
                if path.is_file() {
                    frames.push(path);
                }
            }
            Ok(ref o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                log::warn!(
                    "Per-frame extraction at t={}s failed ({}): {}",
                    timecode,
                    filepath,
                    stderr.trim()
                );
                // The stream most likely ended or the codec is unsupported.
                // Continuing would just produce more failures.
                break;
            }
            Err(e) => {
                log::warn!(
                    "Failed to spawn ffmpeg for per-frame extraction at t={}s: {}",
                    timecode,
                    e
                );
                break;
            }
        }

        timecode += num_seconds_between_screenshots;
        frame_num += 1;
    }

    frames
}

/// Extract one screenshot every `num_seconds_between_screenshots` seconds from
/// a video file.  Prefers a **single** ffmpeg decode pass for speed; if that
/// produces no frames (unsupported codec, corrupt stream, etc.) falls back to
/// the per-frame `ffmpeg -ss` extraction approach.
///
/// The `scale=128:-2` filter down-scales inside ffmpeg so we never pay the
/// cost of JPEG-decoding a full-resolution frame in Rust.
fn create_screenshots(
    filepath: &str,
    video_id: u32,
    num_videos: u32,
    num_seconds_between_screenshots: u32,
    id_in_files_to_process: u32,
) -> Result<(Vec<crate::videocandidate::Screenshot>, VideoMetadata), Box<dyn std::error::Error>> {
    let mut meta = video_metadata(filepath)?;

    // Use a unique temp dir so parallel workers don't clobber each other.
    let tmp_dir =
        std::env::temp_dir().join(format!("duplo_vid_{}_{}", video_id, id_in_files_to_process));
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| format!("failed to create temp dir {}: {}", tmp_dir.display(), e))?;

    // Single-pass ffmpeg: sequential decode, sample 1 frame per N seconds,
    // downscale to 128 px wide (height auto, divisible by 2).
    let vf = format!("fps=1/{},scale=128:-2", num_seconds_between_screenshots);
    let output_pattern = format!(
        "{}/shot_%05d.jpeg",
        tmp_dir.to_str().ok_or("invalid temp dir path")?
    );

    let _ffmpeg_output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            filepath,
            "-vf",
            &vf,
            "-q:v",
            "2",
            &output_pattern,
        ])
        .output()
        .map_err(|e| {
            std::fs::remove_dir_all(&tmp_dir).ok();
            format!("failed to execute ffmpeg: {}", e)
        })?;

    // Collect frames produced by the single-pass decode (zero-padded by
    // ffmpeg).  A non-zero exit or an unreadable dir simply yields an empty
    // list, which triggers the per-frame fallback below.
    let mut frames: Vec<std::path::PathBuf> = match std::fs::read_dir(&tmp_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "jpeg"))
            .collect(),
        Err(e) => {
            log::warn!(
                "Failed to read temp dir {} for video {} of {} ({}): {}",
                tmp_dir.display(),
                id_in_files_to_process,
                num_videos,
                filepath,
                e
            );
            Vec::new()
        }
    };
    frames.sort();

    // If the single-pass produced no usable frames, fall back to the
    // per-frame seek-and-extract approach.  The old per-frame routine was
    // confirmed to parse files that the single-pass `fps` filter cannot
    // handle (e.g. certain HEVC/MKV combinations on older ffmpeg builds).
    if frames.is_empty() {
        log::warn!(
            "Single-pass ffmpeg produced 0 frames for video {} of {} ({}); \
             falling back to per-frame extraction",
            id_in_files_to_process,
            num_videos,
            filepath
        );
        frames = extract_per_frame(filepath, &tmp_dir, &meta, num_seconds_between_screenshots);
        frames.sort();
    }

    if frames.is_empty() {
        // Both approaches failed — the file is truly unreadable.
        log::error!(
            "No screenshots could be extracted for video {} of {} ({})",
            id_in_files_to_process,
            num_videos,
            filepath
        );
        std::fs::remove_dir_all(&tmp_dir).ok();
        return Ok((Vec::new(), meta));
    }

    log::warn!(
        "Created {} screenshot(s) for video {} of {} ({})",
        frames.len(),
        id_in_files_to_process,
        num_videos,
        filepath
    );

    // Decode + hash each frame.  Individual decode failures are logged and
    // skipped — a corrupt JPEG should not abort the whole video.
    let mut v = Vec::new();
    let mut screenshot_id: usize = 1;
    for frame_path in &frames {
        let timecode = (screenshot_id as u32) * num_seconds_between_screenshots;
        match image::ImageReader::open(frame_path) {
            Ok(r) => match r.decode() {
                Ok(img) => {
                    let rgba: image::RgbaImage = img.into();
                    let (hash, _smallimg) = crate::hash::create_hash(&rgba);
                    let ss = crate::videocandidate::Screenshot::from(
                        filepath,
                        video_id as usize,
                        screenshot_id,
                        timecode,
                        &hash,
                    );
                    v.push(ss);
                }
                Err(e) => {
                    log::warn!(
                        "Failed to decode screenshot {}: {}",
                        frame_path.display(),
                        e
                    );
                }
            },
            Err(e) => {
                log::warn!("Failed to open screenshot {}: {}", frame_path.display(), e);
            }
        }
        screenshot_id += 1;
    }

    // Correct the runtime if the probe returned 0 (corrupt/missing metadata)
    // but we successfully extracted frames.
    if meta.duration == 0 && !v.is_empty() {
        meta.duration =
            v.len() as u32 * num_seconds_between_screenshots + num_seconds_between_screenshots / 2;
    }

    // Clean up the temp directory regardless of success/failure.
    std::fs::remove_dir_all(&tmp_dir).ok();

    Ok((v, meta))
}

/// reads a video file, creating screenshots every 10 seconds,
/// creates a hash for each screenshot
/// and compares it with the hashes of screenshots of existing videos.
/// Delivers existing Matches and the new data structure back to the calling program.
pub fn process_video(
    path: &PathBuf,
    video_id: usize,
    num_videos: u32,
    num_seconds_between_screenshots: u32,
    id_in_files_to_process: u32,
) -> crate::videocandidate::VideoCandidate {
    let id = osstring_to_string(path.as_os_str());
    let mut video = crate::videocandidate::VideoCandidate::from(&id, video_id);

    match create_screenshots(
        &id,
        video.index,
        num_videos,
        num_seconds_between_screenshots,
        id_in_files_to_process,
    ) {
        Ok((v, meta)) => {
            video.width = meta.width;
            video.height = meta.height;
            video.runtime = meta.duration;
            video.framerate = meta.framerate;
            for ss in v {
                video.screenshots.push(ss);
            }
            // correct the runtime if the last screenshots could not be read ( possibly defective video )
            let screentime = video.screenshots.len() as u32 * num_seconds_between_screenshots;
            if video.runtime.saturating_sub(screentime) > 10 {
                video.runtime = screentime + 5;
            }
        }
        Err(error) => log::error!(
            "Failed to create screenshots for {}: {}",
            path.display(),
            error
        ),
    }
    video
}

pub fn find_similar_videos(
    store: &crate::videostore::VideoStore,
    id: &str,
    video: &crate::videocandidate::VideoCandidate,
) -> (
    crate::videomatches::VideoMatches,
    String,
    crate::videocandidate::VideoCandidate,
) {
    let matches = store.query(video);
    (matches, id.to_string(), video.clone())
}
