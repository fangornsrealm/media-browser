//! Search-history CRUD (the `searches` table).
//!
//! Split out of `sql.rs` (Phase 3 — modularization). Re-exported from the
//! parent via `pub use search_history::*;` so existing `crate::sql::`
//! call-sites keep working unchanged. Depends only on the parent's
//! `SearchData` type.

use super::SearchData;
use rusqlite::params;

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
