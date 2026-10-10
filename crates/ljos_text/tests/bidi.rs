//! Right-to-left and mixed text.

use ljos_text::{ElideMode, LayoutParams, TextEngine, TextStyle, WrapMode};

const SHALOM_OLAM: &str = "שלום עולם";

/// Qt's number, exactly: both count advances in 64ths of a pixel.
fn same(actual: f64, qt: f64) -> bool {
    actual == qt
}

#[test]
fn the_first_strong_character_sets_the_direction() {
    let mut e = TextEngine::new();
    let s = TextStyle::default();
    let p = LayoutParams::default();
    assert!(e.layout(SHALOM_OLAM, &s, &p).rtl);
    assert!(e.layout("שלום world", &s, &p).rtl);
    assert!(!e.layout("hello עולם", &s, &p).rtl);
    assert!(!e.layout("Hello", &s, &p).rtl);
    assert!(!e.layout("", &s, &p).rtl);
    assert!(!e.layout("123 456", &s, &p).rtl);
    // Digits and punctuation are not strong.
    assert!(e.layout("123 שלום", &s, &p).rtl);
}

#[test]
fn hebrew_is_shaped_with_real_glyphs_in_visual_order() {
    let mut e = TextEngine::new();
    let l = e.layout(SHALOM_OLAM, &TextStyle::default(), &LayoutParams::default());
    // Width in Qt with the same font.
    assert!(same(l.natural_width, 62.984375), "{}", l.natural_width);
    assert_eq!(l.content_height, 16.0);
    let glyphs = &l.lines[0].runs[0].glyphs.glyphs;
    assert_eq!(glyphs.len(), 9);
    assert!(glyphs.iter().all(|g| g.id != 0), "no missing glyphs");
    assert!(glyphs.windows(2).all(|w| w[0].x < w[1].x));

    // The first letter of the text (shin) is drawn last, at the right.
    let shin = e
        .layout("ש", &TextStyle::default(), &LayoutParams::default())
        .lines[0]
        .runs[0]
        .glyphs
        .glyphs[0]
        .id;
    assert_eq!(glyphs.last().unwrap().id, shin);
}

#[test]
fn mixed_text_puts_each_run_on_its_side() {
    let mut e = TextEngine::new();
    let l = e.layout(
        "שלום world",
        &TextStyle::default(),
        &LayoutParams::default(),
    );
    let runs = &l.lines[0].runs;
    assert_eq!(runs.len(), 2);
    let w = e
        .layout("w", &TextStyle::default(), &LayoutParams::default())
        .lines[0]
        .runs[0]
        .glyphs
        .glyphs[0]
        .id;
    // In a right-to-left paragraph the Latin word comes second, so it sits
    // on the left.
    assert_eq!(runs[0].glyphs.glyphs[0].id, w);
    assert_eq!(runs[0].glyphs.glyphs[0].x, 0.0);
    assert!(runs[1].glyphs.glyphs[0].x > 30.0);
}

#[test]
fn wrapped_right_to_left_lines_start_at_zero() {
    let mut e = TextEngine::new();
    let text = "שלום עולם, זה טקסט ארוך מאוד";
    let l = e.layout(
        text,
        &TextStyle::default(),
        &LayoutParams {
            width: Some(100.0),
            wrap: WrapMode::WordWrap,
            ..LayoutParams::default()
        },
    );
    assert!(l.rtl);
    assert_eq!(l.line_count(), 3);
    for line in &l.lines {
        assert!(line.width <= 100.0 && line.width > 20.0, "{}", line.width);
        // Ink starts at the line's left edge: the space the line wrapped at
        // hangs outside, to the left of 0.
        let inked: Vec<_> = line
            .runs
            .iter()
            .flat_map(|r| &r.glyphs.glyphs)
            .filter(|g| g.x >= 0.0)
            .collect();
        let min = inked.iter().map(|g| g.x).fold(f32::MAX, f32::min);
        assert!(min.abs() < 0.01, "{min}");
        let max = inked.iter().map(|g| g.x).fold(f32::MIN, f32::max);
        assert!(f64::from(max) < line.width, "{max} {}", line.width);
    }
}

#[test]
fn eliding_keeps_the_direction() {
    let mut e = TextEngine::new();
    let text = "שלום עולם, זה טקסט ארוך מאוד";
    let s = TextStyle::default();
    let elided = e.elided_text(text, &s, ElideMode::Right, 100.0);
    assert!(elided.starts_with("שלום"), "{elided}");
    assert!(elided.ends_with('\u{2026}'), "{elided}");

    let l = e.layout(
        text,
        &s,
        &LayoutParams {
            width: Some(100.0),
            elide: ElideMode::Right,
            ..LayoutParams::default()
        },
    );
    assert!(l.rtl && l.truncated);
    assert!(l.content_width <= 100.0);
    // The ellipsis ends the text, so in a right-to-left line it is leftmost.
    let dots = e.layout("\u{2026}", &s, &LayoutParams::default()).lines[0].runs[0]
        .glyphs
        .glyphs[0]
        .id;
    let leftmost = l.lines[0]
        .runs
        .iter()
        .flat_map(|r| &r.glyphs.glyphs)
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap();
    assert_eq!(leftmost.id, dots);
}

#[test]
fn the_direction_helper_agrees_with_the_layout() {
    let mut e = TextEngine::new();
    let s = TextStyle::default();
    let p = LayoutParams::default();
    for text in [
        "",
        "Hello",
        SHALOM_OLAM,
        "שלום world",
        "hello עולם",
        "123 456",
        "123 שלום",
        "(…) שלום",
        "\u{200F}abc",
        "\u{200E}שלום",
        "Ελληνικά",
        "Привет",
        "line\nשלום",
    ] {
        assert_eq!(
            ljos_text::reads_right_to_left(text),
            e.layout(text, &s, &p).rtl,
            "{text:?}"
        );
    }
}
