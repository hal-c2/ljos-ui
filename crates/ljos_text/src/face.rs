//! Which of a font's own numbers to believe.
//!
//! A font file states its ascent and descent up to three times (`hhea`, and
//! the typographic and Windows sets in `OS/2`), and the three often disagree.
//! Measured on Qt 6.11 with fonts where they do: Qt takes the typographic set
//! when the font asks for it, otherwise the Windows set, which has no line
//! gap, and `hhea` only for a font without `OS/2`. Text is as tall here as it
//! is there, so the same choice is made here.

use ljos_render::FontTables;
use ljos_render::peniko::FontData;

/// FaceMetrics metrics of a font at one size, in pixels, not yet rounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FaceMetrics {
    /// Baseline to the top of the line.
    pub ascent: f64,
    /// Baseline to the bottom of the line.
    pub descent: f64,
    /// The gap the font asks for between lines.
    pub leading: f64,
}

/// The metrics Qt would use, from the font's tables.
pub(crate) fn chosen(tables: &FontTables, size: f64) -> FaceMetrics {
    let (ascent, descent, gap) = match tables.os2 {
        Some(os2) if os2.use_typo_metrics => (
            f64::from(os2.typo_ascender),
            -f64::from(os2.typo_descender),
            f64::from(os2.typo_line_gap),
        ),
        // Some fonts leave the Windows set empty; it says nothing then.
        Some(os2) if os2.win_ascent != 0 || os2.win_descent != 0 => {
            (f64::from(os2.win_ascent), f64::from(os2.win_descent), 0.0)
        }
        _ => (
            f64::from(tables.ascender),
            -f64::from(tables.descender),
            f64::from(tables.line_gap),
        ),
    };
    let scale = size / f64::from(tables.units_per_em);
    FaceMetrics {
        ascent: (ascent * scale).max(0.0),
        descent: (descent * scale).max(0.0),
        leading: (gap * scale).max(0.0),
    }
}

/// FaceMetrics metrics for a run in `font`. `shaped` is what the shaper says,
/// for data whose tables cannot be read.
pub(crate) fn vertical(font: &FontData, size: f32, shaped: FaceMetrics) -> FaceMetrics {
    crate::measure::font_tables(font).map_or(shaped, |tables| chosen(&tables, f64::from(size)))
}

#[cfg(test)]
mod tests {
    use ljos_render::Os2Table;

    use super::*;

    fn tables(os2: Option<Os2Table>) -> FontTables {
        FontTables {
            units_per_em: 1000,
            ascender: 737,
            descender: -263,
            line_gap: 200,
            advance_width_max: 1000,
            min_left_side_bearing: 0,
            min_right_side_bearing: 0,
            os2,
        }
    }

    fn os2(use_typo_metrics: bool) -> Os2Table {
        Os2Table {
            typo_ascender: 800,
            typo_descender: -200,
            typo_line_gap: 90,
            win_ascent: 1116,
            win_descent: 302,
            use_typo_metrics,
            x_avg_char_width: 500,
        }
    }

    fn numbers(v: FaceMetrics) -> (f64, f64, f64) {
        (v.ascent, v.descent, v.leading)
    }

    // The numbers of two installed fonts, with what Qt reports at 1000 px.
    #[test]
    fn windows_metrics_win_and_have_no_gap() {
        let v = chosen(&tables(Some(os2(false))), 1000.0);
        assert_eq!(numbers(v), (1116.0, 302.0, 0.0));
    }

    #[test]
    fn typographic_metrics_when_the_font_asks() {
        let v = chosen(&tables(Some(os2(true))), 100.0);
        assert_eq!(numbers(v), (80.0, 20.0, 9.0));
    }

    #[test]
    fn hhea_without_an_os2_table() {
        let v = chosen(&tables(None), 10.0);
        assert_eq!(numbers(v), (7.37, 2.63, 2.0));
    }

    #[test]
    fn empty_windows_metrics_say_nothing() {
        let empty = Os2Table {
            win_ascent: 0,
            win_descent: 0,
            ..os2(false)
        };
        let v = chosen(&tables(Some(empty)), 1000.0);
        assert_eq!(numbers(v), (737.0, 263.0, 200.0));
    }
}
