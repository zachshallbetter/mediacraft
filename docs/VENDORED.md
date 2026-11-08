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

| `craft-av1` | `crates/av1` | storytold/filmcraft `crates/av1` (`filmcraft-av1`) | `4d571c2a666cace681209a1f617f68cd9cefefed` | mediacraft-preview |
| `craft-vp9` | `crates/vp9` | storytold/filmcraft `crates/vp9` | same | mediacraft-preview |
| `craft-vp8` | `crates/vp8` | storytold/filmcraft `crates/vp8` | same | mediacraft-preview |
| `craft-dnx` | `crates/dnx` | storytold/filmcraft `crates/dnx` | same | mediacraft-preview |
| `craft-mpeg2v` | `crates/mpeg2v` | storytold/filmcraft `crates/mpeg2v` | same | mediacraft-preview, catalog |
| `craft-mpegts` | `crates/mpegts` | storytold/filmcraft `crates/mpegts` | same | mediacraft-preview, catalog |
| `craft-mxf` | `crates/mxf` | storytold/filmcraft `crates/mxf` | same | mediacraft-preview, catalog |
| `mediacraft-testkit` | `crates/testkit` | storytold/filmcraft `crates/testkit` (dev-dependency only) | same | the seven above |
| `craft-vp8enc` | `crates/vp8enc` | storytold/photocraft `src/codecs/vp8.rs` and `src/codecs/vp8/` | `3aa1d9f43` | craft-image |

The first four craft-libs crates are the commit MediaCraft was already pinned to. `craft-cms` is from craft-libs
`main` on 2026-11-07.

## Local changes

**PhotoCraft VP8 encoder.** PhotoCraft's `codecs::vp8` module becomes the crate root of
`craft-vp8enc`: `vp8.rs` is `src/lib.rs` and its submodules sit beside it unchanged. The only
additions are the crate-root lint attributes MediaCraft's other crates carry and a `rustfmt.toml`
with PhotoCraft's formatting (`max_width = 160`).
One fix is made in place: the B_PRED trial in `encode_macroblock` saved and restored the whole
reconstructed luma plane for every macroblock, making encoding quadratic in the image size (a
2560 px rendition took 12 to 16 seconds). It now saves only the macroblock's 16×16 block. The output
is byte-identical, and `encode_time_is_linear_in_the_pixel_count` guards it. This should go
upstream to PhotoCraft.

**FilmCraft crates.** These are renamed `filmcraft-<x>` → `craft-<x>`, matching MediaCraft's
other codec crates, and `filmcraft-testkit` → `mediacraft-testkit`. Their two internal
dependencies are repointed: `filmcraft-bitstream` → `mediacraft-bitstream`, which carries the same
`BitReader`, and the testkit. References in sources and docs are rewritten to match. A
`rustfmt.toml` with FilmCraft's formatting (`max_width = 160`) is added. Nothing else changes.
The root `Cargo.toml` builds the five decoders at `opt-level = 3` in dev profiles, as FilmCraft
does, because their spec-vector tests are minutes slower without it.

**craft-libs crates.**

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

For the FilmCraft crates, `git archive` them from filmcraft at the new commit and apply the same
renames: package names, `filmcraft_`/`filmcraft-` references, and the bitstream and testkit
dependencies. Then diff against `crates/<x>`. For craft-libs:

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
