//! Adversarial evaluations, diagnostic reporting, and error handling for `craft-matroska`.

use std::io::Cursor;

use craft_matroska::{Error, MkvWriter, MuxOptions, TrackKind, TrackSpec, open};

fn build_synthetic_mkv() -> Vec<u8> {
    let opts = MuxOptions {
        title: Some("Evals Test Media".into()),
        writing_app: "craft-matroska-test".into(),
        ..Default::default()
    };

    let mut track_spec = TrackSpec::new(TrackKind::Video, "V_MPEG4/ISO/AVC");
    track_spec.video_size = Some((1280, 720));
    track_spec.default_duration_ns = Some(40_000_000); // 25 fps

    let mut writer = MkvWriter::new(Cursor::new(Vec::new()), vec![track_spec], opts)
        .expect("writer init should succeed");

    // Write a keyframe sample (track 0, pts 0, keyframe true, payload, duration None)
    writer
        .write_frame(0, 0, true, &[0x11, 0x22, 0x33, 0x44], None)
        .expect("write frame should succeed");

    let cursor = writer.finish().expect("finish should succeed");
    cursor.into_inner()
}

#[test]
fn eval_hostile_truncated_and_non_ebml_inputs() {
    // 1. Empty data
    let empty_data: &[u8] = &[];
    let res = open(empty_data);
    assert!(res.is_err());

    // 2. Bogus header (not EBML signature 0x1A45DFA3)
    let bogus = b"NOT_EBML_HEADER_DATA";
    let res_bogus = open(bogus.as_slice());
    assert!(res_bogus.is_err());
    assert!(matches!(res_bogus.unwrap_err(), Error::NotMatroska(_)));

    // 3. Truncated EBML header
    let truncated_ebml = &[0x1A, 0x45, 0xDF, 0xA3, 0x01];
    let res_trunc = open(truncated_ebml.as_slice());
    assert!(res_trunc.is_err());
}

#[test]
fn eval_matroska_diagnostics_on_synthetic_stream() {
    let bytes = build_synthetic_mkv();
    let mkv = open(bytes.as_slice()).expect("synthetic mkv should open cleanly");

    let report = mkv.diagnose();
    assert_eq!(report.doc_type, "matroska");
    assert_eq!(report.timestamp_scale_ns, 1_000_000);
    assert_eq!(report.track_count, 1);
    assert!(report.cluster_count >= 1);
    assert!(report.errors.is_empty());

    let track = &report.tracks[0];
    assert_eq!(track.kind, "video");
    assert_eq!(track.codec_id, "V_MPEG4/ISO/AVC");
    assert_eq!(track.sample_count, 1);
    assert_eq!(track.sync_sample_count, 1);
    assert!(!track.has_lacing);
    assert!(track.errors.is_empty());
}
