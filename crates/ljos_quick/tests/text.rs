//! `Text` on the text engine: what it paints, where alignment puts it, and
//! what a Text that is not shown keeps (ARCHITECTURE §7.5).
//!
//! Numbers are the bundled font's (Liberation Sans): at 14 px a line is 16 px
//! tall and its baseline 12.671875 px down, as Qt reports for the same font.

mod common;

use std::sync::Arc;

use common::*;
use ljos_engine::{Atom, Engine, ObjectId, TypeTag, Value};
use ljos_quick::enums::{align, elide, wrap_mode};
use ljos_quick::{Item, QuickScene, Text, TextEngine, TextStyle, set_text_engine, text_engine};
use ljos_render::{Decorations, DrawOp, GlyphRun, kurbo};

const BASELINE: f64 = 12.671875;
const LINE: f64 = 16.0;
const HEBREW: &str = "שלום עולם";

fn string(s: &str) -> Value {
    Value::String(Atom::from(s))
}

fn text(e: &Engine, parent: ObjectId, s: &str) -> ObjectId {
    let t = make::<Text>(e, Some(parent));
    at_14_px(e, t);
    set(e, t, "text", string(s));
    t
}

/// Width of `s` on one line, from an engine of its own so that asking does
/// not touch the caches a test is counting.
fn advance(s: &str) -> f64 {
    TextEngine::new().advance_width(s, &TextStyle::default())
}

struct Run {
    origin: kurbo::Vec2,
    run: Arc<GlyphRun>,
    decorations: Decorations,
}

/// The glyph runs in the display list of `item`; `None` when it has none.
fn runs(s: &QuickScene, item: ObjectId) -> Option<Vec<Run>> {
    let node = s.render_tree().node(s.node_of(item)?)?;
    let list = node.content()?;
    Some(
        list.ops()
            .iter()
            .filter_map(|op| match op {
                DrawOp::Glyphs {
                    run,
                    origin,
                    decorations,
                    ..
                } => Some(Run {
                    origin: *origin,
                    run: Arc::clone(run),
                    decorations: *decorations,
                }),
                _ => None,
            })
            .collect(),
    )
}

fn origin(s: &QuickScene, item: ObjectId) -> (f64, f64) {
    let r = runs(s, item).expect("a display list");
    assert_eq!(r.len(), 1, "one run");
    (r[0].origin.x, r[0].origin.y)
}

/// What QML's `font.<member>: v` does.
fn set_font(e: &Engine, t: ObjectId, member: &str, v: Value) {
    let font = get(e, t, "font")
        .with_member(&TypeTag::Font, member, &v)
        .unwrap_or_else(|| panic!("font has no {member}"));
    set(e, t, "font", font);
}

fn layouts() -> usize {
    text_engine().borrow().stats().layouts
}

#[test]
fn every_line_is_a_glyph_run_at_its_baseline() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, "one\ntwo");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let r = runs(&s, t).unwrap();
    assert_eq!(r.len(), 2);
    for (i, line) in r.iter().enumerate() {
        assert_eq!((line.origin.x, line.origin.y), (0.0, 0.0));
        assert_eq!(line.run.glyphs.len(), 3);
        assert_eq!(f64::from(line.run.baseline), BASELINE + LINE * i as f64);
        assert_eq!(line.run.font_size, 14.0);
    }
    assert_eq!(get(&e, t, "lineCount"), Value::Int(2));
    assert_eq!(num(&e, t, "contentHeight"), 2.0 * LINE);
    assert_eq!(num(&e, t, "baselineOffset"), BASELINE);
}

#[test]
fn empty_and_transparent_text_paint_nothing() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let empty = text(&e, root, "");
    let clear = text(&e, root, "ghost");
    set(&e, clear, "color", string("transparent"));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert!(runs(&s, empty).is_none_or(|r| r.is_empty()));
    assert!(runs(&s, clear).is_none_or(|r| r.is_empty()));
    // They are still measured.
    assert_eq!(num(&e, empty, "implicitHeight"), LINE);
    assert_eq!(num(&e, clear, "implicitWidth"), advance("ghost"));
}

