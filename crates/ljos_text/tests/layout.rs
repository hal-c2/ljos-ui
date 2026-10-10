//! Layout numbers with the bundled font.
//!
//! The expected values come from Qt 6.11 (qml6, offscreen) with Liberation
//! Sans installed, and are met to the last bit: Qt cuts every glyph advance
//! down to 1/64 pixel, and so does this engine (`snap.rs`).

use ljos_text::{ElideMode, LayoutParams, LineHeight, TextEngine, TextLayout, TextStyle, WrapMode};

const LONG: &str = "Hello world, this is a long line of text";

/// Sums of 64ths of a pixel are exact in an `f64`, so there is no tolerance.
fn same(actual: f64, qt: f64) -> bool {
    actual == qt
}

fn plain(engine: &mut TextEngine, text: &str) -> std::sync::Arc<TextLayout> {
    engine.layout(text, &TextStyle::default(), &LayoutParams::default())
}

fn with(width: f64, wrap: WrapMode, elide: ElideMode) -> LayoutParams {
    LayoutParams {
        width: Some(width),
        wrap,
        elide,
        ..LayoutParams::default()
    }
}

/// The characters of every glyph run cannot be read back, so tests that care
/// about the elided string ask the engine for it.
fn elided(engine: &mut TextEngine, text: &str, mode: ElideMode, width: f64) -> String {
    engine.elided_text(text, &TextStyle::default(), mode, width)
}

#[test]
fn one_line_has_qt_metrics() {
    let mut e = TextEngine::new();
    let l = plain(&mut e, "Hello world");
    assert!(same(l.natural_width, 69.203125), "{}", l.natural_width);
    assert_eq!(l.content_width, l.natural_width);
    assert_eq!(l.content_height, 16.0);
    assert_eq!(l.implicit_height, 16.0);
    assert_eq!(l.baseline, 12.671875);
    assert_eq!(l.line_count(), 1);
    assert!(!l.truncated);
    assert!(!l.rtl);
    assert_eq!(l.glyph_count(), 11);
}

#[test]
fn widths_are_whole_sixty_fourths() {
    let mut e = TextEngine::new();
    for text in ["Hello world", "AVAVAV", "x", LONG] {
        let w = plain(&mut e, text).natural_width;
        assert_eq!((w * 64.0).fract(), 0.0, "{text}: {w}");
    }
}

#[test]
fn line_height_follows_the_pixel_size() {
    let mut e = TextEngine::new();
    // pixelSize -> height of one line, read off Qt.
    for (size, height) in [
        (8.0, 9.0),
        (10.0, 12.0),
        (12.0, 14.0),
        (14.0, 16.0),
        (16.0, 18.0),
        (20.0, 23.0),
        (24.0, 27.0),
        (32.0, 36.0),
        (48.0, 54.0),
    ] {
        let l = e.layout(
            "Hello world",
            &TextStyle::sized(size),
            &LayoutParams::default(),
        );
        assert_eq!(l.content_height, height, "pixelSize {size}");
    }
    let l = e.layout(
        "Hello world",
        &TextStyle::sized(20.0),
        &LayoutParams::default(),
    );
    assert!(same(l.natural_width, 98.828125), "{}", l.natural_width);
}

#[test]
fn font_metrics_match_qt() {
    let mut e = TextEngine::new();
    let m = e.font_metrics(&TextStyle::default());
    assert_eq!(m.ascent, 12.671875);
    assert_eq!(m.descent, 2.953125);
    assert_eq!(m.height, 15.625);
    assert_eq!(m.line_height, 16.0);
    // The font's `hhea` table asks for a gap; Qt reads the Windows metrics,
    // which have none.
    assert_eq!(m.leading, 0.0);
    assert_eq!(m.x_height, 7.390625);
    assert_eq!(m.underline_position, 0.453125);
    assert_eq!(m.line_width, 1.03125);
}

#[test]
fn empty_text_is_one_empty_line() {
    let mut e = TextEngine::new();
    let l = plain(&mut e, "");
    assert_eq!(l.natural_width, 0.0);
    assert_eq!(l.content_width, 0.0);
    assert_eq!(l.content_height, 16.0);
    assert_eq!(l.baseline, 12.671875);
    assert_eq!(l.line_count(), 1);
    assert_eq!(l.glyph_count(), 0);
}

