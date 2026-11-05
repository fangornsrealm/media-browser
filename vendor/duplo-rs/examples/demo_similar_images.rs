use clap::{command, Arg, ArgAction};
//use ctrlc;
use log::LevelFilter;
use pbr::ProgressBar;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use std::fs::File;

/// Searches for similar images inside a directory or optionally also all it's subdirectories
///
/// It can happen that it finds similarities that you will not see as such.
/// If you want to decrease the sensitivity you can specify a value between 0 and 100 which will decrease the number of found similar images.
///
/// To enable you to check it creates a directory "duplicates" that contains pairs of images.
/// The file with the "KEEP" in the name will stay, as it is a hardlink (or a copy if your filesystem does not support hardlinks)
/// The file with the "REMOVE" in the name will be gone if you delete it as it was moved there.
/// They have the same random prefix so they are sorted together.
/// The "better" of the two images was picked. The criteria for "better" could be more elaborate.
/// Use any Image browser / file manager to review at your leisure.
///
/// If you are satisfied with the selection, just delete all of the images.
/// If images were picked that you would like to keep, you have to move at least the files with "REMOVE" in the name somewhere else.
///
pub fn main() {
    //ctrlc::set_handler(move || {
    //    crate::store::CTRL_C_PRESSED = true;
    //})
    //.expect("Error setting Ctrl-C handler");

    let mut logfile = "demo_similar_images.txt".to_string();
    let mut recursive = false;
    let mut sensitivity: f64 = -60.0;
    let curdir = std::env::current_dir().unwrap().as_os_str().to_owned();
    let mut directory = duplo_rs::files::osstring_to_string(&curdir);
    let matches = command!() // requires `cargo` feature
        .arg(Arg::new("logfile").short('l').long("log"))
        .arg(Arg::new("directory").short('d').long("directory"))
        .arg(Arg::new("sensitivity").short('s').long("sensitivity"))
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
    if let Some(ret) = matches.get_one::<bool>("recursive") {
        recursive = *ret;
    }

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Warn,
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
    let dst: std::path::PathBuf = p.join("similar_images");
    let storepath = p.join("demo_example_images.store");
    let mut store: duplo_rs::store::Store = duplo_rs::store::Store::new(sensitivity);
    if storepath.is_file() {
        let storefile = duplo_rs::files::osstring_to_string(storepath.as_os_str());
        store.slurp_binary(&storefile);
    }

    // get the list of files to process
    let filelist;
    if recursive {
        // Consider all subdirectories --> will take a long time!
        filelist = duplo_rs::files::walk_tree_images(&directory);
    } else {
        filelist = duplo_rs::files::walk_dir_images(&directory);
    }
    let mut progressbar = ProgressBar::new(filelist.len() as u64);

    // process the files
    for file in filelist.iter() {
        let hash = duplo_rs::files::process_image(file);
        let filepath = duplo_rs::files::osstring_to_string(file.as_os_str());
        let (matches, failedid, failedhash) =
            duplo_rs::files::find_similar_images(&store, &filepath, &hash);
        progressbar.inc();
        for i in 0..matches.m.len() {
            log::warn!("Match {} is similar to {}.", matches.m[i].id, filepath);
            let retmatch = imagesize::size(matches.m[i].id.clone());
            let retsource = imagesize::size(filepath.clone());
            if retmatch.is_err() {
                log::error!("Failed to read the size of the image {}!", matches.m[i].id);
                continue;
            }
            if retsource.is_err() {
                log::error!("Failed to read the size of the image {}!", filepath);
                continue;
            }
            let matchsize = retmatch.unwrap();
            let sourcesize = retsource.unwrap();
            if matchsize.width * matchsize.height > sourcesize.width * sourcesize.height {
                // match is the *better* image, drop the new hash
                duplo_rs::files::present_pairs(&dst, &filepath, &matches.m[i].id);
            } else {
                // source is the *better* image, remove match from store, add the source and drop the rest of the matches
                duplo_rs::files::present_pairs(&dst, &matches.m[i].id, &filepath);
                store.delete(&matches.m[i].id);
                store.add(&filepath, &hash);
                break;
            }
        }
        // add the current file to the store
        if matches.m.len() == 0 {
            store.add(&failedid, &failedhash);
        }
    }
    if storepath.is_file() {
        let ret = std::fs::remove_file(storepath);
        if ret.is_err() {
            log::error!("Failed to delete store file");
        }
    }
    store.dump_binary("demo_example_images.store");
}
