//! Adversarial evaluations, diagnostic reporting, and error handling for `craft-isobmff`.

use std::io::Cursor;

use craft_isobmff::{
    AvcConfig, Brand, Error, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions, open,
};

#[test]
fn eval_hostile_truncated_and_missing_moov_containers() {
    // 1. Completely empty source
    let empty_data: &[u8] = &[];
    let res = open(empty_data);
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), Error::NoMoov));

    // 2. Truncated box header (< 8 bytes)
    let truncated_box: &[u8] = &[0x00, 0x00, 0x00, 0x20];
    let res_trunc = open(truncated_box);
    assert!(res_trunc.is_err());
    assert!(matches!(res_trunc.unwrap_err(), Error::NoMoov));

    // 3. ftyp present, but moov is completely missing (e.g. aborted upload)
    let mut ftyp_only = Vec::new();
    ftyp_only.extend_from_slice(&20u32.to_be_bytes()); // length 20
    ftyp_only.extend_from_slice(b"ftyp");
    ftyp_only.extend_from_slice(b"isom");
    ftyp_only.extend_from_slice(&512u32.to_be_bytes()); // minor version
    ftyp_only.extend_from_slice(b"mp41");

    let res_no_moov = open(ftyp_only.as_slice());
    assert!(res_no_moov.is_err());
    assert!(matches!(res_no_moov.unwrap_err(), Error::NoMoov));
}

#[test]
fn eval_hostile_oversized_box_header_rejection() {
    // Box header claiming 64-bit size far larger than the file or allowed cap
    let mut bad_box = Vec::new();
    bad_box.extend_from_slice(&1u32.to_be_bytes()); // 1 indicates 64-bit size follows
    bad_box.extend_from_slice(b"moov");
    bad_box.extend_from_slice(&u64::MAX.to_be_bytes()); // huge size u64::MAX

    let res = open(bad_box.as_slice());
    assert!(res.is_err());
    // Should fail cleanly with Truncated, NoMoov or TooLarge without panicking
    assert!(matches!(
        res.unwrap_err(),
        Error::Truncated(_) | Error::NoMoov | Error::TooLarge(_)
    ));
}

#[test]
fn eval_isobmff_diagnostics_and_faststart_verification() {
    let opts = WriterOptions::new(Brand::Mp4);
    let mut w = Mp4Writer::new(Cursor::new(Vec::new()), opts).expect("writer init should succeed");

    // Add Video Track: AVC 1280x720, timescale 24000
    let avc = AvcConfig::new(
        vec![vec![0x67, 0x64, 0, 0x28, 1, 2]],
        vec![vec![0x68, 3]],
        4,
    );
    let v_entry = SampleEntry::avc(avc, 1280, 720);
    let track_id = w
        .add_track(TrackConfig::new(v_entry, 24000))
        .expect("add track should succeed");

    // Write 24 frames (1 second at 24fps) with keyframes every 12 frames
    for i in 0..24 {
        let is_sync = i % 12 == 0;
        let payload = vec![0xAA; 128];
        w.write_sample(
            track_id,
            WriteSample {
                data: &payload,
                duration: 1000,
                composition_offset: 0,
                is_sync,
            },
        )
        .expect("write sample should succeed");
    }

    let file_bytes = w
        .finish_faststart()
        .expect("finish_faststart should succeed")
        .into_inner();
    let mp4 = open(file_bytes.as_slice()).expect("open should succeed");

    let report = mp4.diagnose();
    assert_eq!(report.track_count, 1);
    assert!(report.is_faststart);
    assert_eq!(report.timescale, 1000);
    assert!(report.errors.is_empty());

    let track_rep = &report.tracks[0];
    assert_eq!(track_rep.timescale, 24000);
    assert_eq!(track_rep.sample_count, 24);
    assert_eq!(track_rep.sync_sample_count, 2);
    assert_eq!(track_rep.max_keyframe_interval_samples, 11);
    assert_eq!(track_rep.kind, "video");
    assert!(!track_rep.has_b_frames);
}

#[test]
fn eval_isobmff_diagnostics_detects_causal_pts_violation() {
    let opts = WriterOptions::new(Brand::Mp4);
    let mut w = Mp4Writer::new(Cursor::new(Vec::new()), opts).expect("writer init should succeed");

    let avc = AvcConfig::new(
        vec![vec![0x67, 0x64, 0, 0x28, 1, 2]],
        vec![vec![0x68, 3]],
        4,
    );
    let v_entry = SampleEntry::avc(avc, 640, 480);
    let track_id = w
        .add_track(TrackConfig::new(v_entry, 1000))
        .expect("add track should succeed");

    // Write a sample with negative composition offset (PTS < DTS)
    let payload = vec![0xAA; 64];
    w.write_sample(
        track_id,
        WriteSample {
            data: &payload,
            duration: 100,
            composition_offset: -50, // Causal violation: PTS is DTS - 50
            is_sync: true,
        },
    )
    .expect("write sample should succeed");

    let file_bytes = w
        .finish_faststart()
        .expect("finish_faststart should succeed")
        .into_inner();
    let mp4 = open(file_bytes.as_slice()).expect("open should succeed");

    let report = mp4.diagnose();
    let track_rep = &report.tracks[0];
    assert!(track_rep.has_b_frames);
    assert!(
        track_rep
            .warnings
            .iter()
            .any(|w| w.contains("causal violation") && w.contains("PTS -50 is less than DTS 0")),
        "diagnostics must warn on causal PTS < DTS anomaly, got warnings: {:?}",
        track_rep.warnings
    );
}
