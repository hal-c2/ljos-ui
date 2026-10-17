//! Ranged layouts: text whose parts differ in font, with objects in its
//! lines. What rich text and syntax highlighting are built on.

use ljos_text::{
    BoxAlign, ElideMode, InlineObject, LayoutParams, Ranged, StyleSpan, TextEngine, TextStyle,
};

fn spans(spans: &[StyleSpan]) -> Ranged<'_> {
    Ranged {
        spans,
        ..Ranged::NONE
    }
}

fn objects(objects: &[InlineObject]) -> Ranged<'_> {
    Ranged {
        objects,
        ..Ranged::NONE
    }
}

fn runs(layout: &ljos_text::TextLayout) -> Vec<(u32, usize)> {
    layout.lines[0]
        .runs
        .iter()
        .map(|r| (r.tag, r.glyphs.glyphs.len()))
        .collect()
}

#[test]
fn a_span_in_another_weight_is_a_run_of_its_own_and_as_wide_as_that_weight() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    let mut bold = StyleSpan::of(&base, 0..3, 1);
    bold.weight = 700.0;
    let l = te.layout_ranged("abcabc", &base, &spans(&[bold]), &LayoutParams::default());
    assert_eq!(runs(&l), [(1, 3), (0, 3)]);
    let plain = te.advance_width("abc", &base);
    let heavy = te.advance_width(
        "abc",
        &TextStyle {
            weight: 700.0,
            ..base.clone()
        },
    );
    assert!(heavy > plain);
    assert_eq!(l.natural_width, plain + heavy);
}

#[test]
fn a_span_that_only_tags_splits_the_run_and_changes_no_width() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    let tag = StyleSpan::of(&base, 1..2, 7);
    let l = te.layout_ranged("abc", &base, &spans(&[tag]), &LayoutParams::default());
    assert_eq!(runs(&l), [(0, 1), (7, 1), (0, 1)]);
    assert_eq!(l.natural_width, te.advance_width("abc", &base));
}

#[test]
fn every_character_has_one_place_however_many_runs_draw_the_line() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    // Same font throughout: the shaper sees one run, the line has three.
    let tag = StyleSpan::of(&base, 4..8, 1);
    let l = te.layout_ranged(
        "see this one",
        &base,
        &spans(&[tag]),
        &LayoutParams::default(),
    );
    let line = &l.lines[0];
    assert_eq!(line.runs.len(), 3);
    assert_eq!(line.clusters.len(), 12);
    for (i, pair) in line.clusters.windows(2).enumerate() {
        assert_eq!(pair[0].range.end, pair[1].range.start, "cluster {i}");
        assert_eq!(pair[0].x1, pair[1].x0, "cluster {i}");
    }
    assert_eq!(line.clusters[0].x0, 0.0);
    assert_eq!(line.clusters[11].x1, l.natural_width);
    let before = te.advance_width("see ", &base);
    assert_eq!(line.covered(&(4..8)).len(), 1);
    assert_eq!(line.covered(&(4..8))[0].0, before);
    assert_eq!(line.byte_at(before + 1.0), Some(4));
    assert_eq!(line.byte_at(before - 1.0), Some(3));
    assert_eq!(line.byte_at(-1.0), None);
    assert_eq!(line.byte_at(l.natural_width + 1.0), None);
    assert_eq!(line.gap_at(-5.0), 0);
    assert_eq!(line.gap_at(900.0), 12);
}

#[test]
fn a_line_is_as_tall_as_its_tallest_style_its_line_break_included() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    let small = te.font_metrics(&base).line_height;
    let large = te.font_metrics(&TextStyle {
        weight: 700.0,
        ..TextStyle::sized(40.0)
    });
    // The span starts at the break that ends the first line.
    let mut h1 = StyleSpan::of(&base, 3..8, 2);
    h1.pixel_size = 40.0;
    h1.weight = 700.0;
    let l = te.layout_ranged(
        "abc\nabc\nabc",
        &base,
        &spans(&[h1]),
        &LayoutParams::default(),
    );
    let heights: Vec<f64> = l.lines.iter().map(|l| l.height).collect();
    assert_eq!(heights, [large.line_height, large.line_height, small]);
    assert_eq!(l.content_height, 2.0 * large.line_height + small);
    // An empty line is as tall as the style of its break and no taller.
    let mut h1 = StyleSpan::of(&base, 0..1, 2);
    h1.pixel_size = 40.0;
    let l = te.layout_ranged("\n\nabc", &base, &spans(&[h1]), &LayoutParams::default());
    let heights: Vec<f64> = l.lines.iter().map(|l| l.height).collect();
    assert!(heights[0] > small);
    assert_eq!(heights[1..], [small, small]);
}

