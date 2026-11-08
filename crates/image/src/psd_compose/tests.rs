use super::*;
use craft_psd::tagged::TaggedBlock;
use craft_psd::{GroupSpec, LayerSpec, MaskSpec, PixelData, PsdBuilder};

const EPS: f32 = 1e-4;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < EPS
}

/// Separable modes against values worked out by hand: (mode, backdrop, source, expected).
#[test]
fn separable_modes_match_hand_computed_values() {
    use BlendMode::*;
    let cases: &[(BlendMode, f32, f32, f32)] = &[
        (Normal, 0.8, 0.4, 0.4),
        (Darken, 0.8, 0.4, 0.4),
        (Multiply, 0.8, 0.4, 0.32),
        (ColorBurn, 0.8, 0.4, 0.5),
        (ColorBurn, 1.0, 0.0, 1.0),
        (ColorBurn, 0.5, 0.0, 0.0),
        (LinearBurn, 0.8, 0.4, 0.2),
        (LinearBurn, 0.3, 0.4, 0.0),
        (Lighten, 0.8, 0.4, 0.8),
        (Screen, 0.8, 0.4, 0.88),
        (ColorDodge, 0.3, 0.4, 0.5),
        (ColorDodge, 0.8, 0.4, 1.0),
        (ColorDodge, 0.5, 1.0, 1.0),
        (ColorDodge, 0.0, 0.9, 0.0),
        (LinearDodge, 0.3, 0.4, 0.7),
        (LinearDodge, 0.8, 0.4, 1.0),
        // overlay is hard light with the layers swapped: 2·b·s below the middle
        (Overlay, 0.2, 0.6, 0.24),
        (Overlay, 0.8, 0.4, 0.76),
        (SoftLight, 0.8, 0.4, 0.768),
        (SoftLight, 0.25, 0.8, 0.4),
        (HardLight, 0.8, 0.4, 0.64),
        (HardLight, 0.4, 0.8, 0.4 + 0.6 - 0.24),
        (VividLight, 0.8, 0.4, 0.75),
        (VividLight, 0.3, 0.75, 0.6),
        (LinearLight, 0.3, 0.4, 0.1),
        (LinearLight, 0.8, 0.4, 0.6),
        (PinLight, 0.8, 0.3, 0.6),
        (PinLight, 0.3, 0.8, 0.6),
        (HardMix, 0.8, 0.4, 1.0),
        (HardMix, 0.5, 0.4, 0.0),
        (Difference, 0.8, 0.4, 0.4),
        (Exclusion, 0.8, 0.4, 0.56),
        (Subtract, 0.8, 0.4, 0.4),
        (Subtract, 0.3, 0.5, 0.0),
        (Divide, 0.2, 0.4, 0.5),
        (Divide, 0.8, 0.4, 1.0),
        (Divide, 0.5, 0.0, 1.0),
    ];
    for &(mode, b, s, want) in cases {
        let got = blend_rgb(mode, [b; 3], [s; 3]);
        assert!(
            got.iter().all(|&g| close(g, want)),
            "{mode:?} b={b} s={s}: {got:?}, want {want}"
        );
    }
}

#[test]
fn soft_light_differs_from_w3c_only_for_dark_backdrops() {
    // W3C uses ((16b-12)b+4)b for b <= 0.25 where the Photoshop form uses sqrt(b).
    let (b, s) = (0.1f32, 0.9f32);
    let ps = 2.0 * b * (1.0 - s) + b.sqrt() * (2.0 * s - 1.0);
    let w3c = b + (2.0 * s - 1.0) * (((16.0 * b - 12.0) * b + 4.0) * b - b);
    assert!(close(sep(BlendMode::SoftLight, b, s), ps));
    assert!((ps - w3c).abs() > 0.01);
}

#[test]
fn whole_pixel_modes_match_hand_computed_values() {
    let cb = [0.2, 0.4, 0.6];
    let cs = [1.0, 0.0, 0.0];
    let check = |mode, want: [f32; 3]| {
        let got = blend_rgb(mode, cb, cs);
        assert!(
            got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-3),
            "{mode:?}: {got:?}, want {want:?}"
        );
    };
    check(BlendMode::Luminosity, [0.138, 0.338, 0.538]);
    check(BlendMode::Color, [1.0, 0.088571, 0.088571]);
    check(BlendMode::Hue, [0.642, 0.242, 0.242]);
    check(BlendMode::Saturation, [0.0, 0.446913, 0.893827]);
    // Channel sums: cb 0.6 against cs 1.1.
    let dark = [0.2; 3];
    let bright = [0.9, 0.1, 0.1];
    assert_eq!(blend_rgb(BlendMode::DarkerColor, dark, bright), dark);
    assert_eq!(blend_rgb(BlendMode::LighterColor, dark, bright), bright);
}

