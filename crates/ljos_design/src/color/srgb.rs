//! Gamma-encoded sRGB colours: the form QML writes and the screen shows.

use std::str::FromStr;

use super::names::NAMES;

/// An opaque sRGB colour, channels 0..=1 and gamma-encoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Srgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// An sRGB colour with straight (not premultiplied) alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Srgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// A string that is none of `#rgb`, `#rrggbb`, `#aarrggbb` or a colour
/// keyword.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a QML colour")]
pub struct ParseColorError(pub String);

impl Srgb {
    pub const BLACK: Srgb = Srgb::new(0.0, 0.0, 0.0);
    pub const WHITE: Srgb = Srgb::new(1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Srgb { r, g, b }
    }

    /// From 8-bit channels.
    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Srgb::new(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        )
    }

    /// The 8-bit channels, rounded and clamped.
    pub fn to_u8(self) -> [u8; 3] {
        [q(self.r), q(self.g), q(self.b)]
    }

    /// `#rrggbb`, lower case.
    pub fn to_hex(self) -> String {
        let [r, g, b] = self.to_u8();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    pub fn with_alpha(self, a: f32) -> Srgba {
        Srgba {
            r: self.r,
            g: self.g,
            b: self.b,
            a,
        }
    }

    /// Parses an opaque colour; a string with alpha other than 1 is refused
    /// so a caller that cannot place alpha does not drop it silently.
    pub fn parse(s: &str) -> Result<Srgb, ParseColorError> {
        let c = Srgba::parse(s)?;
        if c.a < 1.0 {
            return Err(ParseColorError(s.to_owned()));
        }
        Ok(c.rgb())
    }
}

impl Srgba {
    pub const TRANSPARENT: Srgba = Srgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Srgba { r, g, b, a }
    }

    pub fn rgb(self) -> Srgb {
        Srgb::new(self.r, self.g, self.b)
    }

    /// Parses what a QML `color` property accepts as a string: `#rgb`,
    /// `#rrggbb`, `#aarrggbb` (alpha FIRST, unlike CSS) and the SVG colour
    /// keywords, case-insensitively, plus `transparent`.
    pub fn parse(s: &str) -> Result<Srgba, ParseColorError> {
        let err = || ParseColorError(s.to_owned());
        let t = s.trim();
        if let Some(hex) = t.strip_prefix('#') {
            if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(err());
            }
            let v = u32::from_str_radix(hex, 16).map_err(|_| err())?;
            // `#rgb` doubles each digit: 0xf -> 0xff.
            let n = |shift: u32| ((v >> shift) & 0xf) as u8 * 17;
            return match hex.len() {
                3 => Ok(Srgb::from_u8(n(8), n(4), n(0)).with_alpha(1.0)),
                6 => Ok(from_rgb24(v)),
                8 => {
                    let mut c = from_rgb24(v & 0xff_ffff);
                    c.a = f32::from((v >> 24) as u8) / 255.0;
                    Ok(c)
                }
                _ => Err(err()),
            };
        }
        let lower = t.to_ascii_lowercase();
        if lower == "transparent" {
            return Ok(Srgba::TRANSPARENT);
        }
        NAMES
            .binary_search_by_key(&lower.as_str(), |(n, _)| n)
            .map(|i| from_rgb24(NAMES[i].1))
            .map_err(|_| err())
    }

    /// `#rrggbb` when opaque, otherwise QML's `#aarrggbb`.
    pub fn to_hex(self) -> String {
        let rgb = self.rgb().to_hex();
        if self.a >= 1.0 {
            rgb
        } else {
            format!("#{:02x}{}", q(self.a), &rgb[1..])
        }
    }
}

impl FromStr for Srgba {
    type Err = ParseColorError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Srgba::parse(s)
    }
}

impl FromStr for Srgb {
    type Err = ParseColorError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Srgb::parse(s)
    }
}

fn from_rgb24(v: u32) -> Srgba {
    Srgb::from_u8((v >> 16) as u8, (v >> 8) as u8, v as u8).with_alpha(1.0)
}

fn q(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Number of SVG colour keywords, excluding `transparent`.
pub const KEYWORD_COUNT: usize = NAMES.len();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_forms_alpha_first() {
        let c = Srgba::parse("#80ff0000").unwrap();
        assert_eq!(c.rgb().to_u8(), [255, 0, 0]);
        assert!((c.a - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(Srgba::parse("#f80").unwrap().rgb().to_hex(), "#ff8800");
        assert_eq!(Srgb::parse("#FF8800").unwrap().to_hex(), "#ff8800");
        assert_eq!(c.to_hex(), "#80ff0000");
    }

    #[test]
    fn refuses_what_qml_does_not_accept() {
        for bad in [
            "",
            "#",
            "#12",
            "#1234",
            "#12345",
            "#rrggbb",
            "ff0000",
            "notacolor",
            "#ff000080x",
        ] {
            assert!(Srgba::parse(bad).is_err(), "{bad}");
        }
        assert!(Srgb::parse("#80ff0000").is_err());
    }

    #[test]
    fn keywords() {
        // SVG 1.0: 140 distinct names plus 7 grey spellings; CSS4 adds rebeccapurple.
        assert_eq!(KEYWORD_COUNT, 147);
        assert!(
            NAMES.windows(2).all(|w| w[0].0 < w[1].0),
            "sorted for bisecting"
        );
        assert!(Srgb::parse("RebeccaPurple").is_err());
        assert_eq!(Srgb::parse("Steelblue").unwrap().to_hex(), "#4682b4");
        assert_eq!(Srgba::parse("transparent").unwrap().a, 0.0);
        assert_eq!(Srgb::parse("grey").unwrap(), Srgb::parse("gray").unwrap());
    }
}
