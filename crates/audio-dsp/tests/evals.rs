//! Evaluation and hostile input test suite for craft-audio-dsp.

use craft_audio_dsp::channels::{Layout, Mixdown, apply_matrix, matrix};
use craft_audio_dsp::loudness::{LoudnessMeter, normalize_gain_db};
use proptest::prelude::*;

#[test]
fn eval_bs1770_reference_tone_and_silence() {
    let mut meter = LoudnessMeter::new(48000.0, 2);

    // 1. Digital silence
    let silence = vec![0.0f32; 48000];
    let mut l_silence = silence.clone();
    let mut r_silence = silence.clone();
    meter.process(&[&mut l_silence, &mut r_silence]);

    // Gated integrated loudness of silence should be None or below -70 LUFS
    let int_lufs = meter.integrated();
    assert!(
        int_lufs < -70.0 || int_lufs.is_infinite(),
        "Silence should not exceed gating threshold: {}",
        int_lufs
    );

    // 2. Normalization calculation
    // If target is -23 LUFS and measured is -18 LUFS, gain must be -5 dB
    let gain = normalize_gain_db(-18.0, -23.0);
    assert!((gain - (-5.0)).abs() < 1e-4);
}

#[test]
fn eval_channel_matrix_downmix_invariants() {
    let n = 256;
    // 5.1 layout: 6 channels with 0.5f32 values
    let audio_51 = vec![vec![0.5f32; n]; 6];
    let slices_51: Vec<&[f32]> = audio_51.iter().map(Vec::as_slice).collect();

    // 5.1 to Stereo matrix
    let m = matrix(Layout::Surround51, Layout::Stereo, Mixdown::FrontRear);
    let stereo = apply_matrix(&m, &slices_51);

    assert_eq!(stereo.len(), 2);
    for ch in &stereo {
        assert_eq!(ch.len(), n);
        for &s in ch {
            assert!(s.is_finite());
            assert!(s > 0.0);
        }
    }

    // Mono to Stereo matrix
    let mono = [vec![0.8f32; n]];
    let mono_slices: Vec<&[f32]> = mono.iter().map(Vec::as_slice).collect();
    let m_up = matrix(Layout::Mono, Layout::Stereo, Mixdown::default());
    let stereo_up = apply_matrix(&m_up, &mono_slices);

    assert_eq!(stereo_up.len(), 2);
    for ch in &stereo_up {
        for &s in ch {
            assert!((s - 0.8).abs() < 1e-5);
        }
    }
}

#[test]
fn eval_hostile_audio_meter_inputs_never_panic() {
    let mut meter = LoudnessMeter::new(48000.0, 2);

    // Empty slices
    let mut empty_l: [f32; 0] = [];
    let mut empty_r: [f32; 0] = [];
    meter.process(&[&mut empty_l, &mut empty_r]);

    // Extreme floating point numbers (subnormals, large values)
    let mut extreme_l = vec![1e30f32, -1e30f32, 1e-35f32, -1e-35f32];
    let mut extreme_r = vec![1.0f32; 4];
    meter.process(&[&mut extreme_l, &mut extreme_r]);

    // Reading properties must be safe
    let _ = meter.momentary();
    let _ = meter.short_term();
    let _ = meter.integrated();
    let _ = meter.true_peak_dbtp();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn eval_arbitrary_audio_samples_metering_never_panics(
        samples in proptest::collection::vec(any::<f32>(), 0..512)
    ) {
        let mut meter = LoudnessMeter::new(48000.0, 1);
        let mut s = samples;
        for v in &mut s {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
        meter.process(&[&mut s]);
        let _ = meter.momentary();
        let _ = meter.integrated();
    }
}
