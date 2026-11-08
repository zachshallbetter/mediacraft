//! OpenEXR: the attribute list of the first header gives the data window and the channels.

use super::RawHeader;
use crate::bytes::le_u32;
use crate::{Error, Format, Result};

/// Attributes read before giving up; real headers have a few dozen.
const MAX_ATTRIBUTES: usize = 512;

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn cstr(&mut self) -> Option<&'a [u8]> {
        let rest = self.b.get(self.at..)?;
        let n = rest.iter().position(|&c| c == 0)?;
        self.at += n + 1;
        rest.get(..n)
    }

    fn i32(&mut self) -> Option<i32> {
        let v = le_u32(self.b, self.at)? as i32;
        self.at += 4;
        Some(v)
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
}

pub(crate) fn header(b: &[u8]) -> Result<RawHeader> {
    let bad = |why: &str| Error::Malformed(Format::Exr, why.into());
    let cut = || bad("header is cut short");
    let mut c = Cursor { b, at: 8 }; // magic and version/flags
    if b.len() < 8 || b.get(4) != Some(&2) {
        return Err(bad("unsupported version"));
    }
    let mut window = None;
    let mut channels: Option<(u8, bool)> = None;
    for _ in 0..MAX_ATTRIBUTES {
        let name = c.cstr().ok_or_else(cut)?;
        if name.is_empty() {
            break;
        }
        let _ty = c.cstr().ok_or_else(cut)?;
        let size = usize::try_from(c.i32().ok_or_else(cut)?)
            .map_err(|_| bad("negative attribute size"))?;
        let value = c.take(size).ok_or_else(cut)?;
        match name {
            b"dataWindow" => {
                let f = |i: usize| le_u32(value, i * 4).map(|v| i64::from(v as i32));
                let (Some(x0), Some(y0), Some(x1), Some(y1)) = (f(0), f(1), f(2), f(3)) else {
                    return Err(bad("data window is cut short"));
                };
                window = Some((x1 - x0 + 1, y1 - y0 + 1));
            }
            b"channels" => {
                channels = Some(channel_list(value).ok_or_else(|| bad("channel list is damaged"))?)
            }
            _ => {}
        }
        if window.is_some() && channels.is_some() {
            break;
        }
    }
    let (Some((w, h)), Some((bits, alpha))) = (window, channels) else {
        return Err(bad("data window or channel list is missing"));
    };
    let (Ok(width), Ok(height)) = (u32::try_from(w), u32::try_from(h)) else {
        return Err(bad("data window is empty or too large"));
    };
    if width == 0 || height == 0 {
        return Err(bad("zero image size"));
    }
    Ok(RawHeader {
        width,
        height,
        bit_depth: Some(bits),
        has_alpha: Some(alpha),
        animated: false,
        orientation: None,
    })
}

/// The widest sample type in a `chlist` and whether a channel is alpha (`A`, or `layer.A`).
fn channel_list(v: &[u8]) -> Option<(u8, bool)> {
    let mut c = Cursor { b: v, at: 0 };
    let (mut bits, mut alpha, mut n) = (0u8, false, 0usize);
    loop {
        let name = c.cstr()?;
        if name.is_empty() {
            break;
        }
        n += 1;
        if n > 1024 {
            return None;
        }
        // Pixel type (UINT 0, HALF 1, FLOAT 2), linear flag and 3 reserved bytes, then sampling.
        let ty = c.i32()?;
        c.take(4 + 8)?;
        bits = bits.max(match ty {
            1 => 16,
            0 | 2 => 32,
            _ => return None,
        });
        alpha |= name == b"A" || name.ends_with(b".A");
    }
    (n > 0).then_some((bits, alpha))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn attr(out: &mut Vec<u8>, name: &str, ty: &str, value: &[u8]) {
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        out.extend_from_slice(ty.as_bytes());
        out.push(0);
        out.extend_from_slice(&(value.len() as i32).to_le_bytes());
        out.extend_from_slice(value);
    }

    #[test]
    fn data_window_and_channels() {
        let mut b = vec![0x76, 0x2F, 0x31, 0x01, 2, 0, 0, 0];
        let mut ch = Vec::new();
        for n in ["A", "B", "G", "R"] {
            ch.extend_from_slice(n.as_bytes());
            ch.push(0);
            ch.extend_from_slice(&1i32.to_le_bytes()); // HALF
            ch.extend_from_slice(&[0; 4]);
            ch.extend_from_slice(&1i32.to_le_bytes());
            ch.extend_from_slice(&1i32.to_le_bytes());
        }
        ch.push(0);
        attr(&mut b, "channels", "chlist", &ch);
        let win: Vec<u8> = [-2i32, 5, 12, 9]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        attr(&mut b, "dataWindow", "box2i", &win);
        b.push(0);
        let h = header(&b).unwrap();
        assert_eq!(
            (h.width, h.height, h.bit_depth, h.has_alpha),
            (15, 5, Some(16), Some(true))
        );
        assert!(header(&b[..b.len() - 30]).is_err());
    }
}
