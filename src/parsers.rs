// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use crate::config::IconSizes;
use crate::mime_app::mime_apps;
use crate::mime_icon::{mime_for_path, mime_icon};
use crate::tab::{hidden_attribute, DirSize, Item, ItemMetadata, ItemThumbnail, Location};

use chrono::{DateTime, Datelike, NaiveDate};
use cosmic::widget;
use std::cell::Cell;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

/// Read video metadata from XML files movie.nfo (XBMC/Kodi format)
fn parse_nfo(nfo_file: &PathBuf, metadata: &mut crate::sql::VideoMetadata) {
    use std::fs::File;
    use std::io::BufReader;

    use xml::reader::XmlEvent;
    let file = match File::open(nfo_file) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!("failed to read entry in {:?}: {}", nfo_file.display(), err);
            return;
        }
    };

    let mut reader = xml::ParserConfig::default()
        .ignore_root_level_whitespace(false)
        .create_reader(BufReader::new(file));

    let mut prevtag = String::new();
    let mut tag = String::new();
    let mut _level = 0;
    loop {
        match reader.next() {
            Ok(e) => match e {
                XmlEvent::StartDocument { .. } => {
                    //println!("StartDocument({version}, {encoding})");
                }
                XmlEvent::EndDocument => {
                    //println!("EndDocument");
                    break;
                }
                XmlEvent::StartElement { name, .. } => {
                    tag = name.to_string().to_ascii_lowercase();
                    match &tag as &str {
                        "actor" => {
                            prevtag = tag.clone();
                        }
                        "video" => {
                            prevtag = tag.clone();
                        }
                        "audio" => {
                            prevtag = tag.clone();
                        }
                        "subtitle" => {
                            prevtag = tag.clone();
                        }
                        _ => {}
                    }
                    _level += 1;
                    /*
                    if attributes.is_empty() {
                        println!("StartElement({name})");
                    } else {
                        let attrs: Vec<_> = attributes
                            .iter()
                            .map(|a| format!("{}={:?}", &a.name, a.value))
                            .collect();
                        println!("StartElement({name} [{}])", attrs.join(", "));
                    }*/
                }
                XmlEvent::EndElement { name } => {
                    tag = name.to_string().to_ascii_lowercase();
                    if tag == prevtag {
                        prevtag.clear();
                    }
                    //println!("EndElement({name})");
                    _level -= 1;
                }
                XmlEvent::Comment(_data) => {}
                XmlEvent::CData(_data) => {}
                XmlEvent::Characters(data) => {
                    let value = data.escape_debug().to_string();
                    match &tag as &str {
                        "title" => {
                            metadata.title = value.clone();
                        }
                        "plot" => {
                            metadata.description = value.clone();
                        }
                        "runtime" => {
                            let duration = match u32::from_str_radix(&value, 10) {
                                Ok(ok) => ok,
                                Err(err) => {
                                    log::warn!("failed to parse number {:?}: {}", value, err);
                                    continue;
                                }
                            };
                            metadata.duration = duration * 60;
                        }
                        "season" => {
                            let season = match i32::from_str_radix(&value, 10) {
                                Ok(ok) => ok,
                                Err(err) => {
                                    log::warn!("failed to parse number {:?}: {}", value, err);
                                    continue;
                                }
                            };
                            metadata.season = season;
                        }
                        "episode" => {
                            let episode = match i32::from_str_radix(&value, 10) {
                                Ok(ok) => ok,
                                Err(err) => {
                                    log::warn!("failed to parse number {:?}: {}", value, err);
                                    continue;
                                }
                            };
                            metadata.episode = episode;
                        }
                        "premiered" => {
                            let ret = NaiveDate::parse_from_str(&value, "%Y-%m-%d");
                            if ret.is_ok() {
                                metadata.date = ret.unwrap();
                            }
                        }
                        "aired" => {
                            let ret = NaiveDate::parse_from_str(&value, "%Y-%m-%d");
                            if ret.is_ok() {
                                metadata.date = ret.unwrap();
                            }
                        }
                        "director" => {
                            metadata.director.push(value.clone());
                        }
                        "name" => {
                            metadata.actors.push(value.clone());
                            prevtag.clear();
                        }
                        "width" => {
                            let val = match u32::from_str_radix(&value, 10) {
                                Ok(ok) => ok,
                                Err(err) => {
                                    log::warn!("failed to parse number {:?}: {}", value, err);
                                    continue;
                                }
                            };
                            metadata.width = val;
                        }
                        "height" => {
                            let val = match u32::from_str_radix(&value, 10) {
                                Ok(ok) => ok,
                                Err(err) => {
                                    log::warn!("failed to parse number {:?}: {}", value, err);
                                    continue;
                                }
                            };
                            metadata.height = val;
                        }
                        "durationinseconds" => {
                            let val = match u32::from_str_radix(&value, 10) {
                                Ok(ok) => ok,
                                Err(err) => {
                                    log::warn!("failed to parse number {:?}: {}", value, err);
                                    continue;
                                }
                            };
                            metadata.duration = val;
                        }
                        "language" => {
                            if &prevtag == "audio" {
                                metadata.audiolangs.push(value.clone());
                            }
                            if &prevtag == "subtitle" {
                                metadata.sublangs.push(value.clone());
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            Err(_e) => {
                break;
            }
        }
    }
}

/// Read audio metadata from supported audio formats
fn parse_audiotags(file: &PathBuf, metadata: &mut crate::sql::AudioMetadata) {
    use audiotags::{MimeType, Tag};

    // using `default()` or `new()` alone so that the metadata format is
    // guessed (from the file extension) (in this case, Id3v2 tag is read)
    let tag;
    match Tag::new().read_from_path(file) {
        Ok(ok) => tag = ok,
        Err(error) => {
            log::error!(
                "Failed to open audio file {} for reading metadata: {}",
                file.display(),
                error
            );
            return;
        }
    }
    match tag.title() {
        Some(value) => metadata.title = value.to_string(),
        None => {}
    };
    match tag.album_title() {
        Some(value) => metadata.album = value.to_string(),
        None => {}
    };
    match tag.track_number() {
        Some(value) => metadata.track_id = value.into(),
        None => {}
    };
    match tag.genre() {
        Some(value) => metadata.genre = value.to_string(),
        None => {}
    };
    match tag.composer() {
        Some(value) => metadata.composer = value.to_string(),
        None => {}
    };
    match tag.artists() {
        Some(value) => {
            for s in value {
                metadata.artist.push(s.to_string());
            }
        }
        None => {}
    };
    match tag.album_artists() {
        Some(value) => {
            for s in value {
                metadata.albumartist.push(s.to_string());
            }
        }
        None => {}
    };
    match tag.date() {
        Some(timestamp) => {
            if timestamp.month.is_some() && timestamp.day.is_some() {
                let nd = NaiveDate::from_ymd_opt(
                    timestamp.year,
                    timestamp.month.unwrap() as u32,
                    timestamp.day.unwrap() as u32,
                );
                if nd.is_some() {
                    metadata.date = nd.unwrap();
                }
            } else {
                let nd = NaiveDate::from_ymd_opt(timestamp.year, 1, 1);
                if nd.is_some() {
                    metadata.date = nd.unwrap();
                }
            }
        }
        None => {}
    };
    match tag.duration() {
        Some(value) => metadata.duration = value as u32,
        None => {}
    };
    match tag.album_cover() {
        Some(picture) => {
            let max_size = 256;
            let thumbpath = album_path(&file);
            let thumbstring = osstr_to_string(thumbpath.clone().into_os_string());
            if !PathBuf::from(&thumbpath).is_file() {
                match picture.mime_type {
                    MimeType::Jpeg => {
                        match image::load_from_memory_with_format(
                            picture.data,
                            image::ImageFormat::Jpeg,
                        ) {
                            Ok(image) => {
                                let nwidth;
                                let nheight;
                                if image.width() > image.height() {
                                    nwidth = max_size;
                                    nheight = nwidth * image.height() / image.width();
                                } else {
                                    nheight = max_size;
                                    nwidth = nheight * image.width() / image.height();
                                }
                                let thumb = image::imageops::resize(
                                    &image,
                                    nwidth,
                                    nheight,
                                    image::imageops::FilterType::Lanczos3,
                                );
                                let ret = thumb.save(thumbstring.clone());
                                if ret.is_err() {
                                    log::error!(
                                        "Failed to create thumbnail for file {}!",
                                        file.display()
                                    );
                                }
                            }
                            Err(error) => {
                                log::warn!("failed to read audio album art jpeg: {}", error);
                            }
                        }
                    }
                    MimeType::Png => {
                        match image::load_from_memory_with_format(
                            picture.data,
                            image::ImageFormat::Png,
                        ) {
                            Ok(image) => {
                                let nwidth;
                                let nheight;
                                if image.width() > image.height() {
                                    nwidth = max_size;
                                    nheight = nwidth * image.height() / image.width();
                                } else {
                                    nheight = max_size;
                                    nwidth = nheight * image.width() / image.height();
                                }
                                let thumb = image::imageops::resize(
                                    &image,
                                    nwidth,
                                    nheight,
                                    image::imageops::FilterType::Lanczos3,
                                );
                                let ret = thumb.save(thumbstring.clone());
                                if ret.is_err() {
                                    log::error!(
                                        "Failed to create thumbnail for file {}!",
                                        file.display()
                                    );
                                }
                            }
                            Err(error) => {
                                log::warn!("failed to read audio album art jpeg: {}", error);
                            }
                        }
                    }
                    MimeType::Bmp => {
                        match image::load_from_memory_with_format(
                            picture.data,
                            image::ImageFormat::Bmp,
                        ) {
                            Ok(image) => {
                                let nwidth;
                                let nheight;
                                if image.width() > image.height() {
                                    nwidth = max_size;
                                    nheight = nwidth * image.height() / image.width();
                                } else {
                                    nheight = max_size;
                                    nwidth = nheight * image.width() / image.height();
                                }
                                let thumb = image::imageops::resize(
                                    &image,
                                    nwidth,
                                    nheight,
                                    image::imageops::FilterType::Lanczos3,
                                );
                                let ret = thumb.save(thumbstring.clone());
                                if ret.is_err() {
                                    log::error!(
                                        "Failed to create thumbnail for file {}!",
                                        file.display()
                                    );
                                }
                            }
                            Err(error) => {
                                log::warn!("failed to read audio album art jpeg: {}", error);
                            }
                        }
                    }
                    MimeType::Gif => {
                        match image::load_from_memory_with_format(
                            picture.data,
                            image::ImageFormat::Gif,
                        ) {
                            Ok(image) => {
                                let nwidth;
                                let nheight;
                                if image.width() > image.height() {
                                    nwidth = max_size;
                                    nheight = nwidth * image.height() / image.width();
                                } else {
                                    nheight = max_size;
                                    nwidth = nheight * image.width() / image.height();
                                }
                                let thumb = image::imageops::resize(
                                    &image,
                                    nwidth,
                                    nheight,
                                    image::imageops::FilterType::Lanczos3,
                                );
                                let ret = thumb.save(thumbstring.clone());
                                if ret.is_err() {
                                    log::error!(
                                        "Failed to create thumbnail for file {}!",
                                        file.display()
                                    );
                                }
                            }
                            Err(error) => {
                                log::warn!("failed to read audio album art jpeg: {}", error);
                            }
                        }
                    }
                    MimeType::Tiff => {
                        match image::load_from_memory_with_format(
                            picture.data,
                            image::ImageFormat::Tiff,
                        ) {
                            Ok(image) => {
                                let nwidth;
                                let nheight;
                                if image.width() > image.height() {
                                    nwidth = max_size;
                                    nheight = nwidth * image.height() / image.width();
                                } else {
                                    nheight = max_size;
                                    nwidth = nheight * image.width() / image.height();
                                }
                                let thumb = image::imageops::resize(
                                    &image,
                                    nwidth,
                                    nheight,
                                    image::imageops::FilterType::Lanczos3,
                                );
                                let ret = thumb.save(thumbstring.clone());
                                if ret.is_err() {
                                    log::error!(
                                        "Failed to create thumbnail for file {}!",
                                        file.display()
                                    );
                                }
                            }
                            Err(error) => {
                                log::warn!("failed to read audio album art jpeg: {}", error);
                            }
                        }
                    }
                }
            }
            metadata.poster = thumbstring.clone();
            metadata.thumb = thumbstring;
        }
        None => {}
    };
}

/// Read Image metadata form EXIF format
///
fn parse_exif(path: &PathBuf, metadata: &mut crate::sql::ImageMetadata) {
    use nom_exif::*;
    match imagesize::size(path.to_path_buf()) {
        Ok(dim) => {
            metadata.width = dim.width as u32;
            metadata.height = dim.height as u32;
        }
        Err(why) => println!("Error getting size: {:?}", why),
    }
    let mut parser = MediaParser::new();
    if let Some(ext) = path.extension() {
        let extension = osstr_to_string(ext.to_os_string()).to_ascii_lowercase();
        if &extension == "png" || &extension == "gif" || &extension == "webp" {
            return;
        }
    }
    let pathstring = osstr_to_string(path.clone().into_os_string());
    match MediaSource::file_path(&pathstring) {
        Ok(ms) => {
            if ms.has_exif() {
                let iterres = parser.parse(ms);
                if iterres.is_err() {
                    return;
                }
                let iter: ExifIter = iterres.unwrap();
                if let Ok(optres) = iter.parse_gps_info() {
                    if let Some(gps_info) = optres {
                        metadata.gps_string = gps_info.format_iso6709();
                        metadata.gps_latitude =
                            (gps_info.latitude.0 .0 / gps_info.latitude.0 .1) as f32;
                        metadata.gps_latitude =
                            (gps_info.latitude.1 .0 / gps_info.latitude.1 .1) as f32 / 60.0;
                        metadata.gps_latitude =
                            (gps_info.latitude.2 .0 / gps_info.latitude.2 .1) as f32 / 3600.0;
                        if gps_info.latitude_ref == 'S' {
                            metadata.gps_latitude *= -1.0;
                        }
                        metadata.gps_longitude =
                            (gps_info.longitude.0 .0 / gps_info.longitude.0 .1) as f32;
                        metadata.gps_longitude =
                            (gps_info.longitude.1 .0 / gps_info.longitude.1 .1) as f32 / 60.0;
                        metadata.gps_longitude =
                            (gps_info.longitude.2 .0 / gps_info.longitude.2 .1) as f32 / 3600.0;
                        if gps_info.longitude_ref == 'W' {
                            metadata.gps_longitude *= -1.0;
                        }
                        metadata.gps_altitude = (gps_info.altitude.0 / gps_info.altitude.1) as f32;
                    }
                }
                let exif: Exif = iter.into();
                if let Some(val) = exif.get(ExifTag::DateTimeOriginal) {
                    if let Some(s) = val.as_str() {
                        if let Ok(date) = DateTime::parse_from_rfc3339(s) {
                            metadata.date = date.date_naive();
                        }
                    }
                }
                if let Some(val) = exif.get(ExifTag::LensModel) {
                    if let Some(s) = val.as_str() {
                        metadata.lense_model = s.to_string();
                    }
                }
                if let Some(val) = exif.get(ExifTag::FocalLength) {
                    if let Some(s) = val.as_str() {
                        metadata.focal_length = s.to_string();
                    }
                }
                if let Some(val) = exif.get(ExifTag::ExposureTime) {
                    if let Some(s) = val.as_str() {
                        metadata.exposure_time = s.to_string();
                    }
                }
                if let Some(val) = exif.get(ExifTag::FNumber) {
                    if let Some(s) = val.as_str() {
                        metadata.fnumber = s.to_string();
                    }
                }
                if let Some(val) = exif.get(ExifTag::GPSInfo) {
                    if let Some(s) = val.as_str() {
                        metadata.fnumber = s.to_string();
                    }
                }
            }
        }
        Err(error) => log::error!("Failed to open Media Source {}: {}", pathstring, error),
    }
}

pub fn item_from_entry(
    path: PathBuf,
    name: String,
    metadata: std::fs::Metadata,
    sizes: IconSizes,
) -> Item {
    let mut display_name = Item::display_name(&name);

    let hidden = name.starts_with(".") || hidden_attribute(&metadata);
    let (mime, icon_handle_grid, icon_handle_list, icon_handle_list_condensed) =
        if metadata.is_dir() {
            (
                //TODO: make this a static
                "inode/directory".parse().unwrap(),
                crate::tab::folder_icon(&path, sizes.grid()),
                crate::tab::folder_icon(&path, sizes.list()),
                crate::tab::folder_icon(&path, sizes.list_condensed()),
            )
        } else {
            let mime = mime_for_path(&path);
            //TODO: clean this up, implement for trash
            let icon_name_opt = if mime == "application/x-desktop" {
                let (desktop_name_opt, icon_name_opt) = crate::tab::parse_desktop_file(&path);
                if let Some(desktop_name) = desktop_name_opt {
                    display_name = Item::display_name(&desktop_name);
                }
                icon_name_opt
            } else {
                None
            };
            if let Some(icon_name) = icon_name_opt {
                (
                    mime.clone(),
                    widget::icon::from_name(&*icon_name)
                        .size(sizes.grid())
                        .handle(),
                    widget::icon::from_name(&*icon_name)
                        .size(sizes.list())
                        .handle(),
                    widget::icon::from_name(&*icon_name)
                        .size(sizes.list_condensed())
                        .handle(),
                )
            } else {
                (
                    mime.clone(),
                    mime_icon(mime.clone(), sizes.grid()),
                    mime_icon(mime.clone(), sizes.list()),
                    mime_icon(mime, sizes.list_condensed()),
                )
            }
        };

    let open_with = mime_apps(&mime);

    let children = if metadata.is_dir() {
        //TODO: calculate children in the background (and make it cancellable?)
        match std::fs::read_dir(&path) {
            Ok(entries) => entries.count(),
            Err(err) => {
                log::warn!("failed to read directory {:?}: {}", path, err);
                0
            }
        }
    } else {
        0
    };

    let dir_size = if metadata.is_dir() {
        DirSize::Calculating(crate::operation::controller::Controller::new())
    } else {
        DirSize::NotDirectory
    };

    let mut item = Item {
        name,
        display_name,
        metadata: ItemMetadata::Path { metadata, children },
        hidden,
        location_opt: Some(Location::Path(path)),
        mime,
        icon_handle_grid,
        icon_handle_list,
        icon_handle_list_condensed,
        open_with,
        thumbnail_opt: Some(ItemThumbnail::NotImage),
        button_id: widget::Id::unique(),
        pos_opt: Cell::new(None),
        rect_opt: Cell::new(None),
        selected: false,
        highlighted: false,
        overlaps_drag_rect: false,
        dir_size,
        image_opt: None,
        video_opt: None,
        audio_opt: None,
        collection_opt: None,
    };
    item.thumbnail_opt = Some(crate::tab::ItemThumbnail::new(item.clone()));
    item
}

pub fn item_from_path<P: Into<PathBuf>>(path: P, sizes: IconSizes) -> Result<Item, String> {
    let path = path.into();
    let name_os = path
        .file_name()
        .ok_or_else(|| format!("failed to get file name from path {:?}", path))?;
    let name = name_os
        .to_str()
        .ok_or_else(|| {
            format!(
                "failed to parse file name for {:?}: {:?} is not valid UTF-8",
                path, name_os
            )
        })?
        .to_string();
    let metadata = std::fs::metadata(&path)
        .map_err(|err| format!("failed to read metadata for {:?}: {}", path, err))?;
    Ok(item_from_entry(path, name, metadata, sizes))
}

/// convert a byte slice to a Vector of strings, split at newline characters
/// Used to convert system command output into parseable Strings.
pub fn slice_u8_to_vec_string(utfvec: Vec<u8>) -> Vec<String> {
    let mut v = Vec::new();
    let mut null_positions = Vec::new();
    for i in 0..utfvec.len() {
        if utfvec[i] == 0 {
            null_positions.push(i);
        }
    }
    if null_positions.len() < 1 {
        match String::from_utf8(utfvec) {
            Ok(string) => v.push(string),
            Err(error) => log::error!("failed to convert string: {}", error),
        }
        return v;
    }
    let mut start = 0;
    for end in null_positions {
        let mut partial: Vec<u8> = Vec::new();
        partial.extend_from_slice(&utfvec[start..end]);
        match String::from_utf8(partial) {
            Ok(string) => v.push(string),
            Err(error) => log::error!("failed to convert string: {}", error),
        }
        start = end;
    }
    v
}

/// Read video metadata directly from the video file.
/// Mainly used to get languages of internal streams and chapter information.
/// Utilizes a local install of ffmpeg. The only requirement is that the ffmpeg can read the video files.
fn video_metadata(meta: &mut crate::sql::VideoMetadata) {
    let basename;
    let fp = PathBuf::from(&meta.path);
    if let Some(bn) = fp.file_stem() {
        basename = osstr_to_string(bn.to_os_string());
    } else if let Some(bn) = fp.file_name() {
        basename = osstr_to_string(bn.to_os_string());
    } else {
        basename = meta.path.clone();
    }
    if meta.name.len() == 0 {
        meta.name = basename.clone();
    }
    if meta.title.len() == 0 {
        meta.title = basename.clone();
    }
    let command = format!("ffmpeg -i \"{}\"", meta.path);
    log::warn!("Executing command {}.", command);
    let mut cmd_runner = crate::cmd::CmdRunner::new(&command);
    if let Ok((stdout, _stderr)) = cmd_runner.run_with_output() {
        // let ffmpeg_output = std::process::Command::new("ffmpeg")
        //    .args(["-i", filepath])
        //    .output()
        //    .expect("failed to execute process");
        //let lines = slice_u8_to_vec_string(ffmpeg_output.stderr);
        let mut i = 0;
        let num_lines = stdout.len();
        while i < num_lines {
            let line = stdout[i].clone();
            if let Ok(re_duration) = regex::Regex::new(
                r"(?i)\s*Duration:\s+(?P<hours>\d+):(?P<minutes>\d+):(?P<seconds>\d+).\d+\s*,",
            ) {
                if re_duration.is_match(&line) {
                    let caps = re_duration.captures(&line).unwrap();
                    let hours = string_to_uint(&caps["hours"]);
                    let minutes = string_to_uint(&caps["minutes"]);
                    let seconds = string_to_uint(&caps["seconds"]);
                    meta.duration = hours * 3600 + minutes * 60 + seconds;
                }
            }
            if let Ok(re_video) = regex::Regex::new(r"(?i), (?P<width>\d+)x(?P<height>\d+)") {
                if re_video.is_match(&line) {
                    let caps = re_video.captures(&line).unwrap();
                    meta.width = string_to_uint(&caps["width"]);
                    meta.height = string_to_uint(&caps["height"]);
                }
            }
            if let Ok(re_chapter) =
                regex::Regex::new(r"(?i)start (?P<start>\d+\.\d+), end (?P<end>\d+\.\d+)")
            {
                if re_chapter.is_match(&line) {
                    let caps = re_chapter.captures(&line).unwrap();
                    let start = string_to_float(&caps["start"]);
                    let end = string_to_float(&caps["end"]);
                    let mut chapter = crate::sql::Chapter {
                        ..Default::default()
                    };
                    chapter.start = start;
                    chapter.end = end;
                    if i + 2 < num_lines {
                        let v: Vec<&str> = stdout[i + 2].split(" : ").collect();
                        if v.len() > 1 {
                            chapter.title = v[1].to_string();
                        }
                    }
                    if chapter.title.len() == 0 {
                        chapter.title = format!("Chapter_{:02}", meta.chapters.len() + 1);
                    }
                    meta.chapters.push(chapter);
                }
            }

            if let Ok(re_audio) = regex::Regex::new(r"(?i), \((?P<language>\w+)\):\s*Audio") {
                if re_audio.is_match(&line) {
                    let caps = re_audio.captures(&line).unwrap();
                    meta.audiolangs.push(caps["language"].to_string());
                }
            }
            if let Ok(re_sub) = regex::Regex::new(r"(?i), \((?P<language>\w+)\):\s*Subtitle") {
                if re_sub.is_match(&line) {
                    let caps = re_sub.captures(&line).unwrap();
                    meta.sublangs.push(caps["language"].to_string());
                }
            }
            i += 1;
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

/// format time in seconds into hh:mm:ss.000 format
pub fn timecode_to_ffmpeg_time(timecode: u32) -> String {
    let hours = timecode / 3600;
    let minutes = (timecode - hours * 3600) / 60;
    let seconds = timecode - hours * 3600 - minutes * 60;
    format!("{:02}:{:02}:{:02}.000", hours, minutes, seconds)
}

pub fn poster_path(path: &std::path::PathBuf) -> std::path::PathBuf {
    let posterpath;
    let hashvalue: u64 = crate::thumbnails::calculate_hash(path);
    let mut basename = String::from("poster");
    if let Some(base) = path.file_stem() {
        basename = osstr_to_string(base.to_os_string());
    }
    let filename = format!("{:016x}_{}-poster.jpeg", hashvalue, basename);
    match dirs::data_local_dir() {
        Some(pb) => {
            let mut dir = pb.join("media-browser").join("thumbs");
            if !dir.exists() {
                let ret = std::fs::create_dir_all(dir.clone());
                if ret.is_err() {
                    log::warn!("Failed to create directory {}", dir.display());
                    dir = crate::sql::home_dir_or_root();
                }
            }
            posterpath = dir.join(filename);
        }
        None => {
            let dir = crate::sql::home_dir_or_root().join(".thumbs").join("large");
            posterpath = dir.join(filename);
        }
    }
    posterpath
}

pub fn album_path(path: &std::path::PathBuf) -> std::path::PathBuf {
    let posterpath;
    let hashvalue: u64 = crate::thumbnails::calculate_hash(path);
    let mut basename = String::from("albumart");
    if let Some(base) = path.file_stem() {
        basename = osstr_to_string(base.to_os_string());
    }
    let filename = format!("{:016x}_{}-albumart.png", hashvalue, basename);
    match dirs::data_local_dir() {
        Some(pb) => {
            let mut dir = pb.join("media-browser").join("thumbs");
            if !dir.exists() {
                let ret = std::fs::create_dir_all(dir.clone());
                if ret.is_err() {
                    log::warn!("Failed to create directory {}", dir.display());
                    dir = crate::sql::home_dir_or_root();
                }
            }
            posterpath = dir.join(filename);
        }
        None => {
            let dir = crate::sql::home_dir_or_root().join(".thumbs").join("large");
            posterpath = dir.join(filename);
        }
    }
    posterpath
}

/// for a video without external metadata create a screenshot
/// and store it next to the video file.
/// Utilizes a local install of ffmpeg. The only requirement is that the ffmpeg can read the video files.
fn create_screenshots(meta: &mut crate::sql::VideoMetadata) {
    video_metadata(meta);
    let timecode = meta.duration / 10;
    let outputpattern = format!("{}_%03d.jpeg", meta.path);
    let localoutput = format!("{}_001.jpeg", meta.path);
    let localoutputpath = PathBuf::from(&localoutput);
    let inputpath = PathBuf::from(&meta.path);
    let outputpath = poster_path(&inputpath);
    let output = osstr_to_string(outputpath.clone().into_os_string());
    let time = timecode_to_ffmpeg_time(timecode);
    if outputpath.is_file() {
        //let ret = std::fs::remove_file(&output);
        //if ret.is_err() {
        //    log::error!("could not delete file {}", output);
        //}
        meta.poster = output.clone();
        return;
    }

    match std::process::Command::new("ffmpeg")
        .args([
            "-ss",
            &time,
            "-i",
            &meta.path,
            "-frames:v",
            "1",
            "-q:v",
            "2",
            &outputpattern,
        ])
        .output()
    {
        Ok(out) => match String::from_utf8(out.stderr) {
            Ok(text) => log::warn!("{}", &text),
            Err(_error) => {}
        },
        Err(error) => log::error!("Failed to generate Screenshot {}: {}", output, error),
    }
    if !localoutputpath.is_file() {
        log::error!(
            "Failed to create screenshot: {}",
            format!("ffmpeg -ss {} -i {} -frames:v 1 -q:v 2 ", time, meta.path)
        );
        return;
    } else {
        crate::thumbnails::create_thumbnail(&localoutputpath, 256);
        match std::fs::remove_file(&localoutputpath) {
            Ok(()) => {}
            Err(error) => {
                log::error!(
                    "could not write poster to metadata storage for file {}!: {}",
                    meta.path,
                    error
                );
                return;
            }
        }
    }
    //meta.poster = output.clone();
    meta.thumb = output;
}

/// create an item to put into our tabmodel from a video wihtout external metadata
pub fn item_from_video(
    path: PathBuf,
    videometadata: &mut crate::sql::VideoMetadata,
    statdata: &std::fs::Metadata,
    sizes: IconSizes,
    data: &crate::scanmetadata::ScanMetaData,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    from_db: bool,
) -> Item {
    let mut refresh = false;
    let filepath = osstr_to_string(path.clone().into_os_string());
    if !from_db {
        if data.known_files_contains(path.clone()) {
            if let Ok(modified) = statdata.modified() {
                if let Ok(new_date) = modified.duration_since(UNIX_EPOCH) {
                    let filedata = &data.known_files_get(path.clone());
                    let new_seconds_since_epoch = new_date.as_secs();
                    if new_seconds_since_epoch > filedata.modification_time {
                        refresh = true;
                    }
                }
                if refresh {
                    // file is newer
                    log::warn!("Updating entry for modified Video {:?}.", filepath);
                    create_screenshots(videometadata);
                    if videometadata.date.num_days_from_ce() < 100 {
                        if let Ok(created) = statdata.created() {
                            if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                                if let Some(ndt) =
                                    chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                                {
                                    videometadata.date = ndt.date_naive();
                                }
                            }
                        }
                    }
                    videometadata.thumb = crate::thumbnails::create_thumbnail(
                        &PathBuf::from(&videometadata.poster),
                        254,
                    );
                    crate::sql::update_video(sql_connection.clone(), videometadata, statdata, data);
                } else {
                    *videometadata = crate::sql::video(sql_connection.clone(), &filepath, data);
                }
            }
        } else {
            create_screenshots(videometadata);
            if videometadata.date.num_days_from_ce() < 100 {
                if let Ok(created) = statdata.created() {
                    if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                        if let Some(ndt) =
                            chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                        {
                            videometadata.date = ndt.date_naive();
                        }
                    }
                }
            }
            videometadata.thumb =
                crate::thumbnails::create_thumbnail(&PathBuf::from(&videometadata.poster), 254);
            crate::sql::insert_video(sql_connection.clone(), videometadata, statdata, data);
        }
    }

    let mut display_name = Item::display_name(&videometadata.name);

    let hidden = videometadata.name.starts_with(".") || hidden_attribute(&statdata);

    let (mime, icon_handle_grid, icon_handle_list, icon_handle_list_condensed) =
        if statdata.is_dir() {
            (
                //TODO: make this a static
                "inode/directory".parse().unwrap(),
                crate::tab::folder_icon(&path, sizes.grid()),
                crate::tab::folder_icon(&path, sizes.list()),
                crate::tab::folder_icon(&path, sizes.list_condensed()),
            )
        } else {
            if videometadata.path.len() > 0 {
                //let thumbpath = PathBuf::from(&videometadata.poster);
                //let thumbmime = mime_for_path(thumbpath.clone());
                let filemime = mime_for_path(filepath.clone());
                if videometadata.thumb.len() > 0 {
                    let thumbpath = PathBuf::from(&videometadata.thumb);
                    (
                        filemime.clone(),
                        widget::icon::from_path(thumbpath.clone()),
                        widget::icon::from_path(thumbpath.clone()),
                        widget::icon::from_path(thumbpath.clone()),
                    )
                } else {
                    (
                        filemime.clone(),
                        mime_icon(filemime.clone(), sizes.grid()),
                        mime_icon(filemime.clone(), sizes.list()),
                        mime_icon(filemime.clone(), sizes.list_condensed()),
                    )
                }
            } else {
                let mime = mime_for_path(&path);
                //TODO: clean this up, implement for trash
                let icon_name_opt = if mime == "application/x-desktop" {
                    let (desktop_name_opt, icon_name_opt) = crate::tab::parse_desktop_file(&path);
                    if let Some(desktop_name) = desktop_name_opt {
                        display_name = Item::display_name(&desktop_name);
                    }
                    icon_name_opt
                } else {
                    None
                };
                if let Some(icon_name) = icon_name_opt {
                    (
                        mime.clone(),
                        widget::icon::from_name(&*icon_name)
                            .size(sizes.grid())
                            .handle(),
                        widget::icon::from_name(&*icon_name)
                            .size(sizes.list())
                            .handle(),
                        widget::icon::from_name(&*icon_name)
                            .size(sizes.list_condensed())
                            .handle(),
                    )
                } else {
                    (
                        mime.clone(),
                        mime_icon(mime.clone(), sizes.grid()),
                        mime_icon(mime.clone(), sizes.list()),
                        mime_icon(mime, sizes.list_condensed()),
                    )
                }
            }
        };

    let open_with = mime_apps(&mime);

    let children = if statdata.is_dir() {
        //TODO: calculate children in the background (and make it cancellable?)
        match std::fs::read_dir(&path) {
            Ok(entries) => entries.count(),
            Err(err) => {
                log::warn!("failed to read directory {:?}: {}", path, err);
                0
            }
        }
    } else {
        0
    };

    let dir_size = if statdata.is_dir() {
        DirSize::Calculating(crate::operation::controller::Controller::new())
    } else {
        DirSize::NotDirectory
    };

    let mut item = Item {
        name: videometadata.name.to_string(),
        display_name,
        metadata: ItemMetadata::Path {
            metadata: statdata.clone(),
            children,
        },
        hidden,
        location_opt: Some(Location::Path(path)),
        mime,
        icon_handle_grid,
        icon_handle_list,
        icon_handle_list_condensed,
        open_with,
        thumbnail_opt: Some(ItemThumbnail::NotImage),
        button_id: widget::Id::unique(),
        pos_opt: Cell::new(None),
        rect_opt: Cell::new(None),
        selected: false,
        highlighted: false,
        overlaps_drag_rect: false,
        dir_size,
        image_opt: None,
        video_opt: Some(videometadata.to_owned()),
        audio_opt: None,
        collection_opt: None,
    };
    item.thumbnail_opt = Some(crate::tab::ItemThumbnail::new(item.clone()));
    item
}

/// create an item to put into our tabmodel from a video with NFO metadata
pub fn item_from_nfo(
    nfo_file: PathBuf,
    metadata: &mut crate::sql::VideoMetadata,
    statdata: &std::fs::Metadata,
    sizes: IconSizes,
    data: &crate::scanmetadata::ScanMetaData,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    from_db: bool,
) -> Item {
    let filepath = PathBuf::from(&metadata.path);
    let basename;
    if let Some(bn) = filepath.file_stem() {
        basename = osstr_to_string(bn.to_os_string());
    } else {
        if let Some(bn) = filepath.file_name() {
            basename = osstr_to_string(bn.to_os_string());
        } else {
            basename = osstr_to_string(filepath.clone().into_os_string());
        }
    }
    if !from_db {
        if data.known_files_contains(filepath.clone()) {
            let mut refresh = false;
            if let Ok(modified) = statdata.modified() {
                if let Ok(new_date) = modified.duration_since(UNIX_EPOCH) {
                    let filedata = &data.known_files_get(filepath.clone());
                    let new_seconds_since_epoch = new_date.as_secs();
                    if new_seconds_since_epoch > filedata.modification_time {
                        refresh = true;
                    }
                }
                if refresh {
                    // file is newer
                    log::warn!("Updating entry for modified Movie {:?}.", filepath);
                    video_metadata(metadata);
                    parse_nfo(&nfo_file, metadata);
                    if metadata.date.num_days_from_ce() < 100 {
                        if let Ok(created) = statdata.created() {
                            if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                                if let Some(ndt) =
                                    chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                                {
                                    metadata.date = ndt.date_naive();
                                }
                            }
                        }
                    }
                    metadata.thumb =
                        crate::thumbnails::create_thumbnail(&PathBuf::from(&metadata.poster), 254);
                    metadata.name = basename.clone();
                    crate::sql::update_video(sql_connection.clone(), metadata, statdata, data);
                } else {
                    *metadata = crate::sql::video(sql_connection.clone(), &metadata.path, data);
                }
            }
        } else {
            video_metadata(metadata);
            parse_nfo(&nfo_file, metadata);
            if metadata.date.num_days_from_ce() < 100 {
                if let Ok(created) = statdata.created() {
                    if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                        if let Some(ndt) =
                            chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                        {
                            metadata.date = ndt.date_naive();
                        }
                    }
                }
            }
            metadata.thumb =
                crate::thumbnails::create_thumbnail(&PathBuf::from(&metadata.poster), 254);
            metadata.name = basename.clone();
            crate::sql::insert_video(sql_connection.clone(), metadata, statdata, data);
        }
    }

    let name;
    if metadata.title.len() == 0 {
        name = basename.clone();
    } else {
        name = metadata.title.clone();
    }

    let display_name = Item::display_name(&name);

    let hidden = name.starts_with(".") || hidden_attribute(&statdata);

    let (mime, icon_handle_grid, icon_handle_list, icon_handle_list_condensed) = {
        let thumbpath = PathBuf::from(&metadata.thumb);
        let thumbmime = mime_for_path(thumbpath.clone());
        let filemime = mime_for_path(filepath.clone());
        if metadata.thumb.len() > 0 {
            (
                filemime.clone(),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
            )
        } else {
            (
                filemime.clone(),
                mime_icon(thumbmime.clone(), sizes.grid()),
                mime_icon(thumbmime.clone(), sizes.list()),
                mime_icon(thumbmime.clone(), sizes.list_condensed()),
            )
        }
    };

    let open_with = mime_apps(&mime);

    let children = if statdata.is_dir() {
        //TODO: calculate children in the background (and make it cancellable?)
        match std::fs::read_dir(PathBuf::from(&metadata.path)) {
            Ok(entries) => entries.count(),
            Err(err) => {
                log::warn!("failed to read directory {:?}: {}", metadata.path, err);
                0
            }
        }
    } else {
        0
    };

    let dir_size = if statdata.is_dir() {
        DirSize::Calculating(crate::operation::controller::Controller::new())
    } else {
        DirSize::NotDirectory
    };

    let mut item = Item {
        name,
        display_name,
        metadata: ItemMetadata::Path {
            metadata: statdata.clone(),
            children,
        },
        hidden,
        location_opt: Some(Location::Path(PathBuf::from(&metadata.path))),
        mime,
        icon_handle_grid,
        icon_handle_list,
        icon_handle_list_condensed,
        open_with,
        thumbnail_opt: Some(ItemThumbnail::NotImage),
        button_id: widget::Id::unique(),
        pos_opt: Cell::new(None),
        rect_opt: Cell::new(None),
        selected: false,
        highlighted: false,
        overlaps_drag_rect: false,
        dir_size,
        image_opt: None,
        video_opt: Some(metadata.clone()),
        audio_opt: None,
        collection_opt: None,
    };
    item.thumbnail_opt = Some(crate::tab::ItemThumbnail::new(item.clone()));
    item
}

/// try to find external metadata for audio files
/// Lyrics or coverart.
fn audio_metadata(
    audio: PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    meta_data: &mut crate::sql::AudioMetadata,
) {
    // find external lyrics files
    if let Some(path) = audio.parent() {
        if let Some(base) = audio.file_stem() {
            let base = path.join(base);
            let lyrics: String = osstr_to_string(base.into_os_string());
            let mut lyrics_1 = lyrics.clone();
            lyrics_1.extend(".lrc".to_string().chars());
            let mut lyrics_2 = lyrics.clone();
            lyrics_2.extend(".srt".to_string().chars());
            if PathBuf::from(lyrics_1.clone()).is_file() {
                meta_data.lyrics.push(lyrics_1.clone());
                data.special_files_insert(PathBuf::from(lyrics_1.clone()));
            }
            if PathBuf::from(lyrics_2.clone()).is_file() {
                meta_data.lyrics.push(lyrics_2.clone());
                data.special_files_insert(PathBuf::from(lyrics_2.clone()));
            }
        }
    }
    // find a same-stem cover image next to the file (`track.mp3` ->
    // `track.png` / `track.jpg` / `track.jpeg`)
    let audio_str = osstr_to_string(audio.clone().into_os_string());
    let mut posterpath = std::path::PathBuf::new();
    for ext in ["png", "jpg", "jpeg"] {
        let Some(candidate) = crate::audio::coverart::same_stem_with_ext(&audio_str, ext) else {
            continue;
        };
        if candidate.is_file() {
            posterpath = candidate;
            break;
        }
    }
    if !posterpath.as_os_str().is_empty() {
        let poster = osstr_to_string(posterpath.clone().into_os_string());
        meta_data.poster = poster;
        data.special_files_insert(posterpath.clone());
    }
    if meta_data.thumb.len() == 0 {
        if posterpath.exists() {
            data.special_files_insert(posterpath.clone());
            let thumb = crate::thumbnails::create_thumbnail(&posterpath, 256);
            if thumb.len() > 0 {
                meta_data.thumb = thumb.clone();
            }
        }
    } else {
        data.special_files_insert(PathBuf::from(meta_data.thumb.clone()));
    }
}

/// create an item to put into our tabmodel from a audio file with internal metadata
pub fn item_from_audiotags(
    audio: PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    metadata: &mut crate::sql::AudioMetadata,
    statdata: &std::fs::Metadata,
    sizes: IconSizes,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    from_db: bool,
) -> Item {
    let filepath = PathBuf::from(&metadata.path);
    let basename;
    if let Some(bn) = filepath.file_stem() {
        basename = osstr_to_string(bn.to_os_string());
    } else {
        if let Some(bn) = filepath.file_name() {
            basename = osstr_to_string(bn.to_os_string());
        } else {
            basename = osstr_to_string(filepath.clone().into_os_string());
        }
    }
    if from_db {
        // fill the program data structures to work properly
        audio_metadata(audio, data, metadata);
    } else {
        if data.known_files_contains(filepath.clone()) {
            let mut refresh = false;
            if let Ok(modified) = statdata.modified() {
                if let Ok(new_date) = modified.duration_since(UNIX_EPOCH) {
                    let filedata = &data.known_files_get(filepath.clone());
                    let new_seconds_since_epoch = new_date.as_secs();
                    if new_seconds_since_epoch > filedata.modification_time {
                        refresh = true;
                    }
                }
                if refresh {
                    // file is newer
                    log::warn!("Updating entry for modified Audio {:?}.", filepath);
                    parse_audiotags(&audio, metadata);
                    // read chapter data from ffmpeg
                    let mut temp = crate::sql::VideoMetadata {
                        ..Default::default()
                    };
                    if metadata.chapters.len() == 0 {
                        temp.path = metadata.path.clone();
                        video_metadata(&mut temp);
                        if temp.chapters.len() > 0 {
                            metadata.chapters.extend(temp.chapters);
                        }
                        if temp.duration > 0 && metadata.duration == 0 {
                            metadata.duration = temp.duration
                        }
                    }

                    audio_metadata(audio, data, metadata);
                    if metadata.date.num_days_from_ce() < 100 {
                        if let Ok(created) = statdata.created() {
                            if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                                if let Some(ndt) =
                                    chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                                {
                                    metadata.date = ndt.date_naive();
                                }
                            }
                        }
                    }
                    crate::sql::update_audio(sql_connection.clone(), metadata, statdata, data);
                } else {
                    *metadata = crate::sql::audio(sql_connection.clone(), &metadata.path, data);
                    audio_metadata(audio, data, metadata);
                }
            }
        } else {
            parse_audiotags(&audio, metadata);
            // read chapter data from ffmpeg
            let mut temp = crate::sql::VideoMetadata {
                ..Default::default()
            };
            if metadata.chapters.len() == 0 {
                temp.path = metadata.path.clone();
                video_metadata(&mut temp);
                if temp.chapters.len() > 0 {
                    metadata.chapters.extend(temp.chapters);
                }
                if temp.duration > 0 && metadata.duration == 0 {
                    metadata.duration = temp.duration
                }
            }

            audio_metadata(audio, data, metadata);
            if metadata.date.num_days_from_ce() < 100 {
                if let Ok(created) = statdata.created() {
                    if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                        if let Some(ndt) =
                            chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                        {
                            metadata.date = ndt.date_naive();
                        }
                    }
                }
            }
            crate::sql::insert_audio(sql_connection.clone(), metadata, statdata, data);
        }
    }

    let name;
    if metadata.title.len() == 0 {
        name = basename.clone();
    } else {
        name = metadata.title.clone();
    }

    let mut display_name = Item::display_name(&name);

    let hidden = name.starts_with(".") || hidden_attribute(&statdata);

    let thumbpath;
    if metadata.thumb.len() == 0 {
        // generate thumbnail
        thumbpath = PathBuf::from(&metadata.thumb);
    } else {
        thumbpath = PathBuf::from(&metadata.poster);
    }

    let thumbmime = mime_for_path(thumbpath.clone());
    let audiomime = mime_for_path(filepath.clone());

    let (mime, icon_handle_grid, icon_handle_list, icon_handle_list_condensed) = {
        //TODO: clean this up, implement for trash
        let _icon_name_opt = if audiomime == "application/x-desktop" {
            let (desktop_name_opt, icon_name_opt) = crate::tab::parse_desktop_file(&filepath);
            if let Some(desktop_name) = desktop_name_opt {
                display_name = Item::display_name(&desktop_name);
            }
            icon_name_opt
        } else {
            None
        };

        if metadata.thumb.len() > 0 {
            (
                audiomime.clone(),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
            )
        } else {
            (
                audiomime.clone(),
                mime_icon(thumbmime.clone(), sizes.grid()),
                mime_icon(thumbmime.clone(), sizes.list()),
                mime_icon(thumbmime.clone(), sizes.list_condensed()),
            )
        }
    };

    let open_with = mime_apps(&audiomime);

    let children = if statdata.is_dir() {
        //TODO: calculate children in the background (and make it cancellable?)
        match std::fs::read_dir(PathBuf::from(&metadata.path)) {
            Ok(entries) => entries.count(),
            Err(err) => {
                log::warn!("failed to read directory {:?}: {}", metadata.path, err);
                0
            }
        }
    } else {
        0
    };

    let dir_size = if statdata.is_dir() {
        DirSize::Calculating(crate::operation::controller::Controller::new())
    } else {
        DirSize::NotDirectory
    };

    let mut item = Item {
        name,
        display_name,
        metadata: ItemMetadata::Path {
            metadata: statdata.clone(),
            children,
        },
        hidden,
        location_opt: Some(Location::Path(PathBuf::from(&metadata.path))),
        mime,
        icon_handle_grid,
        icon_handle_list,
        icon_handle_list_condensed,
        open_with,
        thumbnail_opt: Some(ItemThumbnail::NotImage),
        button_id: widget::Id::unique(),
        pos_opt: Cell::new(None),
        rect_opt: Cell::new(None),
        selected: false,
        highlighted: false,
        overlaps_drag_rect: false,
        dir_size,
        image_opt: None,
        video_opt: None,
        audio_opt: Some(metadata.clone()),
        collection_opt: None,
    };
    item.thumbnail_opt = Some(crate::tab::ItemThumbnail::new(item.clone()));
    item
}

/// create an item to put into our tabmodel from an image
pub fn item_from_exif(
    image_file: PathBuf,
    metadata: &mut crate::sql::ImageMetadata,
    statdata: &std::fs::Metadata,
    sizes: IconSizes,
    data: &crate::scanmetadata::ScanMetaData,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    from_db: bool,
) -> Item {
    let filepath = PathBuf::from(&metadata.path);
    let basename;
    if let Some(bn) = filepath.file_stem() {
        basename = osstr_to_string(bn.to_os_string());
    } else {
        if let Some(bn) = filepath.file_name() {
            basename = osstr_to_string(bn.to_os_string());
        } else {
            basename = osstr_to_string(filepath.clone().into_os_string());
        }
    }
    if !from_db {
        if data.known_files_contains(filepath.clone()) {
            let mut refresh = false;
            if let Ok(modified) = statdata.modified() {
                if let Ok(new_date) = modified.duration_since(UNIX_EPOCH) {
                    let filedata = &data.known_files_get(filepath.clone());
                    let new_seconds_since_epoch = new_date.as_secs();
                    if new_seconds_since_epoch > filedata.modification_time {
                        refresh = true;
                    }
                }
                if refresh {
                    // file is newer
                    log::warn!("Updating entry for modified Image {:?}.", filepath);
                    parse_exif(&image_file, metadata);
                    if metadata.thumb.len() == 0 {
                        if metadata.resized.len() > 0 {
                            metadata.thumb = crate::thumbnails::create_thumbnail(
                                &PathBuf::from(&metadata.resized),
                                254,
                            );
                        } else {
                            metadata.thumb = crate::thumbnails::create_thumbnail(
                                &PathBuf::from(&metadata.path),
                                254,
                            );
                        }
                    }
                    if metadata.date.num_days_from_ce() < 100 {
                        if let Ok(created) = statdata.created() {
                            if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                                if let Some(ndt) =
                                    chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                                {
                                    metadata.date = ndt.date_naive();
                                }
                            }
                        }
                    }
                    metadata.name = basename.clone();
                    crate::sql::update_image(sql_connection.clone(), metadata, statdata, data);
                } else {
                    *metadata = crate::sql::image(sql_connection.clone(), &metadata.path, data);
                }
            }
        } else {
            parse_exif(&image_file, metadata);
            if metadata.thumb.len() == 0 {
                if metadata.resized.len() > 0 {
                    metadata.thumb =
                        crate::thumbnails::create_thumbnail(&PathBuf::from(&metadata.resized), 254);
                } else {
                    metadata.thumb =
                        crate::thumbnails::create_thumbnail(&PathBuf::from(&metadata.path), 254);
                }
            }
            if metadata.date.num_days_from_ce() < 100 {
                if let Ok(created) = statdata.created() {
                    if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                        if let Some(ndt) =
                            chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                        {
                            metadata.date = ndt.date_naive();
                        }
                    }
                }
            }
            metadata.name = basename.clone();
            crate::sql::insert_image(sql_connection.clone(), metadata, statdata, data);
        }
    }

    let name;
    if metadata.title.len() == 0 {
        name = basename.clone();
    } else {
        name = metadata.title.clone();
    }

    let mut display_name = Item::display_name(&name);

    let hidden = name.starts_with(".") || hidden_attribute(&statdata);

    let thumbpath;
    if metadata.thumb.len() == 0 {
        // generate thumbnail
        if metadata.resized.len() > 0 {
            metadata.thumb =
                crate::thumbnails::create_thumbnail(&PathBuf::from(&metadata.resized), 254);
        } else {
            metadata.thumb =
                crate::thumbnails::create_thumbnail(&PathBuf::from(&metadata.path), 254);
        }
        thumbpath = PathBuf::from(&metadata.thumb);
    } else {
        thumbpath = PathBuf::from(&metadata.thumb);
    }

    let thumbmime = mime_for_path(thumbpath.clone());
    let imagemime = mime_for_path(filepath.clone());

    let (mime, icon_handle_grid, icon_handle_list, icon_handle_list_condensed) = {
        //TODO: clean this up, implement for trash
        let _icon_name_opt = if imagemime == "application/x-desktop" {
            let (desktop_name_opt, icon_name_opt) = crate::tab::parse_desktop_file(&filepath);
            if let Some(desktop_name) = desktop_name_opt {
                display_name = Item::display_name(&desktop_name);
            }
            icon_name_opt
        } else {
            None
        };

        if metadata.thumb.len() > 0 {
            (
                imagemime.clone(),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
            )
        } else {
            (
                imagemime.clone(),
                mime_icon(thumbmime.clone(), sizes.grid()),
                mime_icon(thumbmime.clone(), sizes.list()),
                mime_icon(thumbmime.clone(), sizes.list_condensed()),
            )
        }
    };

    let open_with = mime_apps(&imagemime);

    let children = if statdata.is_dir() {
        //TODO: calculate children in the background (and make it cancellable?)
        match std::fs::read_dir(PathBuf::from(&metadata.path)) {
            Ok(entries) => entries.count(),
            Err(err) => {
                log::warn!("failed to read directory {:?}: {}", metadata.path, err);
                0
            }
        }
    } else {
        0
    };

    let dir_size = if statdata.is_dir() {
        DirSize::Calculating(crate::operation::controller::Controller::new())
    } else {
        DirSize::NotDirectory
    };

    let mut item = Item {
        name,
        display_name,
        metadata: ItemMetadata::Path {
            metadata: statdata.clone(),
            children,
        },
        hidden,
        location_opt: Some(Location::Path(PathBuf::from(&metadata.path))),
        mime,
        icon_handle_grid,
        icon_handle_list,
        icon_handle_list_condensed,
        open_with,
        thumbnail_opt: Some(ItemThumbnail::NotImage),
        button_id: widget::Id::unique(),
        pos_opt: Cell::new(None),
        rect_opt: Cell::new(None),
        selected: false,
        highlighted: false,
        overlaps_drag_rect: false,
        dir_size,
        image_opt: Some(metadata.clone()),
        video_opt: None,
        audio_opt: None,
        collection_opt: None,
    };
    item.thumbnail_opt = Some(crate::tab::ItemThumbnail::new(item.clone()));
    item
}