#[test]
fn line_breaks_in_the_text_make_lines() {
    let mut e = TextEngine::new();
    let l = plain(&mut e, "a\n\nb");
    assert_eq!(l.line_count(), 3);
    assert_eq!(l.content_height, 48.0);
    assert_eq!(l.lines[1].runs.len(), 0);
    assert_eq!(l.lines[2].top, 32.0);
    assert_eq!(l.lines[2].baseline, 32.0 + 12.671875);

    // A line break that ends the text starts no line.
    assert_eq!(plain(&mut e, "a\n").line_count(), 1);
    assert_eq!(plain(&mut e, "\n").line_count(), 1);
    assert_eq!(plain(&mut e, "a\n\n").line_count(), 2);

    let l = plain(&mut e, "short\na much longer line");
    let widest = plain(&mut e, "a much longer line").natural_width;
    assert_eq!(l.natural_width, widest);
    assert_eq!(l.content_width, widest);
}

#[test]
fn trailing_spaces_count() {
    let mut e = TextEngine::new();
    let bare = plain(&mut e, "Hello").natural_width;
    let spaced = plain(&mut e, "Hello  ").natural_width;
    assert!(spaced > bare + 7.0, "{bare} {spaced}");
}

#[test]
fn kerning_is_applied() {
    let mut e = TextEngine::new();
    let pair = plain(&mut e, "AV").natural_width;
    let apart = plain(&mut e, "A").natural_width + plain(&mut e, "V").natural_width;
    assert!(pair < apart - 0.5, "{pair} {apart}");
}

#[test]
fn word_wrap_breaks_between_words() {
    let mut e = TextEngine::new();
    let natural = plain(&mut e, LONG).natural_width;
    assert!(same(natural, 222.28125), "{natural}");

    let l = e.layout(
        LONG,
        &TextStyle::default(),
        &with(100.0, WrapMode::WordWrap, ElideMode::None),
    );
    assert_eq!(l.line_count(), 3);
    assert!(same(l.content_width, 98.71875), "{}", l.content_width);
    assert_eq!(l.content_height, 48.0);
    assert_eq!(l.implicit_height, 48.0);
    assert_eq!(l.natural_width, natural);
    assert!(!l.truncated);
    // Every line starts at the left edge; alignment is the item's business.
    for line in &l.lines {
        let first = &line.runs[0].glyphs.glyphs[0];
        assert_eq!(first.x, 0.0);
        assert!(line.width <= 100.0);
    }
}

#[test]
fn an_item_as_wide_as_its_text_does_not_wrap_or_elide() {
    let mut e = TextEngine::new();
    for text in ["Hello world", LONG, "AVAVAV Wa To"] {
        let natural = plain(&mut e, text).natural_width;
        for wrap in [WrapMode::WordWrap, WrapMode::WrapAnywhere, WrapMode::Wrap] {
            let l = e.layout(
                text,
                &TextStyle::default(),
                &with(natural, wrap, ElideMode::None),
            );
            assert_eq!(l.line_count(), 1, "{text} {wrap:?}");
        }
        let l = e.layout(
            text,
            &TextStyle::default(),
            &with(natural, WrapMode::NoWrap, ElideMode::Right),
        );
        assert!(!l.truncated, "{text}");
    }
}

#[test]
fn a_long_word_overflows_breaks_or_both() {
    let mut e = TextEngine::new();
    let word = "Incomprehensibilities";
    let style = TextStyle::default();
    let natural = plain(&mut e, word).natural_width;

    let l = e.layout(
        word,
        &style,
        &with(50.0, WrapMode::WordWrap, ElideMode::None),
    );
    assert_eq!(l.line_count(), 1);
    assert_eq!(l.content_width, natural);

    let l = e.layout(
        word,
        &style,
        &with(50.0, WrapMode::WrapAnywhere, ElideMode::None),
    );
    assert_eq!(l.line_count(), 3);
    assert!(l.content_width <= 50.0);

    let l = e.layout(word, &style, &with(50.0, WrapMode::Wrap, ElideMode::None));
    assert_eq!(l.line_count(), 3);
    assert!(l.content_width <= 50.0);

    // Wrap prefers the gap between words, WrapAnywhere fills every line.
    let two = "aaa Incomprehensibilities";
    let wrap = e.layout(two, &style, &with(50.0, WrapMode::Wrap, ElideMode::None));
    assert!(same(
        wrap.lines[0].width,
        plain(&mut e, "aaa").natural_width
    ));
    let anywhere = e.layout(
        two,
        &style,
        &with(50.0, WrapMode::WrapAnywhere, ElideMode::None),
    );
    assert!(anywhere.lines[0].width > wrap.lines[0].width + 10.0);
}

