# `craft-media` naming and MediaCraft's independence

Decision record, 2026-11-07.

## The clash

MediaCraft's `crates/media` was published as `craft-media`. storytold/craft-libs publishes a
different crate under the same name: bit I/O plus ISO BMFF/MOV, Matroska/WebM and Ogg, extracted
from FilmCraft. FilmCraft, EffectCraft, SoundCraft and DeckCraft already depend on the upstream
one. Any consumer that depends on both MediaCraft's crates and craft-libs would see two unrelated
packages with one name. That includes a CMS that adopts MediaCraft's engine.

## Decision

1. **Rename.** `crates/media` is now `mediacraft-types`; the directory is unchanged. It holds
   MediaCraft's stream descriptors, buffer views, error taxonomy and rational time. Upstream
   keeps `craft-media`.
2. **Remove unused declarations.** Ten crates declared the old dependency, but only `craft-aac`
   uses it (`AudioBufferRef` in `encode_buffer`). The other nine (`flac`, `h264`, `h264enc`, `hevc`,
   `isobmff`, `opus`, `prores` and `daemon`) no longer declare it.
3. **Stay self-contained.** MediaCraft must run on its own, with every codec and service in its
   own tree. So it keeps its own `craft-bitstream`, `craft-isobmff` and `craft-matroska` and does
   not switch to upstream `craft-media`. The two trees share an origin (FilmCraft) and have
   diverged. They are compared below so that fixes can move in either direction by hand.

## Container crates compared

| Module | MediaCraft | Upstream `craft-media` |
|---|---|---|
| Bit I/O | `craft-bitstream`: 1,370 lines, 17 tests | `bitstream` feature: 979 lines, 25 tests |
| ISO BMFF / MOV | `craft-isobmff`: 7,193 lines, 43 tests, plus oracle and fuzz suites | `isobmff` feature: 5,281 lines, 39 tests |
| Matroska / WebM | `craft-matroska`: 5,793 lines, 38 tests | `matroska` feature: 4,581 lines, 40 tests |
| Ogg | in `craft-opus` | `ogg` feature: 1,471 lines, 26 tests |

The daemon pipelines (`crates/daemon/src/pipeline/*`) use MediaCraft's crates directly. Nothing
here depends on upstream `craft-media`.

## What independence still needs

### Processes MediaCraft starts

| Where | Runs | For | In-tree replacement |
|---|---|---|---|
| `catalog/src/scanner.rs` (`probe_media_streams`) | `ffprobe` | Video and audio stream info when indexing | `craft-isobmff` and `craft-matroska` demuxers plus codec config parsing |
| `app/src/media_preview.rs` | `ffmpeg` | Video poster frames | `craft-isobmff`/`craft-matroska` demux plus `craft-h264`/`craft-hevc`/`craft-prores` decode |
| `app/src/media_preview.rs` | `ffmpeg` | HEIC, AVIF and camera raw stills | HEIF decode (LightCraft/PhotoCraft `heif`) and a raw developer (LightCraft/PhotoCraft `raw`) |
| `app/src/media_preview.rs` | `ffmpeg` | Audio waveforms | `craft-aac`, `craft-flac`, `craft-opus` and WAV decode plus peak extraction |

Still images no longer need ffmpeg on the `feat/craft-image` branch, except HEIC, AVIF and raw.

### Code that lives outside this repository

| Dependency | Source | Used by |
|---|---|---|
| `craft-ui`, `craft-quit`, `craft-settings`, `craft-atomic` | craft-libs @ `666c358`, a Git dependency | app, daemon, catalog |
| `craft-cms` | craft-libs @ `904f55b`, a Git dependency (`feat/craft-image` branch) | `craft-image` (ICC to sRGB) |

These build without network access once fetched and pinned. They are still code MediaCraft does
not own.

### Codecs in the suite that MediaCraft lacks

| Codec or container | Where it lives | Size |
|---|---|---|
| AV1 decode | `filmcraft/crates/av1` | 21.7k lines |
| VP9 decode | `filmcraft/crates/vp9` | 9.4k lines |
| VP8 decode | `filmcraft/crates/vp8` | 3.3k lines |
| DNxHD/DNxHR | `filmcraft/crates/dnx` | 5.3k lines |
| MPEG-2 video | `filmcraft/crates/mpeg2v` | 4.7k lines |
| MPEG-TS | `filmcraft/crates/mpegts` | 3.0k lines |
| MXF | `filmcraft/crates/mxf` | 4.7k lines |
| AV1, HEVC, VP9 and Opus encoders | `effectcraft/crates/{av1enc,hevcenc,vp9enc,opusenc}` | not sized |
| HEIF still decode | `lightcraft/crates/heif`, `photocraft/crates/heif` | 2.4k lines (LightCraft) |
| Camera raw | `lightcraft/crates/raw`, `photocraft/crates/raw` | not sized |
| AIFF, CAF and WAV I/O, FLAC encode | craft-libs `audio` | 7.1k lines |
| 12-bit and arithmetic-coded JPEG | craft-libs `jpeg` | 2.2k lines |
| PDF page rendering | craft-libs `pdf` (`raster`) | 0.7k lines of glue |

The FilmCraft decoders depend only on `filmcraft-bitstream` or nothing, so they can come over
against `craft-bitstream` with few changes.

## Order of work

1. Replace `ffprobe` in the catalogue with in-tree probing. Indexing is the path every asset
   takes, and the demuxers are already here.
2. Replace `ffmpeg` poster frames with in-tree demux and decode (H.264, HEVC, ProRes first).
3. Bring the FilmCraft decoders in (VP9, AV1, VP8, then MXF, MPEG-TS, MPEG-2 and DNx), each as
   its own crate with its tests and an origin note.
4. Bring HEIF decode in, so iPhone photos need no ffmpeg.
5. Decide whether the craft-libs Git dependencies are acceptable for "independent" or should be
   vendored into this tree.