/// convert an OsString (from PathBuf) to a usable String
pub fn osstr_to_string(osstr: std::ffi::OsString) -> String {
    match osstr.to_str() {
        Some(str) => return str.to_string(),
        None => {}
    }
    String::new()
}

/// Scanners to process the filesystem

pub fn scan_files(
    path: PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
) -> ControlFlow<()> {
    if data.special_files_contains(path.clone()) {
        return ControlFlow::Break(());
    }
    let name;
    if let Some(bn) = path.clone().file_name() {
        name = crate::parsers::osstr_to_string(bn.to_os_string());
    } else {
        name = crate::parsers::osstr_to_string(path.clone().into_os_string());
    }
    let metadata = match std::fs::metadata(&path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!("failed to read metadata for entry at {:?}: {}", path, err);
            return ControlFlow::Break(());
        }
    };
    data.special_files_insert(path.clone());
    let item = crate::parsers::item_from_entry(path, name, metadata, sizes);
    data.items_push(item);
    ControlFlow::Continue(())
}

pub fn scan_directories(
    path: PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
) -> ControlFlow<()> {
    if data.special_files_contains(path.clone()) {
        return ControlFlow::Break(());
    }
    let name;
    if let Some(bn) = path.file_name() {
        name = crate::parsers::osstr_to_string(bn.to_os_string());
    } else {
        name = crate::parsers::osstr_to_string(path.clone().into_os_string());
    }
    let metadata = match std::fs::metadata(&path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!("failed to read metadata for entry at {:?}: {}", path, err);
            return ControlFlow::Break(());
        }
    };
    data.special_files_insert(path.clone());
    let item = crate::parsers::item_from_entry(path, name, metadata, sizes);
    data.items_push(item);
    ControlFlow::Continue(())
}

