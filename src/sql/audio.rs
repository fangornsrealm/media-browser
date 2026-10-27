//! Audio-metadata CRUD (the `audio_metadata`, `albums`, `artists`,
//! `album_audio_map`, `artist_audio_map`, `albumartist_audio_map`,
//! `lyrics` tables).
//!
//! Split out of `sql.rs` (Phase 3 — modularization). Re-exported from the
//! parent via `pub use audio::*;` so existing `crate::sql::` call-sites keep
//! working unchanged.

use super::{file, insert_file, insert_tag, Chapter, Tag};
use chrono::NaiveDate;
use rusqlite::params;
use std::path::PathBuf;

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
