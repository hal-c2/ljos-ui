//! Freezing an object and what it owns (ARCHITECTURE.md §8.3): a page a
//! view covers keeps its objects and its values but does no work. Its
//! effects are parked in the graph, its timers wait and the animations of
//! the scene skip it, until it is thawed.

use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::object::ObjectFlags;

impl Engine {
    /// Freezes `obj` and everything it owns, or thaws it.
    ///
    /// While frozen, effects (watches, what the scene follows) are parked
    /// and run once on thaw against the latest values; bindings still
    /// answer reads. Timers wake nobody: one that came due meanwhile fires
    /// once on thaw, however many ticks it missed. Animations stand still
    /// and catch up on thaw, as if they had run unseen. Observers see the
    /// same values and the same signals in the end, not as often.
    pub fn set_frozen(&self, obj: ObjectId, frozen: bool) -> Result<(), EngineError> {
        let scope = self.ensure_scope(obj)?;
        {
            let mut store = self.inner.store.borrow_mut();
            let h = store
                .objects
                .get_mut(obj)
                .ok_or(EngineError::DeadObject(obj))?;
            if h.flags.contains(ObjectFlags::FROZEN) == frozen {
                return Ok(());
            }
            h.flags.set(ObjectFlags::FROZEN, frozen);
        }
        self.inner.graph.set_frozen(scope, frozen)?;
        if frozen {
            crate::qtqml::park_timers(self);
        } else {
            crate::qtqml::unpark_timers(self);
            // What stood still asks for its next frame again.
            self.wake_host();
        }
        Ok(())
    }

    /// Whether `obj` or an object that owns it is frozen.
    pub fn is_frozen(&self, obj: ObjectId) -> bool {
        let store = self.inner.store.borrow();
        let mut at = Some(obj);
        while let Some(o) = at {
            let Some(h) = store.objects.get(o) else {
                return false;
            };
            if h.flags.contains(ObjectFlags::FROZEN) {
                return true;
            }
            at = h.owner;
        }
        false
    }
}
