use clap::{command, Arg, ArgAction};
use log::LevelFilter;
use pbr::ProgressBar;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use std::fs::File;

use std::sync::mpsc;

/// Searches for similar videos inside a directory or optionally also all it's subdirectories
///
/// *Similar* in this definition means that at least a minute should show similar video content
/// ffmpeg is used to generate a screenshot every ten seconds.
/// The similar image routines of duplo-rs are used to find similar screenshots.
/// If a video has six or more similar screenshots in a row they are considered similar.
///
/// Dependencies:
/// ffmpeg needs to be installed and in the path for executables. No development packages required.
///
/// The derived data from all the screenshots is too large to keep all the data in RAM.
/// Depending on the length of the videos even 96 GB of RAM will be filled with around 1000 videos.
/// The data blobs and the indices are stored in a SQLite database.
/// 1 TB of storage will suffice for 10 000 to 15 000 videos, depending on the length.
///
/// The search for similar videos in the database has to be done one by one.
/// The speed is severely limited. Each new video will be compared screenshot by screenshot.
/// A rough estimate is one minute for five minutes of video. There is a cache of the previously compared videos.
/// Reading the data from the database costs time. The more memory you can spare for the Cache, the faster the comparison.
/// But be careful. As long as the program is running the Memory will be blocked by the Cache.
/// If you run this program on a computer you intend to do things on, don't use more than a third of your RAM for cache!
/// The data for a video is around 100 MB of data. A Cache of 100 Videos will take 10 GB of RAM that you cannot use any more.
///
/// The search algorithm and the num_seconds_between_screenshots and min_similar_screenshots_in_sequence are the main parameters to fine-tune
/// what you want to do with the search. The user of the library has to decide the trade-off between the running time and the quality of similar video detection.
/// This demo app rests solely on the precision side of the evaluation. To show what this algorithm can do.
/// Outside of Music or TikTok videos scenes are longer than 10 seconds. So the probability to find a similar frame in a modified video is good.
/// With six matches in a row you can find compilations down to one minute of runtime per video.
/// A screenshot every five minutes would be much faster and use 30x less resources, but only find the same file that has been cut in the end.
/// The chance to find a similar scene with a distance of two and a half minutes in average is next to zero unless you screen security footage of an empty hallway.
///
/// You can stop a scan at any time. Any video that has already been parsed will not be parsed again.
/// It's data will be deleted if the file is no longer available.
///
/// It can happen that it finds similarities that you will not see as such.
/// If you want to decrease the sensitivity you can specify a value between 0 and 100 which will decrease the number of found similar images.
///
/// To enable you to check it creates a directory "similar_videos" that contains a HTML file per video for which similar videos were found.
/// Each HTML file contains a preview of the new file on top and any previous read videos who have at least a minute of similar video.
/// The file path and video metadata are given for each file. The HTML file works for Chromium based browsers. MP4 files also work in Firefox.
///
// ── §9: Ctrl-C / graceful shutdown ──────────────────────────────────────
// A SIGINT handler sets CTRL_C_SEEN; a background thread polls it and
// calls VideoStore::request_stop() so every worker winds down promptly.
// No new crate dependency — uses the `signal()` syscall directly.
static CTRL_C_SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

