//! The similarity metric: a port of pixelmatch 7 (YIQ perceptual distance with
//! anti-aliasing exclusion) and of the `compare.js` scoring built on it.
//!
//! The arithmetic is kept in `f64` and in the same order as the JavaScript, so
//! the scores agree to the last bit rather than to a tolerance. The
//! cross-check test in `tests/js_parity.rs` keeps it that way.

use crate::image::Rgba;

/// Matching threshold of the corpus ratchet (0 strict … 1 loose).
pub const DEFAULT_THRESHOLD: f64 = 0.1;
/// Σ|dR|+|dG|+|dB| up to which a pixel counts as background.
pub const BACKGROUND_TOLERANCE: i32 = 24;
const DIFF_ALPHA: f64 = 0.3;
/// The largest possible YIQ distance.
const MAX_YIQ: f64 = 35215.0;

/// What the scoring says about two pictures.
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    /// Share of all pixels that are the same.
    pub pixels: f64,
    /// The same, counting only pixels that are not background in either
    /// picture. An empty window of the right colour scores high on `pixels`
    /// and nothing here.
    pub content: f64,
    /// A PNG-ready picture of where they differ.
    pub diff: Rgba,
}

/// Why two pictures cannot be scored.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SizeMismatch(pub String);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Same,
    /// Differs, but only as anti-aliasing: not counted.
    AntiAliased,
    Different,
}

fn word(img: &[u8], i: usize) -> [u8; 4] {
    [img[4 * i], img[4 * i + 1], img[4 * i + 2], img[4 * i + 3]]
}

/// Score `actual` against `reference`. Order matters: pixelmatch is not
/// symmetric in its anti-aliasing test, and the background is the
/// reference's.
pub fn compare(
    actual: &Rgba,
    reference: &Rgba,
    threshold: f64,
) -> Result<Comparison, SizeMismatch> {
    if actual.width != reference.width || actual.height != reference.height {
        return Err(SizeMismatch(format!(
            "the actual picture is {}x{}, the reference {}x{}",
            actual.width, actual.height, reference.width, reference.height
        )));
    }
    let (width, height) = (actual.width as usize, actual.height as usize);
    let total = width * height;
    let kinds = classify(&actual.data, &reference.data, width, height, threshold);
    let different = kinds.iter().filter(|&&k| k == Kind::Different).count();

    let [red, green, blue] = background(&reference.data);
    let is_background = |img: &[u8], i: usize| {
        (i32::from(img[4 * i]) - i32::from(red)).abs()
            + (i32::from(img[4 * i + 1]) - i32::from(green)).abs()
            + (i32::from(img[4 * i + 2]) - i32::from(blue)).abs()
            <= BACKGROUND_TOLERANCE
    };
    let mut content = 0usize;
    let mut content_different = 0usize;
    for (i, kind) in kinds.iter().enumerate() {
        if is_background(&actual.data, i) && is_background(&reference.data, i) {
            continue;
        }
        content += 1;
        if *kind == Kind::Different {
            content_different += 1;
        }
    }

    Ok(Comparison {
        pixels: 1.0 - different as f64 / total as f64,
        content: if content == 0 {
            1.0
        } else {
            1.0 - content_different as f64 / content as f64
        },
        diff: render_diff(&actual.data, &kinds, actual.width, actual.height),
    })
}

/// The commonest colour (first to reach the highest count wins ties, as in
/// the JavaScript's Map walk).
fn background(data: &[u8]) -> [u8; 3] {
    let mut counts = std::collections::HashMap::<u32, u32>::new();
    let (mut most, mut colour) = (0, 0u32);
    for p in data.as_chunks::<4>().0 {
        let key = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
        let count = counts.entry(key).and_modify(|c| *c += 1).or_insert(1);
        if *count > most {
            most = *count;
            colour = key;
        }
    }
    [(colour >> 16) as u8, (colour >> 8) as u8, colour as u8]
}

