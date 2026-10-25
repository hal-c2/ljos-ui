//! The spacing scale and the type scale.
//!
//! Spacing comes in steps of [`limits::SPACING_UNIT_PX`], so margins and
//! gaps line up and `layout-spacing-scale` can tell a step from a stray
//! number. Type sizes grow by one ratio from a body size (a modular scale,
//! as typographic guidance since Bringhurst describes it), and each line
//! height is rounded to the spacing unit so text sits on the same rhythm
//! as everything around it. Material 3's type scale and Apple's Dynamic
//! Type table are the reference for the body size (14 to 17 logical
//! pixels) and for line heights of about 1.4 for running text and 1.25 for
//! headings.

use serde::Serialize;

use crate::limits;

/// One step of the spacing scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Space {
    /// What the module names it: `Spacing.md`.
    pub name: &'static str,
    /// How many units.
    pub units: u32,
    pub px: f32,
}

/// The steps, smallest first: 0, then 1, 2, 3, 4, 6, 8, 12 and 16 units.
/// Close together where small differences show, further apart above.
pub const SPACING_UNITS: [(&str, u32); 9] = [
    ("none", 0),
    ("xxs", 1),
    ("xs", 2),
    ("sm", 3),
    ("md", 4),
    ("lg", 6),
    ("xl", 8),
    ("xxl", 12),
    ("xxxl", 16),
];

pub fn spacing() -> Vec<Space> {
    SPACING_UNITS
        .iter()
        .map(|&(name, units)| Space {
            name,
            units,
            px: units as f32 * limits::SPACING_UNIT_PX,
        })
        .collect()
}

/// Whether `px` is a step of the scale's unit (and so allowed by
/// `layout-spacing-scale`).
pub fn on_spacing_scale(px: f32) -> bool {
    let units = px / limits::SPACING_UNIT_PX;
    (units - units.round()).abs() < 1e-3
}

/// One size of the type scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TypeStep {
    /// `Type.body`, `Type.title`.
    pub name: &'static str,
    /// Logical pixels (`font.pixelSize`).
    pub px: u32,
    /// Logical pixels between baselines (`lineHeight` with
    /// `Text.FixedHeight`), a step of the spacing unit.
    pub line_height: u32,
    /// `font.weight`.
    pub weight: u16,
}

/// The body size the scale grows from.
pub const BODY_PX: f32 = 14.0;
/// The ratio between two sizes: a minor third.
pub const RATIO: f32 = 1.2;

/// The steps by how many ratios above or below body, with their weight.
const STEPS: [(&str, i32, u16); 7] = [
    ("caption", -1, 400),
    ("body", 0, 400),
    ("bodyStrong", 0, 600),
    ("subtitle", 1, 500),
    ("title", 2, 600),
    ("headline", 3, 600),
    ("display", 5, 700),
];

fn round_to_unit(px: f32) -> u32 {
    let unit = limits::SPACING_UNIT_PX;
    ((px / unit).round() * unit) as u32
}

/// The type scale, smallest first.
pub fn type_scale() -> Vec<TypeStep> {
    STEPS
        .iter()
        .map(|&(name, n, weight)| {
            let px = (BODY_PX * RATIO.powi(n)).round();
            // Running text needs room between its lines; a heading is
            // one or two lines and reads tighter.
            let leading = if n <= 1 { 1.4 } else { 1.25 };
            TypeStep {
                name,
                px: px as u32,
                line_height: round_to_unit(px * leading).max(round_to_unit(px) + 4),
                weight,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_space_is_on_the_scale_and_they_grow() {
        let s = spacing();
        assert!(s.iter().all(|s| on_spacing_scale(s.px)));
        assert!(s.windows(2).all(|w| w[0].px < w[1].px));
        assert_eq!(s.iter().find(|s| s.name == "md").unwrap().px, 16.0);
        assert!(!on_spacing_scale(5.0));
        assert!(on_spacing_scale(12.0));
    }

    #[test]
    fn the_type_scale_grows_by_its_ratio_and_lines_sit_on_the_unit() {
        let t = type_scale();
        let body = t.iter().find(|t| t.name == "body").unwrap();
        assert_eq!((body.px, body.line_height), (14, 20));
        for step in &t {
            assert!(step.line_height > step.px, "{step:?}");
            assert_eq!(step.line_height % limits::SPACING_UNIT_PX as u32, 0);
        }
        let sizes: Vec<u32> = t.iter().map(|t| t.px).collect();
        assert_eq!(sizes, [12, 14, 14, 17, 20, 24, 35]);
        // The largest step is large text in WCAG's sense; body is not.
        assert!(t.last().unwrap().px as f32 >= limits::LARGE_TEXT_PX);
        assert!((body.px as f32) < limits::LARGE_TEXT_PX);
    }
}
