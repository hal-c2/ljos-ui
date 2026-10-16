//! Singleton types: one object for the whole engine, there for every file
//! that names the type (ARCHITECTURE.md §6.4).
//!
//! A singleton is either a QML file that says `pragma Singleton`, or a
//! native type registered with [`Engine::register_singleton`]. Either way
//! its object is made the first time a script comes to its name, and never
//! again: the compiler gives a script something that stands for the type
//! ([`ljos_ir::Arg::Singleton`]) and has it ask for the object where the
//! name was written (`__ljos_single`), so a branch that is not taken leaves
//! the object unmade.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_ir::TypeRef;
use ljos_js::JsValue;

use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::loader::LoadedFile;
use crate::meta::{MetaObject, MetaRef};
use crate::object::{QmlObject, QmlType};
use crate::registry::Version;
use crate::value::Value;

/// How many objects a singleton type has. A singleton per window is not
/// built (ARCHITECTURE.md §6.4): per-window state lives in an engine
/// singleton, keyed by the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SingletonScope {
    /// One for the engine, shared by all its windows.
    Engine,
}

/// A native type that is one object. Implemented by `#[qml_singleton]`.
///
/// ```
/// use ljos_engine::{Engine, QmlObject, Version, qml_singleton};
///
/// #[derive(Default, QmlObject)]
/// #[qml(module = "Example", name = "Settings")]
/// #[qml_singleton]
/// pub struct Settings {
///     #[qml(notify)]
///     volume: i32,
/// }
///
/// let engine = Engine::new();
/// engine.register_singleton::<Settings>("Example", Version::new(1, 0));
/// let settings = engine.singleton::<Settings>().unwrap();
/// engine.set(settings, Settings::VOLUME, 7).unwrap();
/// assert_eq!(engine.singleton::<Settings>(), Ok(settings));
/// ```
pub trait QmlSingleton: QmlType {
    const SCOPE: SingletonScope;
}

/// What a script is given for the name of a singleton type.
enum Kind {
    /// The QML file of that key.
    File(String),
    Native(Native),
    /// A type the compiler's type table calls a singleton and the engine
    /// has no object of: its name still reaches its enums.
    Keys(Option<MetaRef>),
}

enum State {
    Unmade,
    /// [`Kind::Keys`], found once.
    Keys(Option<MetaRef>),
    /// Its own bindings are running: they read the object as far as it is
    /// made, once there is one.
    Making(Option<ObjectId>),
    Made {
        obj: ObjectId,
        /// The file it was made from, for a QML singleton.
        file: Option<Rc<LoadedFile>>,
        /// [`Singletons::sources`] when the file was last looked at.
        seen: u64,
    },
    /// It did not load, which was reported once.
    Failed,
}

struct Entry {
    ty: TypeRef,
    state: State,
}

/// The engine's singletons.
#[derive(Default)]
pub(crate) struct Singletons {
    /// By what stands for the type in a script. Never shortened.
    entries: RefCell<Vec<Entry>>,
    index: RefCell<HashMap<TypeRef, usize>>,
    /// Native singleton types, by module and name.
    natives: RefCell<HashMap<(String, String), Native>>,
    /// Bumped when the files the engine loads from change: the file of a
    /// singleton may have.
    sources: Cell<u64>,
}

/// A native singleton type and how its one object is made. Its own
/// constructor, not the type's QML-facing one: a type QML may not write
/// (`uncreatable`) is still made by the engine.
#[derive(Clone)]
struct Native {
    meta: &'static MetaObject,
    make: Make,
}

type Make = Rc<dyn Fn(&Engine) -> Box<dyn QmlObject>>;

impl Engine {
    /// Registers `T` as a singleton type of the module `uri`. The engine's
    /// one `T` is made with `T::default()` when it is first read, from QML
    /// by the type's name or from Rust by [`Engine::singleton`]. It is made
    /// even if `T` is `uncreatable`: that only keeps QML from writing
    /// `T {}`.
    pub fn register_singleton<T: QmlSingleton + Default>(&self, uri: &str, version: Version) {
        self.register_singleton_with(uri, version, |_| T::default());
    }

