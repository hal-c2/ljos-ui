//! Rich text on the display list: the pictures of a document's images come
//! from the image cache at the size the layout gave them, nothing a
//! document names is fetched, and a link is in the link colour. What QML
//! sees of it is in `features/text/`.

mod common;

use common::*;
use ljos_engine::{Color, Engine, ObjectId, Value};
use ljos_quick::{ImageStore, QuickScene, to_rgba};
use ljos_render::{DrawOp, Paint, kurbo};

fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let data: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
        w.write_image_data(&data).unwrap();
    }
    out
}

/// An engine that has `red.png` (40×20) beside `Main.qml` and `tall.png`
/// (30×60) compiled in under `qrc:/pictures/`.
fn pictures() -> Engine {
    let e = engine();
    let store = ImageStore::of(&e);
    store.register_resource("file:///red.png", png(40, 20, [255, 0, 0, 255]));
    store.register_resource("qrc:/pictures/tall.png", png(30, 60, [0, 0, 255, 255]));
    e
}

fn ops(s: &QuickScene, item: ObjectId) -> Vec<DrawOp> {
    s.node_of(item)
        .and_then(|n| s.render_tree().node(n))
        .and_then(|n| n.content())
        .map(|list| list.ops().to_vec())
        .unwrap_or_default()
}

