//! Evaluation and hostile input test suite for craft-h264enc.

use craft_h264enc::{
    Encoder, EncoderConfig, FrameType, Packet, RateControl, YuvFrame, diagnose_encoder_output,
};
use proptest::prelude::*;

fn make_yuv(w: usize, h: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let y = vec![128u8; w * h];
    let u = vec![128u8; (w / 2) * (h / 2)];
    let v = vec![128u8; (w / 2) * (h / 2)];
    (y, u, v)
}

#[test]
fn eval_encoder_diagnostics_on_crf_stream() {
    let (w, h) = (64, 48);
    let mut cfg = EncoderConfig::new(w as u32, h as u32, 30, 1);
    cfg.rate = RateControl::Crf(23.0);
    cfg.bframes = 0; // Baseline / IP only
    let mut enc = Encoder::new(cfg.clone()).expect("init encoder");

    let (y, u, v) = make_yuv(w, h);
    let frame = YuvFrame {
        y: &y,
        u: &u,
        v: &v,
        y_stride: w,
        uv_stride: w / 2,
    };

    let mut packets = Vec::new();
    for i in 0..10 {
        packets.extend(enc.encode(&frame, i * 1000).expect("encode frame"));
    }
    packets.extend(enc.flush());

    let report = diagnose_encoder_output(&cfg, &packets);

    assert_eq!(report.packets_generated, 10);
    assert_eq!(report.idr_packets, 1);
    assert_eq!(report.p_packets, 9);
    assert_eq!(report.b_packets, 0);
    assert!(report.avg_qp > 10.0 && report.avg_qp < 40.0);
    assert!(report.total_bytes > 0);
    assert!(report.estimated_bitrate_kbps.is_some());
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
}

#[test]
fn eval_encoder_diagnostics_with_b_frames() {
    let (w, h) = (64, 48);
    let mut cfg = EncoderConfig::new(w as u32, h as u32, 30, 1);
    cfg.rate = RateControl::Crf(20.0);
    cfg.bframes = 2;
    let mut enc = Encoder::new(cfg.clone()).expect("init encoder with B-frames");

    let (y, u, v) = make_yuv(w, h);
    let frame = YuvFrame {
        y: &y,
        u: &u,
        v: &v,
        y_stride: w,
        uv_stride: w / 2,
    };

    let mut packets = Vec::new();
    for i in 0..15 {
        packets.extend(enc.encode(&frame, i * 1000).expect("encode frame"));
    }
    packets.extend(enc.flush());

    let report = diagnose_encoder_output(&cfg, &packets);

    assert_eq!(report.packets_generated, 15);
    assert_eq!(report.idr_packets, 1);
    assert!(report.b_packets > 0, "must produce B-frames when bframes=2");
    assert!(
        report.errors.is_empty(),
        "zero causality or ordering errors: {:?}",
        report.errors
    );
}

#[test]
fn eval_hostile_packet_diagnostics_catches_violations() {
    let cfg = EncoderConfig::new(64, 48, 30, 1);

    // Empty packets
    let report = diagnose_encoder_output(&cfg, &[]);
    assert_eq!(report.packets_generated, 0);
    assert!(!report.warnings.is_empty());

    // 1. First packet is not IDR
    let bad_first = vec![Packet {
        data: vec![0, 0, 0, 1, 0x41],
        pts: 0,
        dts: 0,
        keyframe: false,
        frame_type: FrameType::P,
        qp: 26,
    }];
    let report = diagnose_encoder_output(&cfg, &bad_first);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("First coded packet in stream must be an IDR"))
    );

    // 2. Causal violation: pts < dts
    let bad_timing = vec![
        Packet {
            data: vec![0, 0, 0, 1, 0x65],
            pts: 0,
            dts: 0,
            keyframe: true,
            frame_type: FrameType::Idr,
            qp: 26,
        },
        Packet {
            data: vec![0, 0, 0, 1, 0x41],
            pts: 100,
            dts: 200, // pts < dts!
            keyframe: false,
            frame_type: FrameType::P,
            qp: 26,
        },
    ];
    let report = diagnose_encoder_output(&cfg, &bad_timing);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("causal timeline violation"))
    );

    // 3. Non-monotonic DTS: dts drops backwards
    let non_monotonic = vec![
        Packet {
            data: vec![0, 0, 0, 1, 0x65],
            pts: 0,
            dts: 0,
            keyframe: true,
            frame_type: FrameType::Idr,
            qp: 26,
        },
        Packet {
            data: vec![0, 0, 0, 1, 0x41],
            pts: 200,
            dts: 200,
            keyframe: false,
            frame_type: FrameType::P,
            qp: 26,
        },
        Packet {
            data: vec![0, 0, 0, 1, 0x41],
            pts: 300,
            dts: 100, // dropped backwards!
            keyframe: false,
            frame_type: FrameType::P,
            qp: 26,
        },
    ];
    let report = diagnose_encoder_output(&cfg, &non_monotonic);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("non-monotonic DTS"))
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn eval_arbitrary_packets_diagnostics_never_panics(
        pts in -10_000i64..10_000,
        dts in -10_000i64..10_000,
        qp in 0u8..51
    ) {
        let cfg = EncoderConfig::new(64, 48, 30, 1);
        let packets = vec![Packet {
            data: vec![0, 1, 2],
            pts,
            dts,
            keyframe: true,
            frame_type: FrameType::Idr,
            qp,
        }];
        let _ = diagnose_encoder_output(&cfg, &packets);
    }
}
