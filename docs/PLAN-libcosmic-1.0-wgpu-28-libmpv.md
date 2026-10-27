# Option B: libcosmic 1.0 + wgpu 28 + libmpv Vulkan Migration

**Status:** ⛔ **BLOCKED** — R1 confirmed: wgpu 28 has no public Vulkan handle export (see Appendix B)
**Author:** fangorn
**Date:** 2026-06-17

## Progress Log

| Date | Milestone | Result |
|------|-----------|--------|
| 2026-06-17 | §1 Pre-flight (1.1–1.11) | 10 of 11 gates passed. **1.5 FAILED** (wgpu 28 has no Vulkan handle export). Full details in Appendix B. |
| 2026-06-17 | §2 Toolchain preparation | ✅ rustc 1.96.1 ≥ 1.93. Edition 2024 + rust-version bump deferred (blocked). |
| 2026-06-17 | §3 Dependency bump | ⏸️ Deferred — blocked by R1. |
| 2026-06-17 | §4 iced / libcosmic API migration | ⏸️ Deferred — blocked by R1. |
| 2026-06-17 | §5 Pipeline rewrite (new Primitive) | ⏸️ Deferred — blocked by R1 (no mpv Vulkan render context to draw). |
| 2026-06-17 | §6 wgpu 28 API migration | ⛔ **R1 confirmed blocking.** |
| 2026-06-17 | §7 Libmpv FFI + Vulkan render integration | ✅ **§7.1 + §7.2 complete.** `src/mpv/ffi.rs` (hand-written FFI, correct ABI verified via struct-layout test + standalone probe) and `src/mpv/player.rs` (safe `MpvPlayer` wrapper with `Drop`, property getters, command helpers, event API) are implemented, wired into `lib.rs`, linked via `build.rs`, and pass 3 pure-logic unit tests + 1 ABI-layout test. ⛔ §7.3 (Vulkan render context) remains blocked by R1. **Finding:** libmpv 0.41.0 segfaults when initialized inside the Rust test harness (which links gstreamer/wgpu/iced simultaneously) even though the identical call sequence works in a standalone binary. Runtime tests are therefore excluded from the suite; validated via standalone probe instead. |
| 2026-06-17 | §8 Model port (Video / Audio) | ⏸️ Deferred — depends on §5/§7. |
| 2026-06-17 | §9 Packaging / CI / Flatpak / Docs | ⏸️ Deferred — depends on §3/§8. |
| 2026-06-17 | §10 Testing & validation | ⏸️ Deferred. |

### Decision required

R1 is a **hard blocker** for the mpv Vulkan sub-API path (§6.2 / §7.3) as
designed. Three re-scoping options exist (see Appendix B, Q2):

1. **mpv `opengl_cb` sub-API** (Option A from the original rationale):
   mpv renders into an EGL/OpenGL context; we blit to wgpu via
   `glReadPixels` or a shared GL texture. Requires `EGL` + `GL` linkage.
   This is the most viable path for maintaining iced integration.

2. **mpv VO overlay** (wayland/x11): mpv renders directly into a
   compositor surface. No wgpu involvement. Loses iced compositing
   (overlay, tabs, multi-window) but is the highest-performance path.

3. **Keep GStreamer** for now; do the libcosmic 1.0 / iced 0.14.0 /
   edition 2024 migration (which is still required regardless) **without**
   the mpv swap, and revisit the video backend separately.

**Recommendation:** Option 3 first (unblock the iced/libcosmic/edition
migration which is independent of the video backend), then revisit the
video backend via Option 1 once the UI is stable on iced 0.14.0.

### Additional finding (2026-06-17)

**libmpv test-harness incompatibility:** libmpv 0.41.0 segfaults (SIGSEGV
inside `libmpv.so.2`) when initialized within the media-browser test binary
(which simultaneously links gstreamer, wgpu 22, iced 0.14.0-dev, libcosmic,
and ~150 other crates). The *identical* call sequence (create → set options →
initialize → property read → terminate) succeeds in a standalone `rustc`
binary linked only against `libmpv`. This appears to be a symbol-interposition
or static-initializer-ordering issue with libmpv's internal libplacebo
initialization when many other shared libraries (particularly Vulkan
loader / ICD) are also loaded. Workaround: runtime FFI tests are excluded
from the unit-test suite; the ABI shape is verified by a pure-layout test
and the full runtime path by a standalone probe binary.

## Rationale

The current playback stack (GStreamer + a fork of `iced_video_player`) is
maintenance-heavy: six GStreamer crates, ~12 system packages, a custom
NV12→RGBA wgpu pipeline, and a worker thread that pulls frames out of an
`appsink`. libmpv collapses all of that into a single C library with a
stable FFI, a hardware-accelerated decode path, and a native Vulkan render
context.

The render-context path (`mpv_render_context_*` with the `MPV_SUB_API_VULKAN`
sub-API) requires the application to own the `VkInstance` / `VkPhysicalDevice`
/ `VkDevice` / `VkQueue`. In the current stack those are owned by **wgpu 22**
behind the scenes of **libcosmic 0.x** (commit `e162c59`, iced 0.14.0-dev).
wgpu 22 does not expose its Vulkan handles, so we cannot hand them to mpv.

**Option B** bumps to **libcosmic 1.0** (which ships **wgpu 28**) so that
wgpu 28's Vulkan interop API can be used to extract the handles mpv needs.
This is the correct long-term path and eliminates the GStreamer dependency
entirely.

The cost is a major-version bump of libcosmic (edition 2024, rustc 1.93+,
iced 0.14.0 stable, new `Primitive` trait) that touches the entire UI
codebase. This document is the step-by-step plan.

---

## Table of Contents

