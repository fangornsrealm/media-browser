//! Message handlers for the search context page: the header search input,
//! the advanced search database page, the search history, and the final
//! search commit.
//!
//! Split out of `app.rs` (Phase 3 — modularization). The original
//! `Message::SearchActivate`, `Message::SearchClear`, `Message::SearchInput`,
//! `Message::SearchStart`, `Message::SearchPreviousPick`,
//! `Message::SearchPreviousSelect`, `Message::SearchPreviousDelete`,
//! `Message::SearchImages`, `Message::SearchVideos`, `Message::SearchAudios`,
//! `Message::SearchSearchString`, `Message::SearchSearchStringSubmit`,
//! `Message::SearchSearchFromString`, `Message::SearchSearchFromStringSubmit`,
//! `Message::SearchSearchToString`, `Message::SearchSearchToStringSubmit`,
//! `Message::SearchSearchFromValue`, `Message::SearchSearchFromValueSubmit`,
//! `Message::SearchSearchToValue`, `Message::SearchSearchToValueSubmit`,
//! `Message::SearchFilepath`, `Message::SearchTitle`, `Message::SearchTag`,
//! `Message::SearchDescription`, `Message::SearchActor`,
//! `Message::SearchDirector`, `Message::SearchArtist`,
//! `Message::SearchAlbumartist`, `Message::SearchAlbum`,
//! `Message::SearchComposer`, `Message::SearchGenre`,
//! `Message::SearchDuration`, `Message::SearchCreationDate`,
//! `Message::SearchModificationDate`, `Message::SearchReleaseDate`,
//! `Message::SearchLenseModel`, `Message::SearchFocalLength`,
//! `Message::SearchExposureTime`, `Message::SearchFNumber`,
//! `Message::SearchGpsLatitude`, `Message::SearchGpsLongitude`,
//! `Message::SearchGpsAltitude`, and `Message::SearchCommit` arms of
//! `App::update` are now individual methods on `App`, dispatched from
//! `mod.rs` with a single forwarding line each.

use cosmic::app::Task;
use cosmic::widget;

use super::{App, ContextPage, Message};
use crate::config::IconSizes;
use crate::tab::{Location, Tab};

impl App {
    /// Handle `Message::SearchActivate`.
    ///
    /// Activates the header search input: either focuses the existing input
    /// or initializes an empty search term first.
    pub fn search_activate(&mut self) -> Task<Message> {
        if self.search_get().is_none() {
            self.search_set(Some(String::new()))
        } else {
            widget::text_input::focus(self.search_id.clone())
        }
    }

    /// Handle `Message::SearchClear`.
    ///
    /// Clears the header search input.
    pub fn search_clear(&mut self) -> Task<Message> {
        self.search_set(None)
    }

    /// Handle `Message::SearchInput`.
    ///
    /// Updates the header search input with the given term.
    pub fn search_input(&mut self, input: String) -> Task<Message> {
        self.search_set(Some(input))
    }

    /// Handle `Message::SearchStart`.
    ///
    /// Resets the advanced search state and loads the search history from
    /// the database, then opens the search context page.
    pub fn search_start(&mut self) -> Task<Message> {
        self.search = crate::sql::SearchData {
            ..Default::default()
        };
        self.search_previous.clear();
        self.search_previous
            .extend(crate::sql::searches(self.sql_connection.clone()));
        for s in self.search_previous.iter() {
            self.search_previous_str.push(s.display());
        }
        self.context_page = ContextPage::Search;
        self.core.window.show_context = true;
        Task::none()
    }

    /// Handle `Message::SearchPreviousPick`.
    ///
    /// Records which entry of the search history is currently selected.
    pub fn search_previous_pick(&mut self, pos: usize) -> Task<Message> {
        self.search_previous_pos = pos;
        Task::none()
    }

    /// Handle `Message::SearchPreviousSelect`.
    ///
    /// Loads the selected search history entry into the current search.
    pub fn search_previous_select(&mut self) -> Task<Message> {
        let search = self.search_previous[self.search_previous_pos].clone();
        self.search = search;
        Task::none()
    }

    /// Handle `Message::SearchPreviousDelete`.
    ///
    /// Removes the selected search history entry from both the in-memory
    /// list and the database.
    pub fn search_previous_delete(&mut self) -> Task<Message> {
        let search = self.search_previous[self.search_previous_pos].clone();
        self.search_previous_str.remove(self.search_previous_pos);
        self.search_previous.remove(self.search_previous_pos);
        crate::sql::delete_search(self.sql_connection.clone(), search);
        Task::none()
    }

