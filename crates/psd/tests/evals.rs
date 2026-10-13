//! Adversarial evaluations, diagnostics, and limits verification for `craft-psd`.

use craft_psd::{LayerSpec, PixelData, PsdBuilder, PsdError, PsdFile, Version};

#[test]
fn eval_hostile_invalid_signature_and_truncated_headers() {
    // 1. Completely bogus header signature (e.g. PNG or RIFF header)
    let bad_sig = b"RIFF\x00\x01\x00\x00\x00\x00\x00\x00";
    let res = PsdFile::from_bytes(bad_sig);
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        PsdError::InvalidSignature { .. }
    ));

    // 2. Truncated header (fewer than 26 bytes)
    let truncated = b"8BPS\x00\x01";
    let res_trunc = PsdFile::from_bytes(truncated);
    assert!(res_trunc.is_err());
    assert!(matches!(
        res_trunc.unwrap_err(),
        PsdError::UnexpectedEof { .. }
    ));

    // 3. Unsupported version (e.g. version 3)
    let mut bad_version = vec![0u8; 26];
    bad_version[0..4].copy_from_slice(b"8BPS");
    bad_version[4..6].copy_from_slice(&3u16.to_be_bytes()); // version 3
    let res_ver = PsdFile::from_bytes(&bad_version);
    assert!(res_ver.is_err());
    assert!(matches!(
        res_ver.unwrap_err(),
        PsdError::UnsupportedVersion(3)
    ));
}

#[test]
fn eval_dimension_explosion_limit_protection() {
    // PSD specification enforces maximum dimension of 30,000 for PSD (v1) and 300,000 for PSB (v2).
    // Test that constructing a synthetic header with dimensions beyond limit is rejected safely.
    let mut hostile_header_bytes = vec![0u8; 26];
    hostile_header_bytes[0..4].copy_from_slice(b"8BPS");
    hostile_header_bytes[4..6].copy_from_slice(&1u16.to_be_bytes()); // PSD v1
    hostile_header_bytes[12..14].copy_from_slice(&3u16.to_be_bytes()); // 3 channels
    hostile_header_bytes[14..18].copy_from_slice(&500_000u32.to_be_bytes()); // height = 500,000!
    hostile_header_bytes[18..22].copy_from_slice(&500_000u32.to_be_bytes()); // width = 500,000!
    hostile_header_bytes[22..24].copy_from_slice(&8u16.to_be_bytes()); // 8-bit depth
    hostile_header_bytes[24..26].copy_from_slice(&3u16.to_be_bytes()); // RGB mode

    let res = PsdFile::from_bytes(&hostile_header_bytes);
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), PsdError::LimitExceeded(_)));
}

#[test]
fn eval_psd_diagnostics_on_synthesized_layered_document() {
    let mut b = PsdBuilder::new(100, 50);
    // Add Layer 1: Normal layer within bounds
    b.push_layer(LayerSpec::new(
        "Foreground",
        10,
        10,
        20,
        20,
        PixelData::Rgba8([255, 0, 0, 255].repeat(400)),
    ));
    // Composite: 100x50 RGBA8
    b.composite(PixelData::Rgba8([0, 0, 0, 255].repeat(100 * 50)));

    let bytes = b.to_bytes().expect("serialization should succeed");
    let file = PsdFile::from_bytes(&bytes).expect("parse should succeed");

    let report = file.diagnose();
    assert_eq!(report.version, Version::Psd);
    assert_eq!(report.canvas_width, 100);
    assert_eq!(report.canvas_height, 50);
    assert_eq!(report.channels, 3);
    assert_eq!(report.depth, 8);
    assert_eq!(report.layer_count, 1);
    assert_eq!(report.uncompressed_composite_bytes, 100 * 50 * 3);
    assert_eq!(report.uncompressed_layers_bytes, 20 * 20 * 4);
}

#[test]
fn eval_psd_diagnostics_detects_out_of_bounds_layer() {
    let mut b = PsdBuilder::new(50, 50);
    // Layer placed with right coordinate 80, exceeding canvas width 50
    b.push_layer(LayerSpec::new(
        "OverflowLayer",
        30,
        10,
        50, // width 50 => right = 30 + 50 = 80 > 50
        20,
        PixelData::Rgba8([0, 255, 0, 255].repeat(50 * 20)),
    ));
    b.composite(PixelData::Rgba8([0, 0, 0, 255].repeat(50 * 50)));

    let bytes = b.to_bytes().expect("serialization should succeed");
    let file = PsdFile::from_bytes(&bytes).expect("parse should succeed");

    let report = file.diagnose();
    assert_eq!(report.layer_count, 1);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("extends beyond canvas bounds"))
    );
}