#[test]
fn compositing_follows_the_w3c_alpha_formula() {
    // Half-transparent blue over opaque red.
    let mut d = [1.0, 0.0, 0.0, 1.0];
    composite_px(&mut d, [0.0, 0.0, 1.0], 0.5, BlendMode::Normal, false);
    assert!(
        d.iter()
            .zip([0.5, 0.0, 0.5, 1.0])
            .all(|(a, b)| close(*a, b)),
        "{d:?}"
    );
    // Onto nothing the source colour is kept, whatever the mode.
    let mut d = [0.0; 4];
    composite_px(&mut d, [0.3, 0.6, 0.9], 0.5, BlendMode::Multiply, false);
    assert!(
        d.iter()
            .zip([0.3, 0.6, 0.9, 0.5])
            .all(|(a, b)| close(*a, b)),
        "{d:?}"
    );
    // Two half layers: alpha 0.75.
    composite_px(&mut d, [0.3, 0.6, 0.9], 0.5, BlendMode::Normal, false);
    assert!(close(d[3], 0.75));
    // Clipping keeps the base alpha and ignores empty base pixels.
    let mut d = [0.0, 1.0, 0.0, 0.5];
    composite_px(&mut d, [1.0, 0.0, 0.0], 1.0, BlendMode::Normal, true);
    assert!(close(d[3], 0.5) && close(d[0], 1.0) && close(d[1], 0.0));
    let mut e = [0.0; 4];
    composite_px(&mut e, [1.0, 0.0, 0.0], 1.0, BlendMode::Normal, true);
    assert_eq!(e, [0.0; 4]);
}

#[test]
fn mask_density_invert_and_default() {
    let img = GrayImage {
        left: 1,
        top: 0,
        width: 1,
        height: 1,
        data: vec![0],
        default_value: 255,
    };
    let m = MaskPlane {
        img: img.clone(),
        invert: false,
        density: 1.0,
    };
    assert!(close(m.at(1, 0), 0.0) && close(m.at(0, 0), 1.0) && close(m.at(5, 5), 1.0));
    let half = MaskPlane {
        img: img.clone(),
        invert: false,
        density: 0.5,
    };
    assert!(close(half.at(1, 0), 0.5));
    let inv = MaskPlane {
        img,
        invert: true,
        density: 1.0,
    };
    assert!(close(inv.at(1, 0), 1.0));
}

// ---- whole files ---------------------------------------------------------------------------

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> PixelData {
    PixelData::Rgba8(rgba.repeat((w * h) as usize))
}

fn layer(name: &str, x: i32, y: i32, w: u32, h: u32, rgba: [u8; 4]) -> LayerSpec {
    LayerSpec::new(name, x, y, w, h, solid(w, h, rgba))
}

fn compose(b: &PsdBuilder) -> (Rgba8Image, CompositeReport) {
    let file = b.build().unwrap();
    composite_file(&file, &Limits::default()).unwrap()
}

fn px(img: &Rgba8Image, x: u32, y: u32) -> [u8; 4] {
    let o = ((y * img.width() + x) * 4) as usize;
    img.data()[o..o + 4].try_into().unwrap()
}

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1)
}

fn block(key: &[u8; 4]) -> TaggedBlock {
    TaggedBlock {
        signature: *b"8BIM",
        key: *key,
        data: vec![0; 4],
        padding: None,
    }
}