#[test]
fn width_zero_or_none_does_not_wrap() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    for width in [None, Some(0.0), Some(-5.0), Some(f64::NAN)] {
        let l = e.layout(
            LONG,
            &style,
            &LayoutParams {
                width,
                wrap: WrapMode::WordWrap,
                elide: ElideMode::Right,
                ..LayoutParams::default()
            },
        );
        assert_eq!(l.line_count(), 1, "{width:?}");
        assert!(!l.truncated, "{width:?}");
    }
}

#[test]
fn elide_cuts_one_line_to_the_width() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    // contentWidth of the elided text in Qt.
    for (mode, qt) in [
        (ElideMode::Right, 94.828125),
        (ElideMode::Left, 97.15625),
        (ElideMode::Middle, 97.9375),
    ] {
        let l = e.layout(LONG, &style, &with(100.0, WrapMode::NoWrap, mode));
        assert!(l.truncated, "{mode:?}");
        assert_eq!(l.line_count(), 1, "{mode:?}");
        assert!(same(l.content_width, qt), "{mode:?}: {}", l.content_width);
        assert!(l.content_width <= 100.0);
        assert_eq!(l.content_height, 16.0);
        assert!(same(l.natural_width, 222.28125));
    }
    assert_eq!(
        elided(&mut e, LONG, ElideMode::Right, 100.0),
        "Hello world, t\u{2026}"
    );
    assert_eq!(
        elided(&mut e, LONG, ElideMode::Left, 100.0),
        "\u{2026}ng line of text"
    );
    assert_eq!(
        elided(&mut e, LONG, ElideMode::Middle, 100.0),
        "Hello w\u{2026}of text"
    );
    assert_eq!(elided(&mut e, "Hello", ElideMode::Right, 100.0), "Hello");
    assert_eq!(elided(&mut e, LONG, ElideMode::None, 100.0), LONG);
}

#[test]
fn elide_narrower_than_the_ellipsis_shows_nothing() {
    let mut e = TextEngine::new();
    let l = e.layout(
        LONG,
        &TextStyle::default(),
        &with(2.0, WrapMode::NoWrap, ElideMode::Right),
    );
    assert!(l.truncated);
    assert_eq!(l.content_width, 0.0);
    assert_eq!(l.line_count(), 1);
    assert_eq!(l.content_height, 16.0);
    assert_eq!(l.glyph_count(), 0);
    assert_eq!(elided(&mut e, LONG, ElideMode::Right, 2.0), "");
}

#[test]
fn elide_does_not_touch_lines_from_the_text() {
    let mut e = TextEngine::new();
    let text = "Hello\nHello world, this is a long line of text";
    let l = e.layout(
        text,
        &TextStyle::default(),
        &with(100.0, WrapMode::NoWrap, ElideMode::Right),
    );
    assert_eq!(l.line_count(), 2);
    assert!(!l.truncated);
    assert!(l.content_width > 200.0);
}

#[test]
fn elide_left_and_middle_do_nothing_to_wrapped_text() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    let whole = e.layout(
        LONG,
        &style,
        &with(100.0, WrapMode::WordWrap, ElideMode::None),
    );
    for mode in [ElideMode::Left, ElideMode::Middle, ElideMode::Right] {
        let l = e.layout(LONG, &style, &with(100.0, WrapMode::WordWrap, mode));
        assert_eq!(l.line_count(), 3, "{mode:?}");
        assert!(!l.truncated, "{mode:?}");
        assert_eq!(l.glyph_count(), whole.glyph_count(), "{mode:?}");
    }
}

#[test]
fn maximum_line_count_drops_lines() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    let capped = |max_lines, elide| LayoutParams {
        max_lines,
        ..with(100.0, WrapMode::WordWrap, elide)
    };
    let whole = e.layout(LONG, &style, &capped(0, ElideMode::None));

    let l = e.layout(LONG, &style, &capped(2, ElideMode::None));
    assert_eq!(l.line_count(), 2);
    assert!(l.truncated);
    assert_eq!(l.content_height, 32.0);
    assert_eq!(l.implicit_height, 32.0);
    // Without ElideRight the last line is as the wrap left it.
    assert_eq!(l.lines[1].width, whole.lines[1].width);

    let l = e.layout(LONG, &style, &capped(2, ElideMode::Right));
    assert_eq!(l.line_count(), 2);
    assert!(l.truncated);
    assert!(l.lines[1].width <= 100.0);
    assert!(l.lines[1].width > whole.lines[1].width - 14.0);
    let last = l.lines[1]
        .runs
        .last()
        .unwrap()
        .glyphs
        .glyphs
        .last()
        .unwrap()
        .id;
    let dots = e.layout("\u{2026}", &style, &LayoutParams::default()).lines[0].runs[0]
        .glyphs
        .glyphs[0]
        .id;
    assert_eq!(last, dots, "the last line ends in an ellipsis");

    let l = e.layout(LONG, &style, &capped(50, ElideMode::Right));
    assert_eq!(l.line_count(), 3);
    assert!(!l.truncated);

    // One line of unwrapped text: nothing to drop.
    let l = e.layout(
        LONG,
        &style,
        &LayoutParams {
            max_lines: 1,
            ..LayoutParams::default()
        },
    );
    assert_eq!(l.line_count(), 1);
    assert!(!l.truncated);
}

