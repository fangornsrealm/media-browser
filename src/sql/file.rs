//! File-metadata CRUD (the `file_metadata` table).
//!
//! Split out of `sql.rs` (Phase 3 — modularization). Re-exported from the
//! parent via `pub use file::*;` so existing `crate::sql::` call-sites and
//! `mod tests` references keep working unchanged.

use rusqlite::params;
use std::collections::BTreeMap;
use std::path::PathBuf;

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
