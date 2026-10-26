//! The window's pixels.

use cucumber::then;

use crate::backend::BackendError;
use crate::world::LjosWorld;

use super::StepResult;

#[then(regex = r#"^the pixel at (\d+), (\d+) of the window is "(#\w{6})"$"#)]
fn window_pixel(w: &mut LjosWorld, x: u32, y: u32, colour: String) -> StepResult {
    window_pixel_near(w, x, y, 0, &colour)
}

#[then(regex = r#"^the pixel at (\d+), (\d+) of the window is within (\d+) of "(#\w{6})"$"#)]
fn window_pixel_within(
    w: &mut LjosWorld,
    x: u32,
    y: u32,
    tolerance: u8,
    colour: String,
) -> StepResult {
    window_pixel_near(w, x, y, tolerance, &colour)
}

#[then(regex = r"^the window's picture is within (\d+) of the reference renderer's$")]
fn window_is_reference(w: &mut LjosWorld, tolerance: u8) -> StepResult {
    w.each(|b| {
        let decode = |png: Vec<u8>, what: &str| {
            ljos_render::RgbaImage::from_png(&png)
                .map_err(|e| BackendError::Failed(format!("the picture {what}: {e}")))
        };
        let shown = decode(b.grab()?, "of the window")?;
        let reference = decode(
            b.probes()?.reference_picture()?,
            "the reference renderer drew",
        )?;
        if (shown.width, shown.height) != (reference.width, reference.height) {
            return Err(BackendError::Failed(format!(
                "the window is {} by {}, the reference renderer's picture {} by {}",
                shown.width, shown.height, reference.width, reference.height
            )));
        }
        let mut off = 0usize;
        let mut first = None;
        for y in 0..shown.height {
            for x in 0..shown.width {
                let (a, b) = (
                    over_black(shown.pixel(x, y)),
                    over_black(reference.pixel(x, y)),
                );
                if a.iter().zip(b).any(|(a, b)| a.abs_diff(b) > tolerance) {
                    off += 1;
                    first.get_or_insert((x, y, a, b));
                }
            }
        }
        match first {
            None => Ok(()),
            Some((x, y, a, b)) => Err(BackendError::Failed(format!(
                "{off} pixels differ by more than {tolerance}; the first, at {x}, {y}, is \
                 #{:02x}{:02x}{:02x} in the window and #{:02x}{:02x}{:02x} in the reference",
                a[0], a[1], a[2], b[0], b[1], b[2]
            ))),
        }
    })
}

/// A pixel laid over black, as [`window_pixel_near`] reads one.
fn over_black([r, g, b, a]: [u8; 4]) -> [u8; 3] {
    let over = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
    [over(r), over(g), over(b)]
}

/// One pixel of the window's picture, each channel give or take `tolerance`.
///
/// The picture is laid over black first: where nothing is painted Qt's
/// picture is black and Ljós's is transparent, and a half transparent pixel
/// must read the same on both.
fn window_pixel_near(w: &mut LjosWorld, x: u32, y: u32, tolerance: u8, colour: &str) -> StepResult {
    let wanted = u32::from_str_radix(&colour[1..], 16)
        .map_err(|e| format!("{colour} is not a colour as #rrggbb: {e}"))?;
    let wanted = [(wanted >> 16) as u8, (wanted >> 8) as u8, wanted as u8];
    w.each(|b| {
        let png = b.grab()?;
        let picture = ljos_render::RgbaImage::from_png(&png)
            .map_err(|e| BackendError::Failed(format!("the picture of the window: {e}")))?;
        if x >= picture.width || y >= picture.height {
            return Err(BackendError::Failed(format!(
                "{x}, {y} is outside the window's picture, which is {} by {}",
                picture.width, picture.height
            )));
        }
        let seen = over_black(picture.pixel(x, y));
        if seen
            .iter()
            .zip(wanted)
            .all(|(s, w)| s.abs_diff(w) <= tolerance)
        {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the pixel at {x}, {y} is #{:02x}{:02x}{:02x}, not {colour}",
                seen[0], seen[1], seen[2]
            )))
        }
    })
}
