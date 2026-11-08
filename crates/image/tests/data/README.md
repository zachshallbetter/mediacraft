# Test images

Original synthetic images, made for this repository; MIT OR Apache-2.0 like the crate.

| File | Made with |
|---|---|
| `pattern.png` | ffmpeg `testsrc2=size=320x240`, one frame |
| `pattern_alpha.png` | the same pattern as RGBA: left half opaque, right half alpha 64 |
| `pattern_alpha.heic` | macOS `sips -s format heic pattern_alpha.png` (HEVC primary image plus alpha auxiliary image) |
| `pattern_10bit.avif` | ffmpeg with SVT-AV1, `-pix_fmt yuv420p10le`, from `pattern.png` |
