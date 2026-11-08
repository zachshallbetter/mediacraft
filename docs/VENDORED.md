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
| `craft-ac3` | `crates/ac3` | storytold/filmcraft `crates/ac3` (`filmcraft-ac3`) | `4a56be9cff419a5a9b16a25259c97dc823ca8bae` | mediacraft-preview |
| `craft-avi` | `crates/avi` | storytold/filmcraft `crates/avi` (`filmcraft-avi`) | `4a56be9cff419a5a9b16a25259c97dc823ca8bae` | mediacraft-preview, catalog |
| `craft-time` | `crates/time` | storytold/filmcraft `crates/time` (`filmcraft-time`) | `4a56be9cff419a5a9b16a25259c97dc823ca8bae` | craft-captions |
| `craft-captions` | `crates/captions` | storytold/filmcraft `crates/captions` (`filmcraft-captions`), with local changes below | `4a56be9cff419a5a9b16a25259c97dc823ca8bae` | nothing yet |
| `mediacraft-testkit` | `crates/testkit` | storytold/filmcraft `crates/testkit` (dev-dependency only) | `4d571c2a666cace681209a1f617f68cd9cefefed` | the decoders and demuxers above |
| `craft-raw` | `crates/raw` | storytold/photocraft `crates/raw` (`photocraft-raw`), without `fuzz/` | `3aa1d9f43` | craft-image |
| `craft-vp8enc` | `crates/vp8enc` | storytold/photocraft `src/codecs/vp8.rs` and `src/codecs/vp8/` | `3aa1d9f43` | craft-image |
| `craft-develop` | `crates/develop` | storytold/lightcraft `crates/develop` (`lightcraft-develop`), with local changes below | `b3b49987d` | nothing yet |
| `craft-develop-geom` | `crates/develop-geom` | storytold/lightcraft `crates/geom` (`lightcraft-geom`) | `b3b49987d` | craft-develop, craft-develop-color |
| `craft-develop-color` | `crates/develop-color` | storytold/lightcraft `crates/color` (`lightcraft-color`) | `b3b49987d` | craft-develop |

The first four craft-libs crates are the commit MediaCraft was already pinned to. `craft-cms` is from craft-libs
`main` on 2026-11-07.

## Local changes

**PhotoCraft raw crate.** `photocraft-raw` is renamed `craft-raw` (package name and
`photocraft_raw` references only) and gets PhotoCraft's `rustfmt.toml`. Its `fuzz/` directory is
left out. Its opt-in corpus tests look for `../../corpus/pixls`, which MediaCraft does not carry.

**PhotoCraft VP8 encoder.** PhotoCraft's `codecs::vp8` module becomes the crate root of
`craft-vp8enc`: `vp8.rs` is `src/lib.rs` and its submodules sit beside it unchanged. The only
additions are the crate-root lint attributes MediaCraft's other crates carry and a `rustfmt.toml`
with PhotoCraft's formatting (`max_width = 160`).
One fix is made in place: the B_PRED trial in `encode_macroblock` saved and restored the whole
reconstructed luma plane for every macroblock, making encoding quadratic in the image size (a
2560 px rendition took 12 to 16 seconds). It now saves only the macroblock's 16×16 block. The output
is byte-identical, and `encode_time_is_linear_in_the_pixel_count` guards it. This should go
upstream to PhotoCraft.

**LightCraft develop crates.** `lightcraft-develop` (`DevelopSettings`, the non-destructive edit model,
with its control specs, presets and segmentation masks), `lightcraft-geom` and `lightcraft-color` are
vendored from LightCraft `origin/main` at `b3b49987d` as `craft-develop`, `craft-develop-geom` and
`craft-develop-color`. The `develop-` prefix on the two small crates keeps the generic names
`craft-geom` and `craft-color` free for craft-libs. Only rendering is out of scope here: the pipeline
and LightCraft's raw decoder are not vendored. The changes:

- **Renames:** the three package names, the two internal dependencies, and the `lightcraft_geom` and
  `lightcraft_color` paths in the sources (`craft_develop_geom`, `craft_develop_color`). Comments that
  name LightCraft crates that are not vendored (`lightcraft_pipeline`, `lightcraft_raw`,
  `lightcraft-segment`) are left as they are.
