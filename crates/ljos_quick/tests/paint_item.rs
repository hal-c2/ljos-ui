//! A native item that paints content of its own (`register_paint_item`)
//! lays out and draws text with the text engine the scene lends it.

mod common;

use std::rc::Rc;
use std::sync::Arc;

use common::*;
use ljos_engine::{Engine, MetaRef, ObjectId, QmlObject, QmlType};
use ljos_quick::{Item, ItemPaint, TextEngine, TextStyle, register_paint_item};
use ljos_render::{Decorations, DisplayListBuilder, DrawOp, Rgba, kurbo};
use ljos_text::LayoutParams;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Painted", extends = Item)]
struct Painted {
    #[qml(base)]
    item: Item,
}

/// Draws one line of text.
#[derive(Debug)]
struct Label;

impl ItemPaint for Label {
    fn paint(&self, _size: kurbo::Size, text: &mut TextEngine, out: &mut DisplayListBuilder) {
        let layout = text.layout("Hello", &TextStyle::sized(14.0), &LayoutParams::default());
        for line in &layout.lines {
            for run in &line.runs {
                out.glyphs(
                    Arc::clone(&run.glyphs),
                    kurbo::Vec2::ZERO,
                    Rgba::BLACK,
                    Decorations::default(),
                );
            }
        }
    }
}

fn read(_: &Engine, _: ObjectId) -> Rc<dyn ItemPaint> {
    Rc::new(Label)
}

#[test]
fn a_painted_item_draws_text_with_the_scenes_text_engine() {
    let e = engine();
    e.register_module(ljos_engine::module!("Example" 1.0 => [Painted]));
    register_paint_item(&e, MetaRef::Static(Painted::static_meta()), read);
    let root = make::<Item>(&e, None);
    let p = make::<Painted>(&e, Some(root));
    geom(&e, p, 0.0, 0.0, 100.0, 50.0);
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let node = s.render_tree().node(s.node_of(p).unwrap()).unwrap();
    let glyphs = node
        .content()
        .unwrap()
        .ops()
        .iter()
        .filter(|op| matches!(op, DrawOp::Glyphs { .. }))
        .count();
    assert_eq!(glyphs, 1);
}
