use super::*;
use craft_isobmff::{
    AvcConfig, Brand, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions,
};

/// Luma of frame `i`, so a test can tell which frame came back.
fn luma(i: usize) -> u8 {
    (20 + i * 7) as u8
}

/// A 64×48 H.264 MP4 at 30 fps, `frames` long, each frame one flat grey, keyframe every 10.
fn h264_mp4(frames: usize) -> Vec<u8> {
    let (w, h) = (64usize, 48usize);
    let mut cfg = craft_h264enc::EncoderConfig::new(w as u32, h as u32, 30, 1);
    cfg.format = craft_h264enc::PacketFormat::LengthPrefixed;
    cfg.keyint = 10;
    cfg.bframes = 0;
    let mut enc = craft_h264enc::Encoder::new(cfg).unwrap();
    let (sps, pps) = enc.sps_pps();
    let mut packets = Vec::new();
    for i in 0..frames {
        let y = vec![luma(i); w * h];
        let c = vec![128u8; (w / 2) * (h / 2)];
        let f = craft_h264enc::YuvFrame {
            y: &y,
            u: &c,
            v: &c,
            y_stride: w,
            uv_stride: w / 2,
        };
        packets.extend(enc.encode(&f, i as i64).unwrap());
    }
    packets.extend(enc.flush());

    let mut out = std::io::Cursor::new(Vec::new());
    let mut wr = Mp4Writer::new(&mut out, WriterOptions::new(Brand::Mp4)).unwrap();
    let entry = SampleEntry::avc(AvcConfig::new(vec![sps], vec![pps], 4), w as u16, h as u16);
    let track = wr.add_track(TrackConfig::new(entry, 30)).unwrap();
    for p in &packets {
        let sample = WriteSample {
            data: &p.data,
            duration: 1,
            composition_offset: (p.pts - p.dts) as i32,
            is_sync: p.keyframe,
        };
        wr.write_sample(track, sample).unwrap();
    }
    wr.finish().unwrap();
    out.into_inner()
}

