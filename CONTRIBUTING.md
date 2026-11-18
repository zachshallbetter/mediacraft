# Contributing to MediaCraft

Thanks for your interest. This guide covers setup, the conventions the workspace holds
itself to, and how to verify a change before opening a PR.

## Setup

MediaCraft is a Cargo workspace (Rust edition 2024, MSRV 1.95, resolver 3). The app and some
crates depend on `craft-ui`, `craft-quit`, `craft-settings` and `craft-atomic` from
`https://github.com/storytold/craft-libs`, pinned to a revision in the root `Cargo.toml`,
so the first build needs network access to that repository.

```sh
cargo build --workspace
cargo test --workspace
cargo run -p mediacraft-cli -- oracle-test   # AV sync and priming-delay oracle
```

The workspace layout is in the [README](README.md). The `docs/` directory holds the
contract, definition, interface and media-infrastructure specifications.

## Conventions

- `#![forbid(unsafe_code)]` in every crate (also set as a workspace lint).
- No `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` or `unreachable!` in shipped
  code (the README lists the Clippy deny set).
- Treat user files and IPC messages as hostile: bounded allocation, strict path containment.
- Media, document and packaging crates must compile for `wasm32-unknown-unknown`.
- Do not describe a capability as implemented, tested or validated without the source and
  tests to back it (the README's claim-integrity rule).
- Commit messages mostly use conventional prefixes, for example `feat(app): ...` and
  `docs: ...`. Use that form.

## Verifying a change

Run all of these before opening a PR:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo check --target wasm32-unknown-unknown -p craft-pdf -p craft-archive -p craft-psd -p craft-matroska -p craft-isobmff -p craft-aac -p craft-flac -p craft-opus
```

The repository has no CI workflow, so these local checks are the only gate.

## Pull requests

- Branch off `main`; keep PRs focused.
- Describe the change and how you verified it. The PR template prompts for this.
- Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).
