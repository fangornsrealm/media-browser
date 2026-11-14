# Todo list

## Origins

This project is based on the Alpha 2 of COSMIC Files. Most of the file management features have been removed. A media browser does not need to have multiple tabs for multiple directories. The tab feature is handy for showing search results, though.

The Video Player and the Design of the popover are from COSMIC Player of Alpha 4 and the Iced-Video-Player.

The audio player is also derived from it but heavily modified.

The Image viewer is currently the image_viewer widget from the iced project. There is currently no time to learn how to do it more like I would like it to work.

## Content

This is the history of the project as well as the bug-tracker and the list of implemented and planned features.

Open Features are goals that are planned for a 1.0 release (required features) or something that would be neat over the life-time of the project.

Positions ended with a ? are *stretch goals*. If it is possible AND somebody does the work, it would be good to have these. In 1.0 or later. In some cases the way to make them possible is a rewrite of one of the standard tools used. Which makes the effort explode way beyond the available spare time.

## File browser

### Bugs

#### Fixed Bugs

- fix the About and settings panel
- migrate to Alpha4 of libcosmic
- fix bug that single Movie directories are not parsed correctly
- fix Previous/Next navigation by PageUp/PageDown
- make Esc stop playback of audio and video and kill the gstreamer pipeline
- skip EXIF extraction for unsupported image formats
- Audio files do not open properly from database
- Image files do not open properly from database
- cosmic::iced::wgpu crashes when loading images too large on Intel ARC A770 with 16 GB VRAM if preview is open even with 2000xYYYY images when preview is open
- browsing images fills memory in seconds
- search results don't preview actors and chapters
- PageUp and PageDown do no longer navigate the Presious / Next
- Parse Chapter names from media files
- Chapters not diplayed in audio player
- Image search delivers videos
- Navigation in search results navigates the directory view instead
- New searches are not stored in the database
- unbounded searches will kill the machine. Make sanity checks!
- fix release date 1970-01-01
- recursive_scan needs to check if a file is already in the database
- scroll-wheel sometimes starts seeking at the position it last operated, not the current playback position
- Search Results have no context menu
- Search results do not change sort order
- Check that updates to the database actually work
- deactivate creation time and access time for files
- Parsing directory structures of TV Series does not ignore unnecessary metadata
- for single Movie directories thumbnails are generated for unused metadata images instead of just the poster
- When parsing TV Show metadata PNG crashes while reading PNG file it should not try to read.
- TV Shows show no episodes
- Collection episode view back button works
- Collection episode view breadcrumbs do not work and path is not editable.
- similar video/image scan found nothing (0 entries) when the media is nested under subfolders (e.g. `Show/Season 1/…`): the `recursive` config defaulted to `false`, so `list_media_files` only walked the top-level folder. Default `recursive` to `true`.

#### Open Bugs

- Automatically close Popup GUI after 5 Seconds of no activity?
- key bindings only work in browser mode

### Features

#### Done Features

- Read metadata from video files, Kodi-style NFO files, poster, subtitles
- Read metadata from audio files including cover-art
- Read metadata from image files
- store metadata in database
- create thumbnails from video files without poster images
- when entering a directory the contents are scanned automatically if they are not yet in the database
- display all media files in the tab view and hide the metadata files
- Display media on enter or double click and navigate to Previous and Next file on Button or Page-up and Page-Down, Button or Escape to leave viewer
- resursive scan of the selected directory by menu or right-click menu
- Make recursive scan run in the background
  - Only already parsed images / directories can be used normally!
  - Write access to the database is blocked during runtime!
- Search for filetype, actor, director, artist, albumartist, release date range, duration, chapters, ... in the database
- Saved search management in the database
- multiple file rename feature, using the sort order of the model
- display detail information of entry (Free-Form Text field with all the available information)
- Search panel with a separate result model/view
  - (new tab per search result, derived from tab so ESC returns to the search and the search handles the previous/next)
- disable the NavBar by default
- get the bread-crumb navigation in the tab back
- manage previous searches in the search context menu
- make the details page information clickable - on click it will search for the value (for example: "Director: Quentin Tarantino)
- make the search text appear in the search tab title
- user defined tags
- search for user defined tags
- display user defined tags in detail view (search by clicking)
- add user defined tags to the navigation panel like locations and add them to the database
- add context menu to remove user defined tags from the navigation panel and remove them and their entries from the database
- on click on a tag open a search with all elements that have a user defined tag assigned to them
- assign user defined tags by dragging files onto a tag in the navigation panel
- display size of metadata in settings panel
- display location of metadata in settings panel
- settings option to delete metadata
- sort audio from one album by track id

#### Open Features

- optionally skip different media types in navigation?
- sort by release date / creation / modification time
  - adjust the sort options when just one type is displayed and more details are available?
- view files of only one type?
- find similar images in background? (duplo-rs runs very long, better started on the command line!)
  - fill a new tab with the similar image pairs for comparison. (not necessary as they are in a new directory anyway)
- find similar videos in background? (duplo-rs runs very long, better started on the command line!)
  - fill a new tab with the similar video pairs for comparison? (duplo-rs can be started again and will just continue. COSMIC media browser would have to run for months!)

## Image viewer

### Image Features

#### Image Done Features

- use the iced image_viewer widget as base to display images
- On click show a navigation bar
- add a popup selection strip of the files/images in the same directory
- store image metadata in the database
- images larger than 9000 pixels in any direction are scaled down to thumbnail directory and used from there.
- adjust the size and shape away from the original aspect ratio of the image viewer on zoom-in if there is room to grow in the window?

#### Image Open Features

- make the zoom buttons work? (might require another viewer)
- make the popup strip disappear once the mouse is no longer hovering over it?
  - mouse_area is whole image_viewer, not just the pop_over

## Video viewer

### Video Features

#### Video Done Features

- use the iced_video_player / GStreamer to play videos
- support Matroska container files.
- use a consistent minimal Design - Thanks COSMIC Player!
- add seek on mouse scroll
- add seek buttons
- add tooltips for the buttons
- add a browse button to have the same navigation strip as in image view
- stop playback when opening new file
- stop playback when switching to Browser view
- add chapter navigation
- make the player zoom a video that is smaller than the display area
- make the primary metadata directly searchable from the preview

#### Video Open Features

- add playback speed control?

## Audio Player

### Audio Features

#### Audio Done Features

- modify a copy of the iced_video_player / GStreamer to play audio only files
- use a consistent minimal design - Thanks COSMIC Player!
- display coverart instead of a video if available in the embedded metadata
- add seek on mouse scroll
- add seek buttons
- add tooltips for the buttons
- add a browse button to have the same navigation strip as in image view
- update the playback positon in the number and slider
- stop playback when opening new file
- stop playback when switching to Browser view
- add chapter navigation
- parse more audio metadata and display them in preview
- make the primary metadata directly searchable form the preview

#### Audio Open Features

- add playback speed control?
- add playback of lyrics files (.lrc) as subtitles
