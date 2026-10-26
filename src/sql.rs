// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use chrono::{NaiveDate, Timelike};
use rusqlite::{params, types::Value, Connection, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Hash)]
pub enum SearchType {
    FilePath,
    Title,
    Description,
    Actor,
    Director,
    Producer,
    Artist,
    AlbumArtist,
    Album,
    Composer,
    Genre,
    Duration,
    CreationDate,
    ModificationDate,
    ReleaseDate,
    LenseModel,
    FocalLength,
    ExposureTime,
    FNumber,
    GPSLatitude,
    GPSLongitude,
    GPSAltitude,
    Tag,
}

// `SearchData` derives `Eq`, `PartialEq` and `Hash` because it is used as the
// payload of `tab::Location::DBSearch`, which itself derives those traits for
// tab-location identity/deduplication. That requires an exact, symmetric and
// transitive `PartialEq`, so the free-text fields are now compared
// case-sensitively (the previous hand-written case-insensitive `PartialEq` was
// unsound alongside `Eq`/`Hash` and is removed).
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Hash)]
pub struct SearchData {
    pub search_id: u32,
    pub search_string: String,
    pub from_string: String,
    pub from_value_string: String,
    pub from_value: u64,
    pub from_date: i64,
    pub to_string: String,
    pub to_value_string: String,
    pub to_value: u64,
    pub to_date: i64,
    pub image: bool,
    pub video: bool,
    pub audio: bool,
    pub filepath: bool,
    pub title: bool,
    pub description: bool,
    pub actor: bool,
    pub director: bool,
    pub artist: bool,
    pub album_artist: bool,
    pub album: bool,
    pub composer: bool,
    pub genre: bool,
    pub duration: bool,
    pub creation_date: bool,
    pub modification_date: bool,
    pub release_date: bool,
    pub lense_model: bool,
    pub focal_length: bool,
    pub exposure_time: bool,
    pub fnumber: bool,
    pub gps_latitude: bool,
    pub gps_longitude: bool,
    pub gps_altitude: bool,
    pub tags: bool,
}

impl Default for SearchData {
    fn default() -> SearchData {
        SearchData {
            search_id: 0,
            search_string: String::new(),
            from_string: String::new(),
            from_value_string: String::new(),
            from_value: 0,
            from_date: 0,
            to_string: String::new(),
            to_value_string: String::new(),
            to_value: 0,
            to_date: 0,
            image: false,
            video: false,
            audio: false,
            filepath: false,
            title: true,
            description: false,
            actor: false,
            director: false,
            artist: false,
            album_artist: false,
            album: false,
            composer: false,
            genre: false,
            duration: false,
            creation_date: false,
            modification_date: false,
            release_date: false,
            lense_model: false,
            focal_length: false,
            exposure_time: false,
            fnumber: false,
            gps_latitude: false,
            gps_longitude: false,
            gps_altitude: false,
            tags: false,
        }
    }
}

impl SearchData {
    pub fn display(&self) -> String {
        let mut s;
        s = format!("{}", self.search_id);
        if self.image {
            s = format!("{} Image", s);
        }
        if self.video {
            s = format!("{} Video", s);
        }
        if self.audio {
            s = format!("{} Audio", s);
        }
        if self.from_string.len() > 0 {
            s = format!("{} from {}", s, self.from_string);
        }
        if self.to_string.len() > 0 {
            s = format!("{} to {}", s, self.to_string);
        }
        if self.from_value > 0 {
            s = format!("{} from {}", s, self.from_value);
        }
        if self.to_value > 0 {
            s = format!("{} to {}", s, self.to_value);
        }
        if self.from_date > 0 {
            s = format!("{} from {}", s, self.from_date);
        }
        if self.to_date > 0 {
            s = format!("{} to {}", s, self.to_date);
        }
        s
    }

    pub fn store(
        &mut self,
        sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    ) {
        let id = insert_search(sql_connection.clone(), self.clone());
        self.search_id = id;
    }
}

/// Escape a user supplied search term for use inside a SQL `LIKE` pattern.
///
/// The term is bound as a query parameter (so it can never alter the query),
/// but `%`, `_` and the escape character itself are escaped so that they are
/// matched literally instead of acting as wildcards.
fn escape_like_term(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len() + 4);
    escaped.push_str("%");
    for c in term.chars() {
        match c {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(c);
            }
            _ => escaped.push(c),
        }
    }
    escaped.push_str("%");
    escaped
}

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
            if newfiles[i].file_type != 1 {
                continue;
            }
            if !data.special_files_contains(newfiles[i].filepath.clone()) {
                if newfiles[i].file_type != 3 {
                    continue;
                }
                data.special_files_insert(newfiles[i].filepath.clone());
                if let Ok(metadata) = std::fs::metadata(newfiles[i].filepath.clone()) {
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

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize)]
pub struct Tag {
    pub tag_id: u32,
    pub tag: String,
}

#[derive(Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Chapter {
    pub title: String,
    pub start: f32,
    pub end: f32,
}

impl std::fmt::Display for Chapter {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} -- {}", self.title, self.start)
    }
}

pub fn fill_chapters(
    chapters: Vec<crate::sql::Chapter>,
    duration: u32,
) -> (Vec<crate::sql::Chapter>, Vec<String>) {
    let mut v = Vec::new();
    let mut s = Vec::new();
    if chapters.len() > 0 {
        v.extend(chapters.clone());
    } else {
        let numstep = duration / 5 / 60;
        let mut i = 0;
        while i < numstep {
            let mut c = Chapter {
                ..Default::default()
            };
            c.title = format!("Chapter{:02}", i);
            c.start = (i * 5 * 60) as f32;
            c.end = ((i + 1) * 5 * 60) as f32;
            v.push(c);
            i += 1;
        }
    }
    for i in 0..v.len() {
        let title = format!("{:02} {}", i + 1, v[i].title);
        s.push(title);
    }

    (v, s)
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Hash)]
pub struct CollectionMetadata {
    pub id: u32,
    pub name: String,
    pub file_id: u32,
    pub path: PathBuf,
    pub poster: String,
    pub thumb: String,
    pub description: String,
    pub tags: Vec<Tag>,
    pub episodes: Vec<EpisodeMetadata>,
}

impl Default for CollectionMetadata {
    fn default() -> CollectionMetadata {
        CollectionMetadata {
            id: 0,
            name: String::new(),
            file_id: 0,
            path: PathBuf::new(),
            poster: String::new(),
            thumb: String::new(),
            description: String::new(),
            tags: Vec::new(),
            episodes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Hash)]
pub struct EpisodeMetadata {
    pub series: i32,
    pub episode: i32,
    pub file_id: u32,
    pub path: PathBuf,
    pub poster: String,
    pub thumb: String,
    pub title: String,
}

impl Default for EpisodeMetadata {
    fn default() -> EpisodeMetadata {
        EpisodeMetadata {
            series: 1,
            episode: 1,
            file_id: 0,
            path: PathBuf::new(),
            poster: String::new(),
            thumb: String::new(),
            title: String::new(),
        }
    }
}

pub fn insert_collection(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut crate::sql::CollectionMetadata,
    statdata: &std::fs::Metadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    let file_id = insert_file(
        sql_connection.clone(),
        &crate::parsers::osstr_to_string(metadata.path.clone().into_os_string()),
        statdata,
        2,
        data,
    );
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    metadata.id = file_id;
    let path = crate::parsers::osstr_to_string(metadata.path.clone().into_os_string());
    match connection.execute(
        "INSERT INTO collections (file_id, collection_name, poster, description, path, thumb) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            &metadata.id,
            &metadata.name,
            &metadata.poster,
            &metadata.description,
            &path,
            &metadata.thumb,
        ],
    ) {
        Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
        Err(error) => {
            log::error!(
                "Failed to insert tvshow into collections database: {}",
                error
            );
            drop(connection);
            delete_collection(sql_connection.clone(), metadata, data);
            insert_collection(sql_connection.clone(), metadata, statdata, data);
            return;
        }
    }
    let mut collection_id = 0;
    let query = "SELECT last_insert_rowid()";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(0) {
                        collection_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from video_metadata database: {}", err);
            }
        },
        Err(error) => {
            log::error!("Failed to get collection_id for from database: {}", error);
            return;
        }
    }
    for i in 0..metadata.episodes.len() {
        let path =
            crate::parsers::osstr_to_string(metadata.episodes[i].path.clone().into_os_string());
        match connection.execute(
            "INSERT INTO collections_map (collection_id, episode_id, series, episode, title, path, poster, thumb) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &collection_id,
                &metadata.episodes[i].file_id,
                &metadata.episodes[i].series,
                &metadata.episodes[i].episode,
                &metadata.episodes[i].title,
                &path,
                &metadata.episodes[i].poster,
                &metadata.episodes[i].thumb,
            ],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!(
                    "Failed to insert episode_id  into  collections_map: {}",
                    error
                );
            }
        }
    }
}

pub fn delete_collection(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut crate::sql::CollectionMetadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    // Get the index.
    //let index = self.ids[id];
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    let mut collection_id: u32 = 0;
    let query = "SELECT metadata_id FROM file_metadata WHERE filepath = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![&crate::parsers::osstr_to_string(
            metadata.path.clone().into_os_string()
        )]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(1) {
                        collection_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from file_metadata database: {}", err);
            }
        },
        Err(error) => {
            log::error!(
                "Failed to get file_id for {} from database: {}",
                &crate::parsers::osstr_to_string(metadata.path.clone().into_os_string()),
                error
            );
            return;
        }
    }
    // clear the entry in the candidates list without deleting it
    let ret = connection.execute(
        "DELETE FROM collections WHERE file_id = ?1",
        params![&collection_id],
    );
    if ret.is_err() {
        log::error!("Failed to delete candidate {}!", collection_id);
        return;
    }
    let ret = connection.execute(
        "DELETE FROM collections_map WHERE collection_id = ?1",
        params![&collection_id],
    );
    if ret.is_err() {
        log::error!("Failed to delete candidate {}!", collection_id);
        return;
    }
    // clear the entry in the candidates list without deleting it
    let ret = connection.execute(
        "DELETE FROM file_metadata WHERE filepath = ?1",
        params![&crate::parsers::osstr_to_string(
            metadata.path.clone().into_os_string()
        )],
    );
    if ret.is_err() {
        log::error!("Failed to delete candidate {:?}!", metadata.path);
        return;
    }
    data.known_files_remove(PathBuf::from(&metadata.path));
}

pub fn update_collection(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut crate::sql::CollectionMetadata,
    statdata: &std::fs::Metadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    delete_collection(sql_connection.clone(), metadata, data);
    insert_collection(sql_connection.clone(), metadata, statdata, data);
}

