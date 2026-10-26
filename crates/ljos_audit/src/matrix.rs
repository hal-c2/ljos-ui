//! The settings the scene is audited under: the window as it is, and as
//! a user with other settings or a smaller screen has it.
//!
//! A point names only what it changes; the rest stays as the scene has
//! it. Written and read as the fixtures write it:
//! `scheme=dark contrast=high font-scale=2 size=800x600 reduced-motion=true`,
//! with `size=small` for the window at its minimum size and `current` for
//! the point that changes nothing.

use std::fmt;
use std::str::FromStr;

use ljos_a11y::{ColorScheme, Contrast};
use ljos_design::limits::audit::FONT_SCALES;

/// The size a point gives the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointSize {
    /// Width and height in logical pixels.
    Exact(f64, f64),
    /// The window's `minimumWidth` and `minimumHeight`, or, for a side
    /// it has none on, a fraction of its size
    /// (`ljos_design::limits::audit::NARROW_FRACTION` and
    /// `SHORT_FRACTION`).
    Small,
}

/// One point of the matrix.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MatrixPoint {
    pub scheme: Option<ColorScheme>,
    pub contrast: Option<Contrast>,
    pub font_scale: Option<f64>,
    pub size: Option<PointSize>,
    pub reduced_motion: Option<bool>,
}

impl MatrixPoint {
    /// The point that changes nothing.
    pub const CURRENT: MatrixPoint = MatrixPoint {
        scheme: None,
        contrast: None,
        font_scale: None,
        size: None,
        reduced_motion: None,
    };

    pub fn is_current(&self) -> bool {
        *self == MatrixPoint::CURRENT
    }
}

