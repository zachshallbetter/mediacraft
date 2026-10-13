//! Pure-Rust RIFF/WAVE Parser and Writer
//!
//! Provides zero-dependency, safe decoding and encoding of Linear PCM WAV files.
//! Supports 16-bit, 24-bit, and 32-bit integer PCM as well as 32-bit IEEE float.
//! Decodes interleaved channel samples into planar `Vec<Vec<f32>>` buffers.

use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WavError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid RIFF header or not a WAVE file")]
    InvalidHeader,
    #[error("missing fmt chunk in WAV file")]
    MissingFmtChunk,
    #[error("missing data chunk in WAV file")]
    MissingDataChunk,
    #[error("unsupported audio format {0} (only PCM=1 and IEEE_FLOAT=3 are supported)")]
    UnsupportedFormat(u16),
    #[error("unsupported bits per sample: {0} (supported: 16, 24, 32)")]
    UnsupportedBitsPerSample(u16),
    #[error("unsupported channel count: {0} (supported: 1..=8)")]
    UnsupportedChannels(u16),
    #[error("premature end of file while reading WAV data")]
    PrematureEof,
}

/// Metadata and audio content decoded from a WAV file.
#[derive(Clone, Debug, PartialEq)]
pub struct WavAudio {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    /// Planar audio channels, each normalized to `[-1.0, 1.0]`.
    /// `samples[ch][sample_idx]`
    pub samples: Vec<Vec<f32>>,
}

impl WavAudio {
    /// Number of audio frames (samples per channel).
    pub fn frame_count(&self) -> usize {
        self.samples.first().map_or(0, |ch| ch.len())
    }

    /// Duration of audio in seconds.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.frame_count() as f64 / self.sample_rate as f64
        }
    }
}

/// Reads a linear PCM or IEEE float WAV file from disk.
pub fn read_wav_file<P: AsRef<Path>>(path: P) -> Result<WavAudio, WavError> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    read_wav_bytes(&data)
}

/// Reads a linear PCM or IEEE float WAV file from in-memory bytes.
pub fn read_wav_bytes(bytes: &[u8]) -> Result<WavAudio, WavError> {
    if bytes.len() < 12 {
        return Err(WavError::InvalidHeader);
    }

    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::InvalidHeader);
    }

    let mut cursor = 12;
    let mut fmt_info: Option<(u16, u16, u32, u16)> = None; // (format_tag, channels, sample_rate, bits_per_sample)
    let mut audio_data: Option<&[u8]> = None;

    while cursor + 8 <= bytes.len() {
        let chunk_id = match bytes.get(cursor..cursor + 4) {
            Some(id) => id,
            None => break,
        };
        let chunk_size = match bytes.get(cursor + 4..cursor + 8) {
            Some(sz) => u32::from_le_bytes([sz[0], sz[1], sz[2], sz[3]]) as usize,
            None => break,
        };
        cursor += 8;

        let chunk_payload = match bytes.get(cursor..cursor + chunk_size) {
            Some(slice) => slice,
            None => {
                // If chunk size extends slightly past file (e.g. truncated padding byte), slice to end
                match bytes.get(cursor..) {
                    Some(slice) => slice,
                    None => return Err(WavError::PrematureEof),
                }
            }
        };

        if chunk_id == b"fmt " {
            if chunk_payload.len() < 16 {
                return Err(WavError::InvalidHeader);
            }
            let mut format_tag = u16::from_le_bytes([chunk_payload[0], chunk_payload[1]]);
            let channels = u16::from_le_bytes([chunk_payload[2], chunk_payload[3]]);
            let sample_rate = u32::from_le_bytes([
                chunk_payload[4],
                chunk_payload[5],
                chunk_payload[6],
                chunk_payload[7],
            ]);
            let bits_per_sample = u16::from_le_bytes([chunk_payload[14], chunk_payload[15]]);

            if format_tag == 0xFFFE {
                // WAVE_FORMAT_EXTENSIBLE: SubFormat GUID begins at offset 24
                if chunk_payload.len() >= 26 {
                    format_tag = u16::from_le_bytes([chunk_payload[24], chunk_payload[25]]);
                }
            }

            if format_tag != 1 && format_tag != 3 {
                return Err(WavError::UnsupportedFormat(format_tag));
            }
            if channels == 0 || channels > 8 {
                return Err(WavError::UnsupportedChannels(channels));
            }
            if bits_per_sample != 16 && bits_per_sample != 24 && bits_per_sample != 32 {
                return Err(WavError::UnsupportedBitsPerSample(bits_per_sample));
            }

            fmt_info = Some((format_tag, channels, sample_rate, bits_per_sample));
        } else if chunk_id == b"data" {
            audio_data = Some(chunk_payload);
        }

        // Chunks in RIFF are word-aligned (padded to multiple of 2)
        cursor += chunk_size;
        if chunk_size % 2 == 1 {
            cursor += 1;
        }
    }

    let (format_tag, channels, sample_rate, bits_per_sample) =
        fmt_info.ok_or(WavError::MissingFmtChunk)?;
    let raw_samples = audio_data.ok_or(WavError::MissingDataChunk)?;

    let bytes_per_sample = (bits_per_sample as usize) / 8;
    let frame_bytes = bytes_per_sample * (channels as usize);
    if frame_bytes == 0 {
        return Err(WavError::InvalidHeader);
    }

    let total_frames = raw_samples.len() / frame_bytes;
    let mut channel_data = vec![Vec::with_capacity(total_frames); channels as usize];

    for frame_idx in 0..total_frames {
        let frame_offset = frame_idx * frame_bytes;
        for (ch, channel_vec) in channel_data.iter_mut().enumerate() {
            let sample_offset = frame_offset + ch * bytes_per_sample;
            if sample_offset + bytes_per_sample > raw_samples.len() {
                break;
            }

            let sample_val = match (format_tag, bits_per_sample) {
                (1, 16) => {
                    let b = [raw_samples[sample_offset], raw_samples[sample_offset + 1]];
                    let s = i16::from_le_bytes(b);
                    (s as f32) / 32768.0
                }
                (1, 24) => {
                    let b0 = raw_samples[sample_offset];
                    let b1 = raw_samples[sample_offset + 1];
                    let b2 = raw_samples[sample_offset + 2];
                    let s = i32::from_le_bytes([0, b0, b1, b2]) >> 8;
                    (s as f32) / 8388608.0
                }
                (1, 32) => {
                    let b = [
                        raw_samples[sample_offset],
                        raw_samples[sample_offset + 1],
                        raw_samples[sample_offset + 2],
                        raw_samples[sample_offset + 3],
                    ];
                    let s = i32::from_le_bytes(b);
                    (s as f32) / 2147483648.0
                }
                (3, 32) => {
                    let b = [
                        raw_samples[sample_offset],
                        raw_samples[sample_offset + 1],
                        raw_samples[sample_offset + 2],
                        raw_samples[sample_offset + 3],
                    ];
                    f32::from_le_bytes(b)
                }
                _ => 0.0,
            };

            channel_vec.push(sample_val.clamp(-1.0, 1.0));
        }
    }

    Ok(WavAudio {
        channels,
        sample_rate,
        bits_per_sample,
        samples: channel_data,
    })
}