pub fn scan_videos(
    path: PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> ControlFlow<()> {
    if data.special_files_contains(path.clone()) {
        return ControlFlow::Break(());
    }
    let mut meta_data = crate::sql::VideoMetadata {
        ..Default::default()
    };
    meta_data.path = osstr_to_string(path.clone().into_os_string());
    if meta_data.path.len() == 0 {
        return ControlFlow::Break(());
    }
    let name;
    if let Some(bn) = path.file_name() {
        name = crate::parsers::osstr_to_string(bn.to_os_string());
    } else {
        name = crate::parsers::osstr_to_string(path.clone().into_os_string());
    }
    meta_data.name = name.clone();
    meta_data.title = name.clone();
    let statdata = match std::fs::metadata(&path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!("failed to read metadata for entry at {:?}: {}", path, err);
            return ControlFlow::Break(());
        }
    };
    data.special_files_insert(path.clone());
    let mut poster = osstr_to_string(path.clone().into_os_string());
    poster.push_str("_001.jpeg");
    meta_data.poster = poster.clone();
    data.special_files_insert(PathBuf::from(&poster));
    let item = crate::parsers::item_from_video(
        path,
        &mut meta_data,
        &statdata,
        sizes,
        data,
        sql_connection.clone(),
        false,
    );
    data.items_push(item);
    ControlFlow::Continue(())
}

