//! Radiance HDR pictures (`.hdr`, "RGBE"): light beyond what a byte holds,
//! as a light probe is made of.
//!
//! The pixels are kept as they are stored, a shared exponent and three
//! mantissas a pixel, four bytes as an ordinary picture's are: a renderer
//! turns them into light where it reads them, so a probe costs no more
//! than a picture of its size until then.
//!
//! The format is the one Greg Ward's Radiance writes: a text header that
//! starts `#?`, a blank line, a resolution line, then scanlines that are
//! flat, run-length encoded per channel, or in the older run-length form.

/// The bytes start as a Radiance picture does.
pub(crate) fn is_radiance(bytes: &[u8]) -> bool {
    bytes.starts_with(b"#?RADIANCE") || bytes.starts_with(b"#?RGBE")
}

/// A decoded picture: its size and its RGBE pixels, top row first.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Rgbe {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Decodes a Radiance picture whose pixels fit in `most_bytes`.
///
/// # Errors
/// The text says what is wrong with the file, in words fit for a log.
pub(crate) fn decode(bytes: &[u8], most_bytes: usize) -> Result<Rgbe, String> {
    let mut at = 0;
    let mut line = || -> Result<&[u8], String> {
        let rest = &bytes[at..];
        let end = rest
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("the header does not end")?;
        at += end + 1;
        Ok(&rest[..end])
    };
    if !line()?.starts_with(b"#?") {
        return Err("not a Radiance picture".to_owned());
    }
    loop {
        let l = line()?;
        if l.is_empty() {
            break;
        }
        if let Some(format) = l.strip_prefix(b"FORMAT=")
            && format != b"32-bit_rle_rgbe"
        {
            // XYZE pixels need a colour transform this does not make.
            return Err(format!(
                "pixels in {} are not read",
                String::from_utf8_lossy(format)
            ));
        }
    }
    let resolution = String::from_utf8_lossy(line()?).into_owned();
    let (flip, height, width) = match resolution.split_whitespace().collect::<Vec<_>>()[..] {
        [y @ ("-Y" | "+Y"), h, "+X", w] => (y == "+Y", h, w),
        _ => return Err(format!("the orientation {resolution:?} is not read")),
    };
    let side = |n: &str| {
        n.parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("{n:?} is not a size"))
    };
    let (width, height) = (side(width)?, side(height)?);
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= most_bytes)
        .ok_or("the picture is too large")?;
    let mut data = vec![0u8; len];
    let row = width as usize * 4;
    let mut src = &bytes[at..];
    for y in 0..height as usize {
        let out = &mut data[y * row..(y + 1) * row];
        src = scanline(src, out, width as usize)
            .ok_or_else(|| format!("scanline {y} of {height} is cut short or broken"))?;
    }
    if flip {
        // "+Y": the bottom row comes first.
        let rows: Vec<&[u8]> = data.chunks(row).rev().collect();
        data = rows.concat();
    }
    Ok(Rgbe {
        width,
        height,
        data,
    })
}

/// Reads one scanline of `width` pixels into `out`; what follows it.
fn scanline<'a>(src: &'a [u8], out: &mut [u8], width: usize) -> Option<&'a [u8]> {
    let new_rle = (8..=0x7fff).contains(&width)
        && src.len() >= 4
        && src[0] == 2
        && src[1] == 2
        && src[2] & 0x80 == 0
        && (usize::from(src[2]) << 8 | usize::from(src[3])) == width;
    if new_rle {
        return per_channel(&src[4..], out, width);
    }
    old(src, out, width)
}

/// Each of the four channels in turn, in runs (a count above 128 repeats
/// the next byte, count − 128 times) and literals (a count of at most 128
/// bytes as they are).
fn per_channel<'a>(mut src: &'a [u8], out: &mut [u8], width: usize) -> Option<&'a [u8]> {
    for channel in 0..4 {
        let mut x = 0;
        while x < width {
            let (&count, rest) = src.split_first()?;
            src = rest;
            if count > 128 {
                let n = usize::from(count - 128);
                let (&value, rest) = src.split_first()?;
                src = rest;
                if x + n > width {
                    return None;
                }
                for i in x..x + n {
                    out[i * 4 + channel] = value;
                }
                x += n;
            } else {
                let n = usize::from(count);
                if n == 0 || x + n > width || src.len() < n {
                    return None;
                }
                for (i, v) in src[..n].iter().enumerate() {
                    out[(x + i) * 4 + channel] = *v;
                }
                src = &src[n..];
                x += n;
            }
        }
    }
    Some(src)
}

