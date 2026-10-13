//! `MouseArea.drag`: a MouseArea that moves an item as the pointer drags
//! it, and one that drags from the areas inside it (`filterChildren`).
//!
//! What happens and in which order was read on qml6
//! (`features/input/mouse-area-drag.feature`, `drag-threshold.feature`):
//!
//! - The first move further than the threshold from the press, along an
//!   axis the drag allows, marks where the drag starts; from then on the
//!   area keeps its press from a Flickable around it. The move after that
//!   starts the drag: `drag.active` turns on, then the target moves, then
//!   the area says `positionChanged`.
//! - A smoothed drag moves the target by how far the pointer went since
//!   the mark; one that is not, by how far it went since the press. The
//!   target stays inside the limits.
//! - The release says `released` (no click once the drag started), then
//!   `drag.active` turns off.
//! - An area that filters its children hears the press, moves and release
//!   of the area or control inside it that holds the press, each before
//!   it. When its own drag starts it takes the press and the other is
//!   cancelled.
//!
//! The scene calls in at the press, move, release and cancel of a press a
//! MouseArea holds (`crate::scene`). Nothing here runs between events.

use std::cell::RefCell;
use std::collections::HashMap;

use ljos_engine::{Engine, LogLevel, ObjectId, PropIndex, QmlType, Value, WriteKind};
use ljos_render::kurbo;

use crate::input::{MouseArea, MouseButtons, PointerEvent};
use crate::item::{Item, is_a, is_item, num, scene_transform};
use crate::scene::{self, SCENE_CATEGORY};

/// The properties of the `drag` group.
struct Props {
    target: PropIndex,
    axis: PropIndex,
    minimum_x: PropIndex,
    maximum_x: PropIndex,
    minimum_y: PropIndex,
    maximum_y: PropIndex,
    active: PropIndex,
    filter_children: PropIndex,
    smoothed: PropIndex,
    threshold: PropIndex,
}

fn props() -> Option<Props> {
    let meta = MouseArea::static_meta();
    let p = |name| meta.property_index(name);
    Some(Props {
        target: p("drag.target")?,
        axis: p("drag.axis")?,
        minimum_x: p("drag.minimumX")?,
        maximum_x: p("drag.maximumX")?,
        minimum_y: p("drag.minimumY")?,
        maximum_y: p("drag.maximumY")?,
        active: p("drag.active")?,
        filter_children: p("drag.filterChildren")?,
        smoothed: p("drag.smoothed")?,
        threshold: p("drag.threshold")?,
    })
}

/// One press of one area.
#[derive(Clone, Copy, Debug)]
struct Run {
    /// Where the press was, in the scene.
    press: kurbo::Point,
    /// Where the first move past the threshold was, in the scene.
    mark: Option<kurbo::Point>,
    /// The item that is dragged and where it was when the drag started.
    dragging: Option<(ObjectId, kurbo::Point)>,
}

/// Per engine: the presses areas hold and the areas that filter the one
/// held now.
#[derive(Default)]
struct Runs {
    runs: RefCell<HashMap<ObjectId, Run>>,
    /// Areas that watch the press an area or control inside them holds,
    /// innermost first.
    filters: RefCell<Vec<ObjectId>>,
}

fn runs(engine: &Engine) -> std::rc::Rc<Runs> {
    engine.extension::<Runs>()
}

fn read(engine: &Engine, m: ObjectId, p: PropIndex) -> Value {
    engine.untrack(|| engine.read(m, p))
}

fn target(engine: &Engine, m: ObjectId, p: &Props) -> Option<ObjectId> {
    match read(engine, m, p.target) {
        Value::Object(o) if engine.is_alive(o) && is_item(engine, o) => Some(o),
        _ => None,
    }
}

fn threshold(engine: &Engine, m: ObjectId, p: &Props) -> f64 {
    if crate::item::is_unset(engine, m, p.threshold) {
        crate::system_prefs::start_drag_distance(engine)
    } else {
        read(engine, m, p.threshold).to_number()
    }
}

