//! Evaluation and hostile input test suite for craft-h264.

use craft_h264::diagnose_h264_stream;
use proptest::prelude::*;

/// Valid synthetic Baseline SPS (176x144, 8-bit, profile 66, level 30).
const VALID_SPS_ANNEXB: [u8; 12] = [
    0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x1e, 0xda, 0x0b, 0x13, 0x90,
];

/// Valid synthetic PPS matching the SPS above.
const VALID_PPS_ANNEXB: [u8; 8] = [0x00, 0x00, 0x00, 0x01, 0x68, 0xce, 0x3c, 0x80];

/// Synthetic IDR slice header for the same stream.
const VALID_IDR_ANNEXB: [u8; 8] = [0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x00];

#[test]
fn eval_h264_diagnostics_on_annexb_stream() {
    let mut stream = Vec::new();
    stream.extend_from_slice(&VALID_SPS_ANNEXB);
    stream.extend_from_slice(&VALID_PPS_ANNEXB);
    stream.extend_from_slice(&VALID_IDR_ANNEXB);

    let report = diagnose_h264_stream(&stream);

    assert_eq!(report.sps_count, 1);
    assert_eq!(report.pps_count, 1);
    assert_eq!(report.idr_count, 1);
    assert_eq!(report.non_idr_count, 0);

    assert_eq!(report.profile_idc, Some(66), "Baseline profile 66");
    assert_eq!(report.level_idc, Some(30), "Level 3.0");
    assert_eq!(report.bit_depth, Some(8));
    assert_eq!(report.width, Some(176), "176 px width");
    assert_eq!(report.height, Some(144), "144 px height");
    assert!(!report.is_interlaced);

    assert!(
        report.errors.is_empty(),
        "clean stream has zero errors: {:?}",
        report.errors
    );
}

#[test]
fn eval_h264_length_prefixed_stream() {
    let sps_payload = &VALID_SPS_ANNEXB[4..];
    let pps_payload = &VALID_PPS_ANNEXB[4..];

    let mut stream = Vec::new();
    // 4-byte big-endian length prefix for SPS
    stream.extend_from_slice(&(sps_payload.len() as u32).to_be_bytes());
    stream.extend_from_slice(sps_payload);
    // 4-byte big-endian length prefix for PPS
    stream.extend_from_slice(&(pps_payload.len() as u32).to_be_bytes());
    stream.extend_from_slice(pps_payload);

    let report = diagnose_h264_stream(&stream);

    assert_eq!(report.sps_count, 1);
    assert_eq!(report.pps_count, 1);
    assert_eq!(report.width, Some(176));
    assert_eq!(report.height, Some(144));
    assert!(report.errors.is_empty());
}

#[test]
fn eval_hostile_h264_streams_never_panic() {
    // Empty stream
    let report = diagnose_h264_stream(&[]);
    assert!(!report.errors.is_empty());

    // Non-NAL random noise
    let junk = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
    let report = diagnose_h264_stream(&junk);
    assert!(!report.errors.is_empty());

    // Truncated start code
    let trunc = [0x00, 0x00, 0x01];
    let report = diagnose_h264_stream(&trunc);
    // Either reports error or warns about zero-length NAL
    assert!(report.sps_count == 0);

    // Corrupted SPS payload (start code + 0x67 + junk)
    let bad_sps = [0x00, 0x00, 0x00, 0x01, 0x67, 0xff, 0xff, 0xff];
    let report = diagnose_h264_stream(&bad_sps);
    assert_eq!(report.sps_count, 1);
    assert!(
        !report.errors.is_empty(),
        "must catch SPS parse error gracefully"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn eval_arbitrary_bytes_h264_diagnostics_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = diagnose_h264_stream(&data);
    }
}
