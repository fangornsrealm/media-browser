//! Video-metadata CRUD (the `video_metadata`, `subtitles`, `audiolangs`,
//! `sublangs`, `chapters`, `actors`, `directors` tables).
//!
//! Split out of `sql.rs` (Phase 3 — modularization). Re-exported from the
//! parent via `pub use video::*;` so existing `crate::sql::` call-sites keep
//! working unchanged.

use super::{file, insert_file, insert_tag, Chapter, Tag};
use chrono::NaiveDate;
use rusqlite::params;
use std::path::PathBuf;

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
