//! `TextEngine::bounds` and the font-wide metrics against Qt 6.11
//! (`FontMetrics` in qml6, offscreen, Liberation Sans).
//!
//! Horizontal numbers are Qt's. Vertical ink edges can be one pixel off: Qt
//! measures hinted outlines, and this engine draws, and so measures, unhinted
//! ones.

use ljos_text::{TextEngine, TextStyle};

/// `(text, advance, bounding x y w h, tight x y w h)` as Qt printed them.
type Row = (&'static str, f64, [f64; 4], [f64; 4]);

const A14: f64 = 12.671875;

const SIZE_14: &[Row] = &[
    ("", 0.0, [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]),
    (" ", 3.875, [0.0, -A14, 0.0, 15.625], [0.0, 0.0, 0.0, 0.0]),
    ("x", 7.0, [0.0, -A14, 7.0, 15.625], [0.0, -8.0, 7.0, 8.0]),
    (
        "g",
        7.78125,
        [0.0, -A14, 7.0, 15.671875],
        [0.0, -8.0, 7.0, 11.0],
    ),
    (
        "Hg",
        17.890625,
        [1.0, -A14, 17.0, 15.671875],
        [1.0, -10.0, 17.0, 13.0],
    ),
    (
        "Hello",
        31.890625,
        [1.0, -A14, 32.0, 15.625],
        [1.0, -11.0, 32.0, 11.0],
    ),
    (
        "jolly",
        24.109375,
        [-1.0, -A14, 26.0, 15.671875],
        [-1.0, -11.0, 26.0, 14.0],
    ),
    (
        "AV",
        17.625,
        [0.0, -A14, 19.0, 15.625],
        [0.0, -10.0, 19.0, 10.0],
    ),
    (
        " a ",
        15.53125,
        [0.0, -A14, 12.0, 15.625],
        [0.0, -8.0, 12.0, 8.0],
    ),
    (
        "fj",
        6.984375,
        [0.0, -A14, 7.0, 15.671875],
        [0.0, -11.0, 7.0, 14.0],
    ),
    (
        "W.",
        16.3125,
        [0.0, -A14, 16.0, 15.625],
        [0.0, -10.0, 16.0, 10.0],
    ),
    (
        "שלום",
        30.734375,
        [0.0, -A14, 31.0, 15.625],
        [0.0, -11.0, 31.0, 12.0],
    ),
    (
        "a\nb",
        15.5625,
        [0.0, -A14, 16.0, 15.625],
        [0.0, -11.0, 16.0, 11.0],
    ),
    (
        "Ég",
        17.109375,
        [1.0, -13.0, 16.0, 16.0],
        [1.0, -13.0, 16.0, 16.0],
    ),
    (
        "Hello world",
        69.203125,
        [1.0, -A14, 68.0, 15.625],
        [1.0, -11.0, 68.0, 11.0],
    ),
    (
        "'",
        2.671875,
        [0.0, -A14, 2.0, 15.625],
        [0.0, -10.0, 2.0, 10.0],
    ),
    (
        "-",
        4.65625,
        [0.0, -A14, 5.0, 15.625],
        [0.0, -5.0, 5.0, 5.0],
    ),
    (
        "_",
        7.78125,
        [-1.0, -A14, 9.0, 15.671875],
        [-1.0, 2.0, 9.0, 1.0],
    ),
    (".", 3.875, [1.0, -A14, 2.0, 15.625], [1.0, -2.0, 2.0, 2.0]),
    (
        "o",
        7.78125,
        [0.0, -A14, 8.0, 15.625],
        [0.0, -8.0, 8.0, 8.0],
    ),
    (
        "Ty",
        14.78125,
        [0.0, -A14, 15.0, 15.671875],
        [0.0, -10.0, 15.0, 13.0],
    ),
    (
        "^",
        6.5625,
        [0.0, -A14, 7.0, 15.625],
        [0.0, -11.0, 7.0, 11.0],
    ),
    (
        "|",
        3.625,
        [1.0, -A14, 2.0, 15.671875],
        [1.0, -11.0, 2.0, 14.0],
    ),
    (
        "x y",
        17.875,
        [0.0, -A14, 18.0, 15.671875],
        [0.0, -8.0, 18.0, 11.0],
    ),
    (
        "a\n",
        7.78125,
        [0.0, -A14, 8.0, 15.625],
        [0.0, -8.0, 8.0, 8.0],
    ),
    (
        "é",
        7.78125,
        [0.0, -A14, 8.0, 15.625],
        [0.0, -12.0, 8.0, 12.0],
    ),
    (
        "Q",
        10.875,
        [0.0, -A14, 11.0, 15.671875],
        [0.0, -10.0, 11.0, 13.0],
    ),
    (
        "()",
        9.3125,
        [0.0, -A14, 9.0, 15.671875],
        [0.0, -11.0, 9.0, 14.0],
    ),
    (
        "p",
        7.78125,
        [0.0, -A14, 8.0, 15.671875],
        [0.0, -8.0, 8.0, 11.0],
    ),
    (
        "T",
        8.546875,
        [0.0, -A14, 9.0, 15.625],
        [0.0, -10.0, 9.0, 10.0],
    ),
];

const SIZE_20: &[Row] = &[
    (
        "'",
        3.8125,
        [1.0, -18.09375, 2.0, 22.328125],
        [1.0, -14.0, 2.0, 14.0],
    ),
    (
        "-",
        6.65625,
        [0.0, -18.09375, 6.0, 22.328125],
        [0.0, -7.0, 6.0, 7.0],
    ),
    (
        "_",
        11.109375,
        [-1.0, -18.09375, 13.0, 23.09375],
        [-1.0, 2.0, 13.0, 3.0],
    ),
    (
        ".",
        5.546875,
        [1.0, -18.09375, 3.0, 22.328125],
        [1.0, -3.0, 3.0, 3.0],
    ),
    (
        "o",
        11.109375,
        [0.0, -18.09375, 11.0, 22.328125],
        [0.0, -11.0, 11.0, 11.0],
    ),
    (
        "Ty",
        21.09375,
        [0.0, -18.09375, 22.0, 22.328125],
        [0.0, -14.0, 22.0, 18.0],
    ),
    (
        "^",
        9.375,
        [0.0, -18.09375, 10.0, 22.328125],
        [0.0, -15.0, 10.0, 15.0],
    ),
    (
        "|",
        5.1875,
        [1.0, -18.09375, 3.0, 22.328125],
        [1.0, -15.0, 3.0, 19.0],
    ),
    (
        "x y",
        25.546875,
        [0.0, -18.09375, 26.0, 22.328125],
        [0.0, -11.0, 26.0, 15.0],
    ),
    (
        "a\n",
        11.109375,
        [0.0, -18.09375, 12.0, 22.328125],
        [0.0, -11.0, 12.0, 11.0],
    ),
    (
        "é",
        11.109375,
        [0.0, -18.09375, 11.0, 22.328125],
        [0.0, -16.0, 11.0, 16.0],
    ),
    (
        "Q",
        15.546875,
        [0.0, -18.09375, 15.0, 22.328125],
        [0.0, -14.0, 15.0, 18.0],
    ),
    (
        "()",
        13.3125,
        [1.0, -18.09375, 12.0, 22.328125],
        [1.0, -15.0, 12.0, 19.0],
    ),
    (
        "p",
        11.109375,
        [1.0, -18.09375, 10.0, 22.328125],
        [1.0, -11.0, 10.0, 15.0],
    ),
    (
        "T",
        12.203125,
        [0.0, -18.09375, 12.0, 22.328125],
        [0.0, -14.0, 12.0, 14.0],
    ),
];
const SIZE_37: &[Row] = &[
    (
        "'",
        7.0625,
        [1.0, -33.484375, 5.0, 41.3125],
        [1.0, -26.0, 5.0, 26.0],
    ),
    (
        "-",
        12.3125,
        [1.0, -33.484375, 10.0, 41.3125],
        [1.0, -11.0, 10.0, 11.0],
    ),
    (
        "_",
        20.5625,
        [-1.0, -33.484375, 22.0, 41.3125],
        [-1.0, 4.0, 22.0, 3.0],
    ),
    (
        ".",
        10.265625,
        [3.0, -33.484375, 4.0, 41.3125],
        [3.0, -4.0, 4.0, 4.0],
    ),
    (
        "o",
        20.5625,
        [1.0, -33.484375, 19.0, 41.3125],
        [1.0, -20.0, 19.0, 20.0],
    ),
    (
        "Ty",
        39.046875,
        [0.0, -33.484375, 40.0, 41.484375],
        [0.0, -26.0, 40.0, 34.0],
    ),
    (
        "^",
        17.359375,
        [0.0, -33.484375, 18.0, 41.3125],
        [0.0, -26.0, 18.0, 26.0],
    ),
    (
        "|",
        9.609375,
        [3.0, -33.484375, 4.0, 41.484375],
        [3.0, -27.0, 4.0, 35.0],
    ),
    (
        "x y",
        47.265625,
        [0.0, -33.484375, 48.0, 41.484375],
        [0.0, -20.0, 48.0, 28.0],
    ),
    (
        "a\n",
        20.5625,
        [1.0, -33.484375, 20.0, 41.3125],
        [1.0, -20.0, 20.0, 20.0],
    ),
    (
        "é",
        20.5625,
        [1.0, -33.484375, 18.0, 41.3125],
        [1.0, -27.0, 18.0, 27.0],
    ),
    (
        "Q",
        28.765625,
        [1.0, -33.484375, 27.0, 41.3125],
        [1.0, -26.0, 27.0, 33.0],
    ),
    (
        "()",
        24.625,
        [2.0, -33.484375, 22.0, 41.484375],
        [2.0, -27.0, 22.0, 35.0],
    ),
    (
        "p",
        20.5625,
        [2.0, -33.484375, 18.0, 41.484375],
        [2.0, -20.0, 18.0, 28.0],
    ),
    (
        "T",
        22.59375,
        [0.0, -33.484375, 22.0, 41.3125],
        [0.0, -26.0, 22.0, 26.0],
    ),
];

fn rect(r: ljos_render::kurbo::Rect) -> [f64; 4] {
    [r.x0, r.y0, r.width(), r.height()]
}

const SIZES: [(f32, &[Row]); 3] = [(14.0, SIZE_14), (20.0, SIZE_20), (37.0, SIZE_37)];

#[test]
fn advances_and_horizontal_extents_are_qts() {
    let mut e = TextEngine::new();
    for (size, rows) in SIZES {
        let style = TextStyle::sized(size);
        for (text, advance, bounding, tight) in rows {
            let b = e.bounds(text, &style);
            assert_eq!(b.advance, *advance, "{size} {text:?}");
            // Qt widens a right-to-left run by a pixel on the left.
            if ljos_text::reads_right_to_left(text) {
                continue;
            }
            let (ours, ours_tight) = (rect(b.bounding), rect(b.tight));
            assert_eq!(
                (ours[0], ours[2]),
                (bounding[0], bounding[2]),
                "{size} {text:?}"
            );
            // A text without ink has no tight box at all.
            if tight[3] > 0.0 {
                assert_eq!(
                    (ours_tight[0], ours_tight[2]),
                    (tight[0], tight[2]),
                    "{size} {text:?}"
                );
            } else {
                assert_eq!(ours_tight, [0.0; 4], "{size} {text:?}");
            }
        }
    }
}

#[test]
fn vertical_extents_are_within_a_pixel_of_qts() {
    let mut e = TextEngine::new();
    for (size, rows) in SIZES {
        let style = TextStyle::sized(size);
        for (text, _, bounding, tight) in rows {
            let b = e.bounds(text, &style);
            for (ours, qt) in [(rect(b.bounding), bounding), (rect(b.tight), tight)] {
                let (top, bottom) = (ours[1], ours[1] + ours[3]);
                let (qt_top, qt_bottom) = (qt[1], qt[1] + qt[3]);
                assert!(
                    (top - qt_top).abs() <= 1.0 && (bottom - qt_bottom).abs() <= 1.0,
                    "{size} {text:?}: {ours:?} for {qt:?}"
                );
            }
        }
    }
}

#[test]
fn everyday_text_at_the_default_size_is_exactly_qts() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    for (text, _, bounding, tight) in SIZE_14 {
        // Hinting moves these four by a pixel in Qt.
        if ["שלום", "_", "^", "é"].contains(text) {
            continue;
        }
        let b = e.bounds(text, &style);
        assert_eq!(rect(b.bounding), *bounding, "{text:?}");
        assert_eq!(rect(b.tight), *tight, "{text:?}");
    }
}

