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
    /// The face of each type a file of Qt's QML is given: its enums, and
    /// none of Ljós's homes for what was `Qt`'s. Beside it, which homes the
    /// type has in a file of Ljós's API: none, ones every file shares, or
    /// ones that read an address from the file that asks.
    faces: RefCell<Vec<(MetaRef, JsRef, Homes)>>,
    /// The face of such a type in a file of Ljós's API, by its index and,
    /// for a type with members that read an address, the file.
    ljos: RefCell<FxHashMap<(usize, Option<Atom>), JsRef>>,
}

/// Puts `Symbol.hasInstance` on the enums of a type; not enumerable, so the
/// type still lists only its enums. In a file of Ljós's API, a type
/// `__ljos_typeStatics` names by `module.name` also gets those members, its
/// homes for what was `Qt`'s (docs/DIALECT.md §4.3.1): `Locale.current`,
/// `Font.of`. One `__ljos_typeStaticsAt` names gets, in each such file,
/// members made with that file's address: `Component.load`, which reads a
/// relative address from the file whose script asks, as
/// `Qt.createComponent` does. A file of Qt's QML has `Qt`, and none of them.
pub(crate) const TYPE_FACES: &str = r#"
globalThis.__ljos_typeFace = (enums, index) =>
  Object.defineProperty(enums, Symbol.hasInstance, {
    value: (v) => __ljos_instanceOf(v, index),
  });
globalThis.__ljos_typeFaceAt = (enums, index, name, url) => {
  const face = __ljos_typeFace(enums, index);
  const put = (statics) => {
    for (const [key, d] of Object.entries(Object.getOwnPropertyDescriptors(statics ?? {})))
      Object.defineProperty(face, key, { ...d, enumerable: false });
  };
  put(globalThis.__ljos_typeStatics?.[name]);
  if (url !== undefined) put(globalThis.__ljos_typeStaticsAt?.[name]?.(url));
  return face;
};
"#;

impl Engine {
    /// What stands for `meta` in a script of the file at `at`, or of no
    /// file; `ljos` for a file of Ljós's API, which reads the type's homes
    /// for what was `Qt`'s off it.
    pub(crate) fn type_face(
        &self,
        s: &Scope<'_>,
        meta: &MetaRef,
        at: Option<&Atom>,
        ljos: bool,
    ) -> JsValue {
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
        let (face, homes) = {
            let faces = all.faces.borrow();
            (faces[index].1.clone(), faces[index].2)
        };
        if !ljos || homes == Homes::None {
            return JsValue::Handle(face);
        }
        let name = face_name(meta);
        let url = at.filter(|_| homes == Homes::PerFile).cloned();
        let key = (index, url.clone());
        if let Some(face) = all.ljos.borrow().get(&key) {
            return JsValue::Handle(face.clone());
        }
        let made = (|| {
            let make: ljos_js::rquickjs::Function = s.ctx().globals().get("__ljos_typeFaceAt")?;
            let enums = super::singletons::enum_keys(meta);
            let url = url.as_ref().map(ToString::to_string);
            let value: ljos_js::rquickjs::Value =
                make.call((s.to_value(&enums)?, index, name, url))?;
            Ok::<_, ljos_js::rquickjs::Error>(s.keep(value))
        })();
        match made {
            Ok(made) => {
                all.ljos.borrow_mut().insert(key, made.clone());
                JsValue::Handle(made)
            }
            Err(_) => JsValue::Handle(face),
        }
    }

    /// Makes the face of `meta` every file of Qt's QML shares, and gives
    /// its index.
    fn shared_type_face(&self, s: &Scope<'_>, meta: &MetaRef) -> Option<usize> {
        let all = self.extension::<TypeFaces>();
        let enums = super::singletons::enum_keys(meta);
        let index = all.faces.borrow().len();
        let name = face_name(meta);
        let made = (|| {
            let globals = s.ctx().globals();
            let make: ljos_js::rquickjs::Function = globals.get("__ljos_typeFace")?;
            let homes = if names(s, "__ljos_typeStaticsAt", &name) {
                Homes::PerFile
            } else if names(s, "__ljos_typeStatics", &name) {
                Homes::Shared
            } else {
                Homes::None
            };
            let value: ljos_js::rquickjs::Value = make.call((s.to_value(&enums)?, index))?;
            Ok::<_, ljos_js::rquickjs::Error>((s.keep(value), homes))
        })();
        let (face, homes) = made.ok()?;
        all.faces.borrow_mut().push((meta.clone(), face, homes));
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

/// Whether the global table `table` has members of the type `name`.
fn names(s: &Scope<'_>, table: &str, name: &str) -> bool {
    s.ctx()
        .globals()
        .get::<_, Option<ljos_js::rquickjs::Object>>(table)
        .ok()
        .flatten()
        .is_some_and(|at| at.contains_key(name).unwrap_or(false))
}

/// What a type has of Ljós's homes for what was `Qt`'s.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Homes {
    None,
    /// The same in every file (`Locale.current`).
    Shared,
    /// Some read an address from the file that asks (`Component.load`).
    PerFile,
}
