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
| Detect | `sniff` | JPEG, PNG, GIF, WebP, TIFF/BigTIFF, BMP, PSD/PSB, HEIF, AVIF, JPEG XL, OpenEXR, Radiance HDR, DDS, ICO/CUR, PNM, QOI, TGA, DPX, PCX, FITS, DICOM, SVG/SVGZ, camera raw |
| Header | `read_header` | Size, depth, alpha, animation, orientation, without decoding pixels |
| Metadata blocks | `embedded` | EXIF, XMP, ICC (multi-chunk JPEG, compressed PNG) and IPTC, as raw bytes |
| Metadata | `Metadata::from_embedded`, `parse_exif`, `parse_xmp`, `parse_iptc` | Camera, lens, exposure, capture time (via `chrono`), GPS, title, caption, alt text, keywords, creators, rights, location |
| Probe | `probe`, `probe_file` | All of the above in one call; `probe_file` reads a 16 MiB prefix |
| Colour | `icc_info`, `Decoded::convert_to_srgb` | Profile description; conversion to sRGB through `craft-cms` |
| Decode | `decode` | RGBA8; DCT-scaled JPEG decode and sharp vector rendering when a target size is given |
| Sanitise | `sanitize_svg` | Allowlist SVG sanitiser (always built, no renderer needed) |
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
| PSD / PSB | yes | EXIF, XMP, ICC, IPTC | saved composite via `craft-psd`, else a composite of the layers (`PsdMode::Layers` forces it; `composite_layers` reports what it left out) | no |
| HEIC / HEIF | yes (`ispe`, `irot`, `imir`, `pixi`) | EXIF, XMP, ICC (items, `colr`) | yes, `craft-hevc`; tiled grids; 8/10-bit | no |
| AVIF | yes | EXIF, XMP, ICC | yes, `craft-av1`; 8/10/12-bit, alpha | no |
| Camera raw (DNG, CR2, CR3, NEF, ARW, RAF, ORF, RW2, …) | yes (CR3, RAF, ORF, RW2 from the preview) | EXIF | yes, `craft-raw`: the full-frame embedded preview, else developed (see below) | no |
| JPEG XL | yes (`jxl-oxide`; native builds) | ICC (embedded, else generated from the colour encoding) | yes: first frame, orientation applied, alpha; no CMYK or splines | no |
| OpenEXR | yes (data window, channels) | none | yes: first valid RGBA layer, float clamped to [0,1] and sRGB-encoded (plain clamp, no tone map) | no |
| Radiance HDR | yes | none | yes: RGBE float, same clamp and sRGB encoding as EXR | no |
| DDS | yes | none | DXT1/3/5 (BC1-3) only, first mip | no |
| ICO / CUR | yes (the entry the decoder picks: deepest, then largest) | none | yes (BMP or PNG entries) | no |
| PNM (PBM, PGM, PPM, PAM) | yes | none | yes, 8 and 16 bit, PAM alpha | no |
| QOI | yes | none | yes | no |
| DPX | yes (size, bits, orientation) | none | yes: first element, luma/RGB/RGBA at 8/10/12/16 bits, code values as display-referred (no log conversion) | no |
| PCX | yes | none | yes: RLE, 1/2/4-bit with header palette, 8-bit with VGA palette, 24-bit planes | no |
| FITS | yes | none | yes: primary HDU, BITPIX 8/16/32/-32/-64, linear min/max stretch to 8 bits (lossy) | no |
| DICOM | yes | none | uncompressed transfer syntaxes only, grey (window or min/max) and RGB; compressed syntaxes are refused | no |
| SVG / SVGZ | yes (`width`/`height` or `viewBox`) | none | yes: `usvg` + `resvg`, sanitised first; fitted to the target size, else intrinsic size. `data:` images only; no text yet (see below) | no |
| TGA | yes (v2 footer, else a strict header check) | none | yes, including RLE and colour-mapped | no |

## Cargo features

Every format family is a feature, and all are on by default (`default = ["full"]`), so MediaCraft's
own crates are unchanged. A consumer that wants a small dependency tree turns defaults off and
names the formats it needs. Hermit CMS, for example:

```toml
craft-image = { version = "0.1", default-features = false, features = ["png", "jpeg", "webp", "heic", "avif"] }
```

