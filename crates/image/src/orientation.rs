//! The eight EXIF orientations (TIFF tag 274) as elements of the square's symmetry group, so
//! they compose and invert exactly and apply to pixels with one mapping.

use crate::{Error, Result, Rgba8Image};
use serde::{Deserialize, Serialize};

/// How the stored pixels must be transformed to display upright. Variant names say what to do
/// to the stored image; the EXIF value is in parentheses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Orientation {
    /// Already upright (1).
    #[default]
    Identity,
    /// Mirror left↔right (2).
    MirrorHorizontal,
    /// Turn 180° (3).
    Rotate180,
    /// Mirror top↔bottom (4).
    MirrorVertical,
    /// Mirror across the main diagonal (5).
    Transpose,
    /// Turn 90° clockwise (6).
    Rotate90,
    /// Mirror across the anti-diagonal (7).
    Transverse,
    /// Turn 90° anti-clockwise (8).
    Rotate270,
}

/// Integer 2×2 matrices acting on centred coordinates (x right, y down).
type Mat = [[i8; 2]; 2];

const ALL: [Orientation; 8] = [
    Orientation::Identity,
    Orientation::MirrorHorizontal,
    Orientation::Rotate180,
    Orientation::MirrorVertical,
    Orientation::Transpose,
    Orientation::Rotate90,
    Orientation::Transverse,
    Orientation::Rotate270,
];

impl Orientation {
    /// From the EXIF/TIFF value 1–8.
    pub fn from_exif(v: u16) -> Option<Self> {
        ALL.get(usize::from(v).checked_sub(1)?).copied()
    }

    /// The EXIF/TIFF value 1–8.
    pub fn to_exif(self) -> u16 {
        match self {
            Orientation::Identity => 1,
            Orientation::MirrorHorizontal => 2,
            Orientation::Rotate180 => 3,
            Orientation::MirrorVertical => 4,
            Orientation::Transpose => 5,
            Orientation::Rotate90 => 6,
            Orientation::Transverse => 7,
            Orientation::Rotate270 => 8,
        }
    }

    /// Whether displaying upright swaps width and height.
    pub fn swaps_dimensions(self) -> bool {
        self.matrix()[0][0] == 0
    }