1. [Pre-flight](#1-pre-flight)
2. [Toolchain Preparation](#2-toolchain-preparation)
3. [Dependency Bump](#3-dependency-bump)
4. [iced / libcosmic API Migration](#4-iced--libcosmic-api-migration)
5. [Pipeline Rewrite (New Primitive Trait)](#5-pipeline-rewrite-new-primitive-trait)
6. [wgpu 28 API Migration](#6-wgpu-28-api-migration)
7. [Libmpv FFI + Vulkan Render Integration](#7-libmpv-ffi--vulkan-render-integration)
8. [Model Port (Video / Audio)](#8-model-port-video--audio)
9. [Packaging / CI / Flatpak / Docs](#9-packaging--ci--flatpak--docs)
10. [Testing & Validation](#10-testing--validation)
11. [Rollback Plan](#11-rollback-plan)
12. [Risk Register](#12-risk-register)
13. [Open Questions (resolve in pre-flight)](#13-open-questions)

---

## 1. Pre-flight

Before any code changes, confirm the following. Each item is a **gate**:
if it fails, stop and re-scope.

| # | Check | How | Pass criteria |
|---|-------|-----|---------------|
| 1.1 | libcosmic 1.0 commit | `git ls-remote https://github.com/pop-os/libcosmic.git refs/heads/main` then inspect `Cargo.toml` for `version = "1.0"` and `wgpu = "28"` | A specific commit hash that declares `wgpu = "28"` and `edition = "2024"` |
| 1.2 | iced 0.14.0 stable | Inspect the libcosmic commit's `Cargo.toml` | `iced = "0.14.0"` (not `-dev`) |
| 1.3 | wgpu 28.0 | `cargo info wgpu --version` or inspect lockfile after bump | `wgpu 28.x` resolves |
| 1.4 | rustc ≥ 1.93 | `rustc --version` on this box and on the CI runner image | ≥ 1.93 |
| 1.5 | wgpu 28 Vulkan interop API | Search wgpu 28 source for `as_raw`, `instance_handle`, or equivalent that exposes `VkInstance`, `VkPhysicalDevice`, `VkDevice`, `VkQueue` | A documented way to obtain raw Vulkan handles from a `wgpu::Instance` / `wgpu::Device` |
| 1.6 | New `Primitive` trait signature | Inspect iced 0.14.0 source: `Primitive::prepare`, `Primitive::draw`, associated `type Pipeline` | Confirmed signatures to target in §5 |
| 1.7 | All `Primitive` impls in codebase | `grep -r "impl Primitive" src/` | Should be exactly two: `src/video/pipeline.rs` and `src/audio/pipeline.rs` |
| 1.8 | All `Storage` usages | `grep -r "primitive::Storage\|Storage" src/` | Should only appear in the two pipeline files and the two player files |
| 1.9 | libmpv available | `pkg-config --libs mpv` or `ldconfig -p | grep libmpv` | `libmpv.so` present |
| 1.10 | mpv ≥ 0.40 | `mpv --version` | ≥ 0.40 (Vulkan sub-API stability) |
| 1.11 | Vulkan SDK / driver | `vulkaninfo --summary` | ICD present, Vulkan ≥ 1.2 |

**Output:** Record the exact libcosmic commit hash, the wgpu 28 interop
API function names, and the new `Primitive` trait signature in the
"Resolved" section at the bottom of this file.

---

## 2. Toolchain Preparation

### 2.1 rustc / Rust toolchain

- Bump `rust-version` in `Cargo.toml` from `"1.80"` to `"1.93"` (or the
  minimum required by libcosmic 1.0's `rust-version` field).
- Update `rust-toolchain.toml` if one exists (none currently).
- CI: `dtolnay/rust-toolchain@stable` already tracks the latest stable;
  verify it is ≥ 1.93. Alternatively pin: `dtolnay/rust-toolchain@1.93`.

### 2.2 Edition 2024

- Change `edition = "2021"` → `edition = "2024"` in `Cargo.toml`.
- Run `cargo fix --edition` to auto-migrate most items.
- Manual migration checklist (edition 2024 changes):
  - `unsafe_op_in_unsafe_fn` is now deny-by-default → audit all `unsafe fn`
    bodies (the FFI module in §7 will have many).
  - `gen` is a reserved keyword → rename any `gen` identifiers.
  - `!` (never type) in `dyn` traits: `dyn Trait<!>` no longer allowed.
  - `gen` keyword, `unsafe extern` blocks, `static mut` deprecation
    warnings → plan to replace `static mut` with `AtomicX` or `LazyLock`.
  - `impl Trait` in `static`/`const` items no longer allowed.
  - `unsafe attributes` in function/extern blocks.
  - Path syntax: `use` items with `as self` (no change), but
    `#[macro_use]` is deprecated → prefer explicit imports.
  - `extern` blocks now require `unsafe` unless all items are safe.

### 2.3 MSRV in CI

- Update `.github/workflows/ci.yml` to install rustc ≥ 1.93 (or pin).
- If a separate MSRV job exists, bump it.

---

## 3. Dependency Bump

### 3.1 Cargo.toml changes

```toml
# REMOVE (all GStreamer)
gstreamer = "0.23"
gstreamer-app = "0.23"
gstreamer-base = "0.23"
gstreamer-pbutils = "0.23"
gstreamer-tag = "0.23"
gst-plugin-matroska = "0.0.0"

# REMOVE (no longer needed once GStreamer is gone)
gio = { version = "0.20.6", optional = true }   # if only used by GStreamer
glib = { version = "0.20.6", optional = true }   # if only used by GStreamer
# NOTE: gio/glib are also used by the gvfs feature; verify before removing.

# ADD (libmpv FFI — hand-written, no external crate)
# The FFI module links directly: `cargo:rustc-link-lib=dylib=mpv`
# in a small build script or build.rs. No new [dependencies] entry needed
# unless we want `pkg-config` to locate the library:
# pkg-config = "0.3"   (in [build-dependencies])

# UPDATE (libcosmic)
[dependencies.libcosmic]
git = "https://github.com/pop-os/libcosmic.git"
rev = "<PINNED_COMMIT>"        # ← fill in from §1.1
default-features = false
features = [
    "a11y",
    "multi-window",
    "tokio",
    "winit",
    "wgpu",
    # New/changed features in 1.0 — verify which are needed:
    # "desktop-systemd-scope",
    # "single-instance",
    # "applet",
    # "pipewire",
    # "process",
    # "rfd",
    # "smol",
]

# UPDATE (accesskit_unix — iced 0.14.0 stable may require a different version)
[dependencies.accesskit_unix]
version = "<VERIFY>"          # ← was "= 0.13.1"
default-features = false
features = ["tokio"]

# VERIFY (zbus — libcosmic 1.0 may require a different version)
zbus = { version = "<VERIFY>", default-features = false, features = ["tokio"] }
# ← was "= 4.4.0"
```

### 3.2 Feature flags

Current `[features]` section:

```toml
[features]
default = ["advanced", "bzip2", "desktop", "liblzma", "notify", "winit", "wgpu", "gvfs"]
advanced = []
desktop = ["libcosmic/desktop", "dep:xdg"]
jemalloc = ["dep:tikv-jemallocator"]
notify = ["dep:notify-rust"]
wayland = ["libcosmic/wayland", "dep:wayland-client"]
winit = ["libcosmic/winit"]
wgpu = ["libcosmic/wgpu"]
glib = ["dep:glib"]
gvfs = ["dep:gio", "dep:glib"]
```

Changes:
- **Remove** `glib` and `gvfs` features **only if** `gio`/`glib` are no
  longer needed (verify: they are used by `mounter/gvfs.rs` for GVFS
  operations, which is independent of GStreamer). **Keep them** if the
  GVFS mounter still needs them.
- **Add** `mpv = []` feature (or fold into `wgpu`) that gates the libmpv
  playback path. During migration, a `gst-legacy = []` feature gates the
  old path (see §11).
- **Verify** each `libcosmic/<feature>` still exists in 1.0. The feature
  set changed between 0.x and 1.0.

### 3.3 `[patch.crates-io]`

- Keep the `filetime` patch (still needed).
- Remove any patches that were only for the 0.x libcosmic dependency tree.

### 3.4 `Cargo.lock`

- After all `Cargo.toml` changes, run `cargo update` (or `cargo generate-lockfile`)
  to regenerate the lockfile.
- **Verify** no duplicate major versions of `wgpu`, `iced`, or `cosmic-theme`.

### 3.5 `build.rs`

- Currently uses `vergen` for build metadata. Keep.
- If we add a `build-dependencies` entry for `pkg-config` to locate
  `libmpv`, add the `cargo:rustc-link-lib=dylib=mpv` +
  `cargo:rustc-link-search` instructions there. Alternatively, use a
  tiny `link-mpv` helper crate or just emit the link flags from `build.rs`:

```rust
// build.rs (new section)
fn main() {
    // ... existing vergen code ...

    // Link libmpv
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=dylib=mpv");
        // Optionally: pkg-config to find the .so path
        if let Ok(lib_dir) = std::env::var("LIBMPV_LIB_DIR") {
            println!("cargo:rustc-link-search=native={lib_dir}");
        }
    }
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=dylib=mpv");
    }
}
```

---

## 4. iced / libcosmic API Migration

This is the **bulk of the work**. The iced API between 0.14.0-dev (what
libcosmic 0.x pins) and 0.14.0 stable (what libcosmic 1.0 ships) has
changes. Additionally, the `cosmic::app::Application` trait and
`cosmic::widget::*` APIs may have shifted.

### 4.1 Audit all iced/cosmic imports

Run:

```sh
grep -rn "use cosmic::" src/ | sort
grep -rn "cosmic::iced" src/ | sort
grep -rn "cosmic::iced_core" src/ | sort
grep -rn "cosmic::iced_runtime" src/ | sort
grep -rn "cosmic::iced_wgpu" src/ | sort
grep -rn "cosmic::iced_winit" src/ | sort
grep -rn "cosmic::widget" src/ | sort
grep -rn "cosmic::app" src/ | sort
grep -rn "cosmic::cosmic_theme" src/ | sort
grep -rn "cosmic::cosmic_config" src/ | sort
```

### 4.2 Files that need migration (by module)

| File | What changes |
|------|-------------|
| `src/lib.rs` | `cosmic::app::Settings` builder API, `cosmic::app::run` signature |
| `src/app/mod.rs` | `Application` trait impl (`init`, `update`, `view`, `core`), `Task` API, layer-surface commands, `segmented_button` |
| `src/app/misc.rs` | `cosmic::app::{message, Task}`, `cosmic::iced::keyboard`, `cosmic::iced::window` |
| `src/app/nav.rs` | `cosmic::iced::clipboard::dnd::DndAction`, `segmented_button::Entity` |
| `src/app/ops.rs` | `cosmic::iced_runtime::clipboard`, `cosmic::widget` |
| `src/app/tabs.rs` | `cosmic::iced::window`, `segmented_button::Entity` |
| `src/app/types.rs` | `cosmic::cosmic_theme`, `cosmic::iced::clipboard`, `cosmic::iced::keyboard` |
| `src/app/views.rs` | `cosmic::app::Task`, `cosmic::iced::window` |
| `src/app/dialogs.rs` | Dialog APIs (verify `cosmic::dialog` changes) |
| `src/app/operations.rs` | Operation APIs |
| `src/app/scan.rs` | Scan-related cosmic APIs |
| `src/app/search.rs` | Search UI |
| `src/app/trash.rs` | Trash UI |
| `src/clipboard.rs` | `cosmic::iced::clipboard::mime` |
| `src/config.rs` | `cosmic_config::Config` (verify API stability) |
| `src/dialog.rs` | `cosmic::dialog` |
| `src/key_bind.rs` | `cosmic::iced::keyboard` |
| `src/menu.rs` | `cosmic::widget::menu` |
| `src/mime_app.rs` | `cosmic::widget` |
| `src/mime_icon.rs` | `cosmic::widget::icon` |
| `src/mouse_area.rs` | `cosmic::iced::advanced::widget` |
| `src/tab.rs` | Tab widget, `cosmic::widget` |
| `src/video/video_player.rs` | `Widget` impl, `Renderer` trait bound, `draw_primitive` → new API |
| `src/video/pipeline.rs` | **Full rewrite** (§5) |
| `src/audio/audio_player.rs` | `Widget` impl, `Renderer` trait bound, `draw_primitive` → new API |
| `src/audio/pipeline.rs` | **Full rewrite** (§5) |
| `src/image/image.rs` | `cosmic::iced_core::*` imports |
| `src/image/image_player.rs` | `cosmic::iced_core::*` imports, `Widget` impl |
| `src/image/image_view.rs` | `cosmic::iced::Length` |
| `src/operation/mod.rs` | `cosmic::iced::futures` |
| `src/mounter/gvfs.rs` | `glib` / `gio` (keep if GVFS mounter stays) |

### 4.3 `Application` trait migration

The `cosmic::app::Application` trait in 1.0 may have changed method
signatures. Key things to verify:

- `fn init(state: &mut Self::State, flags: Self::Flags) -> Task<Self::Message>`
  — did `Task` replace `Command`?
- `fn core(&self) -> &Core` → `fn core(&self) -> &Core` (verify)
- `fn update(&mut self, message: Self::Message, app: &mut cosmic::app::Core) -> Task<Self::Message>`
  — did the `app` parameter change?
- `fn view(&self) -> Element<'_, Self::Message>` — verify
- `fn appearance(&self, core: &Core) -> Option<cosmic_theme::Appearance>` — verify
- `fn subscriptions(&self) -> iced::Subscription<Self::Message>` — verify

### 4.4 `Widget` impl migration

The `Widget` trait is the most critical. In iced 0.14.0 stable:

- `fn size(&self) -> Size<Length>` — likely unchanged.
- `fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node` — verify.
- `fn draw(&self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) -> Renderer::Output` — verify the `Renderer::Output` return type (in 0.14.0-dev it may have been `()` or a different type).
- `fn on_event(...)` — verify `Shell` trait changes.
- `fn mouse_interaction(...)` — verify.

The `Renderer` trait bound on `VideoPlayer` / `AudioPlayer` currently is:

```rust
where Renderer: PrimitiveRenderer  // = cosmic::iced_wgpu::primitive::Renderer
```

In 1.0 / wgpu 28, the `PrimitiveRenderer` trait (or its replacement) will
have a different `draw_primitive` method signature (or the method may be
renamed / moved). **This is the key integration point** between the
player widgets and the pipeline.

### 4.5 `cosmic::widget` APIs

Verify each of these still exist and their signatures in 1.0:

- `cosmic::widget::row`, `column`, `button`, `dropdown`, `slider`,
  `popover`, `mouse_area`, `text`, `icon`, `container`
- `cosmic::widget::segmented_button` (used heavily in `app/`)
- `cosmic::widget::menu`
- `cosmic::widget::pane`
- `cosmic::widget::context_drawer`
- `cosmic::widget::toast`

### 4.6 `cosmic::iced_wgpu` module

In 1.0, the module structure may have changed. Currently we use:

```rust
use cosmic::iced_wgpu::{self, primitive::Primitive, wgpu};
use cosmic::iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use cosmic::iced_wgpu::primitive::Storage;
```

In the new version, the equivalent imports need to be identified. The
`wgpu` re-export should still be there. The `primitive` module will have
the new `Primitive` trait (§5).

### 4.7 Migration strategy

1. **First, bump the dependency and let the compiler find the breakage.**
   Do NOT try to fix everything in one pass. Run `cargo check` and triage
   errors by module.

2. **Order of modules to fix** (least to most coupled):
   1. `src/clipboard.rs`, `src/key_bind.rs`, `src/config.rs` — small,
      isolated.
   2. `src/image/*` — image display, no GStreamer.
   3. `src/app/*` — the main app shell. This is the largest surface area.
   4. `src/video/*`, `src/audio/*` — depends on the new `Primitive` trait
      (§5) and the new `Renderer` API.
   5. `src/mounter/*`, `src/operation/*` — peripheral.

3. **After each module compiles, run `cargo clippy` and fix warnings.**

4. **After the whole crate compiles, run `cargo test --lib`.**

---

## 5. Pipeline Rewrite (New Primitive Trait)

### 5.1 Current state

Both `src/video/pipeline.rs` and `src/audio/pipeline.rs` implement the
**old** `Primitive` trait:

```rust
impl Primitive for VideoPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut iced_wgpu::primitive::Storage,
        bounds: &Rectangle,
        _viewport: &iced_wgpu::graphics::Viewport,
    ) { /* ... */ }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &iced_wgpu::primitive::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) { /* ... */ }
}
```

The old trait uses a `Storage` (a type-erased per-frame map) to cache
GPU resources across frames. The new trait (iced 0.14.0 / wgpu 28) uses
an **associated type** `Pipeline` instead.

### 5.2 New trait (target)

Based on iced 0.14.0 stable, the `Primitive` trait is expected to look
like (verify in §1.6):

```rust
pub trait Primitive: Send + Sync {
    type Pipeline: Send + Sync;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        bounds: &Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
    ) { /* ... */ }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut wgpu::RenderPass,
        clip_bounds: &Rectangle<u32>,
    ) -> bool { /* ... */ }
}
```

Key differences:
- **No `Storage`** — state lives in `Self::Pipeline`.
- **`draw` returns `bool`** (likely "did we draw anything?").
- **`draw` takes `&mut wgpu::RenderPass`** directly (not
  `&mut CommandEncoder`). The renderer begins the pass and passes it in.
- **`pipeline` is passed by `&mut`** in `prepare` (mutable, can create
  resources) and by `&` in `draw` (immutable, just record commands).

### 5.3 Rewrite plan

**Step 1:** Define the new `Pipeline` type. For video:

```rust
struct MpvVideoPipeline {
    // wgpu render pipeline for the blit (or: mpv renders directly)
    // In the libmpv Vulkan path, mpv renders into its own VkImage.
    // We then import that as a wgpu::Texture and draw it.
    mpv_render_ctx: Option<MpvRenderContext>,
    // ...
}
```

**Step 2:** Implement `prepare`:
- First call: create the wgpu state (render pipeline, textures, etc.).
- Subsequent calls: update uniforms (bounds/viewport), check for
  mpv frame availability.

**Step 3:** Implement `draw`:
- Call `mpv_render_context_update` + `mpv_render_context_render` into the
  shared `VkImage`.
- Import the `VkImage` as a `wgpu::Texture`.
- `render_pass.set_pipeline(...)` + `render_pass.draw(...)`.
- Return `true` if a frame was rendered, `false` otherwise.

**Step 4:** Delete the old `VideoEntry`, `VideoPipeline` (old),
`VideoPrimitive` structs and the `shader.wgsl` file (both video and
audio).

**Step 5:** Update the player widgets (`video_player.rs`,
`audio_player.rs`) to call the new `draw_primitive` / `draw` API.

### 5.4 The libmpv Vulkan path (detailed in §7)

The pipeline rewrite and the libmpv integration are **the same work** in
Option B. The old NV12→RGBA wgpu pipeline is **deleted**, replaced by
mpv rendering directly into a Vulkan image that wgpu 28 can import.
There is no intermediate YUV→RGBA conversion in Rust.

---

## 6. wgpu 28 API Migration

Between wgpu 22 and wgpu 28, several APIs changed. Audit every `wgpu::*`
reference in the codebase (after the pipeline rewrite, this will be
limited to the new pipeline code and any direct wgpu usage).

### 6.1 Known changes to verify

| Item | wgpu 22 | wgpu 28 (verify) |
|------|---------|-------------------|
| `ShaderModuleDescriptor` | `source: ShaderSource` | Same or renamed? |
| `BindGroupLayoutDescriptor` / `BindGroupLayoutEntry` | Current shape | Same? |
| `RenderPipelineDescriptor` | Current shape | Same? |
| `TextureDescriptor` | Current shape | Same? |
| `BufferDescriptor` | Current shape | Same? |
| `BindGroupDescriptor` / `BindGroupEntry` | Current shape | Same? |
| `RenderPassDescriptor` | Current shape | Same? |
| `SamplerDescriptor` | Current shape | Same? |
| `ImageCopyTexture` / `ImageDataLayout` | Current shape | Same? |
| `PipelineCompilationOptions` | Current shape | Same? |
| `Queue::write_texture` | Current signature | Same? |
| `Queue::write_buffer` | Current signature | Same? |
| `Device::create_*` methods | Current signatures | Same? |
| `Instance` | Current | **New: Vulkan interop** (§7) |

### 6.2 Vulkan interop (the critical new API)

> ⛔ **BLOCKED (pre-flight 2026-06-17):** wgpu 28.0.0 does **not**
> expose the underlying Vulkan handles. No `as_vk_instance()`, no
> `instance_handle()`, no `ash::vk::*` types in the public API.
> `ExternalImageSource` is `#[cfg(all(target_arch = "wasm32",
> feature = "web"))]` — unavailable on native. The native
> `Device::create_external_texture(desc, planes: &[&TextureView])`
> takes WebGPU-style plane views, not an arbitrary `VkImage` handle.
> **Consequence:** the mpv `MPV_SUB_API_VULKAN` render context (§7.3)
> cannot receive the `VkInstance`/`VkDevice`/`VkQueue` it requires,
> because we cannot extract them from `wgpu::Instance` /
> `wgpu::Device`. This blocks the entire Option B approach as written.
> See Appendix B for alternatives.

The expected API (per the original plan, **not available in wgpu 28**):

```rust
// wgpu 28 (expected — verify exact names)
let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { /* ... */ });

// Option A: direct access
let vk_instance: ash::vk::Instance = instance.as_vk_instance();
let vk_physical_device: ash::vk::PhysicalDevice = instance.as_vk_physical_device();

// Option B: via the device
let (adapter, surface) = /* ... */;
let device = adapter.request_device(/* ... */);
let vk_device: ash::vk::Device = device.as_vk_device();
let vk_queue: ash::vk::Queue = device.as_vk_queue(family_index);
```

**This is the single most important API to verify.** If wgpu 28 does
NOT expose these, Option B is blocked and we fall back to Option A
(`opengl_cb`).

### 6.3 Importing a `VkImage` into wgpu

Once mpv renders into a `VkImage`, we need to use it as a wgpu texture.
The expected flow:

```rust
// Create a wgpu texture that wraps the VkImage
let wgpu_texture = device.create_external_texture_from_vk_image(
    vk_image,
    image_width,
    image_height,
    wgpu::TextureFormat::Bgra8Unorm,
);
let view = wgpu_texture.create_view(&Default::default());
```

Alternatively, if wgpu 28 provides `import_external_texture` or a
`wgpu_hal` interop path, use that. **Verify the exact API.**

---

## 7. Libmpv FFI + Vulkan Render Integration

### 7.1 FFI module

Create `src/mpv/ffi.rs` (or `src/video/mpv_ffi.rs`) with hand-written
FFI declarations. No bindgen, no external crate. The mpv client ABI has
been stable since mpv 0.17 (2018).

```rust
// src/mpv/ffi.rs
use std::os::raw::{c_char, c_int, c_void};

// --- Opaque types ---
#[repr(C)] pub struct mpv_handle { _private: [u8; 0] }
#[repr(C)] pub struct mpv_event { _private: [u8; 0] }
#[repr(C)] pub struct mpv_render_context { _private: [u8; 0] }

// --- Constants ---
pub const MPV_FORMAT_NONE: c_int = 0;
pub const MPV_FORMAT_STRING: c_int = 1;
pub const MPV_FORMAT_INT64: c_int = 4;
pub const MPV_FORMAT_DOUBLE: c_int = 8;

pub const MPV_EVENT_SHUTDOWN: c_int = 1;
pub const MPV_EVENT_END_FILE: c_int = 8;
pub const MPV_EVENT_FILE_LOADED: c_int = 9;
pub const MPV_EVENT_ERROR: c_int = 12;
pub const MPV_EVENT_TICK: c_int = 13;

pub const MPV_SUB_API_OPENGL_CB: c_int = 1;
pub const MPV_SUB_API_VULKAN: c_int = 2;

pub const MPV_RENDER_PARAM_VULKAN_INSTANCE: c_int = 100;
pub const MPV_RENDER_PARAM_VULKAN_PHYSICAL_DEVICE: c_int = 101;
pub const MPV_RENDER_PARAM_VULKAN_DEVICE: c_int = 102;
pub const MPV_RENDER_PARAM_VULKAN_QUEUE_FAMILY_INDEX: c_int = 103;
pub const MPV_RENDER_PARAM_VULKAN_QUEUE: c_int = 104;
pub const MPV_RENDER_PARAM_VULKAN_SWAPCHAIN_IMAGE: c_int = 105;
pub const MPV_RENDER_PARAM_VULKAN_SWAPCHAIN_IMAGE_COUNT: c_int = 106;

// --- mpv_event struct (partial — we only need the fields we read) ---
#[repr(C)]
pub struct mpv_event_data {
    pub event_id: c_int,
    pub error: *const c_char,
    pub reason: *const c_char,
    pub file: *const c_char,
    pub time_pos: f64,
    pub duration: f64,
    // ... (extend as needed)
}

// --- Core API ---
extern "C" {
    pub fn mpv_create() -> *mut mpv_handle;
    pub fn mpv_terminate_destroy(handle: *mut mpv_handle);
    pub fn mpv_initialize(handle: *mut mpv_handle) -> c_int;
    pub fn mpv_set_option_string(
        handle: *mut mpv_handle,
        name: *const c_char,
        value: *const c_char,
    ) -> c_int;
    pub fn mpv_command_string(handle: *mut mpv_handle, command: *const c_char) -> c_int;
    pub fn mpv_get_property_string(
        handle: *mut mpv_handle,
        name: *const c_char,
        format: c_int,
    ) -> *mut c_char;
    pub fn mpv_get_property(
        handle: *mut mpv_handle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
    pub fn mpv_request_event(
        handle: *mut mpv_handle,
        event: *const c_char,
        flag: c_int,
    ) -> c_int;
    pub fn mpv_wait_event(
        handle: *mut mpv_handle,
        timeout: f64,
    ) -> *const mpv_event;
    pub fn mpv_free(handle: *mut mpv_handle, data: *mut c_void);
    pub fn mpv_free_node(handle: *mut mpv_handle, data: *mut c_void);
    pub fn mpv_get_error_string(error: c_int) -> *const c_char;

    // --- Render context API ---
    pub fn mpv_render_context_create(
        handle: *mut mpv_handle,
        rc: *mut *mut mpv_render_context,
        api: *const c_char,
        params: *const c_void,  // mpv_render_param[]
    ) -> c_int;
    pub fn mpv_render_context_update(rc: *mut mpv_render_context);
    pub fn mpv_render_context_render(
        rc: *mut mpv_render_context,
        params: *const c_void,  // mpv_render_param[]
    );
    pub fn mpv_render_context_report_swap(rc: *mut mpv_render_context);
    pub fn mpv_render_context_free(handle: *mut mpv_handle, rc: *mut mpv_render_context);
}

// --- mpv_render_param ---
#[repr(C)]
pub struct mpv_render_param {
    pub type_: *const c_char,
    pub data: *const c_void,
}

// --- mpv_node (for mpv_get_property) ---
#[repr(C)]
pub struct mpv_node {
    pub format: c_int,
    pub data: *mut c_void,
}
```

### 7.2 Safe wrapper

Create `src/mpv/player.rs` with a safe Rust wrapper:

```rust
pub struct MpvPlayer {
    handle: *mut mpv_handle,
}

impl MpvPlayer {
    pub fn new() -> Result<Self, MpvError> {
        unsafe {
            let handle = mpv_create();
            if handle.is_null() { return Err(MpvError::CreateFailed); }
            mpv_set_option_string(handle, b"config\0".as_ptr() as _, b"no\0".as_ptr() as _);
            mpv_set_option_string(handle, b"ao\0".as_ptr() as _, b"null\0".as_ptr() as _);
            mpv_set_option_string(handle, b"vo\0".as_ptr() as _, b"null\0".as_ptr() as _);
            let rc = mpv_initialize(handle);
            if rc != 0 {
                mpv_terminate_destroy(handle);
                return Err(MpvError::InitFailed(rc));
            }
            Ok(MpvPlayer { handle })
        }
    }

    pub fn load_file(&self, path: &str) -> Result<(), MpvError> { /* ... */ }
    pub fn seek(&self, time: f64) -> Result<(), MpvError> { /* ... */ }
    pub fn set_paused(&self, paused: bool) { /* ... */ }
    pub fn set_volume(&self, vol: f64) { /* ... */ }
    pub fn position(&self) -> f64 { /* ... */ }
    pub fn duration(&self) -> f64 { /* ... */ }
    pub fn width(&self) -> u32 { /* ... */ }
    pub fn height(&self) -> u32 { /* ... */ }
    pub fn set_speed(&self, speed: f64) { /* ... */ }
    pub fn set_subtitle_url(&self, url: &str) { /* ... */ }
    pub fn tracks(&self) -> Vec<Track> { /* ... */ }
    pub fn set_track(&self, kind: &str, index: i32) { /* ... */ }
    pub fn wait_event(&self, timeout: f64) -> MpvEvent { /* ... */ }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        unsafe { mpv_terminate_destroy(self.handle); }
    }
}
```

### 7.3 Vulkan render context

```rust
pub struct MpvVulkanRender {
    rc: *mut mpv_render_context,
    vk_image: ash::vk::Image,
    vk_image_count: u32,
    current_image_index: u32,
}

impl MpvVulkanRender {
    pub fn new(
        player: &MpvPlayer,
        vk_instance: ash::vk::Instance,
        vk_physical_device: ash::vk::PhysicalDevice,
        vk_device: ash::vk::Device,
        queue_family_index: u32,
        vk_queue: ash::vk::Queue,
        width: u32,
        height: u32,
        image_count: u32,
    ) -> Result<Self, MpvError> {
        // Create VkImage(s) here (or accept them from wgpu)
        // ...
        unsafe {
            let mut rc: *mut mpv_render_context = std::ptr::null_mut();
            let params: [mpv_render_param; 7] = [
                mpv_render_param { type_: b"vulkan-instance\0".as_ptr() as _, data: &vk_instance as *const _ as _ },
                mpv_render_param { type_: b"vulkan-physical-device\0".as_ptr() as _, data: &vk_physical_device as *const _ as _ },
                mpv_render_param { type_: b"vulkan-device\0".as_ptr() as _, data: &vk_device as *const _ as _ },
                mpv_render_param { type_: b"vulkan-queue-family-index\0".as_ptr() as _, data: &queue_family_index as *const _ as _ },
                mpv_render_param { type_: b"vulkan-queue\0".as_ptr() as _, data: &vk_queue as *const _ as _ },
                mpv_render_param { type_: b"vulkan-swapchain-image\0".as_ptr() as _, data: &vk_image as *const _ as _ },
                mpv_render_param { type_: b"vulkan-swapchain-image-count\0".as_ptr() as _, data: &image_count as *const _ as _ },
            ];
            let rc_int = mpv_render_context_create(
                player.handle,
                &mut rc,
                b"vulkan\0".as_ptr() as _,
                params.as_ptr(),
            );
            if rc_int != 0 { return Err(MpvError::RenderContextCreate(rc_int)); }
            Ok(MpvVulkanRender { rc, vk_image, vk_image_count: image_count, current_image_index: 0 })
        }
    }

    pub fn render_frame(&mut self) -> bool {
        unsafe {
            mpv_render_context_update(self.rc);
            mpv_render_context_render(self.rc, std::ptr::null());
            // Increment image index (round-robin)
            self.current_image_index = (self.current_image_index + 1) % self.vk_image_count;
            true
        }
    }

    pub fn current_image(&self) -> ash::vk::Image { self.vk_image }
}

impl Drop for MpvVulkanRender {
    fn drop(&mut self) {
        unsafe { mpv_render_context_free(/* handle */, self.rc); }
    }
}
```

### 7.4 Per-frame render loop

Inside the new `Primitive::draw` (§5):

```rust
fn draw(
    &self,
    pipeline: &MpvVideoPipeline,
    render_pass: &mut wgpu::RenderPass,
    clip_bounds: &Rectangle<u32>,
) -> bool {
    let mpv_render = pipeline.mpv_render.lock().unwrap();
    mpv_render.render_frame();

    // Import the current VkImage as a wgpu texture
    let wgpu_texture = pipeline.import_vk_image(mpv_render.current_image());
    let view = wgpu_texture.create_view(&Default::default());

    // Blit into the render pass
    render_pass.set_pipeline(&pipeline.blit_pipeline);
    render_pass.set_bind_group(0, &pipeline.blit_bind_group, &[]);
    render_pass.set_viewport(
        clip_bounds.x as f32,
        clip_bounds.y as f32,
        clip_bounds.width as f32,
        clip_bounds.height as f32,
        0.0, 1.0,
    );
    render_pass.draw(0..4, 0..1);
    true
}
```

### 7.5 Audio

For audio-only playback, mpv decodes and plays audio through its own
audio output (PulseAudio / PipeWire / ALSA). We do **not** need a render
context for audio — we just need to show cover art (static image) and
handle the `END_FILE` event for looping/EOS. The `Audio` model (§8)
wraps `MpvPlayer` with `--ao=pulse` (or the system default) and no
video output. The existing `AudioPlayer` widget still draws the cover
art image via the normal iced image path — no `Primitive` needed for
audio in the end.

**This simplifies the audio path significantly:** the old
`src/audio/pipeline.rs` (NV12 shader for cover art) can be **deleted**,
and `AudioPlayer` falls back to the standard `renderer.draw(image::Handle)`
path (which already exists as the non-wgpu fallback in the current code).

---

## 8. Model Port (Video / Audio)

### 8.1 `Video` model (`src/video/video.rs`)

Replace the GStreamer-based `Internal` struct with an `MpvPlayer`-backed
one. Keep the **public API identical** so `video_view.rs` and
`video_player.rs` need minimal changes.

Current public API to preserve:
- `Video::new(&str, Option<String>) -> Result<Video, Error>`
- `video.size() -> (u32, u32)`
- `video.framerate() -> f64`
- `video.set_volume(f64)` / `video.volume() -> f64`
- `video.set_muted(bool)` / `video.muted() -> bool`
- `video.eos() -> bool`
- `video.looping() -> bool` / `video.set_looping(bool)`
- `video.set_paused(bool)` / `video.paused() -> bool`
- `video.seek(Position, bool) -> Result<(), Error>`
- `video.set_speed(f64) -> Result<(), Error>` / `video.speed() -> f64`
- `video.position() -> Duration`
- `video.duration() -> Duration`
- `video.restart_stream() -> Result<(), Error>`
- `video.set_subtitle_url(String)` / `video.subtitle_url() -> Option<String>`
- `video.thumbnails() -> Result<Vec<...>, Error>` (screenshot via mpv
  `screenshot-to-file` or `screenshot` command)

Mapping to mpv:

| GStreamer | mpv |
|-----------|-----|
| `playbin uri=...` | `loadfile <path>` |
| `bus.pop_filtered` | `mpv_wait_event` (poll in render loop) |
| `set_state(Paused/Playing)` | `set pause yes/no` |
| `query_position` | `get_property time-pos` |
| `query_duration` | `get_property duration` |
| `set volume` | `set volume <0-100>` |
| `set mute` | `set mute yes/no` |
| `seek(rate, flags, Set, pos, ...)` | `seek <time> absolute` |
| `n-audio` / `get-audio-tags` | `get_property track-list` (JSON) |
| `current-audio` | `get_property aid` |
| `n-text` / `get-text-tags` | `get_property track-list` (JSON, type=sub) |
| `current-text` | `get_property sid` |
| `av-offset` | `set sub-delay <value>` |
| `suburi` | `sub-visibility` + external sub load |
| `set speed` | `set speed <value>` |
| `looping` | `--loop-file` or manual `loadfile` on `END_FILE` |
| `EOS` | `MPV_EVENT_END_FILE` |
| `Error` | `MPV_EVENT_ERROR` / `MPV_EVENT_SHUTDOWN` |

### 8.2 `Audio` model (`src/audio/audio.rs`)

Same pattern. `Audio::new(uri, posterpath)` creates an `MpvPlayer` with
`--no-video` (or `--vo=null`) and the system audio output. The cover art
is loaded from `posterpath` (or the default dummy image) via the existing
`coverart::frame_from_image` function — this part does **not** change.

The only difference from the current code: no `appsink`, no worker
thread, no NV12 frame buffer. The `Audio` struct just wraps `MpvPlayer`
+ the cover art image dimensions.

### 8.3 `video_view.rs` simplification

The current `video_view.rs` has a lot of GStreamer-specific code:
- `language_name()` using `gstreamer_tag::ffi` → replace with a small
  static table or use mpv's `track-list` which already includes names.
- `GST_PLAY_FLAG_*` manipulation → delete.
- `pipeline.property("n-audio")` / `get-audio-tags` → replace with
  `video.tracks()` returning a `Vec<Track>` from mpv's `track-list`.
- `pipeline.property("flags")` → delete.

The `Message` enum can lose `MissingPlugin` (no GStreamer plugins to
miss). `NewFrame` can be replaced by a periodic `TICK` event or a
timer-based redraw.

### 8.4 Delete

After the port:
- `src/video/shader.wgsl` — delete.
- `src/audio/shader.wgsl` — delete.
- `src/video/pipeline.rs` — **replace** with the new `Primitive` impl
  (§5/§7).
- `src/audio/pipeline.rs` — **delete** (audio uses the standard image
  path, no custom `Primitive` needed).
- `src/video/yuv_to_rgba` — delete (no more NV12).
- `src/video/video.rs` GStreamer code — replace with mpv.
- `src/audio/audio.rs` GStreamer code — replace with mpv.

---

## 9. Packaging / CI / Flatpak / Docs

### 9.1 `Cargo.toml` packaging metadata

**`[package.metadata.deb.depends]`:**

Remove all `gstreamer1.0-*` entries. Add:

```toml
depends = [
    "libc6",
    "libxkbcommon0",
    "libmpv2",       # mpv shared library
    "vulkan-tools",  # optional, for vulkaninfo
]
```

**`[package.metadata.generate-rpm.requires]`:**

Remove all `gstreamer1-*` entries. Add:

```toml
[package.metadata.generate-rpm.requires]
glibc = "*"
libxkbcommon = "*"
mpv-libs = "*"      # provides libmpv.so
# vulkan-tools = "*"  # optional
```

### 9.2 `debian/control`

```
Build-Depends:
    clang,
    debhelper-compat (=13),
    just (>= 1.13.0),
    libmpv-dev,          # ← replaces libgstreamer1.0-dev
    libxkbcommon-dev,
    pkg-config,
    rust-all,

Depends: ${misc:Depends}, ${shlibs:Depends},
      libmpv2,           # ← replaces gstreamer1.0-plugins-*
      ffmpeg             # ← keep (mpv uses it for some formats)
```

### 9.3 `.github/workflows/ci.yml`

Replace GStreamer system deps with:

```yaml
- name: Install system dependencies
  run: |
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends \
      build-essential \
      pkg-config \
      libmpv-dev \
      libxkbcommon-dev \
      libwayland-dev \
      libbzip2-dev \
      liblzma-dev \
      vulkan-tools
```

Remove: `gstreamer1.0-dev`, `gstreamer1.0-plugins-base`,
`libglib2.0-dev` (unless still needed by `gio`/`glib` for the GVFS
mounter).

### 9.4 Flatpak

The flatpak manifest (if it exists — currently only `flatpak/fcg.py`
is a helper script) needs the GStreamer runtime replaced with:
- `org.mpv.Mpv` (or `org.freedesktop.Platform` + mpv via extra
  runtime).
- `org.freedesktop.Vulkan` for the Vulkan API.

### 9.5 `justfile`

No GStreamer-specific recipes found. The `build-deb` / `build-rpm`
recipes will automatically pick up the new dependencies from
`Cargo.toml` / `debian/control`. No changes needed unless a new
`libmpv-dev` install step is required for local development:

```just
# Add a recipe for local dev setup
install-dev-deps:
    sudo apt install libmpv-dev libxkbcommon-dev libwayland-dev vulkan-tools
```

### 9.6 Docs

- **`README.md`:** Replace the GStreamer "Required dependencies" section
  with mpv. Update the "Development status" paragraph.
- **`Todo.md`:** Update video/audio feature entries.
- **`SUGGESTIONS.md`:** Update references to GStreamer.
- **`debian/copyright`:** Add mpv / libmpv if needed (usually not, since
  we link against the system library).

---

## 10. Testing & Validation

### 10.1 Smoke test (manual)

1. Build: `just build-release`
2. Run: `just run /path/to/sample.mkv`
3. Verify: video plays, audio is heard.
4. Test: play/pause, seek (scrub + relative ±10s), volume slider,
   mute toggle.
5. Test: subtitle track switching (if the sample has subs).
6. Test: audio track switching (if the sample has multiple audio).
7. Test: speed control (0.5x, 1x, 2x).
8. Test: loop on / off.
9. Test: close video tab → verify mpv is terminated (no zombie process).
10. Test: open audio file → cover art displays, play/pause/seek work.

### 10.2 Regression test (manual)

- Image view: open a JPEG/PNG → displays, zoom, pan.
- Browser: navigate directories, search, sort.
- Tabs: open/close/reorder.
- Menu: all items respond.
- Keyboard shortcuts: verify each binding in `key_bind.rs`.
- Trash: move/restore files.
- Mounter: mount/unmount GVFS (if still present).
- Multi-window: open a second window (libcosmic `multi-window`).

### 10.3 Automated tests

- `cargo test --lib` — existing unit tests.
- `cargo clippy --lib` — no new warnings.
- `cargo fmt --all --check` — clean.
- Add a basic integration test: open the sample MKV via the `Video`
  model API, verify `duration() > 0`, `width() > 0`, `height() > 0`,
  seek to 50%, verify `position()` is in range, pause, resume, close.

### 10.4 Performance

- Measure frame rate with `mpv --gpu-api=vulkan` (should be 60+ fps on
  RTX 5090 for the sample).
- Measure memory: `cat /proc/$(pidof media-browser)/status | grep VmRSS`
  before and after opening a video.
- Measure CPU: `top -p $(pidof media-browser)` during playback.
- Compare with the GStreamer baseline (record before deleting).

### 10.5 Multi-display / Wayland / X11

- Single display: works.
- Multi-display (two monitors): video window on secondary monitor,
  verify correct scaling and no tearing.
- Wayland (KWin): primary target, verify compositor integration.
- X11 (Xwayland): verify via `xhost +` or a VNC session.

---

## 11. Rollback Plan

During the migration (between the first `cargo check` and the final
`cargo clippy --lib` passing), the old GStreamer code must remain
compilable. Strategy:

1. **Feature-gate the old path:**

```toml
[features]
default = ["mpv", ...]
mpv = []
gst-legacy = []   # old GStreamer path
```

2. In `src/video/mod.rs`:

```rust
#[cfg(feature = "mpv")]
mod mpv_video;    // new MpvPlayer-backed Video

#[cfg(feature = "gst-legacy")]
mod video;        // old GStreamer-backed Video
```

3. In `src/video/pipeline.rs`:

```rust
#[cfg(feature = "mpv")]
pub struct MpvVideoPrimitive { ... }

#[cfg(feature = "gst-legacy")]
pub struct VideoPrimitive { ... }  // old code
```

4. The player widgets compile against whichever is active:

```rust
#[cfg(feature = "mpv")]
fn draw_primitive(...) { /* new API */ }

#[cfg(feature = "gst-legacy")]
fn draw_primitive(...) { /* old API */ }
```

5. **Do NOT delete the old code until the new path is fully validated**
   (all tests in §10 pass on at least two different hardware configs).

6. **Final deletion** (after validation):
   - Remove `gst-legacy` feature.
   - Delete `src/video/video.rs` (old), `src/audio/audio.rs` (old),
     `src/video/pipeline.rs` (old), `src/audio/pipeline.rs`,
     `src/video/shader.wgsl`, `src/audio/shader.wgsl`.
   - Remove GStreamer crates from `Cargo.toml`.
   - Remove GStreamer from packaging (as described in §9).

---

## 12. Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|------------|
| R1 | wgpu 28 does **not** expose Vulkan instance/device/queue handles | **Confirmed** | **Blocking** | **RESOLVED: wgpu 28.0.0 has no public API to extract `VkInstance`/`VkPhysicalDevice`/`VkDevice`/`VkQueue`. No `as_vk_*`, no `instance_handle`, no `ash::vk` in the public surface. `ExternalImageSource` is `#[cfg(wasm32)]`-only. Native `create_external_texture` takes `[&TextureView]` (WebGPU planes), not an arbitrary `VkImage`. The mpv Vulkan sub-API path (§6.2/§7.3) is architecturally impossible with wgpu 28's public API. See Appendix B for details and alternatives.** |
| R2 | rustc 1.93 not available on target distros / CI | Low | High | Pin to latest available rustc; verify libcosmic 1.0 still builds. If not, use a Rust docker image for CI. |
| R3 | `accesskit_unix` / `zbus` version pins conflict with iced 0.14.0 stable | Medium | Medium | Unpin; let cargo resolve. If a specific version is required, add it. |
| R4 | `cosmic-theme` / `cosmic-config` API changes break `config.rs` / `app/` | Medium | Medium | These are separate crates; verify their 1.0 APIs early. |
| R5 | The new `Primitive` trait signature differs from the one in §5.2 | Low | High | Verify against iced 0.14.0 source in §1.6. The plan is structured so §5 is the last step — if the trait is different, only §5 changes. |
| R6 | mpv Vulkan render context requires a specific `VkImage` format that wgpu 28 cannot import | Low | High | Test with the sample MKV before integrating. mpv supports `VK_FORMAT_B8G8R8A8_UNORM` and `VK_FORMAT_A8B8G8R8_UNORM` — both map to `wgpu::TextureFormat::Bgra8Unorm` / `Rgba8Unorm`. |
| R7 | Multi-window / multi-tab breaks when two `MpvPlayer` instances run simultaneously | Low | Medium | mpv supports multiple independent handles. Each `Video` / `Audio` gets its own `mpv_handle`. Verify in testing. |
| R8 | Edition 2024 migration introduces subtle `unsafe` / `static mut` breakage | Medium | Low | `cargo fix --edition` handles most. The FFI module (§7) is the only `unsafe`-heavy code; audit it carefully. |
| R9 | Audio playback quality / output device selection differs from GStreamer | Low | Low | mpv uses the same PulseAudio / PipeWire backends. Test on both. |

---

## 13. Open Questions (resolve in pre-flight)

- [ ] **Q1:** Which exact libcosmic 1.0 commit to pin? (`430f3d4` or
      `6af8b70`? Or a newer one?) Both use wgpu 28.0; `6af8b70` also
      introduces a `wgpu-bare` feature. **→ Record answer here.**
- [ ] **Q2:** What is the exact wgpu 28 API for exposing the underlying
      Vulkan instance/physical device/device/queue? **→ Record the
      function names here.**
- [ ] **Q3:** What is the exact new `Primitive` trait signature in iced
      0.14.0? (associated type name, method signatures, return types)
      **→ Record here.**
- [ ] **Q4:** Are there any `Primitive` implementations in the codebase
      besides `src/video/pipeline.rs` and `src/audio/pipeline.rs`?
      **→ Confirmed: no. (grep verified.)**
- [ ] **Q5:** Are there any `Storage` usages outside those two pipeline
      files and the two player files? **→ Confirmed: no. (grep verified.)**
- [ ] **Q6:** Does `gio` / `glib` remain needed for the GVFS mounter
      (`src/mounter/gvfs.rs`)? If yes, keep them as optional deps.
      **→ Verify: `mounter/gvfs.rs` uses `glib::MainContext` and
      `gio::File`. Keep `gio`/`glib` as optional deps behind the `gvfs`
      feature.**
- [ ] **Q7:** Does the `zbus = "= 4.4.0"` pin still work with
      libcosmic 1.0? **→ Verify in §1.**
- [ ] **Q8:** Does `accesskit_unix = "= 0.13.1"` still work with iced
      0.14.0 stable? **→ Verify in §1.**

---

## Appendix A: File Change Summary

| File | Action | Section |
|------|--------|---------|
| `Cargo.toml` | Update | §3 |
| `build.rs` | Update (add mpv link) | §3.5 |
| `.github/workflows/ci.yml` | Update (system deps) | §9.3 |
| `debian/control` | Update (deps) | §9.2 |
| `justfile` | Minor (dev deps recipe) | §9.5 |
| `README.md` | Update (deps) | §9.6 |
| `Todo.md` | Update | §9.6 |
| `SUGGESTIONS.md` | Update | §9.6 |
| `src/lib.rs` | Migrate (cosmic API) | §4 |
| `src/app/*.rs` (11 files) | Migrate (cosmic API) | §4 |
| `src/clipboard.rs` | Migrate | §4 |
| `src/config.rs` | Migrate (verify) | §4 |
| `src/dialog.rs` | Migrate (verify) | §4 |
| `src/key_bind.rs` | Migrate | §4 |
| `src/menu.rs` | Migrate | §4 |
| `src/mime_app.rs` | Migrate | §4 |
| `src/mime_icon.rs` | Migrate | §4 |
| `src/mouse_area.rs` | Migrate | §4 |
| `src/tab.rs` | Migrate | §4 |
| `src/image/*.rs` (3 files) | Migrate | §4 |
| `src/operation/mod.rs` | Migrate | §4 |
| `src/mounter/*.rs` | Migrate (verify gio/glib) | §4 |
| `src/mpv/ffi.rs` | **NEW** | §7.1 |
| `src/mpv/player.rs` | **NEW** | §7.2 |
| `src/mpv/vulkan.rs` | **NEW** | §7.3 |
| `src/mpv/mod.rs` | **NEW** | §7 |
| `src/video/video.rs` | **REWRITE** (mpv-backed) | §8.1 |
| `src/video/video_player.rs` | Migrate (new Primitive API) | §5, §7 |
| `src/video/pipeline.rs` | **REWRITE** (new Primitive + mpv) | §5, §7 |
| `src/video/video_view.rs` | Simplify (remove GStreamer) | §8.3 |
| `src/video/shader.wgsl` | **DELETE** | §8.4 |
| `src/video/mod.rs` | Update (module declarations) | §8 |
| `src/audio/audio.rs` | **REWRITE** (mpv-backed) | §8.2 |
| `src/audio/audio_player.rs` | Migrate (remove Primitive) | §7.5 |
| `src/audio/pipeline.rs` | **DELETE** | §8.4 |
| `src/audio/shader.wgsl` | **DELETE** | §8.4 |
| `src/audio/mod.rs` | Update (module declarations) | §8 |

## Appendix B: Resolved Pre-flight Answers

**Pre-flight performed: 2026-06-17, rustc 1.96.1, libmpv v0.41.0, Vulkan 1.4.341 (NVIDIA renderD128)**

### Q1: libcosmic 1.0 commit

Both `430f3d4` and `6af8b70` (in `~/.cargo/git/checkouts/libcosmic-41009aea1d72760b/`) are:
- `version = "1.0.0"`
- `edition = "2024"`, `rust-version = "1.93"`
- iced **0.14.0 stable** (vendored in-tree, `path = "."`)
- **wgpu 28.0.0** (resolved in Cargo.lock)
- **ash 0.38.0+1.3.281**
- accesskit_unix **0.17.2** (in registry)
- zbus **5.x** (registry has 5.3.0 – 5.19.0; libcosmic 1.0 references `zbus/blocking-api` for single-instance)

**Recommended pin:** `6af8b70` (newer, also introduces `wgpu-bare` feature for decoupled wgpu usage).

### Q2: wgpu 28 Vulkan interop API

**⛔ NOT AVAILABLE.** Searched the full wgpu 28.0.0 source (`~/.cargo/registry/src/*/wgpu-28.0.0/`):

- No `as_vk_instance()`, `as_vk_device()`, `as_vk_queue()`, `as_vk_physical_device()` methods.
- No `instance_handle()`, `device_handle()`, or equivalent raw-handle accessors.
- `ash::vk` types are **not** in the public API surface.
- `wgpu_hal` is re-exported (`pub extern crate wgpu_hal as hal`) but the safe high-level API does not expose `create_texture_from_hal` or any `VkImage` import on native.
- `ExternalImageSource` enum is `#[cfg(all(target_arch = "wasm32", feature = "web"))]` — **web-only**, not available on Linux.
- `Device::create_external_texture(desc, planes: &[&TextureView])` — takes WebGPU-style texture planes, not an arbitrary Vulkan image handle.

**Conclusion:** There is no way to extract `VkInstance`/`VkPhysicalDevice`/`VkDevice`/`VkQueue` from wgpu 28's safe API, and no way to import an mpv-rendered `VkImage` into a wgpu texture. **The mpv Vulkan sub-API path (§7.3) is architecturally blocked.**

**Viable alternatives (re-scope required):**
1. **mpv `screenshot-to-file` per frame** — mpv renders to an RGB file (PNG/JPEG), we decode it and upload via `queue.write_texture()`. Works, but adds disk I/O and decode latency per frame. Not suitable for 60fps.
2. **mpv `opengl_cb` sub-API** — mpv renders into an EGL/OpenGL context we own. Requires `EGL` + `GL` linkage, separate from wgpu. The existing `VideoPrimitive` NV12→RGBA shader pipeline could be adapted to accept RGBA frames via `glReadPixels` or a shared GL texture. This is "Option A" from the original rationale.
3. **mpv `wayland` / `x11` VO (overlay)** — mpv renders directly into a compositor surface. No wgpu involvement. Loses iced integration but is the most performant.
4. **Keep GStreamer** — the current working path.

### Q3: New Primitive trait signature (confirmed from iced 0.14.0 / wgpu 28 source)

```rust
// iced_wgpu/src/primitive.rs (6af8b70)
pub trait Primitive: Debug + MaybeSend + MaybeSync + 'static {
    type Pipeline: Pipeline + MaybeSend + MaybeSync;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    );

    fn draw(&self, _pipeline: &Self::Pipeline, _render_pass: &mut wgpu::RenderPass<'_>) -> bool { false }

    fn render(
        &self,
        _pipeline: &Self::Pipeline,
        _encoder: &mut wgpu::CommandEncoder,
        _target: &wgpu::TextureView,
        _clip_bounds: &Rectangle<u32>,
    ) { }
}

pub trait Pipeline: Any + MaybeSend + MaybeSync {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self where Self: Sized;
    fn trim(&mut self) {}
}

pub trait Renderer: core::Renderer {
    fn draw_primitive(&mut self, bounds: Rectangle, primitive: impl Primitive);
}
```

**Key differences from §5.2 of the plan:**
- `prepare` does **not** take `format` (it's in `Pipeline::new` instead).
- `draw` takes **only** `pipeline` + `render_pass` (no `clip_bounds` — baked into the pass).
- There is a fallback `render()` method (takes `encoder`, `target`, `clip_bounds`) called when `draw()` returns `false`.
- `Pipeline` is a **separate trait** (with `new` and `trim`), not just an associated type.
- `Storage` still exists internally but is managed by iced's `BlackBox<P>` wrapper — user code only sees `Pipeline`.

### Q4: All `Primitive` impls in codebase

Confirmed: exactly two — `src/video/pipeline.rs` and `src/audio/pipeline.rs`.

### Q5: All `Storage` usages

Confirmed: only in the two pipeline files and the two player files.

### Q6: `gio`/`glib` for GVFS mounter

Keep as optional deps behind `gvfs` feature. `mounter/gvfs.rs` uses `glib::MainContext` and `gio::File` — independent of GStreamer.

### Q7: zbus version

Registry has zbus 5.3.0 through 5.19.0. libcosmic 1.0 references `zbus/blocking-api`. The current pin `= 4.4.0` must be updated. **Recommended:** remove the pin, let cargo resolve against libcosmic 1.0's requirement.

### Q8: accesskit_unix version

Registry has `accesskit_unix 0.17.2`. The current pin `= 0.13.1` must be updated. **Recommended:** bump to `0.17.2` or remove the pin.

### Pre-flight gate status

| # | Check | Status |
|---|-------|--------|
| 1.1 | libcosmic 1.0 commit | ✅ `6af8b70` (v1.0.0, wgpu 28.0.0, iced 0.14.0) |
| 1.2 | iced 0.14.0 stable | ✅ confirmed in-tree |
| 1.3 | wgpu 28.0 | ✅ confirmed in Cargo.lock |
| 1.4 | rustc ≥ 1.93 | ✅ 1.96.1 |
| 1.5 | wgpu 28 Vulkan interop | ⛔ **NOT AVAILABLE** — blocks Option B |
| 1.6 | New `Primitive` trait | ✅ confirmed (see Q3 above) |
| 1.7 | `Primitive` impls | ✅ exactly two |
| 1.8 | `Storage` usages | ✅ only in pipeline + player files |
| 1.9 | libmpv available | ✅ `libmpv.so` present, v0.41.0 |
| 1.10 | mpv ≥ 0.40 | ✅ 0.41.0 |
| 1.11 | Vulkan SDK / driver | ✅ Vulkan 1.4.341, NVIDIA renderD128 |

---

*End of plan.*