/// Where `item` paints pictures, and how large each picture is.
fn images(s: &QuickScene, item: ObjectId) -> Vec<(kurbo::Rect, (u32, u32))> {
    ops(s, item)
        .into_iter()
        .filter_map(|op| match op {
            DrawOp::Image { image, dst, .. } => Some((dst, (image.width(), image.height()))),
            _ => None,
        })
        .collect()
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

fn shown(e: &Engine, source: &str) -> (ObjectId, QuickScene) {
    let root = load(e, source);
    let mut s = scene(e, root, 400.0, 300.0);
    frame(&mut s, e);
    (root, s)
}

fn implicit(e: &Engine, o: ObjectId) -> (f64, f64) {
    (num(e, o, "implicitWidth"), num(e, o, "implicitHeight"))
}

#[test]
fn an_image_is_painted_from_the_cache_at_the_size_of_the_layout() {
    let e = pictures();
    let (root, mut s) = shown(
        &e,
        r#"
import QtQuick
Item {
    Text { objectName: "own"; textFormat: Text.RichText; text: "<img src=\"red.png\">" }
    Text {
        objectName: "sized"; y: 100; textFormat: Text.RichText
        text: "<img src=\"red.png\" width=\"80\">"
    }
}
"#,
    );
    let own = find(&e, root, "own");
    let sized = find(&e, root, "sized");
    assert_eq!(implicit(&e, own), (40.0, 20.0));
    assert_eq!(
        images(&s, own),
        vec![(kurbo::Rect::new(0.0, 0.0, 40.0, 20.0), (40, 20))]
    );
    // One side given: the picture keeps its shape, and is the same picture.
    assert_eq!(implicit(&e, sized), (80.0, 40.0));
    assert_eq!(
        images(&s, sized),
        vec![(kurbo::Rect::new(0.0, 0.0, 80.0, 40.0), (40, 20))]
    );
    // Decoded once for both, and kept where every Image looks.
    assert_eq!(ImageStore::of(&e).cached_urls(), vec!["file:///red.png"]);
    // Nothing is waiting: the pictures were there for the first layout.
    assert!(!s.needs_frame());
    frame(&mut s, &e);
    assert_eq!(e.next_timer_deadline(), None);
}

#[test]
fn a_name_is_looked_for_from_the_base() {
    let e = pictures();
    let (root, mut s) = shown(
        &e,
        r#"
import QtQuick
Text { objectName: "text"; textFormat: Text.RichText; text: "<img src=\"tall.png\">" }
"#,
    );
    let text = find(&e, root, "text");
    // Not beside the file: a gap, and nothing painted in it.
    assert_eq!(implicit(&e, text), (16.0, 16.0));
    assert!(images(&s, text).is_empty());

    set(&e, text, "baseUrl", Value::from("qrc:/pictures/"));
    frame(&mut s, &e);
    assert_eq!(implicit(&e, text), (30.0, 60.0));
    assert_eq!(images(&s, text).len(), 1);

    // A base elsewhere loses it again, and a step back finds it.
    set(&e, text, "baseUrl", Value::from("qrc:/pictures/deeper/"));
    frame(&mut s, &e);
    assert_eq!(implicit(&e, text), (16.0, 16.0));
    assert!(images(&s, text).is_empty());
    set(&e, text, "text", Value::from("<img src=\"../tall.png\">"));
    frame(&mut s, &e);
    assert_eq!(implicit(&e, text), (30.0, 60.0));
}

#[test]
fn a_document_fetches_nothing() {
    let e = pictures();
    let (root, s) = shown(
        &e,
        r#"
import QtQuick
Item {
    Text {
        objectName: "far"; textFormat: Text.RichText
        text: "<img src=\"http://127.0.0.1:9/a.png\"><img src=\"image://provider/a\">"
    }
    Text {
        objectName: "markdown"; y: 100; textFormat: Text.MarkdownText
        text: "![far](https://example.invalid/a.png) ![device](file:///dev/zero)"
    }
}
"#,
    );
    for name in ["far", "markdown"] {
        let text = find(&e, root, name);
        assert!(images(&s, text).is_empty(), "{name}");
        // The two gaps, and in Markdown the space between them.
        assert!(num(&e, text, "implicitWidth") >= 32.0, "{name}");
    }
    assert!(ImageStore::of(&e).cached_urls().is_empty());
    assert_eq!(ImageStore::of(&e).pending(), 0);
}

#[test]
fn an_image_of_styled_text_is_drawn_as_large_as_it_is_in_its_room() {
    let e = pictures();
    let (root, s) = shown(
        &e,
        r#"
import QtQuick
Item {
    Text { objectName: "styled"; textFormat: Text.StyledText; text: "<img src=\"red.png\">" }
    Text { objectName: "space"; y: 100; text: "a b" }
    Text { objectName: "letters"; y: 100; text: "ab" }
}
"#,
    );
    let styled = find(&e, root, "styled");
    let space = num(&e, find(&e, root, "space"), "implicitWidth")
        - num(&e, find(&e, root, "letters"), "implicitWidth");
    assert!(space > 0.0);
    let room = ((40.0 / space).floor() + 1.0) * space;
    assert_eq!(implicit(&e, styled), (space + room, 20.0));
    // The room is wider than the picture; the picture is not stretched.
    assert!(room > 40.0);
    assert_eq!(
        images(&s, styled),
        vec![(kurbo::Rect::new(space, 0.0, space + 40.0, 20.0), (40, 20))]
    );
}

#[test]
fn a_link_is_in_the_link_colour() {
    let e = pictures();
    let (root, mut s) = shown(
        &e,
        r##"
import QtQuick
Text {
    objectName: "text"; color: "#102030"
    text: "see <a href=\"https://example.com\">here</a> now"
}
"##,
    );
    let text = find(&e, root, "text");
    let Value::Color(link) = get(&e, text, "linkColor") else {
        panic!("a colour");
    };
    assert_eq!(link, Color::from_rgba8(0, 0, 0xff, 0xff));
    assert_eq!(glyphs(&s, text, link), 4);
    let own = Color::from_rgba8(0x10, 0x20, 0x30, 0xff);
    let rest = glyphs(&s, text, own);
    assert!(rest >= 6, "{rest}");

    let green = Color::from_rgba8(0, 0x80, 0, 0xff);
    set(&e, text, "linkColor", Value::Color(green));
    assert!(s.needs_frame(), "a new link colour is painted");
    frame(&mut s, &e);
    assert_eq!(glyphs(&s, text, green), 4);
    assert_eq!(glyphs(&s, text, link), 0);
    assert_eq!(glyphs(&s, text, own), rest);
    // The colour changed no size: nothing was laid out for it.
    assert!(!s.needs_frame());
}

/// The rectangles `item` fills with `color`, in the order it paints them.
fn fills(s: &QuickScene, item: ObjectId, color: Color) -> Vec<kurbo::Rect> {
    ops(s, item)
        .into_iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                rect,
                fill: Paint::Solid(c),
                ..
            } if c == to_rgba(color) => Some(rect),
            _ => None,
        })
        .collect()
}

/// The bars under and through the glyphs `item` paints in `color`.
fn bars(s: &QuickScene, item: ObjectId, color: Color) -> usize {
    ops(s, item)
        .iter()
        .map(|op| match op {
            DrawOp::Glyphs {
                paint: Paint::Solid(c),
                decorations,
                ..
            } if *c == to_rgba(color) => decorations.bars().count(),
            _ => 0,
        })
        .sum()
}

const RED: Color = Color::from_rgba8(255, 0, 0, 255);
const GREEN: Color = Color::from_rgba8(0, 255, 0, 255);
const BLUE: Color = Color::from_rgba8(0, 0, 255, 255);

