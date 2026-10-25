//! Touch points, one finger or two: [`crate::QuickScene::touch_event`].
//!
//! One finger is a mouse to what is under it, as in Qt. A second finger
//! makes a pinch: of the `PinchArea` under the first, or else of the
//! `PinchHandler` under the two once a finger has gone the drag threshold
//! on an axis; either way what the first finger pressed is cancelled
//! when the pinch takes over. More fingers are not followed.

use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo;
use ljos_runtime::Time;

use crate::handlers::{PinchGesture, PinchPhase};
use crate::input::{MouseArea, MouseButtons, PointerEvent, PointerId, PointerKind, PointerSource};
use crate::item::is_a;
use crate::pinch_area::{self, Fingers};
use crate::scene::{DRAG_THRESHOLD, QuickScene};

/// What a touch point did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Begin,
    Move,
    End,
    /// The platform took the point away.
    Cancel,
}

/// One touch point's event, in scene (logical pixel) coordinates. `id`
/// is the point's for as long as it is down, and is the pointer id of the
/// mouse it makes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEvent {
    pub id: PointerId,
    pub phase: TouchPhase,
    pub pos: kurbo::Point,
    pub time: Time,
}

#[derive(Clone, Copy, Debug)]
struct Point {
    id: PointerId,
    press: kurbo::Point,
    now: kurbo::Point,
}

/// Who the fingers are for.
#[derive(Clone, Copy, Debug, Default)]
enum Owner {
    #[default]
    None,
    /// The first finger is a mouse, to `taker` (what took the press, in
    /// `area`). A second finger on `taker` too makes `area` pinch; or two
    /// may make a PinchHandler pinch (`recognise`).
    Pointer {
        area: Option<ObjectId>,
        taker: Option<ObjectId>,
        recognise: bool,
    },
    /// A PinchArea has the fingers.
    Area(ObjectId),
    /// A PinchHandler pinches: the distance, the screen angle and the
    /// centre it was last told.
    Handler {
        distance: f64,
        angle: f64,
        center: kurbo::Point,
    },
}

#[derive(Debug, Default)]
pub(crate) struct Touches {
    points: Vec<Point>,
    owner: Owner,
}

impl Touches {
    fn fingers(&self) -> Option<Fingers> {
        let [a, b] = self.points.as_slice() else {
            return None;
        };
        Some(Fingers {
            press: [a.press, b.press],
            now: [a.now, b.now],
        })
    }
}

/// The angle of the line between the fingers on screen, clockwise.
fn screen_angle(f: &Fingers) -> f64 {
    let d = f.now[1] - f.now[0];
    d.y.atan2(d.x).to_degrees()
}

fn turned(from: f64, to: f64) -> f64 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

fn pointer(kind: PointerKind, ev: &TouchEvent, id: PointerId) -> PointerEvent {
    let held = match kind {
        PointerKind::Press | PointerKind::Move => MouseButtons::LEFT,
        _ => MouseButtons::empty(),
    };
    let mut p = PointerEvent::moved(ev.pos.x, ev.pos.y, held, ev.time);
    p.kind = kind;
    p.id = id;
    p.source = PointerSource::FINGER;
    if matches!(kind, PointerKind::Press | PointerKind::Release) {
        p.button = Some(MouseButtons::LEFT);
    }
    p
}

impl QuickScene {
    /// Delivers what touch points did together, as one platform touch
    /// event tells them: the moves count as one step of a pinch. Returns
    /// whether something took them.
    pub fn touch_event(&mut self, engine: &Engine, events: &[TouchEvent]) -> bool {
        let mut handled = false;
        let mut moved: Option<(TouchEvent, bool)> = None;
        for ev in events {
            if ev.phase != TouchPhase::Move
                && let Some((m, first)) = moved.take()
            {
                handled |= self.touch_moved(engine, &m, first);
            }
            match ev.phase {
                TouchPhase::Begin => handled |= self.touch_begin(engine, ev),
                TouchPhase::Move => {
                    let Some(i) = self.touch.points.iter().position(|p| p.id == ev.id) else {
                        continue;
                    };
                    self.touch.points[i].now = ev.pos;
                    let first = moved.is_some_and(|m| m.1) || i == 0;
                    moved = Some((*ev, first));
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    let others_end = events.iter().any(|o| {
                        o.id != ev.id && matches!(o.phase, TouchPhase::End | TouchPhase::Cancel)
                    });
                    handled |= self.touch_end(engine, ev, others_end);
                }
            }
        }
        if let Some((m, first)) = moved {
            handled |= self.touch_moved(engine, &m, first);
        }
        handled
    }

    /// Whether a finger is down.
    pub fn touching(&self) -> bool {
        !self.touch.points.is_empty()
    }

