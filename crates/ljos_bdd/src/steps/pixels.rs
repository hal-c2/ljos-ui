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
        let [r, g, b, a] = picture.pixel(x, y);
        let over_black = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
        let seen = [over_black(r), over_black(g), over_black(b)];
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
