//! Tag + tag-media mapping CRUD (the `tags` and `tags_media_map` tables).
//!
//! Split out of `sql.rs` (Phase 3 — modularization). Re-exported from the
//! parent via `pub use tags::*;` so existing `crate::sql::` call-sites keep
//! working unchanged. Depends only on the parent's `Tag` type and `rusqlite`.

use super::Tag;
use rusqlite::params;

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