pub fn scan_audiotags(
    audio: PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> ControlFlow<()> {
    if data.special_files_contains(audio.clone()) {
        return ControlFlow::Break(());
    }
    let mut meta_data = crate::sql::AudioMetadata {
        ..Default::default()
    };
    meta_data.path = osstr_to_string(audio.clone().into_os_string());
    if meta_data.path.len() == 0 {
        return ControlFlow::Break(());
    }
    let statdata = match std::fs::metadata(&meta_data.path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!(
                "failed to read metadata for entry at {:?}: {}",
                meta_data.path,
                err
            );
            return ControlFlow::Break(());
        }
    };
    data.special_files_insert(audio.clone());
    // find external cover art

    let item = crate::parsers::item_from_audiotags(
        audio,
        data,
        &mut meta_data,
        &statdata,
        sizes,
        sql_connection.clone(),
        false,
    );
    data.items_push(item);

    ControlFlow::Continue(())
}

pub fn scan_exif(
    path: PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> ControlFlow<()> {
    if data.special_files_contains(path.clone()) {
        return ControlFlow::Break(());
    }
    let mut meta_data = crate::sql::ImageMetadata {
        ..Default::default()
    };
    meta_data.path = osstr_to_string(path.clone().into_os_string());
    if meta_data.path.len() == 0 {
        return ControlFlow::Break(());
    }
    let statdata = match std::fs::metadata(&meta_data.path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!(
                "failed to read metadata for entry at {:?}: {}",
                meta_data.path,
                err
            );
            return ControlFlow::Break(());
        }
    };
    data.special_files_insert(path.clone());
    let (imagestr, thumbstr) =
        crate::thumbnails::create_thumbnail_downscale_if_necessary(&path, 254, 6000);
    meta_data.thumb = thumbstr.clone();
    if imagestr.len() > 0 {
        meta_data.resized = imagestr.clone();
    }
    let item = crate::parsers::item_from_exif(
        path,
        &mut meta_data,
        &statdata,
        sizes,
        data,
        sql_connection.clone(),
        false,
    );
    data.items_push(item);
    ControlFlow::Continue(())
}