#[test]
fn an_object_takes_its_width_in_the_line_and_makes_it_as_tall_as_it_needs() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    let fm = te.font_metrics(&base);
    let xy = te.advance_width("xy", &base);
    let object = |align| InlineObject {
        id: 9,
        index: 1,
        width: 30.0,
        height: 40.0,
        align,
    };
    let l = te.layout_ranged(
        "xy",
        &base,
        &objects(&[object(BoxAlign::Bottom)]),
        &LayoutParams::default(),
    );
    assert_eq!(l.natural_width, xy + 30.0);
    assert_eq!(l.content_height, 40.0);
    // The object's bottom is the line's bottom, so the text moves down.
    assert_eq!(l.baseline, 40.0 - fm.descent);
    let placed = l.lines[0].boxes[0];
    assert_eq!(placed.id, 9);
    assert_eq!((placed.rect.y0, placed.rect.y1), (0.0, 40.0));
    assert_eq!(placed.rect.width(), 30.0);
    // The characters around it keep their places.
    let clusters = &l.lines[0].clusters;
    assert_eq!(clusters.len(), 2);
    assert_eq!(clusters[0].x1, placed.rect.x0);
    assert_eq!(clusters[1].x0, placed.rect.x1);

    // Standing on the baseline it reaches up from there.
    let l = te.layout_ranged(
        "xy",
        &base,
        &objects(&[object(BoxAlign::Baseline)]),
        &LayoutParams::default(),
    );
    assert_eq!(l.baseline, 40.0);
    assert_eq!(l.content_height, (40.0 + fm.descent).ceil());
    assert_eq!(l.lines[0].boxes[0].rect.y1, 40.0);

    // At the top the text stays where it was and the line grows down.
    let l = te.layout_ranged(
        "xy",
        &base,
        &objects(&[object(BoxAlign::Top)]),
        &LayoutParams::default(),
    );
    assert_eq!(l.baseline, fm.ascent);
    assert_eq!(l.content_height, 40.0);
    assert_eq!(l.lines[0].boxes[0].rect.y0, 0.0);
}

#[test]
fn an_object_alone_is_a_line() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    let alone = |align, height| InlineObject {
        id: 0,
        index: 0,
        width: 30.0,
        height,
        align,
    };
    // On the baseline with no text around it, the line is the object.
    let l = te.layout_ranged(
        "",
        &base,
        &objects(&[alone(BoxAlign::Baseline, 20.0)]),
        &LayoutParams::default(),
    );
    assert_eq!((l.natural_width, l.content_height), (30.0, 20.0));
    assert_eq!(l.baseline, 20.0);
    let l = te.layout_ranged(
        "",
        &base,
        &objects(&[alone(BoxAlign::Top, 40.0)]),
        &LayoutParams::default(),
    );
    assert_eq!((l.natural_width, l.content_height), (30.0, 40.0));
}

#[test]
fn spans_outside_the_text_are_ignored() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    let mut wild = StyleSpan::of(&base, 2..900, 1);
    wild.weight = 700.0;
    // Inside a character.
    let cut = StyleSpan::of(&base, 1..2, 2);
    let stray = InlineObject {
        id: 1,
        index: 77,
        width: 10.0,
        height: 10.0,
        align: BoxAlign::Bottom,
    };
    let ranged = Ranged {
        spans: &[wild, cut],
        objects: &[stray],
        strut: false,
    };
    let l = te.layout_ranged("\u{e9}a", &base, &ranged, &LayoutParams::default());
    let plain = te.layout("\u{e9}a", &base, &LayoutParams::default());
    assert_eq!(l.natural_width, plain.natural_width);
    assert!(l.lines[0].boxes.is_empty());
}

#[test]
fn an_elided_line_keeps_its_spans_and_the_places_before_the_cut() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    let mut bold = StyleSpan::of(&base, 0..3, 1);
    bold.weight = 700.0;
    let params = LayoutParams {
        width: Some(60.0),
        elide: ElideMode::Right,
        ..LayoutParams::default()
    };
    let l = te.layout_ranged("abc abc abc abc", &base, &spans(&[bold]), &params);
    assert!(l.truncated);
    assert!(l.content_width <= 60.0);
    assert_eq!(l.lines[0].runs[0].tag, 1);
    assert_eq!(l.lines[0].runs[0].glyphs.glyphs.len(), 3);
    // Three letters and a space fit before the ellipsis.
    assert_eq!(l.lines[0].clusters.len(), 4);
    assert_eq!(l.lines[0].clusters[3].range, 3..4);
}

#[test]
fn the_empty_line_after_a_last_line_break_is_wanted_but_not_shown() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(20.0);
    let line = te.font_metrics(&base).line_height;
    let l = te.layout("abc\n", &base, &LayoutParams::default());
    assert_eq!(l.line_count(), 1);
    assert_eq!((l.content_height, l.implicit_height), (line, 2.0 * line));
    let l = te.layout("abc\n\n", &base, &LayoutParams::default());
    assert_eq!(l.line_count(), 2);
    assert_eq!(
        (l.content_height, l.implicit_height),
        (2.0 * line, 3.0 * line)
    );
    let l = te.layout("abc", &base, &LayoutParams::default());
    assert_eq!((l.content_height, l.implicit_height), (line, line));
    // A line limit that cuts the text leaves nothing to want.
    let params = LayoutParams {
        max_lines: 1,
        ..LayoutParams::default()
    };
    let l = te.layout("abc\nabc\n", &base, &params);
    assert_eq!((l.content_height, l.implicit_height), (line, line));
    // Twice the line height: the wanted line is that far down too.
    let params = LayoutParams {
        line_height: ljos_text::LineHeight::Proportional(2.0),
        ..LayoutParams::default()
    };
    let l = te.layout("abc\n", &base, &params);
    assert_eq!(
        (l.content_height, l.implicit_height),
        (2.0 * line, 4.0 * line)
    );
}