#[cfg(unix)]
extern "C" fn _on_sigint(_: i32) {
    CTRL_C_SEEN.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Install a SIGINT handler and a forwarder thread that polls
/// CTRL_C_SEEN every 200 ms and calls `store.request_stop()`.
fn install_ctrl_c_handler(store: &std::sync::Arc<duplo_rs::videostore::VideoStore>) {
    #[cfg(unix)]
    unsafe {
        let _ = signal(2, _on_sigint as *const () as usize); // 2 = SIGINT
    }
    let store2 = store.clone();
    std::thread::spawn(move || loop {
        if CTRL_C_SEEN.load(std::sync::atomic::Ordering::SeqCst) {
            eprintln!("\n[Ctrl-C] Requesting stop…");
            store2.request_stop();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    });
}
// ─────────────────────────────────────────────────────────────────────────

pub fn main() {
    let mut logfile = "demo_similar_videos.txt".to_string();
    let mut recursive = false;
    let mut sensitivity: f64 = -20.0;

    let curdir = std::env::current_dir().unwrap().as_os_str().to_owned();
    let mut directory = duplo_rs::files::osstring_to_string(&curdir);
    let mut num_threads = 1;
    let mut num_seconds_between_screenshots = 10u32;
    let mut min_similar_screenshots_in_sequence = 6u32;
    let mut max_candidates_in_cache = 100;
    let matches = command!() // requires `cargo` feature
        .arg(Arg::new("logfile").short('l').long("log"))
        .arg(Arg::new("directory").short('d').long("directory"))
        .arg(Arg::new("sensitivity").short('s').long("sensitivity"))
        .arg(Arg::new("num_threads").short('t').long("num_threads"))
        .arg(
            Arg::new("num_seconds_between_screenshots")
                .short('b')
                .long("num_seconds_between_screenshots"),
        )
        .arg(
            Arg::new("min_similar_screenshots_in_sequence")
                .short('m')
                .long("min_similar_screenshots_in_sequence"),
        )
        .arg(
            Arg::new("max_candidates_in_cache")
                .short('c')
                .long("max_candidates_in_cache"),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    if let Some(ret) = matches.get_one::<String>("logfile") {
        logfile = ret.clone();
    }
    if let Some(ret) = matches.get_one::<String>("directory") {
        directory = ret.clone();
    }
    if let Some(ret) = matches.get_one::<String>("sensitivity") {
        let ret = i64::from_str_radix(ret, 10);
        if ret.is_ok() {
            sensitivity -= ret.unwrap() as f64;
        }
    }
    if let Some(ret) = matches.get_one::<String>("num_seconds_between_screenshots") {
        let ret = i64::from_str_radix(ret, 10);
        if ret.is_ok() {
            num_seconds_between_screenshots = ret.unwrap() as u32;
        }
    }
    if let Some(ret) = matches.get_one::<String>("min_similar_screenshots_in_sequence") {
        let ret = i64::from_str_radix(ret, 10);
        if ret.is_ok() {
            min_similar_screenshots_in_sequence = ret.unwrap() as u32;
        }
    }
    if let Some(ret) = matches.get_one::<String>("max_candidates_in_cache") {
        let ret = i64::from_str_radix(ret, 10);
        if ret.is_ok() {
            max_candidates_in_cache = ret.unwrap() as usize;
        }
    }
    if let Some(ret) = matches.get_one::<String>("num_threads") {
        let ret = i64::from_str_radix(ret, 10);
        if ret.is_ok() {
            num_threads = ret.unwrap() as u32;
        }
    }
    println!("Sensitivity {}\nStartdirectory {}\nnum_threads {}\nnum sec betw. screenshots {}\nmin similar screenshots in sequence {}\nnum candidates in cache {}",
        sensitivity,
        directory,
        num_threads,
        num_seconds_between_screenshots,
        min_similar_screenshots_in_sequence,
        max_candidates_in_cache
    );
    if let Some(ret) = matches.get_one::<bool>("recursive") {
        recursive = *ret;
    }

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Error,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            File::create(&logfile).unwrap(),
        ),
    ])
    .unwrap();
    let dir = directory.clone();
    let p = std::path::Path::new(&dir);
    if !p.is_dir() {
        log::error!(
            "Directory {} does not exist. Specify an existing directory to search!",
            directory
        );
        std::process::exit(1);
    }
    // create the directory where the user can compare the similar image pairs
    let dst: std::path::PathBuf = p.join("similar_videos");
    // get the list of files to process
    let filelist;
    if recursive {
        // Consider all subdirectories --> will take a long time!
        filelist = duplo_rs::files::walk_tree_videos(&directory);
    } else {
        filelist = duplo_rs::files::walk_dir_videos(&directory);
    }
    let mut progressbar = ProgressBar::new(filelist.len() as u64);
    let homedir_opt = dirs::home_dir();
    if homedir_opt.is_none() {
        log::error!("Could not determine the home directory!");
        return;
    }
    let dbpath = homedir_opt.unwrap().join("similar_videos.sqlite3");
    let dbpathstr = duplo_rs::files::osstring_to_string(dbpath.as_os_str());
    let sql_client_opt = duplo_rs::videostore::connect(&dbpathstr);
    if sql_client_opt.is_ok() {
        let sql_client = sql_client_opt.unwrap();
        let store = duplo_rs::videostore::VideoStore::new(
            sql_client,
            sensitivity,
            &directory,
            num_threads,
            num_seconds_between_screenshots,
            min_similar_screenshots_in_sequence,
            max_candidates_in_cache,
        );
        let store = std::sync::Arc::new(store);

        // §9: Wire up Ctrl-C so the user can stop a long scan at any time.
        install_ctrl_c_handler(&store);

        let num_videos = filelist.len() as u32;
        let starting_candidate = store.num_candidates();
        let num_query_workers = (num_threads.max(1)) as usize;

        // Shared receiver (Mutex) so multiple query workers can pull work.
        let (parse_tx, parse_rx) = mpsc::sync_channel::<(
            u32,
            duplo_rs::videocandidate::VideoCandidate,
        )>(num_query_workers * 2);
        let parse_rx = std::sync::Arc::new(std::sync::Mutex::new(parse_rx));
        // Results from query workers to the single writer.
        let (result_tx, result_rx) = mpsc::channel::<(
            duplo_rs::videocandidate::VideoCandidate,
            duplo_rs::videomatches::VideoMatches,
        )>();
        let result_rx = std::sync::Arc::new(std::sync::Mutex::new(result_rx));

        let filelist = std::sync::Arc::new(filelist);

        // ------------------------------------------------------------------
        // Producer: walks the file list, spawns ffmpeg workers (bounded by
        // num_threads), and feeds parsed videos into the shared parse_rx.
        //
        // §9: Bounded concurrency via an AtomicUsize counter (replaces the
        // old FIFO-reap which deadlocked when the first-spawned worker was
        // the slowest to finish). Workers use try_send so they cannot block
        // indefinitely on a full channel (e.g. after Ctrl-C when query
        // workers have exited).
        // ------------------------------------------------------------------
        {
            let parse_tx = parse_tx.clone();
            let filelist = filelist.clone();
            let store = store.clone();
            let mut video_id_counter = starting_candidate + 1;
            let in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize> =
                std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let max_in_flight = num_threads.max(1) as usize;
            let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
            let mut filepos = 0usize;

            loop {
                // §9: stop promptly if the user pressed Ctrl-C.
                if store.stop_requested() {
                    log::warn!("Stop requested — producer winding down.");
                    break;
                }
                if filepos >= filelist.len() {
                    break;
                }

                // Wait for a free slot before spawning.
                while in_flight.load(std::sync::atomic::Ordering::SeqCst) >= max_in_flight {
                    if store.stop_requested() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                let filepath = filelist[filepos].clone();
                let filestring = duplo_rs::files::osstring_to_string(&filepath.as_os_str());
                let next_pos = filepos + 1;
                filepos = next_pos;

                if store.ids_lookup(&filestring).is_some() {
                    progressbar.inc();
                    continue;
                }

                let video_id = video_id_counter;
                video_id_counter += 1;
                let num_seconds = store.num_seconds_between_screenshots();
                let tx = parse_tx.clone();
                let inflight = in_flight.clone();
                let handle = std::thread::spawn(move || {
                    let video = duplo_rs::files::process_video(
                        &filepath,
                        video_id as usize,
                        num_videos,
                        num_seconds,
                        next_pos as u32,
                    );
                    // §9: try_send so the worker can never block indefinitely
                    // on a full channel (e.g. after Ctrl-C).
                    if tx.try_send((video_id, video)).is_err() {
                        log::warn!(
                            "try_send failed for video {} (channel full or closed)",
                            video_id
                        );
                    }
                    inflight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                });
                in_flight.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                handles.push(handle);
            }

            // §9: Join all workers in any order (no FIFO constraint).
            for h in handles {
                let _ = h.join();
            }
        }
        // Drop the producer's sender so workers see EOF.
        drop(parse_tx);

        // ------------------------------------------------------------------
        // Query workers: pull from the shared parse_rx, run store.query
        // (parallel reads via the store's read lock), push results.
        // ------------------------------------------------------------------
        {
            let mut worker_handles = Vec::new();
            for _w in 0..num_query_workers {
                let parse_rx = parse_rx.clone();
                let result_tx = result_tx.clone();
                let store = store.clone();
                let h = std::thread::spawn(move || {
                    loop {
                        // §9: stop promptly if the user pressed Ctrl-C.
                        if store.stop_requested() {
                            break;
                        }
                        // Grab one item from the shared receiver, if any.
                        let item = {
                            let guard = parse_rx.lock().unwrap();
                            guard.try_recv()
                        };
                        match item {
                            Ok((_video_id, video)) => {
                                let matches = store.query(&video);
                                if result_tx.send((video, matches)).is_err() {
                                    break; // writer gone
                                }
                            }
                            Err(mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                            Err(mpsc::TryRecvError::Disconnected) => break,
                        }
                    }
                });
                worker_handles.push(h);
            }
            for h in worker_handles {
                let _ = h.join();
            }
        }
        drop(result_tx);

        // ------------------------------------------------------------------
        // Writer (main thread): the single serial committer.
        // ------------------------------------------------------------------
        let result_rx_guard = result_rx.lock().unwrap();
        loop {
            // §9: stop promptly if the user pressed Ctrl-C.
            if store.stop_requested() {
                log::warn!("Stop requested — writer winding down.");
                break;
            }
            let (video, videomatches) = match result_rx_guard.recv() {
                Ok(v) => v,
                Err(_) => break,
            };
            log::warn!(
                "Found {} matches for video {}.",
                videomatches.len(),
                video.id
            );
            progressbar.inc();
            let mut compare: Vec<duplo_rs::videocandidate::VideoCandidate> = Vec::new();
            compare.push(video.clone());
            for i in 0..videomatches.m.len() {
                log::warn!("Match {} is similar to {}.", videomatches.m[i].id, video.id);
                if let Some(index) = store.ids_lookup(&videomatches.m[i].id) {
                    let (_, candidate) = store.return_candidate(index as u32);
                    if !candidate.id.is_empty() {
                        compare.push(candidate);
                    }
                }
            }
            // §16.4: skip renamed duplicates — if this content is already in
            // the store under a different name, don't add it again.
            if let Some(existing_id) = store.content_fingerprint_lookup(&video) {
                if existing_id != video.id {
                    log::warn!(
                        "Skipping {} — content already stored as {}",
                        video.id,
                        existing_id
                    );
                    continue;
                }
            }
            store.add(&video.id, &video, video.runtime);
            duplo_rs::files::present_video_matches(&dst, &compare);
        }
    }
    progressbar.finish();
}