#[test]
fn a_box_is_at_least_as_tall_as_the_font() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    let m = e.font_metrics(&style);
    for text in ["x", "-", "Hello", " "] {
        let b = e.bounds(text, &style).bounding;
        assert_eq!((b.y0, b.y1), (-m.ascent, m.descent), "{text:?}");
    }
    // Ink beyond the font's ascent or descent makes the box taller.
    let tall = e.bounds("Ég", &style);
    assert!(tall.bounding.y0 < -m.ascent && tall.bounding.y1 > m.descent);
    assert_eq!(tall.bounding, tall.tight);
}

#[test]
fn letter_spacing_moves_the_pen_and_the_box() {
    let mut e = TextEngine::new();
    let plain = e.bounds("Hello", &TextStyle::default());
    let spaced = e.bounds(
        "Hello",
        &TextStyle {
            letter_spacing: 2.0,
            ..TextStyle::default()
        },
    );
    // Measured on Qt: 41.890625 wide, box (1, _, 40, _).
    assert_eq!(spaced.advance, plain.advance + 10.0);
    assert_eq!(rect(spaced.bounding)[0], 1.0);
    assert_eq!(rect(spaced.bounding)[2], 40.0);
}

/// `(pixel size, maximumCharacterWidth, averageCharacterWidth, xHeight,
/// underlinePosition, lineWidth, minimumLeftBearing, minimumRightBearing)`
/// as Qt printed them for Liberation Sans.
const FONT_WIDE: &[(f32, [f64; 7])] = &[
    (
        8.0,
        [
            11.0, 4.640625, 4.234375, 0.265625, 1.0, -4.3515625, -5.5234375,
        ],
    ),
    (
        9.0,
        [
            12.0,
            5.21875,
            4.75,
            0.296875,
            1.0,
            -4.8955078125,
            -6.2138671875,
        ],
    ),
    (
        12.0,
        [
            16.0,
            6.953125,
            6.34375,
            0.390625,
            1.0,
            -6.52734375,
            -8.28515625,
        ],
    ),
    (
        14.0,
        [
            19.0,
            8.109375,
            7.390625,
            0.453125,
            1.03125,
            -7.615234375,
            -9.666015625,
        ],
    ),
    (
        16.0,
        [
            21.0, 9.28125, 8.453125, 0.53125, 1.171875, -8.703125, -11.046875,
        ],
    ),
    (
        20.0,
        [
            27.0,
            11.59375,
            10.5625,
            0.65625,
            1.46875,
            -10.87890625,
            -13.80859375,
        ],
    ),
    (
        24.0,
        [
            32.0,
            13.90625,
            12.6875,
            0.796875,
            1.765625,
            -13.0546875,
            -16.5703125,
        ],
    ),
    (
        48.0,
        [
            64.0, 27.828125, 25.359375, 1.578125, 3.515625, -26.109375, -33.140625,
        ],
    ),
];

