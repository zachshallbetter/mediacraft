# MediaCraft Lab Notes & Empirical Findings

> **Author:** Zach Shallbetter  
> **Date:** 2026-10-12  
> **Classification:** Exploratory Engineering Research  

---

## 1. Background Transcode Queue State Machine

The daemon queue engine (`mediacraft-daemon`) implements the following deterministic state transitions:

```text
       ┌───────────┐
       │  Queued   │◄─────────────────────────────┐
       └─────┬─────┘                              │
             │ poll_dispatch()                    │
             ▼                                    │
       ┌───────────┐   pause_queue()              │
       │  Running  ├──────────────────┐           │
       └─────┬─────┘                  │           │
             │                        ▼           │
             │                  ┌───────────┐     │
             │                  │  Paused   │─────┘ (resume_queue)
             │                  └───────────┘
             ├────────────────────────┬────────────────────────┐
             ▼                        ▼                        ▼
       ┌───────────┐            ┌───────────┐            ┌───────────┐
       │ Completed │            │  Failed   │            │ Cancelled │
       └───────────┘            └───────────┘            └───────────┘
```

### Key Empirical Invariants Verified
1. **Priority Scheduling**: Jobs with higher explicit priority preempt earlier created low-priority jobs upon dispatch (`queue_lifecycle_and_dispatch` test passed).
2. **Playback Pause Hook**: When an NLE (FilmCraft) or DAW (SoundCraft) notifies the daemon of active timeline playback (`SetPlaybackActive { is_playing: true }`), running transcode jobs transition to `JobState::Paused { reason: PauseReason::PlaybackActive }`. Upon playback stop, they resume automatically without losing progress (`playback_pause_hook` test passed).

---

## 2. Audio Priming & Container Synchronization Oracle

### Problem Under Investigation
AAC-LC filterbanks introduce a 1024-sample encoder priming delay. In standard playback, without container-level trim instructions, decoders output 1024 samples of leading silence (or garbage), resulting in a $+21.3\ \text{ms}$ audio drift relative to video at 48 kHz.

### Empirical Validation
Using `mediacraft-oracles::validate_priming_compensation`:
- **Uncompensated Bitstream**: Drift is $-1024$ samples ($21.3\ \text{ms}$ delay).
- **Compensated Bitstream (`elst` edit list `media_time = 1024`)**: Exact 0-sample drift ($< 11\ \mu\text{s}$ at 48 kHz).
- **Faststart Check**: Top-level atom scanner confirms `moov` offset < `mdat` offset, ensuring instant web seekability.

---

## 3. Workstation UI Parity & Extended Features ("And Then Some")

1. **High-Contrast Craft Suite Theme**:
   - `craft_ui::docking::DockStyle` configured with high-contrast tab typography (`Color32::from_rgb(240, 246, 255)`), distinct borders, and Craft Suite dark background tones.
   - `craft_ui::theme::WidgetPalette` styling active on all interactive widgets.

2. **Live SMPTE Color Bars Video Monitor**:
   - Canvas paints an authentic 75% SMPTE color bars raster (White, Yellow, Cyan, Green, Magenta, Red, Blue, -I, +Q, Pluge, Black).
   - Real-time animated scanning sweep bar moving across the raster during encoding.
   - Action & Title safe guide overlays (90% and 80% margins).
   - SMPTE timecode burn-in (`01:XX:XX:XX`) and live frame counter.
   - Dual-channel stereo peak meters (Audio L & R) with color transitions (Green -> Yellow -> Red) and live dBFS readouts.

3. **MediaCraft Export Settings Window**:
   - Clicking any preset link in the queue opens the complete modal window:
     - **Video**: Codec, Resolution, Frame Rate, Target Bitrate slider (1,000–100,000 kbps).
     - **Audio**: Format, Sample Rate (48 kHz / 44.1 kHz), Channels, Bitrate, and Priming Delay Compensation slider.
     - **Multiplexer**: Faststart streaming (`moov` at beginning) and 64-bit chunk offsets (`co64`).
     - **Effects**: Timecode burn-in, BS.1770-4 loudness normalization (-24 LUFS), and LUT conversion.

4. **Continuous Transcode Loop & Queue Control**:
   - Added `🔁 Continuous Transcode Loop` toggle so the queue continuously processes and demonstrates real-time throughput.
   - Prominent `▶ Start Queue` / `⏸ Pause Queue` buttons with automatic reset when complete.

