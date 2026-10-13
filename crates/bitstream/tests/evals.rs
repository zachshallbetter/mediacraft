//! Comprehensive evaluation suite for `mediacraft-bitstream`.
//!
//! Validates:
//! 1. Hostile inputs never panic bit readers or writers.
//! 2. Exp-Golomb bounds and truncation behaviors.
//! 3. Emulation prevention byte escaping/unescaping invariants.
//! 4. Annex-B and length-prefixed NAL extraction safety.
//! 5. ISOBMFF box parser robustness under truncated/oversized/cyclic structures.
//! 6. Diagnostics generation across valid and invalid containers.

use mediacraft_bitstream::{
    BitError, BitReader, BitWriter, annexb_nals, crc32, diagnose_bitstream,
    diagnose_isobmff_stream, escape_rbsp,
    isobmff::{parse_boxes, reorganize_to_faststart},
    length_prefixed_nals, unescape_rbsp,
};
use proptest::prelude::*;

#[test]
fn eval_bit_reader_boundary_conditions() {
    // Empty data
    let mut reader = BitReader::new(&[]);
    assert_eq!(reader.bits_left(), 0);
    assert!(reader.is_byte_aligned());
    assert_eq!(reader.read_bits(1), Err(BitError::Eof));
    assert_eq!(reader.read_bit(), Err(BitError::Eof));
    assert_eq!(reader.read_ue(), Err(BitError::Eof));
    assert_eq!(reader.read_se(), Err(BitError::Eof));
    assert_eq!(reader.peek(8), 0);

    // Single byte boundary checks
    let single = [0b1011_0001];
    let mut reader = BitReader::new(&single);
    assert_eq!(reader.read_bits(0).unwrap(), 0);
    assert_eq!(reader.bits_left(), 8);
    assert!(reader.read_bit().unwrap());
    assert_eq!(reader.bits_left(), 7);
    assert!(!reader.is_byte_aligned());
    reader.byte_align();
    assert!(reader.is_byte_aligned());
    assert_eq!(reader.bits_left(), 0);
    assert_eq!(reader.read_bit(), Err(BitError::Eof));
}

#[test]
fn eval_exp_golomb_overflow_and_pathological_inputs() {
    // Pathological sequence with 33 leading zero bits without a terminal 1 bit
    let zeroes = [0u8; 10];
    let mut reader = BitReader::new(&zeroes);
    assert_eq!(reader.read_ue(), Err(BitError::InvalidExpGolomb));

    // Valid Exp-Golomb extreme value
    let mut writer = BitWriter::new();
    writer.write_ue(1_000_000);
    writer.write_se(-1_000_000);
    let bytes = writer.finish();

    let mut reader = BitReader::new(&bytes);
    assert_eq!(reader.read_ue().unwrap(), 1_000_000);
    assert_eq!(reader.read_se().unwrap(), -1_000_000);
}

#[test]
fn eval_emulation_prevention_hostile_sequences() {
    // Sequence of zeroes followed by 0x03
    let hostile = [0, 0, 0, 0, 0, 3, 0, 0, 1, 0, 0, 2];
    let escaped = escape_rbsp(&hostile);
    let unescaped = unescape_rbsp(&escaped);
    assert_eq!(unescaped, hostile);

    // Pathological unescape input ending prematurely
    let premature = [0, 0, 3];
    assert_eq!(unescape_rbsp(&premature), vec![0, 0]);

    let trailing_zeros = [0, 0, 0, 0];
    assert_eq!(unescape_rbsp(&trailing_zeros), vec![0, 0, 0, 0]);
}

#[test]
fn eval_annexb_and_length_prefixed_hostile_delimiters() {
    // Completely malformed stream
    let garbage = [0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
    assert!(annexb_nals(&garbage).is_empty());

    // Truncated length-prefixed stream
    let truncated_len = [0x00, 0x00, 0x04, 0xAA]; // header specifies 4 bytes, only 1 provided
    assert_eq!(length_prefixed_nals(&truncated_len, 4), Err(BitError::Eof));

    // Zero-byte length-prefixed stream
    assert!(length_prefixed_nals(&[], 4).unwrap().is_empty());

    // Multiple 3-byte and 4-byte start codes mixed
    let mixed = [
        0, 0, 1, 0x67, 1, 2, // 3-byte prefix
        0, 0, 0, 1, 0x68, 3, 4, // 4-byte prefix
        0, 0, 1, 0x65, // 3-byte prefix
    ];
    let nals = annexb_nals(&mixed);
    assert_eq!(nals.len(), 3);
    assert_eq!(nals[0], &[0x67, 1, 2]);
    assert_eq!(nals[1], &[0x68, 3, 4]);
    assert_eq!(nals[2], &[0x65]);
}

#[test]
fn eval_isobmff_corrupt_box_parsing_and_diagnostics() {
    // 0 bytes
    let diag = diagnose_isobmff_stream(&[]);
    assert_eq!(diag.box_count, 0);

    // Box size claiming to be 0 (extends to EOF)
    let box_to_eof = [0x00, 0x00, 0x00, 0x00, b'f', b'r', b'e', b'e', 1, 2, 3, 4];
    let boxes = parse_boxes(&box_to_eof).unwrap();
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].total_size, 12);

    // Box size less than 8 (invalid)
    let invalid_size = [0x00, 0x00, 0x00, 0x04, b'f', b'r', b'e', b'e'];
    assert!(parse_boxes(&invalid_size).is_err());

    // Faststart reorganization on buffer lacking moov
    let no_moov = [0x00, 0x00, 0x00, 0x08, b'm', b'd', b'a', b't'];
    assert!(reorganize_to_faststart(&no_moov).is_err());

    // Full bitstream diagnostics on synthetic data
    let report = diagnose_bitstream(&[0, 0, 1, 0x67, 0x42, 0x00, 0x1F]);
    assert!(report.valid);
    assert_eq!(report.annexb_nal_count, 1);
    assert!(report.isobmff.is_none());

    // CRC32 check
    assert_eq!(crc32(&[]), 0x0000_0000);
}

proptest! {
    #[test]
    fn prop_bit_reader_never_panics_on_arbitrary_input(
        data in proptest::collection::vec(any::<u8>(), 0..256),
        reads in proptest::collection::vec(1u32..=32, 0..50)
    ) {
        let mut reader = BitReader::new(&data);
        for n in reads {
            let _ = reader.read_bits(n);
        }
        let _ = reader.read_ue();
        let _ = reader.read_se();
        let _ = reader.more_rbsp_data();
    }

    #[test]
    fn prop_diagnostics_never_panic_on_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        let _ = diagnose_bitstream(&data);
        let _ = diagnose_isobmff_stream(&data);
        let _ = annexb_nals(&data);
        let _ = length_prefixed_nals(&data, 4);
    }
}