#[test]
fn font_wide_metrics_are_qts() {
    let mut e = TextEngine::new();
    for (size, qt) in FONT_WIDE {
        let m = e.font_metrics(&TextStyle::sized(*size));
        assert_eq!(m.max_char_width, qt[0], "{size}");
        assert_eq!(m.average_char_width, qt[1], "{size}");
        assert_eq!(m.x_height, qt[2], "{size}");
        // Qt rounds the underline's position and thickness separately, so
        // its top can land 1/64 pixel away.
        assert!(
            (m.underline_position - qt[3]).abs() <= 1.0 / 64.0,
            "{size}: {}",
            m.underline_position
        );
        assert_eq!(m.line_width, qt[4], "{size}");
        assert_eq!(m.min_left_bearing, qt[5], "{size}");
        assert_eq!(m.min_right_bearing, qt[6], "{size}");
    }
}

#[test]
fn bold_has_its_own_font_wide_metrics() {
    let mut e = TextEngine::new();
    let m = e.font_metrics(&TextStyle {
        weight: 700.0,
        ..TextStyle::sized(32.0)
    });
    // Qt, Liberation Sans Bold at 32 px.
    assert_eq!(m.max_char_width, 43.0);
    assert_eq!(m.average_char_width, 19.5);
    assert_eq!(m.x_height, 16.90625);
    assert_eq!(m.line_width, 3.359375);
    assert_eq!(m.min_left_bearing, -15.421875);
    assert_eq!(m.min_right_bearing, -22.09375);
    assert_eq!((m.ascent, m.descent), (28.96875, 6.78125));
}

#[test]
fn measuring_twice_shapes_once() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    let first = e.bounds("Hello world", &style);
    let built = e.stats().shapes_built;
    assert_eq!(e.bounds("Hello world", &style), first);
    assert_eq!(e.stats().shapes_built, built);
}