/// Encodes planar floating-point audio into standard 16-bit Linear PCM WAV bytes.
pub fn write_wav_bytes(
    channels: u16,
    sample_rate: u32,
    planar_samples: &[&[f32]],
) -> Result<Vec<u8>, WavError> {
    if channels == 0 || channels as usize != planar_samples.len() {
        return Err(WavError::UnsupportedChannels(channels));
    }

    let frame_count = planar_samples.first().map_or(0, |ch| ch.len());
    let bytes_per_sample = 2usize; // 16-bit PCM
    let data_len = frame_count * (channels as usize) * bytes_per_sample;
    let riff_chunk_size = 36 + data_len;

    let mut out = Cursor::new(Vec::with_capacity(44 + data_len));

    // 1. RIFF Header
    out.write_all(b"RIFF")?;
    out.write_all(&(riff_chunk_size as u32).to_le_bytes())?;
    out.write_all(b"WAVE")?;

    // 2. fmt chunk
    out.write_all(b"fmt ")?;
    out.write_all(&16u32.to_le_bytes())?; // Subchunk1Size = 16 for PCM
    out.write_all(&1u16.to_le_bytes())?; // AudioFormat = 1 (PCM)
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * (channels as u32) * (bytes_per_sample as u32);
    out.write_all(&byte_rate.to_le_bytes())?;
    let block_align = channels * (bytes_per_sample as u16);
    out.write_all(&block_align.to_le_bytes())?;
    out.write_all(&16u16.to_le_bytes())?; // BitsPerSample = 16

    // 3. data chunk
    out.write_all(b"data")?;
    out.write_all(&(data_len as u32).to_le_bytes())?;

    // Interleave planar samples
    for frame_idx in 0..frame_count {
        for ch_idx in 0..channels as usize {
            let sample_f32 = match planar_samples.get(ch_idx).and_then(|ch| ch.get(frame_idx)) {
                Some(s) => *s,
                None => 0.0,
            };
            let clamped = sample_f32.clamp(-1.0, 1.0);
            let sample_i16 = (clamped * 32767.0) as i16;
            out.write_all(&sample_i16.to_le_bytes())?;
        }
    }

    Ok(out.into_inner())
}

/// Writes planar floating-point audio into a 16-bit Linear PCM WAV file on disk.
pub fn write_wav_file<P: AsRef<Path>>(
    path: P,
    channels: u16,
    sample_rate: u32,
    planar_samples: &[&[f32]],
) -> Result<(), WavError> {
    let bytes = write_wav_bytes(channels, sample_rate, planar_samples)?;
    craft_atomic::atomic_write(path.as_ref(), &bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wav_roundtrip_16bit_stereo() {
        let sample_rate = 48_000;
        let channels = 2;
        let frames = 4800; // 0.1s

        let left: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.05).sin() * 0.7).collect();
        let right: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.10).cos() * 0.7).collect();

        let encoded = write_wav_bytes(channels, sample_rate, &[&left, &right])
            .unwrap_or_else(|e| panic!("write error: {e:?}"));

        let decoded = read_wav_bytes(&encoded).unwrap_or_else(|e| panic!("read error: {e:?}"));

        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.bits_per_sample, 16);
        assert_eq!(decoded.frame_count(), frames);

        // Verify sample accuracy within 16-bit quantization epsilon
        for i in 0..frames {
            let diff_l = (decoded.samples[0][i] - left[i]).abs();
            let diff_r = (decoded.samples[1][i] - right[i]).abs();
            assert!(diff_l < 0.0001, "left drift at sample {i}: {diff_l}");
            assert!(diff_r < 0.0001, "right drift at sample {i}: {diff_r}");
        }
    }
}
