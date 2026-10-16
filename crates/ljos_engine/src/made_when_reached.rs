//! Objects a property holds that are made later than the object that holds
//! them: a style gives a control's parts as components, made when the
//! control completes. A member written through such a property
//! (`popup.width: 77`, `background.opacity: 0.5`) is the made object's, so
//! the object is made when the creator first reaches it that way.
//!
//! The creator does not know which properties those are. A crate built on
//! the engine says so for its types ([`Engine::register_made_when_reached`]).

use std::cell::RefCell;
use std::rc::Rc;

use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::meta::MetaObject;

/// Makes, when it is still to be made, the object `obj`'s property `name`
/// (dotted for a group's: `up.indicator`) is to hold.
type MakeWhenReached = Rc<dyn Fn(&Engine, ObjectId, &str)>;

#[derive(Default)]
struct Makers {
    by_type: RefCell<Vec<(&'static MetaObject, MakeWhenReached)>>,
}

impl Engine {
    /// Has `make` called for an object of `meta` (or of what derives from
    /// it) before a member is written through one of its properties.
    pub fn register_made_when_reached(
        &self,
        meta: &'static MetaObject,
        make: impl Fn(&Engine, ObjectId, &str) + 'static,
    ) {
        let makers = self.extension::<Makers>();
        let mut all = makers.by_type.borrow_mut();
        all.retain(|(m, _)| !std::ptr::eq(*m, meta));
        all.push((meta, Rc::new(make)));
    }

    /// Before members are written through `path` from `obj`: makes what
    /// each step of it is to hold.
    pub(crate) fn make_when_reached(&self, obj: ObjectId, path: &[String]) {
        let make = {
            let makers = self.extension::<Makers>();
            let all = makers.by_type.borrow();
            if all.is_empty() {
                return;
            }
            let Some(meta) = self.meta_of(obj) else {
                return;
            };
            match all.iter().find(|(m, _)| meta.inherits(m)) {
                Some((_, f)) => f.clone(),
                None => return,
            }
        };
        for end in 1..=path.len() {
            make(self, obj, &path[..end].join("."));
        }
    }
}