- **`rustfmt.toml`:** LightCraft's own (`max_width = 150`, `use_small_heuristics = "Max"`) in each crate.
- **`Cargo.toml` of each crate:** `[lints] workspace = true` becomes LightCraft's own lint table
  (`unsafe_code = "deny"`, `cognitive_complexity`, `too_many_arguments` and `wildcard_imports` denied),
  because MediaCraft's workspace lints are looser on those. `serde` takes the `rc` feature and `serde_json`
  the `float_roundtrip` feature, as in LightCraft's workspace; the latter keeps parsed floats exact, which
  canonical bytes rely on. `rust-version` is inherited, as in the other crates.
- **Workspace:** the three crates are members and path dependencies, and `miniz_oxide = "0.9"` (MIT,
  Zlib or Apache-2.0) is a workspace dependency, the version LightCraft uses.
- **`crates/develop/src/canonical.rs`** (new) and two lines in `src/lib.rs` (`pub mod canonical;` and its
  re-exports). Hermit stores settings as RFC 8785 (JCS) bytes. `validate_canonical` rejects a NaN or
  infinity, an integer beyond 2^53, and a map whose keys are not in a stable order; it checks the serialisation
  itself, because `serde_json::to_value` turns a NaN into `null`. `to_canonical_json` validates and writes the
  JCS bytes with a small clean-room writer (keys sorted by UTF-16 code units, ECMAScript number format, minimal
  string escapes; no extra dependency), tested against the RFC's examples. LightCraft's JSON omits `process`
  at V1; the canonical bytes always state it, and `version`. Nothing else in the vendored files changes.

**FilmCraft crates.** These are renamed `filmcraft-<x>` → `craft-<x>`, matching MediaCraft's
other codec crates, and `filmcraft-testkit` → `mediacraft-testkit`. Their two internal
dependencies are repointed: `filmcraft-bitstream` → `mediacraft-bitstream`, which carries the same
`BitReader` (used by `craft-ac3`), and the testkit. References in sources and docs are rewritten to
match. A `rustfmt.toml` with FilmCraft's formatting (`max_width = 160`) is added. Nothing else changes.
The root `Cargo.toml` builds the five decoders at `opt-level = 3` in dev profiles, as FilmCraft
does, because their spec-vector tests are minutes slower without it.

**FilmCraft captions crate.** `filmcraft-captions` (SRT, WebVTT, SCC, MCC, EBU STL, TTML/IMSC1/DFXP
and CEA-608) is vendored at `4a56be9c` as `craft-captions` with these changes, because FilmCraft's
project, text and colour crates are not vendored:

- `src/burn.rs` (caption burn-in) is dropped, with its `filmcraft-project`, `filmcraft-text` and
  `filmcraft-color` dependencies and the burn-in section of the README.
- The parts of `src/lib.rs` that convert to and from `filmcraft_project` types are dropped:
  `track_from_document`, `document_from_track` and `Format::track_format` (the types `Caption`,
  `CaptionTrack`, `ClipId`, `TrackId` and `CaptionFormat`). `Document` and `Cue` are unchanged.
- `filmcraft_project::plain_text` is copied into `src/plain_text.rs` (from filmcraft
  `crates/project/src/caption.rs` at the same commit, with its two tests) and re-exported as
  `craft_captions::plain_text`. Its three callers (`lib.rs`, `scc.rs`, `mcc.rs`) use it.
- The test `ttml_and_dfxp_write_the_track_language` is removed: it goes through the caption track.
  The rest of `tests/it/ttml_language.rs` and all other tests are kept.
- The package description no longer mentions burn-in. `roxmltree` (MIT or Apache-2.0) is added as a
  workspace dependency at the version FilmCraft uses.

**FilmCraft AC-3, AVI and time crates.** `craft-avi` and `craft-time` get only the renames and
`rustfmt.toml` described above. `craft-ac3` also gets a `dev` profile at `opt-level = 3`, and one
code change: `Bits::read` in `src/lib.rs`. FilmCraft's `filmcraft-bitstream` re-exports craft-libs'
`BitReader`, whose `peek` accepts widths above 32 bits (returning the low 32 bits of the field);
`mediacraft-bitstream`'s accepts at most 32 and its debug build asserts. `Bits::read` now steps over
the bits above the last 32 before peeking, which gives the same values, and the test
`bits_match_the_old_bit_by_bit_reader` (widths up to 40) passes unchanged.

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