fn write(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mc_preview_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

/// The grey a flat limited-range luma value shows as.
fn grey(y: u8) -> u8 {
    ((f32::from(y) - 16.0) / 219.0 * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[test]
fn poster_frame_seeks_to_the_requested_time() {
    let path = write("clip.mp4", &h264_mp4(30));
    for (at, frame) in [(0.0, 0usize), (0.5, 15), (0.9, 27), (60.0, 29)] {
        let f = poster_frame(&path, &FrameOptions::at(at)).unwrap();
        assert_eq!((f.codec, f.source_width, f.source_height), ("h264", 64, 48));
        assert_eq!((f.image.width(), f.image.height()), (64, 48));
        let p = f.image.pixel(32, 24).unwrap();
        assert!(
            p[0].abs_diff(grey(luma(frame))) <= 3,
            "at {at}s: {p:?}, want frame {frame}"
        );
        assert!(
            (f.seconds - frame as f64 / 30.0).abs() < 1e-6,
            "at {at}s: {}",
            f.seconds
        );
    }
}

#[test]
fn poster_frame_fits_and_caps_decoding() {
    let path = write("clip2.mp4", &h264_mp4(30));
    let opts = FrameOptions {
        max_edge: 32,
        max_decode_frames: 2,
        ..FrameOptions::at(0.5)
    };
    let f = poster_frame(&path, &opts).unwrap();
    assert_eq!((f.image.width(), f.image.height()), (32, 24));
    // Frame 15 is 5 past its keyframe (10); with a budget of 2 the keyframe is shown.
    let p = f.image.pixel(16, 12).unwrap();
    assert!(p[0].abs_diff(grey(luma(10))) <= 3, "{p:?}");
}

#[test]
fn rotation_turns_the_frame_upright() {
    let pic = Picture::H264(craft_h264::Picture {
        width: 4,
        height: 2,
        chroma_width: 2,
        chroma_height: 1,
        y: vec![16, 16, 235, 235, 16, 16, 235, 235],
        u: vec![128; 2],
        v: vec![128; 2],
        y_stride: 4,
        uv_stride: 2,
        pts: 0,
        poc: 0,
        key: true,
        color: craft_h264::ColorInfo {
            full_range: false,
            primaries: 1,
            transfer: 1,
            matrix: 1,
        },
        sar: (1, 1),
        draft: false,
    });
    let f = finish(
        &pic,
        None,
        1,
        &FrameOptions {
            max_edge: 100,
            ..FrameOptions::at(0.0)
        },
        0.0,
        "h264",
        None,
    )
    .unwrap();
    // 4×2 turned 90° clockwise is 2×4; the stored left (black) half is now on top.
    assert_eq!((f.image.width(), f.image.height()), (2, 4));
    assert_eq!(f.image.pixel(0, 0).map(|p| p[0]), Some(0));
    assert_eq!(f.image.pixel(0, 3).map(|p| p[0]), Some(255));
}

#[test]
fn audio_only_and_unknown_files_are_errors() {
    let m4a =
        mediacraft_bitstream::isobmff::build_synthetic_m4a(&[0x11; 1000], 48_000, 2, 1024, true);
    assert!(matches!(
        poster_frame(&write("a.m4a", &m4a), &FrameOptions::at(0.0)),
        Err(Error::NoVideo)
    ));
    assert!(matches!(
        poster_frame(&write("b.bin", b"plain text"), &FrameOptions::at(0.0)),
        Err(Error::UnsupportedContainer)
    ));
    assert!(
        poster_frame(
            &write("c.mp4", b"\0\0\0\x10ftypisom\0\0\0\0"),
            &FrameOptions::at(0.0)
        )
        .is_err()
    );
}

#[test]
fn clean_aperture_crops_like_players_do() {
    // iPhone Live Photo: 1920×1440 coded, 1744×1308 clean, centred.
    let c = craft_isobmff::CleanAperture {
        width: (1744, 1),
        height: (1308, 1),
        horiz_offset: (0, 1),
        vert_offset: (0, 1),
    };
    assert_eq!(clean_aperture(&c, 1920, 1440), Some((88, 66, 1744, 1308)));
    // Offsets move the window; a full-size aperture is no crop.
    let c = craft_isobmff::CleanAperture {
        width: (100, 1),
        height: (50, 1),
        horiz_offset: (-10, 1),
        vert_offset: (5, 2),
    };
    assert_eq!(clean_aperture(&c, 200, 100), Some((40, 28, 100, 50)));
    let full = craft_isobmff::CleanAperture {
        width: (200, 1),
        height: (100, 1),
        horiz_offset: (0, 1),
        vert_offset: (0, 1),
    };
    assert_eq!(clean_aperture(&full, 200, 100), None);
    let zero_den = craft_isobmff::CleanAperture {
        width: (200, 0),
        height: (100, 1),
        horiz_offset: (0, 1),
        vert_offset: (0, 1),
    };
    assert_eq!(clean_aperture(&zero_den, 200, 100), None);
    assert_eq!(matroska_crop((2, 4, 6, 8), 100, 50), Some((6, 2, 86, 44)));
    assert_eq!(matroska_crop((0, 60, 0, 0), 100, 50), None);

    let img = Rgba8Image::new(3, 2, (0..24).collect()).unwrap();
    let c = crop_to(&img, (1, 1, 2, 1)).unwrap();
    assert_eq!(c.data(), &[16, 17, 18, 19, 20, 21, 22, 23]);
    assert!(crop_to(&img, (2, 0, 2, 1)).is_none());
}

/// 25 frames of DNxHR HQ at 25 fps in OP1a MXF, frame `i` a flat grey of `luma(i)`, written
/// with the workspace's own DNx encoder and MXF writer.
fn dnx_mxf() -> Vec<u8> {
    let (w, h) = (256u32, 144u32);
    let mut enc = craft_dnx::Encoder::new(craft_dnx::Profile::Hq, w, h).unwrap();
    let rate = craft_mxf::Rational { num: 25, den: 1 };
    let mut cfg = craft_mxf::WriterConfig::new(
        craft_mxf::Pattern::Op1a,
        rate,
        craft_mxf::PackageIds::from_seed("preview", "DNx clip"),
    );
    cfg.picture = Some(craft_mxf::PictureDesc::new(
        craft_mxf::PictureCoding::Vc3 {
            cid: craft_dnx::Profile::Hq.cid(),
        },
        w,
        h,
    ));
    let mut wr = craft_mxf::MxfWriter::new(std::io::Cursor::new(Vec::new()), cfg).unwrap();
    for i in 0..25 {
        let mut f = craft_dnx::Frame::new(w, h, craft_dnx::ChromaFormat::Yuv422, 8, false);
        f.y.iter_mut().for_each(|s| *s = u16::from(luma(i)));
        wr.push_picture(&enc.encode(&f).unwrap(), craft_mxf::FrameInfo::intra())
            .unwrap();
    }
    wr.finish().unwrap().into_inner()
}

#[test]
fn dnx_in_mxf_through_craft_mxf_and_craft_dnx() {
    let path = write("clip.mxf", &dnx_mxf());
    for (at, frame) in [(0.0, 0usize), (0.4, 10), (5.0, 24)] {
        let f = poster_frame(&path, &FrameOptions::at(at)).unwrap();
        assert_eq!(
            (f.codec, f.source_width, f.source_height),
            ("dnxhd", 256, 144)
        );
        let p = f.image.pixel(128, 72).unwrap();
        assert!(
            p[0].abs_diff(grey(luma(frame))) <= 3,
            "at {at}s: {p:?}, want frame {frame}"
        );
        assert!(
            (f.seconds - frame as f64 / 25.0).abs() < 1e-6,
            "at {at}s: {}",
            f.seconds
        );
    }
}
