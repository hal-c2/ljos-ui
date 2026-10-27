//! Tests of an application's own types (the non-default `testing`
//! feature, for `[dev-dependencies]`): a window without a platform to
//! show items in and hand them input, on a clock that moves only when the
//! test says, and the parts a test plays of the host and the system.
//!
//! ```
//! use ljos::item::{KeyEvent, key};
//! use ljos::prelude::*;
//! use ljos::testing::TestWindow;
//!
//! let engine = ljos::engine();
//! // Before the root: its timers start on the window's clock.
//! let mut window = TestWindow::new(&engine, 200.0, 100.0);
//! let root = engine.create_by_name("QtQuick", "TextInput", None).unwrap();
//! window.show(root).unwrap();
//! ljos::item::force_active_focus(&engine, root, ljos::item::FocusReason::Mouse);
//! assert!(window.commit("hello"));
//! assert!(window.key(&KeyEvent::press(key::BACKSPACE)));
//! assert_eq!(engine.read_by_name(root, "text"), Value::from("hell"));
//! ```

use std::sync::Arc;

use ljos_drive::{Driver, Stage};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::Item;
use ljos_render::{DisplayList, kurbo};
use ljos_runtime::{Clock as _, Time};

pub use ljos_a11y::{AccessibleNode, Contrast, NodeKey, SystemPrefs};
pub use ljos_controls::ScreenSpec;

use crate::item::{KeyEvent, PointerEvent, TextEngine, WheelEvent};
use crate::prelude::ApplicationState;
pub use crate::scene_error::SceneError;

/// A window without a platform: what it shows is laid out and painted
/// into display lists a test reads ([`painted`](Self::painted)), and
/// input reaches its items as a host hands it on. The Engine runs on the
/// window's virtual clock from [`new`](Self::new) on, which stands still
/// until [`advance`](Self::advance) lets time pass.
pub struct TestWindow {
    stage: Stage,
    serve: Option<Box<dyn FnMut()>>,
}

impl std::fmt::Debug for TestWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestWindow")
            .field("now", &self.now())
            .finish_non_exhaustive()
    }
}

impl TestWindow {
    /// A window `width` by `height` logical pixels that shows nothing yet.
    /// Make it before the root is made, for the root's timers and
    /// animations to start on its clock.
    pub fn new(engine: &Engine, width: f64, height: f64) -> Self {
        Self {
            stage: Stage::new(engine, width, height),
            serve: None,
        }
    }

    /// Shows `root`, an item or a `Window`, and lets the scene come to
    /// rest. `Err` says why it cannot be shown or does not come to rest.
    pub fn show(&mut self, root: ObjectId) -> Result<(), SceneError> {
        let Stage { engine, scene, .. } = &mut self.stage;
        scene.set_root(engine, root)?;
        self.settle()
    }

    /// Makes the window `width` by `height` logical pixels, as a user
    /// dragging its edge does, and lets the scene come to rest on the same
    /// clock: a root `Window` is told its new size as the platform tells
    /// it, and a root item takes it as `width` and `height`, as a host
    /// that sizes its root to the view writes it.
    pub fn resize(&mut self, width: f64, height: f64) -> Result<(), SceneError> {
        let Stage { engine, scene, .. } = &mut self.stage;
        scene.set_viewport(kurbo::Rect::new(0.0, 0.0, width, height));
        if let Some(root) = scene.root() {
            let windows = ljos_controls::WindowSystem::of(engine);
            if windows.spec(engine, root).is_some() {
                windows.resized(engine, root, [width, height]);
            } else {
                for (prop, v) in [(Item::WIDTH, width), (Item::HEIGHT, height)] {
                    engine.set(root, prop, v)?;
                }
            }
        }
        self.settle()
    }

    /// The time on the window's clock.
    pub fn now(&self) -> Time {
        self.stage.clock.now()
    }

    /// Frames until the scene is idle, with no time passing: layout,
    /// anchors, paint, deferred deletes, what `Qt.callLater` put off.
    pub fn settle(&mut self) -> Result<(), SceneError> {
        Ok(self.drive().settle()?)
    }

    /// Lets `ms` milliseconds pass, firing each timer at its own deadline
    /// and framing in between, as a host woken at each deadline would.
    ///
    /// Before and after, it waits for what was promised and runs it: the
    /// answers of tasks (`ljos::task::spawn`), fetches and pictures being
    /// decoded, which take real time the virtual clock knows nothing of.
    /// A test that serves what they wait on from this thread (a fake
    /// server) gives that to [`serve_while_waiting`](Self::serve_while_waiting),
    /// or the wait blocks it for up to 10 s without an answer.
    pub fn advance(&mut self, ms: u64) -> Result<(), SceneError> {
        Ok(self.drive().advance(ms)?)
    }

    /// Calls `serve` on this thread every few milliseconds while
    /// [`advance`](Self::advance) waits for outstanding answers, in place
    /// of blocking: a fake server the test runs here answers there.
    pub fn serve_while_waiting(&mut self, serve: impl FnMut() + 'static) {
        self.serve = Some(Box::new(serve));
    }

