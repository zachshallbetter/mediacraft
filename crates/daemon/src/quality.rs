//! Empirical quality measurement engine (Milestone 7).
//!
//! Provides mathematically verified, pure-Rust audio and video quality analysis:
//! - Video: Peak Signal-to-Noise Ratio (PSNR) and Structural Similarity Index (SSIM).
//! - Audio: ITU-R BS.1770-4 / EBU R128 integrated loudness (LUFS) and true peak (dBTP)
//!   via `craft-audio-dsp`.
//!
//! Production paths MUST NEVER use synthetic, simulated, or placeholder metrics.

use mediacraft_protocol::stream::VideoFrame;
use serde::{Deserialize, Serialize};

/// Error types encountered during quality analysis.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QualityError {
    #[error(
        "Dimension mismatch between reference ({ref_w}x{ref_h}) and distorted ({dist_w}x{dist_h})"
    )]
    DimensionMismatch {
        ref_w: u32,
        ref_h: u32,
        dist_w: u32,
        dist_h: u32,
    },
    #[error("Empty frame or channel data")]
    EmptyData,
}

/// Comprehensive empirically measured media quality report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityReport {
    /// Average Luma PSNR across all analyzed frames in decibels (dB).
    pub video_psnr_db: Option<f64>,
    /// Average Luma SSIM across all analyzed frames [0.0, 1.0].
    pub video_ssim: Option<f64>,
    /// Integrated programme loudness in LUFS (ITU-R BS.1770-4).
    pub audio_integrated_lufs: Option<f64>,
    /// Maximum true-peak level in dBTP (oversampled reconstruction).
    pub audio_true_peak_dbtp: Option<f64>,
    /// Maximum sample peak in dBFS.
    pub audio_sample_peak_dbfs: Option<f64>,
    /// Explicit epistemic flag: MUST be false in production paths.
    pub is_simulated: bool,
}

/// Computes Peak Signal-to-Noise Ratio (PSNR) on the Y (luma) plane of two frames.
///
/// Returns PSNR in decibels (dB). If the frames are bit-identical, returns `f64::INFINITY`.
pub fn calculate_psnr(
    ref_frame: &VideoFrame,
    dist_frame: &VideoFrame,
) -> Result<f64, QualityError> {
    if ref_frame.width != dist_frame.width || ref_frame.height != dist_frame.height {
        return Err(QualityError::DimensionMismatch {
            ref_w: ref_frame.width,
            ref_h: ref_frame.height,
            dist_w: dist_frame.width,
            dist_h: dist_frame.height,
        });
    }

    if ref_frame.y.is_empty() {
        return Err(QualityError::EmptyData);
    }

    let n = ref_frame.y.len().min(dist_frame.y.len());
    if n == 0 {
        return Err(QualityError::EmptyData);
    }

    let mut sum_sq_diff = 0u64;
    for i in 0..n {
        let r = ref_frame.y.get(i).copied().unwrap_or(0) as i64;
        let d = dist_frame.y.get(i).copied().unwrap_or(0) as i64;
        let diff = r - d;
        sum_sq_diff = sum_sq_diff.saturating_add((diff * diff) as u64);
    }

    let mse = sum_sq_diff as f64 / n as f64;
    if mse <= 1e-12 {
        Ok(f64::INFINITY)
    } else {
        let max_val = 255.0f64;
        let psnr = 10.0 * (max_val * max_val / mse).log10();
        Ok(psnr)
    }
}

