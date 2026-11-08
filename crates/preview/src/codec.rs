//! One decoder type over every video codec in the workspace, and its pictures as RGBA.

use crate::{Error, Result};
use craft_image::Rgba8Image;
use craft_image::yuv::{self, Plane};

/// H.273 matrix code for "unspecified": the converter then follows players (BT.709 for HD,
/// BT.601 for SD).
const UNSPECIFIED: u8 = 2;

pub(crate) enum Decoder {
    H264(Box<craft_h264::Decoder>),
    Hevc(Box<craft_hevc::Decoder>),
    Vp9(Box<craft_vp9::Decoder>),
    Av1(Box<craft_av1::Decoder>),
    Vp8(Box<craft_vp8::Decoder>),
    Mpeg2(Box<craft_mpeg2v::Decoder>),
    ProRes,
    Dnx,
    Mjpeg,
}

pub(crate) enum Picture {
    H264(craft_h264::Picture),
    Hevc(craft_hevc::Picture),
    Vp9(craft_vp9::Picture),
    Av1(craft_av1::Picture),
    Vp8(craft_vp8::Picture),
    Mpeg2(craft_mpeg2v::Picture),
    ProRes(craft_prores::Frame, i64),
    Dnx(craft_dnx::Frame, i64),
    Rgba(Rgba8Image, i64),
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Decode(e.to_string())
}

impl Decoder {
    pub(crate) fn h264(avcc: Option<&[u8]>) -> Result<Self> {
        Ok(Decoder::H264(Box::new(match avcc {
            Some(c) => craft_h264::Decoder::from_avcc(c).map_err(err)?,
            None => craft_h264::Decoder::new(),
        })))
    }

    pub(crate) fn hevc(hvcc: Option<&[u8]>) -> Result<Self> {
        Ok(Decoder::Hevc(Box::new(match hvcc {
            Some(c) => craft_hevc::Decoder::from_hvcc(c).map_err(err)?,
            None => craft_hevc::Decoder::new(),
        })))
    }

    /// AV1, primed with the container's configuration OBUs (often the only sequence header).
    pub(crate) fn av1(config_obus: &[u8]) -> Self {
        let mut d = craft_av1::Decoder::new();
        if !config_obus.is_empty() {
            // A bad config is reported again by the first frame; nothing to do with it here.
            let _ = d.decode(config_obus);
        }
        Decoder::Av1(Box::new(d))
    }

    pub(crate) fn vp9() -> Self {
        Decoder::Vp9(Box::new(craft_vp9::Decoder::new()))
    }

    pub(crate) fn vp8() -> Self {
        Decoder::Vp8(Box::new(craft_vp8::Decoder::new()))
    }

    pub(crate) fn mpeg2() -> Self {
        Decoder::Mpeg2(Box::new(craft_mpeg2v::Decoder::new()))
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Decoder::H264(_) => "h264",
            Decoder::Hevc(_) => "hevc",
            Decoder::Vp9(_) => "vp9",
            Decoder::Av1(_) => "av1",
            Decoder::Vp8(_) => "vp8",
            Decoder::Mpeg2(_) => "mpeg2video",
            Decoder::ProRes => "prores",
            Decoder::Dnx => "dnxhd",
            Decoder::Mjpeg => "mjpeg",
        }
    }

    pub(crate) fn decode(&mut self, data: &[u8], pts: i64) -> Result<Vec<Picture>> {
        Ok(match self {
            Decoder::H264(d) => d
                .decode(data, pts)
                .map_err(err)?
                .into_iter()
                .map(Picture::H264)
                .collect(),
            Decoder::Hevc(d) => d
                .decode(data, pts)
                .map_err(err)?
                .into_iter()
                .map(Picture::Hevc)
                .collect(),
            Decoder::Vp9(d) => d
                .decode(data, pts)
                .map_err(err)?
                .into_iter()
                .map(Picture::Vp9)
                .collect(),
            Decoder::Av1(d) => d
                .decode_pts(data, pts)
                .map_err(err)?
                .into_iter()
                .map(Picture::Av1)
                .collect(),
            Decoder::Vp8(d) => d
                .decode(data, pts)
                .map_err(err)?
                .into_iter()
                .map(Picture::Vp8)
                .collect(),
            Decoder::Mpeg2(d) => d
                .decode(data, pts)
                .map_err(err)?
                .into_iter()
                .map(Picture::Mpeg2)
                .collect(),
            Decoder::ProRes => vec![Picture::ProRes(
                craft_prores::decode_frame(data).map_err(err)?,
                pts,
            )],
            Decoder::Dnx => vec![Picture::Dnx(
                craft_dnx::decode_frame(data).map_err(err)?,
                pts,
            )],
            Decoder::Mjpeg => {
                let d = craft_image::decode(data, &craft_image::DecodeOptions::default())
                    .map_err(err)?;
                vec![Picture::Rgba(d.to_srgb().0, pts)]
            }
        })
    }

    pub(crate) fn flush(&mut self) -> Vec<Picture> {
        match self {
            Decoder::H264(d) => d.flush().into_iter().map(Picture::H264).collect(),
            Decoder::Hevc(d) => d.flush().into_iter().map(Picture::Hevc).collect(),
            Decoder::Vp9(d) => d.flush().into_iter().map(Picture::Vp9).collect(),
            Decoder::Av1(d) => d.flush().into_iter().map(Picture::Av1).collect(),
            Decoder::Mpeg2(d) => d.flush().into_iter().map(Picture::Mpeg2).collect(),
            Decoder::Vp8(_) | Decoder::ProRes | Decoder::Dnx | Decoder::Mjpeg => Vec::new(),
        }
    }

    /// Intra-only codecs decode the target sample alone.
    pub(crate) fn intra_only(&self) -> bool {
        matches!(self, Decoder::ProRes | Decoder::Dnx | Decoder::Mjpeg)
    }
}

