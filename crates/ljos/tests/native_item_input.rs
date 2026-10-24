//! What a native item asks of the facade besides its properties, and the
//! `testing` window it is tried in: the display's pixel ratio, the scene's
//! text engine outside paint, a repaint for what no property holds, and
//! the host's and the system's part a test plays.

use std::cell::Cell;
use std::rc::Rc;

use ljos::item::{self, DisplayListBuilder, Font, ItemPaint, TextEngine, kurbo};
use ljos::prelude::*;
use ljos::testing::{self, ScreenSpec, TestWindow};

thread_local! {
    static PAINTS: Cell<u32> = const { Cell::new(0) };
    static ENGINE_IN_PAINT: Cell<Option<bool>> = const { Cell::new(None) };
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Probe", name = "Probe", extends = Item)]
pub struct Probe {
    #[qml(base)]
    pub item: Item,
}

/// Counts its paints and asks for the text engine while it paints.
#[derive(Debug)]
struct Counted;

impl ItemPaint for Counted {
    fn paint(&self, size: kurbo::Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        PAINTS.with(|p| p.set(p.get() + 1));
        let lent = item::with_text_engine(|_| ()).is_some();
        ENGINE_IN_PAINT.with(|s| s.set(Some(lent)));
        out.fill_rect(size.to_rect(), item::Rgba::BLACK);
    }
}

fn read_probe(engine: &Engine, probe: ObjectId) -> Rc<dyn ItemPaint> {
    let _ = item::device_pixel_ratio(engine, probe);
    Rc::new(Counted)
}

fn probe(engine: &Engine, owner: Option<ObjectId>) -> ObjectId {
    engine.register_module(ljos::module!("Probe" 1.0 => [Probe]));
    item::register_paint_item::<Probe>(engine, read_probe);
    let p = engine.create_by_name("Probe", "Probe", owner).unwrap();
    engine.set(p, Item::WIDTH, 10.0).unwrap();
    engine.set(p, Item::HEIGHT, 10.0).unwrap();
    p
}

fn paints() -> u32 {
    PAINTS.with(Cell::get)
}

fn screens(engine: &Engine) {
    let fine = ScreenSpec {
        name: "fine".into(),
        scale: 2.0,
        ..ScreenSpec::default()
    };
    let coarse = ScreenSpec {
        name: "coarse".into(),
        position: [800.0, 0.0],
        scale: 1.5,
        ..ScreenSpec::default()
    };
    testing::set_screens(engine, &[fine, coarse]);
}

#[test]
fn the_pixel_ratio_follows_the_window_and_makes_no_screen_object() {
    let engine = ljos::engine();
    screens(&engine);
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    let root = engine.create_by_name("QtQuick", "Window", None).unwrap();
    let content = engine
        .read_by_name(root, "contentItem")
        .as_object()
        .unwrap();
    let p = probe(&engine, Some(root));
    engine
        .write_by_name(p, "parent", Value::Object(content))
        .unwrap();
    window.show(root).unwrap();
    assert_eq!(item::device_pixel_ratio(&engine, p), 2.0);
    let before = paints();
    testing::set_window_screen(&engine, root, 1);
    window.settle().unwrap();
    assert_eq!(item::device_pixel_ratio(&engine, p), 1.5);
    assert!(paints() > before, "the item repaints on the other display");
    let screen = MetaRef::Static(ljos_controls::Screen::static_meta());
    assert_eq!(engine.existing_attached_object(p, &screen), None);
}

#[test]
fn an_item_in_no_window_has_the_primary_displays_ratio() {
    let engine = ljos::engine();
    let p = probe(&engine, None);
    assert_eq!(item::device_pixel_ratio(&engine, p), 1.0);
    screens(&engine);
    assert_eq!(item::device_pixel_ratio(&engine, p), 2.0);
}

#[test]
fn repaint_paints_an_item_again_in_the_next_frame() {
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    let p = probe(&engine, None);
    // Before its first paint it costs nothing and does nothing.
    item::repaint(&engine, p);
    window.show(p).unwrap();
    let before = paints();
    window.settle().unwrap();
    assert_eq!(paints(), before, "nothing changed");
    item::repaint(&engine, p);
    window.settle().unwrap();
    assert_eq!(paints(), before + 1);
    assert!(window.painted(p).is_some());
}

#[test]
fn the_text_engine_outside_paint_is_the_scenes() {
    testing::set_text_engine(TextEngine::new());
    let shared = ljos_quick::text_engine();
    let outside = item::with_text_engine(|te| std::ptr::from_mut(te).addr()).unwrap();
    assert_eq!(outside, shared.as_ptr().addr());
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    let p = probe(&engine, None);
    window.show(p).unwrap();
    assert_eq!(
        ENGINE_IN_PAINT.with(Cell::get),
        Some(false),
        "paint has it lent"
    );
}

#[test]
fn a_fonts_text_style_follows_the_system_text_size() {
    let engine = ljos::engine();
    let font = Font {
        pixel_size: 10,
        ..Font::default()
    };
    assert_eq!(item::text_style(&engine, &font).pixel_size, 10.0);
    testing::update_system_prefs(&engine, |p| p.font_scale = 1.5);
    assert_eq!(item::text_style(&engine, &font).pixel_size, 15.0);
    let fixed = Font {
        scale_with_system: false,
        ..font
    };
    assert_eq!(item::text_style(&engine, &fixed).pixel_size, 10.0);
}

#[test]
fn a_test_plays_the_application_state() {
    let engine = ljos::engine();
    let app = engine.singleton::<Application>().unwrap();
    testing::set_application_state(&engine, ApplicationState::Inactive);
    assert_eq!(engine.read_by_name(app, "state"), Value::Int(2));
}

/// A highlighter that sets every `**word**` in bold.
fn bold_stars(text: &str) -> Vec<item::HighlightSpan> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = text[from..].find("**").map(|i| from + i) {
        let Some(end) = text[start + 2..].find("**").map(|i| start + 2 + i + 2) else {
            break;
        };
        out.push(item::HighlightSpan {
            range: start..end,
            weight: Some(700),
            ..item::HighlightSpan::default()
        });
        from = end;
    }
    out
}

