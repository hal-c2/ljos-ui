//! A QML type written as a value in a script (`Text.AlignLeft`,
//! `item instanceof Rectangle`).
//!
//! What stands for the type is one object for the engine: its enums as
//! members, and `Symbol.hasInstance`, so that `instanceof` asks whether an
//! object's type is this one or derives from it, as Qt answers.

use std::cell::RefCell;

use ljos_js::{JsRef, JsValue, Scope};

use rustc_hash::FxHashMap;

use crate::atom::Atom;
use crate::engine::Engine;
use crate::meta::MetaRef;

/// The name `__ljos_typeStatics` knows `meta` by: `module.name`.
fn face_name(meta: &MetaRef) -> String {
    match &meta.module {
        Some(module) => format!("{module}.{}", meta.name),
        None => meta.name.to_string(),
    }
}

/// Each type a script was given, with what stands for it there. The index
/// is the number `__ljos_instanceOf` is asked with.
#[derive(Default)]
pub(crate) struct TypeFaces {
    /// The face of each type, and whether `__ljos_typeStaticsAt` has
    /// members of it that read an address from the file that asks.
    faces: RefCell<Vec<(MetaRef, JsRef, bool)>>,
    /// The face of such a type in one file, by its index and the file.
    at: RefCell<FxHashMap<(usize, Atom), JsRef>>,
}

/// Puts `Symbol.hasInstance` on the enums of a type; not enumerable, so the
/// type still lists only its enums. A type `__ljos_typeStatics` names by
/// `module.name` also gets those members, its homes for what was `Qt`'s
/// (docs/DIALECT.md §4.3.1): `Locale.current`, `Font.of`. One
/// `__ljos_typeStaticsAt` names gets, in each file, members made with that
/// file's address: `Component.load`, which reads a relative address from
/// the file whose script asks, as `Qt.createComponent` does.
pub(crate) const TYPE_FACES: &str = r#"
globalThis.__ljos_typeFace = (enums, index, name) => {
  const statics = globalThis.__ljos_typeStatics?.[name];
  if (statics)
    for (const [key, d] of Object.entries(Object.getOwnPropertyDescriptors(statics)))
      Object.defineProperty(enums, key, { ...d, enumerable: false });
  return Object.defineProperty(enums, Symbol.hasInstance, {
    value: (v) => __ljos_instanceOf(v, index),
  });
};
globalThis.__ljos_typeFaceAt = (enums, index, name, url) => {
  const face = __ljos_typeFace(enums, index, name);
  const statics = globalThis.__ljos_typeStaticsAt[name](url);
  for (const [key, d] of Object.entries(Object.getOwnPropertyDescriptors(statics)))
    Object.defineProperty(face, key, { ...d, enumerable: false });
  return face;
};
"#;

impl Engine {
    /// What stands for `meta` in a script of the file at `at`, or of no
    /// file. Its enums alone when the object cannot be made.
    pub(crate) fn type_face(&self, s: &Scope<'_>, meta: &MetaRef, at: Option<&Atom>) -> JsValue {
        let all = self.extension::<TypeFaces>();
        let found = all
            .faces
            .borrow()
            .iter()
            .position(|(m, _, _)| MetaRef::ptr_eq(m, meta));
        let index = match found {
            Some(index) => index,
            None => match self.shared_type_face(s, meta) {
                Some(index) => index,
                None => return super::singletons::enum_keys(meta),
            },
        };
        let (face, per_file) = {
            let faces = all.faces.borrow();
            (faces[index].1.clone(), faces[index].2)
        };
        let Some(url) = at.filter(|_| per_file) else {
            return JsValue::Handle(face);
        };
        let key = (index, url.clone());
        if let Some(face) = all.at.borrow().get(&key) {
            return JsValue::Handle(face.clone());
        }
        let made = (|| {
            let make: ljos_js::rquickjs::Function = s.ctx().globals().get("__ljos_typeFaceAt")?;
            let enums = super::singletons::enum_keys(meta);
            let value: ljos_js::rquickjs::Value =
                make.call((s.to_value(&enums)?, index, face_name(meta), url.to_string()))?;
            Ok::<_, ljos_js::rquickjs::Error>(s.keep(value))
        })();
        match made {
            Ok(made) => {
                all.at.borrow_mut().insert(key, made.clone());
                JsValue::Handle(made)
            }
            Err(_) => JsValue::Handle(face),
        }
    }

    /// Makes the face of `meta` every file shares, and gives its index.
    fn shared_type_face(&self, s: &Scope<'_>, meta: &MetaRef) -> Option<usize> {
        let all = self.extension::<TypeFaces>();
        let enums = super::singletons::enum_keys(meta);
        let index = all.faces.borrow().len();
        let name = face_name(meta);
        let made = (|| {
            let globals = s.ctx().globals();
            let make: ljos_js::rquickjs::Function = globals.get("__ljos_typeFace")?;
            let per_file = globals
                .get::<_, Option<ljos_js::rquickjs::Object>>("__ljos_typeStaticsAt")?
                .is_some_and(|at| at.contains_key(name.as_str()).unwrap_or(false));
            let value: ljos_js::rquickjs::Value = make.call((s.to_value(&enums)?, index, name))?;
            Ok::<_, ljos_js::rquickjs::Error>((s.keep(value), per_file))
        })();
        let (face, per_file) = made.ok()?;
        all.faces.borrow_mut().push((meta.clone(), face, per_file));
        Some(index)
    }

    /// `__ljos_instanceOf(value, index)`: whether `value` is an object of the
    /// engine's whose type is the one at `index`, or derives from it.
    pub(crate) fn instance_of(&self, value: &JsValue, index: usize) -> bool {
        let JsValue::Object(o) = value else {
            return false;
        };
        let all = self.extension::<TypeFaces>();
        let faces = all.faces.borrow();
        let Some((ty, _, _)) = faces.get(index) else {
            return false;
        };
        self.meta_of(crate::js::u64_to_obj(*o))
            .is_some_and(|meta| meta.inherits(ty))
    }
}
