//! Evaluation and hostile input test suite for craft-hevc.

use craft_hevc::diagnose_hevc_stream;
use mediacraft_bitstream::{BitWriter, escape_rbsp};
use proptest::prelude::*;

fn nal(nal_type: u8, rbsp: &[u8]) -> Vec<u8> {
    let mut out = vec![0x00, 0x00, 0x00, 0x01];
    out.push((nal_type << 1) & 0x7e);
    out.push(1); // layer 0, temporal_id_plus1 = 1
    out.extend_from_slice(&escape_rbsp(rbsp));
    out
}

fn ptl(w: &mut BitWriter) {
    w.write_bits(0, 2); // profile_space
    w.write_bits(0, 1); // tier (Main)
    w.write_bits(1, 5); // profile_idc: Main
    w.write_bits(0x6000_0000, 32); // compatibility
    w.write_bits(0b1001, 4); // progressive, interlaced, non_packed, frame_only
    w.write_bits(0, 32);
    w.write_bits(0, 11);
    w.write_bits(0, 1);
    w.write_bits(93, 8); // level 3.1 (93)
}

fn make_vps() -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_bits(0, 4);
    w.write_bits(3, 2);
    w.write_bits(0, 6);
    w.write_bits(0, 3);
    w.write_bits(1, 1);
    w.write_bits(0xffff, 16);
    ptl(&mut w);
    w.write_bits(1, 1);
    w.write_ue(4);
    w.write_ue(0);
    w.write_ue(0);
    w.write_bits(0, 6);
    w.write_ue(0);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.rbsp_trailing();
    nal(32, &w.finish())
}

fn make_sps(w_px: u32, h_px: u32) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_bits(0, 4);
    w.write_bits(0, 3);
    w.write_bits(1, 1);
    ptl(&mut w);
    w.write_ue(0); // sps id 0
    w.write_ue(1); // 4:2:0
    w.write_ue(w_px);
    w.write_ue(h_px);
    w.write_bits(0, 1); // no conformance window
    w.write_ue(0); // bit_depth_luma_minus8 = 0 (8 bits)
    w.write_ue(0); // bit_depth_chroma_minus8 = 0
    w.write_ue(4); // log2_max_poc_lsb = 8
    w.write_bits(1, 1);
    w.write_ue(4);
    w.write_ue(0);
    w.write_ue(0);
    w.write_ue(0); // min CB 8
    w.write_ue(1); // CTB 16
    w.write_ue(0); // min TB 4
    w.write_ue(2); // max TB 16
    w.write_ue(0);
    w.write_ue(0);
    w.write_bits(0, 1); // scaling lists
    w.write_bits(0, 1); // amp
    w.write_bits(0, 1); // sao
    w.write_bits(0, 1); // no pcm
    w.write_ue(0); // num_short_term_ref_pic_sets
    w.write_bits(0, 1); // long_term_refs_present
    w.write_bits(0, 1); // temporal mvp
    w.write_bits(0, 1); // strong intra smoothing
    w.write_bits(0, 1); // vui
    w.write_bits(0, 1); // extensions
    w.rbsp_trailing();
    nal(33, &w.finish())
}

fn make_pps() -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_ue(0); // pps id
    w.write_ue(0); // sps id
    w.write_bits(0, 1); // dependent slice segments
    w.write_bits(0, 1); // output_flag_present
    w.write_bits(0, 3); // num_extra_slice_header_bits
    w.write_bits(0, 1); // sign_data_hiding
    w.write_bits(0, 1); // cabac_init_present
    w.write_ue(0); // num_ref_idx_l0_default_active_minus1
    w.write_ue(0); // num_ref_idx_l1_default_active_minus1
    w.write_se(0); // init_qp_minus26
    w.write_bits(0, 1); // constrained_intra_pred
    w.write_bits(0, 1); // transform_skip_enabled
    w.write_bits(0, 1); // cu_qp_delta_enabled
    w.write_se(0); // cb_qp_offset
    w.write_se(0); // cr_qp_offset
    w.write_bits(0, 1); // slice_chroma_qp_offsets_present
    w.write_bits(0, 1); // weighted_pred
    w.write_bits(0, 1); // weighted_bipred
    w.write_bits(0, 1); // transquant_bypass_enabled
    w.write_bits(0, 1); // tiles_enabled
    w.write_bits(0, 1); // entropy_coding_sync_enabled
    w.write_bits(0, 1); // loop_filter_across_slices_enabled
    w.write_bits(0, 1); // deblocking_filter_control_present
    w.write_bits(0, 1); // scaling_list_data_present
    w.write_bits(0, 1); // lists_modification_present
    w.write_ue(0); // log2_parallel_merge_level_minus2
    w.write_bits(0, 1); // slice_segment_header_extension_present
    w.write_bits(0, 1); // pps_extension_present
    w.rbsp_trailing();
    nal(34, &w.finish())
}