pub fn scan_single_nfo_dir(
    dp: &PathBuf,
    tab_path: &PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
    justdirs: &mut Vec<PathBuf>,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> ControlFlow<()> {
    if data.special_files_contains(dp.clone()) {
        return ControlFlow::Break(());
    }
    let mut meta_data = crate::sql::VideoMetadata {
        ..Default::default()
    };
    let mut movie = 0;
    let mut poster = 0;
    let mut nfo = 0;
    let mut nfo_file = dp.clone().join("movie.nfo");
    let mut contents = Vec::new();
    let mut movie_name = String::new();
    let mut nfo_names = Vec::new();
    // test if we have a directory with a TV Show
    let tvshow_nfo_file = dp.clone().join("tvshow.nfo");
    if tvshow_nfo_file.exists() {
        data.tvshows_push(dp.to_path_buf());
        return ControlFlow::Break(());
    }

    match std::fs::read_dir(dp) {
        Ok(entries) => {
            for entry_res in entries {
                let entry = match entry_res {
                    Ok(ok) => ok,
                    Err(err) => {
                        log::warn!("failed to read entry in {:?}: {}", tab_path, err);
                        continue;
                    }
                };
                let path = entry.path();
                let filename = match path.file_name() {
                    Some(name) => name,
                    None => continue,
                };
                if osstr_to_string(filename.to_os_string()).starts_with(".") {
                    continue;
                }
                contents.push(path);
            }
            if contents.len() > 15 {
                justdirs.push(dp.clone());
                return ControlFlow::Break(());
            }
            for path in contents.iter() {
                if path.is_dir() {
                    justdirs.push(dp.clone());
                    return ControlFlow::Break(());
                }
                let mime = crate::mime_icon::mime_for_path(path.clone());
                let lowercase = osstr_to_string(path.clone().into_os_string()).to_ascii_lowercase();
                let f = osstr_to_string(path.clone().into_os_string()).to_ascii_lowercase();
                if f.contains("poster.") {
                    if poster > 0 {
                        justdirs.push(dp.to_path_buf());
                        return ControlFlow::Break(());
                    }
                    meta_data.poster = osstr_to_string(path.clone().into_os_string());
                    poster += 1;
                } else if f.contains(".srt") {
                    meta_data.subtitles.push(f.clone());
                } else if lowercase.ends_with("tvshow.nfo") {
                    if let Some(basepath) = path.parent() {
                        data.tvshows_push(basepath.to_path_buf());
                    } else {
                        data.tvshows_push(path.to_path_buf());
                    }
                    return ControlFlow::Break(());
                } else if f.contains(".nfo") {
                    if nfo > 1 {
                        justdirs.push(dp.to_path_buf());
                        return ControlFlow::Break(());
                    }
                    if !lowercase.ends_with("movie.nfo") {
                        nfo_file = path.to_path_buf();
                    }
                    if let Some(basename) = path.file_stem() {
                        nfo_names.push(osstr_to_string(basename.to_os_string()));
                    } else {
                        nfo_names.push(osstr_to_string(path.clone().into_os_string()));
                    }
                    nfo += 1;
                } else if mime.type_() == mime_guess::mime::VIDEO {
                    if movie > 0 {
                        justdirs.push(dp.to_path_buf());
                        return ControlFlow::Break(());
                    }
                    meta_data.path = osstr_to_string(path.clone().into_os_string());
                    if meta_data.name.len() == 0 {
                        if let Some(file) = path.clone().file_stem() {
                            meta_data.name = osstr_to_string(file.to_os_string());
                        } else {
                            meta_data.name = osstr_to_string(path.clone().into_os_string());
                        }
                        movie_name = meta_data.name.clone();
                    }
                    movie += 1;
                }
            }
            // filter cases where we have multiple NFOs and none of them has the movie name
            if movie_name.len() > 0 && nfo_names.len() > 1 {
                let mut movienamenfo = false;
                for n in nfo_names.iter() {
                    if &movie_name == n {
                        movienamenfo = true;
                    }
                }
                if !movienamenfo {
                    justdirs.push(dp.to_path_buf());
                    return ControlFlow::Break(());
                }
            }
        }
        Err(err) => {
            log::warn!("failed to read directory {:?}: {}", tab_path, err);
        }
    }
    if !PathBuf::from(&nfo_file).exists() {
        justdirs.push(dp.to_path_buf());
        return ControlFlow::Break(());
    }
    if meta_data.path.len() == 0 {
        justdirs.push(dp.to_path_buf());
        return ControlFlow::Break(());
    }
    if meta_data.poster.len() == 0 {
        create_screenshots(&mut meta_data);
        if meta_data.poster.len() == 0 {
            log::error!(
                "Failed to find poster or create a screenshot for movie {}",
                &meta_data.path
            );
            return ControlFlow::Break(());
        }
    }
    data.special_files_insert(dp.to_path_buf());
    for path in contents.iter() {
        data.special_files_insert(path.to_path_buf());
    }
    let thumbpath = PathBuf::from(&meta_data.poster);
    if thumbpath.exists() {
        let thumb = crate::thumbnails::create_thumbnail(&thumbpath, 256);
        if thumb.len() > 0 {
            meta_data.thumb = thumb.clone();
        }
    }

    let statdata = match std::fs::metadata(&meta_data.path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!(
                "failed to read metadata for entry at {:?}: {}",
                meta_data.path,
                err
            );
            return ControlFlow::Break(());
        }
    };
    let item = crate::parsers::item_from_nfo(
        nfo_file,
        &mut meta_data,
        &statdata,
        sizes,
        data,
        sql_connection.clone(),
        false,
    );
    data.items_push(item);
    // test if we have a single movie with NFO in this dir

    ControlFlow::Continue(())
}

