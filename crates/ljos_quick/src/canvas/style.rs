//! What the 2D context reads from strings: CSS colours, the CSS `font`
//! shorthand and the names of the composition modes.

use std::sync::Arc;

use ljos_engine::Color;
use ljos_render::peniko::{BlendMode, Compose, Mix};
use ljos_text::TextStyle;

/// A colour as a canvas takes it: a QML colour string (`#rrggbb`,
/// `#aarrggbb`, an SVG name) or a CSS function (`rgb()`, `rgba()`,
/// `hsl()`, `hsla()`). `None` when it is neither, which leaves the style
/// as it was.
pub(crate) fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    let Some(open) = lower.find('(') else {
        return Color::parse(s);
    };
    let name = lower[..open].trim();
    let inner = lower[open + 1..].strip_suffix(')')?;
    let parts: Vec<&str> = inner
        .split([',', '/', ' '])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let alpha = |p: Option<&&str>| -> Option<f64> {
        match p {
            None => Some(1.0),
            Some(a) => Some(number(a, 1.0)?.clamp(0.0, 1.0)),
        }
    };
    match (name, parts.len()) {
        ("rgb" | "rgba", 3 | 4) => {
            let c = |p: &str| -> Option<f64> { Some(number(p, 255.0)?.clamp(0.0, 255.0) / 255.0) };
            Some(Color::from_rgba_f(
                c(parts[0])?,
                c(parts[1])?,
                c(parts[2])?,
                alpha(parts.get(3))?,
            ))
        }
        ("hsl" | "hsla", 3 | 4) => {
            let h = parts[0].trim_end_matches("deg").parse::<f64>().ok()?;
            let sat = number(parts[1], 100.0)?.clamp(0.0, 100.0) / 100.0;
            let l = number(parts[2], 100.0)?.clamp(0.0, 100.0) / 100.0;
            let [r, g, b] = hsl_to_rgb(h, sat, l);
            Some(Color::from_rgba_f(r, g, b, alpha(parts.get(3))?))
        }
        _ => None,
    }
}

/// A number, or a percentage of `whole`.
fn number(s: &str, whole: f64) -> Option<f64> {
    let v = match s.strip_suffix('%') {
        Some(p) => p.trim().parse::<f64>().ok()? * whole / 100.0,
        None => s.parse::<f64>().ok()?,
    };
    v.is_finite().then_some(v)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let h = h.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |t: f64| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

/// How the context reads a colour back: `#rrggbb` when it is opaque,
/// `rgba(r, g, b, a)` otherwise (measured on Qt 6.11).
pub(crate) fn color_string(c: Color) -> String {
    let [r, g, b, a] = c.to_rgba8();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        let alpha = (f64::from(a) / 255.0 * 1e6).round() / 1e6;
        format!("rgba({r}, {g}, {b}, {alpha})")
    }
}

/// The composition a `globalCompositeOperation` names; `None` for a name
/// it does not know. The Qt-specific spellings (`qt-multiply`) are the
/// blend modes of CSS.
pub(crate) fn blend_mode(name: &str) -> Option<BlendMode> {
    let over = |mix| BlendMode::new(mix, Compose::SrcOver);
    let plain = |compose| BlendMode::new(Mix::Normal, compose);
    Some(match name.strip_prefix("qt-").unwrap_or(name) {
        "source-over" => plain(Compose::SrcOver),
        "source-in" => plain(Compose::SrcIn),
        "source-out" => plain(Compose::SrcOut),
        "source-atop" => plain(Compose::SrcAtop),
        "destination-over" => plain(Compose::DestOver),
        "destination-in" => plain(Compose::DestIn),
        "destination-out" => plain(Compose::DestOut),
        "destination-atop" => plain(Compose::DestAtop),
        "copy" => plain(Compose::Copy),
        "xor" => plain(Compose::Xor),
        "lighter" | "plus" => plain(Compose::Plus),
        "multiply" => over(Mix::Multiply),
        "screen" => over(Mix::Screen),
        "overlay" => over(Mix::Overlay),
        "darken" => over(Mix::Darken),
        "lighten" => over(Mix::Lighten),
        "color-dodge" => over(Mix::ColorDodge),
        "color-burn" => over(Mix::ColorBurn),
        "hard-light" => over(Mix::HardLight),
        "soft-light" => over(Mix::SoftLight),
        "difference" => over(Mix::Difference),
        "exclusion" => over(Mix::Exclusion),
        _ => return None,
    })
}

