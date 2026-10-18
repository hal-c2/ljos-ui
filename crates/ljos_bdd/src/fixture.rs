//! Files a scenario needs that are not text: images, made here from a size
//! and a colour so that no binary file has to live beside the scenarios.

use std::io::Cursor;

/// An image file of one colour, encoded as its extension says (`.png`,
/// `.jpg`/`.jpeg`). The colour is `#rrggbb`, or `#aarrggbb` for a PNG
/// whose alpha matters (a mask, say). A JPEG has no alpha.
///
/// # Errors
/// The extension names no format made here, the colour is not a colour
/// as above, or the size does not fit the format.
pub fn image_file(name: &str, width: u32, height: u32, colour: &str) -> Result<Vec<u8>, String> {
    let rgba = parse_colour(colour)?;
    if width == 0 || height == 0 {
        return Err(format!("{name}: an image has at least one pixel"));
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => png(width, height, rgba),
        Some("jpg" | "jpeg") => jpeg(width, height, [rgba[1], rgba[2], rgba[3]]),
        _ => Err(format!("{name}: image files made here are .png or .jpg")),
    }
}

/// A PNG of noise, the same each time: its bytes are about as many as its
/// pixels, as a photograph's are, where a picture of one colour is a few
/// hundred bytes whatever its size.
///
/// # Errors
/// The size has no pixels.
pub fn noisy_png(width: u32, height: u32) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err("an image has at least one pixel".to_owned());
    }
    let count = width as usize * height as usize * 3;
    let mut state = 0x2545_f491_u32;
    let data: Vec<u8> = (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        })
        .collect();
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_compression(png::Compression::Fastest);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&data).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// An animated GIF whose frames are each of one colour (`#rrggbb`), shown
/// for `delay_ms` (rounded to the hundredth of a second GIF counts in)
/// and repeated for ever. One colour makes a still GIF.
///
/// # Errors
/// A colour is not one, there are more than 256 or none, or the size does
/// not fit a GIF.
pub fn animated_gif(
    width: u32,
    height: u32,
    colours: &[String],
    delay_ms: u32,
) -> Result<Vec<u8>, String> {
    if colours.is_empty() || colours.len() > 256 {
        return Err("an animated image has 1 to 256 frames".to_owned());
    }
    let side = |n: u32| {
        u16::try_from(n)
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("a GIF is 1 to 65535 pixels a side, not {n}"))
    };
    let (w, h) = (side(width)?, side(height)?);
    let mut palette = Vec::with_capacity(colours.len() * 3);
    for c in colours {
        let [_, r, g, b] = parse_colour(c)?;
        palette.extend([r, g, b]);
    }
    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, w, h, &palette).map_err(|e| e.to_string())?;
        if colours.len() > 1 {
            enc.set_repeat(gif::Repeat::Infinite)
                .map_err(|e| e.to_string())?;
        }
        let delay = u16::try_from(delay_ms.div_ceil(10)).unwrap_or(u16::MAX);
        for i in 0..colours.len() {
            let index = u8::try_from(i).unwrap_or(u8::MAX);
            let frame = gif::Frame {
                width: w,
                height: h,
                delay,
                buffer: std::borrow::Cow::Owned(vec![index; usize::from(w) * usize::from(h)]),
                ..gif::Frame::default()
            };
            enc.write_frame(&frame).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// Bytes no image decoder accepts.
#[must_use]
pub fn not_an_image() -> Vec<u8> {
    b"This file is text, whatever its name says.\n".to_vec()
}

fn parse_colour(text: &str) -> Result<[u8; 4], String> {
    let hex = text
        .strip_prefix('#')
        .filter(|h| (h.len() == 6 || h.len() == 8) && h.is_ascii())
        .ok_or_else(|| format!("{text:?} is not a #rrggbb or #aarrggbb colour"))?;
    let channel = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| format!("{text:?} is not a colour"))
    };
    if hex.len() == 8 {
        Ok([channel(0)?, channel(2)?, channel(4)?, channel(6)?])
    } else {
        Ok([255, channel(0)?, channel(2)?, channel(4)?])
    }
}

fn pixels(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let count = width as usize * height as usize;
    rgb.iter().copied().cycle().take(count * 3).collect()
}

fn png(width: u32, height: u32, rgba: [u8; 4]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
    encoder.set_depth(png::BitDepth::Eight);
    let count = width as usize * height as usize;
    let (kind, data) = if rgba[0] == 255 {
        (
            png::ColorType::Rgb,
            pixels(width, height, [rgba[1], rgba[2], rgba[3]]),
        )
    } else {
        let px = [rgba[1], rgba[2], rgba[3], rgba[0]];
        (
            png::ColorType::Rgba,
            px.iter().copied().cycle().take(count * 4).collect(),
        )
    };
    encoder.set_color(kind);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&data).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

fn jpeg(width: u32, height: u32, rgb: [u8; 3]) -> Result<Vec<u8>, String> {
    let side = |n: u32| u16::try_from(n).map_err(|_| format!("{n} is too large for a JPEG"));
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 90)
        .encode(
            &pixels(width, height, rgb),
            side(width)?,
            side(height)?,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_starts_with_the_png_signature() {
        let bytes = image_file("a.png", 4, 2, "#ff0000").unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn a_jpeg_starts_with_the_jpeg_marker() {
        let bytes = image_file("a.JPG", 8, 8, "#0000ff").unwrap();
        assert_eq!(&bytes[..3], [0xff, 0xd8, 0xff]);
    }

    #[test]
    fn other_formats_and_bad_colours_are_refused() {
        assert!(image_file("a.gif", 1, 1, "#000000").is_err());
        assert!(image_file("a.png", 1, 1, "red").is_err());
        assert!(image_file("a.png", 0, 1, "#000000").is_err());
    }
}
