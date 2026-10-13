use craft_aac::{
    AdtsHeader, AudioSpecificConfig, Encoder, EncoderConfig, Error, diagnose_adts_stream,
};

#[test]
fn eval_hostile_adts_header_rejections() {
    // Missing sync word (must start with 0xFFF)
    let bad_sync = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let res = AdtsHeader::parse(&bad_sync);
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), Error::Bitstream(_)));

    // Sync word present but invalid layer (must be 0)
    // 0xFF 0xF2 has layer != 0
    let bad_layer = [0xFF, 0xF2, 0x40, 0x00, 0x00, 0x00, 0x00];
    let res = AdtsHeader::parse(&bad_layer);
    assert!(res.is_err());

    // Truncated buffer (< 7 bytes)
    let truncated = [0xFF, 0xF1, 0x50];
    let res = AdtsHeader::parse(&truncated);
    assert!(res.is_err());
}

#[test]
fn eval_asc_diagnostics_detects_he_aac_and_channels() {
    // Standard stereo 44.1 kHz AAC-LC
    let asc_stereo = AudioSpecificConfig::lc(4, 2);
    let report = asc_stereo.diagnose();
    assert_eq!(report.sample_rate, 44100);
    assert_eq!(report.channel_count, 2);
    assert_eq!(report.frame_length_samples, 1024);
    assert!(!report.sbr_present);
    assert!(!report.ps_present);
    assert!(
        report.warnings.is_empty(),
        "clean stereo should have 0 warnings"
    );

    // 5.1 surround 48 kHz
    let asc_surround = AudioSpecificConfig::lc(3, 6);
    let report = asc_surround.diagnose();
    assert_eq!(report.sample_rate, 48000);
    assert_eq!(report.channel_count, 6);
    assert_eq!(report.frame_length_samples, 1024);

    // Parse explicitly signalled SBR HE-AAC
    // AOT 5 (SBR), sf_index 6 (24kHz), ext rate 3 (48kHz), core AOT 2 (LC), ch 2
    let mut he_data = Vec::new();
    // 5 bits: AOT=5 (00101)
    // 4 bits: rate=6 (0110)
    // 4 bits: ext_rate=3 (0011)
    // 5 bits: core AOT=2 (00010)
    // 4 bits: channel_config=2 (0010)
    // = 0010 1011 0001 1000 1000 1000...
    he_data.extend_from_slice(&[0x2B, 0x18, 0x88, 0x00]);
    let asc_he = AudioSpecificConfig::parse(&he_data).expect("HE-AAC header should parse");
    let report = asc_he.diagnose();
    assert!(report.sbr_present);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("HE-AAC v1") && w.contains("only AAC-LC core")),
        "diagnostic report must warn on SBR core-only decode: {:?}",
        report.warnings
    );
}

#[test]
fn eval_adts_stream_diagnostics_and_bitrate() {
    // Encode a synthetic 1 kHz stereo sine wave at 48 kHz
    let mut enc =
        Encoder::new(EncoderConfig::cbr(48_000, 2, 128_000)).expect("encoder init should succeed");

    let sample_count = 4800; // 0.1s
    let left: Vec<f32> = (0..sample_count)
        .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / 48000.0).sin() * 0.4)
        .collect();
    let right = left.clone();

    let mut aus = enc.encode(&[&left, &right]);
    aus.extend(enc.flush());
    assert!(!aus.is_empty());

    // Wrap raw access units with ADTS framing
    let mut adts_stream = Vec::new();
    let asc_bytes = enc.audio_specific_config();
    let asc = AudioSpecificConfig::parse(&asc_bytes).expect("parse audio specific config");
    for au in &aus {
        let frame_len = 7 + au.len();
        let mut hdr = [0u8; 7];
        hdr[0] = 0xFF;
        hdr[1] = 0xF1; // MPEG-4, layer 0, no CRC
        hdr[2] =
            ((asc.object_type - 1) << 6) | (asc.sf_index << 2) | ((asc.channel_config >> 2) & 1);
        hdr[3] = (asc.channel_config << 6) | ((frame_len >> 11) as u8 & 3);
        hdr[4] = (frame_len >> 3) as u8;
        hdr[5] = ((frame_len as u8 & 7) << 5) | 0x1F;
        hdr[6] = 0xFC;

        adts_stream.extend_from_slice(&hdr);
        adts_stream.extend_from_slice(au);
    }

    let report = diagnose_adts_stream(&adts_stream);
    assert_eq!(report.frame_count, aus.len());
    assert_eq!(report.sample_rate, 48000);
    assert_eq!(report.channel_count, 2);
    assert!(!report.has_crc);
    assert!(
        report.warnings.is_empty(),
        "clean stream has zero warnings: {:?}",
        report.warnings
    );

    if let Some(br) = report.estimated_bitrate_kbps {
        assert!(
            br > 50.0 && br < 300.0,
            "bitrate estimate must be reasonable: {}",
            br
        );
    }
}

#[test]
fn eval_truncated_adts_stream_graceful_handling() {
    let mut enc =
        Encoder::new(EncoderConfig::cbr(48_000, 2, 128_000)).expect("encoder init should succeed");
    let left = vec![0.0f32; 2048];
    let right = left.clone();
    let mut aus = enc.encode(&[&left, &right]);
    aus.extend(enc.flush());

    let mut stream = Vec::new();
    let asc_bytes = enc.audio_specific_config();
    let asc = AudioSpecificConfig::parse(&asc_bytes).expect("parse audio specific config");
    for au in &aus {
        let frame_len = 7 + au.len();
        let mut hdr = [0u8; 7];
        hdr[0] = 0xFF;
        hdr[1] = 0xF1;
        hdr[2] =
            ((asc.object_type - 1) << 6) | (asc.sf_index << 2) | ((asc.channel_config >> 2) & 1);
        hdr[3] = (asc.channel_config << 6) | ((frame_len >> 11) as u8 & 3);
        hdr[4] = (frame_len >> 3) as u8;
        hdr[5] = ((frame_len as u8 & 7) << 5) | 0x1F;
        hdr[6] = 0xFC;
        stream.extend_from_slice(&hdr);
        stream.extend_from_slice(au);
    }

    // Chop off the last frame by 10 bytes
    let truncated_len = stream.len().saturating_sub(10);
    let truncated = &stream[..truncated_len];

    let report = diagnose_adts_stream(truncated);
    assert!(
        report.warnings.iter().any(|w| w.contains("invalid length")
            || w.contains("trailing incomplete bytes")
            || w.contains("sync lost")),
        "truncated stream must warn gracefully, got warnings: {:?}",
        report.warnings
    );
}