fn plane<T>(data: &[T], (stride, width, height): (usize, u32, u32)) -> Plane<'_, T> {
    Plane {
        data,
        stride,
        width,
        height,
    }
}

/// Planes stored as `u8` or `u16` (HEVC, VP9).
fn planes8or16(
    y: (Option<&[u8]>, Option<&[u16]>),
    u: (Option<&[u8]>, Option<&[u16]>),
    v: (Option<&[u8]>, Option<&[u16]>),
    sizes: [(usize, u32, u32); 3],
    bits: u32,
    matrix: u8,
    full_range: bool,
    rgb: bool,
    display: yuv::Display,
) -> Option<Rgba8Image> {
    match (y.0, u.0, v.0) {
        (Some(y8), Some(u8_), Some(v8)) => {
            let (a, b, c) = (
                plane(y8, sizes[0]),
                plane(u8_, sizes[1]),
                plane(v8, sizes[2]),
            );
            if rgb {
                yuv::rgb_to_rgba_display(a, b, c, None, bits, full_range, display)
            } else {
                yuv::to_rgba_display(a, b, c, None, bits, matrix, full_range, display)
            }
        }
        _ => {
            let (a, b, c) = (
                plane(y.1?, sizes[0]),
                plane(u.1?, sizes[1]),
                plane(v.1?, sizes[2]),
            );
            if rgb {
                yuv::rgb_to_rgba_display(a, b, c, None, bits, full_range, display)
            } else {
                yuv::to_rgba_display(a, b, c, None, bits, matrix, full_range, display)
            }
        }
    }
}

/// A stream's signalled primaries and transfer, with the container's filling in what the stream
/// leaves unspecified (0, 2): BT.2020 is converted and PQ and HLG tone mapped to sRGB.
fn display(primaries: u8, transfer: u8, container: Option<(u8, u8)>) -> yuv::Display {
    let unset = |v: u8| matches!(v, 0 | 2);
    let (cp, ct) = container.unwrap_or((2, 2));
    yuv::Display {
        primaries: if unset(primaries) { cp } else { primaries },
        transfer: if unset(transfer) { ct } else { transfer },
    }
}

/// VP9 `color_space` as an H.273 matrix; `None` for sRGB (planes are G, B, R).
fn vp9_matrix(cs: u8) -> Option<u8> {
    match cs {
        1 | 3 => Some(6),
        2 => Some(1),
        4 => Some(7),
        5 => Some(9),
        7 => None,
        _ => Some(UNSPECIFIED),
    }
}

impl Picture {
    pub(crate) fn pts(&self) -> i64 {
        match self {
            Picture::H264(p) => p.pts,
            Picture::Hevc(p) => p.pts,
            Picture::Vp9(p) => p.pts,
            Picture::Av1(p) => p.pts,
            Picture::Vp8(p) => p.pts,
            Picture::Mpeg2(p) => p.pts,
            Picture::ProRes(_, pts) | Picture::Dnx(_, pts) | Picture::Rgba(_, pts) => *pts,
        }
    }

