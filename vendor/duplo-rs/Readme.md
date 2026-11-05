# Duplo-rs - Detect Similar or Duplicate Images and Videos

This Rust library allows you to perform a visual query on a set of images or videos, returning the results in the order of similarity. This allows you to effectively detect duplicates with minor modifications (e.g. some colour correction or watermarks) or text overlays. For videos - depending on the settings - it should be possible to even find clips in a compilation video.

It is an implementation of [Fast Multiresolution Image Querying](http://grail.cs.washington.edu/projects/query/mrquery.pdf) by Jacobs et al. which uses truncated Haar wavelet transforms to create visual hashes of the images. The same method has previously been used in the [imgSeek](http://www.imgseek.net) software and the [retrievr](http://labs.systemone.at/retrievr) website.

This project started as s re-implementation of [rivo/duplo](https://github.com/rivo/duplo) for the similar image technology that works for millions of images if it is allowed to run for weeks.

[duplo-rs](http://github.com/fangornsrealm/duplo-rs) adds the ability to use the image search on videos and video segments. The video algorithm takes screenshots every N seconds and searches similar images for each. A video sequence is considered similar if at least S screenshots in a row match.

The user can fine-tune the sensitivity of the similarity of the screenshots search algorithm with a value between 0 and 100.

- 100 means that only resized images will be seen as similar.
- 0 means that quite heavy modifications are allowed. Also it means that the center piece of the image can change a lot. For example a model shoot series will detect similar images with different poses of the model. The same setup with a different model might also match.

The user can also play with the seconds between the screenshots. The example application takes six screenshots per minute of runtime. That is very precise but **quite** slow! One snapshot per minute might be more appropriate for longer videos. One every five minutes is even faster. But the chance to hit a similar screenshot get much smaller unless you are scanning exactly the same video.

The user can and should also play with the number of frames in a row that have to match. Start with one frame to see how good your image matching parameters work. If you have too many false positives increase the sequence length.

## Examples

There are example programs showing how this library can be used.

### demo_similar_images

```sh
cargo build --release --example demo_similar_images
./target/release/examples/demo_similar_images [options] <path to search>
```

This application reads every image in the directory or directory tree and compares it to the already read images. It compares the recent image with the best match and the better image is kept in the in-memory database. The pair of images is presented in a directory `simiilar_images`, the better one as a hard-link (will stay) the worse one will be moved (will go). The user can review the matches with any app that can display previews and sort files by name. If the match is good, just delete the files. If it isn't, copy the files that are marked `_REMOVE_` somewhere else if you want to keep them.

The storage ist persistent. So you can run the app again, using the same or a different start path. This app is designed to do comparisons that other programs just refuse to do. Most competitors will fall flat on their face with 100 000 images. This one has successfully searched 6 Million images and found 1 Million best matches. A job of this size will take multiple weeks. So a restart capability is nice to have.

### demo_similar_videos

```sh
cargo build --release --example demo_similar_videos
./target/release/examples/demo_similar_videos [options] <path to search>
```

This application reads every video in the directory or directory tree and compares it to the already read videos. The recent video is stored in the database so it doesn't have to be parsed again. When running the program again, the already read files are checked and the deleted files are removed from the database. For each new video it compares the screenshots with previously read videos. If it finds matches it exports a HTML file with previews and metadata into the directory `similar_videos` with the name of the new video. As there is no clear criterum no files are deleted. The users have to review the matches and decide for themselves.

The storage is persistent. So you can run the app again, using the same or a different start path. Theoretically you could change the settings as well. And for the sensitivity and sequence length you can as they only influence the comparison result. But the decision on the time between the screenshots has to be fixed so the comparison partners exist.

## Usage

Include this in your Cargo.toml.

```toml
duplo-rs = { path = "../duplo-rs", version = "1.0.0" }
```

Then call the library like this to search for images:

```rust
use duplo_rs;

// Create an empty store.
let mut store = duplo_rs::store::Store::new(sensitivity);

// Add image "img" to the store.
let hash = duplo_rs::files::process_image(file);
store.add("myimage", hash)

// Query the store based on image "query".
let (matches, failedid, failedhash) =
    duplo_rs::files::find_similar_images(&store, &filepath, &hash);
// matches[0] is the best match.
```

Or like this to search for videos:

```rust
use duplo_rs;

// Create an empty store.
let homedir_opt = dirs::home_dir();
if homedir_opt.is_none() {
    log::error!("Could not determine the home directory!");
    return;
}
let dbpath = homedir_opt.unwrap().join("similar_videos.sqlite3");
let dbpathstr = duplo_rs::files::osstring_to_string(dbpath.as_os_str());
let sql_client_opt = duplo_rs::videostore::connect(&dbpathstr);
if sql_client_opt.is_ok() {
    let mut sql_client = sql_client_opt.unwrap();
    let mut store = duplo_rs::videostore::VideoStore::new(
        &mut sql_client,
        sensitivity,
        &directory,
        num_threads,
        num_seconds_between_screenshots,
        min_similar_screenshots_in_sequence,
        max_candidates_in_cache,
    );
}
// parse the screenshots for movie and add data in "video" to the store.
let video = duplo_rs::files::process_video(file, video_id as usize, num_videos, num_seconds_between_screenshots);
store.add(&mut sql_client, &video.id, &video, video.runtime);

// Query the store based on movie "video".
let (matches, failedid, _failedhash) =
    duplo_rs::files::find_similar_videos(&store, &mut sql_client, &video.id, &video);
// matches[0] is the best match.
```

The modified search does the same but is much quicker, but for defective video files it just fails. In cases that don't deliver a valid result the old, slow method is used.

A trade-off between the running time and the quality of similar video detection is advisable depending on what needs to be detected.

The demo app is designed on the precision side of the evaluation. It takes a screenshot every 10 seconds and considers a minimum length of 1 minute as a similar video. Outside of Music or TikTok videos scenes are longer than 10 seconds. So the probability to find a similar frame in a modified video is good.

A screenshot every five minutes would be much faster and use 30x less resources, but exclusively find the same file that has only been edited after the minimum sequence number of screenshots.

## Documentation

[http://github.com/fangornsrealm/duplo-rs](http://github.com/fangornsrealm/duplo-rs)

## Possible Applications

- Identify copyright violations
- Save disk space by detecting and removing duplicate images or videos
- Search for images or videos by similarity

## Optimizations

The performance of video comparison was not good. And requires way too much resources.

As we don't have the time to test and optimize this by hand we asked an LLM (Quen3.8 27B) to take a look. And it delivered.