#[test]
fn a_link_of_rich_text_or_markdown_is_blue_whatever_the_link_colour() {
    let e = engine();
    let (root, s) = shown(
        &e,
        r##"
import QtQuick
Item {
    width: 400; height: 300
    component T: Text { linkColor: "#ff00ff"; color: "#102030" }
    T { objectName: "styled"; textFormat: Text.StyledText; text: "<a href=\"x\">here</a>" }
    T { objectName: "rich"; textFormat: Text.RichText; text: "<a href=\"x\">here</a>" }
    T { objectName: "markdown"; textFormat: Text.MarkdownText; text: "[here](x)" }
    T { objectName: "own"; textFormat: Text.RichText; text: "<a href=\"x\" style=\"color: #00ff00\">here</a>" }
}
"##,
    );
    let pink = Color::from_rgba8(255, 0, 255, 255);
    let item = |name| find(&e, root, name);
    assert_eq!(glyphs(&s, item("styled"), pink), 4);
    for name in ["rich", "markdown"] {
        assert_eq!(glyphs(&s, item(name), BLUE), 4, "{name}");
        assert_eq!(glyphs(&s, item(name), pink), 0, "{name}");
        // The underline of a link is its glyphs' own bar.
        assert_eq!(bars(&s, item(name), BLUE), 1, "{name}");
    }
    assert_eq!(glyphs(&s, item("own"), GREEN), 4);
}

#[test]
fn a_style_paints_a_background_behind_its_part_of_the_line() {
    let e = engine();
    let (root, s) = shown(
        &e,
        r##"
import QtQuick
Item {
    width: 400; height: 300
    component R: Text { textFormat: Text.RichText; font.pixelSize: 40 }
    R { objectName: "plain"; text: "ab" }
    R { objectName: "one"; text: "ab<span style=\"background-color: #ff0000\">cd</span>ef" }
    R { objectName: "joined"; text: "<span style=\"background: red\">a<b>b</b><i>c</i></span>" }
    R { objectName: "two"; text: "<span style=\"background-color: #ff0000\">a</span><span style=\"background-color: #00ff00\">b</span>" }
    R { objectName: "small"; text: "a<small style=\"background-color: #ff0000\">b</small>" }
    R { objectName: "none"; text: "<span style=\"background-color: transparent\">a</span><span style=\"background-color: #ff00\">b</span><span style=\"background-color: url(x.png)\">c</span>" }
    R { objectName: "block"; text: "<p style=\"background-color: #ff0000\">a</p>" }
}
"##,
    );
    let item = |name| find(&e, root, name);
    let line = num(&e, item("plain"), "implicitHeight");
    let before = num(&e, item("plain"), "implicitWidth");

    let one = fills(&s, item("one"), RED);
    assert_eq!(one.len(), 1);
    assert_eq!(
        (one[0].y0, one[0].y1),
        (0.0, line),
        "the whole line's height"
    );
    assert!((one[0].x0 - before).abs() < 0.01, "{:?}", one[0]);
    assert!(one[0].x1 > one[0].x0 + 10.0);
    assert!(one[0].x1 < num(&e, item("one"), "implicitWidth") - 10.0);
    // It is painted first, under the text.
    assert!(matches!(ops(&s, item("one"))[0], DrawOp::Rect { .. }));

    // Runs of one background are one rectangle, so no seam shows between
    // them; another colour starts where the first ends.
    let joined = fills(&s, item("joined"), RED);
    assert_eq!(joined.len(), 1);
    assert!((joined[0].width() - num(&e, item("joined"), "implicitWidth")).abs() < 0.01);
    let (a, b) = (fills(&s, item("two"), RED), fills(&s, item("two"), GREEN));
    assert_eq!((a.len(), b.len()), (1, 1));
    assert_eq!(a[0].x1, b[0].x0);

    // Measured on Qt 6.11: smaller text is filled as high as its line.
    let small = fills(&s, item("small"), RED);
    assert_eq!((small[0].y0, small[0].y1), (0.0, line));

    // No colour, no rectangle: nothing transparent is painted, and what is
    // no colour is not looked up anywhere.
    for name in ["none", "block"] {
        let any = ops(&s, item(name))
            .iter()
            .any(|op| matches!(op, DrawOp::Rect { .. }));
        assert!(!any, "{name}");
    }
    // A style costs no room.
    let (w, h) = implicit(&e, item("two"));
    assert_eq!(h, line);
    assert_eq!(w, before);
}

