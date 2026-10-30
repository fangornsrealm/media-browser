//! Search implementation for the metadata database.
//!
//! Split out of `sql.rs` (Phase 3 — modularization). Re-exported from the
//! parent module via `pub use search::*;` so all existing `crate::sql::`
//! call-sites and `mod tests` references keep working unchanged.

use super::{audio, image, video};
use super::{
    escape_like_term, file_by_id, AudioMetadata, FileMetadata, ImageMetadata, SearchData,
    VideoMetadata,
};
use chrono::Timelike;
use rusqlite::types::Value;
use std::path::PathBuf;

fn search_video_metadata(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    query: String,
    params: Vec<Value>,
) -> (Vec<VideoMetadata>, Vec<FileMetadata>) {
    let mut files = Vec::new();
    let mut videos = Vec::new();
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return (videos, files);
        }
    };
    match connection.prepare(&query) {
        Ok(mut statement) => {
            match statement.query(rusqlite::params_from_iter(params.iter())) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut video = VideoMetadata::default();
                                let mut file = FileMetadata::default();
                                let video_id: i64 = match row.get(0) {
                                    Ok(val) => val,
                                    Err(error) => {
                                        log::error!("Failed to read id for video: {}", error);
                                        continue;
                                    }
                                };
                                video.id = video_id as u32;
                                file.metadata_id = video_id;
                                if let Ok(s) = row.get::<_, String>(1) {
                                    video.path = s.clone();
                                    file.filepath = PathBuf::from(s);
                                }
                                if let Ok(v) = row.get(2) {
                                    file.creation_time = v;
                                }
                                if let Ok(v) = row.get(3) {
                                    file.modification_time = v;
                                }
                                if let Ok(v) = row.get(4) {
                                    file.file_type = v;
                                }
                                if let Ok(v) = row.get(5) {
                                    video.name = v;
                                }
                                if let Ok(v) = row.get(6) {
                                    video.title = v;
                                }
                                if let Ok(v) = row.get(7) {
                                    video.date = v;
                                }
                                if let Ok(v) = row.get(8) {
                                    video.duration = v;
                                }
                                if let Ok(v) = row.get(9) {
                                    video.width = v;
                                }
                                if let Ok(v) = row.get(10) {
                                    video.height = v;
                                }
                                if let Ok(v) = row.get(11) {
                                    video.framerate = v;
                                }
                                if let Ok(v) = row.get(12) {
                                    video.description = v;
                                }
                                videos.push(video);
                                files.push(file);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from indices: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!(
                        "could not read line from videostore_indices database: {}",
                        err
                    );
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    drop(connection);
    (videos, files)
}

pub fn search_video(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    search: &SearchData,
) -> (Vec<VideoMetadata>, Vec<FileMetadata>) {
    let mut files: Vec<FileMetadata> = Vec::new();
    let mut videos: Vec<VideoMetadata> = Vec::new();
    let mut used_files: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    if !search.video {
        return (videos, files);
    }
    if search.title {
        let query = "SELECT vm.video_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM video_metadata vm INNER JOIN file_metadata fm ON fm.metadata_id = vm.video_id WHERE vm.title LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_video_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                videos.push(newvideos[i].clone());
            }
        }
    }
    if search.description {
        let query = "SELECT vm.video_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM video_metadata vm INNER JOIN file_metadata fm ON fm.metadata_id = vm.video_id WHERE vm.description LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_video_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                videos.push(newvideos[i].clone());
            }
        }
    }
    if search.duration && search.from_value != 0 {
        let (query, params);
        if search.to_value != 0 {
            (query, params) = (
                "SELECT vm.video_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM video_metadata vm INNER JOIN file_metadata fm ON fm.metadata_id = vm.video_id WHERE vm.duration > ? AND vm.duration < ?",
                vec![
                    Value::Integer(search.from_value as i64),
                    Value::Integer(search.to_value as i64),
                ],
            );
        } else {
            (query, params) = (
                "SELECT vm.video_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM video_metadata vm INNER JOIN file_metadata fm ON fm.metadata_id = vm.video_id WHERE vm.duration > ?",
                vec![Value::Integer(search.from_value as i64)],
            );
        }
        let (newvideos, newfiles) =
            search_video_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                videos.push(newvideos[i].clone());
            }
        }
    }
    if search.actor {
        let query = "SELECT a.video_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM actors a INNER JOIN people p ON p.person_id = a.actor_id INNER JOIN video_metadata vm ON vm.video_id = a.video_id INNER JOIN file_metadata fm ON fm.metadata_id = a.video_id WHERE p.person_name LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_video_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                videos.push(newvideos[i].clone());
            }
        }
    }
    if search.director {
        let query = "SELECT d.video_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM directors d INNER JOIN people p ON p.person_id = d.director_id INNER JOIN video_metadata vm ON vm.video_id = d.video_id INNER JOIN file_metadata fm ON fm.metadata_id = d.video_id WHERE p.person_name LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_video_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                videos.push(newvideos[i].clone());
            }
        }
    }
    if search.release_date {
        let (query, params);
        if search.to_date != 0 {
            (query, params) = (
                "SELECT vm.video_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM video_metadata vm INNER JOIN file_metadata fm ON fm.metadata_id = vm.video_id WHERE vm.released > ? AND vm.released < ?",
                vec![
                    Value::Integer(search.from_date),
                    Value::Integer(search.to_date),
                ],
            );
        } else {
            (query, params) = (
                "SELECT vm.video_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM video_metadata vm INNER JOIN file_metadata fm ON fm.metadata_id = vm.video_id WHERE vm.released > ?",
                vec![Value::Integer(search.from_date)],
            );
        }
        let (newvideos, newfiles) =
            search_video_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                videos.push(newvideos[i].clone());
            }
        }
    }
    if search.tags {
        let query = "SELECT tmm.media_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, vm.name, vm.title, vm.released, vm.duration, vm.width, vm.height, vm.framerate, vm.description FROM tags_media_map tmm INNER JOIN tags t ON tmm.tagmap_id = t.tag_id INNER JOIN video_metadata vm ON vm.video_id = tmm.media_id INNER JOIN file_metadata fm ON fm.metadata_id = tmm.media_id WHERE t.tag LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_video_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                videos.push(newvideos[i].clone());
            }
        }
    }

    (videos, files)
}