pub fn scan_nfos_in_dir(
    video: String,
    all: &Vec<PathBuf>,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> ControlFlow<()> {
    let mut meta_data = crate::sql::VideoMetadata {
        ..Default::default()
    };
    let mut nfo_file = PathBuf::from(&format!("{}.nfo", video));
    for fp in all.iter() {
        let mime = crate::mime_icon::mime_for_path(fp.clone());
        let f = osstr_to_string(fp.clone().into_os_string());
        //if re.is_match(&f) {
        if f.contains(&video) {
            // part of a local video or metadata
            if f.contains("poster.") {
                meta_data.poster = f.clone();
            } else if f.contains("thumb.") && meta_data.poster.len() == 0 {
                meta_data.poster = f.clone();
            } else if f.ends_with(".srt") {
                meta_data.subtitles.push(f.clone());
            } else if f.ends_with(".nfo") {
                if !nfo_file.exists() {
                    nfo_file = fp.to_path_buf();
                }
            } else if mime.type_() == mime_guess::mime::VIDEO {
                meta_data.path = osstr_to_string(fp.clone().into_os_string());
            }
            data.special_files_insert(fp.to_path_buf());
        }
    }
    if meta_data.path.len() == 0 {
        return ControlFlow::Break(());
    }
    if !nfo_file.exists() {
        return ControlFlow::Break(());
    }

    if meta_data.poster.len() > 0 {
        let poster = PathBuf::from(&meta_data.poster);
        if poster.exists() {
            let thumb = crate::thumbnails::create_thumbnail(&poster, 256);
            if thumb.len() > 0 {
                meta_data.thumb = thumb.clone();
            }
        }
    }
    let statdata = match std::fs::metadata(&meta_data.path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!(
                "failed to read metadata for entry at {:?}: {}",
                meta_data.path,
                err
            );
            return ControlFlow::Break(());
        }
    };
    let item = crate::parsers::item_from_nfo(
        nfo_file,
        &mut meta_data,
        &statdata,
        sizes,
        data,
        sql_connection.clone(),
        false,
    );
    data.items_push(item);
    ControlFlow::Continue(())
}

