# craft-image

Still images for asset pipelines: everything a media catalogue, a CMS or a delivery worker needs
between "a file arrived" and "here are its renditions". Pure Rust, `#![forbid(unsafe_code)]`, no
panics on hostile input, builds for `wasm32-unknown-unknown`.

```rust
use craft_image::{OutputFormat, RenditionSpec, ThumbnailOptions, probe, render, thumbnail};

let info = probe(&bytes)?;                        // header + merged EXIF/XMP/IPTC + ICC, no pixels
let (w, h) = info.header.display_size();          // orientation applied
let caption = info.metadata.caption.as_deref();   // XMP, then IPTC, then EXIF
let thumb = thumbnail(&bytes, &ThumbnailOptions::new(512))?;          // sRGB, upright, RGBA8
let web = render(&bytes, &RenditionSpec::new(1280, 1280, OutputFormat::Jpeg { quality: 82 }))?;
```

## What it does

| Step | API | Notes |
|---|---|---|
| Detect | `sniff` | JPEG, PNG, GIF, WebP, TIFF/BigTIFF, BMP, PSD/PSB, HEIF, AVIF, JPEG XL, camera raw |
| Header | `read_header` | Size, depth, alpha, animation, orientation, without decoding pixels |
| Metadata blocks | `embedded` | EXIF, XMP, ICC (multi-chunk JPEG, compressed PNG) and IPTC, as raw bytes |
| Metadata | `Metadata::from_embedded`, `parse_exif`, `parse_xmp`, `parse_iptc` | Camera, lens, exposure, capture time (via `chrono`), GPS, title, caption, alt text, keywords, creators, rights, location |
| Probe | `probe`, `probe_file` | All of the above in one call; `probe_file` reads a 16 MiB prefix |
| Colour | `icc_info`, `Decoded::convert_to_srgb` | Profile description; conversion to sRGB through `craft-cms` |
| Decode | `decode` | RGBA8; DCT-scaled JPEG decode when a target size is given |
| Resize | `resize`, `resize_to_fit` | Linear light, premultiplied alpha, box prefilter then Lanczos-3 |
| Thumbnail | `thumbnail` | Embedded EXIF preview when big enough, else scaled decode |
| Rendition | `render` | Fit, sRGB, upright, encode, metadata policy |
| Encode | `encode` | JPEG, PNG, lossy WebP (`craft-vp8enc`) and lossless WebP; ICC and EXIF embedding |

### Formats

| Format | Header | Metadata | Decode | Encode |
|---|---|---|---|---|
| JPEG | yes | EXIF, XMP, ICC, IPTC | yes (DCT-scaled) | yes |
| PNG / APNG | yes | EXIF, XMP, ICC | yes (first frame) | yes |
| WebP | yes | EXIF, XMP, ICC | yes (first frame) | lossy (VP8, alpha, ICC, EXIF) and lossless |
| TIFF | yes | EXIF, XMP, ICC, IPTC | yes | no |
| GIF | yes | none | first frame | no |
| BMP | yes | none | yes | no |
| PSD / PSB | yes | EXIF, XMP, ICC, IPTC | merged composite via `craft-psd` | no |
| HEIC / HEIF | yes (`ispe`, `irot`, `imir`, `pixi`) | EXIF, XMP, ICC (items, `colr`) | yes, `craft-hevc`; tiled grids; 8/10-bit | no |
| AVIF | yes | EXIF, XMP, ICC | yes, `craft-av1`; 8/10/12-bit, alpha | no |
| Camera raw (DNG, CR2, CR3, NEF, ARW, RAF, ORF, RW2, …) | yes (CR3, RAF, ORF, RW2 from the preview) | EXIF | yes, `craft-raw`: the full-frame embedded preview, else developed (see below) | no |
| JPEG XL | detected only | no | no | no |

## Design

* **Untrusted input is the normal case.** Sizes are checked against `Limits` before any pixel
  buffer is allocated. Every offset read from a file is bounds-checked. IFD chains, box trees and
  XML nesting are bounded. Third-party decoders run behind a panic guard. `tests/robustness.rs`
  property-tests every entry point with random bytes, every prefix of real files, and damaged
  real files.
