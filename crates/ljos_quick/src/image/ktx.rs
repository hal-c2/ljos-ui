//! KTX 1.1 pictures (`.ktx`): what light probes are baked into, usually six
//! faces of half floats with a mip chain.
//!
//! Only the first level is read: a baked chain holds rougher and rougher
//! reflections, and the renderer filters its own from what it is given. A
//! cube comes out as the equirectangular picture of Radiance's RGBE pixels
//! every other probe is, so that one path makes the maps of all of them.

use super::radiance::Rgbe;
use std::io::Read;

const MAGIC: [u8; 12] = [
    0xab, b'K', b'T', b'X', b' ', b'1', b'1', 0xbb, b'\r', b'\n', 0x1a, b'\n',
];
const HEADER: usize = 64;
const SAME_ORDER: u32 = 0x0403_0201;
const OTHER_ORDER: u32 = 0x0102_0304;

const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_FLOAT: u32 = 0x1406;
const GL_HALF_FLOAT: u32 = 0x140b;
const GL_RGB: u32 = 0x1907;
const GL_RGBA: u32 = 0x1908;
const GL_SRGB8: u32 = 0x8c41;
const GL_SRGB8_ALPHA8: u32 = 0x8c43;

/// A probe's reflections are filtered from a cube of at most this side,
/// which is more than the maps made from it can show; a larger
/// face is averaged down, so that a small bright sun is not lost or kept
/// by chance of where a sample falls.
const MOST_FACE_SIDE: usize = 512;

/// The bytes start as a KTX 1.1 file does.
pub(crate) fn is_ktx(bytes: &[u8]) -> bool {
    bytes.starts_with(&MAGIC)
}

/// The width and height the header of a KTX file gives, from as much of
/// the file as holds the header; None when it does not hold it.
pub(crate) fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let h = header(bytes).ok()?;
    Some((u32::try_from(h.width).ok()?, u32::try_from(h.height).ok()?))
}

#[derive(Clone, Copy)]
struct Header {
    swapped: bool,
    gl_type: u32,
    format: u32,
    internal: u32,
    width: usize,
    height: usize,
    depth: usize,
    elements: usize,
    faces: usize,
    key_values: usize,
}

fn word(bytes: &[u8], at: usize, swapped: bool) -> Option<u32> {
    let raw: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(if swapped {
        u32::from_be_bytes(raw)
    } else {
        u32::from_le_bytes(raw)
    })
}

fn header(bytes: &[u8]) -> Result<Header, String> {
    if !is_ktx(bytes) {
        return Err("not a KTX 1.1 file".to_owned());
    }
    if bytes.len() < HEADER {
        return Err("the KTX header is cut short".to_owned());
    }
    let swapped = match word(bytes, 12, false) {
        Some(SAME_ORDER) => false,
        Some(OTHER_ORDER) => true,
        _ => return Err("the KTX byte order mark is wrong".to_owned()),
    };
    let field = |at| word(bytes, at, swapped).map_or(0, |w| w as usize);
    Ok(Header {
        swapped,
        gl_type: field(16) as u32,
        format: field(24) as u32,
        internal: field(28) as u32,
        width: field(36),
        height: field(40),
        depth: field(44),
        elements: field(48),
        faces: field(52),
        key_values: field(60),
    })
}