#[test]
fn blend_modes_in_a_file() {
    let mut b = PsdBuilder::new(4, 4);
    b.push_layer(layer("red", 0, 0, 4, 4, [255, 0, 0, 255]));
    let mut blue = layer("blue", 1, 1, 2, 2, [0, 0, 255, 255]);
    blue.blend_mode = BlendMode::Multiply;
    b.push_layer(blue);
    let (img, r) = compose(&b);
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(&img, 1, 1), [0, 0, 0, 255]);
    assert_eq!(px(&img, 3, 3), [255, 0, 0, 255]);
    assert_eq!(r.layers_drawn, 2);

    let mut b = PsdBuilder::new(4, 4);
    b.push_layer(layer("red", 0, 0, 4, 4, [255, 0, 0, 255]));
    let mut blue = layer("blue", 1, 1, 2, 2, [0, 0, 255, 255]);
    blue.blend_mode = BlendMode::Screen;
    blue.opacity = 128;
    b.push_layer(blue);
    let (img, _) = compose(&b);
    // Screen gives magenta; half of it over red.
    assert!(
        near(px(&img, 1, 1), [255, 0, 128, 255]),
        "{:?}",
        px(&img, 1, 1)
    );
}

#[test]
fn opacity_fill_opacity_and_hidden() {
    let mut b = PsdBuilder::new(1, 1);
    b.push_layer(layer("red", 0, 0, 1, 1, [255, 0, 0, 255]));
    let mut blue = layer("blue", 0, 0, 1, 1, [0, 0, 255, 255]);
    blue.opacity = 128;
    blue.fill_opacity = Some(128);
    b.push_layer(blue);
    let mut hidden = layer("green", 0, 0, 1, 1, [0, 255, 0, 255]);
    hidden.visible = false;
    b.push_layer(hidden);
    let (img, r) = compose(&b);
    // 0.5 × 0.5 = 0.25 of blue.
    assert!(
        near(px(&img, 0, 0), [191, 0, 64, 255]),
        "{:?}",
        px(&img, 0, 0)
    );
    assert_eq!(r.hidden_skipped, 1);
}

#[test]
fn clipping_confines_a_layer_to_the_base() {
    let mut b = PsdBuilder::new(2, 1);
    b.push_layer(layer("white", 0, 0, 2, 1, [255, 255, 255, 255]));
    b.push_layer(LayerSpec::new(
        "base",
        0,
        0,
        2,
        1,
        PixelData::Rgba8(vec![0, 255, 0, 255, 0, 255, 0, 0]),
    ));
    let mut red = layer("red", 0, 0, 2, 1, [255, 0, 0, 255]);
    red.clipping = true;
    b.push_layer(red);
    let (img, r) = compose(&b);
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(&img, 1, 0), [255, 255, 255, 255]);
    assert_eq!(r.clipped_layers, 1);
}

#[test]
fn pass_through_and_isolated_groups() {
    let build = |mode: BlendMode, opacity: u8| {
        let mut b = PsdBuilder::new(1, 1);
        b.push_layer(layer("red", 0, 0, 1, 1, [255, 0, 0, 255]));
        let mut g = GroupSpec::new("g");
        g.blend_mode = mode;
        g.opacity = opacity;
        b.begin_group(g);
        let mut green = layer("green", 0, 0, 1, 1, [0, 255, 0, 255]);
        green.blend_mode = BlendMode::Multiply;
        b.push_layer(green);
        b.end_group().unwrap();
        compose(&b)
    };
    // Pass-through: the child multiplies with the red below it.
    let (img, r) = build(BlendMode::PassThrough, 255);
    assert_eq!(px(&img, 0, 0), [0, 0, 0, 255]);
    assert_eq!(r.groups, 1);
    // Isolated: the child multiplies with nothing, then the group draws normally.
    let (img, _) = build(BlendMode::Normal, 255);
    assert_eq!(px(&img, 0, 0), [0, 255, 0, 255]);
    // Group opacity mixes pass-through back toward the backdrop.
    let (img, _) = build(BlendMode::PassThrough, 128);
    assert!(
        near(px(&img, 0, 0), [127, 0, 0, 255]),
        "{:?}",
        px(&img, 0, 0)
    );
    let (img, _) = build(BlendMode::Normal, 128);
    assert!(
        near(px(&img, 0, 0), [127, 128, 0, 255]),
        "{:?}",
        px(&img, 0, 0)
    );
}

#[test]
fn hidden_group_hides_its_children() {
    let mut b = PsdBuilder::new(1, 1);
    b.push_layer(layer("red", 0, 0, 1, 1, [255, 0, 0, 255]));
    let mut g = GroupSpec::new("g");
    g.visible = false;
    b.begin_group(g);
    b.push_layer(layer("green", 0, 0, 1, 1, [0, 255, 0, 255]));
    b.end_group().unwrap();
    let (img, r) = compose(&b);
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert_eq!(r.hidden_skipped, 1);
    assert_eq!(r.layers_drawn, 1);
}

