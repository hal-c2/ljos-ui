//! Input an application hands a live window, and input it watches an item
//! get (ARCHITECTURE.md goal 2: every part of a running application can be
//! observed and driven).
//!
//! [`post_input`] queues a pointer, key or wheel event for the window and
//! wakes the host that shows it. The scene's next frame, which every host
//! makes when woken, hands it to the scene's entry points for the window
//! system's own input before anything else: it is hit-tested, grabbed, focused
//! and filtered as a user's would be, so whatever covers the point gets it,
//! a web page included. Nothing calls an item's handler directly, and the
//! post is what wakes the host: nothing polls.
//!
//! [`watch_input`] calls back for each event the scene routes into an
//! item, whoever sent it, after the scene has delivered it: watching takes
//! nothing and moves nothing. [`WatchedInput::origin`] tells the user's
//! input from input an application or a driver gave, so that an agent can
//! stop when the user takes over.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo;

use crate::input::{KeyEvent, PointerEvent, PointerKind, WheelEvent};
use crate::scene::QuickScene;

/// One event for a window ([`post_input`]) or seen in an item
/// ([`watch_input`]). Positions are in the window's logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Wheel(WheelEvent),
}

/// Who an event a watcher sees came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputOrigin {
    /// The window system's: the user's own hand.
    #[default]
    User,
    /// Posted by the application ([`post_input`]), or given by a driver
    /// (`ljos_drive`: an agent's inspector, a test).
    Synthetic,
}

/// Why [`post_input`] queued nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PostInputError {
    /// No scene shows the object, or a window it lies in.
    #[error("no window shows {0:?}")]
    NotShown(ObjectId),
}

/// Queues `input` for `window`, as if the user had given it there: the
/// host showing it hands it to its scene in its next pass, which this
/// call asks for. `window` is a `Window` or any item a scene shows; the
/// event's position is in its coordinates. The event's time is the host's
/// when it is delivered, so presses posted close together make a double
/// click as the user's would.
pub fn post_input(engine: &Engine, window: ObjectId, input: Input) -> Result<(), PostInputError> {
    let scenes = engine.extension::<crate::scene::Scenes>();
    if !engine.is_alive(window) || !scenes.post(engine, window, input) {
        return Err(PostInputError::NotShown(window));
    }
    Ok(())
}

/// One event a watcher of `item` sees.
#[derive(Debug)]
pub struct WatchedInput<'a> {
    /// The item watched.
    pub item: ObjectId,
    /// The event, as the scene got it: positions in the scene's logical
    /// pixels.
    pub input: &'a Input,
    pub origin: InputOrigin,
    /// Whether something in the scene took it.
    pub taken: bool,
}

/// What [`watch_input`] returns, to stop with [`unwatch_input`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InputWatch(u64);

