//! A visual native item written against the facade alone: it paints
//! itself from its own properties, takes keys while it has the focus, and
//! takes the wheel. Only the host's part (the scene that frames it and
//! hands it events, which the shell is in an application) reaches below
//! the facade.

use std::rc::Rc;

use ljos::item::{
    self, DisplayListBuilder, DrawOp, FocusReason, HighlightSpan, Highlighter, ItemPaint, KeyEvent,
    KeyItem, KeyKind, LayoutParams, Rgba, TextEngine, TextStyle, WheelEvent, WheelItem, key, kurbo,
};
use ljos::prelude::*;
use ljos_quick::QuickScene;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Gauge", extends = Item)]
pub struct Gauge {
    #[qml(base)]
    pub item: Item,
    /// How full, 0 to 1.
    #[qml(notify)]
    pub value: f64,
    #[qml(notify)]
    pub label: String,
}

/// A bar as long as the value, and the label laid out over it.
#[derive(Debug)]
struct Bar {
    value: f64,
    label: String,
}

impl ItemPaint for Bar {
    fn paint(&self, size: kurbo::Size, text: &mut TextEngine, out: &mut DisplayListBuilder) {
        let filled = kurbo::Rect::new(0.0, 0.0, size.width * self.value, size.height);
        out.fill_rect(filled, Rgba::BLACK);
        let layout = text.layout(
            &self.label,
            &TextStyle::sized(12.0),
            &LayoutParams::default(),
        );
        for line in &layout.lines {
            for run in &line.runs {
                out.glyphs(
                    std::sync::Arc::clone(&run.glyphs),
                    kurbo::Vec2::ZERO,
                    Rgba::WHITE,
                    item::Decorations::default(),
                );
            }
        }
    }
}

fn read(engine: &Engine, gauge: ObjectId) -> Rc<dyn ItemPaint> {
    Rc::new(Bar {
        value: item::prop(engine, gauge, Gauge::VALUE),
        label: item::prop(engine, gauge, Gauge::LABEL),
    })
}

fn step(engine: &Engine, gauge: ObjectId, by: f64) {
    let now = item::prop(engine, gauge, Gauge::VALUE);
    engine
        .set(gauge, Gauge::VALUE, (now + by).clamp(0.0, 1.0))
        .unwrap();
}

fn press(engine: &Engine, gauge: ObjectId, ev: &KeyEvent) -> bool {
    match ev.key {
        key::UP => step(engine, gauge, 0.25),
        key::DOWN => step(engine, gauge, -0.25),
        _ => return false,
    }
    true
}

fn wheel(engine: &Engine, gauge: ObjectId, ev: &WheelEvent, _at: kurbo::Point) -> bool {
    step(engine, gauge, ev.angle_delta.y / WheelEvent::NOTCH / 4.0);
    true
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Gauge]));
    item::register_paint_item::<Gauge>(engine, read);
    item::register_key_item::<Gauge>(
        engine,
        KeyItem {
            shortcut_override: |_, _, _| false,
            press,
            release: |_, _, _| false,
        },
    );
    item::register_wheel_item::<Gauge>(engine, WheelItem { wheel });
}

/// The width of the bar the gauge painted last, and how many glyph runs.
fn painted(scene: &QuickScene, gauge: ObjectId) -> (f64, usize) {
    let node = scene
        .render_tree()
        .node(scene.node_of(gauge).unwrap())
        .unwrap();
    let ops = node.content().unwrap().ops();
    let width = ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Rect { rect, .. } => Some(rect.width()),
            _ => None,
        })
        .unwrap();
    let glyphs = ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Glyphs { .. }))
        .count();
    (width, glyphs)
}

fn gauge() -> (Engine, ObjectId, QuickScene) {
    let engine = ljos::engine();
    register(&engine);
    let gauge = engine.create_by_name("Example", "Gauge", None).unwrap();
    engine
        .write_by_name(gauge, "width", Value::from(100.0))
        .unwrap();
    engine
        .write_by_name(gauge, "height", Value::from(20.0))
        .unwrap();
    engine.set(gauge, Gauge::VALUE, 0.5).unwrap();
    engine.set(gauge, Gauge::LABEL, "half".to_owned()).unwrap();
    let mut scene = QuickScene::new(kurbo::Size::new(200.0, 100.0));
    scene.set_root(&engine, gauge).unwrap();
    scene.frame(&engine, Time::ZERO);
    (engine, gauge, scene)
}

#[test]
fn a_painted_item_paints_what_it_read_and_repaints_when_it_changes() {
    let (engine, gauge, mut scene) = gauge();
    assert_eq!(painted(&scene, gauge), (50.0, 1));
    engine.set(gauge, Gauge::VALUE, 1.0).unwrap();
    scene.frame(&engine, Time::ZERO);
    assert_eq!(painted(&scene, gauge).0, 100.0);
}

#[test]
fn a_painted_item_takes_keys_once_it_has_the_focus() {
    let (engine, gauge, mut scene) = gauge();
    let up = KeyEvent::press(key::UP);
    assert!(!scene.key_event(&engine, &up));
    item::force_active_focus(&engine, gauge, FocusReason::Mouse);
    assert!(scene.key_event(&engine, &up));
    assert_eq!(engine.get(gauge, Gauge::VALUE), Ok(0.75));
    assert_eq!(up.kind, KeyKind::Press);
    // A key the item does not take goes on.
    assert!(!scene.key_event(&engine, &KeyEvent::press(key::LEFT)));
    scene.frame(&engine, Time::ZERO);
    assert_eq!(painted(&scene, gauge).0, 75.0);
}

#[test]
fn a_painted_item_takes_the_wheel() {
    let (engine, gauge, mut scene) = gauge();
    let turned = WheelEvent::notches(10.0, 10.0, 0.0, -1.0, Time::ZERO);
    assert!(scene.wheel_event(&engine, &turned));
    assert_eq!(engine.get(gauge, Gauge::VALUE), Ok(0.25));
}

#[test]
fn a_text_edit_takes_a_highlighter_and_a_gauge_does_not() {
    let (engine, gauge, _scene) = gauge();
    let edit = engine.create_by_name("QtQuick", "TextEdit", None).unwrap();
    let words: Highlighter = Box::new(|text: &str| {
        vec![HighlightSpan {
            range: 0..text.len().min(4),
            underline: true,
            ..HighlightSpan::default()
        }]
    });
    assert!(item::set_highlighter(&engine, edit, Some(words)));
    assert!(!item::set_highlighter(&engine, gauge, None));
}
