//! The layer above a window's content: drawn over the root item, asked
//! first by the pointer, and part of the same window.

mod common;

use std::cell::Cell;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{
    Item, LayerHooks, MouseArea, PointerEvent, layer_host, layer_of, set_layer, set_layer_hooks,
    window_item, window_of,
};
use ljos_runtime::Time;

thread_local! {
    /// What the hooks of the test were asked.
    static PRESSES: Cell<u32> = const { Cell::new(0) };
    static RELEASES: Cell<u32> = const { Cell::new(0) };
    /// The hook keeps the pointer from everything under the layer.
    static MODAL: Cell<bool> = const { Cell::new(false) };
}

fn hooks() -> LayerHooks {
    LayerHooks {
        filter: |e, layer, hits| {
            if MODAL.get() {
                hits.retain(|h| window_item(e, *h) != *h && e.visual_parent(*h) == Some(layer));
            }
        },
        press: |_, _, _| PRESSES.set(PRESSES.get() + 1),
        release: |_, _, _| RELEASES.set(RELEASES.get() + 1),
    }
}

/// A root with an area over all of it, and a layer with a smaller area.
fn world(e: &Engine) -> (ObjectId, ObjectId, ObjectId, ObjectId) {
    let root = make::<Item>(e, None);
    geom(e, root, 0.0, 0.0, 100.0, 100.0);
    let under = make::<MouseArea>(e, Some(root));
    geom(e, under, 0.0, 0.0, 100.0, 100.0);
    let layer = make::<Item>(e, None);
    geom(e, layer, 0.0, 0.0, 100.0, 100.0);
    let over = make::<MouseArea>(e, Some(layer));
    geom(e, over, 20.0, 20.0, 30.0, 30.0);
    set_layer(e, root, layer);
    (root, under, layer, over)
}

fn press(s: &mut ljos_quick::QuickScene, e: &Engine, x: f64, y: f64) {
    s.pointer_event(e, &PointerEvent::press(x, y, Time::ZERO));
}

fn pressed(e: &Engine, area: ObjectId) -> bool {
    get(e, area, "pressed").to_boolean()
}

#[test]
fn a_layer_belongs_to_the_window_of_its_tree() {
    let e = engine();
    let (root, under, layer, over) = world(&e);
    assert_eq!(layer_of(&e, root), Some(layer));
    assert_eq!(layer_host(&e, layer), Some(root));
    assert_eq!(layer_host(&e, root), None);
    assert_eq!(window_item(&e, over), root);
    assert_eq!(window_item(&e, under), root);
    // No scene shows the tree yet.
    assert_eq!(window_of(&e, over), None);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(window_of(&e, over), Some(root));
}

#[test]
fn the_pointer_asks_the_layer_before_the_content() {
    let e = engine();
    set_layer_hooks(&e, hooks());
    let (root, under, _, over) = world(&e);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);

    press(&mut s, &e, 30.0, 30.0);
    assert!(pressed(&e, over));
    assert!(!pressed(&e, under));
    s.pointer_event(&e, &PointerEvent::release(30.0, 30.0, Time::ZERO));

    // Beside what is in the layer the content is reached as before.
    press(&mut s, &e, 80.0, 80.0);
    assert!(pressed(&e, under));
    s.pointer_event(&e, &PointerEvent::release(80.0, 80.0, Time::ZERO));
    assert_eq!((PRESSES.get(), RELEASES.get()), (2, 2));
}

#[test]
fn the_maker_of_a_layer_can_keep_the_pointer_from_the_content() {
    let e = engine();
    set_layer_hooks(&e, hooks());
    let (root, under, _, over) = world(&e);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    MODAL.set(true);
    press(&mut s, &e, 80.0, 80.0);
    assert!(!pressed(&e, under));
    s.pointer_event(&e, &PointerEvent::release(80.0, 80.0, Time::ZERO));
    press(&mut s, &e, 30.0, 30.0);
    assert!(pressed(&e, over));
    MODAL.set(false);
}

#[test]
fn a_layer_that_was_destroyed_is_no_layer() {
    let e = engine();
    let (root, _, layer, _) = world(&e);
    e.destroy_now(layer);
    assert_eq!(layer_of(&e, root), None);
    let mut s = scene(&e, root, 100.0, 100.0);
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None);
    assert!(!s.needs_frame());
}