#[test]
fn layer_masks() {
    let mask = |disabled| MaskSpec {
        rect: Rect::from_xywh(0, 0, 2, 1),
        data: vec![255, 0],
        default_color: 255,
        disabled,
    };
    for disabled in [false, true] {
        let mut b = PsdBuilder::new(2, 1);
        b.push_layer(layer("white", 0, 0, 2, 1, [255, 255, 255, 255]));
        let mut red = layer("red", 0, 0, 2, 1, [255, 0, 0, 255]);
        red.mask = Some(mask(disabled));
        b.push_layer(red);
        let (img, r) = compose(&b);
        assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
        if disabled {
            assert_eq!(px(&img, 1, 0), [255, 0, 0, 255]);
            assert_eq!((r.masks_applied, r.masks_disabled), (0, 1));
        } else {
            assert_eq!(px(&img, 1, 0), [255, 255, 255, 255]);
            assert_eq!((r.masks_applied, r.masks_disabled), (1, 0));
        }
    }
}

#[test]
fn unknown_dissolve_and_unsupported_features_are_reported() {
    let mut b = PsdBuilder::new(1, 1);
    b.push_layer(layer("bg", 0, 0, 1, 1, [10, 20, 30, 255]));
    let mut a = layer("unknown", 0, 0, 1, 1, [200, 0, 0, 255]);
    a.blend_mode = BlendMode::Unknown(*b"zzzz");
    b.push_layer(a);
    let mut d = layer("dissolve", 0, 0, 1, 1, [0, 200, 0, 255]);
    d.blend_mode = BlendMode::Dissolve;
    b.push_layer(d);
    let mut adj = layer("adj", 0, 0, 1, 1, [0, 0, 0, 0]);
    adj.extra_blocks.push(block(b"levl"));
    b.push_layer(adj);
    let mut fill = layer("fill", 0, 0, 1, 1, [0, 0, 0, 0]);
    fill.extra_blocks.push(block(b"SoCo"));
    b.push_layer(fill);
    let mut fx = layer("fx", 0, 0, 1, 1, [0, 0, 0, 0]);
    fx.extra_blocks.extend([
        block(b"lfx2"),
        block(b"vmsk"),
        block(b"TySh"),
        block(b"SoLd"),
    ]);
    b.push_layer(fx);
    let (img, r) = compose(&b);
    assert_eq!(px(&img, 0, 0), [0, 200, 0, 255]);
    assert_eq!((r.unknown_blend_modes, r.dissolve_as_normal), (1, 1));
    assert_eq!((r.adjustment_layers_ignored, r.fill_layers_ignored), (1, 1));
    assert_eq!((r.layer_effects_ignored, r.vector_masks_ignored), (1, 1));
    assert_eq!((r.text_layers_raster, r.smart_objects_raster), (1, 1));
}

#[test]
fn layers_off_the_canvas_and_negative_bounds() {
    let mut b = PsdBuilder::new(2, 2);
    b.push_layer(layer("big", -1, -1, 3, 3, [1, 2, 3, 255]));
    b.push_layer(layer("gone", 10, 10, 2, 2, [9, 9, 9, 255]));
    b.push_layer(layer("left", -5, 0, 2, 2, [9, 9, 9, 255]));
    let (img, r) = compose(&b);
    assert_eq!(px(&img, 1, 1), [1, 2, 3, 255]);
    assert_eq!(r.layers_drawn, 1);
}

#[test]
fn absurd_layer_bounds_are_refused_before_allocating() {
    let mut b = PsdBuilder::new(2, 2);
    b.push_layer(layer("bg", 0, 0, 2, 2, [1, 2, 3, 255]));
    b.push_layer(layer("x", 0, 0, 1, 1, [5, 5, 5, 255]));
    let mut file = b.build().unwrap();
    file.layers_mut()[1].rect = Rect {
        top: -2_000_000_000,
        left: -2_000_000_000,
        bottom: 2_000_000_000,
        right: 2_000_000_000,
    };
    let (_, r) = composite_file(&file, &Limits::default()).unwrap();
    assert_eq!(r.layers_oversized, 1);
    file.layers_mut()[1].rect = Rect {
        top: 5,
        left: 5,
        bottom: 1,
        right: 1,
    };
    assert!(composite_file(&file, &Limits::default()).is_ok());
}