#[test]
fn small_capitals_are_capitals_of_seven_tenths_in_a_line_of_the_font() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(40.0);
    let (drawn, spans) = Ranged::small_caps("Hello wORLD", &base);
    assert_eq!(drawn, "HELLO WORLD");
    // The capitals that were written, and the space, are not in a span.
    assert_eq!(
        spans.iter().map(|s| s.range.clone()).collect::<Vec<_>>(),
        vec![1..5, 6..7]
    );
    assert!(spans.iter().all(|s| s.pixel_size == 28.0));

    let (drawn, spans) = Ranged::small_caps("hello", &base);
    let none = LayoutParams::default();
    let small = Ranged {
        spans: &spans,
        strut: true,
        ..Ranged::NONE
    };
    let l = te.layout_ranged(&drawn, &base, &small, &none);
    let written = te.layout("hello", &base, &none);
    let low = te.layout("HELLO", &TextStyle::sized(28.0), &none);
    assert_eq!(l.natural_width, low.natural_width);
    // As tall as the font and on its baseline, though every glyph is smaller.
    assert_eq!(l.implicit_height, written.implicit_height);
    assert_eq!(l.baseline, written.baseline);
    assert!(low.implicit_height < written.implicit_height);

    // Without the strut the line is as low as its glyphs, as a line of
    // smaller markup is, and the two are not one cache entry.
    let loose = te.layout_ranged(&drawn, &base, &self::spans(&spans), &none);
    assert_eq!(loose.implicit_height, low.implicit_height);

    // A line cut short keeps the strut.
    let cut = LayoutParams {
        width: Some(60.0),
        elide: ElideMode::Right,
        ..LayoutParams::default()
    };
    let l = te.layout_ranged(&drawn, &base, &small, &cut);
    assert!(l.truncated);
    assert_eq!(l.content_height, written.content_height);
}

#[test]
fn a_letter_without_one_capital_stays_in_small_capitals() {
    let base = TextStyle::sized(20.0);
    // "ß" has no single capital; "é" has one, of as many bytes.
    let (drawn, spans) = Ranged::small_caps("stra\u{df}e 1\u{e9}", &base);
    assert_eq!(drawn, "STRA\u{df}E 1\u{c9}");
    assert_eq!(drawn.len(), "stra\u{df}e 1\u{e9}".len());
    assert_eq!(
        spans.iter().map(|s| s.range.clone()).collect::<Vec<_>>(),
        vec![0..4, 6..7, 9..11]
    );
    assert!(spans.iter().all(|s| s.pixel_size == 14.0));
    assert_eq!(Ranged::small_caps("", &base), (String::new(), Vec::new()));
}

#[test]
fn every_run_says_where_a_line_over_it_goes() {
    let mut te = TextEngine::new();
    let base = TextStyle::sized(40.0);
    let tag = StyleSpan::of(&base, 1..2, 7);
    let l = te.layout_ranged("abc", &base, &spans(&[tag]), &LayoutParams::default());
    let line = &l.lines[0];
    let under = te.font_metrics(&base).line_width;
    let mut x = 0.0;
    for run in &line.runs {
        let over = run.overline;
        // Along the top of the line, on a whole pixel row, as thick as an
        // underline of the font and never thinner than a pixel.
        assert_eq!(over.y0, line.top);
        assert_eq!(over.y0, over.y0.round());
        assert!((over.height() - under).abs() < 1.0 / 32.0, "{over:?}");
        assert!(over.height() >= 1.0);
        // From where the run starts to where it ends: the runs of a line
        // leave no gap between their lines.
        assert_eq!(over.x0, x);
        assert!(over.x1 > over.x0);
        x = over.x1;
    }
    assert!((x - l.natural_width).abs() < 0.01, "{x}");

    // A smaller part of the line has its line lower: where its own ascent
    // starts, not at the top of the line.
    let mut small = StyleSpan::of(&base, 1..2, 7);
    small.pixel_size = 20.0;
    let l = te.layout_ranged("abc", &base, &spans(&[small]), &LayoutParams::default());
    let line = &l.lines[0];
    assert_eq!(line.runs[0].overline.y0, line.top);
    assert!(line.runs[1].overline.y0 > line.top + 5.0);
    assert!(line.runs[1].overline.y1 < line.baseline);
}
