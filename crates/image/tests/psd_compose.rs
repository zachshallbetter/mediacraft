//! PSD layer compositing against Photoshop's own saved composites, and hostile input.
#![cfg(feature = "psd")]

use craft_image::{DecodeOptions, Limits, PsdMode, Rgba8Image, composite_layers, decode};

const FILES: [&str; 5] = [
    "psd_rgb8_layers.psd",
    "psd_cmyk8_layers.psd",
    "psd_clipped_blend_adjustment.psd",
    "psd_masked_opacity_adjustment.psd",
    "psd_effects_satin.psd",
];

fn read(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Mean absolute difference over all RGBA samples, on a 0..255 scale.
fn mean_abs_diff(a: &Rgba8Image, b: &Rgba8Image) -> f64 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let sum: u64 = a
        .data()
        .iter()
        .zip(b.data())
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    sum as f64 / a.data().len() as f64
}

/// The saved composite is Photoshop's rendering; the layer composite should approach it, and
/// differ only by what is reported as not drawn.
#[test]
fn layers_match_the_saved_composite() {
    // Mean absolute difference (0..255) allowed per file. Files that use features the
    // compositor reports as not drawn are allowed a larger gap.
    let ceiling = [5.0, 8.0, 60.0, 60.0, 60.0];
    let mut failures = Vec::new();
    for (name, max) in FILES.iter().zip(ceiling) {
        let bytes = read(name);
        let saved = decode(&bytes, &DecodeOptions::default()).unwrap().image;
        let (layers, report) = composite_layers(&bytes, &Limits::default()).unwrap();
        let diff = mean_abs_diff(&saved, &layers);
        println!("PARITY {name}: mean_abs_diff={diff:.4} {report:?}");
        if diff > max || report.layers_drawn == 0 {
            failures.push(format!(
                "{name}: mean abs diff {diff:.3} (ceiling {max}), drew {}",
                report.layers_drawn
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn forcing_layers_goes_through_decode() {
    for name in FILES {
        let bytes = read(name);
        let forced = decode(
            &bytes,
            &DecodeOptions {
                psd: PsdMode::Layers,
                ..Default::default()
            },
        )
        .unwrap();
        let direct = composite_layers(&bytes, &Limits::default()).unwrap().0;
        assert_eq!(forced.image.data(), direct.data(), "{name}");
    }
}

/// Take the saved composite away (as a file saved without "maximise compatibility" has it) and
/// the file must still render the same pixels from its layers.
#[test]
fn a_file_without_its_composite_still_renders() {
    for name in FILES {
        let bytes = read(name);
        let mut file = craft_psd::PsdFile::from_bytes(&bytes).unwrap();
        let h = file.header.clone();
        let blank = vec![0u8; h.row_bytes() * h.height as usize * usize::from(h.channels)];
        file.image_data =
            craft_psd::ImageData::encode(craft_psd::Compression::Raw, &blank, &h).unwrap();
        // Mark it as a placeholder, as Photoshop does.
        if let Some(r) = file
            .resources
            .iter_mut()
            .find(|r| r.id == craft_psd::resources::ids::VERSION_INFO)
        {
            *r = craft_psd::resources::version_info_resource(false);
        }
        let stripped = file.to_bytes().unwrap();
        let auto = decode(&stripped, &DecodeOptions::default()).unwrap().image;
        let direct = composite_layers(&bytes, &Limits::default()).unwrap().0;
        assert_eq!(auto.data(), direct.data(), "{name}");
    }
}

fn small_limits() -> Limits {
    Limits {
        max_pixels: 4_000_000,
        max_alloc: 64 << 20,
        max_input_bytes: 64 << 20,
    }
}

/// Deterministic xorshift, so failures reproduce.
fn rng(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn prefixes_and_byte_flips_never_panic() {
    let limits = small_limits();
    let opts = DecodeOptions {
        psd: PsdMode::Layers,
        limits,
        ..Default::default()
    };
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for name in FILES {
        let bytes = read(name);
        for i in 0..48 {
            let cut = bytes.len() * i / 48;
            let _ = composite_layers(&bytes[..cut], &limits);
            let _ = decode(&bytes[..cut], &opts);
        }
        for _ in 0..40 {
            let mut b = bytes.clone();
            for _ in 0..1 + rng(&mut seed) % 8 {
                let at = (rng(&mut seed) % b.len() as u64) as usize;
                b[at] ^= 1 << (rng(&mut seed) % 8);
            }
            let _ = composite_layers(&b, &limits);
            let _ = decode(&b, &opts);
        }
    }
}