#[test]
fn a_cut_line_gets_an_ellipsis_even_if_it_fits() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    let text = "Hello\nHello world, this is a long line of text";
    let l = e.layout(
        text,
        &style,
        &LayoutParams {
            max_lines: 1,
            ..with(100.0, WrapMode::NoWrap, ElideMode::Right)
        },
    );
    assert_eq!(l.line_count(), 1);
    assert!(l.truncated);
    let expected = plain(&mut e, "Hello\u{2026}").natural_width;
    assert_eq!(l.content_width, expected);
}

#[test]
fn elide_right_cuts_wrapped_text_at_the_height() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    let tall = |height, elide| LayoutParams {
        height: Some(height),
        ..with(100.0, WrapMode::WordWrap, elide)
    };
    let l = e.layout(LONG, &style, &tall(20.0, ElideMode::Right));
    assert_eq!(l.line_count(), 1);
    assert!(l.truncated);
    assert_eq!(l.content_height, 16.0);
    assert_eq!(l.implicit_height, 48.0, "the wish is still every line");
    assert!(l.content_width <= 100.0);

    let l = e.layout(LONG, &style, &tall(32.0, ElideMode::Right));
    assert_eq!(l.line_count(), 2);

    // Too low for even one line: one line shows anyway.
    let l = e.layout(LONG, &style, &tall(4.0, ElideMode::Right));
    assert_eq!(l.line_count(), 1);
    assert!(l.truncated);

    // The height means nothing without ElideRight.
    for mode in [ElideMode::None, ElideMode::Left, ElideMode::Middle] {
        let l = e.layout(LONG, &style, &tall(4.0, mode));
        assert_eq!(l.line_count(), 3, "{mode:?}");
        assert!(!l.truncated, "{mode:?}");
    }
}

#[test]
fn line_height_moves_lines_not_the_baseline() {
    let mut e = TextEngine::new();
    let style = TextStyle::default();
    let lh = |text: &str, line_height| {
        let p = LayoutParams {
            line_height,
            ..LayoutParams::default()
        };
        TextEngine::layout(&mut TextEngine::new(), text, &style, &p)
    };
    let p = LineHeight::Proportional;
    let f = LineHeight::Fixed;
    // (lines, mode) -> height, all read off Qt with this font.
    assert_eq!(lh("a", p(0.5)).content_height, 16.0);
    assert_eq!(lh("a\nb", p(0.5)).content_height, 24.0);
    assert_eq!(lh("a\nb\nc", p(0.5)).content_height, 32.0);
    assert_eq!(lh("a", p(0.9)).content_height, 15.4);
    assert_eq!(lh("a\nb\nc", p(0.9)).content_height, 44.2);
    assert_eq!(lh("a\nb\nc", p(1.3)).content_height, 62.400000000000006);
    assert_eq!(lh("a", f(7.0)).content_height, 16.0);
    assert_eq!(lh("a\nb\nc", f(7.0)).content_height, 30.0);
    assert_eq!(lh("a", f(30.0)).content_height, 30.0);
    assert_eq!(lh("a\nb", f(30.0)).content_height, 60.0);
    assert_eq!(lh("", f(30.0)).content_height, 30.0);

    let l = lh("a\nb", p(1.5));
    assert_eq!(l.baseline, 12.671875);
    assert_eq!(l.lines[1].top, 24.0);
    assert_eq!(l.lines[1].baseline, 24.0 + 12.671875);
    // A nonsense line height leaves lines alone.
    assert_eq!(lh("a\nb", p(f64::NAN)).content_height, 32.0);
    assert_eq!(lh("a\nb", f(-3.0)).content_height, 32.0);
    let _ = &mut e;
}