    /// Presentation order within a coded video sequence, where the bitstream carries one
    /// (H.264 and HEVC picture order count).
    pub(crate) fn order(&self) -> Option<i64> {
        match self {
            Picture::H264(p) => Some(i64::from(p.poc)),
            Picture::Hevc(p) => Some(i64::from(p.poc)),
            _ => None,
        }
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        match self {
            Picture::H264(p) => (p.width, p.height),
            Picture::Hevc(p) => (p.width, p.height),
            Picture::Vp9(p) => (p.width, p.height),
            Picture::Av1(p) => (p.width, p.height),
            Picture::Vp8(p) => (p.width, p.height),
            Picture::Mpeg2(p) => (p.width, p.height),
            Picture::ProRes(f, _) => (f.width, f.height),
            Picture::Dnx(f, _) => (f.width, f.height),
            Picture::Rgba(i, _) => (i.width(), i.height()),
        }
    }

    /// `container` is the container's (primaries, transfer), used where the stream leaves them
    /// unspecified.
    pub(crate) fn to_rgba(&self, container: Option<(u8, u8)>) -> Option<Rgba8Image> {
        let display = |primaries: u8, transfer: u8| display(primaries, transfer, container);
        match self {
            Picture::H264(p) => {
                let c = |d| Plane {
                    data: d,
                    stride: p.uv_stride,
                    width: p.chroma_width,
                    height: p.chroma_height,
                };
                let y = Plane {
                    data: &p.y[..],
                    stride: p.y_stride,
                    width: p.width,
                    height: p.height,
                };
                yuv::to_rgba_display(
                    y,
                    c(&p.u[..]),
                    c(&p.v[..]),
                    None,
                    8,
                    p.color.matrix,
                    p.color.full_range,
                    display(p.color.primaries, p.color.transfer),
                )
            }
            Picture::Hevc(p) => planes8or16(
                (p.y.as_u8(), p.y.as_u16()),
                (p.u.as_u8(), p.u.as_u16()),
                (p.v.as_u8(), p.v.as_u16()),
                [
                    (p.y_stride, p.width, p.height),
                    (p.uv_stride, p.chroma_width, p.chroma_height),
                    (p.uv_stride, p.chroma_width, p.chroma_height),
                ],
                p.bit_depth,
                p.color.matrix,
                p.color.full_range,
                false,
                display(p.color.primaries, p.color.transfer),
            ),
            Picture::Vp9(p) => {
                let matrix = vp9_matrix(p.color.color_space);
                planes8or16(
                    (p.y.as_u8(), p.y.as_u16()),
                    (p.u.as_u8(), p.u.as_u16()),
                    (p.v.as_u8(), p.v.as_u16()),
                    [
                        (p.y_stride, p.width, p.height),
                        (p.uv_stride, p.chroma_width, p.chroma_height),
                        (p.uv_stride, p.chroma_width, p.chroma_height),
                    ],
                    p.bit_depth,
                    matrix.unwrap_or(0),
                    p.color.full_range,
                    matrix.is_none(),
                    // VP9 signals BT.2020 but not its transfer; the container may.
                    display(if p.color.color_space == 5 { 9 } else { 2 }, 2),
                )
            }
            Picture::Av1(p) => {
                let (w1, h1) = (p.plane_width(1), p.plane_height(1));
                let neutral;
                let (u, v): (&[u16], &[u16]) = if p.mono_chrome || p.planes[1].is_empty() {
                    neutral = vec![1u16 << (p.bit_depth.max(1) - 1); w1 as usize * h1 as usize];
                    (&neutral, &neutral)
                } else {
                    (&p.planes[1], &p.planes[2])
                };
                let y = Plane {
                    data: &p.planes[0][..],
                    stride: p.width as usize,
                    width: p.width,
                    height: p.height,
                };
                let c = |d| Plane {
                    data: d,
                    stride: w1 as usize,
                    width: w1,
                    height: h1,
                };
                let bits = u32::from(p.bit_depth);
                // Matrix 0 is the identity: planes hold G, B, R.
                let d = display(p.color_primaries, p.transfer_characteristics);
                if p.matrix_coefficients == 0 && !p.mono_chrome {
                    yuv::rgb_to_rgba_display(y, c(u), c(v), None, bits, p.full_range, d)
                } else {
                    yuv::to_rgba_display(
                        y,
                        c(u),
                        c(v),
                        None,
                        bits,
                        p.matrix_coefficients,
                        p.full_range,
                        d,
                    )
                }
            }
            Picture::Vp8(p) => {
                let y = Plane {
                    data: &p.y[..],
                    stride: p.width as usize,
                    width: p.width,
                    height: p.height,
                };
                let c = |d| Plane {
                    data: d,
                    stride: p.chroma_width as usize,
                    width: p.chroma_width,
                    height: p.chroma_height,
                };
                // RFC 6386: BT.601, studio swing.
                yuv::to_rgba(y, c(&p.u[..]), c(&p.v[..]), None, 8, 6, false)
            }
            Picture::Mpeg2(p) => {
                let (cw, ch) = (p.chroma_width(), p.chroma_height());
                let y = Plane {
                    data: &p.y[..],
                    stride: p.width as usize,
                    width: p.width,
                    height: p.height,
                };
                let c = |d| Plane {
                    data: d,
                    stride: cw as usize,
                    width: cw,
                    height: ch,
                };
                let matrix = p.info.colour.map_or(UNSPECIFIED, |c| c.2);
                yuv::to_rgba(y, c(&p.cb[..]), c(&p.cr[..]), None, 8, matrix, false)
            }
            Picture::ProRes(f, _) => {
                let cw = f.chroma.chroma_width(f.width);
                let y = Plane {
                    data: &f.y[..],
                    stride: f.width as usize,
                    width: f.width,
                    height: f.height,
                };
                let c = |d| Plane {
                    data: d,
                    stride: cw as usize,
                    width: cw,
                    height: f.height,
                };
                let alpha = f.alpha.as_deref().map(|a| Plane {
                    data: a,
                    stride: f.width as usize,
                    width: f.width,
                    height: f.height,
                });
                // ProRes is video range.
                yuv::to_rgba(
                    y,
                    c(&f.cb[..]),
                    c(&f.cr[..]),
                    alpha,
                    u32::from(f.bit_depth),
                    f.color.matrix,
                    false,
                )
            }
            Picture::Dnx(f, _) => {
                let (cw, ch) = (
                    f.chroma.chroma_width(f.width),
                    f.chroma.chroma_height(f.height),
                );
                let y = Plane {
                    data: &f.y[..],
                    stride: f.width as usize,
                    width: f.width,
                    height: f.height,
                };
                let c = |d| Plane {
                    data: d,
                    stride: cw as usize,
                    width: cw,
                    height: ch,
                };
                let alpha = f.alpha.as_deref().map(|a| Plane {
                    data: a,
                    stride: f.width as usize,
                    width: f.width,
                    height: f.height,
                });
                let bits = u32::from(f.bit_depth);
                if f.rgb {
                    yuv::rgb_to_rgba(y, c(&f.cb[..]), c(&f.cr[..]), alpha, bits, false)
                } else {
                    let matrix = match f.color_volume {
                        craft_dnx::ColorVolume::Bt709 => 1,
                        craft_dnx::ColorVolume::Bt2020Ncl | craft_dnx::ColorVolume::Bt2020Cl => 9,
                        craft_dnx::ColorVolume::OutOfBand => UNSPECIFIED,
                    };
                    yuv::to_rgba(y, c(&f.cb[..]), c(&f.cr[..]), alpha, bits, matrix, false)
                }
            }
            Picture::Rgba(i, _) => Some(i.clone()),
        }
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;

    #[test]
    fn the_container_fills_in_what_the_stream_leaves_unspecified() {
        let pq_2020 = Some((9, 16));
        // x265 often writes transfer 0 while the container says PQ.
        assert_eq!(
            display(9, 0, pq_2020),
            yuv::Display {
                primaries: 9,
                transfer: 16
            }
        );
        assert_eq!(
            display(2, 2, pq_2020),
            yuv::Display {
                primaries: 9,
                transfer: 16
            }
        );
        // A stream's own values win.
        assert_eq!(
            display(1, 1, pq_2020),
            yuv::Display {
                primaries: 1,
                transfer: 1
            }
        );
        assert_eq!(
            display(9, 18, pq_2020),
            yuv::Display {
                primaries: 9,
                transfer: 18
            }
        );
        // Nothing anywhere: shown as it is.
        assert!(!display(2, 2, None).is_hdr());
    }
}