pub fn collection(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    filepath: &str,
    data: &crate::scanmetadata::ScanMetaData,
) -> CollectionMetadata {
    let mut v = CollectionMetadata {
        ..Default::default()
    };
    let filedata = file(sql_connection.clone(), filepath);
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };
    v.path = PathBuf::from(filepath);
    let query = "SELECT collection_id, file_id, collection_name, poster, thumb, description, path FROM collections WHERE file_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&filedata.metadata_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => v.id = val,
                                    Err(error) => {
                                        log::error!("Failed to read id for collections: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.file_id = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read file_id for collections: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.name = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read name for collections: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.poster = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read poster for vidcollectionseo: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.thumb = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read thumb for collections: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(5) {
                                    Ok(val) => v.description = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read description for collections: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(6) {
                                    Ok(val) => {
                                        let path: String = val;
                                        v.path = PathBuf::from(&path);
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read path for collections: {}",
                                            error
                                        );
                                        continue;
                                    }
                                };
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
    let collection_id = v.id;

    let mut episodes = Vec::new();
    let query = "SELECT episode_id, series, episode, title, path, poster, thumb FROM collections_map WHERE collection_id = ?1 ORDER BY series ASC, episode ASC";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&collection_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut e = EpisodeMetadata {
                                    ..Default::default()
                                };
                                match row.get(0) {
                                    Ok(val) => {
                                        e.file_id = val;
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read file_id for collections_map: {}",
                                            error
                                        );
                                        continue;
                                    }
                                };
                                match row.get(1) {
                                    Ok(val) => {
                                        e.series = val;
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read series for vidcollections_mapeo: {}",
                                            error
                                        );
                                        continue;
                                    }
                                };
                                match row.get(2) {
                                    Ok(val) => {
                                        e.episode = val;
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read episode for collections_map: {}",
                                            error
                                        );
                                        continue;
                                    }
                                };
                                match row.get(3) {
                                    Ok(val) => e.title = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read title for collections_map: {}",
                                            error
                                        );
                                        continue;
                                    }
                                };
                                match row.get(4) {
                                    Ok(val) => {
                                        let path: String = val;
                                        e.path = PathBuf::from(&path);
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read path for collections_map: {}",
                                            error
                                        );
                                        continue;
                                    }
                                };
                                match row.get(5) {
                                    Ok(val) => e.poster = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read poster for collections_map: {}",
                                            error
                                        );
                                        continue;
                                    }
                                };
                                match row.get(6) {
                                    Ok(val) => e.thumb = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read thumb for collections_map: {}",
                                            error
                                        );
                                        continue;
                                    }
                                };
                                episodes.push(e);
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
                    log::error!("could not read line from subtitles database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT tag_id, tag FROM tags
                        INNER JOIN tags_media_map
                        ON tags_media_map.tagmap_id = tags.tag_id
                        WHERE tags_media_map.media_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&v.file_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut tag = Tag {
                                    ..Default::default()
                                };
                                match row.get::<usize, u32>(0) {
                                    Ok(val) => tag.tag_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get::<usize, String>(1) {
                                    Ok(val) => tag.tag = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                v.tags.push(tag);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from tags: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from tags database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    drop(connection);
    for mut e in episodes {
        let file = file_by_id(sql_connection.clone(), e.file_id as i64);
        let video = video_by_id(
            sql_connection.clone(),
            &crate::parsers::osstr_to_string(file.filepath.clone().into_os_string()),
            e.file_id as i64,
        );
        e.path = PathBuf::from(&video.path);
        e.poster = video.poster;
        e.thumb = video.thumb;
        v.episodes.push(e);
    }

    if !data.known_files_contains(PathBuf::from(&v.path)) {
        data.known_files_insert(PathBuf::from(&v.path), filedata.clone());
    }

    v
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct VideoMetadata {
    pub id: u32,
    pub name: String,
    pub title: String,
    pub date: NaiveDate,
    pub path: String,
    pub poster: String,
    pub thumb: String,
    pub subtitles: Vec<String>,
    pub audiolangs: Vec<String>,
    pub sublangs: Vec<String>,
    pub duration: u32,
    pub width: u32,
    pub height: u32,
    pub framerate: f32,
    pub description: String,
    pub director: Vec<String>,
    pub actors: Vec<String>,
    pub chapters: Vec<Chapter>,
    pub tags: Vec<Tag>,
    pub season: i32,
    pub episode: i32,
}

impl Default for VideoMetadata {
    fn default() -> VideoMetadata {
        VideoMetadata {
            id: 0,
            name: String::new(),
            title: String::new(),
            date: NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            path: String::new(),
            poster: String::new(),
            thumb: String::new(),
            subtitles: Vec::new(),
            audiolangs: Vec::new(),
            sublangs: Vec::new(),

            duration: 0,
            width: 0,
            height: 0,
            framerate: 0.0,
            description: String::new(),
            director: Vec::new(),
            actors: Vec::new(),
            chapters: Vec::new(),
            tags: Vec::new(),
            season: 0,
            episode: 0,
        }
    }
}

pub fn insert_video(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut crate::sql::VideoMetadata,
    statdata: &std::fs::Metadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    let video_id = insert_file(sql_connection.clone(), &metadata.path, statdata, 2, data);
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    metadata.id = video_id;
    match connection.execute(
        "INSERT INTO video_metadata (video_id, name, title, released, poster, thumb, duration, width, height, framerate, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![&metadata.id, &metadata.name, &metadata.title, &metadata.date, &metadata.poster, &metadata.thumb, &metadata.duration, &metadata.width, &metadata.height, &metadata.framerate, &metadata.description],
    ) {
        Ok(_retval) => {}, //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
        Err(error) => {
            log::error!("Failed to insert video into  database: {}", error);
            drop(connection);
            delete_video(sql_connection.clone(), metadata, data);
            insert_video(sql_connection.clone(), metadata, statdata, data);
            return;
        }
    }
    let mut video_id = 0;
    let query = "SELECT last_insert_rowid()";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(0) {
                        video_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from video_metadata database: {}", err);
            }
        },
        Err(error) => {
            log::error!("Failed to get video_id for from database: {}", error);
            return;
        }
    }
    for i in 0..metadata.subtitles.len() {
        match connection.execute(
            "INSERT INTO subtitles (video_id, subpath) VALUES (?1, ?2)",
            params![&video_id, &metadata.subtitles[i]],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert video into  database: {}", error);
                return;
            }
        }
    }
    for i in 0..metadata.audiolangs.len() {
        match connection.execute(
            "INSERT INTO audiolangs (video_id, audiolang) VALUES (?1, ?2)",
            params![&video_id, &metadata.audiolangs[i]],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert video into  database: {}", error);
                return;
            }
        }
    }
    for i in 0..metadata.sublangs.len() {
        match connection.execute(
            "INSERT INTO sublangs (video_id, sublang) VALUES (?1, ?2)",
            params![&video_id, &metadata.sublangs[i]],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert video into  database: {}", error);
                return;
            }
        }
    }
    for i in 0..metadata.chapters.len() {
        match connection.execute(
            "INSERT INTO chapters (video_id, title, start, end) VALUES (?1, ?2, ?3, ?4)",
            params![
                &video_id,
                &metadata.chapters[i].title,
                &metadata.chapters[i].start,
                &metadata.chapters[i].end
            ],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert video into  database: {}", error);
                return;
            }
        }
    }
    for i in 0..metadata.director.len() {
        let mut director_id = -1;
        let query = "SELECT person_id FROM people WHERE person_name = ?1";
        match connection.prepare(query) {
            Ok(mut statement) => match statement.query(params![&metadata.director[i]]) {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(val) = row.get(1) {
                            director_id = val;
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from people database: {}", err);
                }
            },
            Err(error) => {
                log::error!(
                    "Failed to get person_id for {} from database: {}",
                    metadata.director[i],
                    error
                );
            }
        }
        if director_id == -1 {
            match connection.execute(
                "INSERT INTO people (person_name) VALUES (?1)",
                params![&metadata.director[i]],
            ) {
                Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
                Err(error) => {
                    log::error!("Failed to insert video into  database: {}", error);
                }
            }

            let query = "SELECT last_insert_rowid()";
            match connection.prepare(query) {
                Ok(mut statement) => match statement.query(params![]) {
                    Ok(mut rows) => {
                        while let Ok(Some(row)) = rows.next() {
                            if let Ok(val) = row.get(0) {
                                director_id = val;
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("could not read last generated id from database: {}", err);
                    }
                },
                Err(error) => {
                    log::error!("Failed to get person_id for from database: {}", error);
                    return;
                }
            }
        }
        if director_id == -1 {
            continue;
        }
        match connection.execute(
            "INSERT INTO directors (director_id, video_id) VALUES (?1, ?2)",
            params![&director_id, &video_id],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert director into  database: {}", error);
                return;
            }
        }
    }

    for i in 0..metadata.actors.len() {
        let mut actor_id = -1;
        let query = "SELECT person_id FROM people WHERE person_name = ?1";
        match connection.prepare(query) {
            Ok(mut statement) => match statement.query(params![&metadata.actors[i]]) {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(val) = row.get(1) {
                            actor_id = val;
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from people database: {}", err);
                }
            },
            Err(error) => {
                log::error!(
                    "Failed to get actor_id for {} from database: {}",
                    metadata.actors[i],
                    error
                );
                return;
            }
        }
        if actor_id == -1 {
            match connection.execute(
                "INSERT INTO people (person_name) VALUES (?1)",
                params![&metadata.actors[i]],
            ) {
                Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
                Err(error) => {
                    log::error!("Failed to insert video into  database: {}", error);
                    return;
                }
            }

            let query = "SELECT last_insert_rowid()";
            match connection.prepare(query) {
                Ok(mut statement) => match statement.query(params![]) {
                    Ok(mut rows) => {
                        while let Ok(Some(row)) = rows.next() {
                            if let Ok(val) = row.get(0) {
                                actor_id = val;
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("could not read last generated id from database: {}", err);
                    }
                },
                Err(error) => {
                    log::error!("Failed to get person_id for from database: {}", error);
                    return;
                }
            }
        }
        if actor_id == -1 {
            continue;
        }
        match connection.execute(
            "INSERT INTO actors (actor_id, video_id) VALUES (?1, ?2)",
            params![&actor_id, &video_id],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert actor into  database: {}", error);
                return;
            }
        }
    }
    drop(connection);
    for i in 0..metadata.tags.len() {
        let tag_id = insert_tag(
            sql_connection.clone(),
            metadata.id,
            metadata.tags[i].tag.clone(),
        );
        if tag_id == -1 {
            continue;
        }
    }
}

pub fn tags(sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>) -> Vec<Tag> {
    let mut tags = Vec::new();
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return tags;
        }
    };
    let query = "SELECT tag_id, tag FROM tags";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut s = Tag {
                                    ..Default::default()
                                };
                                match row.get(0) {
                                    Ok(val) => s.tag_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read id for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => s.tag = val,
                                    Err(error) => {
                                        log::error!("Failed to read video_id for video: {}", error);
                                        continue;
                                    }
                                }
                                tags.push(s);
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

    tags
}

pub fn delete_tag(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    tag: String,
) {
    let mut tag_id = -1;
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    let query = "SELECT tag_id FROM tags WHERE tag = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![&tag]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(1) {
                        tag_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from tags database: {}", err);
            }
        },
        Err(error) => {
            log::error!("Failed to get tag_id for {} from database: {}", tag, error);
            return;
        }
    }
    if tag_id == -1 {
        let ret = connection.execute("DELETE FROM tags WHERE tag_id = ?1", params![&tag_id]);
        if ret.is_err() {
            log::error!("Failed to delete candidate {}!", tag_id);
            return;
        }
        let ret = connection.execute(
            "DELETE FROM tags_media_map WHERE tag_id = ?1",
            params![&tag_id],
        );
        if ret.is_err() {
            log::error!("Failed to delete candidate {}!", tag_id);
            return;
        }
    }
}