/// `(x, y)`: which coordinates the drag moves.
fn axes(engine: &Engine, m: ObjectId, p: &Props) -> (bool, bool) {
    let a = read(engine, m, p.axis).to_number() as i32;
    (a & 1 != 0, a & 2 != 0)
}

/// An area pressed and took the press.
pub(crate) fn pressed(engine: &Engine, m: ObjectId, ev: &PointerEvent) {
    runs(engine).runs.borrow_mut().insert(
        m,
        Run {
            press: ev.pos,
            mark: None,
            dragging: None,
        },
    );
}

/// Whether the press of `m` became a drag: it is then no click.
pub(crate) fn dragged(engine: &Engine, m: ObjectId) -> bool {
    runs(engine)
        .runs
        .borrow()
        .get(&m)
        .is_some_and(|r| r.dragging.is_some())
}

/// The pointer that presses `m` moved. Moves the target of a drag that is
/// under way, and starts one when the pointer has gone far enough. Returns
/// whether the drag started with this move.
pub(crate) fn moved(engine: &Engine, m: ObjectId, ev: &PointerEvent) -> bool {
    let Some(p) = props() else { return false };
    let Some(mut run) = runs(engine).runs.borrow().get(&m).copied() else {
        return false;
    };
    let mut started = false;
    if run.dragging.is_none() {
        let Some(t) = target(engine, m, &p) else {
            return false;
        };
        if run.mark.is_none() {
            let (x, y) = axes(engine, m, &p);
            let d = ev.pos - run.press;
            let limit = threshold(engine, m, &p);
            if (x && d.x.abs() > limit) || (y && d.y.abs() > limit) {
                run.mark = Some(ev.pos);
                runs(engine).runs.borrow_mut().insert(m, run);
                // A Flickable around the area no longer takes the press.
                crate::flickable::keep_pointer_grab(engine, m, true);
            }
            return false;
        }
        run.dragging = Some((t, position(engine, t)));
        runs(engine).runs.borrow_mut().insert(m, run);
        set_active(engine, m, &p, true);
        started = true;
    }
    // A handler of `activeChanged` may have let go or dropped the area.
    if !engine.is_alive(m) {
        return started;
    }
    let Some(run) = runs(engine).runs.borrow().get(&m).copied() else {
        return started;
    };
    if let Some((t, start)) = run.dragging {
        place(engine, m, &p, &run, t, start, ev.pos);
    }
    started
}

/// The press of `m` ended or was taken away: its drag ends.
pub(crate) fn released(engine: &Engine, m: ObjectId) {
    let run = runs(engine).runs.borrow_mut().remove(&m);
    crate::flickable::keep_pointer_grab(engine, m, false);
    if run.is_some_and(|r| r.dragging.is_some())
        && engine.is_alive(m)
        && let Some(p) = props()
    {
        set_active(engine, m, &p, false);
    }
}

fn set_active(engine: &Engine, m: ObjectId, p: &Props, on: bool) {
    let _ = engine.write_internal(m, p.active, Value::Bool(on), WriteKind::Imperative);
}

fn position(engine: &Engine, item: ObjectId) -> kurbo::Point {
    engine.untrack(|| {
        kurbo::Point::new(
            num(engine, item, Item::X.index),
            num(engine, item, Item::Y.index),
        )
    })
}

fn local(engine: &Engine, obj: ObjectId, pos: kurbo::Point) -> kurbo::Point {
    engine.untrack(|| scene_transform(engine, obj)).inverse() * pos
}