fn classify(a: &[u8], b: &[u8], width: usize, height: usize, threshold: f64) -> Vec<Kind> {
    let len = width * height;
    let mut kinds = vec![Kind::Same; len];
    if a == b {
        return kinds;
    }
    let max_delta = MAX_YIQ * threshold * threshold;
    for (i, kind) in kinds.iter_mut().enumerate() {
        if word(a, i) == word(b, i) {
            continue;
        }
        let delta = colour_delta(a, b, 4 * i);
        if delta.abs() > max_delta {
            let (x, y) = (i % width, i / width);
            let aa = antialiased(a, x, y, width, height, a, b)
                || antialiased(b, x, y, width, height, b, a);
            *kind = if aa {
                Kind::AntiAliased
            } else {
                Kind::Different
            };
        }
    }
    kinds
}

/// Checkerboard backdrop used to blend translucent pixels. pixelmatch
/// derives it from the byte offset, which is why it looks odd.
fn backdrop(k: usize) -> (f64, f64, f64) {
    let k = k as f64;
    (
        48.0 + 159.0 * ((k as i64 % 2) as f64),
        48.0 + 159.0 * ((((k / 1.618_033_988_749_895) as i64) % 2) as f64),
        48.0 + 159.0 * ((((k / 2.618_033_988_749_895) as i64) % 2) as f64),
    )
}

fn channel_deltas(img1: &[u8], img2: &[u8], k: usize, m: usize) -> (f64, f64, f64) {
    let [r1, g1, b1, a1] = [img1[k], img1[k + 1], img1[k + 2], img1[k + 3]].map(f64::from);
    let [r2, g2, b2, a2] = [img2[m], img2[m + 1], img2[m + 2], img2[m + 3]].map(f64::from);
    if a1 < 255.0 || a2 < 255.0 {
        let (rb, gb, bb) = backdrop(k);
        let da = a1 - a2;
        (
            (r1 * a1 - r2 * a2 - rb * da) / 255.0,
            (g1 * a1 - g2 * a2 - gb * da) / 255.0,
            (b1 * a1 - b2 * a2 - bb * da) / 255.0,
        )
    } else {
        (r1 - r2, g1 - g2, b1 - b2)
    }
}

/// Squared YIQ distance; negative when the second pixel is darker.
fn colour_delta(img1: &[u8], img2: &[u8], k: usize) -> f64 {
    let (dr, dg, db) = channel_deltas(img1, img2, k, k);
    let y = dr * 0.298_895_31 + dg * 0.586_622_47 + db * 0.114_482_23;
    let i = dr * 0.595_977_99 - dg * 0.274_176_10 - db * 0.321_801_89;
    let q = dr * 0.211_470_17 - dg * 0.522_617_11 + db * 0.311_146_94;
    let delta = 0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q;
    if y > 0.0 { -delta } else { delta }
}

fn brightness_delta(img: &[u8], k: usize, m: usize) -> f64 {
    if img[k..k + 4] == img[m..m + 4] {
        return 0.0;
    }
    let (dr, dg, db) = channel_deltas(img, img, k, m);
    dr * 0.298_895_31 + dg * 0.586_622_47 + db * 0.114_482_23
}

/// Vysniauskas' anti-aliased pixel detector, as pixelmatch implements it.
fn antialiased(
    img: &[u8],
    x1: usize,
    y1: usize,
    width: usize,
    height: usize,
    a: &[u8],
    b: &[u8],
) -> bool {
    let x0 = x1.saturating_sub(1);
    let y0 = y1.saturating_sub(1);
    let x2 = (x1 + 1).min(width - 1);
    let y2 = (y1 + 1).min(height - 1);
    let pos = (y1 * width + x1) * 4;
    let mut zeroes = u32::from(x1 == x0 || x1 == x2 || y1 == y0 || y1 == y2);
    let (mut min, mut max) = (0.0f64, 0.0f64);
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (0, 0, 0, 0);

    for x in x0..=x2 {
        for y in y0..=y2 {
            if x == x1 && y == y1 {
                continue;
            }
            let delta = brightness_delta(img, pos, (y * width + x) * 4);
            if delta == 0.0 {
                zeroes += 1;
                if zeroes > 2 {
                    return false;
                }
            } else if delta < min {
                min = delta;
                (min_x, min_y) = (x, y);
            } else if delta > max {
                max = delta;
                (max_x, max_y) = (x, y);
            }
        }
    }
    if min == 0.0 || max == 0.0 {
        return false;
    }
    (many_siblings(a, min_x, min_y, width, height) && many_siblings(b, min_x, min_y, width, height))
        || (many_siblings(a, max_x, max_y, width, height)
            && many_siblings(b, max_x, max_y, width, height))
}

