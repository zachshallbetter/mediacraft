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

### Ecosystem Testbed & Staging Ground

MediaCraft serves as the primary empirical testbed and staging harness for capabilities proposed for `storytold/craft-libs`. Rather than designing shared abstractions in the abstract, components are first implemented, stress-tested, and proven here in accordance with authoritative suite specifications:
- [`CRAFT_UI_SCHEMA.md`](../CRAFT_UI_SCHEMA.md): Presentation widget contracts, docking layout, dark theme tokens, and canvas controls tested live in `mediacraft-app`.
- [`CRAFT_MEDIA_RFC.md`](../CRAFT_MEDIA_RFC.md): Clean-room media codecs, bitstream I/O, audio DSP, and stream-copy remuxing proven against ffmpeg verification oracles.
- [`CRAFT_LIBS_RFC.md`](../CRAFT_LIBS_RFC.md): Shared library architecture, atomic persistence (`craft-atomic`), configuration self-healing (`craft-settings`), lifecycle termination (`craft-quit`), ISO 32000 PDF parsing (`craft-pdf`), and safe packaging (`craft-archive`).

---

## 2. Workspace Topology

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
│       └── psd/                (craft-psd: Adobe Photoshop PSD/PSB layered raster image decoder)
└── docs/
    ├── INTERFACE.md            (UI Screen Map: 5 workspaces, 20 screens, 14 panels)
    ├── MEDIA_INFRASTRUCTURE_SPEC.md (Shared media library architecture)
    ├── CONTRACT.md             (Product & system contract, boundaries, invariants)
    ├── DEFINITION.md           (Product definition and lifecycle model)
    ├── FEATURES.md             (Comprehensive feature & capability inventory)
    └── LAB_NOTES.md            (Exploration logs, empirical receipts, and findings)
```

---

## 3. Strict Non-Negotiables

All crates in this workspace adhere to the authoritative Craft shared library standards:
- `#![forbid(unsafe_code)]` enforced across every crate.
- `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]` on all shipped code.
- Clean compilation for `wasm32-unknown-unknown` on all media, document, and packaging crates.
- Clean formatting (`cargo fmt --check`) and zero warnings (`cargo clippy --all-targets -- -D warnings`).
- Defensive input handling: All user files and IPC messages are treated as hostile, with bounded allocation budgets and strict path containment.

---

## 4. Verification Commands

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
