# MediaCraft: Professional Media Workspace & Encoding Pipeline

> **Author:** Zach Shallbetter  
> **Status:** Independent Media Application Workspace  
> **Purpose:** Clean-room implementation, media asset workspace, and shared media infrastructure for the Craft creative ecosystem.

---

## 1. Scope & System Boundary

MediaCraft is a filesystem-native desktop application and background service for discovering, organizing, inspecting, transforming, validating, and delivering media assets across the Craft ecosystem.

It provides:

1. The **Asset Engine & Digital Asset Management** (`mediacraft-catalog`): Fast visual browsing, smart collections, metadata facets, and derivative relationship tracking.
2. The **Background Transcode Queue & Concurrency Engine** (`mediacraft-daemon`): Multi-source batch execution, watch folder automation, and priority scheduling.
3. The **IPC Protocol & Remote Client Commands** (`mediacraft-protocol`): Typed socket communication allowing FilmCraft and EffectCraft to dispatch background encoding jobs without blocking timeline playback.
4. The **Synthetic AV Sync & Priming Delay Verification Oracles** (`mediacraft-oracles`): Mathematical proofs for edit list (`elst`) synchronization, AAC 1024-sample priming compensation, and faststart progressive multiplexing.
5. The **Desktop Workspace UI** (`mediacraft-app`): A dark-mode desktop interface built with `craft-ui`, `craft-settings`, `craft-atomic`, and `craft-quit`.
6. A headless testing CLI (`mediacraft-cli`).

---

## 2. Empirical Testbed & Staging Ground for `craft-libs`

MediaCraft is the primary empirical testbed and staging harness for architectural primitives, format handlers, and presentation widgets being developed for the shared `storytold/craft-libs` ecosystem.

Rather than designing shared library abstractions in isolation, components are first implemented, stress-tested, and empirically verified inside MediaCraft under real multi-gigabyte production workloads before being extracted upstream:

### 1. Shared UI Contracts & Presentation Engine ([`CRAFT_UI_SCHEMA.md`](../CRAFT_UI_SCHEMA.md))

- **Active Reference Consumer**: `mediacraft-app` acts as the reference workbench and testbed for `craft-ui` across its 5 primary workspaces (Catalog, Inspector, Queue, Automations, Settings).
- **Component Qualification**: Empirically validates presentation widgets, dockable panel hierarchies, tabbed workspaces, facet filter chips, canvas controls, and rich procedural thumbnails without causing event loop latency or window dormancy.
- **Service Decoupling**: Proves seamless integration with `craft-quit` for non-blocking shutdown confirmation, `craft-settings` for schema-validated configuration storage, and `craft-atomic` for crash-safe state persistence.

### 2. Clean-Room Media Infrastructure ([`CRAFT_MEDIA_RFC.md`](../CRAFT_MEDIA_RFC.md))

- **5-Layer Media Stack**: Implements and stages the complete clean-room media pipeline proposed in `CRAFT_MEDIA_RFC.md`:
  - *Bitstream Primitives*: Pure-Rust MSB/LSB bit readers/writers, Exp-Golomb, and RBSP handling (`craft-bitstream`).
  - *Audio Infrastructure*: AAC-LC encoder/decoder (`craft-aac`), lossless FLAC (`craft-flac`), and Opus (`craft-opus`).
  - *Audio DSP & Acoustics*: EBU R128 loudness, true-peak meter, and cross-correlation delay sync (`craft-audio-dsp`).
  - *Video Codecs*: Intra/inter H.264/AVC decoder (`craft-h264`), multi-pass rate-controlled encoder (`craft-h264enc`), HEVC/H.265 (`craft-hevc`), and Apple ProRes 422/4444 (`craft-prores`).
  - *Multiplexing & Remuxing*: ISO Base Media File Format MP4/MOV faststart muxer (`craft-isobmff`), Matroska MKV/WebM parser (`craft-matroska`), and stream-copy routing facade (`craft-media`).