/// Computes Structural Similarity Index (SSIM) on the Y (luma) plane of two frames.
///
/// Evaluates local 8x8 blocks with standard constants:
/// C1 = (0.01 * 255)^2 = 6.5025
/// C2 = (0.03 * 255)^2 = 58.5225
pub fn calculate_ssim(
    ref_frame: &VideoFrame,
    dist_frame: &VideoFrame,
) -> Result<f64, QualityError> {
    let w = ref_frame.width as usize;
    let h = ref_frame.height as usize;
    if w != dist_frame.width as usize || h != dist_frame.height as usize {
        return Err(QualityError::DimensionMismatch {
            ref_w: ref_frame.width,
            ref_h: ref_frame.height,
            dist_w: dist_frame.width,
            dist_h: dist_frame.height,
        });
    }

    if w < 8 || h < 8 {
        return Err(QualityError::EmptyData);
    }

    const C1: f64 = 6.5025;
    const C2: f64 = 58.5225;

    let stride = ref_frame.y_stride;
    let blocks_x = w / 8;
    let blocks_y = h / 8;
    let total_blocks = blocks_x * blocks_y;

    if total_blocks == 0 {
        return Err(QualityError::EmptyData);
    }

    let mut ssim_sum = 0.0f64;

    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let start_x = bx * 8;
            let start_y = by * 8;

            let mut sum_x = 0.0f64;
            let mut sum_y = 0.0f64;
            let mut sum_sq_x = 0.0f64;
            let mut sum_sq_y = 0.0f64;
            let mut sum_xy = 0.0f64;

            for dy in 0..8 {
                let row_offset = (start_y + dy) * stride;
                for dx in 0..8 {
                    let idx = row_offset + start_x + dx;
                    let px_x = ref_frame.y.get(idx).copied().unwrap_or(0) as f64;
                    let px_y = dist_frame.y.get(idx).copied().unwrap_or(0) as f64;

                    sum_x += px_x;
                    sum_y += px_y;
                    sum_sq_x += px_x * px_x;
                    sum_sq_y += px_y * px_y;
                    sum_xy += px_x * px_y;
                }
            }

            let n = 64.0f64;
            let mu_x = sum_x / n;
            let mu_y = sum_y / n;

            let var_x = (sum_sq_x / n) - (mu_x * mu_x);
            let var_y = (sum_sq_y / n) - (mu_y * mu_y);
            let cov_xy = (sum_xy / n) - (mu_x * mu_y);

            let numerator = (2.0 * mu_x * mu_y + C1) * (2.0 * cov_xy + C2);
            let denominator = (mu_x * mu_x + mu_y * mu_y + C1) * (var_x + var_y + C2);

            let block_ssim = if denominator.abs() > 1e-12 {
                numerator / denominator
            } else {
                1.0
            };

            ssim_sum += block_ssim;
        }
    }

    Ok(ssim_sum / total_blocks as f64)
}

/// Measures audio programme loudness according to ITU-R BS.1770-4.
pub fn measure_audio_loudness(
    channels: &[Vec<f32>],
    sample_rate: u32,
) -> Result<craft_audio_dsp::loudness::LoudnessSummary, QualityError> {
    if channels.is_empty() || channels.iter().any(|c| c.is_empty()) {
        return Err(QualityError::EmptyData);
    }

    let channel_slices: Vec<&[f32]> = channels.iter().map(|c| c.as_slice()).collect();
    Ok(craft_audio_dsp::loudness::measure(
        &channel_slices,
        sample_rate as f64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_frames_yield_infinite_psnr_and_unit_ssim() {
        let frame1 = VideoFrame::black(64, 64, 0, mediacraft_protocol::stream::Rational::FPS_24);
        let frame2 = VideoFrame::black(64, 64, 0, mediacraft_protocol::stream::Rational::FPS_24);

        let psnr = calculate_psnr(&frame1, &frame2).unwrap();
        assert_eq!(psnr, f64::INFINITY);

        let ssim = calculate_ssim(&frame1, &frame2).unwrap();
        assert!((ssim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_distorted_frames_produce_finite_psnr_and_degraded_ssim() {
        let frame1 = VideoFrame::black(64, 64, 0, mediacraft_protocol::stream::Rational::FPS_24);
        let mut frame2 =
            VideoFrame::black(64, 64, 0, mediacraft_protocol::stream::Rational::FPS_24);

        // Perturb pixel values
        for p in frame2.y.iter_mut() {
            *p = p.saturating_add(10);
        }

        let psnr = calculate_psnr(&frame1, &frame2).unwrap();
        assert!(psnr.is_finite());
        assert!(
            psnr > 20.0 && psnr < 35.0,
            "Expected PSNR ~28 dB, got: {psnr}"
        );

        let ssim = calculate_ssim(&frame1, &frame2).unwrap();
        assert!(ssim < 1.0);
        assert!(
            ssim > 0.5,
            "Expected high SSIM for small shift, got: {ssim}"
        );
    }

    #[test]
    fn test_real_audio_loudness_measurement() {
        // Generate 1 second of 1 kHz sine at -20 dBFS in stereo
        let sample_rate = 48000;
        let mut left = Vec::with_capacity(sample_rate);
        let mut right = Vec::with_capacity(sample_rate);
        let amp = 0.1f32; // -20 dBFS

        for i in 0..sample_rate {
            let phase = 2.0 * std::f32::consts::PI * 1000.0 * (i as f32) / (sample_rate as f32);
            let s = amp * phase.sin();
            left.push(s);
            right.push(s);
        }

        let summary = measure_audio_loudness(&[left, right], sample_rate as u32).unwrap();
        assert!(summary.integrated_lufs.is_finite());
        // For a -20 dBFS sine through K-weighting at 1kHz (+0.5 dB boost) and 2 channels,
        // loudness is around -20 LUFS.
        assert!(
            summary.integrated_lufs < -15.0 && summary.integrated_lufs > -25.0,
            "Integrated LUFS was: {}",
            summary.integrated_lufs
        );
        assert!(summary.sample_peak_dbfs.is_finite());
        assert!(summary.true_peak_dbtp.is_finite());
    }
}
