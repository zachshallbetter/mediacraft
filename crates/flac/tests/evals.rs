//! Evaluation and hostile input test suite for craft-flac.

use craft_flac::{Encoder, EncoderConfig, STREAMINFO_OFFSET, diagnose_flac_stream};
use proptest::prelude::*;

#[test]
fn eval_flac_stream_diagnostics_on_clean_stream() {
    let mut enc =
        Encoder::new(EncoderConfig::new(48_000, 2, 16)).expect("valid encoder configuration");

    let left: Vec<i32> = (0..8192)
        .map(|i| ((i as f64 * 0.05).sin() * 15_000.0) as i32)
        .collect();
    let right = left.clone();

    let mut stream = enc.header();
    stream.extend(enc.encode(&[&left, &right]).expect("encode stereo"));
    stream.extend(enc.finish());

    let at = STREAMINFO_OFFSET as usize;
    stream[at..at + 34].copy_from_slice(&enc.streaminfo());

    let report = diagnose_flac_stream(&stream);

    assert!(report.valid_marker, "must detect valid fLaC marker");
    assert!(
        report.metadata_blocks_count >= 2,
        "at least STREAMINFO and VORBIS_COMMENT"
    );
    assert!(
        report.has_vorbis_comment,
        "must detect vendor vorbis comment"
    );
    assert!(
        report.errors.is_empty(),
        "clean stream has zero errors: {:?}",
        report.errors
    );
    assert!(
        report.warnings.is_empty(),
        "clean stream has zero warnings: {:?}",
        report.warnings
    );

    let info = report.streaminfo.expect("must parse streaminfo");
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(info.channels, 2);
    assert_eq!(info.bits_per_sample, 16);
    assert_eq!(info.total_samples, 8192);
    assert!(report.has_md5, "finished streaminfo must contain md5");
    assert!(
        report.frame_count >= 2,
        "expected at least 2 frames for 8192 samples (blocksize 4096)"
    );

    if let Some(duration) = report.estimated_duration_secs {
        assert!((duration - (8192.0 / 48000.0)).abs() < 1e-6);
    }
}

#[test]
fn eval_flac_stream_with_placeholder_md5() {
    let mut enc =
        Encoder::new(EncoderConfig::new(44_100, 1, 16)).expect("valid encoder configuration");

    let mono: Vec<i32> = (0..4096)
        .map(|i| ((i as f64 * 0.1).sin() * 10_000.0) as i32)
        .collect();

    let mut stream = enc.header();
    stream.extend(enc.encode(&[&mono]).expect("encode mono"));
    stream.extend(enc.finish());
    // Note: deliberately DO NOT overwrite STREAMINFO with finish() streaminfo to test uncalculated md5

    let report = diagnose_flac_stream(&stream);
    assert!(report.valid_marker);
    assert!(!report.has_md5, "placeholder md5 is all zeroes");
    assert!(report.errors.is_empty());
    assert_eq!(report.streaminfo.map(|s| s.sample_rate), Some(44_100));
}

#[test]
fn eval_flac_various_bit_depths_and_channels() {
    let test_cases: &[(u32, u8, u8)] = &[
        (44_100, 1, 8),   // 8-bit mono
        (48_000, 2, 24),  // 24-bit stereo
        (96_000, 6, 24),  // 24-bit 5.1 surround
        (192_000, 2, 32), // 32-bit hi-res stereo
    ];

    for &(rate, channels, bps) in test_cases {
        let mut cfg = EncoderConfig::new(rate, channels, bps);
        cfg.block_size = 512;
        let mut enc = Encoder::new(cfg).expect("valid configuration");

        let samples_per_channel = 1024;
        let max_val = (1i64 << (bps - 1)) - 2;
        let pcm: Vec<Vec<i32>> = (0..channels)
            .map(|ch| {
                (0..samples_per_channel)
                    .map(|i| {
                        let scale = (ch as f64 + 1.0) / (channels as f64 + 1.0);
                        (((i as f64 * 0.05).sin() * max_val as f64 * scale) as i64) as i32
                    })
                    .collect()
            })
            .collect();

        let slices: Vec<&[i32]> = pcm.iter().map(Vec::as_slice).collect();

        let mut stream = enc.header();
        stream.extend(enc.encode(&slices).expect("encode multi-channel"));
        stream.extend(enc.finish());

        let at = STREAMINFO_OFFSET as usize;
        stream[at..at + 34].copy_from_slice(&enc.streaminfo());

        let report = diagnose_flac_stream(&stream);
        assert!(report.valid_marker);
        assert!(
            report.errors.is_empty(),
            "failed for case ({}, {}, {}): {:?}",
            rate,
            channels,
            bps,
            report.errors
        );

        let info = report.streaminfo.expect("streaminfo present");
        assert_eq!(info.sample_rate, rate);
        assert_eq!(info.channels, channels);
        assert_eq!(info.bits_per_sample, bps);
        assert_eq!(info.total_samples, samples_per_channel as u64);
        assert!(report.frame_count >= 2);
    }
}

#[test]
fn eval_hostile_flac_inputs_never_panic() {
    // Empty data
    let report = diagnose_flac_stream(&[]);
    assert!(!report.valid_marker);
    assert!(!report.errors.is_empty());

    // Bad magic
    let report = diagnose_flac_stream(b"RIFF\x00\x00\x00\x00WAVE");
    assert!(!report.valid_marker);
    assert!(report.errors[0].contains("Invalid FLAC stream marker"));

    // Magic only
    let report = diagnose_flac_stream(b"fLaC");
    assert!(report.valid_marker);
    assert!(!report.errors.is_empty());

    // Corrupted metadata block header (declares 50000 bytes, only 10 present)
    let mut bad_meta = b"fLaC".to_vec();
    bad_meta.extend_from_slice(&[0x00, 0x00, 0xC3, 0x50]); // block type 0, len 50000
    bad_meta.extend_from_slice(&[0u8; 10]);
    let report = diagnose_flac_stream(&bad_meta);
    assert!(report.valid_marker);
    assert!(!report.warnings.is_empty());
    assert!(report.streaminfo.is_none());

    // Illegal reserved block type 127
    let mut illegal_block = b"fLaC".to_vec();
    illegal_block.extend_from_slice(&[0x7F, 0x00, 0x00, 0x04]);
    illegal_block.extend_from_slice(&[0, 1, 2, 3]);
    let report = diagnose_flac_stream(&illegal_block);
    assert!(report.valid_marker);
    assert!(report.errors.iter().any(|e| e.contains("reserved/illegal")));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn eval_arbitrary_bytes_flac_diagnostics_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = diagnose_flac_stream(&data);
    }
}
