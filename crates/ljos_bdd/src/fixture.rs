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

/// A PNG split into four quadrants of one colour each, given top left,
/// top right, bottom left and bottom right, so that a picture of it says
/// which part of the image landed where. An odd side gives the extra
/// row or column to the right and bottom quadrants.
///
/// # Errors
/// There are not four colours, one is not a colour, or the size has no
/// pixels.
pub fn quadrants_png(width: u32, height: u32, colours: &[String]) -> Result<Vec<u8>, String> {
    let [tl, tr, bl, br] = colours else {
        return Err(format!(
            "quadrants take four colours, not {}",
            colours.len()
        ));
    };
    if width == 0 || height == 0 {
        return Err("an image has at least one pixel".to_owned());
    }
    let quads = [tl, tr, bl, br]
        .into_iter()
        .map(|c| parse_colour(c))
        .collect::<Result<Vec<_>, _>>()?;
    let mut data = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let i = usize::from(y >= height / 2) * 2 + usize::from(x >= width / 2);
            let [a, r, g, b] = quads[i];
            data.extend([r, g, b, a]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&data).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// A Radiance HDR picture (`.hdr`) whose upper half is one linear
/// radiance and lower half another, written as flat RGBE scanlines,
/// which every reader of the format takes. A light probe is laid around
/// the scene from such a picture, its top row straight up.
///
/// # Errors
/// The size has no pixels or a radiance is negative or not a number.
pub fn radiance(
    width: u32,
    height: u32,
    upper: [f32; 3],
    lower: [f32; 3],
) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err("an image has at least one pixel".to_owned());
    }
    if upper
        .iter()
        .chain(&lower)
        .any(|c| !(c.is_finite() && *c >= 0.0))
    {
        return Err("a radiance is a number of at least 0".to_owned());
    }
    let mut out =
        format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y {height} +X {width}\n").into_bytes();
    let (up, down) = (rgbe(upper), rgbe(lower));
    for y in 0..height {
        let px = if y < height / 2 { up } else { down };
        for _ in 0..width {
            out.extend(px);
        }
    }
    Ok(out)
}

/// One colour in Radiance's shared-exponent form.
fn rgbe([r, g, b]: [f32; 3]) -> [u8; 4] {
    let max = r.max(g).max(b);
    if max < 1e-32 {
        return [0; 4];
    }
    // max = m · 2^e with m in [0.5, 1); each channel keeps 8 bits of m.
    let e = max.log2().floor() as i32 + 1;
    let scale = 256.0 / 2f32.powi(e);
    let byte = |c: f32| (c * scale).clamp(0.0, 255.0) as u8;
    [byte(r), byte(g), byte(b), (e + 128) as u8]
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

/// A PDF document with a page of each size, in points, each painted with
/// a rectangle of `colour` (`#rrggbb`) over its middle half and nothing
/// around it, so that what is around a page shows through.
///
/// # Errors
/// The colour is not one, there is no page, or a side is not positive.
pub fn pdf(pages: &[(f64, f64)], colour: &str) -> Result<Vec<u8>, String> {
    use std::fmt::Write as _;
    let [_, r, g, b] = parse_colour(colour)?;
    if pages.is_empty() {
        return Err("a PDF has at least one page".to_owned());
    }
    if let Some((w, h)) = pages.iter().find(|(w, h)| !(*w > 0.0 && *h > 0.0)) {
        return Err(format!(
            "a page is larger than 0 by 0 points, not {w} by {h}"
        ));
    }
    let unit = |c: u8| f64::from(c) / 255.0;
    // Objects 1 and 2 are the catalogue and the page tree; each page is a
    // page and its contents.
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        String::new(),
    ];
    let mut kids = String::new();
    for (i, (w, h)) in pages.iter().enumerate() {
        let page = 3 + 2 * i;
        let _ = write!(kids, "{page} 0 R ");
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Contents {} 0 R >>",
            page + 1
        ));
        let paint = format!(
            "{} {} {} rg {} {} {} {} re f",
            unit(r),
            unit(g),
            unit(b),
            w / 4.0,
            h / 4.0,
            w / 2.0,
            h / 2.0
        );
        objects.push(format!(
            "<< /Length {} >>\nstream\n{paint}\nendstream",
            paint.len()
        ));
    }
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.trim_end(),
        pages.len()
    );
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", i + 1);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for at in offsets {
        let _ = writeln!(out, "{at:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    Ok(out.into_bytes())
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
    fn a_pdf_has_a_page_of_each_size_and_a_cross_reference_table() {
        let bytes = pdf(&[(300.0, 200.0), (150.5, 400.0)], "#ff0000").unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 2"));
        assert!(text.contains("/MediaBox [0 0 150.5 400]"));
        let xref: usize = text.lines().rev().nth(1).unwrap().parse().unwrap();
        assert!(text[xref..].starts_with("xref"));
        assert!(pdf(&[], "#ff0000").is_err());
        assert!(pdf(&[(0.0, 1.0)], "#ff0000").is_err());
    }

    #[test]
    fn other_formats_and_bad_colours_are_refused() {
        assert!(image_file("a.gif", 1, 1, "#000000").is_err());
        assert!(image_file("a.png", 1, 1, "red").is_err());
        assert!(image_file("a.png", 0, 1, "#000000").is_err());
    }
}
