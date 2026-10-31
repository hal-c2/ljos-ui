//! The scene a view shows, and word to its host when the picture changes.
//!
//! A host builds its view's scene again whenever something the mapping read
//! changed, and most builds come out the same: an object moved and moved
//! back, a member was written with the value it had. A [`SceneSlot`] keeps
//! the scene it holds when an offered one draws the same, so the renderer,
//! which keys its work by the scene's revision, draws nothing again; and it
//! tells the host only when the picture changed, once, until the host takes
//! it. A web host asks the browser for a frame from that word alone, never
//! comparing revisions or drawing on a timer.

use std::cell::Cell;
use std::fmt;
use std::sync::Arc;

use crate::scene::Scene;

/// The scene a view shows. See the [module](self).
#[derive(Default)]
pub struct SceneSlot {
    current: Option<Arc<Scene>>,
    listener: Option<Box<dyn Fn()>>,
    /// A change the host was told of and has not taken: told once, however
    /// many changes come before it takes them.
    told: Cell<bool>,
}

impl fmt::Debug for SceneSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SceneSlot")
            .field("revision", &self.current.as_ref().map(|s| s.revision()))
            .field("listener", &self.listener.is_some())
            .field("told", &self.told.get())
            .finish()
    }
}

impl SceneSlot {
    pub fn new() -> Self {
        SceneSlot::default()
    }

    /// Calls `listener` when the slot comes to hold a scene that draws
    /// differently. It runs inside [`offer`](Self::offer), with the slot
    /// borrowed: it should ask for a frame, not read the slot.
    pub fn on_change(&mut self, listener: impl Fn() + 'static) {
        self.listener = Some(Box::new(listener));
    }

    /// Offers the scene just built; returns the scene the slot now holds.
    /// One that draws the same as the held scene leaves it held, revision
    /// and all; another replaces it and tells the listener, unless it was
    /// told of a change not taken yet.
    pub fn offer(&mut self, scene: Scene) -> Arc<Scene> {
        if let Some(held) = &self.current
            && held.same_as(&scene)
        {
            return held.clone();
        }
        let scene = Arc::new(scene);
        self.current = Some(scene.clone());
        if !self.told.replace(true)
            && let Some(listener) = &self.listener
        {
            listener();
        }
        scene
    }

    /// The scene held, the change taken: the next change tells the
    /// listener again.
    pub fn current(&self) -> Option<Arc<Scene>> {
        self.told.set(false);
        self.current.clone()
    }

    /// The scene held if it changed since the host last took it, the
    /// change taken; `None` when the host has drawn it already.
    pub fn take(&self) -> Option<Arc<Scene>> {
        if self.told.replace(false) {
            self.current.clone()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::math::Vec3;
    use crate::scene::{Node, NodeKind, Transform};

    fn at(x: f32) -> Scene {
        let mut s = Scene::new();
        s.add(
            None,
            Node::new(NodeKind::Group).with_transform(Transform {
                position: Vec3::new(x, 0.0, 0.0),
                ..Transform::default()
            }),
        );
        s
    }

    fn counted() -> (SceneSlot, Rc<Cell<u32>>) {
        let calls = Rc::new(Cell::new(0));
        let mut slot = SceneSlot::new();
        let c = calls.clone();
        slot.on_change(move || c.set(c.get() + 1));
        (slot, calls)
    }

    #[test]
    fn a_scene_that_draws_the_same_keeps_the_one_held_and_tells_nobody() {
        let (mut slot, calls) = counted();
        let first = slot.offer(at(1.0));
        assert_eq!(calls.get(), 1);
        assert!(slot.take().is_some());
        let again = slot.offer(at(1.0));
        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!(calls.get(), 1);
        assert!(slot.take().is_none());
    }

    #[test]
    fn changes_before_the_host_takes_them_tell_it_once() {
        let (mut slot, calls) = counted();
        slot.offer(at(1.0));
        slot.offer(at(2.0));
        slot.offer(at(3.0));
        assert_eq!(calls.get(), 1);
        let shown = slot.take().expect("a change");
        assert!(shown.same_as(&at(3.0)));
        slot.offer(at(4.0));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn reading_the_current_scene_takes_the_change() {
        let (mut slot, calls) = counted();
        assert!(slot.current().is_none());
        slot.offer(at(1.0));
        assert!(slot.current().is_some());
        assert!(slot.take().is_none());
        slot.offer(at(2.0));
        assert_eq!(calls.get(), 2);
    }
}