fn search_audio_metadata(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    query: String,
    params: Vec<Value>,
) -> (Vec<AudioMetadata>, Vec<FileMetadata>) {
    let mut files = Vec::new();
    let mut audios = Vec::new();
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return (audios, files);
        }
    };

    match connection.prepare(&query) {
        Ok(mut statement) => {
            match statement.query(rusqlite::params_from_iter(params.iter())) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut audio = AudioMetadata::default();
                                let mut file = FileMetadata::default();
                                let audio_id: i64 = match row.get(0) {
                                    Ok(val) => val,
                                    Err(error) => {
                                        log::error!("Failed to read id for audio: {}", error);
                                        continue;
                                    }
                                };
                                audio.id = audio_id as u32;
                                file.metadata_id = audio_id;
                                if let Ok(s) = row.get::<_, String>(1) {
                                    audio.path = s.clone();
                                    file.filepath = PathBuf::from(s);
                                }
                                if let Ok(v) = row.get(2) {
                                    file.creation_time = v;
                                }
                                if let Ok(v) = row.get(3) {
                                    file.modification_time = v;
                                }
                                if let Ok(v) = row.get(4) {
                                    file.file_type = v;
                                }
                                if let Ok(v) = row.get(5) {
                                    audio.name = v;
                                }
                                if let Ok(v) = row.get(6) {
                                    audio.title = v;
                                }
                                if let Ok(v) = row.get(7) {
                                    audio.date = v;
                                }
                                if let Ok(v) = row.get(8) {
                                    audio.duration = v;
                                }
                                if let Ok(v) = row.get(9) {
                                    audio.genre = v;
                                }
                                if let Ok(v) = row.get(10) {
                                    audio.composer = v;
                                }
                                audios.push(audio);
                                files.push(file);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from indices: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!(
                        "could not read line from audiostreamstore_indices database: {}",
                        err
                    );
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    drop(connection);
    (audios, files)
}

pub fn search_audio(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    search: &SearchData,
) -> (Vec<AudioMetadata>, Vec<FileMetadata>) {
    let mut files: Vec<FileMetadata> = Vec::new();
    let mut audios: Vec<AudioMetadata> = Vec::new();
    let mut used_files: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    if !search.audio {
        return (audios, files);
    }
    if search.title {
        let query = "SELECT am.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM audio_metadata am INNER JOIN file_metadata fm ON fm.metadata_id = am.audio_id WHERE am.title LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.duration && search.from_value != 0 {
        let (query, params);
        if search.to_value != 0 {
            (query, params) = (
                "SELECT am.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM audio_metadata am INNER JOIN file_metadata fm ON fm.metadata_id = am.audio_id WHERE am.duration > ? AND am.duration < ?",
                vec![
                    Value::Integer(search.from_value as i64 / 1000000),
                    Value::Integer(search.to_value as i64 / 1000000),
                ],
            );
        } else {
            (query, params) = (
                "SELECT am.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM audio_metadata am INNER JOIN file_metadata fm ON fm.metadata_id = am.audio_id WHERE am.duration > ?",
                vec![Value::Integer(search.from_value as i64 / 1000000)],
            );
        }
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.artist {
        let query = "SELECT aam.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM artist_audio_map aam INNER JOIN artists a ON a.artist_id = aam.artist_id INNER JOIN audio_metadata am ON am.audio_id = aam.audio_id INNER JOIN file_metadata fm ON fm.metadata_id = aam.audio_id WHERE a.artist_name LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.title {
        let query = "SELECT aam.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM album_audio_map aam INNER JOIN albums al ON al.album_id = aam.album_id INNER JOIN audio_metadata am ON am.audio_id = aam.audio_id INNER JOIN file_metadata fm ON fm.metadata_id = aam.audio_id WHERE al.album_name LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.album_artist {
        let query = "SELECT aam.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM albumartist_audio_map aam INNER JOIN artists a ON a.artist_id = aam.albumartist_id INNER JOIN audio_metadata am ON am.audio_id = aam.audio_id INNER JOIN file_metadata fm ON fm.metadata_id = aam.audio_id WHERE a.artist_name LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.album {
        let query = "SELECT aam.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM album_audio_map aam INNER JOIN albums al ON al.album_id = aam.album_id INNER JOIN audio_metadata am ON am.audio_id = aam.audio_id INNER JOIN file_metadata fm ON fm.metadata_id = aam.audio_id WHERE al.album_name LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.composer {
        let query = "SELECT am.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM audio_metadata am INNER JOIN file_metadata fm ON fm.metadata_id = am.audio_id WHERE am.composer LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.genre {
        let query = "SELECT am.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM audio_metadata am INNER JOIN file_metadata fm ON fm.metadata_id = am.audio_id WHERE am.genre LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.release_date {
        let (query, params);
        if search.to_date != 0 {
            (query, params) = (
                "SELECT am.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM audio_metadata am INNER JOIN file_metadata fm ON fm.metadata_id = am.audio_id WHERE am.released > ? AND am.released < ?",
                vec![
                    Value::Integer(search.from_date),
                    Value::Integer(search.to_date),
                ],
            );
        } else {
            (query, params) = (
                "SELECT am.audio_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM audio_metadata am INNER JOIN file_metadata fm ON fm.metadata_id = am.audio_id WHERE am.released > ?",
                vec![Value::Integer(search.from_date)],
            );
        }
        let (newvideos, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newvideos[i].clone());
            }
        }
    }
    if search.tags {
        let query = "SELECT tmm.media_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, am.name, am.title, am.released, am.duration, am.genre, am.composer FROM tags_media_map tmm INNER JOIN tags t ON tmm.tagmap_id = t.tag_id INNER JOIN audio_metadata am ON am.audio_id = tmm.media_id INNER JOIN file_metadata fm ON fm.metadata_id = tmm.media_id WHERE t.tag LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newaudios, newfiles) =
            search_audio_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                audios.push(newaudios[i].clone());
            }
        }
    }

    (audios, files)
}

