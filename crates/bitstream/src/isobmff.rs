//! ISO Base Media File Format (ISOBMFF / ISO/IEC 14496-12) container tools.
//!
//! Provides:
//! - Pure-Rust faststart reorganizer (moving `moov` atom ahead of `mdat` and updating `stco`/`co64` chunk offsets).
//! - Box reader and walker.
//! - Minimal compliant M4A/MP4 container generator with `edts`/`elst` edit list support for AAC priming compensation.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsoBmffError {
    UnexpectedEof,
    InvalidBoxSize,
    MissingBox(&'static str),
    OffsetOverflow,
    MalformedStructure(String),
}

impl fmt::Display for IsoBmffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of ISOBMFF stream"),
            Self::InvalidBoxSize => f.write_str("invalid box size (less than header)"),
            Self::MissingBox(name) => write!(f, "missing required box: {name}"),
            Self::OffsetOverflow => f.write_str("chunk offset overflowed 32-bit integer"),
            Self::MalformedStructure(msg) => write!(f, "malformed ISOBMFF box: {msg}"),
        }
    }
}

impl std::error::Error for IsoBmffError {}

pub type Result<T> = std::result::Result<T, IsoBmffError>;

/// A parsed top-level or child box header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxEntry {
    pub start: usize,
    pub header_len: usize,
    pub total_size: usize,
    pub fourcc: [u8; 4],
}

impl BoxEntry {
    pub fn payload_start(&self) -> usize {
        self.start.saturating_add(self.header_len)
    }

    pub fn payload_end(&self) -> usize {
        self.start.saturating_add(self.total_size)
    }

    pub fn fourcc_str(&self) -> String {
        self.fourcc.iter().map(|&b| b as char).collect()
    }
}

/// Parse top-level boxes in an ISOBMFF buffer.
pub fn parse_boxes(data: &[u8]) -> Result<Vec<BoxEntry>> {
    let mut boxes = Vec::new();
    let mut pos = 0usize;

    while pos.saturating_add(8) <= data.len() {
        let size_u32 = u32::from_be_bytes([
            *data.get(pos).unwrap_or(&0),
            *data.get(pos + 1).unwrap_or(&0),
            *data.get(pos + 2).unwrap_or(&0),
            *data.get(pos + 3).unwrap_or(&0),
        ]);

        let fourcc = [
            *data.get(pos + 4).unwrap_or(&0),
            *data.get(pos + 5).unwrap_or(&0),
            *data.get(pos + 6).unwrap_or(&0),
            *data.get(pos + 7).unwrap_or(&0),
        ];

        let (header_len, total_size) = if size_u32 == 1 {
            if pos.saturating_add(16) > data.len() {
                return Err(IsoBmffError::UnexpectedEof);
            }
            let mut s = [0u8; 8];
            for (idx, b) in s.iter_mut().enumerate() {
                *b = *data.get(pos + 8 + idx).unwrap_or(&0);
            }
            let total_u64 = u64::from_be_bytes(s);
            let total = usize::try_from(total_u64).map_err(|_| IsoBmffError::InvalidBoxSize)?;
            (16usize, total)
        } else if size_u32 == 0 {
            (8usize, data.len().saturating_sub(pos))
        } else {
            (8usize, size_u32 as usize)
        };

        if total_size < header_len {
            return Err(IsoBmffError::InvalidBoxSize);
        }

        boxes.push(BoxEntry {
            start: pos,
            header_len,
            total_size,
            fourcc,
        });

        pos = pos.saturating_add(total_size);
    }

    Ok(boxes)
}