fn many_siblings(img: &[u8], x1: usize, y1: usize, width: usize, height: usize) -> bool {
    let x0 = x1.saturating_sub(1);
    let y0 = y1.saturating_sub(1);
    let x2 = (x1 + 1).min(width - 1);
    let y2 = (y1 + 1).min(height - 1);
    let val = word(img, y1 * width + x1);
    let mut zeroes = u32::from(x1 == x0 || x1 == x2 || y1 == y0 || y1 == y2);
    for x in x0..=x2 {
        for y in y0..=y2 {
            if x == x1 && y == y1 {
                continue;
            }
            zeroes += u32::from(val == word(img, y * width + x));
            if zeroes > 2 {
                return true;
            }
        }
    }
    false
}

/// pixelmatch's non-mask output at alpha 0.3: the actual picture faded to
/// grey, anti-aliasing yellow, differences red.
fn render_diff(a: &[u8], kinds: &[Kind], width: u32, height: u32) -> Rgba {
    let mut data = vec![0u8; a.len()];
    for (i, kind) in kinds.iter().enumerate() {
        let out = &mut data[4 * i..4 * i + 4];
        let rgb = match kind {
            Kind::Different => [255, 0, 0],
            Kind::AntiAliased => [255, 255, 0],
            Kind::Same => {
                let p = &a[4 * i..4 * i + 4];
                let lum = f64::from(p[0]) * 0.298_895_31
                    + f64::from(p[1]) * 0.586_622_47
                    + f64::from(p[2]) * 0.114_482_23;
                let v = 255.0 + (lum - 255.0) * DIFF_ALPHA * f64::from(p[3]) / 255.0;
                [v as u8; 3]
            }
        };
        out[..3].copy_from_slice(&rgb);
        out[3] = 255;
    }
    Rgba {
        width,
        height,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, p: [u8; 4]) -> Rgba {
        Rgba::new(w, h, p.repeat((w * h) as usize)).unwrap()
    }

    #[test]
    fn identical_pictures_score_one() {
        let a = flat(4, 4, [10, 20, 30, 255]);
        let c = compare(&a, &a, DEFAULT_THRESHOLD).unwrap();
        assert_eq!((c.pixels, c.content), (1.0, 1.0));
    }

    #[test]
    fn empty_window_of_the_right_colour_has_no_content() {
        let mut reference = flat(10, 10, [255, 255, 255, 255]);
        let actual = reference.clone();
        // One dark pixel in the reference is content; the actual lacks it.
        reference.data[..3].copy_from_slice(&[0, 0, 0]);
        let c = compare(&actual, &reference, DEFAULT_THRESHOLD).unwrap();
        assert_eq!(c.pixels, 0.99);
        assert_eq!(c.content, 0.0);
    }

    #[test]
    fn near_background_pixels_are_not_content() {
        let reference = flat(5, 5, [255, 255, 255, 255]);
        let actual = flat(5, 5, [250, 250, 250, 255]); // Σ|d| = 15 ≤ 24
        let c = compare(&actual, &reference, DEFAULT_THRESHOLD).unwrap();
        assert_eq!(c.content, 1.0);
    }

    #[test]
    fn sizes_must_match() {
        let e = compare(&flat(2, 2, [0; 4]), &flat(3, 2, [0; 4]), 0.1).unwrap_err();
        assert!(e.0.contains("2x2") && e.0.contains("3x2"));
    }

    #[test]
    fn diff_marks_real_differences_red() {
        let reference = flat(3, 3, [255, 255, 255, 255]);
        let mut actual = reference.clone();
        actual.data[16..20].copy_from_slice(&[0, 0, 0, 255]);
        let c = compare(&actual, &reference, DEFAULT_THRESHOLD).unwrap();
        assert_eq!(&c.diff.data[16..20], &[255, 0, 0, 255]);
        assert!((c.pixels - 8.0 / 9.0).abs() < 1e-12);
    }

    #[test]
    fn png_round_trip() {
        let a = flat(3, 2, [1, 2, 3, 200]);
        assert_eq!(Rgba::from_png(&a.to_png().unwrap()).unwrap(), a);
    }
}
