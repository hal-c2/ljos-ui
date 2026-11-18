//! The window's pixels, and the pictures the host takes of an item.

use cucumber::{then, when};

use crate::backend::{Backend, BackendError, HostGrab};
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

/// The window's top left, as large as the picture, against a picture of
/// `features/` taken from another renderer (a GIF's frame, say), where no
/// single pixel can be pinned: a GIF's palette dithers flat colours and
/// moves an edge's blend, and two painters put an anti-aliased edge half a
/// pixel apart. Each picture is first blurred by its 3 by 3 neighbourhood,
/// which takes both out, and the two are then compared pixel by pixel by
/// their CIE76 colour difference (ΔE, the distance in CIELAB; 2.3 is just
/// noticeable, and black to white is 100). The step allows `share` per cent of the pixels to be further
/// than `distance` apart, for the edges a blur cannot reconcile. The
/// scenario's window is the driver's size whatever its root's, hence the
/// top left: the root is at the window's origin.
#[then(
    regex = r#"^the top left of the window looks like the picture "([^"]+\.png)": at most (\d+(?:\.\d+)?) % of its pixels differ by more than ΔE (\d+(?:\.\d+)?)$"#
)]
fn window_looks_like(w: &mut LjosWorld, path: String, share: f64, distance: f64) -> StepResult {
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", &path));
    let reference = ljos_render::RgbaImage::from_png(&super::files::feature_file(dir, name)?)
        .map_err(|e| format!("the picture {path}: {e}"))?;
    w.each(|b| {
        let window = ljos_render::RgbaImage::from_png(&b.grab()?)
            .map_err(|e| BackendError::Failed(format!("the picture of the window: {e}")))?;
        let shown =
            top_left(&window, reference.width, reference.height).map_err(BackendError::Failed)?;
        perceptual_difference(&shown, &reference, share, distance).map_err(BackendError::Failed)
    })
}

/// The `width` by `height` pixels at the top left of `picture`.
fn top_left(
    picture: &ljos_render::RgbaImage,
    width: u32,
    height: u32,
) -> Result<ljos_render::RgbaImage, String> {
    if width > picture.width || height > picture.height {
        return Err(format!(
            "the window is {} by {}, smaller than the picture's {width} by {height}",
            picture.width, picture.height
        ));
    }
    let row = |y: u32| {
        let start = (y * picture.width * 4) as usize;
        &picture.data[start..start + (width * 4) as usize]
    };
    Ok(ljos_render::RgbaImage {
        width,
        height,
        data: (0..height).flat_map(row).copied().collect(),
    })
}

/// [`window_looks_like`]'s measure, apart from where the pictures come from.
fn perceptual_difference(
    shown: &ljos_render::RgbaImage,
    reference: &ljos_render::RgbaImage,
    share: f64,
    distance: f64,
) -> Result<(), String> {
    let (w, h) = (shown.width, shown.height);
    if (w, h) != (reference.width, reference.height) {
        return Err(format!(
            "the window is {w} by {h}, the picture {} by {}",
            reference.width, reference.height
        ));
    }
    let (a, b) = (lab_blurred(shown), lab_blurred(reference));
    let mut off = 0usize;
    let mut worst = (0.0f64, 0, 0);
    for (i, (p, q)) in a.iter().zip(&b).enumerate() {
        let d = p
            .iter()
            .zip(q)
            .map(|(p, q)| (p - q).powi(2))
            .sum::<f64>()
            .sqrt();
        if d > distance {
            off += 1;
        }
        if d > worst.0 {
            #[allow(clippy::cast_possible_truncation)]
            let i = i as u32;
            worst = (d, i % w, i / w);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let seen = off as f64 * 100.0 / f64::from(w * h);
    if seen <= share {
        Ok(())
    } else {
        Err(format!(
            "{seen:.2} % of the pixels ({off}) differ by more than ΔE {distance}; the most, \
             ΔE {:.1}, at {}, {}",
            worst.0, worst.1, worst.2
        ))
    }
}

/// Each pixel, laid over black, averaged with its neighbours (those inside
/// the picture), in CIELAB under D65.
fn lab_blurred(picture: &ljos_render::RgbaImage) -> Vec<[f64; 3]> {
    let (w, h) = (picture.width, picture.height);
    let rgb: Vec<[f64; 3]> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| over_black(picture.pixel(x, y)).map(|c| f64::from(c) / 255.0))
        .collect();
    let mut out = Vec::with_capacity(rgb.len());
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0.0; 3];
            let mut n = 0.0;
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let c = rgb[(ny * w + nx) as usize];
                    sum.iter_mut().zip(c).for_each(|(s, c)| *s += c);
                    n += 1.0;
                }
            }
            out.push(lab(sum.map(|s| s / n)));
        }
    }
    out
}

