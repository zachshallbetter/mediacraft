//! Canonical bytes for [`DevelopSettings`]: RFC 8785 (JSON Canonicalization Scheme) serialisation, and
//! the check that the settings can be written that way at all.
//!
//! This module is MediaCraft's addition to the vendored LightCraft crate (docs/VENDORED.md). Hermit
//! stores develop settings as canonical bytes, so equal settings always give equal bytes, and the
//! bytes can be hashed, signed and compared without parsing.
//!
//! * [`validate_canonical`] says whether the settings can be written canonically: every float is
//!   finite (RFC 8785 has no NaN or infinity, and `serde_json` would quietly turn one into `null`,
//!   losing the value), every integer is exactly representable as an IEEE-754 double (JCS numbers
//!   are doubles), and every map serialises its keys in a stable order.
//! * [`to_canonical_json`] validates, then writes the bytes. `version` and `process` are always
//!   present in them. LightCraft leaves `process` out of its plain JSON while it is the legacy V1
//!   (so older settings keep their hash); the canonical form states it, as
//!   [`DevelopSettings::to_json_for_merge`] does for `detail.nr_outliers`.
//!
//! The JCS writer is written from the RFC text: object keys sorted by UTF-16 code units, numbers
//! in the ECMAScript `Number::toString` format, strings with the minimal escapes.

use std::cmp::Ordering;
use std::fmt;

use serde::Serialize;
use serde::ser::{self, Serializer};
use serde_json::{Map, Number, Value};

use crate::settings::{DevelopSettings, ProcessVersion};

/// The largest integer magnitude a double (and so a JCS number) holds exactly: 2^53.
const MAX_SAFE_INTEGER: u64 = 1 << 53;

/// Why settings cannot be written as canonical JSON. `path` is the field, like `light.exposure` or
/// `masks[2].feather`.
#[derive(Clone, Debug, PartialEq)]
pub enum CanonicalError {
    /// A NaN or infinity. RFC 8785 cannot encode one.
    NonFinite { path: String },
    /// An integer beyond ±2^53, which a JCS number cannot hold exactly.
    UnsafeInteger { path: String },
    /// A map whose keys are not serialised in ascending order, so its output order is not stable
    /// (a hash map, say).
    UnstableKeyOrder { path: String },
    /// A map with the same key twice. JSON objects need unique keys.
    DuplicateKey { path: String },
    /// A map key that is not a string.
    NonStringKey { path: String },
    /// The settings failed to serialise for another reason.
    Serialize(String),
}

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanonicalError::NonFinite { path } => write!(f, "{path}: NaN and infinity cannot be written as RFC 8785 JSON"),
            CanonicalError::UnsafeInteger { path } => write!(f, "{path}: integer beyond 2^53 is not exact as a JCS number"),
            CanonicalError::UnstableKeyOrder { path } => write!(f, "{path}: map keys are not serialised in a stable order"),
            CanonicalError::DuplicateKey { path } => write!(f, "{path}: duplicate map key"),
            CanonicalError::NonStringKey { path } => write!(f, "{path}: map key is not a string"),
            CanonicalError::Serialize(msg) => write!(f, "settings did not serialise: {msg}"),
        }
    }
}

impl std::error::Error for CanonicalError {}

impl ser::Error for CanonicalError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        CanonicalError::Serialize(msg.to_string())
    }
}

/// Checks that `settings` can be written as canonical JSON: no non-finite float, no integer beyond
/// 2^53, every map in a stable key order. Walks the serialisation itself rather than the
/// `serde_json::Value`, because `serde_json::to_value` turns a NaN into `null` and the value is
/// gone by the time the tree exists.
pub fn validate_canonical(settings: &DevelopSettings) -> Result<(), CanonicalError> {
    check(settings)
}