/// Puts the target where the pointer at `at` (scene) takes it.
fn place(
    engine: &Engine,
    m: ObjectId,
    p: &Props,
    run: &Run,
    target: ObjectId,
    start: kurbo::Point,
    at: kurbo::Point,
) {
    let from = if read(engine, m, p.smoothed).to_boolean() {
        run.mark.unwrap_or(run.press)
    } else {
        run.press
    };
    // The target moves in its parent, which may be scaled or turned.
    let d = match engine.untrack(|| engine.visual_parent(target)) {
        Some(parent) if is_item(engine, parent) => {
            local(engine, parent, at) - local(engine, parent, from)
        }
        _ => at - from,
    };
    let (x, y) = axes(engine, m, p);
    let clamp = |v: f64, lo: PropIndex, hi: PropIndex| {
        let (lo, hi) = (
            read(engine, m, lo).to_number(),
            read(engine, m, hi).to_number(),
        );
        // Limits the wrong way round keep the target at the minimum.
        v.min(hi).max(lo)
    };
    let write = |prop: PropIndex, v: f64| {
        if engine
            .untrack(|| engine.read(target, prop))
            .change_equals(&Value::Real(v))
        {
            return;
        }
        if let Err(err) = engine.write(target, prop, Value::Real(v), WriteKind::Imperative) {
            engine.log(
                LogLevel::Warning,
                SCENE_CATEGORY,
                format!("MouseArea.drag: cannot move the target: {err}"),
            );
        }
    };
    if x {
        write(
            Item::X.index,
            clamp(start.x + d.x, p.minimum_x, p.maximum_x),
        );
    }
    if y {
        write(
            Item::Y.index,
            clamp(start.y + d.y, p.minimum_y, p.maximum_y),
        );
    }
}

// ------------------------------------------------------------ filtering

/// Whether `m` drags from the presses of what is inside it.
fn filters(engine: &Engine, m: ObjectId, p: &Props, ev: &PointerEvent) -> bool {
    let button = ev.button.unwrap_or(MouseButtons::LEFT).bits() as i32;
    is_a::<MouseArea>(engine, m)
        && read(engine, m, p.filter_children).to_boolean()
        && target(engine, m, p).is_some()
        && scene::mouse_area_enabled(engine, m)
        && num(engine, m, MouseArea::ACCEPTED_BUTTONS.index) as i32 & button != 0
        && scene::contains_local(engine, m, local(engine, m, ev.pos))
}

/// `child` is about to take a press. The areas around it that filter
/// their children hear it first, innermost first; an area that rejects the
/// press stops watching.
pub(crate) fn filter_press(engine: &Engine, child: ObjectId, ev: &PointerEvent) {
    filter_undo(engine);
    let Some(p) = props() else { return };
    let mut found = Vec::new();
    let mut cur = engine.untrack(|| engine.visual_parent(child));
    for _ in 0..4096 {
        let Some(o) = cur.filter(|o| engine.is_alive(*o)) else {
            break;
        };
        if filters(engine, o, &p, ev) && press_watcher(engine, o, ev) {
            pressed(engine, o, ev);
            found.push(o);
        }
        cur = engine.untrack(|| engine.visual_parent(o));
    }
    *runs(engine).filters.borrow_mut() = found;
}

/// The child did not take the press after all: the areas that heard it
/// forget it, without a word, as the press goes on to what is below.
pub(crate) fn filter_undo(engine: &Engine) {
    let gone = std::mem::take(&mut *runs(engine).filters.borrow_mut());
    for m in gone {
        released(engine, m);
        if engine.is_alive(m) {
            unpress_watcher(engine, m);
        }
    }
}

/// The pointer moved while an area or control holds the press. The areas
/// that filter it hear the move first. Returns the area whose drag
/// started, which takes the press; the others then stop watching.
pub(crate) fn filter_move(engine: &Engine, ev: &PointerEvent) -> Option<ObjectId> {
    let watching = runs(engine).filters.borrow().clone();
    for m in watching {
        if !engine.is_alive(m) {
            continue;
        }
        let started = moved(engine, m, ev);
        if engine.is_alive(m) {
            watcher_moved(engine, m, ev);
        }
        if started {
            let rest: Vec<ObjectId> = std::mem::take(&mut *runs(engine).filters.borrow_mut())
                .into_iter()
                .filter(|o| *o != m)
                .collect();
            for o in rest {
                released(engine, o);
                if engine.is_alive(o) {
                    cancel_watcher(engine, o);
                }
            }
            return Some(m);
        }
    }
    None
}

