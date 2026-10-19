Refs #67, refs #77.

### Description

Multi-panel workstation layouts across the suite (PhotoCraft, VectorCraft, SoundCraft, MediaCraft) suffer from nested outer margins and unstyled gray backgrounds, creating an unstyled "box-in-a-box" layout rather than an edge-to-edge docked workstation.

### Root Cause

Current `craft-ui` primitives lack baseline theme defaults and flush frame containers:
1. `craft_ui::theme::WidgetPalette` only configures the 5 button interaction states (`crates/ui/src/theme/mod.rs`). Global `egui::Visuals` (`panel_fill`, `window_fill`, `extreme_bg_color`, `selection`) remain at egui's default gray palette unless manually wired by each app.
2. Panels lack flush docked frame helpers: eframe's root `CentralPanel` margin (8-12px) combined with layout column padding and individual panel card borders creates concentric insets around viewports and tables.
3. macOS windows default to native title bars rather than fullsize content view with traffic lights integrated over the top menu bar.

### Required Changes in craft-ui

1. Add `craft_ui::theme::baseline_dark_visuals()` returning an `egui::Visuals` with suite-standard dark tokens (`panel_fill: #0E1015`, `window_fill: #151821`, `extreme_bg_color: #080A0E`, `window_stroke: 1px #222836`).
2. Add `craft_ui::surfaces::panel_card_frame()` and `recessed_well_frame()` for flush 1px border docking without nested outer margins.
3. Document `ViewportBuilder` configuration for macOS `with_fullsize_content_view(true)` and borderless window integration.

### Acceptance Criteria

- [ ] Baseline dark visuals helper available in `craft_ui::theme`.
- [ ] Flush panel and well frame constructors in `craft_ui::surfaces` with zero outer margin.
- [ ] Documentation for seamless window chrome in `docs/workstation-layout.md`.
- [ ] Synthetic visual regression test in `craft-ui` verifying flush panel borders without double insets.
