# MediaCraft compatibility guide: Bridge and Media Encoder

**Purpose:** Compare capabilities documented for Adobe Bridge and Adobe Media Encoder with the MediaCraft code and UI currently present in this repository. This is a source-code inventory, not a claim of feature parity or a runtime validation report.

**Status definitions**

- **Implemented:** A corresponding usable capability exists in MediaCraft source/UI.
- **Partial:** A related mechanism exists, but the Adobe capability's scope or workflow is incomplete.
- **Planned:** Product direction documents it, but code evidence was not found.
- **No evidence:** No corresponding implementation was found in the inspected code.

Code presence does not prove production quality or end-to-end correctness. “Implemented” here means implementation evidence exists; runtime behavior still needs its own verification.

## Bridge comparison

| Bridge capability | Status | MediaCraft implementation and gap |
|---|---|---|
| Browse folders and files | **Partial** | The catalog scanner walks a selected path and classifies supported files; the Library UI presents catalog assets. This is not yet evidence of a complete filesystem browser for arbitrary volumes, network locations, favorites, multiple content tabs, or Bridge-equivalent navigation. `crates/catalog/src/scanner.rs`, `crates/app/src/library_view.rs` |
| Thumbnails and rich previews | **Partial** | Imported raster images decode from their source files; video poster frames and audio waveforms are decoded asynchronously through FFmpeg. RAW, document, project, and archive rendering and a complete format/caching matrix remain unsupported. `crates/app/src/media_preview.rs`, `crates/app/src/inspector_view.rs`, `crates/app/src/library_view.rs` |
| Search, sort, and filter | **Partial** | Catalog collections support predicates and the Library UI has filters; a complete metadata query language, indexed deep search, and Bridge's operating-system search options were not established. `crates/catalog/src/collections.rs`, `crates/app/src/library_view.rs` |
| Metadata inspection | **Partial** | Scanner extracts technical metadata for supported formats, and the Inspector exposes asset details. Complete EXIF/IPTC/XMP coverage and parity for Adobe-specific document metadata are not established. `crates/catalog/src/scanner.rs`, `crates/app/src/inspector_view.rs` |
| Metadata editing/templates | **Partial** | Sidecars read/write selected rating, label, tags, title, description, and creator fields. No full metadata-template editor/import workflow or broad field mapping is evidenced. `crates/catalog/src/sidecar.rs`, `crates/app/src/receipts.rs` |
| Keywords, labels, ratings | **Partial** | Tags and ratings exist in asset models, batch mutation, sidecars, collection rules, and UI actions. Hierarchical keyword management and full Bridge metadata interoperability are not established. `crates/catalog/src/lib.rs`, `crates/catalog/src/collections.rs`, `crates/catalog/src/batch_rename.rs`, `crates/catalog/src/sidecar.rs` |
| Static and smart collections | **Implemented** | Catalog defines static reference collections and dynamic rule-based collections; the app exposes collection navigation. Verify persistence and editing behavior separately. `crates/catalog/src/collections.rs`, `crates/app/src/sidebar_nav.rs` |
| Stacks and related versions | **Partial** | Catalog has stack/derivative models and relinking support; parity with Bridge's capture stacks and automatic HDR/panorama grouping is not established. `crates/catalog/src/stacks.rs`, `crates/catalog/src/relink.rs` |
| Batch rename | **Implemented** | Token-based rename planning previews targets and detects collisions; execution has rollback handling. String substitution/regex parity and every Bridge naming token are not established. `crates/catalog/src/batch_rename.rs`, `crates/app/src/main.rs` |
| Move, copy, delete, arbitrary file operations | **Partial** | Rename and relink operations exist. A complete safe file manager matching Bridge copy/move/delete workflows was not found. `crates/catalog/src/batch_rename.rs`, `crates/catalog/src/relink.rs` |
| Camera/photo ingest | **No evidence** | Bridge Photo Downloader-style device ingest, destination rules, and ingest verification were not found. Product direction lists ingest as future work. `docs/FEATURES.md` |
| Contact sheets | **Partial** | Catalog generates HTML contact sheets and a desktop action writes one to a temporary path. Bridge's configurable PDF Output workspace is not evidenced as implemented. `crates/catalog/src/contact_sheet.rs`, `crates/app/src/main.rs` |
| Repeatable workflows and quick actions | **Partial** | Daemon watch-folder rules and encoding presets exist, but a general ordered workflow builder for rename, metadata, resize, and format conversion is not established. `crates/daemon/src/watch_folder.rs`, `crates/app/src/automate_view.rs` |
| Image conversion, resize, Camera Raw, HDR/panorama | **Partial / No evidence** | Media encoders and image/file parsers exist for selected formats; Bridge's general image conversion, resize, Camera Raw editing, and HDR/panorama merge flows are not evidenced as equivalent workflows. `crates/media/src`, `crates/psd/src`; see format-specific implementation before claiming support. |
| Adobe libraries, Stock, Portfolio publishing | **No evidence** | No Adobe cloud-library or publishing integration was found. |
| Media Encoder/Premiere hand-off | **Partial** | MediaCraft has typed IPC/client infrastructure for Craft apps; specific Bridge-to-Adobe application integrations are not evidenced. `crates/protocol/src/lib.rs`, `crates/client/src`, `crates/daemon/src/ipc.rs` |

## Media Encoder comparison