type Watcher = Rc<dyn Fn(&Engine, &WatchedInput<'_>)>;

#[derive(Default)]
pub(crate) struct Watchers {
    next: Cell<u64>,
    all: RefCell<Vec<(InputWatch, ObjectId, Watcher)>>,
}

/// Calls `f` for every event the scene routes into `item`, after it was
/// delivered, from the user or not ([`WatchedInput::origin`]):
///
/// - a pointer press whose topmost item under it is `item` or inside it,
///   and the moves, release or cancel of that press;
/// - a pointer move with no button held over `item`, and a wheel event;
/// - a key event (typed text included) while the item with active focus is
///   `item` or inside it.
///
/// Watching changes nothing about where an event goes. The watcher is
/// dropped with the item.
pub fn watch_input(
    engine: &Engine,
    item: ObjectId,
    f: impl Fn(&Engine, &WatchedInput<'_>) + 'static,
) -> InputWatch {
    let w = engine.extension::<Watchers>();
    let id = InputWatch(w.next.get());
    w.next.set(id.0 + 1);
    w.all.borrow_mut().push((id, item, Rc::new(f)));
    id
}

/// Stops a watcher [`watch_input`] started. Whether it was there.
pub fn unwatch_input(engine: &Engine, watch: InputWatch) -> bool {
    let w = engine.extension::<Watchers>();
    let mut all = w.all.borrow_mut();
    let before = all.len();
    all.retain(|(id, _, _)| *id != watch);
    all.len() != before
}

impl Watchers {
    fn is_empty(&self) -> bool {
        self.all.borrow().is_empty()
    }

    /// The watched items `hit` lies in, `hit` itself included.
    fn items_holding(&self, engine: &Engine, hit: ObjectId) -> Vec<ObjectId> {
        let mut all = self.all.borrow_mut();
        all.retain(|(_, item, _)| engine.is_alive(*item));
        let mut found = Vec::new();
        for (_, item, _) in all.iter() {
            if !found.contains(item) && is_in(engine, hit, *item) {
                found.push(*item);
            }
        }
        found
    }

    /// Calls the watchers of `items`, none of them borrowed: a watcher may
    /// watch, unwatch or post.
    fn tell(
        &self,
        engine: &Engine,
        items: &[ObjectId],
        input: &Input,
        origin: InputOrigin,
        taken: bool,
    ) {
        // A held press keeps the items it began in, which may have gone
        // since: their watchers went with them.
        let called: Vec<(ObjectId, Watcher)> = {
            let mut all = self.all.borrow_mut();
            all.retain(|(_, item, _)| engine.is_alive(*item));
            all.iter()
                .filter(|(_, item, _)| items.contains(item))
                .map(|(_, item, f)| (*item, Rc::clone(f)))
                .collect()
        };
        for (item, f) in called {
            f(
                engine,
                &WatchedInput {
                    item,
                    input,
                    origin,
                    taken,
                },
            );
        }
    }
}

/// Whether `obj` is `ancestor` or lies inside it. An item in a layer (a
/// popup's) lies inside the item the layer hangs from, though the layer
/// is no child of it.
pub(crate) fn is_in(engine: &Engine, obj: ObjectId, ancestor: ObjectId) -> bool {
    let mut cur = Some(obj);
    // Bounded so a corrupt parent cycle cannot hang.
    for _ in 0..4096 {
        match cur {
            Some(o) if o == ancestor => return true,
            Some(o) => {
                cur = engine.untrack(|| {
                    engine
                        .visual_parent(o)
                        .or_else(|| crate::layer::layer_host(engine, o))
                });
            }
            None => return false,
        }
    }
    false
}

/// What a scene keeps for its watchers: who sends what it is handed now,
/// and the watched items its held press began in.
#[derive(Debug, Default)]
pub(crate) struct SceneWatch {
    pub(crate) origin: InputOrigin,
    /// While a press is held, the topmost item it came to, if any: whose
    /// watchers, the ones there now, hear the rest of it.
    held: Option<Option<ObjectId>>,
}

impl SceneWatch {
    pub(crate) fn forget_press(&mut self) {
        self.held = None;
    }
}

impl QuickScene {
    /// Hands the scene the input posted to the windows it shows
    /// ([`post_input`]), at `now` on the host's clock, through the entry
    /// points the host's own input takes. Whether anything took any.
    pub(crate) fn deliver_posted_input(
        &mut self,
        engine: &Engine,
        now: ljos_runtime::Time,
    ) -> bool {
        // Before the first picture nothing is found under a point: what
        // was posted waits for the frame after it, which it asks for.
        if !self.has_picture() {
            return false;
        }
        let posted = self.take_posted();
        if posted.is_empty() {
            return false;
        }
        let Some(root) = self.root() else {
            return false;
        };
        self.with_input_origin(InputOrigin::Synthetic, |scene| {
            let mut taken = false;
            for (window, input) in posted {
                // Posted to a tree the scene has stopped showing since:
                // it is not this scene's to hand on.
                if !engine.is_alive(window) || !is_in(engine, window, root) {
                    continue;
                }
                // A window inside the scene (one host showing them all)
                // is where it is now, not where it was at the post.
                let place = |p: kurbo::Point| {
                    if window == root {
                        p
                    } else {
                        crate::item::map_to_scene(engine, window, p)
                    }
                };
                taken |= match input {
                    Input::Pointer(mut ev) => {
                        ev.pos = place(ev.pos);
                        ev.time = now;
                        scene.pointer_event(engine, &ev)
                    }
                    Input::Key(ev) => scene.key_event(engine, &ev),
                    Input::Wheel(mut ev) => {
                        ev.pos = place(ev.pos);
                        ev.time = now;
                        scene.wheel_event(engine, &ev)
                    }
                };
            }
            taken
        })
    }

    /// Runs `f` with what the scene is handed counted as from `origin`, for
    /// the watchers ([`watch_input`]): a driver that is not the user's hand
    /// says so.
    pub fn with_input_origin<R>(
        &mut self,
        origin: InputOrigin,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let was = std::mem::replace(&mut self.watch.origin, origin);
        let r = f(self);
        self.watch.origin = was;
        r
    }

    /// Before a pointer event is delivered: the watched items it is
    /// routed into. Those holding the item under it or, while a press is
    /// held, the item the press came to.
    pub(crate) fn watched_route(&mut self, engine: &Engine, ev: &PointerEvent) -> Vec<ObjectId> {
        let watchers = engine.extension::<Watchers>();
        if watchers.is_empty() {
            self.watch.held = None;
            return Vec::new();
        }
        let held = self.watch.held.take();
        let had_press = held.is_some();
        let on = match (ev.kind, held) {
            (_, Some(Some(on))) => Some(on),
            (PointerKind::Press, _) => self.hits(engine, ev.pos).first().copied(),
            (PointerKind::Move, None) if ev.buttons.is_empty() => {
                self.hits(engine, ev.pos).first().copied()
            }
            // A press held since before anything watched is the item's
            // that holds the pointer, not the one under it now.
            _ => self.pointer_grab(),
        };
        // The press goes on until its last button is let go or it is
        // cancelled; the pointer leaving the window does not end it.
        let goes_on = match ev.kind {
            PointerKind::Cancel => false,
            PointerKind::Leave => had_press,
            _ => !ev.buttons.is_empty(),
        };
        if goes_on {
            self.watch.held = Some(on);
        }
        on.map_or_else(Vec::new, |o| watchers.items_holding(engine, o))
    }

    /// After a pointer event: the watchers of the items it was routed
    /// into ([`QuickScene::watched_route`]).
    pub(crate) fn watched_pointer(
        &self,
        engine: &Engine,
        ev: &PointerEvent,
        items: &[ObjectId],
        taken: bool,
    ) {
        if !items.is_empty() {
            let input = Input::Pointer(*ev);
            engine
                .extension::<Watchers>()
                .tell(engine, items, &input, self.watch.origin, taken);
        }
    }

    /// After a wheel event, given the items under it, topmost first.
    pub(crate) fn watched_wheel(
        &self,
        engine: &Engine,
        ev: &WheelEvent,
        hits: &[ObjectId],
        taken: bool,
    ) {
        let watchers = engine.extension::<Watchers>();
        if watchers.is_empty() {
            return;
        }
        let Some(&top) = hits.first() else {
            return;
        };
        let items = watchers.items_holding(engine, top);
        if !items.is_empty() {
            let input = Input::Wheel(*ev);
            watchers.tell(engine, &items, &input, self.watch.origin, taken);
        }
    }

    /// Before a key event is delivered: the watched items holding the
    /// item with active focus, which is where it goes.
    pub(crate) fn watched_key_route(&self, engine: &Engine, root: ObjectId) -> Vec<ObjectId> {
        let watchers = engine.extension::<Watchers>();
        if watchers.is_empty() {
            return Vec::new();
        }
        // Focus asked for since the last key counts from now, as delivery
        // settles it, so the item read is the one the key reaches.
        crate::focus::tree::settle(engine);
        crate::focus::tree::active_focus_item(engine, root)
            .map_or_else(Vec::new, |f| watchers.items_holding(engine, f))
    }

    /// After a key event: the watchers of the items it was routed into
    /// ([`QuickScene::watched_key_route`]).
    pub(crate) fn watched_key(
        &self,
        engine: &Engine,
        ev: &KeyEvent,
        items: &[ObjectId],
        taken: bool,
    ) {
        if !items.is_empty() {
            let input = Input::Key(ev.clone());
            engine
                .extension::<Watchers>()
                .tell(engine, items, &input, self.watch.origin, taken);
        }
    }
}