- **Empirical Oracles**: Codec implementations are verified against rigorous mathematical and ffmpeg/ffprobe verification oracles (`mediacraft-oracles`), proving bit-exactness, zero panics, and conformance.

### 3. Shared Library Architecture & Document Packaging ([`CRAFT_LIBS_RFC.md`](../CRAFT_LIBS_RFC.md))

- **System Contracts**: Proves adherence to `craft-libs` core non-negotiables: crash-safe atomic write ordering (`sync_file < rename < sync_dir` in `craft-atomic`), self-healing configuration backups in `craft-settings`, and signed 64-bit media time in `craft-time`.
- **Document & Archive Staging**: Houses the initial staging and consumer testing for:
  - *`craft-pdf`*: Pure-Rust ISO 32000 COS object graph, stream filters (Flate, LZW, ASCII85), crypt security handlers (RC4, AES-128/256), and metadata inspection.
  - *`craft-archive`*: Safe multi-format unarchiver (ZIP, TAR, TAR.GZ) with `ResourceBudget` decompression limits, path containment, and Zip Slip traversal prevention.
  - *`craft-psd`*: Layered raster image decoder for multi-app asset ingestion.

### 4. Creative File Format & Conversion Reference ([`FORMAT_COMPATIBILITY_REFERENCE.md`](../FORMAT_COMPATIBILITY_REFERENCE.md))

- **Four Creative Document Paradigms**: Directly aligns MediaCraft cataloging, thumbnail extraction, and transcode services with the suite-wide compatibility matrix covering Raster (`.psd`/`.psb`), Vector (`.ai`/`.svg`/`.pdf`), Publishing (`.indd`/`.idml`), and Animation (`.fla`/`.xfl`/`.swf`).
- **Four-Tier Claim Integrity**: Adheres to the strict claim governance ($\text{Documented} \neq \text{Implemented} \neq \text{Tested} \neq \text{Empirically Validated}$) across all media decoders and muxers.

### 5. Testing, Interaction, Validation & Fallback Standards ([`CRAFT_METHODOLOGY.md`](../CRAFT_METHODOLOGY.md))

- **Empirical Testing**: Employs in-tree synthetic stream generators, property-based fuzzing (`proptest`), hostile boundary evaluation suites (`tests/evals.rs`), and differential verification oracles (`mediacraft-oracles`, `ffmpeg`, `ffprobe`).
- **Autonomous Interaction**: Enforces headless-first execution (`mediacraft-cli`), in-process agent MCP server integration, loopback TCP control (`control.rs`), and live window framebuffer capture (`ui.screenshot`).
- **Rigorous Validation**: Integrates cross-application compatibility runners (`cargo xtask compat`), pre-mutation diagnostic reports (`*DiagnosticReport`), and pure-Rust WebAssembly verification (`wasm32-unknown-unknown`).
- **Safe Fallback Policies**: Implements causal timeline correction (backwards DTS clamping, PTS $\ge$ DTS enforcement), audio gap silence concealment, and atomic persistence fallbacks (`move_aside` on broken files with `.bak` recovery).

---

## 3. Workspace Topology

MediaCraft is organized as an integrated cargo workspace containing 22 high-performance, `#![forbid(unsafe_code)]` member crates:

