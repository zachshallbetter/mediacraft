# Vendored crates

MediaCraft builds and runs from this repository alone. The craft-libs crates it uses are copied
into `crates/` rather than fetched as Git dependencies.

| Crate | Path | Source | Commit | Used by |
|---|---|---|---|---|
| `craft-atomic` | `crates/atomic` | storytold/craft-libs `crates/atomic` | `666c358065261a2816b376fa3bb10538dbf53040` | daemon, catalog, settings, app |
| `craft-settings` | `crates/settings` | storytold/craft-libs `crates/settings` | `666c358065261a2816b376fa3bb10538dbf53040` | app |
| `craft-quit` | `crates/quit` | storytold/craft-libs `crates/quit` | `666c358065261a2816b376fa3bb10538dbf53040` | app |
| `craft-ui` | `crates/ui` | storytold/craft-libs `crates/ui` | `666c358065261a2816b376fa3bb10538dbf53040` | app |
| `craft-cms` | `crates/cms` | storytold/craft-libs `crates/cms` | `904f55bb210c5a0570051204ebd210043d8e50ea` | craft-image |

The first four are the commit MediaCraft was already pinned to. `craft-cms` is from craft-libs
`main` on 2026-11-07.

## Local changes

Each crate is byte-identical to its source commit, with one addition: a `rustfmt.toml` with
craft-libs' formatting (`max_width = 150`). That lets `cargo fmt --all --check` pass without
reformatting the crates, and keeps them easy to diff against upstream.

The workspace changes that make them build here are:

- **Member paths:** the root `Cargo.toml` lists the five crates as members and as path
  dependencies.
- **`repository` key:** `[workspace.package]` gained `repository`, which the crates inherit.
- **`clippy.toml`:** it allows `unwrap`, `expect` and `panic` in tests only, as craft-libs does.

Licences and third-party notices (two CC0 ICC profiles, Fogra's FOGRA39 data) are in `NOTICE`.

## Updating from upstream

```sh
REV=<craft-libs commit>
cd path/to/craft-libs
for c in atomic settings quit ui cms; do
  git archive "$REV" "crates/$c" | tar -x -C /tmp/craft-libs-$REV
done
diff -r /tmp/craft-libs-$REV/crates/ui path/to/mediacraft/crates/ui   # review, then copy over
```

After copying, run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
warnings` and `cargo fmt --all --check`, and update the commit in the table above. The four UI
crates are about 40k lines behind craft-libs `main`. Moving them forward is its own change,
tested against `mediacraft-app`.