#[test]
fn canvas_over_the_allocation_limit_is_refused() {
    let mut b = PsdBuilder::new(64, 64);
    b.push_layer(layer("bg", 0, 0, 64, 64, [1, 2, 3, 255]));
    let file = b.build().unwrap();
    let limits = Limits {
        max_alloc: 1024,
        ..Limits::default()
    };
    assert!(matches!(
        composite_file(&file, &limits),
        Err(Error::TooLarge { .. })
    ));
}

#[test]
fn gray_cmyk_and_sixteen_bit() {
    let mut b = PsdBuilder::new(1, 1).color_mode(ColorMode::Grayscale);
    b.push_layer(LayerSpec::new(
        "g",
        0,
        0,
        1,
        1,
        PixelData::GrayA8(vec![128, 255]),
    ));
    assert_eq!(px(&compose(&b).0, 0, 0), [128, 128, 128, 255]);

    let mut b = PsdBuilder::new(2, 1).color_mode(ColorMode::Cmyk);
    b.push_layer(LayerSpec::new(
        "c",
        0,
        0,
        2,
        1,
        PixelData::Cmyka8(vec![0, 0, 0, 0, 255, 0, 0, 0, 255, 255]),
    ));
    let (img, _) = compose(&b);
    assert_eq!(px(&img, 0, 0), [255, 255, 255, 255]);
    assert_eq!(px(&img, 1, 0), [0, 0, 0, 255]);

    let mut b = PsdBuilder::new(1, 1).depth(16);
    b.push_layer(LayerSpec::new(
        "r",
        0,
        0,
        1,
        1,
        PixelData::Rgba16(vec![65535, 0, 0, 65535]),
    ));
    assert_eq!(px(&compose(&b).0, 0, 0), [255, 0, 0, 255]);
}

#[test]
fn unsupported_modes_and_empty_files_are_errors() {
    let mut b = PsdBuilder::new(1, 1);
    b.push_layer(layer("bg", 0, 0, 1, 1, [1, 2, 3, 255]));
    let mut file = b.build().unwrap();
    file.header.color_mode = ColorMode::Lab;
    assert!(matches!(
        composite_file(&file, &Limits::default()),
        Err(Error::Unsupported(..))
    ));
    if let Ok(empty) = PsdBuilder::new(1, 1).build() {
        assert!(matches!(
            composite_file(&empty, &Limits::default()),
            Err(Error::Unsupported(..))
        ));
    }
}

#[test]
fn deep_nesting_is_an_error_not_a_stack_overflow() {
    let mut b = PsdBuilder::new(1, 1);
    for i in 0..(MAX_DEPTH + 10) {
        b.begin_group(GroupSpec::new(format!("g{i}")));
    }
    b.push_layer(layer("x", 0, 0, 1, 1, [1, 2, 3, 255]));
    for _ in 0..(MAX_DEPTH + 10) {
        b.end_group().unwrap();
    }
    let file = b.build().unwrap();
    assert!(composite_file(&file, &Limits::default()).is_err());
}

// ---- decode order ----------------------------------------------------------------------------

#[test]
fn decode_order_prefers_a_valid_saved_composite() {
    use crate::{DecodeOptions, PsdMode, decode};
    let mut b = PsdBuilder::new(1, 1);
    b.push_layer(layer("red", 0, 0, 1, 1, [255, 0, 0, 255]));
    // No composite: the file says it has no real merged data, so layers render.
    let bytes = b.to_bytes().unwrap();
    let d = decode(&bytes, &DecodeOptions::default()).unwrap();
    assert_eq!(d.image.data(), &[255, 0, 0, 255]);

    // A saved composite that differs from the layers wins by default, loses when forced.
    b.composite(PixelData::Rgba8(vec![0, 0, 255, 255]));
    let bytes = b.to_bytes().unwrap();
    let auto = decode(&bytes, &DecodeOptions::default()).unwrap();
    assert_eq!(auto.image.data(), &[0, 0, 255, 255]);
    let forced = decode(
        &bytes,
        &DecodeOptions {
            psd: PsdMode::Layers,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(forced.image.data(), &[255, 0, 0, 255]);
    assert_eq!(
        composite_layers(&bytes, &Limits::default())
            .unwrap()
            .0
            .data(),
        &[255, 0, 0, 255]
    );
}
