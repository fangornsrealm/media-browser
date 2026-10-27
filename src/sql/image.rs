//! Image-metadata CRUD (the `image_metadata` table).
//!
//! Split out of `sql.rs` (Phase 3 — modularization). Re-exported from the
//! parent via `pub use image::*;` so existing `crate::sql::` call-sites keep
//! working unchanged.

use super::{file, insert_file, insert_tag, Tag};
use chrono::NaiveDate;
use rusqlite::params;
use std::path::PathBuf;

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
