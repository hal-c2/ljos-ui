//! A QML type written as a value in a script (`Text.AlignLeft`,
//! `item instanceof Rectangle`).
//!
//! What stands for the type is one object for the engine: its enums as
//! members, and `Symbol.hasInstance`, so that `instanceof` asks whether an
//! object's type is this one or derives from it, as Qt answers.

use std::cell::RefCell;

use ljos_js::{JsRef, JsValue, Scope};

use crate::engine::Engine;
use crate::meta::MetaRef;

/// Each type a script was given, with what stands for it there. The index
/// is the number `__ljos_instanceOf` is asked with.
#[derive(Default)]
pub(crate) struct TypeFaces {
    faces: RefCell<Vec<(MetaRef, JsRef)>>,
}

/// Puts `Symbol.hasInstance` on the enums of a type; not enumerable, so the
/// type still lists only its enums.
pub(crate) const TYPE_FACES: &str = r#"
globalThis.__ljos_typeFace = (enums, index) => Object.defineProperty(enums, Symbol.hasInstance, {
  value: (v) => __ljos_instanceOf(v, index),
});
"#;

impl Engine {
    /// What stands for `meta` in a script. Its enums alone when the
    /// object cannot be made.
    pub(crate) fn type_face(&self, s: &Scope<'_>, meta: &MetaRef) -> JsValue {
        let all = self.extension::<TypeFaces>();
        if let Some((_, face)) = all
            .faces
            .borrow()
            .iter()
            .find(|(m, _)| MetaRef::ptr_eq(m, meta))
        {
            return JsValue::Handle(face.clone());
        }
        let enums = super::singletons::enum_keys(meta);
        let index = all.faces.borrow().len();
        let made = (|| {
            let make: ljos_js::rquickjs::Function = s.ctx().globals().get("__ljos_typeFace")?;
            let value: ljos_js::rquickjs::Value = make.call((s.to_value(&enums)?, index))?;
            Ok::<_, ljos_js::rquickjs::Error>(s.keep(value))
        })();
        match made {
            Ok(face) => {
                all.faces.borrow_mut().push((meta.clone(), face.clone()));
                JsValue::Handle(face)
            }
            Err(_) => enums,
        }
    }

    /// `__ljos_instanceOf(value, index)`: whether `value` is an object of the
    /// engine's whose type is the one at `index`, or derives from it.
    pub(crate) fn instance_of(&self, value: &JsValue, index: usize) -> bool {
        let JsValue::Object(o) = value else {
            return false;
        };
        let all = self.extension::<TypeFaces>();
        let faces = all.faces.borrow();
        let Some((ty, _)) = faces.get(index) else {
            return false;
        };
        self.meta_of(crate::js::u64_to_obj(*o))
            .is_some_and(|meta| meta.inherits(ty))
    }
}