What stays on whatever you choose: `sniff`, `read_header`, `embedded` and `probe` (so a catalogue
can still identify a file it cannot decode), the colour, EXIF, XMP, IPTC, resize and orientation
code, and the `Format` and `OutputFormat` enums with all their variants. A feature only adds
pixels. Decoding a format whose feature is off returns `Error::Unsupported` (and `probe` reports
`decodable: false`); encoding an `OutputFormat` whose feature is off returns `Error::Encode`
(`OutputFormat::can_encode` tells you first). Two headers do need their feature: JPEG XL (`jxl`)
and the size of non-TIFF raws such as CR3 (`raw`).

| Feature | Adds | Outside dependencies it brings |
|---|---|---|
| `jpeg` | JPEG decode (DCT-scaled) and encode | `image` codec (`zune-jpeg`, `zune-core`), `jpeg-decoder` |
| `png` | PNG decode and encode | `png` (`fdeflate`, `miniz_oxide`, `crc32fast`, `bitflags`) |
| `webp` | WebP decode, lossless encode, lossy encode | `image-webp`, `craft-vp8enc` (in this workspace, no dependencies) |
| `gif` | GIF decode | `gif`, `color_quant` |
| `bmp` | BMP decode | none beyond `image` |
| `tiff` | TIFF / BigTIFF decode | `tiff` |
| `psd` | PSD / PSB composite | `craft-psd` (`tracing`) |
| `heic` | HEIC decode | `craft-hevc` (`rayon`, `tracing`, `craft-bitstream`) |
| `avif` | AVIF decode | `craft-av1` (`tracing`, `craft-bitstream`) |
| `jxl` | JPEG XL decode (native targets only) | `jxl-oxide` |
| `raw` | Camera raw preview and development; turns on `jpeg` | `craft-raw` (`zune-jpeg`, `jxl-oxide`, `rayon`) |
| `exr` | OpenEXR decode | `exr` |
| `hdr`, `dds`, `ico`, `pnm`, `qoi`, `tga` | Decode through `image` | none beyond `image` (`ico` uses `image`'s own BMP and PNG; `qoi` adds `qoi`) |
| `dpx`, `pcx`, `fits`, `dicom` | Decode, written in this crate | none |
| `svg` | SVG / SVGZ rendering | `usvg` and `resvg` (`tiny-skia`, `png`, `roxmltree`, `svgtypes`, and `gif`, `image-webp`, `zune-jpeg` for `data:` images); no text shaping, no fonts |
| `full` | All of the above (the default) | all of the above |

Always present: `image` (the resampler; with no codec it adds `moxcms`, `bytemuck`, `byteorder-lite`
and `num-traits`), `craft-cms` (with `rayon`), `quick-xml`, `chrono`, `flate2`, `serde` and `thiserror`.

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

## SVG

`sanitize_svg(bytes)` is always built and has no renderer behind it, so a service that only stores
or serves SVG can call it with every feature off. It parses with `roxmltree` and writes back plain
SVG from an allowlist of elements and attributes. It removes `script`, `foreignObject`,
animation, every `on*` attribute, every `href` that is not a local `#fragment` or a
`data:image/png|jpeg|gif|webp` URI, `javascript:` anywhere, `<style>` content with `@import`,
`expression(`, CSS escapes or a `url(` that is not a fragment, comments, processing instructions,
DOCTYPE and entity declarations. Internal entities are expanded first; external ones are never
read (a reference to one is an error). Element count, depth, `<use>` expansion (a "billion laughs"
built from nested `<use>`) and decompressed size are capped, and a document over a cap is an error.
`SanitizeLimits` and `sanitize_svg_with` change the caps.

With the `svg` feature `decode` rasterises SVG and SVGZ. Every document goes through the sanitiser
first. With `DecodeOptions::fit` the result is the largest size within it that keeps the aspect
ratio (up as well as down); without it the intrinsic size, which `Limits` must allow. Images
load from `data:` URIs only: a path or URL in `href` never opens a file or a socket.

Text does not render yet. `usvg` is built without its `text` feature, so `<text>` is skipped and
system fonts are never read. Rendering text with fonts embedded in the document or from the shared
font stack is tracked in #24.

## Not yet

* **SVG text:** needs the shared font stack (#24).
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