/// Recursively find all `stco` and `co64` boxes within a slice (typically the `moov` payload)
/// and update their offsets by adding `shift`.
pub fn patch_chunk_offsets(moov_bytes: &mut [u8], shift: u64) -> Result<usize> {
    let mut patched_boxes = 0usize;
    let mut i = 0usize;

    // Scan for `stco` or `co64` fourcc
    while i.saturating_add(8) <= moov_bytes.len() {
        let fourcc = [
            *moov_bytes.get(i + 4).unwrap_or(&0),
            *moov_bytes.get(i + 5).unwrap_or(&0),
            *moov_bytes.get(i + 6).unwrap_or(&0),
            *moov_bytes.get(i + 7).unwrap_or(&0),
        ];

        if &fourcc == b"stco" {
            // stco structure:
            // 4 bytes: size
            // 4 bytes: 'stco'
            // 1 byte: version
            // 3 bytes: flags
            // 4 bytes: entry_count
            // entry_count * 4 bytes: offsets
            let header_len = 16usize;
            if i.saturating_add(header_len) <= moov_bytes.len() {
                let count = u32::from_be_bytes([
                    *moov_bytes.get(i + 12).unwrap_or(&0),
                    *moov_bytes.get(i + 13).unwrap_or(&0),
                    *moov_bytes.get(i + 14).unwrap_or(&0),
                    *moov_bytes.get(i + 15).unwrap_or(&0),
                ]) as usize;

                let table_end = i
                    .saturating_add(header_len)
                    .saturating_add(count.saturating_mul(4));
                if table_end <= moov_bytes.len() {
                    for entry_idx in 0..count {
                        let offset_pos = i + header_len + entry_idx * 4;
                        let old_offset = u32::from_be_bytes([
                            *moov_bytes.get(offset_pos).unwrap_or(&0),
                            *moov_bytes.get(offset_pos + 1).unwrap_or(&0),
                            *moov_bytes.get(offset_pos + 2).unwrap_or(&0),
                            *moov_bytes.get(offset_pos + 3).unwrap_or(&0),
                        ]) as u64;

                        let new_offset_u64 = old_offset.saturating_add(shift);
                        let new_offset = u32::try_from(new_offset_u64)
                            .map_err(|_| IsoBmffError::OffsetOverflow)?;
                        let new_bytes = new_offset.to_be_bytes();

                        for (b_idx, &b) in new_bytes.iter().enumerate() {
                            if let Some(target) = moov_bytes.get_mut(offset_pos + b_idx) {
                                *target = b;
                            }
                        }
                    }
                    patched_boxes = patched_boxes.saturating_add(1);
                    i = table_end;
                    continue;
                }
            }
        } else if &fourcc == b"co64" {
            // co64 structure:
            // 4 bytes: size
            // 4 bytes: 'co64'
            // 1 byte: version
            // 3 bytes: flags
            // 4 bytes: entry_count
            // entry_count * 8 bytes: offsets
            let header_len = 16usize;
            if i.saturating_add(header_len) <= moov_bytes.len() {
                let count = u32::from_be_bytes([
                    *moov_bytes.get(i + 12).unwrap_or(&0),
                    *moov_bytes.get(i + 13).unwrap_or(&0),
                    *moov_bytes.get(i + 14).unwrap_or(&0),
                    *moov_bytes.get(i + 15).unwrap_or(&0),
                ]) as usize;

                let table_end = i
                    .saturating_add(header_len)
                    .saturating_add(count.saturating_mul(8));
                if table_end <= moov_bytes.len() {
                    for entry_idx in 0..count {
                        let offset_pos = i + header_len + entry_idx * 8;
                        let mut old_bytes = [0u8; 8];
                        for (idx, b) in old_bytes.iter_mut().enumerate() {
                            *b = *moov_bytes.get(offset_pos + idx).unwrap_or(&0);
                        }
                        let old_offset = u64::from_be_bytes(old_bytes);
                        let new_offset = old_offset.saturating_add(shift);
                        let new_bytes = new_offset.to_be_bytes();

                        for (b_idx, &b) in new_bytes.iter().enumerate() {
                            if let Some(target) = moov_bytes.get_mut(offset_pos + b_idx) {
                                *target = b;
                            }
                        }
                    }
                    patched_boxes = patched_boxes.saturating_add(1);
                    i = table_end;
                    continue;
                }
            }
        }

        i = i.saturating_add(1);
    }

    Ok(patched_boxes)
}