    /// Registers `T` as a singleton type of the module `uri`, made by
    /// `make` when it is first read: for a singleton that is handed what
    /// it depends on (a service, a channel to a worker) or is not
    /// `Default`. `make` runs on the Engine's thread, once, unless the
    /// object is destroyed and read again.
    ///
    /// ```
    /// use std::rc::Rc;
    /// use ljos_engine::{Engine, QmlObject, Version, qml_singleton};
    ///
    /// #[derive(QmlObject)]
    /// #[qml(module = "Example", name = "Store", uncreatable)]
    /// #[qml_singleton]
    /// pub struct Store {
    ///     #[qml(constant, readonly)]
    ///     limit: i32,
    /// }
    ///
    /// let limit = Rc::new(40);
    /// let engine = Engine::new();
    /// engine.register_singleton_with("Example", Version::new(1, 0), move |_| Store {
    ///     limit: *limit,
    /// });
    /// let store = engine.singleton::<Store>().unwrap();
    /// assert_eq!(engine.with::<Store, _>(store, |s| s.limit).ok(), Some(40));
    /// ```
    pub fn register_singleton_with<T: QmlSingleton>(
        &self,
        uri: &str,
        version: Version,
        make: impl Fn(&Engine) -> T + 'static,
    ) {
        let meta = T::static_meta();
        self.register_type(uri, version, MetaRef::Static(meta));
        let all = self.extension::<Singletons>();
        let make: Make = Rc::new(move |engine| Box::new(make(engine)));
        all.natives.borrow_mut().insert(
            (uri.to_owned(), meta.name.to_string()),
            Native { meta, make },
        );
        // A script may have asked for the name before the type was there.
        for e in all.entries.borrow_mut().iter_mut() {
            if matches!(e.state, State::Keys(_)) {
                e.state = State::Unmade;
            }
        }
    }

    /// Whether the type `name` of module `uri` is a native singleton
    /// ([`Engine::register_singleton`]): one object, not a type to make
    /// objects of.
    pub fn is_native_singleton(&self, uri: &str, name: &str) -> bool {
        let all = self.extension::<Singletons>();
        let natives = all.natives.borrow();
        natives.contains_key(&(uri.to_owned(), name.to_owned()))
    }

    /// The engine's one `T`, made on first use.
    pub fn singleton<T: QmlSingleton>(&self) -> Result<ObjectId, EngineError> {
        let meta = T::static_meta();
        let found = {
            let all = self.extension::<Singletons>();
            let natives = all.natives.borrow();
            natives
                .iter()
                .find(|(_, n)| std::ptr::eq(n.meta, meta))
                .map(|((module, name), _)| TypeRef::Native {
                    module: module.clone(),
                    name: name.clone(),
                })
        };
        let ty = found.ok_or_else(|| EngineError::UnknownType {
            uri: String::new(),
            name: meta.name.to_string(),
        })?;
        self.singleton_object(&ty)
            .ok_or_else(|| EngineError::Load(format!("{}: the singleton was not made", meta.name)))
    }

    /// The object of the singleton type `name` of the module `uri`, made
    /// on first use; `None` when the engine has no such singleton. With an
    /// empty `uri`, `name` is the key of a file of the application that
    /// says `pragma Singleton` (`"Theme"`, `"look/Tints"`).
    pub fn singleton_by_name(&self, uri: &str, name: &str) -> Option<ObjectId> {
        if !uri.is_empty() {
            return self.singleton_object(&TypeRef::Native {
                module: uri.to_owned(),
                name: name.to_owned(),
            });
        }
        // Only a file that is one: any other would be made as one here.
        let file = self.load_file(name).ok()?;
        if !file.ir.pragmas.iter().any(|p| p.name == "Singleton") {
            return None;
        }
        self.singleton_object(&TypeRef::Component {
            file: name.to_owned(),
            inline: None,
        })
    }