pub fn insert_tag(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    media_id: u32,
    tag: String,
) -> i32 {
    let mut tag_id = -1;
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return tag_id;
        }
    };
    let query = "SELECT tag_id FROM tags WHERE tag = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![&tag]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(1) {
                        tag_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from tags database: {}", err);
            }
        },
        Err(error) => {
            log::error!("Failed to get tag_id for {} from database: {}", tag, error);
        }
    }
    if tag_id == -1 {
        match connection.execute("INSERT INTO tags (tag) VALUES (?1)", params![&tag]) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert tag into  database: {}", error);
                return tag_id;
            }
        }

        let query = "SELECT last_insert_rowid()";
        match connection.prepare(query) {
            Ok(mut statement) => match statement.query(params![]) {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(val) = row.get(0) {
                            tag_id = val;
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read last generated id from database: {}", err);
                }
            },
            Err(error) => {
                log::error!("Failed to get tag_id for from database: {}", error);
                return tag_id;
            }
        }
    }
    if media_id > 0 {
        match connection.execute(
            "INSERT INTO tags_media_map (media_id, tagmap_id) VALUES (?1, ?2)",
            params![&media_id, tag_id],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!(
                    "Failed to insert media_id into tags_media_map database: {}",
                    error
                );
                return tag_id;
            }
        }
    }

    tag_id
}

pub fn insert_media_tag(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    media_id: u32,
    tag_id: u32,
) {
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    if media_id > 0 {
        match connection.execute(
            "INSERT INTO tags_media_map (media_id, tagmap_id) VALUES (?1, ?2)",
            params![&media_id, tag_id],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!(
                    "Failed to insert media_id into tags_media_map database: {}",
                    error
                );
                return;
            }
        }
    }
}

pub fn delete_video(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut crate::sql::VideoMetadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    // Get the index.
    //let index = self.ids[id];
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    let mut video_id: u32 = 0;
    let query = "SELECT metadata_id FROM file_metadata WHERE filepath = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![&metadata.path]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(1) {
                        video_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from file_metadata database: {}", err);
            }
        },
        Err(error) => {
            log::error!(
                "Failed to get video_id for {} from database: {}",
                metadata.path,
                error
            );
            return;
        }
    }
    // clear the entry in the candidates list without deleting it
    let ret = connection.execute(
        "DELETE FROM video_metadata WHERE video_id = ?1",
        params![&video_id],
    );
    if ret.is_err() {
        log::error!("Failed to delete candidate {}!", video_id);
        return;
    }
    // clear the entry in the candidates list without deleting it
    let ret = connection.execute(
        "DELETE FROM file_metadata WHERE filepath = ?1",
        params![&metadata.path],
    );
    if ret.is_err() {
        log::error!("Failed to delete candidate {}!", metadata.path);
        return;
    }
    data.known_files_remove(PathBuf::from(&metadata.path));
}

pub fn update_video(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut crate::sql::VideoMetadata,
    statdata: &std::fs::Metadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    delete_video(sql_connection.clone(), metadata, data);
    insert_video(sql_connection.clone(), metadata, statdata, data);
}

