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
    // Prevents glibc from hoarding memory via memory fragmentation.
    #[cfg(all(not(feature = "jemalloc"), target_env = "gnu"))]
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 65536);
    }

    media_browser::main()
}
