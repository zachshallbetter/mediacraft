//! Signed media time and rational timebase representation.
//!
//! Follows the signed media time contract across `[i64::MIN, i64::MAX]`.
//! Negative timestamps represent valid media constructs (e.g. audio preroll,
//! count-in lead-in bars, negative edit trim offsets).

use serde::{Deserialize, Serialize};

/// Rational fraction used for frame rates, aspect ratios, and timebases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rational {
    pub num: u32,
    pub den: u32,
}

impl Rational {
    pub const ZERO: Self = Self { num: 0, den: 1 };

    pub const fn new(num: u32, den: u32) -> Self {
        Self {
            num,
            den: if den == 0 { 1 } else { den },
        }
    }

    /// Evaluates the rational value as a 64-bit floating-point number.
    pub fn as_f64(&self) -> f64 {
        if self.den == 0 {
            0.0
        } else {
            self.num as f64 / self.den as f64
        }
    }

    /// Returns the reciprocal fraction (den / num).
    pub fn invert(&self) -> Self {
        Self::new(self.den, self.num)
    }

    /// Reduces the fraction using the greatest common divisor.
    pub fn reduced(&self) -> Self {
        if self.num == 0 {
            return Self::new(0, 1);
        }
        let g = gcd(self.num, self.den);
        Self {
            num: self.num / g,
            den: self.den / g,
        }
    }

    /// Rescales an integer timestamp `val` from timebase `self` to timebase `target`.
    pub fn rescale(&self, val: i64, target: Rational) -> i64 {
        if self.den == 0 || target.num == 0 {
            return 0;
        }
        // val * (self.num / self.den) / (target.num / target.den)
        // = val * self.num * target.den / (self.den * target.num)
        let num = (self.num as i128).saturating_mul(target.den as i128);
        let den = (self.den as i128).saturating_mul(target.num as i128);
        if den == 0 {
            return 0;
        }
        let res = (val as i128).saturating_mul(num) / den;
        res.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

/// Signed media timestamp relative to a rational timebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaTime {
    /// Timestamp value in timebase units (signed).
    pub pts: i64,
    /// Decoding timestamp (if different from presentation timestamp).
    pub dts: i64,
    /// Duration of the frame/packet in timebase units.
    pub duration: u64,
    /// Rational timebase (e.g. 1/48000 for 48kHz audio, 1/24 for 24fps video).
    pub timebase: Rational,
}

impl MediaTime {
    pub const ZERO: Self = Self {
        pts: 0,
        dts: 0,
        duration: 0,
        timebase: Rational::new(1, 1000),
    };

    pub fn new(pts: i64, duration: u64, timebase: Rational) -> Self {
        Self {
            pts,
            dts: pts,
            duration,
            timebase,
        }
    }

    pub fn with_dts(pts: i64, dts: i64, duration: u64, timebase: Rational) -> Self {
        Self {
            pts,
            dts,
            duration,
            timebase,
        }
    }

    /// Evaluates presentation timestamp in seconds.
    pub fn pts_seconds(&self) -> f64 {
        if self.timebase.den == 0 {
            0.0
        } else {
            (self.pts as f64) * (self.timebase.num as f64) / (self.timebase.den as f64)
        }
    }

    /// Evaluates duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        if self.timebase.den == 0 {
            0.0
        } else {
            (self.duration as f64) * (self.timebase.num as f64) / (self.timebase.den as f64)
        }
    }

    /// Rescales this media timestamp to a new rational timebase.
    pub fn rescale(&self, target_timebase: Rational) -> Self {
        let new_pts = self.timebase.rescale(self.pts, target_timebase);
        let new_dts = self.timebase.rescale(self.dts, target_timebase);
        let new_duration = if self.duration > 0 {
            self.timebase
                .rescale(self.duration as i64, target_timebase)
                .max(0) as u64
        } else {
            0
        };
        Self {
            pts: new_pts,
            dts: new_dts,
            duration: new_duration,
            timebase: target_timebase,
        }
    }
}