pub fn video_by_id(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    filepath: &str,
    video_id: i64,
) -> VideoMetadata {
    let mut v = VideoMetadata {
        ..Default::default()
    };
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };

    v.path = filepath.to_string();
    let query = "SELECT name, title, released, poster, duration, width, height, framerate, description, thumb FROM video_metadata WHERE video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => v.name = val,
                                    Err(error) => {
                                        log::error!("Failed to read id for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.title = val,
                                    Err(error) => {
                                        log::error!("Failed to read video_id for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.date = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read screenshot_id for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.poster = val,
                                    Err(error) => {
                                        log::error!("Failed to read poster for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.duration = val,
                                    Err(error) => {
                                        log::error!("Failed to read duration for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(5) {
                                    Ok(val) => v.width = val,
                                    Err(error) => {
                                        log::error!("Failed to read width for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(6) {
                                    Ok(val) => v.height = val,
                                    Err(error) => {
                                        log::error!("Failed to read height for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(7) {
                                    Ok(val) => v.framerate = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read framerate for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(8) {
                                    Ok(val) => v.description = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read description for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(9) {
                                    Ok(val) => v.thumb = val,
                                    Err(error) => {
                                        log::error!("Failed to read thumb for video: {}", error);
                                        continue;
                                    }
                                }
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
    let query = "SELECT subpath FROM subtitles WHERE video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => {
                                    let s: String = val;
                                    v.subtitles.push(s);
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
                    log::error!("could not read line from subtitles database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT audiolang FROM audiolangs WHERE video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.audiolangs.push(val.clone()),
                                Err(error) => {
                                    log::error!("Failed to audiolangs for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from audiolangs: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from audiolangs database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT sublang FROM sublangs WHERE video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.sublangs.push(val.clone()),
                                Err(error) => {
                                    log::error!("Failed to read sublangs for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from sublangs.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from sublangs: {}", error);
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
    let query = "SELECT person_name FROM people
                        INNER JOIN directors
                        ON directors.director_id = people.person_id
                        WHERE directors.video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.director.push(val.clone()),
                                Err(error) => {
                                    log::error!("Failed to read directors for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from directors: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from directors database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT person_name FROM people
                        INNER JOIN actors
                        ON actors.actor_id = people.person_id
                        WHERE actors.video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.actors.push(val),
                                Err(error) => {
                                    log::error!("Failed to read actors for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from actors: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from actors database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT tag_id, tag FROM tags
                        INNER JOIN tags_media_map
                        ON tags_media_map.tagmap_id = tags.tag_id
                        WHERE tags_media_map.media_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut tag = Tag {
                                    ..Default::default()
                                };
                                match row.get::<usize, u32>(0) {
                                    Ok(val) => tag.tag_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get::<usize, String>(1) {
                                    Ok(val) => tag.tag = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                v.tags.push(tag);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from tags: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from tags database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }

    v
}

pub fn video(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    filepath: &str,
    data: &crate::scanmetadata::ScanMetaData,
) -> VideoMetadata {
    let mut v = VideoMetadata {
        ..Default::default()
    };
    let filedata = file(sql_connection.clone(), filepath);
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };
    v.path = filepath.to_string();
    let video_id = filedata.metadata_id;
    let query = "SELECT name, title, released, poster, duration, width, height, framerate, description, thumb FROM video_metadata WHERE video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => v.name = val,
                                    Err(error) => {
                                        log::error!("Failed to read id for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.title = val,
                                    Err(error) => {
                                        log::error!("Failed to read video_id for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.date = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read screenshot_id for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.poster = val,
                                    Err(error) => {
                                        log::error!("Failed to read poster for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.duration = val,
                                    Err(error) => {
                                        log::error!("Failed to read duration for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(5) {
                                    Ok(val) => v.width = val,
                                    Err(error) => {
                                        log::error!("Failed to read width for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(6) {
                                    Ok(val) => v.height = val,
                                    Err(error) => {
                                        log::error!("Failed to read height for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(7) {
                                    Ok(val) => v.framerate = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read framerate for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(8) {
                                    Ok(val) => v.description = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read description for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(9) {
                                    Ok(val) => v.thumb = val,
                                    Err(error) => {
                                        log::error!("Failed to read thumb for video: {}", error);
                                        continue;
                                    }
                                }
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
    let query = "SELECT subpath FROM subtitles WHERE video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => {
                                    let s: String = val;
                                    v.subtitles.push(s);
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
                    log::error!("could not read line from subtitles database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT audiolang FROM audiolangs WHERE video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.audiolangs.push(val.clone()),
                                Err(error) => {
                                    log::error!("Failed to audiolangs for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from audiolangs: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from audiolangs database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT sublang FROM sublangs WHERE video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.sublangs.push(val.clone()),
                                Err(error) => {
                                    log::error!("Failed to read sublangs for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from sublangs.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from sublangs: {}", error);
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
    let query =
        "SELECT title, start, end FROM chapters WHERE video_id = ?1 ORDER BY start, end ASC";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut c = Chapter {
                                    ..Default::default()
                                };
                                match row.get::<usize, String>(0) {
                                    Ok(val) => c.title = val.clone(),
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter title for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get::<usize, f32>(1) {
                                    Ok(val) => c.start = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter start for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get::<usize, f32>(2) {
                                    Ok(val) => c.end = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter end for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                v.chapters.push(c);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from sublangs.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from sublangs: {}", error);
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
    let query = "SELECT person_name FROM people
                        INNER JOIN directors
                        ON directors.director_id = people.person_id
                        WHERE directors.video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.director.push(val.clone()),
                                Err(error) => {
                                    log::error!("Failed to read directors for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from directors: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from directors database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT person_name FROM people
                        INNER JOIN actors
                        ON actors.actor_id = people.person_id
                        WHERE actors.video_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.actors.push(val),
                                Err(error) => {
                                    log::error!("Failed to read actors for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from actors: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from actors database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT tag_id, tag FROM tags
                        INNER JOIN tags_media_map
                        ON tags_media_map.tagmap_id = tags.tag_id
                        WHERE tags_media_map.media_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&video_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut tag = Tag {
                                    ..Default::default()
                                };
                                match row.get::<usize, u32>(0) {
                                    Ok(val) => tag.tag_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get::<usize, String>(1) {
                                    Ok(val) => tag.tag = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                v.tags.push(tag);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from tags: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from tags database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    if !data.known_files_contains(PathBuf::from(&v.path)) {
        data.known_files_insert(PathBuf::from(&v.path), filedata.clone());
    }

    v
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct AudioMetadata {
    pub id: u32,
    pub name: String,
    pub title: String,
    pub date: NaiveDate,
    pub path: String,
    pub poster: String,
    pub thumb: String,
    pub genre: String,
    pub composer: String,
    pub track_id: u32,
    pub duration: u32,
    pub bitrate: f32,
    pub album: String,
    pub artist: Vec<String>,
    pub albumartist: Vec<String>,
    pub chapters: Vec<Chapter>,
    pub lyrics: Vec<String>,
    pub tags: Vec<Tag>,
}

impl Default for AudioMetadata {
    fn default() -> AudioMetadata {
        AudioMetadata {
            id: 0,
            name: String::new(),
            title: String::new(),
            date: NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            path: String::new(),
            poster: String::new(),
            thumb: String::new(),
            genre: String::new(),
            composer: String::new(),
            track_id: 0,
            duration: 0,
            bitrate: 0.0,
            album: String::new(),
            artist: Vec::new(),
            albumartist: Vec::new(),
            chapters: Vec::new(),
            lyrics: Vec::new(),
            tags: Vec::new(),
        }
    }
}

pub fn insert_audio(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut AudioMetadata,
    statdata: &std::fs::Metadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    let audio_id = insert_file(sql_connection.clone(), &metadata.path, statdata, 3, data);
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    metadata.id = audio_id;
    match connection.execute(
        "INSERT INTO audio_metadata (audio_id, name, title, released, poster, thumb, genre, composer, track_id, duration, bitrate) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![&metadata.id, &metadata.name, &metadata.title, &metadata.date, &metadata.poster, &metadata.thumb, &metadata.genre, &metadata.composer, &metadata.track_id, &metadata.duration, &metadata.bitrate],
    ) {
        Ok(_retval) => {}, //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
        Err(error) => {
            log::error!("Failed to insert audio into  database: {}\nUpdating the existing entry.", error);
            drop(connection);
            delete_audio(sql_connection.clone(), metadata, data);
            insert_audio(sql_connection.clone(), metadata, statdata, data);
            return;
        }
    }
    // insert album name into table and get the id
    let mut album_id = -1;
    let query = "SELECT album_id FROM albums WHERE album_name = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![&metadata.album]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(0) {
                        album_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from people database: {}", err);
            }
        },
        Err(error) => {
            log::error!(
                "Failed to get album_id for {} from database: {}",
                metadata.album,
                error
            );
        }
    }
    if album_id == -1 {
        match connection.execute(
            "INSERT INTO albums (album_name) VALUES (?1)",
            params![&metadata.album],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert video into  database: {}", error);
            }
        }

        let query = "SELECT last_insert_rowid()";
        match connection.prepare(query) {
            Ok(mut statement) => match statement.query(params![]) {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(val) = row.get(0) {
                            album_id = val;
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read last generated id from database: {}", err);
                }
            },
            Err(error) => {
                log::error!("Failed to get album_id for from database: {}", error);
            }
        }
    }
    // insert the album id and map it to the audio file
    if album_id != -1 {
        match connection.execute(
            "INSERT INTO album_audio_map (audio_id, album_id) VALUES (?1, ?2)",
            params![&audio_id, &album_id],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert album into  database: {}", error);
                return;
            }
        }
    }
    // insert the artist name into a table and get the artist id
    for i in 0..metadata.artist.len() {
        let mut artist_id = -1;
        let query = "SELECT artist_id FROM artists WHERE artist_name = ?1";
        match connection.prepare(query) {
            Ok(mut statement) => match statement.query(params![&metadata.artist[i]]) {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(val) = row.get(0) {
                            artist_id = val;
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from people database: {}", err);
                }
            },
            Err(error) => {
                log::error!(
                    "Failed to get artist_id for {} from database: {}",
                    metadata.artist[i],
                    error
                );
            }
        }
        if artist_id == -1 {
            match connection.execute(
                "INSERT INTO artists (artist_name) VALUES (?1)",
                params![&metadata.artist[i]],
            ) {
                Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
                Err(error) => {
                    log::error!("Failed to insert artist into  database: {}", error);
                }
            }

            let query = "SELECT last_insert_rowid()";
            match connection.prepare(query) {
                Ok(mut statement) => match statement.query(params![]) {
                    Ok(mut rows) => {
                        while let Ok(Some(row)) = rows.next() {
                            if let Ok(val) = row.get(0) {
                                artist_id = val;
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("could not read last generated id from database: {}", err);
                    }
                },
                Err(error) => {
                    log::error!("Failed to get artist_id for from database: {}", error);
                }
            }
        }
        if artist_id == -1 {
            continue;
        }
        // map the artist id to the audio id
        match connection.execute(
            "INSERT INTO artist_audio_map (audio_id, artist_id) VALUES (?1, ?2)",
            params![&audio_id, &artist_id],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert artist into  database: {}", error);
                return;
            }
        }
    }
    // insert the album artist name into a table and get the id
    for i in 0..metadata.albumartist.len() {
        let mut albumartist_id = -1;
        let query = "SELECT artist_id FROM artists WHERE artist_name = ?1";
        match connection.prepare(query) {
            Ok(mut statement) => match statement.query(params![&metadata.albumartist[i]]) {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(val) = row.get(0) {
                            albumartist_id = val;
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from people database: {}", err);
                }
            },
            Err(error) => {
                log::error!(
                    "Failed to get artist_id for {} from database: {}",
                    metadata.albumartist[i],
                    error
                );
            }
        }
        if albumartist_id == -1 {
            match connection.execute(
                "INSERT INTO artists (artist_name) VALUES (?1)",
                params![&metadata.albumartist[i]],
            ) {
                Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
                Err(error) => {
                    log::error!("Failed to insert artist into  database: {}", error);
                }
            }

            let query = "SELECT last_insert_rowid()";
            match connection.prepare(query) {
                Ok(mut statement) => match statement.query(params![]) {
                    Ok(mut rows) => {
                        while let Ok(Some(row)) = rows.next() {
                            if let Ok(val) = row.get(0) {
                                albumartist_id = val;
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("could not read last generated id from database: {}", err);
                    }
                },
                Err(error) => {
                    log::error!("Failed to get albumartist_id for from database: {}", error);
                }
            }
        }
        if albumartist_id == -1 {
            continue;
        }
        // map the album artist id to the audio id
        match connection.execute(
            "INSERT INTO albumartist_audio_map (audio_id, albumartist_id) VALUES (?1, ?2)",
            params![&audio_id, &albumartist_id],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert albumartist_id into  database: {}", error);
                return;
            }
        }
    }
    //  insert chapters into table
    for i in 0..metadata.chapters.len() {
        match connection.execute(
            "INSERT INTO indexes (audio_id, title, start, end) VALUES (?1, ?2, ?3, ?4)",
            params![
                &audio_id,
                &metadata.chapters[i].title,
                &metadata.chapters[i].start,
                &metadata.chapters[i].end
            ],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert video into  database: {}", error);
                return;
            }
        }
    }
    // insert external lyrics in to table
    for i in 0..metadata.lyrics.len() {
        match connection.execute(
            "INSERT INTO lyrics (audio_id, lyricsfile) VALUES (?1, ?2)",
            params![&audio_id, &metadata.lyrics[i]],
        ) {
            Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
            Err(error) => {
                log::error!("Failed to insert video into  database: {}", error);
                return;
            }
        }
    }
    // insert user defined tags into table
    for i in 0..metadata.tags.len() {
        let tag_id = insert_tag(
            sql_connection.clone(),
            metadata.id,
            metadata.tags[i].tag.clone(),
        );
        if tag_id == -1 {
            continue;
        }
    }
}

pub fn delete_audio(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut AudioMetadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    // Get the index.
    //let index = self.ids[id];
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    let mut audio_id: u32 = 0;
    let query = "SELECT metadata_id FROM file_metadata WHERE filepath = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![&metadata.path]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(1) {
                        audio_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from file_metadata database: {}", err);
            }
        },
        Err(error) => {
            log::error!(
                "Failed to get video_id for {} from database: {}",
                metadata.path,
                error
            );
            return;
        }
    }
    // clear the entry in the candidates list without deleting it
    let ret = connection.execute(
        "DELETE FROM audio_metadata WHERE audio_id = ?1",
        params![&audio_id],
    );
    if ret.is_err() {
        log::error!("Failed to delete audio file {}!", audio_id);
        return;
    }
    // clear the entry in the candidates list without deleting it
    let ret = connection.execute(
        "DELETE FROM file_metadata WHERE filepath = ?1",
        params![&metadata.path],
    );
    if ret.is_err() {
        log::error!("Failed to delete candidate {}!", metadata.path);
        return;
    }
    data.known_files_remove(PathBuf::from(&metadata.path));
}

pub fn update_audio(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut crate::sql::AudioMetadata,
    statdata: &std::fs::Metadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    delete_audio(sql_connection.clone(), metadata, data);
    insert_audio(sql_connection.clone(), metadata, statdata, data);
}

pub fn audio_by_id(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    filepath: &str,
    audio_id: i64,
) -> AudioMetadata {
    let mut v = AudioMetadata {
        ..Default::default()
    };
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };
    v.path = filepath.to_string();
    // fill v from all tables
    v.id = audio_id as u32;
    let query = "SELECT name, title, released, poster, thumb, duration, genre, composer, track_id FROM audio_metadata WHERE audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => v.name = val,
                                    Err(error) => {
                                        log::error!("Failed to read name for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.title = val,
                                    Err(error) => {
                                        log::error!("Failed to read title for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.date = val,
                                    Err(error) => {
                                        log::error!("Failed to read date for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.poster = val,
                                    Err(error) => {
                                        log::error!("Failed to read poster for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.thumb = val,
                                    Err(error) => {
                                        log::error!("Failed to read bitrate for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(5) {
                                    Ok(val) => v.duration = val,
                                    Err(error) => {
                                        log::error!("Failed to read duration for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(6) {
                                    Ok(val) => v.genre = val,
                                    Err(error) => {
                                        log::error!("Failed to read genre for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(7) {
                                    Ok(val) => v.composer = val,
                                    Err(error) => {
                                        log::error!("Failed to read composer for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(8) {
                                    Ok(val) => v.track_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read track_id for audio: {}", error);
                                        continue;
                                    }
                                }
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
    let query = "SELECT artist_name FROM artists
                        INNER JOIN artist_audio_map
                        ON artist_audio_map.artist_id = artists.artist_id
                        WHERE artist_audio_map.audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => {
                                    let s: String = val.to_string();
                                    v.artist.push(s);
                                }
                                Err(error) => {
                                    log::error!("Failed to read artists for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from artists: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from artists database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT album_name FROM albums
                        INNER JOIN album_audio_map
                        ON album_audio_map.album_id = albums.album_id
                        WHERE album_audio_map.audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.album = val.clone(),
                                Err(error) => {
                                    log::error!("Failed to read albums for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from albums: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from albums database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT artist_name FROM artists
                        INNER JOIN albumartist_audio_map
                        ON albumartist_audio_map.albumartist_id = artists.artist_id
                        WHERE albumartist_audio_map.audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.albumartist.push(val.clone()),
                                Err(error) => {
                                    log::error!(
                                        "Failed to read album_artists for audio: {}",
                                        error
                                    );
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from album_artists: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from album_artists database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT title, start, end FROM indexes WHERE audio_id = ?1 ORDER BY start, end ASC";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut c = Chapter {
                                    ..Default::default()
                                };
                                match row.get::<usize, String>(0) {
                                    Ok(val) => c.title = val.clone(),
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter title for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get::<usize, f32>(1) {
                                    Ok(val) => c.start = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter start for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get::<usize, f32>(2) {
                                    Ok(val) => c.end = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter end for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                v.chapters.push(c);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from sublangs.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from sublangs: {}", error);
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
    let query = "SELECT lyricsfile FROM lyrics WHERE audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => {
                                    let s = val.clone();
                                    v.lyrics.push(s);
                                }
                                Err(error) => {
                                    log::error!(
                                        "Failed to read chapter title for video: {}",
                                        error
                                    );
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from sublangs.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from sublangs: {}", error);
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
    let query = "SELECT tag_id, tag FROM tags
                        INNER JOIN tags_media_map
                        ON tags_media_map.tagmap_id = tags.tag_id
                        WHERE tags_media_map.media_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut tag = Tag {
                                    ..Default::default()
                                };
                                match row.get::<usize, u32>(0) {
                                    Ok(val) => tag.tag_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get::<usize, String>(1) {
                                    Ok(val) => tag.tag = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                v.tags.push(tag);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from tags: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from tags database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    v
}

pub fn audio(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    filepath: &str,
    data: &crate::scanmetadata::ScanMetaData,
) -> AudioMetadata {
    let mut v = AudioMetadata {
        ..Default::default()
    };
    // fill v from all tables
    v.path = filepath.to_string();
    let filedata = file(sql_connection.clone(), filepath);
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };
    let audio_id = filedata.metadata_id;
    v.id = audio_id as u32;
    let query = "SELECT name, title, released, poster, thumb, duration, genre, composer, track_id FROM audio_metadata WHERE audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => v.name = val,
                                    Err(error) => {
                                        log::error!("Failed to read name for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.title = val,
                                    Err(error) => {
                                        log::error!("Failed to read title for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.date = val,
                                    Err(error) => {
                                        log::error!("Failed to read date for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.poster = val,
                                    Err(error) => {
                                        log::error!("Failed to read poster for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.thumb = val,
                                    Err(error) => {
                                        log::error!("Failed to read bitrate for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(5) {
                                    Ok(val) => v.duration = val,
                                    Err(error) => {
                                        log::error!("Failed to read duration for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(6) {
                                    Ok(val) => v.genre = val,
                                    Err(error) => {
                                        log::error!("Failed to read genre for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(7) {
                                    Ok(val) => v.composer = val,
                                    Err(error) => {
                                        log::error!("Failed to read composer for audio: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(8) {
                                    Ok(val) => v.track_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read track_id for audio: {}", error);
                                        continue;
                                    }
                                }
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
    let query = "SELECT artist_name FROM artists
                        INNER JOIN artist_audio_map
                        ON artist_audio_map.artist_id = artists.artist_id
                        WHERE artist_audio_map.audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => {
                                    let s: String = val.to_string();
                                    v.artist.push(s);
                                }
                                Err(error) => {
                                    log::error!("Failed to read artists for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from artists: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from artists database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT album_name FROM albums
                        INNER JOIN album_audio_map
                        ON album_audio_map.album_id = albums.album_id
                        WHERE album_audio_map.audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.album = val.clone(),
                                Err(error) => {
                                    log::error!("Failed to read albums for video: {}", error);
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from albums: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from albums database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT artist_name FROM artists
                        INNER JOIN albumartist_audio_map
                        ON albumartist_audio_map.albumartist_id = artists.artist_id
                        WHERE albumartist_audio_map.audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => v.albumartist.push(val.clone()),
                                Err(error) => {
                                    log::error!(
                                        "Failed to read album_artists for audio: {}",
                                        error
                                    );
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from album_artists: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from album_artists database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    let query = "SELECT title, start, end FROM indexes WHERE audio_id = ?1 ORDER BY start, end ASC";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut c = Chapter {
                                    ..Default::default()
                                };
                                match row.get::<usize, String>(0) {
                                    Ok(val) => c.title = val.clone(),
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter title for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get::<usize, f32>(1) {
                                    Ok(val) => c.start = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter start for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get::<usize, f32>(2) {
                                    Ok(val) => c.end = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read chapter end for video: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                v.chapters.push(c);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from sublangs.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from sublangs: {}", error);
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
    let query = "SELECT lyricsfile FROM lyrics WHERE audio_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => match row.get::<usize, String>(0) {
                                Ok(val) => {
                                    let s = val.clone();
                                    v.lyrics.push(s);
                                }
                                Err(error) => {
                                    log::error!(
                                        "Failed to read chapter title for video: {}",
                                        error
                                    );
                                    continue;
                                }
                            },
                            Ok(None) => {
                                //log::warn!("No data read from sublangs.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from sublangs: {}", error);
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
    let query = "SELECT tag_id, tag FROM tags
                        INNER JOIN tags_media_map
                        ON tags_media_map.tagmap_id = tags.tag_id
                        WHERE tags_media_map.media_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&audio_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut tag = Tag {
                                    ..Default::default()
                                };
                                match row.get::<usize, u32>(0) {
                                    Ok(val) => tag.tag_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get::<usize, String>(1) {
                                    Ok(val) => tag.tag = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                v.tags.push(tag);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from tags: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from tags database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    if !data.known_files_contains(PathBuf::from(&v.path)) {
        data.known_files_insert(PathBuf::from(&v.path), filedata.clone());
    }
    v
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct ImageMetadata {
    pub id: u32,
    pub name: String,
    pub title: String,
    pub date: NaiveDate,
    pub path: String,
    pub resized: String,
    pub thumb: String,
    pub width: u32,
    pub height: u32,
    pub photographer: String,
    pub lense_model: String,
    pub focal_length: String,
    pub exposure_time: String,
    pub fnumber: String,
    pub gps_string: String,
    pub gps_latitude: f32,
    pub gps_longitude: f32,
    pub gps_altitude: f32,
    pub tags: Vec<Tag>,
}

impl Default for ImageMetadata {
    fn default() -> ImageMetadata {
        ImageMetadata {
            id: 0,
            name: String::new(),
            title: String::new(),
            date: NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            path: String::new(),
            resized: String::new(),
            thumb: String::new(),
            width: 0,
            height: 0,
            photographer: String::new(),
            lense_model: String::new(),
            focal_length: String::new(),
            exposure_time: String::new(),
            fnumber: String::new(),
            gps_string: String::new(),
            gps_latitude: 0.0,
            gps_longitude: 0.0,
            gps_altitude: 0.0,
            tags: Vec::new(),
        }
    }
}

pub fn insert_image(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut ImageMetadata,
    statdata: &std::fs::Metadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    let image_id = insert_file(sql_connection.clone(), &metadata.path, statdata, 1, data);
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    metadata.id = image_id;
    match connection.execute(
        "INSERT INTO image_metadata (image_id, name, path, created, resized, thumb, width, height, photographer, LenseModel, Focallength, Exposuretime, FNumber, gpsstring, gpslatitude, gpslongitude, gpsaltitude) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![&metadata.id, &metadata.name, &metadata.path, &metadata.date, &metadata.resized, &metadata.thumb, &metadata.width, &metadata.height, &metadata.photographer, &metadata.lense_model, &metadata.focal_length, &metadata.exposure_time, &metadata.fnumber, &metadata.gps_string, &metadata.gps_latitude, &metadata.gps_longitude, &metadata.gps_altitude],
    ) {
        Ok(_retval) => {}, //log::warn!("Inserted {} image with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
        Err(error) => {
            log::warn!("Failed to insert image into  database, updating the existing entry instead: {}", error);
            drop(connection);
            delete_image(sql_connection.clone(), metadata, data);
            insert_image(sql_connection.clone(), metadata, statdata, data);
            return;
        }
    }
    drop(connection);
    for i in 0..metadata.tags.len() {
        let tag_id = insert_tag(
            sql_connection.clone(),
            metadata.id,
            metadata.tags[i].tag.clone(),
        );
        if tag_id == -1 {
            continue;
        }
    }
}

pub fn delete_image(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut ImageMetadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    // Get the index.
    //let index = self.ids[id];
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    let mut image_id: u32 = 0;
    let query = "SELECT metadata_id FROM file_metadata WHERE filepath = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![&metadata.path]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(1) {
                        image_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from file_metadata database: {}", err);
            }
        },
        Err(error) => {
            log::error!(
                "Failed to get video_id for {} from database: {}",
                metadata.path,
                error
            );
            return;
        }
    }
    // clear the entry in the candidates list without deleting it
    let ret = connection.execute(
        "DELETE FROM image_metadata WHERE image_id = ?1",
        params![&image_id],
    );
    if ret.is_err() {
        log::error!("Failed to delete candidate {}!", image_id);
        return;
    }
    data.known_files_remove(PathBuf::from(&metadata.path));
}

pub fn update_image(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata: &mut crate::sql::ImageMetadata,
    statdata: &std::fs::Metadata,
    data: &crate::scanmetadata::ScanMetaData,
) {
    delete_image(sql_connection.clone(), metadata, data);
    insert_image(sql_connection.clone(), metadata, statdata, data);
}

pub fn image_by_id(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    filepath: &str,
    image_id: i64,
) -> ImageMetadata {
    let mut v = ImageMetadata {
        ..Default::default()
    };
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };
    v.path = filepath.to_string();
    v.id = image_id as u32;
    // fill v from all tables
    let query = "SELECT name, path, created, resized, thumb, width, height, Photographer, LenseModel, Focallength, Exposuretime, FNumber, GPSLatitude, GPSLongitude, GPSAltitude FROM image_metadata WHERE image_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&image_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => v.name = val,
                                    Err(error) => {
                                        log::error!("Failed to read name for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.path = val,
                                    Err(error) => {
                                        log::error!("Failed to read title for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.date = val,
                                    Err(error) => {
                                        log::error!("Failed to read date for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.resized = val,
                                    Err(error) => {
                                        log::error!("Failed to read resized for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.thumb = val,
                                    Err(error) => {
                                        log::error!("Failed to read thumb for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(5) {
                                    Ok(val) => v.width = val,
                                    Err(error) => {
                                        log::error!("Failed to read width for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(6) {
                                    Ok(val) => v.height = val,
                                    Err(error) => {
                                        log::error!("Failed to read height for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(7) {
                                    Ok(val) => v.photographer = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read photographer for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(8) {
                                    Ok(val) => v.lense_model = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read lense_model for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(9) {
                                    Ok(val) => v.focal_length = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read focal_length for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(10) {
                                    Ok(val) => v.exposure_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read exposure_time for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(11) {
                                    Ok(val) => v.fnumber = val,
                                    Err(error) => {
                                        log::error!("Failed to read fnumber for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(12) {
                                    Ok(val) => v.gps_latitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_latitude for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(13) {
                                    Ok(val) => v.gps_longitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_longitude for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(14) {
                                    Ok(val) => v.gps_altitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_altitude for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
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
    let query = "SELECT tag_id, tag FROM tags
                        INNER JOIN tags_media_map
                        ON tags_media_map.tagmap_id = tags.tag_id
                        WHERE tags_media_map.media_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&image_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut tag = Tag {
                                    ..Default::default()
                                };
                                match row.get::<usize, u32>(0) {
                                    Ok(val) => tag.tag_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get::<usize, String>(1) {
                                    Ok(val) => tag.tag = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                v.tags.push(tag);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from tags: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from tags database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    v
}

pub fn image(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    filepath: &str,
    data: &crate::scanmetadata::ScanMetaData,
) -> ImageMetadata {
    let mut v = ImageMetadata {
        ..Default::default()
    };
    // fill v from all tables
    v.path = filepath.to_string();

    let filedata = file(sql_connection.clone(), filepath);
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };
    let image_id = filedata.metadata_id;
    v.id = image_id as u32;
    let query = "SELECT name, path, created, resized, thumb, width, height, Photographer, LenseModel, Focallength, Exposuretime, FNumber, GPSLatitude, GPSLongitude, GPSAltitude, image_id FROM image_metadata WHERE image_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&image_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => v.name = val,
                                    Err(error) => {
                                        log::error!("Failed to read name for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.path = val,
                                    Err(error) => {
                                        log::error!("Failed to read title for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.date = val,
                                    Err(error) => {
                                        log::error!("Failed to read date for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.resized = val,
                                    Err(error) => {
                                        log::error!("Failed to read resized for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.thumb = val,
                                    Err(error) => {
                                        log::error!("Failed to read thumb for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(5) {
                                    Ok(val) => v.width = val,
                                    Err(error) => {
                                        log::error!("Failed to read width for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(6) {
                                    Ok(val) => v.height = val,
                                    Err(error) => {
                                        log::error!("Failed to read height for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(7) {
                                    Ok(val) => v.photographer = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read photographer for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(8) {
                                    Ok(val) => v.lense_model = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read lense_model for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(9) {
                                    Ok(val) => v.focal_length = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read focal_length for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(10) {
                                    Ok(val) => v.exposure_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read exposure_time for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(11) {
                                    Ok(val) => v.fnumber = val,
                                    Err(error) => {
                                        log::error!("Failed to read fnumber for image: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(12) {
                                    Ok(val) => v.gps_latitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_latitude for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(13) {
                                    Ok(val) => v.gps_longitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_longitude for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(14) {
                                    Ok(val) => v.gps_altitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_altitude for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(15) {
                                    Ok(val) => v.id = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_altitude for image: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
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
    let query = "SELECT tag_id, tag FROM tags
                        INNER JOIN tags_media_map
                        ON tags_media_map.tagmap_id = tags.tag_id
                        WHERE tags_media_map.media_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&image_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut tag = Tag {
                                    ..Default::default()
                                };
                                match row.get::<usize, u32>(0) {
                                    Ok(val) => tag.tag_id = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                match row.get::<usize, String>(1) {
                                    Ok(val) => tag.tag = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for video: {}", error);
                                        continue;
                                    }
                                }
                                v.tags.push(tag);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from tags: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from tags database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }
    if !data.known_files_contains(PathBuf::from(&v.path)) {
        data.known_files_insert(PathBuf::from(&v.path), filedata.clone());
    }
    v
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Hash)]
pub struct FileMetadata {
    pub filepath: PathBuf,
    pub creation_time: u64,
    pub modification_time: u64,
    pub file_type: i32,
    pub metadata_id: i64,
}

impl Default for FileMetadata {
    fn default() -> FileMetadata {
        FileMetadata {
            filepath: PathBuf::new(),
            creation_time: 0,
            modification_time: 0,
            file_type: 0,
            metadata_id: -1,
        }
    }
}

pub fn file_by_id(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    metadata_id: i64,
) -> FileMetadata {
    let mut v = FileMetadata {
        ..Default::default()
    };
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };
    let query = "SELECT * FROM file_metadata WHERE metadata_id = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![&metadata_id]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => {
                                        let st: String = val;
                                        v.filepath = PathBuf::from(&st);
                                    }
                                    Err(error) => {
                                        log::error!("Failed to read id for file: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.creation_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read creation_time for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.modification_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read modification_time for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.file_type = val,
                                    Err(error) => {
                                        log::error!("Failed to read file_type for file: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.metadata_id = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read metadata_id for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
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

    v
}

pub fn file(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    filepath: &str,
) -> FileMetadata {
    let mut v = FileMetadata {
        ..Default::default()
    };
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return v;
        }
    };
    let query = "SELECT * FROM file_metadata WHERE filepath = ?1";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![filepath]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                match row.get(0) {
                                    Ok(val) => {
                                        let st: String = val;
                                        v.filepath = PathBuf::from(&st);
                                    }
                                    Err(error) => {
                                        log::error!("Failed to read filepath for file: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.creation_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read creation_time for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => v.modification_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read modification_time for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.file_type = val,
                                    Err(error) => {
                                        log::error!("Failed to read file_type for file: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.metadata_id = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read metadata_id for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
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

    v
}

pub fn files(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> std::collections::BTreeMap<PathBuf, FileMetadata> {
    let mut known_files = BTreeMap::new();
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return known_files;
        }
    };
    let query = "SELECT * FROM file_metadata";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut s = FileMetadata {
                                    ..Default::default()
                                };
                                match row.get(0) {
                                    Ok(val) => {
                                        let st: String = val;
                                        s.filepath = PathBuf::from(&st);
                                    }
                                    Err(error) => {
                                        log::error!("Failed to read filepath for file: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => s.creation_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read creation_time for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => s.modification_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read modification_time for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => s.file_type = val,
                                    Err(error) => {
                                        log::error!("Failed to read file_type for file: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => s.metadata_id = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read metadata_id for file: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                known_files.insert(s.filepath.clone(), s.clone());
                                if s.file_type == 3 {
                                    let mut thumbstring = crate::parsers::osstr_to_string(
                                        s.filepath.clone().into_os_string(),
                                    );
                                    thumbstring.extend(".png".to_string().chars());
                                    let thumbpath = PathBuf::from(&thumbstring);
                                    if !known_files.contains_key(&thumbpath) {
                                        known_files.insert(thumbpath, s.clone());
                                    }
                                    let mut lyricstring = crate::parsers::osstr_to_string(
                                        s.filepath.clone().into_os_string(),
                                    );
                                    lyricstring.extend(".lrc".to_string().chars());
                                    let lyricpath = PathBuf::from(&lyricstring);
                                    if !known_files.contains_key(&lyricpath) {
                                        known_files.insert(lyricpath, s);
                                    }
                                } else if s.file_type == 2 {
                                    let mut thumbstring = crate::parsers::osstr_to_string(
                                        s.filepath.clone().into_os_string(),
                                    );
                                    thumbstring.extend("_001.jpeg".to_string().chars());
                                    let thumbpath = PathBuf::from(&thumbstring);
                                    if !known_files.contains_key(&thumbpath) {
                                        known_files.insert(thumbpath, s);
                                    }
                                }
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

    known_files
}

pub fn insert_file(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    path: &str,
    metadata: &std::fs::Metadata,
    file_type: i32,
    data: &crate::scanmetadata::ScanMetaData,
) -> u32 {
    let mut creation_time: u64 = 0;
    if let Ok(created) = metadata.created() {
        match created.duration_since(std::time::UNIX_EPOCH) {
            Ok(n) => creation_time = n.as_secs(),
            Err(_) => panic!("SystemTime before UNIX EPOCH!"),
        }
    }
    let mut modification_time: u64 = 0;
    if let Ok(modified) = metadata.modified() {
        match modified.duration_since(std::time::UNIX_EPOCH) {
            Ok(n) => modification_time = n.as_secs(),
            Err(_) => panic!("SystemTime before UNIX EPOCH!"),
        }
    }
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return 0;
        }
    };
    match connection.execute(
        "INSERT INTO file_metadata (filepath, creation_time, modification_time,
            file_type) VALUES (?1, ?2, ?3, ?4)",
        params![&path, &creation_time, &modification_time, &file_type],
    ) {
        Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
        Err(error) => {
            log::warn!(
                "Failed to insert file into  database, updating the existing entry instead: {}",
                error
            );
            drop(connection);
            delete_file(sql_connection.clone(), path, data);
            insert_file(sql_connection.clone(), path, metadata, file_type, data);
            return 0;
        }
    }
    let mut metadata_id = 0;
    let query = "SELECT last_insert_rowid()";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(0) {
                        metadata_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from audio_metadata database: {}", err);
            }
        },
        Err(error) => {
            log::error!("Failed to get audio_id for from database: {}", error);
            return 0;
        }
    }

    let meta = FileMetadata {
        filepath: PathBuf::from(&path),
        creation_time,
        modification_time,
        file_type,
        metadata_id,
        ..Default::default()
    };
    data.known_files_insert(meta.filepath.clone(), meta.clone());

    if file_type == 3 {
        let mut thumbstring = path.to_string();
        thumbstring.extend(".png".to_string().chars());
        let thumbpath = PathBuf::from(&thumbstring);
        if !data.known_files_contains(thumbpath.clone()) {
            data.known_files_insert(thumbpath.clone(), meta.clone());
        }
        let mut lyricstring = path.to_string();
        lyricstring.extend(".lrc".to_string().chars());
        let lyricpath = PathBuf::from(&lyricstring);
        if !data.known_files_contains(lyricpath.clone()) {
            data.known_files_insert(lyricpath, meta);
        }
    } else if file_type == 2 {
        let mut thumbstring = path.to_string();
        thumbstring.extend("_001.jpeg".to_string().chars());
        let thumbpath = PathBuf::from(&thumbstring);
        if !data.known_files_contains(thumbpath.clone()) {
            data.known_files_insert(thumbpath, meta);
        }
    }
    metadata_id as u32
}

pub fn delete_file(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    path: &str,
    data: &crate::scanmetadata::ScanMetaData,
) {
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    let _ret = connection.execute(
        "DELETE FROM file_metadata WHERE filepath = ?1",
        params![&path],
    );
    data.known_files_remove(PathBuf::from(path));
}

pub fn update_file(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    path: &str,
    metadata: &std::fs::Metadata,
    file_type: i32,
    data: &crate::scanmetadata::ScanMetaData,
) {
    delete_file(sql_connection.clone(), path, data);
    insert_file(sql_connection.clone(), path, metadata, file_type, data);
}

pub fn insert_search(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    s: SearchData,
) -> u32 {
    let mut search_id = 0;
    let fromvalue = (s.from_value / 1000000) as f32;
    let tovalue = (s.to_value / 1000000) as f32;
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return 0;
        }
    };
    match connection.execute(
        "INSERT INTO searches (from_string, from_value, from_date, to_string, to_value, to_date,
                image, video, audio, filepath, title, description,
                actor, director, artist, album_artist,
                duration, creation_date, modification_date, release_date,
                lense_model, focal_length, exposure_time, fnumber,
                gps_latitude, gps_longitude, gps_altitude,
                album, composer, genre, tags) VALUES
                (?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16,
                ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24,
                ?25, ?26, ?27,
                ?28, ?29, ?30, ?31)",
        params![
            &s.from_string.to_ascii_lowercase(),
            &fromvalue,
            &s.from_date,
            &s.to_string.to_ascii_lowercase(),
            &tovalue,
            &s.to_date,
            &s.image,
            &s.video,
            &s.audio,
            &s.filepath,
            &s.title,
            &s.description,
            &s.actor,
            &s.director,
            &s.artist,
            &s.album_artist,
            &s.duration,
            &s.creation_date,
            &s.modification_date,
            &s.release_date,
            &s.lense_model,
            &s.focal_length,
            &s.exposure_time,
            &s.fnumber,
            &s.gps_latitude,
            &s.gps_longitude,
            &s.gps_altitude,
            &s.album,
            &s.composer,
            &s.genre,
            &s.tags
        ],
    ) {
        Ok(_retval) => {} //log::warn!("Inserted {} video with ID {} and location {} into candidates.", video.id, video.index, candidate_id),
        Err(error) => {
            log::error!("Failed to insert search into searches database: {}", error);
            return search_id;
        }
    }
    let query = "SELECT last_insert_rowid()";
    match connection.prepare(query) {
        Ok(mut statement) => match statement.query(params![]) {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(val) = row.get(0) {
                        search_id = val;
                    }
                }
            }
            Err(err) => {
                log::error!("could not read line from audio_metadata database: {}", err);
            }
        },
        Err(error) => {
            log::error!("Failed to get audio_id for from database: {}", error);
            return search_id;
        }
    }
    search_id
}

pub fn delete_search(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    s: SearchData,
) {
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return;
        }
    };
    let _ret = connection.execute(
        "DELETE FROM searches WHERE search_id = ?1",
        params![&s.search_id],
    );
}

pub fn update_search(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    s: SearchData,
) {
    delete_search(sql_connection.clone(), s.clone());
    insert_search(sql_connection.clone(), s);
}

pub fn searches(
    sql_connection: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> Vec<SearchData> {
    let mut searches = Vec::new();
    let connection = match sql_connection.lock() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Failed to lock sql connection for use! {}", error);
            return searches;
        }
    };
    let query = "SELECT * FROM searches";
    match connection.prepare(query) {
        Ok(mut statement) => {
            match statement.query(params![]) {
                Ok(mut rows) => {
                    loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let mut v = SearchData {
                                    ..Default::default()
                                };
                                match row.get(0) {
                                    Ok(val) => {
                                        v.search_id = val;
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read search_id for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(1) {
                                    Ok(val) => v.from_string = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read from_string for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(2) {
                                    Ok(val) => {
                                        let f: f32 = val;
                                        v.from_value = (f * 1000000.0) as u64;
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read from_value for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(3) {
                                    Ok(val) => v.from_date = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read from_date for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(4) {
                                    Ok(val) => v.to_string = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read to_string for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(5) {
                                    Ok(val) => {
                                        let f: f32 = val;
                                        v.to_value = (f * 1000000.0) as u64;
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read to_value for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(6) {
                                    Ok(val) => v.to_date = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read to_date for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(7) {
                                    Ok(val) => v.image = val,
                                    Err(error) => {
                                        log::error!("Failed to read image for searches: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(8) {
                                    Ok(val) => v.video = val,
                                    Err(error) => {
                                        log::error!("Failed to read video for searches: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(9) {
                                    Ok(val) => v.audio = val,
                                    Err(error) => {
                                        log::error!("Failed to read audio for searches: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(10) {
                                    Ok(val) => v.filepath = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read filepath for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(11) {
                                    Ok(val) => v.title = val,
                                    Err(error) => {
                                        log::error!("Failed to read title for searches: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(12) {
                                    Ok(val) => v.description = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read description for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(13) {
                                    Ok(val) => v.actor = val,
                                    Err(error) => {
                                        log::error!("Failed to read actor for searches: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(14) {
                                    Ok(val) => v.director = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read director for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(15) {
                                    Ok(val) => v.artist = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read artist for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(16) {
                                    Ok(val) => v.album_artist = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read album_artist for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(17) {
                                    Ok(val) => v.duration = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read duration for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(18) {
                                    Ok(val) => v.creation_date = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read creation_date for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(19) {
                                    Ok(val) => v.modification_date = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read modification_date for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(20) {
                                    Ok(val) => v.release_date = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read release_date for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(21) {
                                    Ok(val) => v.lense_model = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read lense_model for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(22) {
                                    Ok(val) => v.focal_length = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read focal_length for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(23) {
                                    Ok(val) => v.exposure_time = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read exposure_time for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(24) {
                                    Ok(val) => v.fnumber = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read fnumber for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(25) {
                                    Ok(val) => v.gps_latitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_latitude for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(26) {
                                    Ok(val) => v.gps_longitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_longitude for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(27) {
                                    Ok(val) => v.gps_altitude = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read gps_altitude for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(28) {
                                    Ok(val) => v.album = val,
                                    Err(error) => {
                                        log::error!("Failed to read album for searches: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(29) {
                                    Ok(val) => v.genre = val,
                                    Err(error) => {
                                        log::error!("Failed to read genre for searches: {}", error);
                                        continue;
                                    }
                                }
                                match row.get(30) {
                                    Ok(val) => v.composer = val,
                                    Err(error) => {
                                        log::error!(
                                            "Failed to read composer for searches: {}",
                                            error
                                        );
                                        continue;
                                    }
                                }
                                match row.get(31) {
                                    Ok(val) => v.tags = val,
                                    Err(error) => {
                                        log::error!("Failed to read tags for searches: {}", error);
                                        continue;
                                    }
                                }

                                searches.push(v);
                            }
                            Ok(None) => {
                                //log::warn!("No data read from indices.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from searches: {}", error);
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("could not read line from searches database: {}", err);
                }
            }
        }
        Err(err) => {
            log::error!("could not prepare SQL statement: {}", err);
        }
    }

    searches
}

/// Resolve the user's home directory. Tries `dirs::home_dir()` first,
/// then falls back to the `$HOME` environment variable. Returns `None`
/// only if neither source is available (effectively impossible on a
/// normal login session).
pub fn home_dir() -> Option<PathBuf> {
    if let Some(dir) = dirs::home_dir() {
        return Some(dir);
    }
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Fallback used when `home_dir()` returns `None`: log the error and
/// use the filesystem root so the app degrades gracefully instead of
/// panicking.
pub fn home_dir_or_root() -> PathBuf {
    match home_dir() {
        Some(dir) => dir,
        None => {
            log::error!("Unable to determine home directory; falling back to /");
            PathBuf::from("/")
        }
    }
}

pub fn connect() -> Result<rusqlite::Connection, rusqlite::Error> {
    let sqlite_file;
    let connection;
    match dirs::data_local_dir() {
        Some(pb) => {
            let mut dir = pb.join("media-browser");
            if !dir.exists() {
                let ret = std::fs::create_dir_all(dir.clone());
                if ret.is_err() {
                    log::warn!("Failed to create directory {}", dir.display());
                    dir = home_dir_or_root();
                }
            }
            sqlite_file = dir.join("metadata.sqlite");
        }
        None => {
            let dir = home_dir_or_root();
            sqlite_file = dir.join("metadata.sqlite");
        }
    }
    if !sqlite_file.is_file() {
        connection = Connection::open(sqlite_file)?;
        // Enable WAL and a 5s busy timeout to reduce writer/reader contention
        // and avoid failing immediately on a locked database.
        connection.execute("PRAGMA journal_mode=WAL", ()).ok();
        connection.execute("PRAGMA busy_timeout=5000", ()).ok();
        // file_type: 0 other file, 1 image, 2, video, 3 audio
        match connection.execute(
            "CREATE TABLE file_metadata (
                filepath TEXT NOT NULL unique NOT NULL,
                creation_time UNSIGNED BIG INT,
                modification_time UNSIGNED BIG INT,
                file_type INT,
                metadata_id INTEGER,
                PRIMARY KEY(metadata_id AUTOINCREMENT)
            )",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table file_metadata: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_file_metadata_filename ON file_metadata (filepath)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on file_metadata: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE video_metadata (
                video_id INTEGER PRIMARY KEY,
                name  TEXT NOT NULL,
                title  TEXT NOT NULL,
                released UNSIGNED BIG INT NOT NULL,
                poster  TEXT,
                thumb   TEXT,
                subtitles BIG INT,
                duration INT,
                width INT,
                height INT,
                framerate FLOAT,
                description TEXT
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table indices: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_video_metadata_title ON video_metadata (title)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on file_metadata: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE subtitles (
                subtitle_id INTEGER,
                video_id INTEGER,
                subpath TEXT,
                PRIMARY KEY(subtitle_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_subtitles_video_id ON subtitles (video_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on subtitles: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE audiolangs (
                audiolang_id INTEGER,
                video_id INTEGER,
                audiolang TEXT,
                PRIMARY KEY(audiolang_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_audiolangs_video_id ON audiolangs (video_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on audiolangs: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE sublangs (
                sublang_id INTEGER,
                video_id INTEGER,
                sublang TEXT,
                PRIMARY KEY(sublang_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_sublangs_video_id ON sublangs (video_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on sublangs: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE chapters (
                chapter_id INTEGER,
                video_id INTEGER,
                title TEXT,
                start DOUBLE,
                end DOUBLE,
                PRIMARY KEY(chapter_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_chapters_video_id ON chapters (video_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on sublangs: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
        CREATE TABLE people (
            person_id INTEGER,
            person_name TEXT,
            PRIMARY KEY(person_id AUTOINCREMENT)
        )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table people: {}", error);
                return Err(error);
            }
        }
        match connection.execute("CREATE INDEX index_people_name ON people (person_name)", ()) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on directors: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE directors (
                entry_id INTEGER,
                director_id INTEGER,
                video_id INTEGER,
                PRIMARY KEY(entry_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table directors: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_directors_video_id ON directors (video_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on directors: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE actors (
                entry_id INTEGER,
                actor_id INTEGER,
                video_id INTEGER,
                PRIMARY KEY(entry_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_actors_video_id ON actors (video_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on actors: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
        CREATE TABLE collections (
            collection_id INTEGER,
            file_id INTEGER,
            collection_name TEXT,
            poster TEXT,
            thumb TEXT,
            description TEXT,
            path TEXT,
            PRIMARY KEY(collection_id AUTOINCREMENT)
        )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table collections: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX collection_file_id ON collections (file_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on collections_file_id: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX collection_collection_name ON collections (collection_name)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on collection_name: {}", error);
                return Err(error);
            }
        }

        match connection.execute(
            "
            CREATE TABLE collections_map (
                entry_id INTEGER,
                collection_id INTEGER,
                episode_id INTEGER,
                series INTEGER,
                episode INTEGER,
                title TEXT,
                path TEXT,
                poster TEXT,
                thumb TEXT,
                PRIMARY KEY(entry_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table collections_map: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX collection_map_episode_id ON collections_map (episode_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on collection_name: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE audio_metadata (
                audio_id INTEGER PRIMARY KEY,
                name  TEXT NOT NULL,
                title  TEXT NOT NULL,
                released UNSIGNED BIG INT NOT NULL,
                poster  TEXT,
                thumb   TEXT,
                genre   TEXT,
                composer TEXT,
                track_id INT,
                duration INT,
                bitrate FLOAT
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table indices: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE albums (
                album_id INTEGER,
                album_name TEXT,
                PRIMARY KEY(album_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute("CREATE INDEX index_albums_name ON albums (album_name)", ()) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on albums: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
        CREATE TABLE album_audio_map (
            entry_id INTEGER,
            audio_id INTEGER,
            album_id INTEGER,
            PRIMARY KEY(entry_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table albums: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_album_audio_audio_id ON album_audio_map (audio_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on albums: {}", error);
                return Err(error);
            }
        }

        match connection.execute(
            "
            CREATE TABLE artists (
                artist_id INTEGER,
                artist_name TEXT,
                PRIMARY KEY(artist_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table artists: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_artists_name_id ON artists (artist_name)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on artists: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
        CREATE TABLE artist_audio_map (
            entry_id INTEGER,
            audio_id INTEGER,
            artist_id INTEGER,
            PRIMARY KEY(entry_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table artist_audio_map: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_artist_audio_map_audio_id ON artist_audio_map (audio_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on artist_audio_map: {}", error);
                return Err(error);
            }
        }

        match connection.execute(
            "
        CREATE TABLE albumartist_audio_map (
            entry_id INTEGER,
            audio_id INTEGER,
            albumartist_id INTEGER,
            PRIMARY KEY(entry_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table albumartist_audio_map: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_albumartist_audio_map_audio_id ON artist_audio_map (audio_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on albumartist_audio_map: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE indexes (
                index_id INTEGER,
                audio_id INTEGER,
                title TEXT,
                start DOUBLE,
                end DOUBLE,
                PRIMARY KEY(index_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_indexes_audio_id ON indexes (audio_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on artist_audio_map: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE lyrics (
                lyrics_id INTEGER,
                audio_id INTEGER,
                lyricsfile TEXT,
                PRIMARY KEY(lyrics_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_lyrics_audio_id ON lyrics (audio_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on lyrics: {}", error);
                return Err(error);
            }
        }

        match connection.execute(
            "
            CREATE TABLE image_metadata (
                image_id INTEGER PRIMARY KEY,
                name     TEXT NOT NULL,
                path     TEXT NOT NULL,
                created  UNSIGNED BIG INT NOT NULL,
                resized  TEXT,
                thumb    TEXT,
                width    INTEGER,
                height   INTEGER,
                Photographer TEXT,
                LenseModel TEXT,
                Focallength TEXT,
                Exposuretime TEXT,
                FNumber  TEXT,
                GPSString  TEXT,
                GPSLatitude DOUBLE,
                GPSLongitude DOUBLE,
                GPSAltitude DOUBLE
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table image_metadata: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_image_photographer ON image_metadata (Photographer)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on image_metadata: {}", error);
                return Err(error);
            }
        }
        match connection.execute("CREATE INDEX index_image_name ON image_metadata (name)", ()) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on image_metadata: {}", error);
                return Err(error);
            }
        }

        match connection.execute(
            "
            CREATE TABLE tags (
                tag_id INTEGER,
                tag TEXT,
                PRIMARY KEY(tag_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute("CREATE INDEX index_tags_tag ON tags (tag)", ()) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on tags: {}", error);
                return Err(error);
            }
        }

        match connection.execute(
            "
            CREATE TABLE tags_media_map (
                element_id INTEGER,
                media_id INTEGER,
                tagmap_id INTEGER,
                PRIMARY KEY(element_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        /*
                match connection.execute(
                    "CREATE INDEX index_tagsmediamap_media_id ON tags_media_map (media_id)", (),
                ) {
                    Ok(_ret) => {},
                    Err(error) => {
                        log::error!("Failed to create index on tags_media_map: {}", error);
                        return Err(error);
                    }
                }
                match connection.execute(
                    "CREATE INDEX index_tagsmediamap_tag_id ON tags_media_map (tagmap_id)", (),
                ) {
                    Ok(_ret) => {},
                    Err(error) => {
                        log::error!("Failed to create index on tags_media_map: {}", error);
                        return Err(error);
                    }
                }
        */
        match connection.execute(
            "
            CREATE TABLE searches (
                search_id INTEGER,
                from_string  TEXT,
                from_value DOUBLE,
                from_date INTEGER,
                to_string  TEXT,
                to_value DOUBLE,
                to_date INTEGER,
                image  INTEGER,
                video  INTEGER,
                audio  INTEGER,
                filepath  INTEGER,
                title  INTEGER,
                description  INTEGER,
                actor  INTEGER,
                director  INTEGER,
                artist  INTEGER,
                album_artist  INTEGER,
                duration  INTEGER,
                creation_date  INTEGER,
                modification_date  INTEGER,
                release_date  INTEGER,
                lense_model  INTEGER,
                focal_length  INTEGER,
                exposure_time  INTEGER,
                fnumber  INTEGER,
                gps_latitude  INTEGER,
                gps_longitude  INTEGER,
                gps_altitude  INTEGER,
                album  INTEGER,
                composer  INTEGER,
                genre  INTEGER,
                tags  INTEGER,
                PRIMARY KEY(search_id AUTOINCREMENT)
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table image_metadata: {}", error);
                return Err(error);
            }
        }
    } else {
        connection = Connection::open(sqlite_file)?;
    }
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_term_wraps_and_escapes() {
        assert_eq!(escape_like_term("abc"), "%abc%");
        assert_eq!(escape_like_term(""), "%%");
        // Wildcards in user input must be neutralised so they match literally.
        assert_eq!(escape_like_term("100%"), "%100\\%%");
        assert_eq!(escape_like_term("a_b"), "%a\\_b%");
        // The escape character itself must be escaped.
        assert_eq!(escape_like_term("a\\b"), "%a\\\\b%");
    }

    #[test]
    fn search_data_partial_eq_distinguishes_gps() {
        let base = SearchData::default();
        assert_eq!(base, SearchData::default());

        // Two fields that only differ must NOT compare equal. (Regression
        // guard for the previously duplicated gps_longitude comparison.)
        let mut lon = SearchData::default();
        lon.gps_longitude = true;
        assert_ne!(base, lon);

        let mut lat = SearchData::default();
        lat.gps_latitude = true;
        assert_ne!(base, lat);
    }

    /// In-memory DB with the minimal schema the search paths need, plus one
    /// video whose title contains a single quote (to exercise LIKE escaping
    /// and parameterized binding).
    fn test_db() -> std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        conn.execute(
            "CREATE TABLE video_metadata (
                video_id INTEGER PRIMARY KEY,
                name TEXT, title TEXT, released INTEGER, poster TEXT, thumb TEXT,
                duration INTEGER, width INTEGER, height INTEGER, framerate REAL,
                description TEXT
            )",
            [],
        )
        .expect("create video_metadata");
        conn.execute(
            "CREATE TABLE file_metadata (
                metadata_id INTEGER PRIMARY KEY,
                filepath TEXT, creation_time INTEGER, modification_time INTEGER,
                file_type INTEGER
            )",
            [],
        )
        .expect("create file_metadata");
        // Title contains a single quote: a `format!`-built query would break.
        conn.execute(
            "INSERT INTO video_metadata (video_id, name, title)
             VALUES (1, 'movie.mkv', \"O'Malley's Gang\")",
            [],
        )
        .expect("insert video");
        conn.execute(
            "INSERT INTO file_metadata (metadata_id, filepath, file_type)
             VALUES (1, '/tmp/movie.mkv', 2)",
            [],
        )
        .expect("insert file");
        conn.execute(
            "CREATE TABLE image_metadata (
                image_id INTEGER PRIMARY KEY,
                name TEXT, path TEXT, created INTEGER, resized TEXT, thumb TEXT,
                width INTEGER, height INTEGER, Photographer TEXT, LenseModel TEXT,
                Focallength TEXT, Exposuretime TEXT, FNumber TEXT, GPSString TEXT,
                GPSLatitude REAL, GPSLongitude REAL, GPSAltitude REAL
            )",
            [],
        )
        .expect("create image_metadata");
        conn.execute(
            "CREATE TABLE tags (
                tag_id INTEGER PRIMARY KEY,
                tag TEXT
            )",
            [],
        )
        .expect("create tags");
        conn.execute(
            "CREATE TABLE tags_media_map (
                tagmap_id INTEGER PRIMARY KEY,
                media_id INTEGER,
                tag_id INTEGER
            )",
            [],
        )
        .expect("create tags_media_map");
        // Name contains a single quote: a `format!`-built query would break.
        conn.execute(
            "INSERT INTO image_metadata (image_id, name, path, created, GPSLatitude, GPSLongitude)
             VALUES (7, ?1, '/tmp/photo.jpg', 0, 1.0, 2.0)",
            ["Smith's Trip"],
        )
        .expect("insert image");
        conn.execute(
            "INSERT INTO file_metadata (metadata_id, filepath, file_type)
             VALUES (7, '/tmp/photo.jpg', 1)",
            [],
        )
        .expect("insert image file");
        conn.execute("INSERT INTO tags (tag_id, tag) VALUES (1, 'holiday')", [])
            .expect("insert tag");
        conn.execute(
            "INSERT INTO tags_media_map (tagmap_id, media_id, tag_id) VALUES (1, 7, 1)",
            [],
        )
        .expect("insert tag map");
        // A second image with a real created date (2020-01-01T00:00:00Z =
        // 1577836800) so the image date-range search path can be exercised.
        // The `created` column stores a Unix timestamp (rusqlite stores
        // NaiveDate as i64 seconds-since-epoch), matching the `from_date` /
        // `to_date` Unix-timestamp bounds used in the range query.
        conn.execute(
            "INSERT INTO image_metadata (image_id, name, path, created)
             VALUES (8, 'Dated Shot', '/tmp/dated.jpg', 1577836800)",
            [],
        )
        .expect("insert dated image");
        conn.execute(
            "INSERT INTO file_metadata (metadata_id, filepath, file_type)
             VALUES (8, '/tmp/dated.jpg', 1)",
            [],
        )
        .expect("insert dated image file");
        std::sync::Arc::new(std::sync::Mutex::new(conn))
    }

    #[test]
    fn search_video_matches_title_containing_single_quote() {
        let conn = test_db();
        let mut search = SearchData::default();
        search.video = true;
        search.title = true;
        search.from_string = "O'Malley".to_string();

        let (videos, files) = search_video(conn, &search);
        assert_eq!(videos.len(), 1, "single-quote title should match");
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn search_video_rejects_sql_injection_term() {
        let conn = test_db();
        let mut search = SearchData::default();
        search.video = true;
        search.title = true;
        // If naively interpolated this becomes `LIKE '%%' OR '1'='1%'` and
        // matches every row. Bound as a parameter it must match nothing.
        search.from_string = "%' OR '1'='1".to_string();

        let (videos, files) = search_video(conn, &search);
        assert_eq!(videos.len(), 0, "injection term must not match");
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn search_video_case_insensitive_substring() {
        let conn = test_db();
        let mut search = SearchData::default();
        search.video = true;
        search.title = true;
        search.from_string = "gang".to_string(); // title is "O'Malley's Gang"

        let (videos, files) = search_video(conn, &search);
        assert_eq!(videos.len(), 1, "substring should match case-insensitively");
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn search_image_matches_name_containing_single_quote() {
        let conn = test_db();
        let mut search = SearchData::default();
        search.image = true;
        search.title = true;
        search.from_string = "Smith's".to_string();

        let (images, files) = search_image(conn, &search);
        assert_eq!(images.len(), 1, "single-quote name should match");
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].filepath,
            std::path::PathBuf::from("/tmp/photo.jpg")
        );
        assert_eq!(
            images[0].gps_latitude, 1.0,
            "joined row must carry GPS data"
        );
    }

    #[test]
    fn search_image_matches_tag_via_join() {
        let conn = test_db();
        let mut search = SearchData::default();
        search.image = true;
        search.title = false;
        search.tags = true;
        search.from_string = "holiday".to_string();

        let (images, files) = search_image(conn, &search);
        assert_eq!(
            images.len(),
            1,
            "tag search should resolve through the join"
        );
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn search_image_rejects_sql_injection_term() {
        let conn = test_db();
        let mut search = SearchData::default();
        search.image = true;
        search.title = true;
        // If naively interpolated this becomes `LIKE '%%' OR '1'='1%'` and
        // matches every row. Bound as a parameter it must match nothing.
        search.from_string = "%' OR '1'='1".to_string();

        let (images, files) = search_image(conn, &search);
        assert_eq!(images.len(), 0, "injection term must not match");
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn search_image_date_range_matches_only_dated_image() {
        let conn = test_db();
        let mut search = SearchData::default();
        search.image = true;
        search.title = false; // only the release_date branch should run
        search.release_date = true;
        // 2020-01-01 (1577836800) is bracketed so the dated image (id 8)
        // matches while the epoch image (id 7, created = 0) does not.
        search.from_date = 1_577_800_000;
        search.to_date = 1_577_870_000;

        let (images, files) = search_image(conn, &search);
        assert_eq!(
            images.len(),
            1,
            "date range must match only the dated image"
        );
        assert_eq!(files.len(), 1);
        assert_eq!(images[0].id, 8);
    }

    #[test]
    fn search_image_gps_range_injection_does_not_match() {
        let conn = test_db();
        let mut search = SearchData::default();
        search.image = true;
        search.title = false;
        search.gps_latitude = true;
        search.gps_longitude = true;
        // from_value is scaled by 1e6 before the `/ 1000000` integer division
        // in the query builder; any value under 1e6 becomes 0, which is still
        // a legitimate bound for stored values >= 0, so use a bound that is
        // known to exclude the sample row (stored at lat 1.0 / lon 2.0).
        search.from_value = 2_000_000_000; // 2.0 in scaled units

        let (images, _files) = search_image(conn, &search);
        assert_eq!(images.len(), 0, "GPS range must exclude the sample image");
    }
}