/// The press ends: the areas that filter it hear the release first.
pub(crate) fn filter_release(engine: &Engine, ev: &PointerEvent) {
    let watching = std::mem::take(&mut *runs(engine).filters.borrow_mut());
    for m in watching {
        if engine.is_alive(m) {
            let click = !dragged(engine, m);
            watcher_released(engine, m, ev, click);
        }
        released(engine, m);
    }
}

/// The press was taken away: the areas that filter it are cancelled.
pub(crate) fn filter_cancel(engine: &Engine) {
    let watching = std::mem::take(&mut *runs(engine).filters.borrow_mut());
    for m in watching {
        released(engine, m);
        if engine.is_alive(m) {
            cancel_watcher(engine, m);
        }
    }
}

// The areas that filter hear what an area that holds a press hears
// (`crate::scene`), without holding it.

fn press_watcher(engine: &Engine, m: ObjectId, ev: &PointerEvent) -> bool {
    let button = ev.button.unwrap_or(MouseButtons::LEFT);
    let at = scene::local_point(engine, m, ev.pos);
    scene::set(engine, m, MouseArea::PRESSED.index, Value::Bool(true));
    scene::set(
        engine,
        m,
        MouseArea::PRESSED_BUTTONS.index,
        Value::Int(button.bits() as i32),
    );
    scene::set_mouse_pos(engine, m, at);
    if scene::set_contains(engine, m, true) {
        scene::emit(engine, m, "entered", &[]);
    }
    if scene::emit_mouse(engine, m, "pressed", ev, at, false) {
        return true;
    }
    unpress_watcher(engine, m);
    false
}

fn unpress_watcher(engine: &Engine, m: ObjectId) {
    scene::set(engine, m, MouseArea::PRESSED.index, Value::Bool(false));
    scene::set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
    if !scene::hover_enabled(engine, m) && scene::set_contains(engine, m, false) {
        scene::emit(engine, m, "exited", &[]);
    }
}

fn watcher_moved(engine: &Engine, m: ObjectId, ev: &PointerEvent) {
    let at = scene::local_point(engine, m, ev.pos);
    let inside = scene::contains_local(engine, m, at);
    if scene::set_contains(engine, m, inside) {
        scene::emit(engine, m, if inside { "entered" } else { "exited" }, &[]);
    }
    scene::set_mouse_pos(engine, m, at);
    scene::emit_mouse(engine, m, "positionChanged", ev, at, false);
}

fn watcher_released(engine: &Engine, m: ObjectId, ev: &PointerEvent, click: bool) {
    let at = scene::local_point(engine, m, ev.pos);
    let click = click && scene::contains_local(engine, m, at);
    scene::set_mouse_pos(engine, m, at);
    scene::set(engine, m, MouseArea::PRESSED.index, Value::Bool(false));
    scene::set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
    scene::emit_mouse(engine, m, "released", ev, at, click);
    if click && engine.is_alive(m) {
        scene::emit_mouse(engine, m, "clicked", ev, at, true);
    }
    if engine.is_alive(m)
        && !scene::hover_enabled(engine, m)
        && scene::set_contains(engine, m, false)
    {
        scene::emit(engine, m, "exited", &[]);
    }
}

fn cancel_watcher(engine: &Engine, m: ObjectId) {
    scene::set(engine, m, MouseArea::PRESSED.index, Value::Bool(false));
    scene::set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
    if !scene::hover_enabled(engine, m) {
        scene::set_contains(engine, m, false);
    }
    scene::emit(engine, m, "canceled", &[]);
}