#[test]
fn a_test_reads_what_an_editors_highlighter_applied() {
    let engine = ljos::engine();
    let edit = engine.create_by_name("QtQuick", "TextEdit", None).unwrap();
    engine
        .write_by_name(edit, "text", Value::from("**word** rest"))
        .unwrap();
    let ranges = |engine: &Engine| -> Vec<std::ops::Range<usize>> {
        item::highlights(engine, edit)
            .expect("a TextEdit is an editor")
            .iter()
            .map(|s| s.range.clone())
            .collect()
    };
    assert!(ranges(&engine).is_empty());

    // The highlighter installed is the one that paints.
    assert!(item::set_highlighter(
        &engine,
        edit,
        Some(Box::new(bold_stars))
    ));
    let spans = item::highlights(&engine, edit).unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].range, 0..8);
    assert_eq!(spans[0].weight, Some(700));

    // It ran again for the new text.
    engine
        .write_by_name(edit, "text", Value::from("a **b** and **c**"))
        .unwrap();
    assert_eq!(ranges(&engine), vec![2..7, 12..17]);

    // Switched off, nothing is painted.
    assert!(item::set_highlighter(&engine, edit, None));
    assert!(ranges(&engine).is_empty());

    let rect = engine.create_by_name("QtQuick", "Rectangle", None).unwrap();
    assert!(item::highlights(&engine, rect).is_none());
}

#[test]
fn a_test_reads_how_a_rich_editor_sets_its_text() {
    let engine = ljos::engine();
    let edit = engine.create_by_name("QtQuick", "TextEdit", None).unwrap();
    engine
        .write_by_name(edit, "textFormat", Value::Int(1))
        .unwrap();
    engine
        .write_by_name(
            edit,
            "text",
            Value::from("<b>bold</b> <i><a href=\"https://example.com/\">x</a></i>"),
        )
        .unwrap();
    let formats = item::char_formats(&engine, edit, 0..usize::MAX).unwrap();
    let at = |byte: usize| formats.iter().find(|s| s.range.contains(&byte)).unwrap();
    assert_eq!(at(0).font.weight, 700);
    assert!(!at(0).font.italic);
    assert!(at(5).font.italic);
    assert_eq!(at(5).link.as_deref(), Some("https://example.com/"));
}