    /// A native singleton by its name alone, for a script whose compiler
    /// did not know the type: the name was left to be looked up when the
    /// program runs.
    pub(crate) fn singleton_named(&self, name: &str) -> Option<ObjectId> {
        let ty = {
            let all = self.extension::<Singletons>();
            let natives = all.natives.borrow();
            if natives.is_empty() {
                return None;
            }
            let (module, _) = natives.keys().find(|(_, n)| n == name)?;
            TypeRef::Native {
                module: module.clone(),
                name: name.to_owned(),
            }
        };
        self.singleton_object(&ty)
    }

    /// The files the engine loads from changed: a QML singleton is looked
    /// at again when it is next read, and made again if its file is not
    /// the one it was made from.
    pub(crate) fn singletons_sources_changed(&self) {
        let all = self.extension::<Singletons>();
        all.sources.set(all.sources.get() + 1);
    }

    /// What stands for the singleton `ty` in a script: the number
    /// `__ljos_single` is called with.
    pub(crate) fn singleton_handle(&self, ty: &TypeRef) -> usize {
        let all = self.extension::<Singletons>();
        if let Some(i) = all.index.borrow().get(ty) {
            return *i;
        }
        let mut entries = all.entries.borrow_mut();
        entries.push(Entry {
            ty: ty.clone(),
            state: State::Unmade,
        });
        let i = entries.len() - 1;
        all.index.borrow_mut().insert(ty.clone(), i);
        i
    }

    /// `__ljos_single(handle)`: the object, or for a type with none the
    /// keys of its enums.
    pub(crate) fn singleton_js(&self, handle: usize) -> JsValue {
        let ty = {
            let all = self.extension::<Singletons>();
            let entries = all.entries.borrow();
            match entries.get(handle) {
                Some(e) => e.ty.clone(),
                None => return JsValue::Undefined,
            }
        };
        self.singleton_value_js(&ty)
    }

    /// The singleton `ty` as a script has it once it has asked.
    pub(crate) fn singleton_value_js(&self, ty: &TypeRef) -> JsValue {
        if let Some(o) = self.singleton_object(ty) {
            return self.to_js(&Value::Object(o));
        }
        let all = self.extension::<Singletons>();
        let entries = all.entries.borrow();
        let found = all.index.borrow().get(ty).copied();
        match found.and_then(|i| entries.get(i)).map(|e| &e.state) {
            Some(State::Keys(Some(meta))) => enum_keys(meta),
            _ => JsValue::Undefined,
        }
    }

    fn singleton_kind(&self, ty: &TypeRef) -> Kind {
        match ty {
            TypeRef::Component { file, inline: None } => Kind::File(file.clone()),
            TypeRef::Native { module, name } => {
                let all = self.extension::<Singletons>();
                let native = {
                    let natives = all.natives.borrow();
                    natives
                        .get(&(module.clone(), name.clone()))
                        .or_else(|| {
                            // The compiler may know the type by another
                            // module than the one it was registered in.
                            natives
                                .iter()
                                .find_map(|((_, n), m)| (n == name).then_some(m))
                        })
                        .cloned()
                };
                if let Some(native) = native {
                    return Kind::Native(native);
                }
                // A type of a module written in QML (a style's file).
                if let Some(key) = self.qml_type_key(module, name)
                    && let Ok(file) = self.load_file(&key)
                    && file.ir.pragmas.iter().any(|p| p.name == "Singleton")
                {
                    return Kind::File(key);
                }
                Kind::Keys(self.native_meta(module, name).ok())
            }
            _ => Kind::Keys(None),
        }
    }