pub fn parse_tv_show_episodes(
    nfo_file: PathBuf,
    meta_data: &mut crate::sql::CollectionMetadata,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) {
    let path = match nfo_file.parent() {
        Some(ok) => ok.to_path_buf(),
        None => return,
    };
    if data.special_files_contains(path.clone()) {
        return;
    }
    let mut contents = Vec::new();
    let mut nfo_names = Vec::new();
    let mut dirs = Vec::new();
    match std::fs::read_dir(&path) {
        Ok(entries) => {
            for entry_res in entries {
                let entry = match entry_res {
                    Ok(ok) => ok,
                    Err(err) => {
                        log::warn!("failed to read entry in {:?}: {}", path, err);
                        continue;
                    }
                };
                let path = entry.path();
                let filename = match path.file_name() {
                    Some(name) => name,
                    None => continue,
                };
                if osstr_to_string(filename.to_os_string()).starts_with(".") {
                    continue;
                }
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    contents.push(path);
                }
            }
            for path in contents.iter() {
                let f = osstr_to_string(path.clone().into_os_string()).to_ascii_lowercase();
                if f.contains("-poster.") {
                    // Season poster
                    //meta_data.poster = osstr_to_string(path.clone().into_os_string());
                    //poster += 1;
                } else if f.contains("poster.") {
                    // Series poster
                    meta_data.poster = osstr_to_string(path.clone().into_os_string());
                } else if osstr_to_string(path.clone().into_os_string())
                    .to_ascii_lowercase()
                    .ends_with("tvshow.nfo")
                {
                    continue;
                } else if f.contains(".nfo") {
                    // episode in the current directory
                    if let Some(basename) = path.file_stem() {
                        nfo_names.push(osstr_to_string(basename.to_os_string()));
                    } else {
                        nfo_names.push(osstr_to_string(path.clone().into_os_string()));
                    }
                }
                data.special_files_insert(path.to_path_buf());
            }
        }
        Err(err) => {
            log::warn!("failed to read directory {:?}: {}", path, err);
        }
    }
    // parse the tvshow.nfo
    let mut videometadata = crate::sql::VideoMetadata {
        ..Default::default()
    };
    parse_nfo(&nfo_file, &mut videometadata);
    meta_data.description = videometadata.description;
    meta_data.name = videometadata.title;
    // parse episodes in subdirectories
    // searches for NFO files to send to the general video functionality
    contents.clear();
    for path in dirs {
        match std::fs::read_dir(path.clone()) {
            Ok(entries) => {
                for entry_res in entries {
                    let entry = match entry_res {
                        Ok(ok) => ok,
                        Err(err) => {
                            log::warn!("failed to read entry in {:?}: {}", path, err);
                            continue;
                        }
                    };
                    let filepath = entry.path();
                    let filename = match filepath.file_name() {
                        Some(name) => name,
                        None => continue,
                    };
                    if osstr_to_string(filename.to_os_string()).starts_with(".") {
                        continue;
                    }
                    contents.push(filepath);
                }
                for filepath in contents.iter() {
                    if data.known_files_contains(filepath.to_owned()) {
                        continue;
                    }

                    let f = osstr_to_string(filepath.clone().into_os_string()).to_ascii_lowercase();
                    if f.contains(".nfo") {
                        // episode in the current directory
                        if let Some(basename) = filepath.file_stem() {
                            nfo_names.push(osstr_to_string(path.join(basename).into_os_string()));
                        } else {
                            nfo_names.push(osstr_to_string(filepath.clone().into_os_string()));
                        }
                    }
                }
            }
            Err(err) => {
                log::warn!("failed to read directory {:?}: {}", path, err);
            }
        }
    }
    for nfo in nfo_names {
        let localdata = crate::scanmetadata::ScanMetaData::new();
        let knownfiles = data.known_files_clone();
        let specialfiles = data.special_files_clone();
        localdata.known_files_extend(knownfiles);
        localdata.special_files_extend(specialfiles);
        let ret = scan_nfos_in_dir(
            nfo,
            &mut contents,
            &localdata,
            sizes,
            sql_connection.clone(),
        );
        if ret == ControlFlow::Break(()) {
            continue;
        }
        if let Some(item) = localdata.items_pop() {
            if let Some(video) = item.video_opt.clone() {
                //let file = crate::sql::file_by_id(sql_connection.clone(), video.id as i64);
                let e = crate::sql::EpisodeMetadata {
                    series: video.season,
                    episode: video.episode,
                    file_id: video.id,
                    path: PathBuf::from(&video.path),
                    poster: video.poster,
                    thumb: video.thumb,
                    title: video.name,
                };
                meta_data.episodes.push(e);
            }
        }
        let knownfiles2 = localdata.known_files_clone();
        let specialfiles2 = localdata.special_files_clone();
        data.known_files_extend(knownfiles2);
        data.special_files_extend(specialfiles2);
    }
    if !PathBuf::from(&nfo_file).exists() {
        return;
    }
    data.special_files_insert(path.clone());
    for p in contents {
        if !data.special_files_contains(p.clone()) {
            data.special_files_insert(p);
        }
    }
    let thumbpath = PathBuf::from(&meta_data.poster);
    if thumbpath.exists() {
        let thumb = crate::thumbnails::create_thumbnail(&thumbpath, 256);
        if thumb.len() > 0 {
            meta_data.thumb = thumb.clone();
        }
    }
}