    fn touch_begin(&mut self, engine: &Engine, ev: &TouchEvent) -> bool {
        let point = Point {
            id: ev.id,
            press: ev.pos,
            now: ev.pos,
        };
        match self.touch.points.len() {
            0 => {
                self.touch.points.push(point);
                let hits = self.hits(engine, ev.pos);
                let area = hits
                    .iter()
                    .copied()
                    .find(|&o| pinch_area::takes_touch(engine, o));
                // A PinchArea above what takes a press keeps the finger
                // from it; one below lets it have the finger, and takes
                // over at the second.
                let above = area.is_some_and(|a| {
                    hits.iter()
                        .take_while(|&&o| o != a)
                        .all(|&o| !takes_press(engine, o))
                });
                if let (true, Some(a)) = (above, area) {
                    self.note_input();
                    self.touch.owner = Owner::Area(a);
                    return true;
                }
                let taker = area.and_then(|a| {
                    hits.iter()
                        .copied()
                        .take_while(|&o| o != a)
                        .find(|&o| takes_press(engine, o))
                });
                self.touch.owner = Owner::Pointer {
                    area,
                    taker,
                    recognise: true,
                };
                self.pointer_event(engine, &pointer(PointerKind::Press, ev, ev.id))
            }
            1 => {
                self.touch.points.push(point);
                match self.touch.owner {
                    // Qt's PinchArea takes the fingers from what is in it
                    // only when both are on it; a second beside it is no
                    // pinch.
                    Owner::Pointer {
                        area: Some(a),
                        taker: Some(t),
                        ..
                    } if engine.is_alive(a) && self.hits(engine, ev.pos).contains(&t) => {
                        self.cancel_pointer(engine, ev);
                        self.touch.owner = Owner::Area(a);
                        self.area_moved(engine, a)
                    }
                    Owner::Area(a) => self.area_moved(engine, a),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The points moved; `first` is whether the first did.
    fn touch_moved(&mut self, engine: &Engine, ev: &TouchEvent, first: bool) -> bool {
        match self.touch.owner {
            Owner::None => false,
            Owner::Area(a) => self.area_moved(engine, a),
            Owner::Pointer { recognise, .. } => {
                if recognise && let Some(f) = self.touch.fingers() {
                    let far = self.touch.points.iter().any(|p| {
                        let d = p.now - p.press;
                        d.x.abs() > DRAG_THRESHOLD || d.y.abs() > DRAG_THRESHOLD
                    });
                    if far {
                        let center = f.now[0].midpoint(f.now[1]);
                        if self.pinch_event(engine, &PinchGesture::new(PinchPhase::Begin, center)) {
                            self.cancel_pointer(engine, ev);
                            self.touch.owner = Owner::Handler {
                                distance: (f.now[1] - f.now[0]).hypot(),
                                angle: screen_angle(&f),
                                center,
                            };
                            return true;
                        }
                        // Nothing pinches here; the mouse goes on.
                        if let Owner::Pointer { recognise, .. } = &mut self.touch.owner {
                            *recognise = false;
                        }
                    }
                }
                let p = self.touch.points[0];
                let ev = TouchEvent {
                    id: p.id,
                    pos: p.now,
                    ..*ev
                };
                first && self.pointer_event(engine, &pointer(PointerKind::Move, &ev, ev.id))
            }
            Owner::Handler {
                distance,
                angle,
                center,
            } => {
                let Some(f) = self.touch.fingers() else {
                    return false;
                };
                let now = f.now[0].midpoint(f.now[1]);
                let d = (f.now[1] - f.now[0]).hypot();
                let a = screen_angle(&f);
                let mut g = PinchGesture::new(PinchPhase::Update, now);
                g.scale = if distance > 0.0 { d / distance } else { 1.0 };
                g.rotation = turned(angle, a);
                g.translation = now - center;
                self.touch.owner = Owner::Handler {
                    distance: d,
                    angle: a,
                    center: now,
                };
                self.pinch_event(engine, &g)
            }
        }
    }

    /// `others_end`: another point ends in the same event.
    fn touch_end(&mut self, engine: &Engine, ev: &TouchEvent, others_end: bool) -> bool {
        let Some(i) = self.touch.points.iter().position(|p| p.id == ev.id) else {
            return false;
        };
        let lifted = self.touch.points.remove(i);
        let canceled = ev.phase == TouchPhase::Cancel;
        let handled = match self.touch.owner {
            Owner::None => false,
            Owner::Area(a) => {
                if self.touch.points.len() == 1 {
                    pinch_area::released(engine, a, !others_end);
                }
                true
            }
            Owner::Handler { center, .. } => {
                let phase = if canceled {
                    PinchPhase::Cancel
                } else {
                    PinchPhase::End
                };
                self.touch.owner = Owner::None;
                self.pinch_event(engine, &PinchGesture::new(phase, center))
            }
            Owner::Pointer { .. } if i == 0 => {
                self.touch.owner = Owner::None;
                let ev = TouchEvent {
                    pos: lifted.now,
                    ..*ev
                };
                if canceled {
                    self.pointer_event(engine, &pointer(PointerKind::Cancel, &ev, ev.id))
                } else {
                    let r = self.pointer_event(engine, &pointer(PointerKind::Release, &ev, ev.id));
                    // A lifted finger does not hover.
                    self.pointer_event(engine, &pointer(PointerKind::Leave, &ev, ev.id));
                    r
                }
            }
            Owner::Pointer { .. } => false,
        };
        if self.touch.points.is_empty() {
            self.touch.owner = Owner::None;
        }
        handled
    }

    fn area_moved(&mut self, engine: &Engine, area: ObjectId) -> bool {
        self.note_input();
        if let Some(f) = self.touch.fingers() {
            pinch_area::moved(engine, area, f);
        }
        true
    }

    /// What the first finger pressed lets it go.
    fn cancel_pointer(&mut self, engine: &Engine, ev: &TouchEvent) {
        let first = self.touch.points[0];
        let ev = TouchEvent {
            id: first.id,
            pos: first.now,
            ..*ev
        };
        self.pointer_event(engine, &pointer(PointerKind::Cancel, &ev, ev.id));
    }
}

/// Whether `obj` takes a press itself.
fn takes_press(engine: &Engine, obj: ObjectId) -> bool {
    (is_a::<MouseArea>(engine, obj) && crate::scene::mouse_area_enabled(engine, obj))
        || crate::pointer_item::pointer_item(engine, obj).is_some()
}
