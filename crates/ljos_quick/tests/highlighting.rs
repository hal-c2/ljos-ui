//! Highlighting from a program's side: the spans it gives an editor come
//! out as colours on the display list and change nothing else, and a
//! highlighter is asked once for each text (ARCHITECTURE §7.5). What QML
//! sees of it is in `features/text-input/highlighting.feature`.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use ljos_engine::{Color, Engine, ObjectId, Value};
use ljos_quick::text_input::{
    HighlightSpan, accept_text, highlights, set_highlighter, set_highlights,
};
use ljos_quick::{QuickScene, to_rgba};
use ljos_render::{DrawOp, Paint, Rgba, kurbo};

const EDITORS: &str = r##"
import QtQuick
Item {
    width: 300; height: 200
    TextEdit { objectName: "edit"; width: 200; height: 80; text: "let one\nlet two" }
    TextInput {
        objectName: "secret"; y: 100; width: 200; height: 20
        echoMode: TextInput.Password; text: "hunter2"
    }
}
"##;

const RED: Color = Color::from_rgba8(0xff, 0, 0, 0xff);
const GREEN: Color = Color::from_rgba8(0, 0xff, 0, 0xff);

fn ready(e: &Engine) -> (ObjectId, QuickScene) {
    let root = load(e, EDITORS);
    let mut s = scene(e, root, 300.0, 200.0);
    frame(&mut s, e);
    (root, s)
}

fn ops(s: &QuickScene, item: ObjectId) -> Vec<DrawOp> {
    s.node_of(item)
        .and_then(|n| s.render_tree().node(n))
        .and_then(|n| n.content())
        .map(|list| list.ops().to_vec())
        .unwrap_or_default()
}

/// How many glyphs `item` paints in `color`.
fn glyphs(s: &QuickScene, item: ObjectId, color: Color) -> usize {
    ops(s, item)
        .iter()
        .map(|op| match op {
            DrawOp::Glyphs {
                run,
                paint: Paint::Solid(c),
                ..
            } if *c == to_rgba(color) => run.glyphs.len(),
            _ => 0,
        })
        .sum()
}

fn rects(s: &QuickScene, item: ObjectId) -> Vec<(kurbo::Rect, Rgba)> {
    ops(s, item)
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                rect,
                fill: Paint::Solid(c),
                ..
            } => Some((*rect, *c)),
            _ => None,
        })
        .collect()
}

fn colored(range: std::ops::Range<usize>, color: Color) -> HighlightSpan {
    HighlightSpan {
        range,
        color: Some(color),
        ..HighlightSpan::default()
    }
}

#[test]
fn a_span_colours_the_glyphs_of_its_stretch_and_changes_nothing_else() {
    let e = engine();
    let (root, mut s) = ready(&e);
    let edit = find(&e, root, "edit");
    let all = glyphs(&s, edit, Color::BLACK);
    assert!(all >= 14, "{all} glyphs");
    let size = (
        num(&e, edit, "contentWidth"),
        num(&e, edit, "contentHeight"),
    );

    assert!(set_highlights(&e, edit, vec![colored(0..3, RED)]));
    assert!(s.needs_frame(), "new colours are painted");
    frame(&mut s, &e);
    assert_eq!(glyphs(&s, edit, RED), 3);
    assert_eq!(glyphs(&s, edit, Color::BLACK), all - 3);
    assert_eq!(
        size,
        (
            num(&e, edit, "contentWidth"),
            num(&e, edit, "contentHeight")
        )
    );
    assert_eq!(
        get(&e, edit, "text").to_js_string().as_str(),
        "let one\nlet two"
    );
    assert!(!get(&e, edit, "canUndo").to_boolean());
    // Painted once, and then nothing has work to do.
    assert!(!s.needs_frame());
    assert_eq!(e.next_timer_deadline(), None);

    assert!(set_highlights(&e, edit, Vec::new()));
    frame(&mut s, &e);
    assert_eq!(glyphs(&s, edit, RED), 0);
    assert_eq!(glyphs(&s, edit, Color::BLACK), all);
}

#[test]
fn a_selected_glyph_is_in_the_selection_colour_whatever_its_span() {
    let e = engine();
    let (root, mut s) = ready(&e);
    let edit = find(&e, root, "edit");
    set_highlights(&e, edit, vec![colored(0..3, RED)]);
    e.invoke_by_name(edit, "select", &[Value::Int(1), Value::Int(5)])
        .unwrap();
    frame(&mut s, &e);
    let Value::Color(selected) = get(&e, edit, "selectedTextColor") else {
        panic!("a colour");
    };
    assert_eq!(glyphs(&s, edit, selected), 4);
    assert_eq!(glyphs(&s, edit, RED), 1);
}