#[test]
fn alignment_moves_the_runs_and_the_baseline() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, "Hello");
    set(&e, t, "width", Value::Real(200.0));
    set(&e, t, "height", Value::Real(100.0));
    let w = advance("Hello");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(origin(&s, t), (0.0, 0.0));

    set(&e, t, "horizontalAlignment", Value::Int(align::RIGHT));
    set(&e, t, "verticalAlignment", Value::Int(align::V_CENTER));
    frame(&mut s, &e);
    assert_eq!(origin(&s, t), (200.0 - w, 42.0));
    assert_eq!(num(&e, t, "baselineOffset"), BASELINE + 42.0);

    set(&e, t, "horizontalAlignment", Value::Int(align::H_CENTER));
    set(&e, t, "verticalAlignment", Value::Int(align::BOTTOM));
    frame(&mut s, &e);
    assert_eq!(origin(&s, t), ((200.0 - w) / 2.0, 84.0));
    assert_eq!(num(&e, t, "baselineOffset"), BASELINE + 84.0);

    // The item's size moves aligned text without a new layout of its lines.
    set(&e, t, "height", Value::Real(60.0));
    frame(&mut s, &e);
    assert_eq!(origin(&s, t), ((200.0 - w) / 2.0, 44.0));
    assert_eq!(num(&e, t, "baselineOffset"), BASELINE + 44.0);
    // The content size is the text's, whatever the item's.
    assert_eq!(num(&e, t, "contentWidth"), w);
    assert_eq!(num(&e, t, "contentHeight"), LINE);
}

#[test]
fn right_to_left_text_is_aligned_right_until_an_alignment_is_set() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, HEBREW);
    set(&e, t, "width", Value::Real(200.0));
    let w = advance(HEBREW);
    assert!(w > 20.0, "the bundled font has Hebrew: {w}");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let h = |e: &Engine| get(e, t, "horizontalAlignment");
    assert_eq!(h(&e), Value::Int(align::RIGHT));
    assert_eq!(
        get(&e, t, "effectiveHorizontalAlignment"),
        Value::Int(align::RIGHT)
    );
    assert_eq!(origin(&s, t), (200.0 - w, 0.0));

    // The alignment follows the text.
    set(&e, t, "text", string("Hello"));
    frame(&mut s, &e);
    assert_eq!(h(&e), Value::Int(align::LEFT));
    assert_eq!(origin(&s, t), (0.0, 0.0));

    // An alignment that was set stays.
    set(&e, t, "horizontalAlignment", Value::Int(align::LEFT));
    set(&e, t, "text", string(HEBREW));
    frame(&mut s, &e);
    assert_eq!(h(&e), Value::Int(align::LEFT));
    assert_eq!(
        get(&e, t, "effectiveHorizontalAlignment"),
        Value::Int(align::LEFT)
    );
    assert_eq!(origin(&s, t), (0.0, 0.0));
}

#[test]
fn right_to_left_glyphs_run_from_the_right_edge() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, "אב");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let r = runs(&s, t).unwrap();
    assert_eq!(r.len(), 1);
    let g = &r[0].run.glyphs;
    assert_eq!(g.len(), 2);
    // Glyphs are stored in visual order, so the first letter of the text is
    // the one further right, and the two are different glyphs.
    assert!(g[0].x < g[1].x);
    assert_ne!(g[0].id, g[1].id);
    let alef = TextEngine::new().advance_width("א", &TextStyle::default());
    assert!(
        (f64::from(g[1].x) - (num(&e, t, "implicitWidth") - alef)).abs() < 0.1,
        "alef starts its own advance from the right edge"
    );
}

#[test]
fn elide_and_a_line_limit_truncate() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let long = "Hello world, this is a long line of text";
    let t = text(&e, root, long);
    set(&e, t, "width", Value::Real(60.0));
    set(&e, t, "elide", Value::Int(elide::RIGHT));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(get(&e, t, "truncated"), Value::Bool(true));
    assert_eq!(get(&e, t, "lineCount"), Value::Int(1));
    let cw = num(&e, t, "contentWidth");
    assert!(cw <= 60.0 && cw > 40.0, "{cw}");
    assert_eq!(num(&e, t, "implicitWidth"), advance(long));
    let shown = runs(&s, t).unwrap()[0].run.glyphs.len();
    assert!(shown < long.chars().count(), "{shown} glyphs");

    set(&e, t, "elide", Value::Int(elide::NONE));
    set(&e, t, "wrapMode", Value::Int(wrap_mode::WORD_WRAP));
    frame(&mut s, &e);
    assert_eq!(get(&e, t, "truncated"), Value::Bool(false));
    let all = get(&e, t, "lineCount").to_number();
    assert!(all > 3.0, "{all}");
    assert_eq!(num(&e, t, "height"), all * LINE);

    set(&e, t, "maximumLineCount", Value::Int(2));
    frame(&mut s, &e);
    assert_eq!(get(&e, t, "lineCount"), Value::Int(2));
    assert_eq!(get(&e, t, "truncated"), Value::Bool(true));
    assert_eq!(num(&e, t, "height"), 2.0 * LINE);
    assert_eq!(runs(&s, t).unwrap().len(), 2);
}