/// Reorganize an ISOBMFF stream so that the `moov` box precedes the `mdat` box (Web Faststart).
///
/// If `moov` is already before `mdat`, returns the data unchanged.
/// If `mdat` is before `moov`, shifts `moov` before `mdat` and adjusts all `stco` / `co64` offsets.
pub fn reorganize_to_faststart(data: &[u8]) -> Result<Vec<u8>> {
    let boxes = parse_boxes(data)?;

    let mut ftyp_box = None;
    let mut moov_box = None;
    let mut mdat_box = None;

    for b in &boxes {
        if &b.fourcc == b"ftyp" && ftyp_box.is_none() {
            ftyp_box = Some(b.clone());
        } else if &b.fourcc == b"moov" && moov_box.is_none() {
            moov_box = Some(b.clone());
        } else if &b.fourcc == b"mdat" && mdat_box.is_none() {
            mdat_box = Some(b.clone());
        }
    }

    let moov = moov_box.ok_or(IsoBmffError::MissingBox("moov"))?;
    let mdat = mdat_box.ok_or(IsoBmffError::MissingBox("mdat"))?;

    // If moov is already ahead of mdat, no reorganization required
    if moov.start < mdat.start {
        return Ok(data.to_vec());
    }

    // Extract moov box bytes
    let moov_end = moov.start.saturating_add(moov.total_size);
    if moov_end > data.len() {
        return Err(IsoBmffError::UnexpectedEof);
    }
    let mut moov_bytes = data
        .get(moov.start..moov_end)
        .ok_or(IsoBmffError::UnexpectedEof)?
        .to_vec();

    // Patch chunk offsets inside moov by adding moov.total_size
    let shift = moov.total_size as u64;
    patch_chunk_offsets(&mut moov_bytes, shift)?;

    // Construct new buffer:
    // 1. All boxes prior to mdat (typically ftyp)
    // 2. Patched moov box
    // 3. mdat box and any other non-moov boxes
    let mut output = Vec::with_capacity(data.len());

    let mdat_start = mdat.start;
    if let Some(prefix) = data.get(0..mdat_start) {
        output.extend_from_slice(prefix);
    }

    // Append moov
    output.extend_from_slice(&moov_bytes);

    // Append remaining boxes (skipping the original moov range)
    for b in &boxes {
        if b.start < mdat_start {
            continue;
        }
        if &b.fourcc == b"moov" {
            continue;
        }
        let b_end = b.start.saturating_add(b.total_size);
        if let Some(slice) = data.get(b.start..b_end) {
            output.extend_from_slice(slice);
        }
    }

    // Append any trailing unboxed bytes after the last box
    if let Some(last_box) = boxes.last() {
        let last_end = last_box.start.saturating_add(last_box.total_size);
        if let Some(trailing) = data.get(last_end..)
            && !trailing.is_empty()
        {
            output.extend_from_slice(trailing);
        }
    }

    Ok(output)
}

/// Helper to construct a box with automatic size calculation.
fn write_box(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = 8u32.saturating_add(payload.len() as u32);
    let mut b = Vec::with_capacity(size as usize);
    b.extend_from_slice(&size.to_be_bytes());
    b.extend_from_slice(fourcc);
    b.extend_from_slice(payload);
    b
}

