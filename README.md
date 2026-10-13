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

## 2. Workspace Topology

```text
mediacraft/
├── Cargo.toml                  (Workspace root; MSRV 1.95, Edition 2024, forbid unsafe)
├── crates/
│   ├── bitstream/              (Pure bit I/O, MSB/LSB readers/writers, Exp-Golomb, RBSP)
│   ├── protocol/               (Typed IPC messages, job descriptors, encoding presets)
│   ├── daemon/                 (Priority transcode queue, state machine, playback hook)
│   ├── oracles/                (Synthetic AV sync generator, 1024-sample priming oracle)
│   ├── cli/                    (Headless CLI binary: status, add, pause, oracle-test)
│   └── app/                    (Dockable desktop UI using craft-ui docking & craft-quit)
└── docs/
    ├── INTERFACE.md            (UI Screen Map: 5 workspaces, 20 screens, 14 panels)
    ├── MEDIA_INFRASTRUCTURE_SPEC.md (5-layer shared library architecture)
    ├── CONTRACT.md             (Product & system contract, boundaries, invariants)
    ├── DEFINITION.md           (Product definition and lifecycle model)
    ├── FEATURES.md             (Comprehensive feature & capability inventory)
    └── LAB_NOTES.md            (Exploration logs, empirical receipts, and findings)
```

---

## 3. Strict Non-Negotiables

All crates in this workspace adhere to the authoritative Craft shared library standards:
- `#![forbid(unsafe_code)]` enforced across all crates.
- `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]` on shipped code.
- Clean compilation for `wasm32-unknown-unknown` on leaf crates (`bitstream`, `protocol`, `oracles`).
- Clean formatting (`cargo fmt --check`) and zero warnings (`cargo clippy --all-targets -- -D warnings`).

---

## 4. Verification Commands

```bash
# Run all unit and property tests across workspace
cargo test

# Run clippy quality gate
cargo clippy --all-targets -- -D warnings

# Verify formatting
cargo fmt --check

# Verify WebAssembly target compilation
cargo check --target wasm32-unknown-unknown -p mediacraft-bitstream -p mediacraft-protocol -p mediacraft-oracles

# Run AV sync and priming delay verification oracle
cargo run -p mediacraft-cli -- oracle-test
```
