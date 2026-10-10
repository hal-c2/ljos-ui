//! Files a scenario needs that are not text: images, made here from a size
//! and a colour so that no binary file has to live beside the scenarios.

use std::io::Cursor;

/// An image file of one colour, encoded as its extension says (`.png`,
/// `.jpg`/`.jpeg`).
///
/// # Errors
/// The extension names no format made here, the colour is not `#rrggbb`, or
/// the size does not fit the format.
pub fn image_file(name: &str, width: u32, height: u32, colour: &str) -> Result<Vec<u8>, String> {
    let rgb = parse_colour(colour)?;
    if width == 0 || height == 0 {
        return Err(format!("{name}: an image has at least one pixel"));
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => png(width, height, rgb),
        Some("jpg" | "jpeg") => jpeg(width, height, rgb),
        _ => Err(format!("{name}: image files made here are .png or .jpg")),
    }
}

/// Bytes no image decoder accepts.
#[must_use]
pub fn not_an_image() -> Vec<u8> {
    b"This file is text, whatever its name says.\n".to_vec()
}

fn parse_colour(text: &str) -> Result<[u8; 3], String> {
    let hex = text
        .strip_prefix('#')
        .filter(|h| h.len() == 6 && h.is_ascii())
        .ok_or_else(|| format!("{text:?} is not a #rrggbb colour"))?;
    let channel = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| format!("{text:?} is not a colour"))
    };
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

fn pixels(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let count = width as usize * height as usize;
    rgb.iter().copied().cycle().take(count * 3).collect()
}

fn png(width: u32, height: u32, rgb: [u8; 3]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&pixels(width, height, rgb))
        .map_err(|e| e.to_string())?;
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
