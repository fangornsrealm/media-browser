// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod audio;
mod collection;
mod file;
mod image;
mod search;
mod search_history;
mod tags;
mod video;
pub use audio::*;
pub use collection::*;
pub use file::*;
pub use image::*;
pub use search::*;
pub use search_history::*;
pub use tags::*;
pub use video::*;

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