#[test]
fn font_properties_reach_the_layout() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, "Hello");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let plain = num(&e, t, "implicitWidth");
    assert!(runs(&s, t).unwrap()[0].decorations.underline.is_none());

    set_font(&e, t, "bold", Value::Bool(true));
    frame(&mut s, &e);
    let bold = num(&e, t, "implicitWidth");
    assert!(bold > plain, "{bold} > {plain}");

    set_font(&e, t, "bold", Value::Bool(false));
    set_font(&e, t, "letterSpacing", Value::Real(2.0));
    frame(&mut s, &e);
    assert!((num(&e, t, "implicitWidth") - (plain + 10.0)).abs() < 0.1);

    set_font(&e, t, "letterSpacing", Value::Real(0.0));
    set_font(&e, t, "pixelSize", Value::Int(32));
    set_font(&e, t, "underline", Value::Bool(true));
    set_font(&e, t, "strikeout", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(num(&e, t, "implicitHeight"), 36.0);
    let r = runs(&s, t).unwrap();
    assert_eq!(r[0].run.font_size, 32.0);
    let under = r[0].decorations.underline.expect("an underline");
    let strike = r[0].decorations.strike_through.expect("a strike-through");
    let baseline = f64::from(r[0].run.baseline);
    assert!(under.y0 > baseline - 1.0 && strike.y1 < baseline);
    assert!((under.width() - num(&e, t, "implicitWidth")).abs() < 1.0);
}

#[test]
fn resizing_a_plain_label_does_not_lay_it_out_again() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, "Hello");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let before = text_engine().borrow().stats();
    assert_eq!(before.layout_misses, 1);
    assert_eq!(before.shapes_built, 1);
    // Neither wrapping, eliding nor aligned: its size changes nothing.
    set(&e, t, "width", Value::Real(250.0));
    set(&e, t, "height", Value::Real(80.0));
    frame(&mut s, &e);
    let after = text_engine().borrow().stats();
    assert_eq!(after.layout_misses, 1);
    assert_eq!(after.shapes_built, 1);
    // A width that wraps is one more line breaking of the same shaped text.
    set(&e, t, "wrapMode", Value::Int(wrap_mode::WORD_WRAP));
    set(&e, t, "width", Value::Real(20.0));
    frame(&mut s, &e);
    let wrapped = text_engine().borrow().stats();
    assert_eq!(wrapped.layout_misses, 2);
    assert_eq!(wrapped.shapes_built, 1);
}

#[test]
fn two_texts_that_say_the_same_share_one_layout() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let a = text(&e, root, "Hello");
    let b = text(&e, root, "Hello");
    set(&e, b, "y", Value::Real(40.0));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(layouts(), 1);
    let (ra, rb) = (runs(&s, a).unwrap(), runs(&s, b).unwrap());
    assert!(Arc::ptr_eq(&ra[0].run, &rb[0].run));
}

/// How many frames the engine keeps a layout nobody asks for.
const KEEP: usize = 30;

