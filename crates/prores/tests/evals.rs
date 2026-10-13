//! Evaluation and hostile input test suite for craft-prores.

use craft_prores::{
    ChromaFormat, ColorInfo, Encoder, EncoderConfig, Frame, Interlace, Profile,
    diagnose_prores_frame,
};
use proptest::prelude::*;

fn make_test_frame_422(w: u32, h: u32) -> Frame {
    let y_len = (w * h) as usize;
    let c_len = (w.div_ceil(2) * h) as usize;

    let y = vec![512u16; y_len];
    let cb = vec![512u16; c_len];
    let cr = vec![512u16; c_len];

    Frame {
        width: w,
        height: h,
        chroma: ChromaFormat::Yuv422,
        bit_depth: 10,
        y,
        cb,
        cr,
        alpha: None,
        interlace: Interlace::Progressive,
        color: ColorInfo {
            primaries: 1,
            transfer: 1,
            matrix: 1,
        },
        aspect_ratio: 0,
        frame_rate_code: 0,
    }
}

fn make_test_frame_444_alpha(w: u32, h: u32) -> Frame {
    let len = (w * h) as usize;
    let y = vec![1024u16; len];
    let cb = vec![2048u16; len];
    let cr = vec![2048u16; len];
    let alpha = vec![4095u16; len];

    Frame {
        width: w,
        height: h,
        chroma: ChromaFormat::Yuv444,
        bit_depth: 12,
        y,
        cb,
        cr,
        alpha: Some(alpha),
        interlace: Interlace::Progressive,
        color: ColorInfo {
            primaries: 1,
            transfer: 1,
            matrix: 1,
        },
        aspect_ratio: 0,
        frame_rate_code: 0,
    }
}

#[test]
fn eval_prores_clean_frame_diagnostics() {
    let frame = make_test_frame_422(320, 240);
    let mut enc = Encoder::new(Profile::Hq, 320, 240);
    let sample = enc.encode(&frame).expect("encode 422 HQ frame");

    let report = diagnose_prores_frame(&sample);

    assert!(report.valid_marker, "must have icpf atom");
    assert_eq!(report.frame_size_bytes, sample.len());
    assert_eq!(report.width, 320);
    assert_eq!(report.height, 240);
    assert_eq!(report.chroma_format.as_deref(), Some("4:2:2"));
    assert_eq!(report.interlace.as_deref(), Some("Progressive"));
    assert_eq!(report.encoder_id.as_deref(), Some("fmcr"));
    assert!(!report.has_alpha);
    assert_eq!(report.pictures_count, 1);
    assert!(report.total_slices > 0);
    assert!(
        report.errors.is_empty(),
        "clean frame has zero errors: {:?}",
        report.errors
    );
    assert!(
        report.warnings.is_empty(),
        "clean frame has zero warnings: {:?}",
        report.warnings
    );
}

#[test]
fn eval_prores_4444_alpha_frame_diagnostics() {
    let frame = make_test_frame_444_alpha(160, 120);
    let mut cfg = EncoderConfig::new(Profile::P4444, 160, 120);
    cfg.encode_alpha = true;
    let mut enc = Encoder::with_config(cfg);
    let sample = enc.encode(&frame).expect("encode 4444 with alpha");

    let report = diagnose_prores_frame(&sample);

    assert!(report.valid_marker);
    assert_eq!(report.width, 160);
    assert_eq!(report.height, 120);
    assert_eq!(report.chroma_format.as_deref(), Some("4:4:4"));
    assert!(report.has_alpha);
    assert_eq!(report.alpha_type, "16-bit");
    assert_eq!(report.pictures_count, 1);
    assert!(report.errors.is_empty());
}

#[test]
fn eval_hostile_prores_inputs_never_panic() {
    // Zero-length
    let report = diagnose_prores_frame(&[]);
    assert!(!report.valid_marker);
    assert!(!report.errors.is_empty());

    // 4 bytes only
    let report = diagnose_prores_frame(&[0, 0, 0, 10]);
    assert!(!report.valid_marker);
    assert!(!report.errors.is_empty());

    // Wrong atom identifier
    let report = diagnose_prores_frame(b"\x00\x00\x00\x20mdat123456789012345678901234");
    assert!(!report.valid_marker);
    assert!(report.errors.iter().any(|e| e.contains("Missing 'icpf'")));

    // Giant frame size declaration exceeding data
    let mut giant = b"\x00\x10\x00\x00icpf".to_vec();
    giant.extend_from_slice(&[0u8; 32]);
    let report = diagnose_prores_frame(&giant);
    assert!(report.valid_marker);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("exceeds available data"))
    );

    // Sub-minimum frame size
    let tiny = b"\x00\x00\x00\x10icpf\x00\x00\x00\x00\x00\x00\x00\x00";
    let report = diagnose_prores_frame(tiny);
    assert!(report.valid_marker);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("below minimum valid size"))
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn eval_arbitrary_bytes_prores_diagnostics_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = diagnose_prores_frame(&data);
    }
}