    /// Display size of a `width × height` stored image.
    pub fn display_size(self, width: u32, height: u32) -> (u32, u32) {
        if self.swaps_dimensions() {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// `self` followed by `next`.
    pub fn then(self, next: Orientation) -> Orientation {
        let (a, b) = (next.matrix(), self.matrix());
        let mut m = [[0i8; 2]; 2];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j];
            }
        }
        Self::from_matrix(m)
    }

    /// The transform that undoes this one.
    pub fn inverse(self) -> Orientation {
        let m = self.matrix();
        // Orthogonal matrices: the inverse is the transpose.
        Self::from_matrix([[m[0][0], m[1][0]], [m[0][1], m[1][1]]])
    }

    fn matrix(self) -> Mat {
        match self {
            Orientation::Identity => [[1, 0], [0, 1]],
            Orientation::MirrorHorizontal => [[-1, 0], [0, 1]],
            Orientation::Rotate180 => [[-1, 0], [0, -1]],
            Orientation::MirrorVertical => [[1, 0], [0, -1]],
            Orientation::Transpose => [[0, 1], [1, 0]],
            // Clockwise on screen with y pointing down: (x, y) → (−y, x).
            Orientation::Rotate90 => [[0, -1], [1, 0]],
            Orientation::Transverse => [[0, -1], [-1, 0]],
            Orientation::Rotate270 => [[0, 1], [-1, 0]],
        }
    }

    fn from_matrix(m: Mat) -> Orientation {
        ALL.into_iter()
            .find(|o| o.matrix() == m)
            .unwrap_or(Orientation::Identity)
    }

    /// Transform stored pixels so they display upright.
    pub fn apply(self, img: &Rgba8Image) -> Result<Rgba8Image> {
        if self == Orientation::Identity {
            return Ok(img.clone());
        }
        let (w, h) = (img.width() as usize, img.height() as usize);
        let (dw, dh) = self.display_size(img.width(), img.height());
        let src = img.data();
        let mut out = vec![0u8; src.len()];
        let m = self.matrix();
        // Map each source pixel centre through the matrix about the image centre, using doubled
        // coordinates so every value stays an integer.
        let (cx, cy) = (w as i64 - 1, h as i64 - 1);
        let (dcx, dcy) = (dw as i64 - 1, dh as i64 - 1);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (2 * x as i64 - cx, 2 * y as i64 - cy);
                let tx = i64::from(m[0][0]) * sx + i64::from(m[0][1]) * sy;
                let ty = i64::from(m[1][0]) * sx + i64::from(m[1][1]) * sy;
                let (Ok(dx), Ok(dy)) = (
                    usize::try_from((tx + dcx) / 2),
                    usize::try_from((ty + dcy) / 2),
                ) else {
                    return Err(Error::InvalidArgument(
                        "orientation mapping left the image".into(),
                    ));
                };
                let si = (y * w + x) * 4;
                let di = (dy * dw as usize + dx) * 4;
                let (Some(s), Some(d)) = (src.get(si..si + 4), out.get_mut(di..di + 4)) else {
                    return Err(Error::InvalidArgument(
                        "orientation mapping left the image".into(),
                    ));
                };
                d.copy_from_slice(s);
            }
        }
        Rgba8Image::new(dw, dh, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3×2 image whose pixels are numbered 0..6 in the red channel.
    fn numbered() -> Rgba8Image {
        let mut d = Vec::new();
        for i in 0..6u8 {
            d.extend_from_slice(&[i, 0, 0, 255]);
        }
        Rgba8Image::new(3, 2, d).unwrap()
    }

    fn reds(img: &Rgba8Image) -> Vec<u8> {
        img.data().as_chunks::<4>().0.iter().map(|p| p[0]).collect()
    }

    #[test]
    fn exif_values_round_trip() {
        for v in 1..=8 {
            assert_eq!(Orientation::from_exif(v).unwrap().to_exif(), v);
        }
        assert_eq!(Orientation::from_exif(0), None);
        assert_eq!(Orientation::from_exif(9), None);
    }

    #[test]
    fn apply_matches_the_exif_definitions() {
        // Stored rows: [0 1 2] / [3 4 5].
        let img = numbered();
        let cases: [(u16, (u32, u32), [u8; 6]); 8] = [
            (1, (3, 2), [0, 1, 2, 3, 4, 5]),
            (2, (3, 2), [2, 1, 0, 5, 4, 3]),
            (3, (3, 2), [5, 4, 3, 2, 1, 0]),
            (4, (3, 2), [3, 4, 5, 0, 1, 2]),
            (5, (2, 3), [0, 3, 1, 4, 2, 5]),
            (6, (2, 3), [3, 0, 4, 1, 5, 2]),
            (7, (2, 3), [5, 2, 4, 1, 3, 0]),
            (8, (2, 3), [2, 5, 1, 4, 0, 3]),
        ];
        for (v, size, want) in cases {
            let o = Orientation::from_exif(v).unwrap();
            let out = o.apply(&img).unwrap();
            assert_eq!((out.width(), out.height()), size, "orientation {v}");
            assert_eq!(reds(&out), want, "orientation {v}");
        }
    }

    #[test]
    fn composition_and_inverse_agree_with_pixels() {
        let img = numbered();
        for a in ALL {
            assert_eq!(a.then(a.inverse()), Orientation::Identity);
            for b in ALL {
                let two_steps = b.apply(&a.apply(&img).unwrap()).unwrap();
                let one_step = a.then(b).apply(&img).unwrap();
                assert_eq!(two_steps, one_step, "{a:?} then {b:?}");
            }
        }
        assert_eq!(
            Orientation::Rotate90.then(Orientation::Rotate90),
            Orientation::Rotate180
        );
    }
}
