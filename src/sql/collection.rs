//! Collection + episode CRUD (the `collections` table).
//!
//! Split out of `sql.rs` (Phase 3 — modularization). Re-exported from the
//! parent via `pub use collection::*;` so existing `crate::sql::` call-sites
//! keep working unchanged.

use super::{file, file_by_id, insert_file, video_by_id, Tag};
use rusqlite::params;
use std::path::PathBuf;

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