fn search_image_metadata(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    query: String,
    params: Vec<Value>,
) -> (Vec<ImageMetadata>, Vec<FileMetadata>) {
    let mut files = Vec::new();
    let mut images = Vec::new();
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return (images, files);
        }
    };
    match connection.prepare(&query) {
        Ok(mut statement) => {
            match statement.query(rusqlite::params_from_iter(params.iter())) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut image = ImageMetadata::default();
                                let mut file = FileMetadata::default();
                                let image_id: i64 = match row.get(0) {
                                    Ok(val) => val,
                                    Err(error) => {
                                        log::error!("Failed to read id for image: {}", error);
                                        continue;
                                    }
                                };
                                image.id = image_id as u32;
                                file.metadata_id = image_id;
                                if let Ok(s) = row.get::<_, String>(1) {
                                    image.path = s.clone();
                                    file.filepath = PathBuf::from(s);
                                }
                                if let Ok(v) = row.get(2) {
                                    file.creation_time = v;
                                }
                                if let Ok(v) = row.get(3) {
                                    file.modification_time = v;
                                }
                                if let Ok(v) = row.get(4) {
                                    file.file_type = v;
                                }
                                if let Ok(v) = row.get(5) {
                                    image.name = v;
                                }
                                if let Ok(v) = row.get(6) {
                                    image.path = v;
                                }
                                if let Ok(v) = row.get(7) {
                                    image.date = v;
                                }
                                if let Ok(v) = row.get(8) {
                                    image.resized = v;
                                }
                                if let Ok(v) = row.get(9) {
                                    image.thumb = v;
                                }
                                if let Ok(v) = row.get(10) {
                                    image.width = v;
                                }
                                if let Ok(v) = row.get(11) {
                                    image.height = v;
                                }
                                if let Ok(v) = row.get(12) {
                                    image.photographer = v;
                                }
                                if let Ok(v) = row.get(13) {
                                    image.lense_model = v;
                                }
                                if let Ok(v) = row.get(14) {
                                    image.focal_length = v;
                                }
                                if let Ok(v) = row.get(15) {
                                    image.exposure_time = v;
                                }
                                if let Ok(v) = row.get(16) {
                                    image.fnumber = v;
                                }
                                if let Ok(v) = row.get(17) {
                                    image.gps_latitude = v;
                                }
                                if let Ok(v) = row.get(18) {
                                    image.gps_longitude = v;
                                }
                                if let Ok(v) = row.get(19) {
                                    image.gps_altitude = v;
                                }
                                images.push(image);
                                files.push(file);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from indices: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from imagestore database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    drop(connection);
    (images, files)
}

pub fn search_image(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    search: &SearchData,
) -> (Vec<ImageMetadata>, Vec<FileMetadata>) {
    let mut files = Vec::new();
    let mut images: Vec<ImageMetadata> = Vec::new();

    let mut used_files: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    if !search.image {
        return (images, files);
    }
    if search.title {
        let query = "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.name LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.lense_model {
        let query = "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.LenseModel LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.focal_length {
        let query = "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.Focallength LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.exposure_time {
        let query = "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.Exposuretime LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.fnumber {
        let query = "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.FNumber LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.gps_latitude && search.from_value != 0 {
        let (query, params);
        if search.to_value != 0 {
            (query, params) = (
                "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.GPSLatitude > ? AND im.GPSLatitude < ?",
                vec![
                    Value::Integer(search.from_value as i64 / 1000000),
                    Value::Integer(search.to_value as i64 / 1000000),
                ],
            );
        } else {
            (query, params) = (
                "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.GPSLatitude > ?",
                vec![Value::Integer(search.from_value as i64 / 1000000)],
            );
        }
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.gps_longitude && search.from_value != 0 {
        let (query, params);
        if search.to_value != 0 {
            (query, params) = (
                "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.GPSLongitude > ? AND im.GPSLongitude < ?",
                vec![
                    Value::Integer(search.from_value as i64 / 1000000),
                    Value::Integer(search.to_value as i64 / 1000000),
                ],
            );
        } else {
            (query, params) = (
                "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.GPSLongitude > ?",
                vec![Value::Integer(search.from_value as i64 / 1000000)],
            );
        }
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.gps_altitude && search.from_value != 0 {
        let (query, params);
        if search.to_value != 0 {
            (query, params) = (
                "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.GPSAltitude > ? AND im.GPSAltitude < ?",
                vec![
                    Value::Integer(search.from_value as i64 / 1000000),
                    Value::Integer(search.to_value as i64 / 1000000),
                ],
            );
        } else {
            (query, params) = (
                "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.GPSAltitude > ?",
                vec![Value::Integer(search.from_value as i64 / 1000000)],
            );
        }
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.release_date && search.from_date != 0 {
        let (query, params);
        if search.to_date != 0 {
            (query, params) = (
                "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.created > ? AND im.created < ?",
                vec![
                    Value::Integer(search.from_date),
                    Value::Integer(search.to_date),
                ],
            );
        } else {
            (query, params) = (
                "SELECT im.image_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM image_metadata im INNER JOIN file_metadata fm ON fm.metadata_id = im.image_id WHERE im.created > ?",
                vec![Value::Integer(search.from_date)],
            );
        }
        let (newvideos, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newvideos[i].clone());
            }
        }
    }
    if search.tags {
        let query = "SELECT tmm.media_id, fm.filepath, fm.creation_time, fm.modification_time, fm.file_type, im.name, im.path, im.created, im.resized, im.thumb, im.width, im.height, im.Photographer, im.LenseModel, im.Focallength, im.Exposuretime, im.FNumber, im.GPSLatitude, im.GPSLongitude, im.GPSAltitude FROM tags_media_map tmm INNER JOIN tags t ON tmm.tagmap_id = t.tag_id INNER JOIN image_metadata im ON im.image_id = tmm.media_id INNER JOIN file_metadata fm ON fm.metadata_id = tmm.media_id WHERE t.tag LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let (newimages, newfiles) =
            search_image_metadata(sql_connection.clone(), query.to_string(), params);
        for i in 0..newfiles.len() {
            if !used_files.contains(&newfiles[i].filepath) {
                used_files.insert(newfiles[i].filepath.clone());
                files.push(newfiles[i].clone());
                images.push(newimages[i].clone());
            }
        }
    }

    (images, files)
}

fn search_file_metadata(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    query: String,
    params: Vec<Value>,
) -> Vec<FileMetadata> {
    let mut files = Vec::new();
    let mut ids = Vec::new();
    let mut file_id;

    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return files;
        }
    };
    match connection.prepare(&query) {
        Ok(mut statement) => {
            match statement.query(rusqlite::params_from_iter(params.iter())) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get(0) {
                                Ok(val) => {
                                    file_id = val;
                                    ids.push(file_id);
                                }
                                Err(error) => {
                                    log::error!("Failed to read id for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from indices: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!(
                        "could not read line from videostore_indices database: {}",
                        err
                    );
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    drop(connection);
    for file_id in ids {
        let filedata = file_by_id(sql_connection.clone(), file_id);
        files.push(filedata);
    }
    files
}

fn stuff_items(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    search: &SearchData,
    data: &crate::scanmetadata::ScanMetaData,
    file: FileMetadata,
) {
    if !data.special_files_contains(file.filepath.clone()) {
        data.special_files_insert(file.filepath.clone());
        if file.file_type == 2 && search.video {
            let pathstr = crate::parsers::osstr_to_string(file.filepath.clone().into_os_string());
            let mut video = video(sql_connection.clone(), &pathstr, data);
            if let Ok(metadata) = std::fs::metadata(file.filepath.clone()) {
                let item = crate::parsers::item_from_nfo(
                    PathBuf::new(),
                    &mut video,
                    &metadata,
                    crate::config::IconSizes::default(),
                    data,
                    sql_connection.clone(),
                    true,
                );
                data.items_push(item);
            }
        }
        if file.file_type == 3 && search.audio {
            let pathstr = crate::parsers::osstr_to_string(file.filepath.clone().into_os_string());
            let mut audio = audio(sql_connection.clone(), &pathstr, data);
            if let Ok(metadata) = std::fs::metadata(file.filepath.clone()) {
                let item = crate::parsers::item_from_audiotags(
                    file.filepath.clone(),
                    &data,
                    &mut audio,
                    &metadata,
                    crate::config::IconSizes::default(),
                    sql_connection.clone(),
                    true,
                );
                data.items_push(item);
            }
        }
        if file.file_type == 1 && search.image {
            let pathstr = crate::parsers::osstr_to_string(file.filepath.clone().into_os_string());
            let mut image = image(sql_connection.clone(), &pathstr, data);
            if let Ok(metadata) = std::fs::metadata(file.filepath.clone()) {
                let item = crate::parsers::item_from_exif(
                    file.filepath.clone(),
                    &mut image,
                    &metadata,
                    crate::config::IconSizes::default(),
                    data,
                    sql_connection.clone(),
                    true,
                );
                data.items_push(item);
            }
        }
    }
}

/// date has to be in the following format
/// YYYY-MM-DDThh:mm:ss (1996-12-19T16:39:57)
pub fn string_to_linux_time(date: &str) -> i64 {
    let mut linuxtime = 0;
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(date) {
        let naive = date.naive_utc();
        linuxtime = naive.and_utc().timestamp();
    }

    linuxtime
}

pub fn search_items(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    s: &SearchData,
) -> Vec<crate::tab::Item> {
    let data = crate::scanmetadata::ScanMetaData::new();
    let mut search = s.to_owned();
    // if the search term was entered into the to box switch the boxes
    if search.from_string.trim().len() == 0 && search.to_string.len() > 0 {
        search.from_string = search.to_string.clone();
        search.to_string.clear();
    }
    // if the string is a date, convert it
    if search.from_string.len() > 0 && search.from_date == 0 {
        let lt = crate::sql::string_to_linux_time(&search.from_string);
        if lt > 0 {
            search.from_date = lt as i64;
            search.from_string.clear();
        }
    }
    // if the string is a number, convert it
    if search.from_string.len() > 0 && search.from_value == 0 {
        match search.from_string.parse::<f32>() {
            Ok(float) => {
                search.from_value = (float * 1000000.0) as u64;
                search.from_string.clear();
            }
            Err(_) => {}
        }
    }
    // if the string is a date, convert it
    if search.to_string.len() > 0 && search.to_date == 0 {
        let lt = crate::sql::string_to_linux_time(&search.to_string);
        if lt > 0 {
            search.to_date = lt as i64;
            search.to_string.clear();
        }
    }
    // if the string is a number, convert it
    if search.to_string.len() > 0 && search.to_value == 0 {
        match search.to_string.parse::<f32>() {
            Ok(float) => {
                search.to_value = (float * 1000000.0) as u64;
                search.to_string.clear();
            }
            Err(_) => {}
        }
    }
    // if we are searching for dates, activate them if necessary
    if (search.from_date > 0 || search.to_date > 0)
        && !search.creation_date
        && !search.modification_date
        && !search.release_date
    {
        search.creation_date = true;
        search.modification_date = true;
        search.release_date = true;
    }

    // if we search for a single date, search for a whole day.
    if search.from_date > 0 && search.to_date == 0 {
        if let Some(datetime) = chrono::DateTime::from_timestamp(search.from_date, 0) {
            if let Some(starthour) =
                datetime.checked_add_signed(chrono::Duration::hours(-(datetime.hour() as i64)))
            {
                if let Some(startminute) = starthour
                    .checked_add_signed(chrono::Duration::minutes(-(datetime.minute() as i64)))
                {
                    if let Some(startsecond) = startminute
                        .checked_add_signed(chrono::Duration::minutes(-(datetime.second() as i64)))
                    {
                        let start = startsecond.timestamp();
                        if let Some(enddatetime) =
                            datetime.checked_add_signed(chrono::Duration::days(1))
                        {
                            search.from_date = start;
                            search.to_date = enddatetime.timestamp();
                        }
                    }
                }
            }
        }
    }

    if search.from_string.len() == 0
        && search.to_string.len() == 0
        && search.from_value == 0
        && search.to_value == 0
        && search.from_date == 0
        && search.to_date == 0
    {
        log::error!("Please enter some value to search for!");
        return Vec::new();
    }
    // if we don't search for anything, exit with warning
    if !search.image && !search.video && !search.audio {
        log::error!("Please select some media to search for!");
        return Vec::new();
    }

    let mut items: Vec<crate::tab::Item> = Vec::new();
    if search.video {
        let (mut newmetadata, newfiles) = search_video(sql_connection.clone(), &search);
        for i in 0..newfiles.len() {
            if newfiles[i].file_type != 2 {
                continue;
            }
            if !data.special_files_contains(newfiles[i].filepath.clone()) {
                data.special_files_insert(newfiles[i].filepath.clone());
                if let Ok(metadata) = std::fs::metadata(newfiles[i].filepath.clone()) {
                    // The search row only carries the searched columns; load
                    // the full metadata (poster, thumb, directors, actors,
                    // tags, ...) so search results match folder browsing.
                    let pathstr = crate::parsers::osstr_to_string(
                        newfiles[i].filepath.clone().into_os_string(),
                    );
                    let full = video(sql_connection.clone(), &pathstr, &data);
                    if !full.name.is_empty() || !full.title.is_empty() {
                        newmetadata[i] = full;
                    }
                    let item = crate::parsers::item_from_nfo(
                        PathBuf::new(),
                        &mut newmetadata[i],
                        &metadata,
                        crate::config::IconSizes::default(),
                        &data,
                        sql_connection.clone(),
                        true,
                    );
                    items.push(item);
                }
            }
        }
    }
    if search.audio {
        let (mut newmetadata, newfiles) = search_audio(sql_connection.clone(), &search);
        for i in 0..newfiles.len() {
            if !data.special_files_contains(newfiles[i].filepath.clone()) {
                data.special_files_insert(newfiles[i].filepath.clone());
                if let Ok(metadata) = std::fs::metadata(newfiles[i].filepath.clone()) {
                    // Full metadata (poster, thumb, album, artist,
                    // album artist, composer, genre, chapters) — see above.
                    let pathstr = crate::parsers::osstr_to_string(
                        newfiles[i].filepath.clone().into_os_string(),
                    );
                    let full = audio(sql_connection.clone(), &pathstr, &data);
                    if !full.name.is_empty() || !full.title.is_empty() {
                        newmetadata[i] = full;
                    }
                    let item = crate::parsers::item_from_audiotags(
                        newfiles[i].filepath.clone(),
                        &data,
                        &mut newmetadata[i],
                        &metadata,
                        crate::config::IconSizes::default(),
                        sql_connection.clone(),
                        true,
                    );
                    items.push(item);
                }
            }
        }
    }
    if search.image {
        let (mut newmetadata, newfiles) = search_image(sql_connection.clone(), &search);
        for i in 0..newfiles.len() {
            // file_type: 1 image (2 video, 3 audio)
            if newfiles[i].file_type != 1 {
                continue;
            }
            if !data.special_files_contains(newfiles[i].filepath.clone()) {
                data.special_files_insert(newfiles[i].filepath.clone());
                if let Ok(metadata) = std::fs::metadata(newfiles[i].filepath.clone()) {
                    // Full metadata (thumb, photographer, lense, ...) — see above.
                    let pathstr = crate::parsers::osstr_to_string(
                        newfiles[i].filepath.clone().into_os_string(),
                    );
                    let full = image(sql_connection.clone(), &pathstr, &data);
                    if !full.name.is_empty() || !full.title.is_empty() {
                        newmetadata[i] = full;
                    }
                    let item = crate::parsers::item_from_exif(
                        newfiles[i].filepath.clone(),
                        &mut newmetadata[i],
                        &metadata,
                        crate::config::IconSizes::default(),
                        &data,
                        sql_connection.clone(),
                        true,
                    );
                    items.push(item);
                }
            }
        }
    }
    if search.creation_date && search.from_date != 0 {
        let (query, params);
        if search.to_date != 0 {
            (query, params) = (
                "SELECT metadata_id FROM file_metadata WHERE creation_time > ? AND creation_time < ?",
                vec![
                    Value::Integer(search.from_date),
                    Value::Integer(search.to_date),
                ],
            );
        } else {
            (query, params) = (
                "SELECT metadata_id FROM file_metadata WHERE creation_time > ?",
                vec![Value::Integer(search.from_date)],
            );
        }
        let newfiles: Vec<FileMetadata> =
            search_file_metadata(sql_connection.clone(), query.to_string(), params);
        for file in newfiles {
            stuff_items(sql_connection.clone(), &search, &data, file);
        }
    }
    if search.modification_date && search.from_date != 0 {
        let (query, params);
        if search.to_date != 0 {
            (query, params) = (
                "SELECT metadata_id FROM file_metadata WHERE modification_time > ? AND modification_time < ?",
                vec![
                    Value::Integer(search.from_date),
                    Value::Integer(search.to_date),
                ],
            );
        } else {
            (query, params) = (
                "SELECT metadata_id FROM file_metadata WHERE modification_time > ?",
                vec![Value::Integer(search.from_date)],
            );
        }
        let newfiles: Vec<FileMetadata> =
            search_file_metadata(sql_connection.clone(), query.to_string(), params);
        for file in newfiles {
            stuff_items(sql_connection.clone(), &search, &data, file);
        }
    }
    if search.filepath && !search.from_string.is_empty() {
        let query = "SELECT metadata_id FROM file_metadata WHERE filepath LIKE ? ESCAPE '\\'";
        let params = vec![Value::Text(escape_like_term(&search.from_string))];
        let newfiles: Vec<FileMetadata> =
            search_file_metadata(sql_connection.clone(), query.to_string(), params);
        for file in newfiles {
            stuff_items(sql_connection.clone(), &search, &data, file);
        }
    }

    items
}
