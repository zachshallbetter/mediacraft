//! Evaluation and hostile input test suite for craft-opus.

use craft_opus::{diagnose_opus_head, diagnose_opus_packet, diagnose_opus_stream};
use proptest::prelude::*;

fn make_opus_head_ogg(channels: u8, pre_skip: u16, sample_rate: u32, family: u8) -> Vec<u8> {
    let mut v = b"OpusHead".to_vec();
    v.push(1); // version 1
    v.push(channels);
    v.extend_from_slice(&pre_skip.to_le_bytes());
    v.extend_from_slice(&sample_rate.to_le_bytes());
    v.extend_from_slice(&0i16.to_le_bytes()); // output gain 0 dB
    v.push(family);
    if family == 1 {
        // 5.1 surround layout: 6 channels, 4 streams, 2 coupled
        v.push(4); // streams
        v.push(2); // coupled
        v.extend_from_slice(&[0, 4, 1, 2, 3, 5]);
    }
    v
}

fn make_dops_mp4(channels: u8, pre_skip: u16, sample_rate: u32) -> Vec<u8> {
    let mut v = Vec::new();
    v.push(0); // version 0
    v.push(channels);
    v.extend_from_slice(&pre_skip.to_be_bytes());
    v.extend_from_slice(&sample_rate.to_be_bytes());
    v.extend_from_slice(&0i16.to_be_bytes());
    v.push(0); // family 0
    v
}

#[test]
fn eval_opus_head_diagnostics() {
    // 1. Ogg stereo
    let ogg_head = make_opus_head_ogg(2, 312, 48000, 0);
    let diag = diagnose_opus_head(&ogg_head).expect("parse ogg head");
    assert_eq!(diag.channels, 2);
    assert_eq!(diag.pre_skip, 312);
    assert_eq!(diag.input_sample_rate, 48000);
    assert_eq!(diag.mapping_family, 0);
    assert_eq!(diag.streams, 1);
    assert_eq!(diag.coupled_streams, 1);
    assert_eq!(diag.output_gain_db, 0.0);

    // 2. 5.1 surround
    let surround_head = make_opus_head_ogg(6, 384, 48000, 1);
    let s_diag = diagnose_opus_head(&surround_head).expect("parse surround head");
    assert_eq!(s_diag.channels, 6);
    assert_eq!(s_diag.mapping_family, 1);
    assert_eq!(s_diag.streams, 4);
    assert_eq!(s_diag.coupled_streams, 2);
    assert_eq!(s_diag.channel_mapping, vec![0, 4, 1, 2, 3, 5]);

    // 3. MP4 dOps box payload
    let dops = make_dops_mp4(2, 312, 48000);
    let d_diag = diagnose_opus_head(&dops).expect("parse dops box");
    assert_eq!(d_diag.channels, 2);
    assert_eq!(d_diag.pre_skip, 312);
    assert_eq!(d_diag.input_sample_rate, 48000);
}

#[test]
fn eval_opus_packet_diagnostics() {
    // CELT-only fullband stereo: config 31 = (31 << 3) | (1 << 2) | 0 = 0xFC | 0x04 = 0xFC
    // Code 0: 1 frame
    let celt_packet = [0xFC, 0x00, 0x11, 0x22, 0x33];
    let diag = diagnose_opus_packet(&celt_packet).expect("parse celt packet");
    assert_eq!(diag.mode, "CeltOnly");
    assert_eq!(diag.bandwidth, "Fullband");
    assert!(diag.is_stereo);
    assert_eq!(diag.frame_count, 1);
    assert_eq!(diag.samples_per_frame_48k, 960); // 20ms at 48k = 960 samples
    assert_eq!(diag.duration_ms, 20.0);

    // SILK-only narrowband mono: config 0 = (0 << 3) | 0 = 0x00
    // Code 0: 1 frame (10ms = 480 samples at 48k)
    let silk_packet = [0x00, 0x55, 0x66];
    let s_diag = diagnose_opus_packet(&silk_packet).expect("parse silk packet");
    assert_eq!(s_diag.mode, "SilkOnly");
    assert_eq!(s_diag.bandwidth, "Narrowband");
    assert!(!s_diag.is_stereo);
    assert_eq!(s_diag.duration_ms, 10.0);

    // Stream diagnostic over both packets
    let head = make_opus_head_ogg(2, 312, 48000, 0);
    let packets: [&[u8]; 2] = [&celt_packet, &silk_packet];
    let report = diagnose_opus_stream(Some(&head), &packets);
    assert!(report.head.is_some());
    assert_eq!(report.packets_inspected, 2);
    assert!(report.modes_detected.contains(&"CeltOnly".to_string()));
    assert!(report.modes_detected.contains(&"SilkOnly".to_string()));
    assert!(report.errors.is_empty());
}

#[test]
fn eval_hostile_opus_inputs_never_panic() {
    // Truncated header
    assert!(diagnose_opus_head(&[]).is_err());
    assert!(diagnose_opus_head(b"OpusHead\x01").is_err());

    // Bad magic
    assert!(
        diagnose_opus_head(b"fLaC\x00\x00\x00\x22\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00")
            .is_err()
    );

    // Empty packet
    assert!(diagnose_opus_packet(&[]).is_err());

    // Corrupted multi-frame packet (code 3 declares frames but no sizes)
    let bad_code3 = [0x03];
    assert!(diagnose_opus_packet(&bad_code3).is_err());

    // Stream diagnostic with broken inputs
    let report = diagnose_opus_stream(Some(b"bad_head"), &[&[], &[0x03]]);
    assert!(!report.errors.is_empty(), "bad header caught as error");
    assert_eq!(
        report.warnings.len(),
        2,
        "2 bad packets reported as warnings"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn eval_arbitrary_bytes_opus_diagnostics_never_panics(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let _ = diagnose_opus_head(&data);
        let _ = diagnose_opus_packet(&data);
        let _ = diagnose_opus_stream(Some(&data), &[&data]);
    }
}