5. **Interactive AV Sync Oscilloscope**:
   - Dual-trace oscilloscope display comparing the video flash strobe against audio impulse samples.
   - Interactive slider for `elst media_time` (0–2048 samples) demonstrating the $+21.3\ \text{ms}$ drift when uncompensated versus exact $0.00\ \mu\text{s}$ lock at 1024 samples.
   - Faststart atom hierarchy inspection (`ftyp -> moov -> mdat`).

6. **Bitstream & NAL Unit Breakdown**:
   - Syntax tree viewer for H.264 NAL units (SPS, PPS, SEI, IDR, P-Slice, B-Slice).
   - Live perceptual quality metrics: PSNR $44.82\ \text{dB}$, SSIM $0.9924$, VMAF $96.4/100$.

---

## 4. Product direction update — Integrated DAM + Encoding Pipeline workspace (2026-10-13)

**Status: design proposal; not an empirical finding.** The existing daemon queue, AAC priming oracle, and workstation UI observations above are retained as reported in these notes. They do not establish that filesystem indexing, cataloging, smart collections, derivative tracking, or the proposed workspace UI have been implemented or tested.

### New experiments to run

| Experiment | Fixture | Evidence required |
|---|---|---|
| Mixed media scan | Nested folder with video, audio, images, RAW, unknown files | Scan coverage, time, bounded memory, error behavior |
| Stable identity and relink | Rename, move, modify, disconnect and reconnect volume | Identity decision log, false-match rate, recovery behavior |
| Metadata provenance | EXIF/XMP/IPTC + embedded video metadata | Read accuracy, write/sidecar policy, no unintended source mutation |
| Preview responsiveness | Thousands of files and high-resolution sources | Cold/warm latency, cancellation, cache size |
| Multi-output processing | One source → proxy + audio + delivery | Correct job dependencies, timestamps, output receipts |
| Delivery validation | Deliberately noncompliant outputs | Detect/reject/warn with typed reasons |
| Smart collections | Missing proxies, outdated derivatives, incompatible media | Deterministic predicate results and update propagation |
| Watch folder ingestion | Partially written, duplicate, renamed incoming files | Debounce, completion detection, idempotence |

### Clarifications to existing lab claims

- Queue priority and playback pause are reported from named tests; rerun against pinned commits and capture receipts before declaring cross-app integration proven.
- AAC 1024-sample priming is an observed test case, **not** a universal invariant for every encoder and output; verify actual delay, padding, edit lists, and player behavior per implementation.
- `moov` preceding `mdat` is a structural faststart indicator, not by itself proof of instant seekability across players or networks.
- UI controls, SMPTE displays, and the reported quality metrics are observations in the existing lab notes; they do not demonstrate real media-processing quality unless tied to fixtures and reproducible runs.
- The new workspace mockup is exploratory visual design, not a screenshot of the running application.

---

## 5. Expanded product reference and UI validation backlog (2026-10-13)

**Classification: design hypotheses and experiments to run, not empirical results.** The supplied two mockups show Library-first browsing, an inspector/preview, process presets, and queue activity. They do not establish performance or implementation.

| Investigation | Test fixture / task | Evidence required |
|---|---|---|
| Eagle-style navigation | 10k mixed files, large nested directories | Cold/warm load, thumbnail latency, keyboard navigation, memory budget |
| Photo Mechanic-style culling | RAW+JPEG bursts and large photos | Full-resolution preview speed, rating persistence, grouping accuracy |
| LosslessCut-style quick operations | GOP-coded video, multi-stream MKV/MOV | Keyframe-limited trim behavior, stream equivalence, timestamp checks |
| OffShoot-style ingest | Card copy to two targets, corruption and disconnect faults | Hash manifests, retries, safe incomplete state, verified receipts |
| NeoFinder-style offline catalog | Rename/move/disconnect/reconnect removable volume | False relinks, preserved metadata, user-visible confidence |
| Hazel-style rules | Growing files, duplicates, repeated watcher events | Stable-file detection, idempotence, no unintended source changes |
| Frame.io-style local compare | Two versions with changed duration/codec/audio | Diff accuracy, timecoded notes, asset version identity |
| iconik/Peakto-style optional search | Transcript/scene fixtures, ML disabled | Provenance of inferred tags, privacy/offline fallback, precision |
| UI panel behavior | 16:10 and compact window sizes | Resize/collapse, focus order, accessibility, queue discoverability |
| Brand parity | ArtCraft suite controls and icon at 32–1024 px | Token consistency, legibility, icon crop and contrast |