| Media Encoder capability | Status | MediaCraft implementation and gap |
|---|---|---|
| Add sources and create queue jobs | **Implemented** | Protocol, daemon, CLI, and app support typed jobs and queue display/actions. Exact supported source classes vary by pipeline. `crates/protocol/src/lib.rs`, `crates/daemon/src/lib.rs`, `crates/cli/src/main.rs`, `crates/app/src/queue_view.rs` |
| Queue lifecycle, progress, pause/resume, cancel, retry | **Implemented** | Daemon state includes queued/running/paused/completed/failed/cancelled states and pause/resume/cancel/retry methods. Persistence/restart semantics and parity with all AME queue actions need separate review. `crates/daemon/src/lib.rs`, `crates/protocol/src/lib.rs` |
| Persistent queue and recoverable jobs | **Partial** | Queue and job data structures exist; durable restart recovery/checkpoint-resume equivalent to a persistent professional encoder queue was not established. |
| Multiple outputs per source | **Partial** | Protocol and UI support presets/output jobs, but shared decode/fan-out and full AME-style multiple-output orchestration are not established. `crates/protocol/src/lib.rs`, `crates/daemon/src/pipeline` |
| Presets and custom settings | **Partial** | Typed `EncodingPreset`, preset UI, and plan compilation exist. AME preset import/export/groups, complete codec settings, and broad format/device preset parity are not established. `crates/protocol/src/lib.rs`, `crates/app/src/preset_view.rs`, `crates/daemon/src/plan.rs` |
| Format and codec support | **Partial** | MediaCraft contains clean-room codec/container crates and pipeline adapters for a limited set of formats. It is not equivalent to AME's plug-in ecosystem or its broad supported import/export format lists. `Cargo.toml`, `crates/media/src`, codec/container crates |
| Export settings: video/audio, effects, captions, metadata, publish | **Partial** | MediaCraft exposes selected processing/encoding controls and metadata sidecars. Full AME export setting coverage, arbitrary effects/LUTs/overlays, captions/subtitles, publish tab, and format-specific settings are not established. `crates/app/src/process_view.rs`, `crates/app/src/main.rs`, `crates/protocol/src/lib.rs` |
| Hardware acceleration | **Partial** | Daemon hardware discovery exists, but discovery does not prove equivalent hardware encoding paths across codecs, formats, and settings. `crates/daemon/src/hardware.rs` |
| Watch folders | **Partial** | Watch-folder rules and automatic enqueueing exist. AME's multiple watch-folder outputs, stable-file semantics, full rule UI/configuration, and behavior under restart need verification. `crates/daemon/src/watch_folder.rs`, `crates/app/src/automate_view.rs` |
| Media Browser / source preview | **Partial** | MediaCraft has a catalog/library and inspector, but does not establish AME's project-aware Media Browser, favorites, and all preview/trim functions. `crates/app/src/library_view.rs`, `crates/app/src/inspector_view.rs` |
| Trimming source ranges | **Partial** | Media timeline/time structures exist, but equivalence to AME Export Settings In/Out trimming across formats is not established by the inspected app/pipeline paths. `crates/protocol/src/stream.rs`, `crates/app/src/process_view.rs` |
| Audio channelization and loudness | **Partial** | Audio DSP and loudness analysis exist; complete per-format channel mapping controls and AME automatic loudness correction workflow are not established. `crates/audio-dsp/src`, `crates/daemon/src/pipeline`, `crates/app/src/process_view.rs` |
| Delivery checks and receipts | **Partial** | Verification oracles and structured delivery reports/receipts exist. Tests/oracles do not by themselves establish that every desktop job runs a user-selected delivery contract before reporting success. `crates/oracles/src`, `crates/catalog/src/delivery_report.rs`, `crates/app/src/receipts.rs` |
| Publish/upload destinations | **No evidence** | No equivalent social-platform or FTP publishing workflow was found. |
| Adobe Premiere/After Effects/Team Projects dispatch | **Partial** | Craft IPC integrations exist, but Adobe application project dispatch and Adobe Team Projects support were not found. `crates/client/src`, `crates/protocol/src/lib.rs` |
| Logs, cache, preferences, keyboard shortcuts | **Partial** | Settings, logs, and caches exist in parts of the app; full AME preference surface, diagnostics UX, cache controls, and shortcut map are not established. `crates/app/src/settings.rs`, `crates/daemon/src`, `crates/catalog/src/scanner.rs` |
| Media Intelligence / AI analysis | **No evidence** | No equivalent content intelligence workflow was found; semantic discovery remains a future product direction. `docs/FEATURES.md` |

## Readout

MediaCraft already implements meaningful parts of both references: catalog and collections, metadata sidecars, batch rename, contact sheets, media inspection, presets, background queueing, watch-folder enqueueing, processing, and delivery/oracle infrastructure. Its strongest current alignment with Bridge is asset organization and with Media Encoder is local queued processing.

The largest visible parity gaps are comprehensive fast browsing/previews and metadata interoperability; AME's breadth of supported formats/settings, multiple-output behavior, publish destinations, and project integrations; and durable, auditable end-to-end delivery validation in the desktop workflow. Treat the gap statements as “not found in inspected source,” not proof that no prototype or external integration exists.

## Evidence and scope

This guide inspects the MediaCraft Rust workspace and compares it with [BRIDGE_CAPABILITY_REFERENCE.md](BRIDGE_CAPABILITY_REFERENCE.md) and [MEDIA_ENCODER_CAPABILITY_REFERENCE.md](MEDIA_ENCODER_CAPABILITY_REFERENCE.md). Paths identify implementation locations; they are not test receipts. No runtime tests were run for this guide, so code presence, UI affordances, and actual successful behavior remain separate claims. Revisit statuses when implementation or the Adobe source inventory changes.