/// The RFC 8785 (JCS) bytes of `settings`. Equal settings give equal bytes, and parsing the bytes
/// and writing them again gives the same bytes. `version` and `process` are always present.
pub fn to_canonical_json(settings: &DevelopSettings) -> Result<Vec<u8>, CanonicalError> {
    validate_canonical(settings)?;
    let mut value = serde_json::to_value(settings).map_err(|e| CanonicalError::Serialize(e.to_string()))?;
    if let Some(object) = value.as_object_mut().filter(|o| !o.contains_key("process")) {
        // Left out of the plain JSON while it is V1; the canonical form says so.
        let process = serde_json::to_value(ProcessVersion::legacy()).map_err(|e| CanonicalError::Serialize(e.to_string()))?;
        object.insert("process".to_owned(), process);
    }
    jcs_bytes(&value)
}

/// Serialises a JSON value as RFC 8785 bytes. Fails on an integer beyond ±2^53.
pub fn jcs_bytes(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = String::new();
    write_value(&mut out, value, &mut String::new())?;
    Ok(out.into_bytes())
}

fn write_value(out: &mut String, value: &Value, path: &mut String) -> Result<(), CanonicalError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => write_number(out, n, path)?,
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let len = path.len();
                path.push_str(&format!("[{i}]"));
                write_value(out, item, path)?;
                path.truncate(len);
            }
            out.push(']');
        }
        Value::Object(map) => write_object(out, map, path)?,
    }
    Ok(())
}

fn write_object(out: &mut String, map: &Map<String, Value>, path: &mut String) -> Result<(), CanonicalError> {
    let mut entries: Vec<(&String, &Value)> = map.iter().collect();
    entries.sort_by(|a, b| cmp_utf16(a.0, b.0));
    out.push('{');
    for (i, (key, item)) in entries.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_string(out, key);
        out.push(':');
        let len = path.len();
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(key);
        write_value(out, item, path)?;
        path.truncate(len);
    }
    out.push('}');
    Ok(())
}

/// RFC 8785 §3.2.3: property names sort by their UTF-16 code units.
fn cmp_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// RFC 8785 §3.2.2.2: only `"` and `\` and the control characters below U+0020 are escaped; the
/// short forms for backspace, tab, line feed, form feed and carriage return, lower-case `\u00xx`
/// for the rest. Everything else, including U+007F and characters outside the BMP, is written as is.
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_number(out: &mut String, n: &Number, path: &str) -> Result<(), CanonicalError> {
    if let Some(u) = n.as_u64() {
        if u > MAX_SAFE_INTEGER {
            return Err(CanonicalError::UnsafeInteger { path: path.to_owned() });
        }
        out.push_str(&u.to_string());
    } else if let Some(i) = n.as_i64() {
        if i.unsigned_abs() > MAX_SAFE_INTEGER {
            return Err(CanonicalError::UnsafeInteger { path: path.to_owned() });
        }
        out.push_str(&i.to_string());
    } else if let Some(x) = n.as_f64() {
        if !x.is_finite() {
            return Err(CanonicalError::NonFinite { path: path.to_owned() });
        }
        out.push_str(&es_number(x));
    } else {
        return Err(CanonicalError::NonFinite { path: path.to_owned() });
    }
    Ok(())
}

