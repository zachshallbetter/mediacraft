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

| Where | Was | Now |
|---|---|---|
| Catalogue indexing (`catalog/src/av_probe.rs`) | `ffprobe` for every video and audio file | In-tree: `craft-isobmff`, `craft-matroska` and header parsers. Agrees with ffprobe on 299 local files (see the commit). No ffprobe call remains. |
| Still previews (`app/src/media_preview.rs`) | `image` crate, ffmpeg fallback | `craft-image`, HEIC and AVIF included. camera raw through `craft-raw`. ffmpeg only for JPEG XL and raws `craft-raw` cannot read. |
| Video poster frames | `ffmpeg` | `mediacraft-preview`: MP4/MOV, MKV/WebM, MPEG-TS/PS and MXF with H.264, HEVC, VP8, VP9, AV1, MPEG-1/2, ProRes, DNxHD/HR and MJPEG. ffmpeg only for MPEG-4 Part 2, JPEG 2000, DV and AVI. |
| Audio waveforms | `ffmpeg` | `mediacraft-preview` (WAV, AIFF, MP4/MKV PCM, AAC, Opus, Ogg Opus). ffmpeg only for FLAC, MP3, Vorbis, E-AC-3 and raw ADTS. |

The app runs without ffmpeg installed; files in the "only" columns then show no preview.

### Code that lived outside this repository

Done: `craft-ui`, `craft-quit`, `craft-settings`, `craft-atomic` and `craft-cms` are vendored
into `crates/` (see `docs/VENDORED.md`). `Cargo.lock` no longer references craft-libs.

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

1. ~~Replace `ffprobe` in the catalogue.~~ Done.
2. ~~Replace `ffmpeg` poster frames and waveforms for the codecs in this tree.~~ Done.
3. ~~Bring the FilmCraft decoders in.~~ Done: VP9, AV1, VP8, DNx, MPEG-2, MPEG-TS/PS and MXF
   (`docs/VENDORED.md`), wired into probing and poster frames.
4. ~~HEIF decode.~~ Done: HEIC through `craft-hevc` and AVIF through `craft-av1` in `craft-image`.
   Still to do: 4:0:0 HEVC for HEIC alpha.
5. Audio decoders for FLAC, MP3 and Vorbis, and ADTS framing for raw AAC files.
6. ~~Vendor the craft-libs Git dependencies.~~ Done; see `docs/VENDORED.md`.