/// Flat pixels, where a pixel of 1, 1, 1 repeats the one before it as
/// often as its exponent says, shifted further by each such pixel in a row.
fn old<'a>(mut src: &'a [u8], out: &mut [u8], width: usize) -> Option<&'a [u8]> {
    let mut x = 0;
    let mut shift = 0;
    while x < width {
        let (px, rest) = src.split_first_chunk::<4>()?;
        src = rest;
        if px[..3] == [1, 1, 1] {
            let n = usize::from(px[3]).checked_shl(shift)?;
            if x == 0 || x + n > width {
                return None;
            }
            let prev: [u8; 4] = out[(x - 1) * 4..x * 4].try_into().ok()?;
            for i in x..x + n {
                out[i * 4..i * 4 + 4].copy_from_slice(&prev);
            }
            x += n;
            shift += 8;
        } else {
            out[x * 4..x * 4 + 4].copy_from_slice(px);
            x += 1;
            shift = 0;
        }
    }
    Some(src)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(rest: &str) -> Vec<u8> {
        format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n{rest}\n").into_bytes()
    }

    #[test]
    fn flat_scanlines_read_as_they_are() {
        let mut b = header("-Y 2 +X 2");
        b.extend([1, 2, 3, 128, 4, 5, 6, 129, 7, 8, 9, 130, 10, 11, 12, 131]);
        let p = decode(&b, usize::MAX).unwrap();
        assert_eq!((p.width, p.height), (2, 2));
        assert_eq!(
            p.data,
            [1, 2, 3, 128, 4, 5, 6, 129, 7, 8, 9, 130, 10, 11, 12, 131]
        );
    }

    #[test]
    fn run_length_scanlines_read_channel_by_channel() {
        // Ten pixels: red a run of ten 200s; green, blue and exponent a
        // literal of two then a run of eight.
        let mut b = header("-Y 1 +X 10");
        b.extend([2, 2, 0, 10]);
        b.extend([128 + 10, 200]);
        for c in [10u8, 20, 129] {
            b.extend([2, c, c, 128 + 8, c]);
        }
        let p = decode(&b, usize::MAX).unwrap();
        for x in 0..10 {
            assert_eq!(p.data[x * 4..x * 4 + 4], [200, 10, 20, 129], "pixel {x}");
        }
    }

    #[test]
    fn the_older_run_length_repeats_the_pixel_before() {
        let mut b = header("-Y 1 +X 4");
        b.extend([9, 8, 7, 130, 1, 1, 1, 3]);
        let p = decode(&b, usize::MAX).unwrap();
        assert_eq!(p.data, [9, 8, 7, 130].repeat(4));
    }

    #[test]
    fn a_bottom_up_picture_is_turned_top_row_first() {
        let mut b = header("+Y 2 +X 1");
        b.extend([1, 1, 2, 128, 3, 3, 3, 129]);
        assert_eq!(
            decode(&b, usize::MAX).unwrap().data,
            [3, 3, 3, 129, 1, 1, 2, 128]
        );
    }

    #[test]
    fn broken_files_are_refused_with_a_reason() {
        let cut = {
            let mut b = header("-Y 2 +X 2");
            b.extend([1, 2, 3, 4]);
            b
        };
        assert!(decode(&cut, usize::MAX).unwrap_err().contains("scanline"));
        let xyze = b"#?RADIANCE\nFORMAT=32-bit_rle_xyze\n\n-Y 1 +X 1\n\0\0\0\0";
        assert!(decode(xyze, usize::MAX).unwrap_err().contains("xyze"));
        let big = header("-Y 4096 +X 4096");
        assert!(decode(&big, 1 << 20).unwrap_err().contains("too large"));
        assert!(decode(&header("+X 2 -Y 2"), usize::MAX).is_err());
    }
}
