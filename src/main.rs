// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

#[cfg(feature = "jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // libmpv (and glibc) parse/format numbers with the C library locale.
    // Under a non-C locale (e.g. de_DE, comma decimals) mpv mangles
    // dotted-decimal command arguments and emits locale-formatted numbers
    // in generated JSON (e.g. `track-list`), which breaks playback
    // control and track parsing. Pin `LC_ALL` to C before anything
    // (glib, mpv) is initialized — glib's own `setlocale(LC_ALL, "")`
    // call inherits this value. The app's UI text goes through ICU and
    // chrono (Rust-native), so user-visible localization is unaffected.
    // SAFETY: process start, before any threads read the environment.
    unsafe { std::env::set_var("LC_ALL", "C") };

    // Prevents glibc from hoarding memory via memory fragmentation.
    #[cfg(all(not(feature = "jemalloc"), target_env = "gnu"))]
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 65536);
    }

    media_browser::main()
}