#[test]
fn text_off_screen_keeps_its_numbers_and_no_layout() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let long = "A paragraph that is far below the window ".repeat(20);
    let t = text(&e, root, &long);
    set(&e, t, "y", Value::Real(5000.0));
    set(&e, t, "width", Value::Real(200.0));
    set(&e, t, "wrapMode", Value::Int(wrap_mode::WORD_WRAP));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    // Measured, but never painted: no display list, and the only layout is
    // the one in the engine's cache.
    let lines = get(&e, t, "lineCount").to_number();
    assert!(lines > 20.0, "{lines}");
    assert_eq!(num(&e, t, "implicitHeight"), lines * LINE);
    assert_eq!(num(&e, t, "implicitWidth"), advance(&long));
    assert!(runs(&s, t).is_none());
    assert_eq!(layouts(), 1);

    // Frames that run anyway age the cache; nobody asks for this layout.
    for _ in 0..=KEEP {
        frame(&mut s, &e);
    }
    let stats = text_engine().borrow().stats();
    assert_eq!((stats.layouts, stats.shapes), (0, 0), "{stats:?}");
    assert!(runs(&s, t).is_none());
    // The numbers stay, and nothing was laid out to keep them.
    assert_eq!(get(&e, t, "lineCount").to_number(), lines);
    assert_eq!(num(&e, t, "height"), lines * LINE);
    assert_eq!(stats.layout_misses, 1);

    // Scrolled into view it is laid out and painted again.
    set(&e, t, "y", Value::Real(10.0));
    frame(&mut s, &e);
    assert_eq!(runs(&s, t).unwrap().len() as f64, lines);
    assert_eq!(layouts(), 1);
    assert_eq!(text_engine().borrow().stats().layout_misses, 2);
}

#[test]
fn text_that_leaves_the_screen_drops_its_glyph_runs() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, "Hello");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let run = Arc::downgrade(&runs(&s, t).unwrap()[0].run);
    assert!(run.upgrade().is_some());

    set(&e, t, "y", Value::Real(900.0));
    let out = frame(&mut s, &e);
    assert!(out.changed);
    assert!(runs(&s, t).is_none(), "the display list goes at once");
    // The cache still has the layout, so coming straight back is free.
    assert!(run.upgrade().is_some());
    for _ in 0..=KEEP {
        frame(&mut s, &e);
    }
    assert!(run.upgrade().is_none(), "nothing holds the glyphs any more");
    assert_eq!(layouts(), 0);
    assert_eq!(num(&e, t, "implicitWidth"), advance("Hello"));

    set(&e, t, "y", Value::Real(0.0));
    frame(&mut s, &e);
    assert_eq!(runs(&s, t).unwrap().len(), 1);
}

#[test]
fn hidden_text_is_measured_and_not_painted() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, "Hello");
    set(&e, t, "visible", Value::Bool(false));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(num(&e, t, "implicitWidth"), advance("Hello"));
    assert_eq!(num(&e, t, "implicitHeight"), LINE);
    assert!(runs(&s, t).is_none());

    set(&e, t, "visible", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(runs(&s, t).unwrap().len(), 1);
}

#[test]
fn text_clipped_away_is_not_painted() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let window = make::<Item>(&e, Some(root));
    geom(&e, window, 0.0, 0.0, 100.0, 50.0);
    set(&e, window, "clip", Value::Bool(true));
    let inside = text(&e, window, "in");
    let outside = text(&e, window, "out");
    set(&e, outside, "y", Value::Real(60.0));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(runs(&s, inside).unwrap().len(), 1);
    assert!(runs(&s, outside).is_none());
}

#[test]
fn a_scene_with_text_is_idle_after_its_frame() {
    let e = engine();
    let root = make::<Item>(&e, None);
    text(&e, root, "Hello");
    let mut s = scene(&e, root, 300.0, 300.0);
    let first = frame(&mut s, &e);
    assert!(first.changed);
    // Nothing in the text engine asks for another frame: no timer, no wake.
    assert!(first.next_wake.is_none());
    assert!(!s.needs_frame());
    let second = frame(&mut s, &e);
    assert!(!second.changed);
    assert!(second.next_wake.is_none());
}

#[test]
fn the_scene_follows_a_new_text_engine() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = text(&e, root, "Hello");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let old = text_engine();
    let installed = set_text_engine(TextEngine::new());
    assert_eq!(installed.borrow().stats().layout_misses, 0);
    frame(&mut s, &e);
    // The Text was laid out and painted by the new engine.
    assert_eq!(installed.borrow().stats().layout_misses, 1);
    assert_eq!(old.borrow().stats().layout_misses, 1);
    assert_eq!(runs(&s, t).unwrap().len(), 1);
    assert_eq!(num(&e, t, "implicitWidth"), advance("Hello"));
}
