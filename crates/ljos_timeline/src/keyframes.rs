//! `keyframeSource` files: `["QTimelineKeyframes", 1, type, [frame, easing,
//! value...]]` in CBOR, a value being as many numbers as its type has
//! members. Only what such a file is made of is read: arrays, numbers, text,
//! true and false.

use std::sync::Arc;

use ljos::item::{Vector2d, Vector4d};
use ljos::prelude::*;

/// One keyframe of a file; `easing` is an `Easing.Type`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FileKey {
    pub frame: f64,
    pub easing: i32,
    pub value: Value,
}

/// Why a file was refused, in the words of the warning.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Corrupt {
    #[error("invalid format.(array expected)")]
    NotAnArray,
    #[error("invalid data size")]
    Size,
    #[error("invalid keyframeSource header string")]
    Header,
    #[error("invalid keyframeSource version {0}")]
    Version(i64),
    #[error("unsupported property type")]
    PropertyType,
    #[error("invalid keyframe array")]
    Keyframes,
}

enum Item {
    Number(f64),
    Bool(bool),
    Text(String),
    List(Vec<Item>),
}

impl Item {
    fn number(&self) -> Option<f64> {
        match self {
            Item::Number(n) => Some(*n),
            Item::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    depth: u32,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let end = self.at.checked_add(n)?;
        let got = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(got)
    }

    fn argument(&mut self, info: u8) -> Option<u64> {
        Some(match info {
            0..=23 => u64::from(info),
            24 => u64::from(self.take(1)?[0]),
            25 => u64::from(u16::from_be_bytes(self.take(2)?.try_into().ok()?)),
            26 => u64::from(u32::from_be_bytes(self.take(4)?.try_into().ok()?)),
            27 => u64::from_be_bytes(self.take(8)?.try_into().ok()?),
            _ => return None,
        })
    }

    fn item(&mut self) -> Option<Item> {
        let first = self.take(1)?[0];
        let (major, info) = (first >> 5, first & 31);
        Some(match major {
            0 => Item::Number(self.argument(info)? as f64),
            1 => Item::Number(-1.0 - self.argument(info)? as f64),
            3 => {
                let n = usize::try_from(self.argument(info)?).ok()?;
                Item::Text(String::from_utf8_lossy(self.take(n)?).into_owned())
            }
            4 => self.list(info)?,
            7 => match info {
                20 => Item::Bool(false),
                21 => Item::Bool(true),
                25 => {
                    let bits = u16::from_be_bytes(self.take(2)?.try_into().ok()?);
                    Item::Number(half(bits))
                }
                26 => {
                    let raw = self.take(4)?.try_into().ok()?;
                    Item::Number(f64::from(f32::from_be_bytes(raw)))
                }
                27 => Item::Number(f64::from_be_bytes(self.take(8)?.try_into().ok()?)),
                _ => return None,
            },
            _ => return None,
        })
    }

    fn list(&mut self, info: u8) -> Option<Item> {
        // A corrupt file must not be able to run the stack out.
        if self.depth >= 8 {
            return None;
        }
        self.depth += 1;
        let mut list = Vec::new();
        if info == 31 {
            while *self.bytes.get(self.at)? != 0xff {
                list.push(self.item()?);
            }
            self.at += 1;
        } else {
            let n = self.argument(info)?;
            // Each item is a byte at least, so a length is not believed
            // beyond the bytes left.
            if n > (self.bytes.len() - self.at) as u64 {
                return None;
            }
            for _ in 0..n {
                list.push(self.item()?);
            }
        }
        self.depth -= 1;
        Some(Item::List(list))
    }
}

fn half(bits: u16) -> f64 {
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f64::from(bits & 1023);
    let size = match exponent {
        0 => fraction * 2f64.powi(-24),
        31 if fraction != 0.0 => f64::NAN,
        31 => f64::INFINITY,
        _ => (fraction + 1024.0) * 2f64.powi(exponent - 25),
    };
    if bits & 0x8000 != 0 { -size } else { size }
}

/// How many numbers a value of Qt's type number `ty` takes.
fn members(ty: i64) -> Option<usize> {
    Some(match ty {
        1 | 2 | 6 | 38 => 1,
        21 | 22 | 25 | 26 | 0x1012 => 2,
        0x1013 => 3,
        19 | 20 | 0x1003 | 0x1014 | 0x1015 => 4,
        _ => return None,
    })
}

fn make(ty: i64, n: &[f64]) -> Value {
    let f = |i: usize| n[i] as f32;
    let whole = |i: usize| n[i].trunc();
    match ty {
        1 => Value::Bool(n[0] != 0.0),
        2 => Value::Int(n[0] as i32),
        6 | 38 => Value::Real(n[0]),
        19 => rect(whole(0), whole(1), whole(2), whole(3)),
        20 => rect(n[0], n[1], n[2], n[3]),
        21 => size(whole(0), whole(1)),
        22 => size(n[0], n[1]),
        25 => point(whole(0), whole(1)),
        26 => point(n[0], n[1]),
        0x1003 => {
            let c = |i: usize| n[i].trunc().clamp(0.0, 255.0) as u8;
            Value::Color(Color::from_rgba8(c(0), c(1), c(2), c(3)))
        }
        0x1012 => Value::Vector2d(Vector2d { x: f(0), y: f(1) }),
        0x1013 => Value::Vector3d(Vector3d {
            x: f(0),
            y: f(1),
            z: f(2),
        }),
        0x1014 => Value::Vector4d(Arc::new(Vector4d {
            x: f(0),
            y: f(1),
            z: f(2),
            w: f(3),
        })),
        _ => Value::Quaternion(Arc::new(Quaternion {
            scalar: f(0),
            x: f(1),
            y: f(2),
            z: f(3),
        })),
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Value {
    Value::Rect(Arc::new(Rect {
        x,
        y,
        width,
        height,
    }))
}

fn size(width: f64, height: f64) -> Value {
    Value::Size(Arc::new(Size { width, height }))
}

fn point(x: f64, y: f64) -> Value {
    Value::Point(Arc::new(Point { x, y }))
}

/// The keyframes a file holds.
pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<FileKey>, Corrupt> {
    let mut reader = Reader {
        bytes,
        at: 0,
        depth: 0,
    };
    let Some(Item::List(file)) = reader.item() else {
        return Err(Corrupt::NotAnArray);
    };
    let [header, version, ty, data] = file.as_slice() else {
        return Err(Corrupt::Size);
    };
    if !matches!(header, Item::Text(t) if t == "QTimelineKeyframes") {
        return Err(Corrupt::Header);
    }
    let version = version.number().unwrap_or(0.0);
    if version != 1.0 {
        return Err(Corrupt::Version(version as i64));
    }
    let ty = ty.number().unwrap_or(0.0) as i64;
    let Some(count) = members(ty) else {
        return Err(Corrupt::PropertyType);
    };
    let Item::List(data) = data else {
        return Ok(Vec::new());
    };
    let stride = count + 2;
    if !data.len().is_multiple_of(stride) {
        return Err(Corrupt::Keyframes);
    }
    let mut keys = Vec::with_capacity(data.len() / stride);
    let mut numbers = Vec::with_capacity(count);
    for key in data.chunks_exact(stride) {
        let frame = key[0].number().ok_or(Corrupt::Keyframes)?;
        let easing = key[1].number().ok_or(Corrupt::Keyframes)?;
        if easing.fract() != 0.0 {
            return Err(Corrupt::Keyframes);
        }
        numbers.clear();
        for n in &key[2..] {
            numbers.push(n.number().ok_or(Corrupt::Keyframes)?);
        }
        keys.push(FileKey {
            frame,
            easing: easing as i32,
            value: make(ty, &numbers),
        });
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `["QTimelineKeyframes", version, ty, data]` in CBOR.
    fn file(version: u8, ty: u16, data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x84, 0x72];
        out.extend_from_slice(b"QTimelineKeyframes");
        out.push(version);
        out.push(0x19);
        out.extend_from_slice(&ty.to_be_bytes());
        out.extend_from_slice(data);
        out
    }

    fn double(n: f64) -> Vec<u8> {
        let mut out = vec![0xfb];
        out.extend_from_slice(&n.to_be_bytes());
        out
    }

    #[test]
    fn a_vector_file_reads_as_vectors() {
        let mut data = vec![0x85];
        data.extend(double(33.5));
        data.push(0x01); // Easing.InQuad.
        data.extend(double(1.0));
        data.extend(double(2.0));
        data.extend(double(3.0));
        let keys = parse(&file(1, 0x1013, &data)).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].frame, 33.5);
        assert_eq!(keys[0].easing, 1);
        assert_eq!(
            keys[0].value,
            Value::Vector3d(Vector3d {
                x: 1.0,
                y: 2.0,
                z: 3.0
            })
        );
    }

    #[test]
    fn a_quaternion_is_scalar_first_and_singles_and_halves_read() {
        // An indefinite array: a half (1.5) for the frame, a whole easing, four
        // singles.
        let mut data = vec![0x9f, 0xf9, 0x3e, 0x00, 0x00];
        for n in [0.5f32, 0.25, 0.125, 1.0] {
            data.push(0xfa);
            data.extend_from_slice(&n.to_be_bytes());
        }
        data.push(0xff);
        let keys = parse(&file(1, 0x1015, &data)).unwrap();
        assert_eq!(keys[0].frame, 1.5);
        let Value::Quaternion(q) = &keys[0].value else {
            panic!("{:?}", keys[0].value);
        };
        assert_eq!((q.scalar, q.x, q.y, q.z), (0.5, 0.25, 0.125, 1.0));
    }

    #[test]
    fn what_is_wrong_with_a_file_is_said() {
        assert_eq!(parse(b"not cbor"), Err(Corrupt::NotAnArray));
        assert_eq!(parse(&[0x82, 0x01, 0x01]), Err(Corrupt::Size));
        let mut wrong = file(1, 6, &[0x80]);
        wrong[2] = b'X';
        assert_eq!(parse(&wrong), Err(Corrupt::Header));
        assert_eq!(parse(&file(2, 6, &[0x80])), Err(Corrupt::Version(2)));
        assert_eq!(parse(&file(1, 999, &[0x80])), Err(Corrupt::PropertyType));
        assert_eq!(
            parse(&file(1, 6, &[0x82, 0x01, 0x01])),
            Err(Corrupt::Keyframes)
        );
    }

    #[test]
    fn a_length_beyond_the_file_is_refused_and_depth_is_bounded() {
        assert_eq!(
            parse(&[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Err(Corrupt::NotAnArray)
        );
        assert_eq!(parse(&[0x9f; 100]), Err(Corrupt::NotAnArray));
    }
}