    /// The object the singleton `ty` is, made now if it was not. `None`
    /// when the engine has no object for the type and when it failed to
    /// load (reported once). While it is being made its own bindings get
    /// the object as far as it is made, which is what Qt gives them.
    pub(crate) fn singleton_object(&self, ty: &TypeRef) -> Option<ObjectId> {
        let all = self.extension::<Singletons>();
        let handle = self.singleton_handle(ty);
        let sources = all.sources.get();
        // The object as it stands, and whether its file has to be looked
        // at again.
        let stands = match &all.entries.borrow()[handle].state {
            State::Made { obj, file, seen } if self.is_alive(*obj) => {
                if file.is_none() || *seen == sources {
                    return Some(*obj);
                }
                Some((*obj, file.clone()))
            }
            State::Making(obj) => return *obj,
            State::Failed | State::Keys(_) => return None,
            State::Made { .. } | State::Unmade => None,
        };
        let kind = self.singleton_kind(ty);
        let kind = match kind {
            Kind::Keys(meta) => {
                all.entries.borrow_mut()[handle].state = State::Keys(meta);
                return None;
            }
            kind => kind,
        };
        if let (Some((obj, Some(old))), Kind::File(key)) = (&stands, &kind) {
            let same = self
                .load_file(key)
                .is_ok_and(|now| now.ir.source_hash == old.ir.source_hash);
            if same {
                if let State::Made { seen, .. } = &mut all.entries.borrow_mut()[handle].state {
                    *seen = sources;
                }
                return Some(*obj);
            }
            // The file is another one now: the object of the old one goes,
            // as everything else made from the old files does.
            self.destroy(*obj);
        }
        all.entries.borrow_mut()[handle].state = State::Making(None);
        // Whoever reads the name first does not depend on what making the
        // object reads.
        let made = self.untrack(|| match &kind {
            Kind::File(key) => {
                let template = self.file_template(key)?;
                let file = template.file.clone();
                let allocated = |obj| {
                    all.entries.borrow_mut()[handle].state = State::Making(Some(obj));
                };
                self.create_from_template_noting(&template, None, Vec::new(), allocated)
                    .map(|obj| (obj, Some(file)))
            }
            Kind::Native(native) => self
                .create_with(MetaRef::Static(native.meta), (native.make)(self), None)
                .map(|obj| (obj, None)),
            Kind::Keys(_) => unreachable!("returned above"),
        });
        let (state, out) = match made {
            Ok((obj, file)) => (
                State::Made {
                    obj,
                    file,
                    seen: sources,
                },
                Some(obj),
            ),
            Err(e) => {
                self.warn(format!(
                    "the singleton {} is not available: {e}",
                    type_name(ty)
                ));
                (State::Failed, None)
            }
        };
        all.entries.borrow_mut()[handle].state = state;
        out
    }
}

fn type_name(ty: &TypeRef) -> &str {
    match ty {
        TypeRef::Native { name, .. } | TypeRef::Unresolved(name) => name,
        TypeRef::Component { file, inline } => inline
            .as_deref()
            .unwrap_or_else(|| file.rsplit('/').next().unwrap_or(file)),
    }
}

/// A type as a script reads its enums: every key by its name, and every
/// enum by its own with its keys (`Type.Key`, `Type.Enum.Key`).
pub(crate) fn enum_keys(meta: &MetaObject) -> JsValue {
    let number = |v: i64| JsValue::Number(v as f64);
    let mut out: Vec<(String, JsValue)> = meta
        .enums
        .iter()
        .flat_map(|e| e.keys.iter())
        .map(|(k, v)| (k.to_string(), number(*v)))
        .collect();
    for e in &meta.enums {
        if e.name.is_empty() || e.keys.iter().any(|(k, _)| *k == e.name) {
            continue;
        }
        out.push((
            e.name.to_string(),
            JsValue::Map(
                e.keys
                    .iter()
                    .map(|(k, v)| (k.to_string(), number(*v)))
                    .collect(),
            ),
        ));
    }
    JsValue::Map(out)
}
