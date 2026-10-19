# Gaps in `craft-libs` Shared UI Layer

## Context
During the implementation and visual styling of MediaCraft to match the workstation specification (`docs/mediacraft-mockup-3.png`), several missing abstractions and gaps in `craft-libs` (`craft-ui`) were identified.

Currently, `craft-ui` establishes in `docs/theme-infrastructure.md` that:
> *"Applications own their assets, themes, layouts, commands and document semantics. Components accept focused style values and return egui responses; no application engine is required."*

While keeping theme ownership inside applications is architecturally sound, `craft-ui` currently stops short of providing common composable visual primitives. As a result, every application (PhotoCraft, VectorCraft, DesignCraft, MediaCraft) duplicates low-level `egui::Visuals` plumbing and frame boilerplate.

---

## Catalog of Identified Gaps

### 1. Missing Global Visuals / Baseline Theme Constructor (`craft-ui::theme`)
- **Current State**: `craft-ui` only exports `craft_ui::theme::WidgetPalette`, which applies solely to `egui::style::Widgets` (5 interactive button states: `noninteractive`, `inactive`, `hovered`, `active`, `open`).
- **The Gap**: It leaves `egui::Visuals` unconfigured. Fields like `panel_fill`, `window_fill`, `extreme_bg_color`, `faint_bg_color`, `window_stroke`, `selection`, `hyperlink_color`, and shadows default to raw egui grays unless manually wired by the app.
- **Evidence of Duplication**:
  - `vectorcraft/crates/ui-egui/src/theme.rs` lines 295–325
  - `designcraft/crates/ui-egui/src/theme.rs` lines 330–355
  - `lightcraft/crates/ui-egui/src/theme.rs` lines 420–445
  - `mediacraft/crates/app/src/craft_theme.rs`
- **Recommended Extraction**: A `CraftVisuals` builder or baseline function `craft_dark_visuals(tokens: &ThemeTokens) -> egui::Visuals` that configures global panel, input, selection, and shadow tokens without prescribing app-specific accent colors.

### 2. Missing Panel and Surface Frame Primitives (`craft-ui::layout` or `craft-ui::surfaces`)
- **Current State**: `craft-ui` contains `docking` and `layout` helpers, but zero standardized `egui::Frame` definitions.
- **The Gap**: Multi-panel creative workstations (PhotoCraft, SoundCraft, EffectCraft, MediaCraft) require distinct surface hierarchy:
  - Base workspace chrome (`#0E1015`)
  - Elevated panel card frames (`#151821` with `Stroke::new(1.0, #222836)`)
  - Recessed wells for previews, canvas, and data tables (`#090B0E`)
  - Floating dialog/modal frames (`#1A1D27` with drop shadow)
- **Evidence of Duplication**: Every app manually instantiates `egui::Frame { fill, stroke, corner_radius, inner_margin, outer_margin }` repeatedly across views.
- **Recommended Extraction**: Standardized frame constructors:
  ```rust
  craft_ui::surfaces::panel_card_frame(tokens)
  craft_ui::surfaces::recessed_well_frame(tokens)
  craft_ui::surfaces::modal_frame(tokens)
  ```

### 3. Missing Status Badges and Metadata Pills (`craft-ui::badges`)
- **Current State**: `craft-ui` has `craft_ui::buttons::IconButton`, but no non-interactive badge or pill component.
- **The Gap**: Media, export, and document workflows frequently require compact badges:
  - Status indicators: `Encoding` (Cyan), `Waiting` (Dim Gray), `Completed` (Green), `Failed` (Red).
  - Format/Codec pills: `H.264`, `ProRes 422 HQ`, `MP4`, `AAC`.
  - Preset tags: `YouTube 4K`, `Podcast 1080p`.
- **Evidence of Duplication**: MediaCraft, LightCraft, and PhotoCraft each render ad-hoc rects with `ui.painter().rect_filled` or custom small buttons.
- **Recommended Extraction**: A standalone, zero-allocation `Pill` / `Badge` widget with semantic tints (`Neutral`, `Info`, `Success`, `Warning`, `Danger`).

### 4. Missing High-Contrast Progress Meter with Metrics (`craft-ui::progress`)
- **Current State**: Only stock `egui::ProgressBar` is available.
- **The Gap**: Stock `ProgressBar` lacks support for:
  - Custom track background and border strokes.
  - Multi-part status strings (e.g. `42% • 00:01:23 remaining`).
  - Pulsing / indeterminate states for active background jobs.
- **Evidence of Duplication**: MediaCraft queue transcoding, LightCraft batch export, and Jeremy's model downloader in PR #74 / PR #76.

### 5. Missing Two-Column Property Inspector Grid (`craft-ui::inspector`)
- **Current State**: None.
- **The Gap**: Technical sheets (Media Info, Metadata, EXIF, Transform, Audio Properties) across PhotoCraft, VectorCraft, PdfCraft, and MediaCraft all need:
  - Fixed-width dimmed label column on the left.
  - Highlighted or editable value column on the right.
  - Subtle alternating row backgrounds or dividers.
- **Recommended Extraction**: A lightweight `PropertyGrid` helper in `craft-ui`.

### 6. Missing Media Transport and Timecode Controls (`craft-ui::media_controls`)
- **Current State**: None.
- **The Gap**: Time-based Craft apps (MediaCraft, FilmCraft, SoundCraft, EffectCraft) all share:
  - Signed SMPTE / frame-accurate timecode formatting (`HH:MM:SS:FF`).
  - Transport button clusters (`|<`, `<<`, `Play/Pause`, `>>`, `>|`).
  - Scrubbing timeline bar with playhead.
- **Recommended Extraction**: Media transport presentation widgets decoupled from audio/video engine specifics.