/// CIELAB of an sRGB colour, channels 0 to 1.
fn lab(srgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = srgb.map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    // sRGB to XYZ, each over the D65 white's.
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.950_47;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.088_83;
    let f = |t: f64| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
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
    let wanted = rgb(colour)?;
    w.each(|b| {
        let seen = window_pixel_seen(b, x, y)?;
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

/// For a picture whose exact colours depend on lighting the scenario does
/// not pin: something was drawn there, over what would show otherwise.
#[then(regex = r#"^the pixel at (\d+), (\d+) of the window is not "(#\w{6})"$"#)]
fn window_pixel_not(w: &mut LjosWorld, x: u32, y: u32, colour: String) -> StepResult {
    let unwanted = rgb(&colour)?;
    w.each(|b| {
        if window_pixel_seen(b, x, y)? == unwanted {
            Err(BackendError::Failed(format!(
                "the pixel at {x}, {y} is {colour}"
            )))
        } else {
            Ok(())
        }
    })
}

#[when(regex = r#"^the host asks for a picture of "([^"]+)"$"#)]
fn host_asks_picture(w: &mut LjosWorld, object: String) -> StepResult {
    w.each(|b| b.probes()?.host_grab(&object))
}

/// The host's picture, once a frame has drawn it; a failure says what
/// came of it instead.
fn host_picture(b: &mut dyn Backend) -> Result<(u32, u32, Vec<u8>), BackendError> {
    match b.probes()?.host_grabbed()? {
        HostGrab::Picture {
            width,
            height,
            rgba,
        } => Ok((width, height, rgba)),
        other => Err(BackendError::Failed(format!(
            "the host has no picture: {other:?}"
        ))),
    }
}

#[then(regex = r"^the host's picture is (\d+) by (\d+) pixels$")]
fn host_picture_size(w: &mut LjosWorld, width: u32, height: u32) -> StepResult {
    w.each(|b| match host_picture(b)? {
        (w, h, _) if (w, h) == (width, height) => Ok(()),
        (w, h, _) => Err(BackendError::Failed(format!(
            "the host's picture is {w} by {h} pixels, not {width} by {height}"
        ))),
    })
}

/// The pixels are premultiplied, so each is already as it reads over
/// black, as a window pixel is read.
#[then(regex = r#"^the pixel at (\d+), (\d+) of the host's picture is "(#\w{6})"$"#)]
fn host_picture_pixel(w: &mut LjosWorld, x: u32, y: u32, colour: String) -> StepResult {
    let wanted = rgb(&colour)?;
    w.each(|b| {
        let (width, height, rgba) = host_picture(b)?;
        if x >= width || y >= height {
            return Err(BackendError::Failed(format!(
                "{x}, {y} is outside the host's picture, which is {width} by {height}"
            )));
        }
        let at = ((y * width + x) * 4) as usize;
        let seen = [rgba[at], rgba[at + 1], rgba[at + 2]];
        if seen == wanted {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the pixel at {x}, {y} of the host's picture is #{:02x}{:02x}{:02x}, not {colour}",
                seen[0], seen[1], seen[2]
            )))
        }
    })
}

#[then(regex = r#"^the host is refused a picture because "(.*)"$"#)]
fn host_picture_refused(w: &mut LjosWorld, why: String) -> StepResult {
    w.each(|b| match b.probes()?.host_grabbed()? {
        HostGrab::Refused(said) if said == why => Ok(()),
        other => Err(BackendError::Failed(format!(
            "the host's picture came to {other:?}, not a refusal because {why:?}"
        ))),
    })
}

#[then("the host is called back with no picture")]
fn host_picture_empty(w: &mut LjosWorld) -> StepResult {
    w.each(|b| match b.probes()?.host_grabbed()? {
        HostGrab::Empty => Ok(()),
        other => Err(BackendError::Failed(format!(
            "the host's picture came to {other:?}, not an empty callback"
        ))),
    })
}