```text
mediacraft/
├── Cargo.toml                  (Workspace root; MSRV 1.95, Edition 2024, forbid unsafe)
├── crates/
│   ├── Application & Service Layer:
│   │   ├── app/                (Desktop workspace UI using craft-ui docking, theme & craft-quit)
│   │   ├── catalog/            (Digital Asset Management engine: audio, video, image, doc, archive facets)
│   │   ├── daemon/             (Priority transcode queue, state machine, playback hook)
│   │   ├── client/             (High-level async client library connecting over domain sockets)
│   │   ├── protocol/           (Typed IPC messages, job descriptors, encoding presets)
│   │   ├── cli/                (Headless CLI binary: status, add, pause, oracle-test)
│   │   └── oracles/            (Synthetic AV sync generator, 1024-sample priming oracle)
│   │
│   ├── Clean-Room Audio Infrastructure (Staged for craft-libs):
│   │   ├── aac/                (MPEG-4 AAC-LC encoder & decoder, ADTS parsing, psychoacoustics)
│   │   ├── flac/               (Lossless FLAC encoder & decoder, LPC prediction, Rice coding)
│   │   ├── opus/               (Opus audio decoder, Ogg demuxing, SILK/CELT mode parsing)
│   │   └── audio-dsp/          (EBU R128 loudness, true-peak, cross-correlation, dynamic range)
│   │
│   ├── Clean-Room Video Infrastructure (Staged for craft-libs):
│   │   ├── h264/               (H.264 / AVC video decoder, CAVLC/CABAC entropy, deblocking)
│   │   ├── h264enc/            (H.264 / AVC video encoder, rate control, multi-pass VBR)
│   │   ├── hevc/               (H.265 / HEVC video encoder & decoder, SAO, wavefront processing)
│   │   └── prores/             (Apple ProRes 422/4444 decoder, frame headers, DCT reconstruction)
│   │
│   ├── Container & Bitstream Primitives (Staged for craft-libs):
│   │   ├── bitstream/          (Pure bit I/O, MSB/LSB readers/writers, Exp-Golomb, RBSP)
│   │   ├── isobmff/            (ISO Base Media File Format parser/muxer: MP4, MOV, faststart)
│   │   ├── matroska/           (Matroska / WebM container parser: EBML header, Cluster parsing)
│   │   └── media/              (Stream-copy remuxing facade and media routing pipeline)
│   │
│   └── Documents, Layered Images & Safe Packaging (Staged for craft-libs):
│       ├── pdf/                (craft-pdf: Clean-room ISO 32000 PDF COS parser, filters, crypt, content)
│       ├── archive/            (craft-archive: Safe ZIP/TAR inspection, Zip Slip defense, ResourceBudget)
│       └── psd/                (craft-psd: PSD/PSB layered raster image decoder)
└── docs/
    ├── INTERFACE.md            (UI Screen Map: 5 workspaces, 20 screens, 14 panels)
    ├── MEDIA_INFRASTRUCTURE_SPEC.md (Shared media library architecture)
    ├── CONTRACT.md             (Product & system contract, boundaries, invariants)
    ├── DEFINITION.md           (Product definition and lifecycle model)
    ├── FEATURES.md             (Comprehensive feature & capability inventory)
    └── LAB_NOTES.md            (Exploration logs, empirical receipts, and findings)
```

---

## 4. Strict Non-Negotiables

All crates in this workspace adhere to the authoritative Craft shared library standards:

- `#![forbid(unsafe_code)]` enforced across every crate.
- `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]` on all shipped code.
- Clean compilation for `wasm32-unknown-unknown` on all media, document, and packaging crates.
- Clean formatting (`cargo fmt --check`) and zero warnings (`cargo clippy --all-targets -- -D warnings`).
- Defensive input handling: All user files and IPC messages are treated as hostile, with bounded allocation budgets and strict path containment.

---

## 5. Verification Commands

```bash
# Run all unit and property tests across workspace
cargo test --workspace

# Run clippy quality gate
cargo clippy --workspace --all-targets -- -D warnings

# Verify formatting
cargo fmt --check

# Verify WebAssembly target compilation on staged library crates
cargo check --target wasm32-unknown-unknown -p craft-pdf -p craft-archive -p craft-psd -p craft-matroska -p craft-isobmff -p craft-aac -p craft-flac -p craft-opus

# Run AV sync and priming delay verification oracle
cargo run -p mediacraft-cli -- oracle-test
```
