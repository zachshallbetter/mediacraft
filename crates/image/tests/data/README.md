# Test images

Original synthetic images, made for this repository; MIT OR Apache-2.0 like the crate.

| File | Made with |
|---|---|
| `pattern.png` | ffmpeg `testsrc2=size=320x240`, one frame |
| `pattern_alpha.png` | the same pattern as RGBA: left half opaque, right half alpha 64 |
| `pattern_alpha.heic` | macOS `sips -s format heic pattern_alpha.png` (HEVC primary image plus alpha auxiliary image) |
| `pattern_10bit.avif` | ffmpeg with SVT-AV1, `-pix_fmt yuv420p10le`, from `pattern.png` |
| `pattern_alpha.jxl` | `cjxl pattern_alpha.png -d 0 -e 7` (lossless, so it decodes to exactly `pattern_alpha.png`) |
| `pattern_linear_small.jxl` | `cjxl -d 0 -e 7 -x color_space=RGB_D65_SRG_Rel_Lin` on `pattern.png` scaled to 64×48 (`sips -z 48 64`): colour encoding is linear sRGB, no embedded ICC |

## Photoshop files (`psd_*.psd`)

Made in Adobe Photoshop by the PhotoCraft Corpus scripts (MIT OR Apache-2.0, ArtCraft Team and the PhotoCraft contributors), copied unchanged from `photocraft-corpus/photoshop`. Each stores layers and Photoshop's own merged composite. `psd_compose.rs` composites the layers and compares.

| File | Corpus path | Uses |
|---|---|---|
| `psd_rgb8_layers.psd` | `adjustments/rgb8/baseline-layer-copy` | RGB 8-bit, plain layers |
| `psd_cmyk8_layers.psd` | `adjustments/cmyk8/baseline-layer-copy` | CMYK 8-bit, plain layers |
| `psd_clipped_blend_adjustment.psd` | `adjustments/rgb8/clipped-hue-sat-blend-color` | clipping group, blend mode, an adjustment layer (not drawn) |
| `psd_masked_opacity_adjustment.psd` | `adjustments/rgb8/masked-curves-gradient-mask-opacity60` | layer mask, opacity, an adjustment layer (not drawn) |
| `psd_effects_satin.psd` | `effects/satin-default` | a layer effect (not drawn) |