#[test]
fn eval_hevc_clean_stream_diagnostics() {
    let mut stream = Vec::new();
    stream.extend_from_slice(&make_vps());
    stream.extend_from_slice(&make_sps(128, 96));
    stream.extend_from_slice(&make_pps());

    // Synthetic IDR_W_RADL slice NAL (nal_type = 19)
    let idr_slice = nal(19, &[0x80, 0x00]);
    stream.extend_from_slice(&idr_slice);

    let report = diagnose_hevc_stream(&stream);

    assert_eq!(report.vps_count, 1);
    assert_eq!(report.sps_count, 1);
    assert_eq!(report.pps_count, 1);
    assert_eq!(report.irap_count, 1);
    assert_eq!(report.trail_count, 0);

    assert_eq!(report.width, Some(128));
    assert_eq!(report.height, Some(96));
    assert_eq!(report.profile_idc, Some(1), "Main profile 1");
    assert_eq!(report.is_high_tier, Some(false));
    assert_eq!(report.level_idc, Some(93));
    assert_eq!(report.bit_depth, Some(8));
    assert_eq!(report.chroma_format_idc, Some(1));

    assert!(
        report.errors.is_empty(),
        "clean stream has zero errors: {:?}",
        report.errors
    );
}

#[test]
fn eval_hevc_length_prefixed_format() {
    let vps_nal = make_vps();
    let sps_nal = make_sps(160, 120);
    let pps_nal = make_pps();

    let vps_payload = &vps_nal[4..];
    let sps_payload = &sps_nal[4..];
    let pps_payload = &pps_nal[4..];

    let mut stream = Vec::new();
    stream.extend_from_slice(&(vps_payload.len() as u32).to_be_bytes());
    stream.extend_from_slice(vps_payload);
    stream.extend_from_slice(&(sps_payload.len() as u32).to_be_bytes());
    stream.extend_from_slice(sps_payload);
    stream.extend_from_slice(&(pps_payload.len() as u32).to_be_bytes());
    stream.extend_from_slice(pps_payload);

    let report = diagnose_hevc_stream(&stream);

    assert_eq!(report.vps_count, 1);
    assert_eq!(report.sps_count, 1);
    assert_eq!(report.pps_count, 1);
    assert_eq!(report.width, Some(160));
    assert_eq!(report.height, Some(120));
    assert!(report.errors.is_empty());
}

#[test]
fn eval_hostile_hevc_inputs_never_panic() {
    // Empty
    let report = diagnose_hevc_stream(&[]);
    assert!(!report.errors.is_empty());

    // Non-NAL random data
    let junk = [0x55, 0xaa, 0x12, 0x34];
    let report = diagnose_hevc_stream(&junk);
    assert!(!report.errors.is_empty());

    // Sub-2-byte NAL
    let mut tiny_nal = vec![0x00, 0x00, 0x01];
    tiny_nal.push(0x40); // 1 byte only
    let report = diagnose_hevc_stream(&tiny_nal);
    assert!(report.sps_count == 0);

    // Corrupted SPS RBSP
    let bad_sps = nal(33, &[0xff, 0xff]);
    let report = diagnose_hevc_stream(&bad_sps);
    assert_eq!(report.sps_count, 1);
    assert!(
        !report.errors.is_empty(),
        "must catch SPS parse error gracefully"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn eval_arbitrary_bytes_hevc_diagnostics_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = diagnose_hevc_stream(&data);
    }
}
