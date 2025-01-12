# media-browser

Media browser with database backend for the COSMIC desktop environment

The GUI of the file manager part is a clone of COSMIC files with a few modifications.

The GUI of the video / audio player is a clone of COSMIC player. The player itself is a modification of iced-video-player.

The Image viewer GUI is inspired by gthumb.

The backend is a genuine creation. And making all the above work together was quite a lot of work.

This project is developed and tested on Linux using Wayland and Pipewire.

> [!NOTE]
> This project is still a work in progress.
> Currently it is considered Alpha Code.
> New features are still added.
> But most of the planned features are basically working.

## Required dependencies

Video and audio playback requires GStreamer.

Gstreamer is modular. Depending on what formats you want to playback (mp3, m4a, aiff, flac, mp4, mkv, wmv, av1, vp9, h264, hevc, ...) and what backend you want to use (pipewire, pulseaudio, alsa, jack, ...) specific gst-plugin-`format` have to be installed.

[Installing Gstreamer on Linux](https://gstreamer.freedesktop.org/documentation/installing/on-linux.html?gi-language=c)

## Build the project from source

```sh
# Clone the project using `git`
git clone https://github.com/fangornsrealm/media-browser
# Change to the directory that was created by `git`
cd media-browser
# Build an optimized version using `cargo`, this may take a while
cargo build --release
# Run the optimized version using `cargo`
cargo run --release
```

## License

This project is licensed under [GPLv3](LICENSE)
Parts coming from external projects are specially marked
Usually they are licensed [MIT](http://opensource.org/licenses/MIT)