/// Constructs a minimal valid M4A container with AAC audio and optional edit list (`elst`) priming compensation.
pub fn build_synthetic_m4a(
    audio_data: &[u8],
    sample_rate: u32,
    channels: u16,
    priming_samples: u32,
    faststart: bool,
) -> Vec<u8> {
    let mut ftyp_payload = Vec::new();
    ftyp_payload.extend_from_slice(b"M4A ");
    ftyp_payload.extend_from_slice(&0u32.to_be_bytes());
    ftyp_payload.extend_from_slice(b"M4A ");
    ftyp_payload.extend_from_slice(b"isom");
    ftyp_payload.extend_from_slice(b"mp42");
    let ftyp_box = write_box(b"ftyp", &ftyp_payload);
    let ftyp_size = ftyp_box.len() as u32;

    let mdat_box = write_box(b"mdat", audio_data);

    // Build moov box
    let mut moov_payload = Vec::new();

    // mvhd (Movie Header)
    let mut mvhd_payload = Vec::new();
    mvhd_payload.extend_from_slice(&[0u8; 4]); // version & flags
    mvhd_payload.extend_from_slice(&0u32.to_be_bytes()); // creation time
    mvhd_payload.extend_from_slice(&0u32.to_be_bytes()); // mod time
    mvhd_payload.extend_from_slice(&sample_rate.to_be_bytes()); // timescale
    mvhd_payload.extend_from_slice(&1000u32.to_be_bytes()); // duration
    mvhd_payload.extend_from_slice(&0x00010000u32.to_be_bytes()); // rate 1.0
    mvhd_payload.extend_from_slice(&0x0100u16.to_be_bytes()); // volume 1.0
    mvhd_payload.extend_from_slice(&[0u8; 10]); // reserved
    let matrix: [u32; 9] = [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000];
    for val in matrix {
        mvhd_payload.extend_from_slice(&val.to_be_bytes());
    }
    mvhd_payload.extend_from_slice(&[0u8; 24]); // pre_defined
    mvhd_payload.extend_from_slice(&2u32.to_be_bytes()); // next_track_id
    moov_payload.extend_from_slice(&write_box(b"mvhd", &mvhd_payload));

    // trak (Track Box)
    let mut trak_payload = Vec::new();

    // tkhd (Track Header)
    let mut tkhd_payload = Vec::new();
    tkhd_payload.extend_from_slice(&[0u8, 0, 0, 0x0f]); // version 0, flags = enabled|in_movie|in_preview
    tkhd_payload.extend_from_slice(&0u32.to_be_bytes()); // creation
    tkhd_payload.extend_from_slice(&0u32.to_be_bytes()); // mod
    tkhd_payload.extend_from_slice(&1u32.to_be_bytes()); // track_id = 1
    tkhd_payload.extend_from_slice(&0u32.to_be_bytes()); // reserved
    tkhd_payload.extend_from_slice(&1000u32.to_be_bytes()); // duration
    tkhd_payload.extend_from_slice(&[0u8; 8]); // reserved
    tkhd_payload.extend_from_slice(&0i16.to_be_bytes()); // layer
    tkhd_payload.extend_from_slice(&0i16.to_be_bytes()); // alternate_group
    tkhd_payload.extend_from_slice(&0x0100u16.to_be_bytes()); // volume
    tkhd_payload.extend_from_slice(&0u16.to_be_bytes()); // reserved
    for val in matrix {
        tkhd_payload.extend_from_slice(&val.to_be_bytes());
    }
    tkhd_payload.extend_from_slice(&0u32.to_be_bytes()); // width 0 (audio)
    tkhd_payload.extend_from_slice(&0u32.to_be_bytes()); // height 0 (audio)
    trak_payload.extend_from_slice(&write_box(b"tkhd", &tkhd_payload));

    // Optional edts / elst for AAC priming delay compensation
    if priming_samples > 0 {
        let mut elst_payload = Vec::new();
        elst_payload.extend_from_slice(&[0u8; 4]); // version 0
        elst_payload.extend_from_slice(&1u32.to_be_bytes()); // entry_count = 1
        elst_payload.extend_from_slice(&1000u32.to_be_bytes()); // segment_duration
        elst_payload.extend_from_slice(&(priming_samples as i32).to_be_bytes()); // media_time (priming offset!)
        elst_payload.extend_from_slice(&0x00010000u32.to_be_bytes()); // media_rate = 1.0
        let elst_box = write_box(b"elst", &elst_payload);
        trak_payload.extend_from_slice(&write_box(b"edts", &elst_box));
    }

    // mdia (Media Box)
    let mut mdia_payload = Vec::new();

    // mdhd (Media Header)
    let mut mdhd_payload = Vec::new();
    mdhd_payload.extend_from_slice(&[0u8; 4]);
    mdhd_payload.extend_from_slice(&0u32.to_be_bytes());
    mdhd_payload.extend_from_slice(&0u32.to_be_bytes());
    mdhd_payload.extend_from_slice(&sample_rate.to_be_bytes());
    mdhd_payload.extend_from_slice(&1000u32.to_be_bytes());
    mdhd_payload.extend_from_slice(&0x55c4u16.to_be_bytes()); // language = 'und'
    mdhd_payload.extend_from_slice(&0u16.to_be_bytes());
    mdia_payload.extend_from_slice(&write_box(b"mdhd", &mdhd_payload));

    // hdlr (Handler)
    let mut hdlr_payload = Vec::new();
    hdlr_payload.extend_from_slice(&[0u8; 4]);
    hdlr_payload.extend_from_slice(&0u32.to_be_bytes());
    hdlr_payload.extend_from_slice(b"soun"); // sound handler
    hdlr_payload.extend_from_slice(&[0u8; 12]);
    hdlr_payload.extend_from_slice(b"SoundHandler\0");
    mdia_payload.extend_from_slice(&write_box(b"hdlr", &hdlr_payload));

    // minf (Media Information Box)
    let mut minf_payload = Vec::new();

    // smhd (Sound Media Header)
    let mut smhd_payload = Vec::new();
    smhd_payload.extend_from_slice(&[0u8; 4]);
    smhd_payload.extend_from_slice(&0u16.to_be_bytes()); // balance
    smhd_payload.extend_from_slice(&0u16.to_be_bytes()); // reserved
    minf_payload.extend_from_slice(&write_box(b"smhd", &smhd_payload));

    // dinf (Data Information)
    let dinf_bytes: [u8; 36] = [
        0, 0, 0, 36, b'd', b'i', b'n', b'f', 0, 0, 0, 28, b'd', b'r', b'e', b'f', 0, 0, 0, 0, 0, 0,
        0, 1, // count = 1
        0, 0, 0, 12, b'u', b'r', b'l', b' ', 0, 0, 0, 1, // flags = 1 (in same file)
    ];
    minf_payload.extend_from_slice(&dinf_bytes);

    // stbl (Sample Table Box)
    let mut stbl_payload = Vec::new();

    // stsd (Sample Description Box with mp4a)
    let mut mp4a_payload = Vec::new();
    mp4a_payload.extend_from_slice(&[0u8; 6]); // reserved
    mp4a_payload.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index = 1
    mp4a_payload.extend_from_slice(&0u32.to_be_bytes()); // reserved
    mp4a_payload.extend_from_slice(&0u32.to_be_bytes()); // reserved
    mp4a_payload.extend_from_slice(&channels.to_be_bytes()); // channels
    mp4a_payload.extend_from_slice(&16u16.to_be_bytes()); // sample_size = 16
    mp4a_payload.extend_from_slice(&0u32.to_be_bytes()); // reserved
    mp4a_payload.extend_from_slice(&(sample_rate << 16).to_be_bytes()); // time_scale 16.16
    let mp4a_box = write_box(b"mp4a", &mp4a_payload);

    let mut stsd_payload = Vec::new();
    stsd_payload.extend_from_slice(&[0u8; 4]); // version & flags
    stsd_payload.extend_from_slice(&1u32.to_be_bytes()); // count = 1
    stsd_payload.extend_from_slice(&mp4a_box);
    stbl_payload.extend_from_slice(&write_box(b"stsd", &stsd_payload));

    // stts (Time-to-Sample Box)
    let stts_bytes: [u8; 24] = [
        0, 0, 0, 24, b's', b't', b't', b's', 0, 0, 0, 0, 0, 0, 0, 1, // count = 1
        0, 0, 0, 1, 0, 0, 3, 0xe8, // 1 sample of duration 1000
    ];
    stbl_payload.extend_from_slice(&stts_bytes);

    // stsc (Sample-to-Chunk Box)
    let stsc_bytes: [u8; 28] = [
        0, 0, 0, 28, b's', b't', b's', b'c', 0, 0, 0, 0, 0, 0, 0, 1, // count = 1
        0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, // first_chunk 1, samples_per_chunk 1, desc 1
    ];
    stbl_payload.extend_from_slice(&stsc_bytes);

    // stsz (Sample Size Box)
    let sample_len = audio_data.len() as u32;
    let mut stsz_payload = Vec::new();
    stsz_payload.extend_from_slice(&[0u8; 4]);
    stsz_payload.extend_from_slice(&sample_len.to_be_bytes()); // uniform sample size
    stsz_payload.extend_from_slice(&1u32.to_be_bytes()); // count = 1
    stbl_payload.extend_from_slice(&write_box(b"stsz", &stsz_payload));

    // stco (Chunk Offset Box)
    let mut stco_payload = Vec::new();
    stco_payload.extend_from_slice(&[0u8; 4]);
    stco_payload.extend_from_slice(&1u32.to_be_bytes()); // count = 1
    stco_payload.extend_from_slice(&0u32.to_be_bytes()); // placeholder chunk_offset
    stbl_payload.extend_from_slice(&write_box(b"stco", &stco_payload));

    minf_payload.extend_from_slice(&write_box(b"stbl", &stbl_payload));
    mdia_payload.extend_from_slice(&write_box(b"minf", &minf_payload));
    trak_payload.extend_from_slice(&write_box(b"mdia", &mdia_payload));
    moov_payload.extend_from_slice(&write_box(b"trak", &trak_payload));

    let mut moov_box = write_box(b"moov", &moov_payload);
    let moov_size = moov_box.len() as u32;

    let mut raw = Vec::new();
    raw.extend_from_slice(&ftyp_box);

    if faststart {
        // moov comes BEFORE mdat
        let chunk_offset = ftyp_size.saturating_add(moov_size).saturating_add(8);
        let _ = patch_chunk_offsets(&mut moov_box, chunk_offset as u64);

        raw.extend_from_slice(&moov_box);
        raw.extend_from_slice(&mdat_box);
    } else {
        // mdat comes BEFORE moov (streaming / naive layout)
        let chunk_offset = ftyp_size.saturating_add(8);
        let _ = patch_chunk_offsets(&mut moov_box, chunk_offset as u64);

        raw.extend_from_slice(&mdat_box);
        raw.extend_from_slice(&moov_box);
    }

    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_reorganize_faststart() {
        let fake_audio = vec![0x12, 0x34, 0x56, 0x78];
        // Build naive (mdat before moov)
        let naive_bytes = build_synthetic_m4a(&fake_audio, 48_000, 2, 1024, false);

        let boxes_before = parse_boxes(&naive_bytes).unwrap();
        assert_eq!(&boxes_before[0].fourcc, b"ftyp");
        assert_eq!(&boxes_before[1].fourcc, b"mdat");
        assert_eq!(&boxes_before[2].fourcc, b"moov");

        // Reorganize to faststart
        let faststart_bytes = reorganize_to_faststart(&naive_bytes).unwrap();
        let boxes_after = parse_boxes(&faststart_bytes).unwrap();

        assert_eq!(&boxes_after[0].fourcc, b"ftyp");
        assert_eq!(&boxes_after[1].fourcc, b"moov");
        assert_eq!(&boxes_after[2].fourcc, b"mdat");

        // Total byte count should be preserved exactly
        assert_eq!(naive_bytes.len(), faststart_bytes.len());
    }

    #[test]
    fn already_faststart_is_idempotent() {
        let fake_audio = vec![0xaa, 0xbb];
        let faststart_orig = build_synthetic_m4a(&fake_audio, 44_100, 2, 0, true);
        let faststart_reorg = reorganize_to_faststart(&faststart_orig).unwrap();
        assert_eq!(faststart_orig, faststart_reorg);
    }
}