/// What `reader` holds through the first level of a KTX file, or all of
/// it when it is anything else: the levels after the first make up nothing
/// that is read, and in a baked probe they are hundreds of megabytes.
pub(crate) fn read_through_first_level(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(HEADER as u64)
        .read_to_end(&mut bytes)?;
    let Ok(h) = header(&bytes) else {
        reader.read_to_end(&mut bytes)?;
        return Ok(bytes);
    };
    let sized = HEADER + h.key_values + 4;
    reader
        .by_ref()
        .take((h.key_values + 4) as u64)
        .read_to_end(&mut bytes)?;
    let image_size = word(&bytes, sized - 4, h.swapped).unwrap_or(0) as u64;
    let padded = (image_size + 3) & !3;
    let size = if h.faces == 6 && h.elements == 0 {
        padded * 6
    } else {
        padded
    };
    reader.take(size).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Decodes a KTX file whose result fits in `most_bytes`; the file may end
/// after its first level.
///
/// # Errors
/// The text says what is wrong with the file, in words fit for a log.
pub(crate) fn decode(bytes: &[u8], most_bytes: usize) -> Result<Rgbe, String> {
    let h = header(bytes)?;
    if h.gl_type == 0 {
        return Err(format!(
            "compressed KTX (internal format {:#x}) is not read",
            h.internal
        ));
    }
    let components = match h.format {
        GL_RGB => 3,
        GL_RGBA => 4,
        other => return Err(format!("KTX pixel format {other:#x} is not read")),
    };
    let size = match h.gl_type {
        GL_UNSIGNED_BYTE => 1,
        GL_HALF_FLOAT => 2,
        GL_FLOAT => 4,
        other => return Err(format!("KTX pixel type {other:#x} is not read")),
    };
    if h.width == 0 || h.height == 0 || h.depth != 0 || h.elements != 0 {
        return Err("only flat KTX pictures and cubes are read, not arrays or volumes".to_owned());
    }
    if h.faces != 1 && h.faces != 6 {
        return Err(format!("a KTX file has 1 or 6 faces, not {}", h.faces));
    }
    if h.faces == 6 && h.width != h.height {
        return Err("the faces of a KTX cube are square".to_owned());
    }
    let row = h
        .width
        .checked_mul(components * size)
        .and_then(|n| n.checked_add(3))
        .map(|n| n & !3)
        .ok_or("the KTX picture is too large")?;
    let face_bytes = row
        .checked_mul(h.height)
        .ok_or("the KTX picture is too large")?;
    let sized = HEADER
        .checked_add(h.key_values)
        .filter(|end| end.checked_add(4).is_some_and(|e| e <= bytes.len()))
        .ok_or("the KTX file ends before its first level")?;
    let image_size = word(bytes, sized, h.swapped).unwrap_or(0) as usize;
    if image_size != face_bytes {
        return Err(format!(
            "the first level of the KTX file says {image_size} bytes where its size takes {face_bytes}"
        ));
    }
    let start = sized + 4;
    let stride = (face_bytes + 3) & !3;
    let last = h
        .faces
        .checked_mul(stride)
        .and_then(|n| n.checked_add(start))
        .filter(|end| *end <= bytes.len());
    if last.is_none() {
        return Err("the KTX file ends inside its first level".to_owned());
    }
    let read = Pixels {
        swapped: h.swapped,
        gl_type: h.gl_type,
        srgb: h.internal == GL_SRGB8 || h.internal == GL_SRGB8_ALPHA8,
        components,
        size,
        row,
    };
    if h.faces == 1 {
        return flat(&h, &bytes[start..start + face_bytes], &read, most_bytes);
    }
    let faces: Vec<Face> = (0..6)
        .map(|f| {
            let at = start + f * stride;
            Face::averaged(&bytes[at..at + face_bytes], h.width, &read)
        })
        .collect();
    let width = faces[0].side * 4;
    let height = faces[0].side * 2;
    if width * height * 4 > most_bytes {
        return Err("the KTX picture is too large".to_owned());
    }
    Ok(Rgbe {
        width: width as u32,
        height: height as u32,
        data: equirectangular(&faces, width, height),
    })
}

/// How the pixels of a level are laid out and read.
struct Pixels {
    swapped: bool,
    gl_type: u32,
    srgb: bool,
    components: usize,
    size: usize,
    row: usize,
}

impl Pixels {
    /// The linear red, green and blue at the pixel `x`, `y` of `level`.
    fn at(&self, level: &[u8], x: usize, y: usize) -> [f32; 3] {
        let at = y * self.row + x * self.components * self.size;
        let channel = |c: usize| {
            let b = &level[at + c * self.size..at + (c + 1) * self.size];
            match self.gl_type {
                GL_HALF_FLOAT => {
                    let raw = [b[0], b[1]];
                    half(if self.swapped {
                        u16::from_be_bytes(raw)
                    } else {
                        u16::from_le_bytes(raw)
                    })
                }
                GL_FLOAT => {
                    let raw = [b[0], b[1], b[2], b[3]];
                    if self.swapped {
                        f32::from_be_bytes(raw)
                    } else {
                        f32::from_le_bytes(raw)
                    }
                }
                _ if self.srgb => from_srgb(f32::from(b[0]) / 255.0),
                _ => f32::from(b[0]) / 255.0,
            }
        };
        [channel(0), channel(1), channel(2)]
    }
}

fn flat(h: &Header, level: &[u8], read: &Pixels, most_bytes: usize) -> Result<Rgbe, String> {
    if h.width
        .checked_mul(h.height)
        .and_then(|n| n.checked_mul(4))
        .is_none_or(|n| n > most_bytes)
    {
        return Err("the KTX picture is too large".to_owned());
    }
    let mut data = Vec::with_capacity(h.width * h.height * 4);
    for y in 0..h.height {
        for x in 0..h.width {
            data.extend(rgbe(read.at(level, x, y)));
        }
    }
    Ok(Rgbe {
        width: h.width as u32,
        height: h.height as u32,
        data,
    })
}

/// One face of a cube in linear light.
struct Face {
    side: usize,
    pixels: Vec<[f32; 3]>,
}

impl Face {
    fn averaged(level: &[u8], side: usize, read: &Pixels) -> Self {
        let step = side.div_ceil(MOST_FACE_SIDE);
        let small = side / step;
        let mut pixels = Vec::with_capacity(small * small);
        let share = (step * step) as f32;
        for y in 0..small {
            for x in 0..small {
                let mut sum = [0.0; 3];
                for yy in y * step..(y + 1) * step {
                    for xx in x * step..(x + 1) * step {
                        let p = read.at(level, xx, yy);
                        // A NaN or a negative is no light, and one would
                        // spread through everything filtered from it.
                        for (s, c) in sum.iter_mut().zip(p) {
                            *s += if c > 0.0 && c.is_finite() { c } else { 0.0 };
                        }
                    }
                }
                pixels.push(sum.map(|s| s / share));
            }
        }
        Self {
            side: small,
            pixels,
        }
    }

    /// Bilinear between the pixels at `s`, `t` in 0..1 of the face. The
    /// edges clamp: a face is large enough that the seam does not show in
    /// what is filtered from it.
    fn sample(&self, s: f32, t: f32) -> [f32; 3] {
        let n = self.side;
        let at = |v: f32| {
            let v = (v * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
            let low = v.floor();
            (low as usize, (low as usize + 1).min(n - 1), v - low)
        };
        let (x0, x1, fx) = at(s);
        let (y0, y1, fy) = at(t);
        let p = |x: usize, y: usize| self.pixels[y * n + x];
        std::array::from_fn(|c| {
            let top = p(x0, y0)[c] * (1.0 - fx) + p(x1, y0)[c] * fx;
            let bottom = p(x0, y1)[c] * (1.0 - fx) + p(x1, y1)[c] * fx;
            top * (1.0 - fy) + bottom * fy
        })
    }
}

/// The picture a probe's maps are made from, with the direction each of
/// its pixels stands for as those maps read it.
fn equirectangular(faces: &[Face], width: usize, height: usize) -> Vec<u8> {
    use std::f32::consts::PI;
    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let (sin_t, cos_t) = ((y as f32 + 0.5) / height as f32 * PI).sin_cos();
        for x in 0..width {
            let (sin_p, cos_p) = (((x as f32 + 0.5) / width as f32 - 0.5) * 2.0 * PI).sin_cos();
            let (face, s, t) = cube_face([sin_t * sin_p, cos_t, -sin_t * cos_p]);
            data.extend(rgbe(faces[face].sample(s, t)));
        }
    }
    data
}

/// The face in the order a cube file stores them (+X, -X, +Y, -Y, +Z, -Z)
/// and the place on it in OpenGL's cube convention, whose first row is at
/// t = 0: `d` looks at it there.
fn cube_face([x, y, z]: [f32; 3]) -> (usize, f32, f32) {
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, sc, tc, ma) = if ax >= ay && ax >= az {
        if x > 0.0 {
            (0, -z, -y, ax)
        } else {
            (1, z, -y, ax)
        }
    } else if ay >= az {
        if y > 0.0 {
            (2, x, z, ay)
        } else {
            (3, x, -z, ay)
        }
    } else if z > 0.0 {
        (4, x, -y, az)
    } else {
        (5, -x, -y, az)
    };
    (face, (sc / ma + 1.0) / 2.0, (tc / ma + 1.0) / 2.0)
}

fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    sign * match exponent {
        0 => fraction * 2f32.powi(-24),
        31 if fraction == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15),
    }
}

fn from_srgb(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn rgbe(c: [f32; 3]) -> [u8; 4] {
    let [r, g, b] = c.map(|v| if v > 0.0 && v.is_finite() { v } else { 0.0 });
    let max = r.max(g).max(b);
    if max < 1e-32 {
        return [0; 4];
    }
    // max = m * 2^e with m in [0.5, 1); each channel keeps 8 bits of m.
    let e = max.log2().floor() as i32 + 1;
    let scale = 256.0 / 2f32.powi(e);
    let byte = |c: f32| (c * scale).min(255.0) as u8;
    [byte(r), byte(g), byte(b), (e + 128).clamp(0, 255) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of `faces` faces of half floats, each of its own grey.
    fn file(side: u32, greys: &[f32]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        let words = [
            SAME_ORDER,
            GL_HALF_FLOAT,
            2,
            GL_RGBA,
            0x881a,
            GL_RGBA,
            side,
            side,
            0,
            0,
            greys.len() as u32,
            1,
            4,
        ];
        words.iter().for_each(|w| out.extend(w.to_le_bytes()));
        out.extend([0; 4]);
        out.extend((side * side * 8).to_le_bytes());
        for grey in greys {
            let bits: u16 = match *grey {
                1.0 => 0x3c00,
                0.5 => 0x3800,
                0.25 => 0x3400,
                _ => 0,
            };
            for _ in 0..side * side * 4 {
                out.extend(bits.to_le_bytes());
            }
        }
        out
    }

    fn light(p: &[u8]) -> f32 {
        f32::from(p[0]) * 2f32.powi(i32::from(p[3]) - 128 - 8)
    }

    #[test]
    fn each_face_of_a_cube_is_where_it_looks() {
        let picture = decode(&file(4, &[1.0, 0.5, 0.25, 0.0, 1.0, 0.5]), 1 << 20).unwrap();
        assert_eq!((picture.width, picture.height), (16, 8));
        let at = |x: usize, y: usize| light(&picture.data[(y * 16 + x) * 4..]);
        // Up the middle, down at the bottom.
        assert_eq!(at(8, 0), 0.25);
        assert_eq!(at(8, 7), 0.0);
        // The middle of the picture looks along -Z, a quarter on along +X,
        // half way back along +Z, three quarters along -X.
        assert_eq!(at(0, 4), 1.0, "+Z");
        assert_eq!(at(4, 4), 0.5, "-X");
        assert_eq!(at(8, 4), 0.5, "-Z");
        assert_eq!(at(12, 4), 1.0, "+X");
    }

    #[test]
    fn a_row_starts_at_the_top_of_a_side_face() {
        let mut bytes = file(2, &[0.0; 6]);
        let start = bytes.len() - 6 * 2 * 2 * 8;
        // +Z is the fifth face; its first row is up.
        for c in bytes[start + 4 * 32..start + 4 * 32 + 16]
            .as_chunks_mut::<2>()
            .0
        {
            c.copy_from_slice(&0x3c00u16.to_le_bytes());
        }
        let picture = decode(&bytes, 1 << 20).unwrap();
        let at = |x: usize, y: usize| light(&picture.data[(y * 8 + x) * 4..]);
        assert!(at(0, 1) > 0.5, "above the horizon along +Z");
        assert!(at(0, 2) < at(0, 1), "below it");
    }

    #[test]
    fn a_large_face_is_averaged_not_sampled() {
        let side = MOST_FACE_SIDE + 2;
        let mut bytes = file(side as u32, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let start = bytes.len() - side * side * 8 * 6;
        // One pixel of +X is a small sun.
        bytes[start + 6..start + 8].copy_from_slice(&0x3c00u16.to_le_bytes());
        bytes[start..start + 2].copy_from_slice(&0x3c00u16.to_le_bytes());
        let picture = decode(&bytes, 256 << 20).unwrap();
        assert_eq!(picture.width as usize, side / 2 * 4);
        assert!(picture.data.as_chunks::<4>().0.iter().any(|p| p[3] != 0));
    }

    #[test]
    fn a_flat_picture_keeps_its_size() {
        let picture = decode(&file(3, &[0.5]), 1 << 20).unwrap();
        assert_eq!((picture.width, picture.height), (3, 3));
        assert_eq!(light(&picture.data), 0.5);
    }

    #[test]
    fn what_is_wrong_with_a_file_is_said() {
        let good = file(2, &[1.0; 6]);
        let error = |bytes: &[u8]| decode(bytes, 1 << 20).unwrap_err();
        assert!(error(&good[..40]).contains("header"));
        assert!(error(&good[..70]).contains("first level"));
        assert!(error(&good[..good.len() - 1]).contains("first level"));
        let mut order = good.clone();
        order[12] = 9;
        assert!(error(&order).contains("byte order"));
        let mut compressed = good.clone();
        compressed[16..20].fill(0);
        assert!(error(&compressed).contains("compressed"));
        let mut three = good.clone();
        three[52..56].copy_from_slice(&3u32.to_le_bytes());
        assert!(error(&three).contains("faces"));
        let mut huge = good.clone();
        huge[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
        huge[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(!error(&huge).is_empty());
        let mut key_values = good.clone();
        key_values[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(error(&key_values).contains("first level"));
    }

    #[test]
    fn only_the_first_level_is_read_from_a_file() {
        let mut bytes = file(2, &[1.0; 6]);
        let first = bytes.len();
        bytes.extend([7; 1000]);
        let read = read_through_first_level(&mut bytes.as_slice()).unwrap();
        assert_eq!(read.len(), first);
        assert_eq!(decode(&read, 1 << 20).unwrap().width, 8);
        let plain = b"#?RADIANCE and the rest of a much longer file than a KTX header is, so long that it runs beyond".to_vec();
        assert_eq!(
            read_through_first_level(&mut plain.as_slice()).unwrap(),
            plain
        );
    }

    #[test]
    fn halves_are_read() {
        assert_eq!(half(0x3c00), 1.0);
        assert_eq!(half(0xc000), -2.0);
        assert_eq!(half(0x0001), 2f32.powi(-24));
    }
}
