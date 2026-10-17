//! The caches: what is reused, and what a frame forgets.

use std::sync::Arc;

use ljos_text::{LayoutParams, TextEngine, TextStyle, WrapMode};

fn wrapped(width: f64) -> LayoutParams {
    LayoutParams {
        width: Some(width),
        wrap: WrapMode::WordWrap,
        ..LayoutParams::default()
    }
}

#[test]
fn the_same_request_returns_the_same_layout() {
    let mut e = TextEngine::new();
    let s = TextStyle::default();
    let a = e.layout("Hello world", &s, &LayoutParams::default());
    let b = e.layout("Hello world", &s, &LayoutParams::default());
    assert!(Arc::ptr_eq(&a, &b));
    let stats = e.stats();
    assert_eq!((stats.layout_hits, stats.layout_misses), (1, 1));
    assert_eq!(stats.shapes_built, 1);
    assert_eq!((stats.layouts, stats.shapes), (1, 1));
}

#[test]
fn a_new_width_breaks_lines_again_without_shaping() {
    let mut e = TextEngine::new();
    let s = TextStyle::default();
    let text = "Hello world, this is a long line of text";
    let narrow = e.layout(text, &s, &wrapped(100.0));
    let wide = e.layout(text, &s, &wrapped(150.0));
    assert!(narrow.line_count() > wide.line_count());
    let stats = e.stats();
    assert_eq!(stats.layout_misses, 2);
    assert_eq!(stats.shapes_built, 1, "shaped once, broken twice");
    // And the first width is still right when asked again.
    assert!(Arc::ptr_eq(&narrow, &e.layout(text, &s, &wrapped(100.0))));
}

#[test]
fn constraints_that_change_nothing_share_an_entry() {
    let mut e = TextEngine::new();
    let s = TextStyle::default();
    let a = e.layout("Hello world", &s, &LayoutParams::default());
    // A width does nothing to text that neither wraps nor elides, and a
    // height does nothing without ElideRight.
    let b = e.layout(
        "Hello world",
        &s,
        &LayoutParams {
            width: Some(321.0),
            height: Some(55.0),
            ..LayoutParams::default()
        },
    );
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(e.stats().layouts, 1);
}

#[test]
fn style_and_text_are_part_of_the_key() {
    let mut e = TextEngine::new();
    let p = LayoutParams::default();
    let a = e.layout("Hello", &TextStyle::default(), &p);
    let b = e.layout("Hello", &TextStyle::sized(20.0), &p);
    let c = e.layout("Hellp", &TextStyle::default(), &p);
    assert!(a.natural_width < b.natural_width);
    assert!(!Arc::ptr_eq(&a, &c));
    assert_eq!(e.stats().layouts, 3);
}

#[test]
fn unused_layouts_are_dropped_after_some_frames() {
    let mut e = TextEngine::new();
    e.set_keep_frames(3);
    let s = TextStyle::default();
    let p = LayoutParams::default();
    e.layout("kept", &s, &p);
    e.layout("dropped", &s, &p);
    for _ in 0..5 {
        e.end_frame();
        e.layout("kept", &s, &p);
    }
    let stats = e.stats();
    // The shaped text went too: a layout that is only read does not need it,
    // and a width change shapes again.
    assert_eq!((stats.layouts, stats.shapes), (1, 0));

    for _ in 0..5 {
        e.end_frame();
    }
    let stats = e.stats();
    assert_eq!((stats.layouts, stats.shapes), (0, 0));
}

#[test]
fn nothing_is_dropped_between_frames() {
    let mut e = TextEngine::new();
    e.set_keep_frames(0);
    let s = TextStyle::default();
    for i in 0..50 {
        e.layout(&format!("line {i}"), &s, &LayoutParams::default());
    }
    assert_eq!(e.stats().layouts, 50);
    e.end_frame();
    assert_eq!(e.stats().layouts, 0);
}

#[test]
fn a_dropped_layout_lives_on_while_someone_holds_it() {
    let mut e = TextEngine::new();
    e.set_keep_frames(1);
    let s = TextStyle::default();
    let held = e.layout("Hello", &s, &LayoutParams::default());
    let run = held.lines[0].runs[0].glyphs.clone();
    drop(held);
    for _ in 0..3 {
        e.end_frame();
    }
    assert_eq!(e.stats().layouts, 0);
    // The display list's handle is the only one left.
    assert_eq!(Arc::strong_count(&run), 1);
    assert_eq!(run.glyphs.len(), 5);
}

#[test]
fn registering_a_font_invalidates_everything() {
    let mut e = TextEngine::new();
    let s = TextStyle::default();
    e.layout("Hello", &s, &LayoutParams::default());
    let before = e.generation();
    assert!(e.register_font(vec![1, 2, 3]).is_err());
    assert_eq!(e.generation(), before, "a bad file changes nothing");
    assert_eq!(e.stats().layouts, 1);

    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fonts/LiberationSans-Bold.ttf"
    ))
    .unwrap();
    let names = e.register_font(bytes).unwrap();
    assert_eq!(names, ["Liberation Sans"]);
    assert_eq!(e.generation(), before + 1);
    assert_eq!(e.stats().layouts, 0);
}

#[test]
fn a_default_family_applies_to_styles_that_name_none() {
    let mut e = TextEngine::new();
    let before = e.generation();
    e.set_default_family("Liberation Sans");
    assert_eq!(e.generation(), before + 1);
    e.set_default_family("Liberation Sans");
    assert_eq!(e.generation(), before + 1, "no change, no invalidation");
    let l = e.layout("Hello", &TextStyle::default(), &LayoutParams::default());
    assert!(l.natural_width > 20.0);
}

#[test]
fn a_glyph_is_read_from_its_font_once() {
    // Rows of a list: new texts, so each is shaped, of the same few glyphs.
    let mut e = TextEngine::new();
    let s = TextStyle::default();
    let first = e.layout("Row 1", &s, &LayoutParams::default());
    let after_first = e.stats().glyphs_measured;
    assert!(after_first > 0);
    let again = e.layout("Row 11", &s, &LayoutParams::default());
    assert_eq!(e.stats().shapes_built, 2);
    assert_eq!(
        e.stats().glyphs_measured,
        after_first,
        "every glyph of the second row was read for the first"
    );
    // And the measurement is the one a fresh read gives.
    let mut fresh = TextEngine::new();
    let alone = fresh.layout("Row 11", &s, &LayoutParams::default());
    assert_eq!(
        (again.advance, again.natural_width),
        (alone.advance, alone.natural_width)
    );
    assert!(first.advance < again.advance);
}