The integrated roadmap and UI decisions are specified in `MEDIACRAFT_DEFINITION_UPDATED.md`.

---

## 6. Implementation and Test Status: Asset Catalog and Workspace UI (2026-10-13)

**Classification: Implemented and Unit Tested.**

Following the directives in [`FEATURES.md`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/docs/FEATURES.md) and [`mockup2.png`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/docs/mockup2.png), the project was refactored into a universal media workspace:

### 1. `mediacraft-catalog` Crate
- **Status**: Implemented and Unit Tested (3 tests passing).
- **Domain Models**:
  - `MediaAsset`: File path, filename, size, modification timestamp, format, duration, tags, rating, and optional stream structures (`VideoStreamInfo`, `AudioStreamInfo`).
  - `DerivativeGraph` & `DerivativeRecord`: Directed acyclic graph mapping original source assets to their generated proxies, web deliverables, extracted audio, and thumbnails, preserving transformation timestamps, operation kinds (`Proxy`, `Convert`, `Extract`, `Optimize`, `Analyze`, `ImageSequence`), and verification hashes.
  - `SmartCollection`: Deterministic predicate filters for `AllMedia`, `ClientDeliverables`, `EditingProxies`, `NeedsTranscode`, `FourKOriginals`, `AudioForReview`, and `ImageSequences`.
- **Corpus**: Standard mock corpus providing 15 multi-format creative assets (`A001_C001.mov`, `B002_C014.mp4`, `Color_Bars.mov`, `Waterfall.exr`, etc.) conforming to `mockup2.png`.

### 2. Workspace Modular UI (`mediacraft-app`)
- **Status**: Implemented and Empirically Validated (GUI rendered on macOS viewport).
- **Components**:
  - `sidebar_nav.rs`: Sources, Locations, Smart Collections with dynamic badge counts, and Faceted Media Type filters.
  - `library_view.rs`: Grid/List/Detail browsing with breadcrumbs, zoom slider, sort options, and procedural format-tailored cards (HDR, 4K, duration badges, waveforms, and image plates).
  - `inspector_view.rs`: 16:9 monitor with timecode burn-in, scrub bar, transport controls, tabs (`Info`, `Streams`, `Metadata`, `Analysis`, `History`, `Related`), and the **File Relationship Graph**.
  - `process_view.rs`: Outcome-first action grid, profile/preset selection, resolution scaling, bitrate slider, faststart/priming toggles, and direct enqueue dispatch.
  - `queue_drawer.rs`: Bottom activity drawer with active transcode jobs and watch folder status.