fn rgb(colour: &str) -> Result<[u8; 3], String> {
    let c = u32::from_str_radix(&colour[1..], 16)
        .map_err(|e| format!("{colour} is not a colour as #rrggbb: {e}"))?;
    Ok([(c >> 16) as u8, (c >> 8) as u8, c as u8])
}

fn window_pixel_seen(b: &mut dyn Backend, x: u32, y: u32) -> Result<[u8; 3], BackendError> {
    let png = b.grab()?;
    let picture = ljos_render::RgbaImage::from_png(&png)
        .map_err(|e| BackendError::Failed(format!("the picture of the window: {e}")))?;
    if x >= picture.width || y >= picture.height {
        return Err(BackendError::Failed(format!(
            "{x}, {y} is outside the window's picture, which is {} by {}",
            picture.width, picture.height
        )));
    }
    Ok(over_black(picture.pixel(x, y)))
}

#[cfg(test)]
mod tests {
    use ljos_render::RgbaImage;

    use super::perceptual_difference;

    /// The ΔE 15 the scenarios allow is only what it was calibrated as if
    /// CIELAB is: known colours land where the standard puts them.
    #[test]
    fn colours_land_where_cielab_puts_them() {
        for (srgb, want) in [
            ([1.0, 1.0, 1.0], [100.0, 0.0, 0.0]),
            ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            ([1.0, 0.0, 0.0], [53.24, 80.09, 67.20]),
            ([0.0, 1.0, 0.0], [87.73, -86.18, 83.18]),
            ([0.0, 0.0, 1.0], [32.30, 79.19, -107.86]),
            ([0.5, 0.5, 0.5], [53.39, 0.0, 0.0]),
        ] {
            let got = super::lab(srgb);
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() < 0.1, "{srgb:?} is {got:?}, not {want:?}");
            }
        }
    }

    fn avatar(name: &str) -> RgbaImage {
        RgbaImage::from_png(&super::super::files::feature_file("lottie/avatars", name).unwrap())
            .unwrap()
    }

    /// The picture dithered as a GIF's palette dithers it: every other
    /// pixel a little lighter, the rest a little darker.
    fn dithered(p: &RgbaImage) -> RgbaImage {
        let mut out = p.clone();
        for (i, px) in out.data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (i as u32 % p.width, i as u32 / p.width);
            for c in &mut px[..3] {
                *c = if (x + y) % 2 == 0 {
                    c.saturating_add(12)
                } else {
                    c.saturating_sub(12)
                };
            }
        }
        out
    }

    #[test]
    fn a_picture_looks_like_itself_and_like_itself_dithered() {
        let lean = avatar("dance-07.png");
        perceptual_difference(&lean, &lean, 0.0, 0.0).unwrap();
        perceptual_difference(&dithered(&lean), &lean, 0.1, 15.0).unwrap();
    }

    #[test]
    fn a_lean_to_one_side_does_not_look_like_the_other() {
        let err =
            perceptual_difference(&avatar("dance-07.png"), &avatar("dance-19.png"), 0.1, 15.0)
                .unwrap_err();
        assert!(err.contains("differ by more than ΔE 15"), "{err}");
    }

    #[test]
    fn the_genie_from_nothing_does_not_look_like_its_face() {
        perceptual_difference(&avatar("genie-01.png"), &avatar("genie-41.png"), 0.1, 15.0)
            .unwrap_err();
    }

    #[test]
    fn the_top_left_is_cut_from_the_window() {
        let mut window = RgbaImage::filled(3, 2, [0, 0, 0, 255]);
        window.data[4..8].copy_from_slice(&[9, 9, 9, 255]);
        let corner = super::top_left(&window, 2, 1).unwrap();
        assert_eq!(corner.data, [0, 0, 0, 255, 9, 9, 9, 255]);
        assert!(super::top_left(&window, 4, 1).is_err());
    }

    #[test]
    fn pictures_of_two_sizes_never_look_alike() {
        let err = perceptual_difference(
            &RgbaImage::filled(2, 2, [255; 4]),
            &RgbaImage::filled(3, 2, [255; 4]),
            100.0,
            100.0,
        )
        .unwrap_err();
        assert!(err.contains("2 by 2"), "{err}");
    }
}