#[test]
fn weight_and_spacing_change_the_width() {
    let mut e = TextEngine::new();
    let p = LayoutParams::default();
    let regular = e.layout("Hello world", &TextStyle::default(), &p);

    let bold = TextStyle {
        weight: 700.0,
        ..TextStyle::default()
    };
    let l = e.layout("Hello world", &bold, &p);
    assert!(same(l.natural_width, 75.34375), "{}", l.natural_width);
    // A real bold face, not a thickened regular one.
    assert!(!l.lines[0].runs[0].glyphs.embolden);

    let spaced = TextStyle {
        letter_spacing: 2.0,
        ..TextStyle::default()
    };
    let l = e.layout("Hello world", &spaced, &p);
    assert!(
        (l.natural_width - regular.natural_width - 22.0).abs() < 0.02,
        "{}",
        l.natural_width - regular.natural_width
    );

    let words = TextStyle {
        word_spacing: 5.0,
        ..TextStyle::default()
    };
    let l = e.layout("Hello world", &words, &p);
    assert!(
        (l.natural_width - regular.natural_width - 5.0).abs() < 0.02,
        "{}",
        l.natural_width - regular.natural_width
    );
}

#[test]
fn italic_is_slanted_when_the_family_has_no_italic() {
    let mut e = TextEngine::new();
    let italic = TextStyle {
        italic: true,
        ..TextStyle::default()
    };
    let l = e.layout("Hello", &italic, &LayoutParams::default());
    let run = &l.lines[0].runs[0].glyphs;
    assert!(run.skew > 0.1 && run.skew < 0.5, "{}", run.skew);
    let upright = plain(&mut e, "Hello");
    assert_eq!(upright.lines[0].runs[0].glyphs.skew, 0.0);
    assert_eq!(l.content_height, upright.content_height);
}

#[test]
fn underline_and_strikeout_are_bars_and_cost_no_space() {
    let mut e = TextEngine::new();
    let p = LayoutParams::default();
    let regular = e.layout("Hello world", &TextStyle::default(), &p);
    assert!(regular.lines[0].runs[0].decorations.underline.is_none());
    assert!(
        regular.lines[0].runs[0]
            .decorations
            .strike_through
            .is_none()
    );

    let style = TextStyle {
        underline: true,
        strikeout: true,
        ..TextStyle::default()
    };
    let l = e.layout("Hello world", &style, &p);
    assert_eq!(l.natural_width, regular.natural_width);
    assert_eq!(l.content_height, regular.content_height);
    let d = l.lines[0].runs[0].decorations;
    let under = d.underline.expect("underline");
    let strike = d.strike_through.expect("strike-through");
    assert!(
        under.y0 > l.baseline && under.y0 < l.baseline + 2.0,
        "{under:?}"
    );
    assert!(
        strike.y0 < l.baseline - 2.0 && strike.y0 > 0.0,
        "{strike:?}"
    );
    assert!((under.height() - 1.03125).abs() < 0.02);
    assert!((under.width() - l.natural_width).abs() < 0.1);
    assert_eq!(under.x0, 0.0);
}

#[test]
fn an_unknown_family_falls_back_to_the_default() {
    let mut e = TextEngine::new();
    let p = LayoutParams::default();
    let regular = e.layout("Hello world", &TextStyle::default(), &p);
    let odd = TextStyle {
        family: "No Such Family 12345".into(),
        ..TextStyle::default()
    };
    assert_eq!(
        e.layout("Hello world", &odd, &p).natural_width,
        regular.natural_width
    );
    assert!(e.has_family("Liberation Sans"));
    assert!(e.has_family(ljos_text::BUNDLED_FAMILY));
    assert!(!e.has_family("No Such Family 12345"));

    let named = TextStyle {
        family: "\"No Such Family\", Liberation Sans".into(),
        ..TextStyle::default()
    };
    assert_eq!(
        e.layout("Hello world", &named, &p).natural_width,
        regular.natural_width
    );
}

#[test]
fn nonsense_sizes_do_not_panic() {
    let mut e = TextEngine::new();
    let p = LayoutParams::default();
    for size in [0.0, -4.0, f32::NAN, f32::INFINITY, 1.0e9] {
        let l = e.layout("Hello", &TextStyle::sized(size), &p);
        assert!(l.content_height.is_finite(), "{size}");
        assert!(l.natural_width.is_finite(), "{size}");
    }
    let l = e.layout("Hello", &TextStyle::sized(0.0), &p);
    assert_eq!(l.natural_width, 0.0);
}