    fn drive(&mut self) -> Driver<'_> {
        let serve = self.serve.as_deref_mut().map(|s| s as &mut dyn FnMut());
        self.stage.drive().serving(serve)
    }

    /// Settles after input, as a host makes the frames input asks for. A
    /// scene that does not come to rest says so at the next
    /// [`settle`](Self::settle) or [`advance`](Self::advance).
    fn after_input(&mut self, taken: bool) -> bool {
        let _ = self.settle();
        taken
    }

    /// Hands the window a key, then lets the scene come to rest. Whether
    /// an item took it.
    pub fn key(&mut self, ev: &KeyEvent) -> bool {
        let taken = self.drive().send_key(ev);
        self.after_input(taken)
    }

    /// Hands the window a pointer event (in its coordinates), then lets
    /// the scene come to rest. A move with no button held is hover.
    /// Whether an item or handler took it.
    pub fn pointer(&mut self, ev: &PointerEvent) -> bool {
        let taken = self.drive().send_pointer(ev);
        self.after_input(taken)
    }

    /// Hands the window a wheel event, then lets the scene come to rest.
    /// Whether an item used it.
    pub fn wheel(&mut self, ev: &WheelEvent) -> bool {
        let taken = self.drive().send_wheel(ev);
        self.after_input(taken)
    }

    /// Is the input method composing `text` (`cursor` in UTF-16 units of
    /// it) in the item with the active focus; an empty `text` ends the
    /// composition. Whether the item took it.
    pub fn preedit(&mut self, text: &str, cursor: Option<usize>) -> bool {
        let Stage { engine, scene, .. } = &mut self.stage;
        let taken = scene.ime_preedit(engine, text, cursor);
        self.after_input(taken)
    }

    /// Is the input method committing `text` to the item with the active
    /// focus. Whether the item took it.
    pub fn commit(&mut self, text: &str) -> bool {
        let Stage { engine, scene, .. } = &mut self.stage;
        let taken = scene.ime_commit(engine, text);
        self.after_input(taken)
    }

    /// The item an input method talks to: the one with the active focus,
    /// if it is a text input client.
    pub fn input_method_item(&self) -> Option<ObjectId> {
        self.stage
            .scene
            .text_input_target(&self.stage.engine)
            .map(|(o, _)| o)
    }

    /// Where the input method is told the cursor is, in the window's
    /// coordinates: the place its candidate window goes.
    pub fn input_method_cursor(&self) -> Option<kurbo::Rect> {
        let (item, client) = self.stage.scene.text_input_target(&self.stage.engine)?;
        let local = client.cursor_rect(&self.stage.engine, item);
        let to_window = self
            .stage
            .engine
            .untrack(|| ljos_quick::scene_transform(&self.stage.engine, item));
        Some(to_window.transform_rect_bbox(local))
    }

    /// Turns the accessibility tree on, as an assistive technology starting
    /// up does, or off, and lets the scene come to rest. The scene builds
    /// the tree only while it is on.
    pub fn set_accessibility_active(&mut self, active: bool) -> Result<(), SceneError> {
        let Stage { engine, scene, .. } = &mut self.stage;
        scene.set_accessibility_active(engine, active);
        self.settle()
    }

    /// The accessibility tree as of the last frame: a "window" node, named
    /// by the shown `Window`'s title, holding the exposed items with their
    /// computed roles, names, states and children. Only the window while
    /// the tree is off.
    pub fn accessibility_tree(&self) -> AccessibleNode {
        let sys = ljos_controls::WindowSystem::of(&self.stage.engine);
        let title = self
            .stage
            .scene
            .root()
            .and_then(|r| sys.spec(&self.stage.engine, r))
            .map(|s| s.title)
            .unwrap_or_default();
        ljos_drive::a11y::tree(&self.stage.scene, title)
    }

    /// The node `item` is exposed as, while the tree is on.
    pub fn accessible_node(&self, item: ObjectId) -> Option<NodeKey> {
        self.stage.scene.accessible_node(item)
    }

    /// The item behind a node of the tree; for a part of an item (a link
    /// in a text), that item. `None` for an announcement.
    pub fn accessible_item(&self, node: NodeKey) -> Option<ObjectId> {
        self.stage.scene.accessible_item(node)
    }

    /// What `item` painted in the last frame, in its own coordinates;
    /// `None` for an item that paints nothing or is not shown.
    pub fn painted(&self, item: ObjectId) -> Option<Arc<DisplayList>> {
        let node = self.stage.scene.node_of(item)?;
        self.stage
            .scene
            .render_tree()
            .node(node)?
            .content()
            .cloned()
    }
}

/// Makes `text` the text engine of this thread, for a test that lays text
/// out with fonts of its own. The scenes lay their text out again in their
/// next frame. A host chooses between the bundled and the system's fonts
/// with `ljos::render::set_fonts` (the `render` feature) instead.
pub fn set_text_engine(text: TextEngine) {
    ljos_quick::set_text_engine(text);
}

/// Plays the system: tells the application what it prefers (colour
/// scheme, contrast, less motion, text size), changing what `change`
/// changes and leaving the rest as the host said last.
pub fn update_system_prefs(engine: &Engine, change: impl FnOnce(&mut SystemPrefs)) {
    ljos_quick::update_system_prefs(engine, change);
}

/// Plays the host: says how much of the application its user has
/// (`Qt.application.state`).
pub fn set_application_state(engine: &Engine, state: ApplicationState) {
    ljos_controls::WindowSystem::of(engine).set_application_state(engine, state);
}

/// Plays the host: says which displays there are, the primary first.
pub fn set_screens(engine: &Engine, screens: &[ScreenSpec]) {
    ljos_controls::WindowSystem::of(engine).set_screens(engine, screens);
}

/// Plays the host: says that the `Window` `window` is on display `index`
/// of those [`set_screens`] said.
pub fn set_window_screen(engine: &Engine, window: ObjectId, index: usize) {
    ljos_controls::WindowSystem::of(engine).set_window_screen(engine, window, index);
}