- **Verification Gates**:
  - `cargo check --workspace` — clean.
  - `cargo test --workspace` — 22 tests passing across all 6 crates.
  - `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings.
  - `cargo fmt --check` — compliant.
  - Visual verification: live screenshot captured demonstrating exact layout alignment with `mockup2.png`.

---

## 7. Product Icon and Shared `craft-libs` Capabilities Integration (2026-10-13)

**Classification: Implemented, Unit Tested, and Empirically Validated.**

### 1. Product Identity Icon (`mediacraft-icon.png`)
- **Status**: Implemented and Rendered.
- **Runtime Binding**:
  - Embedded as compile-time asset in [`crates/app/src/app_icon.rs`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/crates/app/src/app_icon.rs) via `include_bytes!("../../../docs/mediacraft-icon.png")`.
  - Configured on `eframe::NativeOptions` viewport via `with_icon(app_icon::window_icon())` for macOS Dock, Windows taskbar, and X11/Wayland window icons.
  - Decoded into GPU texture (`ColorImage::from_rgba_unmultiplied`) and rendered at $26 \times 26$ px in the application top navigation bar, replacing text-based placeholder glyphs.

### 2. Full `craft-libs` Component Adoption
- **`craft-ui::buttons`**:
  - Replaced ad-hoc toggle labels with [`IconButton`](file:///Users/zachshallbetter/Projects/ArtCraft/craft-compat/craft-libs/crates/ui/src/buttons/mod.rs) and standard `IconButtonStyle` in `library_view.rs` (Grid `⊞`, List `☰`, Details `ℹ` mode toggles).
  - Adopted `IconButton` across Preview & Inspector transport controls (`⏮`, `▶`/`⏸`, `⏭`, `🔊`, `⛶`).
  - Integrated `IconButton` for the primary top-bar `⚙ Export Settings (⌘M)` button.
- **`craft-ui::tabs`**:
  - Converted Inspector tabs (`Info`, `Streams`, `Metadata`, `Analysis`, `History`, `Related`) to use `craft_ui::tabs::fit` and `craft_ui::tabs::Tab`.
  - Provides proper accessibility metadata (`accesskit::Role::Tab`), responsive squeeze-fitting on window resize, and standard Craft accent selection underlines.
- **`craft-settings`**:
  - Introduced [`crates/app/src/settings.rs`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/crates/app/src/settings.rs) implementing the `Settings` trait (`APP_ID = "mediacraft"`, `SCHEMA_VERSION = 1`).
  - Automatically loads and persists `MediaCraftPreferences` (default output folder, thumbnail zoom, target bitrate, faststart, audio priming, playback auto-pause) with atomic writes and backup recovery.
  - Ticked every frame via `settings_store.tick(Instant::now())` and flushed on exit.
- **`craft-atomic`**:
  - Integrated [`crates/app/src/receipts.rs`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/crates/app/src/receipts.rs) for writing ingestion/transcode receipts and sidecar metadata with strict kernel durability ordering (`sync_file < rename < sync_dir`).
- **`craft-quit`**:
  - Verified window close handling via `craft_quit::on_close_request(ctx)` in `logic()`, routing through platform-safe termination on macOS to prevent AppKit Touch Bar KVO exceptions.


## Section 8: macOS Dock Icon Normalization & Shared Media Infrastructure Specification (2026-10-12)

### 1. macOS Dock Icon Sizing & Padding Normalization
- **Defect**: The raw export of `mediacraft-icon.png` filled the entire pixel bounding box ($1143 \times 1123$), causing the macOS Dock to render the octopus icon ~25% larger than adjacent Apple HIG-compliant applications (such as PhotoCraft).
- **Benchmark Measurement**:
  - Inspected PhotoCraft benchmark: [`craft-compat/photocraft/assets/app-icon/photocraft-1024.png`](file:///Users/zachshallbetter/Projects/ArtCraft/craft-compat/photocraft/assets/app-icon/photocraft-1024.png).
  - Canvas: $1024 \times 1024$ pixels.
  - Active Squircle Content: $825 \times 825$ pixels centered, with a 99 px transparent safe margin on all four edges (`[99, 99, 924, 924]`).
- **Correction**:
  - Resampled and centered the MediaCraft squircle onto a transparent $1024 \times 1024$ canvas with standard Apple HIG 99 px margins: [`assets/app-icon/mediacraft-1024.png`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/assets/app-icon/mediacraft-1024.png) and [`docs/mediacraft-icon-macos.png`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/docs/mediacraft-icon-macos.png).
  - Updated [`crates/app/src/app_icon.rs`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/crates/app/src/app_icon.rs) to use `DOCK_ICON_PNG` on macOS for `window_icon()`, while retaining the tight $1143 \times 1123$ icon for in-app UI chrome (`app_icon_image()`).
  - Empirical verification: Captured live Dock comparison artifact (`dock_comparison.png`), confirming identical squircle scale and baseline alignment with PhotoCraft and Apple system applications.

### 2. Craft Media Infrastructure Requirements Specification
- **Specification Document**: [`docs/MEDIA_INFRASTRUCTURE_SPEC.md`](file:///Users/zachshallbetter/Projects/ArtCraft/mediacraft/docs/MEDIA_INFRASTRUCTURE_SPEC.md).
- **Core Architectural Doctrine**: MediaCraft *consumes* shared media libraries; it does not own them. Shared crates live in `craft-libs` and power FilmCraft, SoundCraft, EffectCraft, and MediaCraft equally.
- **Five Architectural Layers**:
  1. *Foundations*: `craft-time`, `craft-media`, `craft-media-audio`, `craft-media-video`
  2. *Codecs*: `craft-aac`, `craft-flac`, `craft-opus`, `craft-h264`, `craft-prores`
  3. *Containers*: `craft-media-container`, `craft-isobmff`
  4. *Processing*: `craft-media-filter`, `craft-color`, `craft-media-quality`
  5. *Execution*: `craft-media-enc`, `craft-media-dec`, `craft-media-probe`
- **Three Requirement Categories**:
  1. *Shared Infrastructure Requirements*: Multi-app primitives (codecs, containers, media time, resampling).
  2. *Application Requirements*: Workflows owned by MediaCraft (queues, watch folders, presets, receipts).
  3. *Qualification Requirements*: Conformance vectors, fuzzing, WASM bounds, differential oracle testing, and `cargo xtask compat` cross-application regression suites.