    /// Handle `Message::SearchImages`.
    ///
    /// Toggles whether image media are included in the search. Any change
    /// invalidates the stored search ID so the next commit stores a new row.
    pub fn search_images(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.image = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchVideos`.
    ///
    /// Toggles whether video media are included in the search.
    pub fn search_videos(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.video = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchAudios`.
    ///
    /// Toggles whether audio media are included in the search.
    pub fn search_audios(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.audio = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchSearchString`.
    ///
    /// Stores the free-form search string being typed.
    pub fn search_search_string(&mut self, input: String) -> Task<Message> {
        self.search.search_id = 0;
        self.search.search_string = input;
        Task::none()
    }

    /// Handle `Message::SearchSearchStringSubmit`.
    ///
    /// Logs the free-form search string (submission is applied when the
    /// search is committed).
    pub fn search_search_string_submit(&mut self) -> Task<Message> {
        self.search.search_id = 0;
        log::warn!("{}", self.search.search_string);
        Task::none()
    }

    /// Handle `Message::SearchSearchFromString`.
    ///
    /// Stores the "from" value being typed (a date string or a numeric
    /// value depending on the searched field).
    pub fn search_search_from_string(&mut self, input: String) -> Task<Message> {
        self.search.search_id = 0;
        self.search.from_string = input;
        Task::none()
    }

    /// Handle `Message::SearchSearchFromStringSubmit`.
    ///
    /// Interprets the "from" value as a linux time (for date fields) or as
    /// a scaled numeric value (for value fields).
    pub fn search_search_from_string_submit(&mut self) -> Task<Message> {
        self.search.search_id = 0;
        log::warn!("{}", self.search.from_string);
        let lt = crate::sql::string_to_linux_time(&self.search.from_string);
        if lt > 0 {
            self.search.from_date = lt as i64;
        }
        match self.search.from_string.parse::<f32>() {
            Ok(float) => self.search.from_value = (float * 1000000.0) as u64,
            Err(_) => {}
        }
        Task::none()
    }

    /// Handle `Message::SearchSearchToString`.
    ///
    /// Stores the "to" value being typed (a date string or a numeric value
    /// depending on the searched field).
    pub fn search_search_to_string(&mut self, input: String) -> Task<Message> {
        self.search.search_id = 0;
        self.search.to_string = input;
        Task::none()
    }

    /// Handle `Message::SearchSearchToStringSubmit`.
    ///
    /// Interprets the "to" value as a linux time (for date fields) or as a
    /// scaled numeric value (for value fields).
    pub fn search_search_to_string_submit(&mut self) -> Task<Message> {
        self.search.search_id = 0;
        log::warn!("{}", self.search.to_string);
        let lt = crate::sql::string_to_linux_time(&self.search.to_string);
        if lt > 0 {
            self.search.to_date = lt as i64;
        }
        match self.search.to_string.parse::<f32>() {
            Ok(float) => self.search.to_value = (float * 1000000.0) as u64,
            Err(_) => {}
        }
        Task::none()
    }

    /// Handle `Message::SearchSearchFromValue`.
    ///
    /// Stores the "from" value as a string and also interprets it as a
    /// scaled numeric value.
    pub fn search_search_from_value(&mut self, input: String) -> Task<Message> {
        self.search.search_id = 0;
        self.search.from_value_string = input;
        let float = crate::parsers::string_to_float(&self.search.from_value_string);
        self.search.from_value = (float * 1000000.0) as u64;
        Task::none()
    }

    /// Handle `Message::SearchSearchFromValueSubmit`.
    ///
    /// Logs the current search string (submission is applied when the
    /// search is committed).
    pub fn search_search_from_value_submit(&mut self) -> Task<Message> {
        self.search.search_id = 0;
        log::warn!("{}", self.search.search_string);
        Task::none()
    }

    /// Handle `Message::SearchSearchToValue`.
    ///
    /// Stores the "to" value as a string and also interprets it as a
    /// scaled numeric value.
    pub fn search_search_to_value(&mut self, input: String) -> Task<Message> {
        self.search.search_id = 0;
        self.search.to_value_string = input;
        let float = crate::parsers::string_to_float(&self.search.to_value_string);
        self.search.to_value = (float * 1000000.0) as u64;
        Task::none()
    }

    /// Handle `Message::SearchSearchToValueSubmit`.
    ///
    /// Logs the current "to" value (submission is applied when the search
    /// is committed).
    pub fn search_search_to_value_submit(&mut self) -> Task<Message> {
        self.search.search_id = 0;
        log::warn!("{}", self.search.to_value);
        Task::none()
    }

    /// Handle `Message::SearchFilepath`.
    ///
    /// Toggles whether the file path is searched.
    pub fn search_filepath(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.filepath = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchTitle`.
    ///
    /// Toggles whether the title is searched.
    pub fn search_title(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.title = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchTag`.
    ///
    /// Toggles whether tags are searched.
    pub fn search_tag(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.tags = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchDescription`.
    ///
    /// Toggles whether the description is searched and, if not already
    /// enabled, includes video media.
    pub fn search_description(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.description = is_checked;
        if !self.search.video {
            self.search.video = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchActor`.
    ///
    /// Toggles whether the actor is searched and, if not already enabled,
    /// includes video media.
    pub fn search_actor(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.actor = is_checked;
        if !self.search.video {
            self.search.video = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchDirector`.
    ///
    /// Toggles whether the director is searched and, if not already
    /// enabled, includes video media.
    pub fn search_director(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.director = is_checked;
        if !self.search.video {
            self.search.video = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchArtist`.
    ///
    /// Toggles whether the artist is searched and, if not already enabled,
    /// includes audio media.
    pub fn search_artist(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.artist = is_checked;
        if !self.search.audio {
            self.search.audio = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchAlbumartist`.
    ///
    /// Toggles whether the album artist is searched and, if not already
    /// enabled, includes audio media.
    pub fn search_albumartist(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.album_artist = is_checked;
        if !self.search.audio {
            self.search.audio = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchAlbum`.
    ///
    /// Toggles whether the album is searched and, if not already enabled,
    /// includes audio media.
    pub fn search_album(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.album = is_checked;
        if !self.search.audio {
            self.search.audio = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchComposer`.
    ///
    /// Toggles whether the composer is searched and, if not already
    /// enabled, includes audio media.
    pub fn search_composer(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.composer = is_checked;
        if !self.search.audio {
            self.search.audio = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchGenre`.
    ///
    /// Toggles whether the genre is searched and, if not already enabled,
    /// includes audio media.
    pub fn search_genre(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.genre = is_checked;
        if !self.search.audio {
            self.search.audio = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchDuration`.
    ///
    /// Toggles whether the duration is searched.
    pub fn search_duration(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.duration = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchCreationDate`.
    ///
    /// Toggles whether the creation date is searched.
    pub fn search_creation_date(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.creation_date = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchModificationDate`.
    ///
    /// Toggles whether the modification date is searched.
    pub fn search_modification_date(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.modification_date = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchReleaseDate`.
    ///
    /// Toggles whether the release date is searched.
    pub fn search_release_date(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.release_date = is_checked;
        Task::none()
    }

    /// Handle `Message::SearchLenseModel`.
    ///
    /// Toggles whether the lens model is searched and, if not already
    /// enabled, includes image media.
    pub fn search_lense_model(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.lense_model = is_checked;
        if !self.search.image {
            self.search.image = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchFocalLength`.
    ///
    /// Toggles whether the focal length is searched and, if not already
    /// enabled, includes image media.
    pub fn search_focal_length(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.focal_length = is_checked;
        if !self.search.image {
            self.search.image = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchExposureTime`.
    ///
    /// Toggles whether the exposure time is searched and, if not already
    /// enabled, includes image media.
    pub fn search_exposure_time(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.exposure_time = is_checked;
        if !self.search.image {
            self.search.image = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchFNumber`.
    ///
    /// Toggles whether the F-number (aperture) is searched and, if not
    /// already enabled, includes image media.
    pub fn search_f_number(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.fnumber = is_checked;
        if !self.search.image {
            self.search.image = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchGpsLatitude`.
    ///
    /// Toggles whether the GPS latitude is searched and, if not already
    /// enabled, includes image media.
    pub fn search_gps_latitude(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.gps_latitude = is_checked;
        if !self.search.image {
            self.search.image = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchGpsLongitude`.
    ///
    /// Toggles whether the GPS longitude is searched and, if not already
    /// enabled, includes image media.
    pub fn search_gps_longitude(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.gps_longitude = is_checked;
        if !self.search.image {
            self.search.image = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchGpsAltitude`.
    ///
    /// Toggles whether the GPS altitude is searched and, if not already
    /// enabled, includes image media.
    pub fn search_gps_altitude(&mut self, is_checked: bool) -> Task<Message> {
        self.search.search_id = 0;
        self.search.gps_altitude = is_checked;
        if !self.search.image {
            self.search.image = true;
        }
        Task::none()
    }

    /// Handle `Message::SearchCommit`.
    ///
    /// Reuses the ID of an identical existing search (to keep history
    /// stable), stores the search if it is new, then opens a tab at the
    /// resulting `Location::DBSearch` and fills it with the scan results.
    pub fn search_commit(&mut self) -> Task<Message> {
        let mut s = self.search.clone();
        for s2 in self.search_previous.iter() {
            if &s == s2 {
                s.search_id = s2.search_id;
            }
        }
        if s.search_id == 0 {
            s.store(self.sql_connection.clone());
            self.search_previous.push(s.clone());
        }
        self.search = s.clone();
        let location = Location::DBSearch(s);
        let (parent_item_opt, items) =
            location.scan(self.sql_connection.clone(), IconSizes::default());
        let (entity, command) = self.open_tab_entity(location, true, None);
        if let Some(tab) = self.tab_model.data_mut::<Tab>(entity) {
            tab.parent_item_opt = parent_item_opt;
            tab.set_items(items);
        }
        command
    }
}