#[test]
fn the_lines_of_a_style_are_in_the_text_colour_or_their_own() {
    let e = engine();
    let (root, s) = shown(
        &e,
        r##"
import QtQuick
Item {
    width: 400; height: 300
    component R: Text { textFormat: Text.RichText; font.pixelSize: 40 }
    R { objectName: "under"; text: "<span style=\"color: #ff0000; text-decoration: underline line-through\">ab</span>" }
    R { objectName: "over"; text: "<span style=\"color: #ff0000; text-decoration: overline\">ab</span>" }
    R { objectName: "other"; text: "<u style=\"color: #ff0000; text-decoration-color: #00ff00\">ab</u>" }
    R { objectName: "all"; text: "<span style=\"text-decoration: underline overline line-through; text-decoration-color: rgb(0, 255, 0)\">ab</span>" }
    R { objectName: "hidden"; text: "<u style=\"text-decoration-color: transparent\">ab</u>" }
    R { objectName: "none"; font.underline: true; text: "ab<span style=\"text-decoration: none\">cd</span>" }
    R { objectName: "link"; text: "<a href=\"x\" style=\"text-decoration: none\">ab</a>" }
}
"##,
    );
    let item = |name| find(&e, root, name);

    // In the colour of the text the lines are the glyphs' own bars, which
    // the painters put on whole pixels.
    assert_eq!(glyphs(&s, item("under"), RED), 2);
    assert_eq!(bars(&s, item("under"), RED), 2);
    assert!(fills(&s, item("under"), RED).is_empty());

    // An overline is a rectangle along the top of the line.
    let over = fills(&s, item("over"), RED);
    assert_eq!(over.len(), 1);
    assert_eq!(over[0].y0, 0.0);
    assert!(over[0].height() >= 1.0 && over[0].height() <= 4.0);
    assert_eq!(over[0].height(), over[0].height().round());
    assert!((over[0].width() - num(&e, item("over"), "implicitWidth")).abs() <= 1.0);

    // In another colour the glyphs carry no bar and the line is painted
    // beside them.
    assert_eq!(glyphs(&s, item("other"), RED), 2);
    assert_eq!(bars(&s, item("other"), RED), 0);
    assert_eq!(fills(&s, item("other"), GREEN).len(), 1);
    let all = fills(&s, item("all"), GREEN);
    assert_eq!(all.len(), 3);
    assert_eq!(bars(&s, item("all"), Color::BLACK), 0);
    for bar in &all {
        assert_eq!(bar.y0, bar.y0.round(), "{bar:?}");
        assert_eq!(bar.height(), bar.height().round(), "{bar:?}");
    }

    // A line nobody can see is not painted.
    assert_eq!(glyphs(&s, item("hidden"), Color::BLACK), 2);
    assert_eq!(bars(&s, item("hidden"), Color::BLACK), 0);
    assert!(fills(&s, item("hidden"), Color::TRANSPARENT).is_empty());

    // `none` takes the font's own underline away, and a link's.
    assert_eq!(glyphs(&s, item("none"), Color::BLACK), 4);
    assert_eq!(bars(&s, item("none"), Color::BLACK), 1);
    assert_eq!(glyphs(&s, item("link"), BLUE), 2);
    assert_eq!(bars(&s, item("link"), BLUE), 0);
}

#[test]
fn small_capitals_are_painted_in_two_sizes() {
    let e = engine();
    let (root, s) = shown(
        &e,
        r##"
import QtQuick
Item {
    width: 400; height: 300
    Text { objectName: "small"; text: "Hello"; font.pixelSize: 40; font.capitalization: Font.SmallCaps }
    Text { objectName: "written"; y: 100; text: "Hello"; font.pixelSize: 40 }
}
"##,
    );
    let small = find(&e, root, "small");
    let written = find(&e, root, "written");
    assert_eq!(glyphs(&s, small, Color::BLACK), 5);
    let sizes = |item| -> Vec<f32> {
        ops(&s, item)
            .iter()
            .filter_map(|op| match op {
                DrawOp::Glyphs { run, .. } => Some(run.font_size),
                _ => None,
            })
            .collect()
    };
    assert_eq!(sizes(small), vec![40.0, 28.0]);
    assert_eq!(sizes(written), vec![40.0]);
    assert_eq!(implicit(&e, small).1, implicit(&e, written).1);
    assert!(implicit(&e, small).0 != implicit(&e, written).0);
    // The text is what was written.
    assert_eq!(get(&e, small, "text").to_js_string().as_str(), "Hello");
}