* **Correct pixels.** EXIF and HEIF orientation is modelled as the dihedral group, so it composes
  and inverts exactly. Resampling happens in linear light with premultiplied alpha, so fine detail
  keeps its brightness and transparent edges don't fringe. Colour converts to sRGB with relative
  colorimetric intent and black point compensation. That covers Display P3, Adobe RGB (including
  the EXIF-only "R03" convention), ProPhoto and others.
* **Cheap where it can be.** Header reads and `probe` decode no pixels. JPEG thumbnails decode at
  1/2, 1/4 or 1/8 scale. Colour conversion runs after downscaling. Large reductions take a box
  prefilter straight from the 8-bit source, so the float buffer stays near the output size.
* **Renditions are for publishing.** GPS, serial numbers and the EXIF thumbnail are never copied.
  `MetadataPolicy::Attribution` keeps only the creator and copyright; `Strip` keeps nothing.
  `Metadata::without_location` gives a public copy of the merged metadata.

## Shared tooling

* **`craft-cms`** (vendored from storytold/craft-libs into this workspace) handles ICC parsing and transforms, the same colour
  management module the Craft apps use.
* **`craft-psd`** (this workspace) provides PSD/PSB composites; **`craft-hevc`** and
  **`craft-av1`** (this workspace) decode HEIC and AVIF.
* **`craft_image::yuv`** converts Y′CbCr planes to RGBA for stills and for
  `mediacraft-preview`'s video frames alike.
* **`image`** does the raster codecs, and **`jpeg-decoder`** adds DCT-scaled JPEG decoding, which
  `image` does not expose.
* **`quick-xml`** reads XMP, **`chrono`** handles dates, and **`flate2`** handles PNG's
  compressed chunks.

Every dependency's licence is on the Craft licence allowlist (checked with `cargo tree`).

## Origin

The format sniffing, TIFF/IFD walking and thumbnail strategy are refined from LightCraft's
`lightcraft-codecs` and `lightcraft-meta`, at
[storytold/lightcraft@de09656](https://github.com/storytold/lightcraft/commit/de096566ff7a9607a2933c5135e79a83f555ecbf)
(MIT OR Apache-2.0). This crate keeps what asset pipelines need and leaves out the editor's linear
working space, HDR gain maps, proofing and print. It uses `craft-cms` where LightCraft bundles
`moxcms`.

## Camera raw

`DecodeOptions::raw` (and `RenditionSpec::raw`) choose how a raw becomes pixels:

* `RawMode::Preview` (the default) uses the camera's own embedded JPEG: the largest preview that
  shows the whole frame. Cameras also store crops, such as Sony's 16:9 preview of a 3:2 sensor,
  and those are skipped. When no full-frame preview exists, or the preview is smaller than the
  size asked for, the raw is developed.
* `RawMode::Develop` develops the sensor data with `craft-raw` into ProPhoto RGB, which then
  converts to sRGB like any tagged image. A file without colour calibration (most non-DNG
  raws) gets a colour matrix and tone curve fitted to the camera's own JPEG. A cropped preview is
  matched against the same crop of the sensor, and the full frame is developed.

## Not yet

* **HEIC alpha:** HEIF stores alpha as a monochrome (4:0:0) HEVC image, which `craft-hevc` does
  not decode yet; such images decode opaque, with correct colour. AVIF alpha is decoded.
* **JPEG XL:** headers and decoding.
* **Camera raw:** formats `craft-raw` cannot develop (Sony compressed RAW 2, compressed ORF and
  PEF, newer RW2) fall back to the embedded preview, which may be cropped or small.
* **Extended XMP:** JPEG packets split across several APP1 segments.
* **Encoding:** AVIF output; EXIF in lossless WebP.
* **Large images:** non-JPEG sources decode at full size before resizing. `Limits::max_pixels`
  bounds that memory.

## Try it on your own library

`examples/survey.rs` probes and thumbnails every path on stdin and prints only aggregates, never
per-file metadata:

```sh
find ~/Pictures -iname '*.jpg' | cargo run --release -p craft-image --example survey
```