/// The CSS `font` shorthand: `[style] [variant] [weight] size[/line-height]
/// family`. `None` when there is no size, which leaves the font as it was.
pub(crate) fn parse_font(s: &str) -> Option<TextStyle> {
    let mut style = TextStyle::default();
    let mut words = s.split_whitespace().peekable();
    let mut size = None;
    while let Some(w) = words.next() {
        let lower = w.to_ascii_lowercase();
        match lower.as_str() {
            "normal" | "small-caps" => {}
            "italic" | "oblique" => style.italic = true,
            "bold" | "bolder" => style.weight = 700.0,
            "lighter" => style.weight = 300.0,
            _ => {
                if let Ok(n) = lower.parse::<u32>()
                    && (1..=1000).contains(&n)
                    && words.peek().is_some()
                {
                    style.weight = n as f32;
                    continue;
                }
                let sz = lower.split('/').next().unwrap_or("");
                let px = if let Some(v) = sz.strip_suffix("px") {
                    v.parse::<f64>().ok()
                } else if let Some(v) = sz.strip_suffix("pt") {
                    v.parse::<f64>().ok().map(|p| (p * 4.0 / 3.0).round())
                } else {
                    None
                };
                size = Some(px.filter(|p| p.is_finite() && *p > 0.0)?);
                break;
            }
        }
    }
    style.pixel_size = size? as f32;
    let family: Vec<&str> = words.collect();
    let family = family.join(" ");
    let first = family
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(['"', '\'']);
    style.family = Arc::from(first);
    Some(style)
}

/// The 8 by 8 cell of a hatching `Qt.BrushStyle` (`Dense1Pattern` to
/// `DiagCrossPattern`, 2 to 14), `#` where the colour is drawn. Read off
/// qml6's canvas pixels (features/items/canvas-effects.feature).
pub(crate) fn hatch_cell(style: i32) -> Option<[&'static str; 8]> {
    const D1: [&str; 4] = ["########", "##.###.#", "########", "########"];
    const D2: [&str; 4] = ["###.###.", "########", "#.###.##", "########"];
    const D3: [&str; 4] = ["#.#.#.#.", "##.###.#", "#.#.#.#.", ".###.###"];
    const D4: [&str; 4] = [".#.#.#.#", "#.#.#.#.", ".#.#.#.#", "#.#.#.#."];
    const D5: [&str; 4] = [".#.#.#.#", "..#...#.", ".#.#.#.#", "#...#..."];
    const D6: [&str; 4] = ["...#...#", "........", ".#...#..", "........"];
    const D7: [&str; 4] = ["........", "..#...#.", "........", "........"];
    let twice = |q: [&'static str; 4]| [q[0], q[1], q[2], q[3], q[0], q[1], q[2], q[3]];
    let (h, v, n) = ("########", "....#...", "........");
    Some(match style {
        2 => twice(D1),
        3 => twice(D2),
        4 => twice(D3),
        5 => twice(D4),
        6 => twice(D5),
        7 => twice(D6),
        8 => twice(D7),
        9 => [n, n, n, h, n, n, n, n],
        10 => [v; 8],
        11 => [v, v, v, h, v, v, v, v],
        12 => [
            ".......#", "......#.", ".....#..", "....#...", "...#....", "..#.....", ".#......",
            "#.......",
        ],
        13 => [
            "#.......", ".#......", "..#.....", "...#....", "....#...", ".....#..", "......#.",
            ".......#",
        ],
        14 => [
            "#......#", ".#....#.", "..#..#..", "...##...", "...##...", "..#..#..", ".#....#.",
            "#......#",
        ],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_read_back_as_qt_writes_them() {
        assert_eq!(color_string(parse_color("red").unwrap()), "#ff0000");
        assert_eq!(
            color_string(parse_color("rgba(0,255,0,0.5)").unwrap()),
            "rgba(0, 255, 0, 0.501961)"
        );
        assert_eq!(
            color_string(parse_color("rgb(10%, 0, 255)").unwrap()),
            "#1a00ff"
        );
        assert_eq!(
            color_string(parse_color("hsl(120, 100%, 50%)").unwrap()),
            "#00ff00"
        );
        assert_eq!(parse_color("bogus"), None);
        assert_eq!(parse_color("rgb(1,2)"), None);
    }

    #[test]
    fn the_font_shorthand() {
        let f = parse_font("bold 20px sans-serif").unwrap();
        assert_eq!(
            (f.pixel_size, f.weight, &*f.family),
            (20.0, 700.0, "sans-serif")
        );
        let f = parse_font("italic 600 12pt \"DejaVu Sans\", serif").unwrap();
        assert_eq!((f.pixel_size, f.weight, f.italic), (16.0, 600.0, true));
        assert_eq!(&*f.family, "DejaVu Sans");
        assert!(parse_font("sans-serif").is_none());
    }

    #[test]
    fn composition_names() {
        assert!(blend_mode("copy").is_some());
        assert!(blend_mode("qt-multiply").is_some());
        assert!(blend_mode("bogus").is_none());
    }
}