/// ECMAScript `Number::toString(x)` (ECMA-262 §6.1.6.1.20) for a finite `x`, which RFC 8785 §3.2.2.3
/// requires. The shortest digits that read back as `x` come from Rust's `{:e}` formatting (also
/// shortest round-trip); this places the decimal point and exponent as ECMAScript does.
fn es_number(x: f64) -> String {
    if x == 0.0 {
        return "0".to_owned(); // also -0
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let shortest = format!("{:e}", x.abs()); // like "1.2345e-7" or "5e0"
    // Rust's shortest digits can differ from ECMAScript's at an exact tie (1424953923781206.25 is
    // "...206.3" there, "...206.2" in ECMAScript, which takes the even digit). The same number of
    // digits, rounded from the exact value, is the closest k-digit decimal; keep it when it reads back as `x`.
    let k = shortest.chars().take_while(|c| *c != 'e').filter(char::is_ascii_digit).count();
    let sci = match format!("{:.*e}", k.saturating_sub(1), x.abs()) {
        exact if exact.parse::<f64>().ok() == Some(x.abs()) => exact,
        _ => shortest,
    };
    let (mantissa, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exp10: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0').to_owned();
    let digits = if digits.is_empty() { "0".to_owned() } else { digits };
    let k = digits.len() as i32; // digits, no trailing zeros
    let n = exp10 + 1; // value = 0.DIGITS x 10^n
    let body = if (k..=21).contains(&n) {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if (1..=21).contains(&n) {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if (-5..=0).contains(&n) {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let e_sign = if e < 0 { '-' } else { '+' };
        if k == 1 { format!("{digits}e{e_sign}{}", e.abs()) } else { format!("{}.{}e{e_sign}{}", &digits[..1], &digits[1..], e.abs()) }
    };
    format!("{sign}{body}")
}

// ---------------------------------------------------------------------------------------------
// The check: a serde Serializer that writes nothing and looks at what it is given.
// ---------------------------------------------------------------------------------------------

fn check<T: Serialize + ?Sized>(value: &T) -> Result<(), CanonicalError> {
    let mut probe = Probe { path: String::new() };
    value.serialize(&mut probe)
}

struct Probe {
    path: String,
}

impl Probe {
    fn at<R>(&mut self, segment: &str, f: impl FnOnce(&mut Probe) -> R) -> R {
        let len = self.path.len();
        self.path.push_str(segment);
        let r = f(self);
        self.path.truncate(len);
        r
    }

    fn field_segment(&self, name: &str) -> String {
        if self.path.is_empty() { name.to_owned() } else { format!(".{name}") }
    }

    fn float(&self, x: f64) -> Result<(), CanonicalError> {
        if x.is_finite() { Ok(()) } else { Err(CanonicalError::NonFinite { path: self.path.clone() }) }
    }

    fn int(&self, magnitude: u128) -> Result<(), CanonicalError> {
        if magnitude <= u128::from(MAX_SAFE_INTEGER) { Ok(()) } else { Err(CanonicalError::UnsafeInteger { path: self.path.clone() }) }
    }
}

/// Compound values (sequences, tuples, structs, maps): checks each element, each map key's order.
struct Compound<'a> {
    probe: &'a mut Probe,
    index: usize,
    /// For maps: the previous key, to check the order, and the key of the entry being written.
    prev_key: Option<String>,
    key: Option<String>,
}

impl<'a> Compound<'a> {
    fn new(probe: &'a mut Probe) -> Self {
        Compound { probe, index: 0, prev_key: None, key: None }
    }

    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonicalError> {
        let segment = format!("[{}]", self.index);
        self.index += 1;
        self.probe.at(&segment, |p| value.serialize(&mut *p))
    }
}

impl ser::SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonicalError> {
        self.element(value)
    }
    fn end(self) -> Result<(), CanonicalError> {
        Ok(())
    }
}

impl ser::SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonicalError> {
        self.element(value)
    }
    fn end(self) -> Result<(), CanonicalError> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonicalError> {
        self.element(value)
    }
    fn end(self) -> Result<(), CanonicalError> {
        Ok(())
    }
}

impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonicalError> {
        self.element(value)
    }
    fn end(self) -> Result<(), CanonicalError> {
        Ok(())
    }
}

impl ser::SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), CanonicalError> {
        let segment = self.probe.field_segment(key);
        self.probe.at(&segment, |p| value.serialize(&mut *p))
    }
    fn end(self) -> Result<(), CanonicalError> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), CanonicalError> {
        ser::SerializeStruct::serialize_field(self, key, value)
    }
    fn end(self) -> Result<(), CanonicalError> {
        Ok(())
    }
}