/// What a point could not be read from.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "matrix point `{0}`: expected `current` or settings like `scheme=dark contrast=high \
     font-scale=2 size=800x600 size=small reduced-motion=true`"
)]
pub struct PointError(pub String);

impl FromStr for MatrixPoint {
    type Err = PointError;

    fn from_str(s: &str) -> Result<Self, PointError> {
        let bad = || PointError(s.to_string());
        let mut p = MatrixPoint::CURRENT;
        for word in s.split_whitespace() {
            if word == "current" {
                continue;
            }
            let (key, value) = word.split_once('=').ok_or_else(bad)?;
            match key {
                "scheme" => {
                    p.scheme = Some(match value {
                        "light" => ColorScheme::Light,
                        "dark" => ColorScheme::Dark,
                        _ => return Err(bad()),
                    });
                }
                "contrast" => {
                    p.contrast = Some(match value {
                        "high" => Contrast::High,
                        "normal" => Contrast::NoPreference,
                        _ => return Err(bad()),
                    });
                }
                "font-scale" => {
                    let n: f64 = value.parse().map_err(|_| bad())?;
                    if !(0.25..=8.0).contains(&n) {
                        return Err(bad());
                    }
                    p.font_scale = Some(n);
                }
                "size" if value == "small" => p.size = Some(PointSize::Small),
                "size" => {
                    let (w, h) = value.split_once('x').ok_or_else(bad)?;
                    let side =
                        |v: &str| v.parse::<f64>().ok().filter(|n| *n >= 1.0 && *n <= 16384.0);
                    p.size = Some(PointSize::Exact(
                        side(w).ok_or_else(bad)?,
                        side(h).ok_or_else(bad)?,
                    ));
                }
                "reduced-motion" => {
                    p.reduced_motion = Some(value.parse().map_err(|_| bad())?);
                }
                _ => return Err(bad()),
            }
        }
        Ok(p)
    }
}

fn number(n: f64) -> String {
    let s = format!("{n:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl fmt::Display for MatrixPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut words = Vec::new();
        match self.scheme {
            Some(ColorScheme::Dark) => words.push("scheme=dark".to_string()),
            Some(ColorScheme::Light) => words.push("scheme=light".to_string()),
            _ => {}
        }
        match self.contrast {
            Some(Contrast::High) => words.push("contrast=high".to_string()),
            Some(Contrast::NoPreference) => words.push("contrast=normal".to_string()),
            None => {}
        }
        if let Some(n) = self.font_scale {
            words.push(format!("font-scale={}", number(n)));
        }
        match self.size {
            Some(PointSize::Exact(w, h)) => words.push(format!("size={}x{}", number(w), number(h))),
            Some(PointSize::Small) => words.push("size=small".to_string()),
            None => {}
        }
        if let Some(r) = self.reduced_motion {
            words.push(format!("reduced-motion={r}"));
        }
        if words.is_empty() {
            f.write_str("current")
        } else {
            f.write_str(&words.join(" "))
        }
    }
}

/// Which points an audit looks at.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Matrix {
    /// The scene as it is: cheap, and changes nothing.
    #[default]
    Current,
    /// [`Matrix::full_points`].
    Full,
    Points(Vec<MatrixPoint>),
}

impl Matrix {
    /// The scene as it is; light and dark, each at normal and high
    /// contrast; each larger font scale of `limits::audit::FONT_SCALES`;
    /// and the small window, at the scale it is and at the largest. Not
    /// every combination: each point is there for the rules it moves,
    /// and the product would cost dozens of layouts for little more.
    pub fn full_points() -> Vec<MatrixPoint> {
        let mut out = vec![MatrixPoint::CURRENT];
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            for contrast in [Contrast::NoPreference, Contrast::High] {
                out.push(MatrixPoint {
                    scheme: Some(scheme),
                    contrast: Some(contrast),
                    ..MatrixPoint::CURRENT
                });
            }
        }
        for &scale in FONT_SCALES.iter().filter(|s| **s > 1.0) {
            out.push(MatrixPoint {
                font_scale: Some(f64::from(scale)),
                ..MatrixPoint::CURRENT
            });
        }
        out.push(MatrixPoint {
            size: Some(PointSize::Small),
            ..MatrixPoint::CURRENT
        });
        if let Some(&largest) = FONT_SCALES.iter().max_by(|a, b| a.total_cmp(b)) {
            out.push(MatrixPoint {
                size: Some(PointSize::Small),
                font_scale: Some(f64::from(largest)),
                ..MatrixPoint::CURRENT
            });
        }
        out
    }

    pub fn points(&self) -> Vec<MatrixPoint> {
        match self {
            Matrix::Current => vec![MatrixPoint::CURRENT],
            Matrix::Full => Matrix::full_points(),
            Matrix::Points(p) => p.clone(),
        }
    }
}

impl FromStr for Matrix {
    type Err = PointError;

    /// `current`, `full`, or points separated by `;` or `,`.
    fn from_str(s: &str) -> Result<Self, PointError> {
        match s.trim() {
            "" | "current" => Ok(Matrix::Current),
            "full" => Ok(Matrix::Full),
            many => many
                .split([';', ','])
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::parse)
                .collect::<Result<Vec<_>, _>>()
                .map(Matrix::Points),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_reads_back_as_it_is_written() {
        for s in [
            "current",
            "scheme=dark contrast=high font-scale=2 size=800x600 reduced-motion=true",
            "size=small font-scale=1.5",
        ] {
            let p: MatrixPoint = s.parse().unwrap();
            let again: MatrixPoint = p.to_string().parse().unwrap();
            assert_eq!(p, again, "{s}");
        }
        assert_eq!(
            "font-scale=2 size=640x480"
                .parse::<MatrixPoint>()
                .unwrap()
                .to_string(),
            "font-scale=2 size=640x480"
        );
        assert!("scheme=sepia".parse::<MatrixPoint>().is_err());
        assert!("size=0x10".parse::<MatrixPoint>().is_err());
    }

    #[test]
    fn the_full_matrix_starts_with_the_scene_as_it_is() {
        let points = Matrix::Full.points();
        assert!(points[0].is_current());
        assert!(points.iter().any(|p| p.size == Some(PointSize::Small)));
        assert!(points.iter().any(|p| p.font_scale == Some(2.0)));
        assert_eq!("full".parse::<Matrix>().unwrap(), Matrix::Full);
        assert_eq!(
            "scheme=dark; font-scale=2"
                .parse::<Matrix>()
                .unwrap()
                .points()
                .len(),
            2
        );
    }
}