#[test]
fn a_background_is_behind_the_text_and_an_underline_below_it() {
    let e = engine();
    let (root, mut s) = ready(&e);
    let edit = find(&e, root, "edit");
    set_highlights(
        &e,
        edit,
        vec![HighlightSpan {
            // "let" of the second line.
            range: 8..11,
            color: None,
            background: Some(GREEN),
            underline: true,
        }],
    );
    frame(&mut s, &e);
    let rects = rects(&s, edit);
    let behind: Vec<_> = rects.iter().filter(|(_, c)| *c == to_rgba(GREEN)).collect();
    assert_eq!(behind.len(), 1, "{rects:?}");
    let r = behind[0].0;
    assert_eq!((r.x0, r.y0, r.height()), (0.0, 16.0, 16.0));
    let under: Vec<_> = rects
        .iter()
        .filter(|(r, c)| *c == to_rgba(Color::BLACK) && r.height() == 1.0)
        .collect();
    assert_eq!(under.len(), 1, "{rects:?}");
    assert_eq!((under[0].0.x0, under[0].0.x1), (r.x0, r.x1));
    assert!(under[0].0.y0 > 16.0 + 12.0 && under[0].0.y0 < 32.0);
    // The background comes first, so the text is on it.
    let ops = ops(&s, edit);
    let first_glyphs = ops
        .iter()
        .position(|op| matches!(op, DrawOp::Glyphs { .. }))
        .expect("text");
    let background = ops
        .iter()
        .position(
            |op| matches!(op, DrawOp::Rect { fill: Paint::Solid(c), .. } if *c == to_rgba(GREEN)),
        )
        .expect("the background");
    assert!(background < first_glyphs);
}

#[test]
fn a_highlighter_is_asked_once_for_each_text() {
    let e = engine();
    let (root, mut s) = ready(&e);
    let edit = find(&e, root, "edit");
    let asked = Rc::new(Cell::new(0));
    let count = Rc::clone(&asked);
    assert!(set_highlighter(
        &e,
        edit,
        Some(Box::new(move |text: &str| {
            count.set(count.get() + 1);
            text.match_indices("let")
                .map(|(at, word)| colored(at..at + word.len(), RED))
                .collect()
        })),
    ));
    frame(&mut s, &e);
    assert_eq!(asked.get(), 1);
    assert_eq!(glyphs(&s, edit, RED), 6);

    // What changes the picture and not the text asks nothing.
    e.invoke_by_name(edit, "select", &[Value::Int(0), Value::Int(1)])
        .unwrap();
    frame(&mut s, &e);
    e.invoke_by_name(edit, "deselect", &[]).unwrap();
    set(&e, edit, "width", Value::Real(120.0));
    frame(&mut s, &e);
    assert_eq!(asked.get(), 1);

    e.invoke_by_name(edit, "insert", &[Value::Int(0), Value::from("let x\n")])
        .unwrap();
    frame(&mut s, &e);
    assert_eq!(asked.get(), 2);
    assert_eq!(glyphs(&s, edit, RED), 9);
    assert!(accept_text(&e, edit, "y"));
    assert!(accept_text(&e, edit, "z"));
    frame(&mut s, &e);
    // Once for each text, and not again for the picture.
    assert_eq!(asked.get(), 4);
    assert_eq!(
        highlights(&e, edit)
            .iter()
            .map(|s| s.range.clone())
            .collect::<Vec<_>>(),
        // The cursor was after the first letter of what is now the
        // second line, so that "let" is no word any more.
        vec![0..3, 16..19]
    );

    assert!(set_highlighter(&e, edit, None));
    frame(&mut s, &e);
    assert_eq!(asked.get(), 4);
    assert_eq!(glyphs(&s, edit, RED), 0);
    assert!(highlights(&e, edit).is_empty());
}

#[test]
fn the_document_of_a_text_edit_is_a_handle_to_its_highlighting() {
    let e = engine();
    let (root, mut s) = ready(&e);
    let edit = find(&e, root, "edit");
    let Value::Object(doc) = get(&e, edit, "textDocument") else {
        panic!("a TextEdit has a document");
    };
    assert!(set_highlights(&e, doc, vec![colored(4..7, RED)]));
    frame(&mut s, &e);
    assert_eq!(glyphs(&s, edit, RED), 3);
    assert_eq!(highlights(&e, edit), highlights(&e, doc));
    // An item that is no editor takes no spans.
    assert!(!set_highlights(&e, root, vec![colored(0..1, RED)]));
    assert!(highlights(&e, root).is_empty());
}

#[test]
fn spans_that_overlap_or_reach_past_the_text_are_painted_where_they_can_be() {
    let e = engine();
    let (root, mut s) = ready(&e);
    let edit = find(&e, root, "edit");
    let all = glyphs(&s, edit, Color::BLACK);
    set_highlights(
        &e,
        edit,
        vec![
            colored(0..7, RED),
            colored(4..7, GREEN),
            colored(12..4000, GREEN),
            // A stretch that ends before it starts.
            colored(std::ops::Range { start: 9, end: 2 }, RED),
        ],
    );
    frame(&mut s, &e);
    assert_eq!(glyphs(&s, edit, RED), 4);
    // "one" and "two".
    assert_eq!(glyphs(&s, edit, GREEN), 6);
    assert_eq!(glyphs(&s, edit, Color::BLACK), all - 10);
}

#[test]
fn a_field_that_hides_its_text_is_not_painted_in_spans() {
    let e = engine();
    let (root, mut s) = ready(&e);
    let secret = find(&e, root, "secret");
    let masks = glyphs(&s, secret, Color::BLACK);
    assert_eq!(masks, 7);
    // The spans count bytes of the text, and what is shown is not the text.
    assert!(set_highlights(&e, secret, vec![colored(0..3, RED)]));
    frame(&mut s, &e);
    assert_eq!(glyphs(&s, secret, RED), 0);
    assert_eq!(glyphs(&s, secret, Color::BLACK), masks);
}