impl ser::SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), CanonicalError> {
        let key = match serde_json::to_value(key) {
            Ok(Value::String(s)) => s,
            _ => return Err(CanonicalError::NonStringKey { path: self.probe.path.clone() }),
        };
        if let Some(prev) = &self.prev_key {
            // Ascending by code point (what a `BTreeMap<String, _>` gives) or by UTF-16 code unit
            // (what JCS sorts by) is a fixed order; anything else (a hash map) is not.
            if key == *prev {
                return Err(CanonicalError::DuplicateKey { path: self.probe.path.clone() });
            }
            if key < *prev && cmp_utf16(&key, prev) != Ordering::Greater {
                return Err(CanonicalError::UnstableKeyOrder { path: self.probe.path.clone() });
            }
        }
        self.prev_key = Some(key.clone());
        self.key = Some(key);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonicalError> {
        let key = self.key.take().unwrap_or_default();
        let segment = self.probe.field_segment(&key);
        self.probe.at(&segment, |p| value.serialize(&mut *p))
    }
    fn end(self) -> Result<(), CanonicalError> {
        Ok(())
    }
}

impl<'a> Serializer for &'a mut Probe {
    type Ok = ();
    type Error = CanonicalError;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;

    fn serialize_bool(self, _: bool) -> Result<(), CanonicalError> {
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<(), CanonicalError> {
        self.int(u128::from(v.unsigned_abs()))
    }
    fn serialize_i16(self, v: i16) -> Result<(), CanonicalError> {
        self.int(u128::from(v.unsigned_abs()))
    }
    fn serialize_i32(self, v: i32) -> Result<(), CanonicalError> {
        self.int(u128::from(v.unsigned_abs()))
    }
    fn serialize_i64(self, v: i64) -> Result<(), CanonicalError> {
        self.int(u128::from(v.unsigned_abs()))
    }
    fn serialize_i128(self, v: i128) -> Result<(), CanonicalError> {
        self.int(v.unsigned_abs())
    }
    fn serialize_u8(self, v: u8) -> Result<(), CanonicalError> {
        self.int(u128::from(v))
    }
    fn serialize_u16(self, v: u16) -> Result<(), CanonicalError> {
        self.int(u128::from(v))
    }
    fn serialize_u32(self, v: u32) -> Result<(), CanonicalError> {
        self.int(u128::from(v))
    }
    fn serialize_u64(self, v: u64) -> Result<(), CanonicalError> {
        self.int(u128::from(v))
    }
    fn serialize_u128(self, v: u128) -> Result<(), CanonicalError> {
        self.int(v)
    }
    fn serialize_f32(self, v: f32) -> Result<(), CanonicalError> {
        self.float(f64::from(v))
    }
    fn serialize_f64(self, v: f64) -> Result<(), CanonicalError> {
        self.float(v)
    }
    fn serialize_char(self, _: char) -> Result<(), CanonicalError> {
        Ok(())
    }
    fn serialize_str(self, _: &str) -> Result<(), CanonicalError> {
        Ok(())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), CanonicalError> {
        Ok(())
    }
    fn serialize_none(self) -> Result<(), CanonicalError> {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), CanonicalError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), CanonicalError> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), CanonicalError> {
        Ok(())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), CanonicalError> {
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<(), CanonicalError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), CanonicalError> {
        let segment = self.field_segment(variant);
        self.at(&segment, |p| value.serialize(&mut *p))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Compound<'a>, CanonicalError> {
        Ok(Compound::new(self))
    }
    fn serialize_tuple(self, _: usize) -> Result<Compound<'a>, CanonicalError> {
        Ok(Compound::new(self))
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Compound<'a>, CanonicalError> {
        Ok(Compound::new(self))
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Compound<'a>, CanonicalError> {
        Ok(Compound::new(self))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Compound<'a>, CanonicalError> {
        Ok(Compound::new(self))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Compound<'a>, CanonicalError> {
        Ok(Compound::new(self))
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Compound<'a>, CanonicalError> {
        Ok(Compound::new(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Process;
    use std::collections::BTreeMap;

    fn canon(s: &DevelopSettings) -> Vec<u8> {
        to_canonical_json(s).unwrap()
    }

    fn edited() -> DevelopSettings {
        let mut s = DevelopSettings::for_raw(5503.0, 11.0);
        s.light.exposure = 0.35;
        s.light.highlights = -41.5;
        s.crop.geometry.rect = craft_develop_geom::Rect::new(0.1, 0.2, 0.8, 0.9);
        s.disabled_sections = vec!["effects".into(), "détail \u{1f600}".into()];
        s
    }

    fn es(bits: u64) -> String {
        es_number(f64::from_bits(bits))
    }

    // --- RFC 8785 number serialisation (Appendix B) ----------------------------------------

    #[test]
    fn rfc8785_appendix_b_numbers() {
        let cases: [(u64, &str); 24] = [
            (0x0000_0000_0000_0000, "0"),
            (0x8000_0000_0000_0000, "0"),
            (0x0000_0000_0000_0001, "5e-324"),
            (0x8000_0000_0000_0001, "-5e-324"),
            (0x7fef_ffff_ffff_ffff, "1.7976931348623157e+308"),
            (0xffef_ffff_ffff_ffff, "-1.7976931348623157e+308"),
            (0x4340_0000_0000_0000, "9007199254740992"),
            (0xc340_0000_0000_0000, "-9007199254740992"),
            (0x4430_0000_0000_0000, "295147905179352830000"),
            (0x44b5_2d02_c7e1_4af5, "9.999999999999997e+22"),
            (0x44b5_2d02_c7e1_4af6, "1e+23"),
            (0x44b5_2d02_c7e1_4af7, "1.0000000000000001e+23"),
            (0x444b_1ae4_d6e2_ef4e, "999999999999999700000"),
            (0x444b_1ae4_d6e2_ef4f, "999999999999999900000"),
            (0x444b_1ae4_d6e2_ef50, "1e+21"),
            (0x3eb0_c6f7_a0b5_ed8c, "9.999999999999997e-7"),
            (0x3eb0_c6f7_a0b5_ed8d, "0.000001"),
            (0x41b3_de43_5555_5553, "333333333.3333332"),
            (0x41b3_de43_5555_5554, "333333333.33333325"),
            (0x41b3_de43_5555_5555, "333333333.3333333"),
            (0x41b3_de43_5555_5556, "333333333.3333334"),
            (0x41b3_de43_5555_5557, "333333333.33333343"),
            (0x4314_3ff3_c1cb_0959, "1424953923781206.2"),
            (0x3ff0_0000_0000_0000, "1"),
        ];
        for (bits, expected) in cases {
            assert_eq!(es(bits), expected, "bits {bits:016x}");
        }
    }

    #[test]
    fn es_number_places_the_point_and_the_exponent() {
        assert_eq!(es_number(0.5), "0.5");
        assert_eq!(es_number(-0.002), "-0.002");
        assert_eq!(es_number(4.5), "4.5");
        assert_eq!(es_number(1e30), "1e+30");
        assert_eq!(es_number(1e-27), "1e-27");
        assert_eq!(es_number(1e21), "1e+21");
        assert_eq!(es_number(1e20), "100000000000000000000");
        assert_eq!(es_number(1e-6), "0.000001");
        assert_eq!(es_number(1e-7), "1e-7");
        assert_eq!(es_number(123456.789), "123456.789");
        assert_eq!(es_number(-12.0), "-12");
    }

    // --- RFC 8785 §3.2.2 / §3.2.3 --------------------------------------------------------------

    #[test]
    fn rfc8785_section_3_2_2_example() {
        // Input of the RFC's main example, numbers written as the RFC writes them.
        let input = r#"{
            "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001],
            "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
            "literals": [null, true, false]
        }"#;
        let value: Value = serde_json::from_str(input).unwrap();
        let out = String::from_utf8(jcs_bytes(&value).unwrap()).unwrap();
        let expected = "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],\"string\":\"\u{20ac}$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}";
        assert_eq!(out, expected);
    }

    #[test]
    fn rfc8785_section_3_2_3_sorts_by_utf16_code_units() {
        let input = r#"{
            "\u20ac": "Euro Sign",
            "\r": "Carriage Return",
            "\ufb33": "Hebrew Letter Dalet With Dagesh",
            "1": "One",
            "\ud83d\ude00": "Emoji: Grinning Face",
            "\u0080": "Control",
            "\u00f6": "Latin Small Letter O With Diaeresis"
        }"#;
        let value: Value = serde_json::from_str(input).unwrap();
        let out = String::from_utf8(jcs_bytes(&value).unwrap()).unwrap();
        let keys: Vec<&str> = ["\\r", "1", "\u{80}", "\u{f6}", "\u{20ac}", "\u{1f600}", "\u{fb33}"].to_vec();
        let mut at = 0;
        for key in keys {
            let needle = format!("\"{key}\":");
            let found = out[at..].find(&needle).unwrap_or_else(|| panic!("{key:?} out of order in {out}"));
            at += found + needle.len();
        }
        // U+1F600 (surrogates D83D DE00) sorts before U+FB33, though its code point is larger.
        assert!(out.find('\u{1f600}').unwrap() < out.find('\u{fb33}').unwrap());
    }

    #[test]
    fn strings_use_the_minimal_escapes() {
        let mut out = String::new();
        write_string(&mut out, "a\"b\\c\u{8}\t\n\u{c}\r\u{1}\u{1f}\u{7f}/\u{e9}\u{1f600}");
        assert_eq!(out, "\"a\\\"b\\\\c\\b\\t\\n\\f\\r\\u0001\\u001f\u{7f}/\u{e9}\u{1f600}\"");
    }

    #[test]
    fn integers_beyond_2_pow_53_are_refused() {
        let ok: Value = serde_json::from_str("[9007199254740992, -9007199254740992]").unwrap();
        assert_eq!(jcs_bytes(&ok).unwrap(), b"[9007199254740992,-9007199254740992]");
        let too_big: Value = serde_json::from_str("[9007199254740993]").unwrap();
        assert_eq!(jcs_bytes(&too_big), Err(CanonicalError::UnsafeInteger { path: "[0]".into() }));
    }

    // --- the settings -------------------------------------------------------------------------------

    #[test]
    fn default_settings_round_trip_through_serde_json() {
        let s = DevelopSettings::default();
        let text = serde_json::to_string(&s).unwrap();
        let back: DevelopSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(back, s);
        let edited = edited();
        let back: DevelopSettings = serde_json::from_str(&serde_json::to_string(&edited).unwrap()).unwrap();
        assert_eq!(back, edited);
    }

    #[test]
    fn canonical_bytes_are_stable_and_survive_a_parse_round_trip() {
        for s in [DevelopSettings::default(), edited()] {
            let a = canon(&s);
            assert_eq!(a, canon(&s.clone()), "two serialisations differ");
            let parsed: DevelopSettings = serde_json::from_slice(&a).unwrap();
            assert_eq!(parsed, s, "canonical bytes do not read back as the same settings");
            assert_eq!(canon(&parsed), a, "parse then serialise changed the bytes");
        }
    }

    #[test]
    fn canonical_bytes_are_compact_sorted_json() {
        let text = String::from_utf8(canon(&edited())).unwrap();
        assert!(!text.contains('\n') && !text.contains(": ") && !text.contains(", "));
        let v: Value = serde_json::from_str(&text).unwrap();
        let top: Vec<&String> = v.as_object().unwrap().keys().collect();
        let mut sorted = top.clone();
        sorted.sort();
        assert_eq!(top, sorted);
        // The canonical text lists keys in sorted order, whatever order the struct declares them in.
        let (a, b) = (text.find("\"crop\"").unwrap(), text.find("\"wb\"").unwrap());
        assert!(a < b);
    }

    #[test]
    fn nan_and_infinity_are_rejected_wherever_they_sit() {
        let mut s = DevelopSettings::default();
        s.light.exposure = f64::NAN;
        assert_eq!(validate_canonical(&s), Err(CanonicalError::NonFinite { path: "light.exposure".into() }));
        assert!(to_canonical_json(&s).is_err());
        let mut s = DevelopSettings::default();
        s.wb.temp = f64::INFINITY;
        assert_eq!(validate_canonical(&s), Err(CanonicalError::NonFinite { path: "wb.temp".into() }));
        let mut s = DevelopSettings::default();
        s.light.blacks = f64::NEG_INFINITY;
        assert!(matches!(to_canonical_json(&s), Err(CanonicalError::NonFinite { .. })));
        // The plain serde_json path hides it: this is why the check walks the serialisation.
        assert_eq!(s.to_json()["light"]["blacks"], Value::Null);
        // Finite settings pass.
        assert_eq!(validate_canonical(&edited()), Ok(()));
        assert_eq!(validate_canonical(&DevelopSettings::default()), Ok(()));
    }

    #[test]
    fn nan_inside_a_list_is_found_with_its_path() {
        let mut s = DevelopSettings::default();
        s.curve.master = vec![craft_develop_geom::Point::new(0.0, 0.0), craft_develop_geom::Point::new(f64::NAN, 1.0)];
        assert_eq!(validate_canonical(&s), Err(CanonicalError::NonFinite { path: "curve.master[1].x".into() }));
    }

    #[test]
    fn version_and_process_are_always_present() {
        let default = DevelopSettings::default();
        let v: Value = serde_json::from_slice(&canon(&default)).unwrap();
        assert_eq!(v["version"], Value::from(crate::settings::SCHEMA_VERSION));
        assert_eq!(v["process"], Value::from(ProcessVersion::LATEST.0));

        // LightCraft's plain JSON leaves `process` out at V1; the canonical bytes state it.
        let legacy = DevelopSettings { process: ProcessVersion::legacy(), ..DevelopSettings::default() };
        assert!(legacy.to_json().get("process").is_none(), "upstream behaviour changed: re-check the canonical form");
        let v: Value = serde_json::from_slice(&canon(&legacy)).unwrap();
        assert_eq!(v["process"], Value::from(1));
        assert_eq!(v["version"], Value::from(crate::settings::SCHEMA_VERSION));
        let back: DevelopSettings = serde_json::from_slice(&canon(&legacy)).unwrap();
        assert_eq!(back, legacy);
        for p in Process::ALL {
            let s = DevelopSettings { process: p.version(), ..DevelopSettings::default() };
            let v: Value = serde_json::from_slice(&canon(&s)).unwrap();
            assert_eq!(v["process"], Value::from(p.version().0));
        }
    }

    // --- map key order --------------------------------------------------------------------------------

    struct Pairs(Vec<(&'static str, f64)>);
    impl Serialize for Pairs {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use ser::SerializeMap;
            let mut m = s.serialize_map(Some(self.0.len()))?;
            for (k, v) in &self.0 {
                m.serialize_entry(k, v)?;
            }
            m.end()
        }
    }

    #[test]
    fn maps_need_a_stable_key_order() {
        assert_eq!(check(&Pairs(vec![("a", 1.0), ("b", 2.0), ("c", 3.0)])), Ok(()));
        assert_eq!(check(&Pairs(vec![("b", 1.0), ("a", 2.0)])), Err(CanonicalError::UnstableKeyOrder { path: String::new() }));
        assert_eq!(check(&Pairs(vec![("a", 1.0), ("a", 2.0)])), Err(CanonicalError::DuplicateKey { path: String::new() }));
        assert_eq!(check(&Pairs(vec![("a", 1.0), ("b", f64::NAN)])), Err(CanonicalError::NonFinite { path: "b".into() }));
        let sorted: BTreeMap<String, f64> = [("x".to_owned(), 1.0), ("y".to_owned(), 2.0)].into();
        assert_eq!(check(&sorted), Ok(()));
    }

    #[test]
    fn the_settings_serialise_with_no_unstable_maps() {
        // Any map in the settings would have to be ordered; today there is none, and validation passes
        // for settings with every list and section populated.
        let mut s = edited();
        s.disabled_sections.push("zz".into());
        assert_eq!(validate_canonical(&s), Ok(()));
    }
}