/// create an item to put into our tabmodel from a video with NFO metadata
pub fn item_from_tvshow(
    nfo_file: PathBuf,
    metadata: &mut crate::sql::CollectionMetadata,
    statdata: &std::fs::Metadata,
    sizes: IconSizes,
    data: &crate::scanmetadata::ScanMetaData,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    from_db: bool,
) -> Item {
    let filepath = PathBuf::from(&metadata.path);
    let basename;
    if let Some(bn) = filepath.file_stem() {
        basename = osstr_to_string(bn.to_os_string());
    } else {
        if let Some(bn) = filepath.file_name() {
            basename = osstr_to_string(bn.to_os_string());
        } else {
            basename = osstr_to_string(filepath.clone().into_os_string());
        }
    }
    if !from_db {
        let mut video = crate::sql::VideoMetadata {
            ..Default::default()
        };
        if data.known_files_contains(filepath.clone()) {
            let mut refresh = false;
            if let Ok(modified) = statdata.modified() {
                if let Ok(new_date) = modified.duration_since(UNIX_EPOCH) {
                    let filedata = &data.known_files_get(filepath.clone());
                    let new_seconds_since_epoch = new_date.as_secs();
                    if new_seconds_since_epoch > filedata.modification_time {
                        refresh = true;
                    }
                }
                if refresh {
                    // file is newer
                    parse_tv_show_episodes(
                        nfo_file.clone(),
                        metadata,
                        data,
                        sizes,
                        sql_connection.clone(),
                    );
                    parse_nfo(&nfo_file, &mut video);
                    if video.date.num_days_from_ce() < 100 {
                        if let Ok(created) = statdata.created() {
                            if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                                if let Some(ndt) =
                                    chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                                {
                                    video.date = ndt.date_naive();
                                }
                            }
                        }
                    }
                    metadata.thumb =
                        crate::thumbnails::create_thumbnail(&PathBuf::from(&metadata.poster), 254);
                    crate::sql::update_collection(sql_connection.clone(), metadata, statdata, data);
                } else {
                    *metadata = crate::sql::collection(
                        sql_connection.clone(),
                        &osstr_to_string(metadata.path.clone().into_os_string()),
                        data,
                    );
                }
            }
        } else {
            parse_tv_show_episodes(
                nfo_file.clone(),
                metadata,
                data,
                sizes,
                sql_connection.clone(),
            );
            video.poster = metadata.poster.clone();
            parse_nfo(&nfo_file, &mut video);
            if video.date.num_days_from_ce() < 100 {
                if let Ok(created) = statdata.created() {
                    if let Ok(nsecs) = created.duration_since(UNIX_EPOCH) {
                        if let Some(ndt) =
                            chrono::DateTime::from_timestamp(nsecs.as_secs() as i64, 0)
                        {
                            video.date = ndt.date_naive();
                        }
                    }
                }
            }
            metadata.thumb =
                crate::thumbnails::create_thumbnail(&PathBuf::from(&video.poster), 254);
            crate::sql::insert_collection(sql_connection.clone(), metadata, statdata, data);
        }
    } else {
        *metadata = crate::sql::collection(
            sql_connection.clone(),
            &osstr_to_string(metadata.path.clone().into_os_string()),
            data,
        );
    }

    let name;
    if metadata.name.len() == 0 {
        name = basename.clone();
    } else {
        name = metadata.name.clone();
    }

    let display_name = Item::display_name(&name);

    let hidden = name.starts_with(".") || hidden_attribute(&statdata);

    let (mime, icon_handle_grid, icon_handle_list, icon_handle_list_condensed) = {
        let thumbpath = PathBuf::from(&metadata.thumb);
        let thumbmime = mime_for_path(thumbpath.clone());
        let filemime = mime_for_path(filepath.clone());
        if metadata.thumb.len() > 0 {
            (
                filemime.clone(),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
            )
        } else {
            (
                filemime.clone(),
                mime_icon(thumbmime.clone(), sizes.grid()),
                mime_icon(thumbmime.clone(), sizes.list()),
                mime_icon(thumbmime.clone(), sizes.list_condensed()),
            )
        }
    };

    let open_with = mime_apps(&mime);

    let children = if statdata.is_dir() {
        //TODO: calculate children in the background (and make it cancellable?)
        match std::fs::read_dir(PathBuf::from(&metadata.path)) {
            Ok(entries) => entries.count(),
            Err(err) => {
                log::warn!("failed to read directory {:?}: {}", metadata.path, err);
                0
            }
        }
    } else {
        0
    };

    let dir_size = if statdata.is_dir() {
        DirSize::Calculating(crate::operation::controller::Controller::new())
    } else {
        DirSize::NotDirectory
    };

    let mut item = Item {
        name,
        display_name,
        metadata: ItemMetadata::Path {
            metadata: statdata.clone(),
            children,
        },
        hidden,
        location_opt: Some(Location::Collection(metadata.clone())),
        mime,
        icon_handle_grid,
        icon_handle_list,
        icon_handle_list_condensed,
        open_with,
        thumbnail_opt: Some(ItemThumbnail::NotImage),
        button_id: widget::Id::unique(),
        pos_opt: Cell::new(None),
        rect_opt: Cell::new(None),
        selected: false,
        highlighted: false,
        overlaps_drag_rect: false,
        dir_size,
        image_opt: None,
        video_opt: None,
        audio_opt: None,
        collection_opt: Some(metadata.clone()),
    };
    item.thumbnail_opt = Some(crate::tab::ItemThumbnail::new(item.clone()));
    item
}

pub fn scan_tvshow(
    path: PathBuf,
    data: &crate::scanmetadata::ScanMetaData,
    sizes: IconSizes,
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> ControlFlow<()> {
    let nfo_file = path.clone().join("tvshow.nfo");
    let mut meta_data = crate::sql::CollectionMetadata {
        ..Default::default()
    };
    meta_data.path = path.clone();

    let statdata = match std::fs::metadata(&meta_data.path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!(
                "failed to read metadata for entry at {:?}: {}",
                meta_data.path,
                err
            );
            return ControlFlow::Break(());
        }
    };
    let item = crate::parsers::item_from_tvshow(
        nfo_file,
        &mut meta_data,
        &statdata,
        sizes,
        data,
        sql_connection.clone(),
        false,
    );
    data.items_push(item);
    // test if we have a single movie with NFO in this dir

    ControlFlow::Continue(())
}

pub fn item_from_collection_episode(
    metadata: &mut crate::sql::VideoMetadata,
    sizes: IconSizes,
    data: &crate::scanmetadata::ScanMetaData,
) {
    let filepath = PathBuf::from(&metadata.path);
    let file_stem = match filepath.clone().file_stem() {
        Some(stem) => crate::parsers::osstr_to_string(stem.to_os_string()),
        None => return,
    };
    let mut nfo_file = file_stem.clone();
    nfo_file.extend(".nfo".to_string().chars());
    let statdata = match std::fs::metadata(&metadata.path) {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!(
                "failed to read metadata for entry at {:?}: {}",
                metadata.path,
                err
            );
            return;
        }
    };
    let name;
    if metadata.title.len() == 0 {
        name = format!(
            "S{:02}E{:02} {}",
            metadata.season, metadata.episode, metadata.name
        );
    } else {
        name = format!(
            "S{:02}E{:02} {}",
            metadata.season, metadata.episode, metadata.title
        );
    }

    let display_name = Item::display_name(&name);

    let hidden = name.starts_with(".") || hidden_attribute(&statdata);

    let (mime, icon_handle_grid, icon_handle_list, icon_handle_list_condensed) = {
        let thumbpath = PathBuf::from(&metadata.thumb);
        let thumbmime = mime_for_path(thumbpath.clone());
        let filemime = mime_for_path(filepath.clone());
        if metadata.thumb.len() > 0 {
            (
                filemime.clone(),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
                widget::icon::from_path(thumbpath.clone()),
            )
        } else {
            (
                filemime.clone(),
                mime_icon(thumbmime.clone(), sizes.grid()),
                mime_icon(thumbmime.clone(), sizes.list()),
                mime_icon(thumbmime.clone(), sizes.list_condensed()),
            )
        }
    };

    let open_with = mime_apps(&mime);

    let children = if statdata.is_dir() {
        //TODO: calculate children in the background (and make it cancellable?)
        match std::fs::read_dir(PathBuf::from(&metadata.path)) {
            Ok(entries) => entries.count(),
            Err(err) => {
                log::warn!("failed to read directory {:?}: {}", metadata.path, err);
                0
            }
        }
    } else {
        0
    };

    let dir_size = if statdata.is_dir() {
        DirSize::Calculating(crate::operation::controller::Controller::new())
    } else {
        DirSize::NotDirectory
    };

    let mut item = Item {
        name,
        display_name,
        metadata: ItemMetadata::Path {
            metadata: statdata.clone(),
            children,
        },
        hidden,
        location_opt: Some(Location::Path(PathBuf::from(&metadata.path))),
        mime,
        icon_handle_grid,
        icon_handle_list,
        icon_handle_list_condensed,
        open_with,
        thumbnail_opt: Some(ItemThumbnail::NotImage),
        button_id: widget::Id::unique(),
        pos_opt: Cell::new(None),
        rect_opt: Cell::new(None),
        selected: false,
        highlighted: false,
        overlaps_drag_rect: false,
        dir_size,
        image_opt: None,
        video_opt: Some(metadata.clone()),
        audio_opt: None,
        collection_opt: None,
    };
    item.thumbnail_opt = Some(crate::tab::ItemThumbnail::new(item.clone()));
    data.items_push(item);
}
