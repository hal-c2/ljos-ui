//! Singleton types: one object for the whole engine, there for every file
//! that names the type, or one for each window (ARCHITECTURE.md §6.4).
//!
//! A singleton is either a QML file that says `pragma Singleton`, or a
//! native type registered with [`Engine::register_singleton`]. Either way
//! its object is made the first time a script comes to its name, and never
//! again: the compiler gives a script something that stands for the type
//! ([`ljos_ir::Arg::Singleton`]) and has it ask for the object where the
//! name was written (`__ljos_single`), so a branch that is not taken leaves
//! the object unmade.
//!
//! **Windows.** A native type with [`SingletonScope::Window`] has one
//! object for each scope. A scope opens when the host makes a root of its
//! own (no parent object, from Rust): the root owns it. A root made with a
//! parent object is in the parent's scope, and one a script makes with no
//! parent (`createObject(null)`) is in the scope of the script that made
//! it; everything inside a root reads through its contexts, which lead to
//! the root's. The handle a script is given is the scope's own entry, so
//! the compiled program is the same for both kinds. A QML singleton whose
//! scripts read such a type is one for each window too, and its object's
//! context is in the window's scope. A reload makes the
//! root again in the scope the old one owned, so the window keeps its
//! objects.
//!
//! **A host's objects.** A host that makes a window's object itself, before
//! the window's QML, opens the scope first ([`Engine::window_scope`]),
//! gives it the object ([`Engine::set_singleton_in`]) and makes the root in
//! it ([`Engine::in_window_scope`]), which takes the scope as a reload's
//! root does. A scope no root has taken yet is a window without QML, which
//! a headless test reads with [`Engine::singleton_in_scope`].

use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_ir::{Arg, TypeRef};
use ljos_js::JsValue;

use super::Instance;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ContextId, ObjectId};
use crate::loader::LoadedFile;
use crate::meta::{MetaObject, MetaRef};
use crate::object::{QmlObject, QmlType};
use crate::registry::Version;
use crate::value::Value;

/// How many objects a singleton type has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SingletonScope {
    /// One for the engine, shared by all its windows.
    Engine,
    /// One for each root the host makes, read by everything under that
    /// root and destroyed with it: what an application that made a
    /// `QQmlEngine` for each window keeps in a singleton
    /// (`#[qml_singleton(scope = "window")]`, [`Engine::singleton_in`]).
    Window,
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

/// What became of a window's object of a [`SingletonScope::Window`] type
/// ([`Engine::on_scoped_singleton`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedEvent {
    /// It was made, as something under the window first read it; its
    /// bindings have run.
    Made,
    /// The window is going: the object still stands, and is destroyed
    /// once the window's own objects are.
    Dropped,
}

/// A window's set of the objects of [`SingletonScope::Window`] types, opened
/// by the host before the window's root ([`Engine::window_scope`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowScope(ScopeId);

/// One object of a [`SingletonScope::Window`] type and its window.
#[derive(Clone, Copy, Debug)]
pub struct ScopedSingleton {
    pub event: ScopedEvent,
    /// The root that owns the scope.
    pub root: ObjectId,
    pub object: ObjectId,
    pub meta: &'static MetaObject,
}

impl ScopedSingleton {
    /// Whether the object is a `T`.
    pub fn is<T: QmlType>(&self) -> bool {
        std::ptr::eq(self.meta, T::static_meta())
    }
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
    /// A type that is one for each window, read where there is no
    /// window: reported once.
    Outside,
    /// Its window is gone.
    Gone,
}

struct Entry {
    ty: TypeRef,
    /// The window's scope, for the entry of a type that is one per window.
    scope: Option<ScopeId>,
    /// The type is one for each window. Its entry without a scope stands
    /// for it where there is no window.
    window: bool,
    /// The host made the object and gave it to the scope: the scope's end
    /// leaves it standing.
    given: bool,
    state: State,
}

/// A scope: one window's set of singletons.
type ScopeId = u64;

type Listener = Rc<dyn Fn(&Engine, &ScopedSingleton)>;

#[derive(Default)]
struct Scopes {
    next: ScopeId,
    /// The context of each root in a scope.
    by_context: FxHashMap<ContextId, ScopeId>,
    /// Each root in a scope, with its context.
    members: FxHashMap<ObjectId, (ContextId, ScopeId)>,
    /// The scopes that stand.
    owners: FxHashMap<ScopeId, Owner>,
}

struct Owner {
    /// `None` until a root takes a scope the host opened.
    root: Option<ObjectId>,
    /// The entries made in the scope.
    entries: Vec<usize>,
}

/// The engine's singletons.
#[derive(Default)]
pub(crate) struct Singletons {
    /// By what stands for the type in a script. Never shortened.
    entries: RefCell<Vec<Entry>>,
    /// The entry of each type, without a scope.
    index: RefCell<FxHashMap<TypeRef, usize>>,
    /// The entry of a type (by its entry without a scope) in a scope.
    scoped: RefCell<FxHashMap<(usize, ScopeId), usize>>,
    /// Native singleton types, by module and name.
    natives: RefCell<FxHashMap<(String, String), Native>>,
    /// Bumped when the files the engine loads from change: the file of a
    /// singleton may have.
    sources: Cell<u64>,
    scopes: RefCell<Scopes>,
    /// The root about to be made is the host's own: it opens a scope.
    host: Cell<bool>,
    /// The scope the host's next root opens is this one (a reload's).
    adopt: Cell<Option<ScopeId>>,
    /// A QML singleton is being made: it is in no window.
    unscoped: Cell<u32>,
    listeners: RefCell<Vec<Listener>>,
}

/// A native singleton type and how its one object is made. Its own
/// constructor, not the type's QML-facing one: a type QML may not write
/// (`uncreatable`) is still made by the engine.
#[derive(Clone)]
struct Native {
    meta: &'static MetaObject,
    make: Make,
    scope: SingletonScope,
}

pub(crate) type Make = Rc<dyn Fn(&Engine) -> Box<dyn QmlObject>>;

impl Engine {
    /// Registers `T` as a singleton type of the module `uri`. The engine's
    /// one `T` is made with `T::default()` when it is first read, from QML
    /// by the type's name or from Rust by [`Engine::singleton`]. It is made
    /// even if `T` is `uncreatable`: that only keeps QML from writing
    /// `T {}`. A `T` that is one for each window
    /// ([`SingletonScope::Window`]) is made for each window that reads it.
    pub fn register_singleton<T: QmlSingleton + Default>(&self, uri: &str, version: Version) {
        self.register_singleton_with(uri, version, |_| T::default());
    }

    /// Registers `T` as a singleton type of the module `uri`, made by
    /// `make` when it is first read: for a singleton that is handed what
    /// it depends on (a service, a channel to a worker) or is not
    /// `Default`. `make` runs on the Engine's thread, once (once for each
    /// window, for a type that is one for each window), unless the object
    /// is destroyed and read again.
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
        let make: Make = Rc::new(move |engine| Box::new(make(engine)));
        self.register_singleton_meta(uri, version, T::static_meta(), make, T::SCOPE);
    }

    /// Registers `meta` as a singleton type whose object `make` makes.
    pub(crate) fn register_singleton_meta(
        &self,
        uri: &str,
        version: Version,
        meta: &'static MetaObject,
        make: Make,
        scope: SingletonScope,
    ) {
        self.register_type(uri, version, MetaRef::Static(meta));
        let all = self.extension::<Singletons>();
        all.natives.borrow_mut().insert(
            (uri.to_owned(), meta.name.to_string()),
            Native { meta, make, scope },
        );
        // A script may have asked for the name before the type was there.
        let mut entries = all.entries.borrow_mut();
        let asked: Vec<usize> = (0..entries.len())
            .filter(|&i| matches!(entries[i].state, State::Keys(_)))
            .collect();
        for i in asked {
            let window = self.singleton_native(&entries[i].ty).map(|n| n.scope)
                == Some(SingletonScope::Window);
            entries[i].window = window;
            entries[i].state = State::Unmade;
        }
    }

    /// Whether the type `name` of module `uri` is a native singleton
    /// ([`Engine::register_singleton`]): one object, not a type to make
    /// objects of.
    pub(crate) fn is_native_singleton(&self, uri: &str, name: &str) -> bool {
        let all = self.extension::<Singletons>();
        let natives = all.natives.borrow();
        natives.contains_key(&(uri.to_owned(), name.to_owned()))
    }

    /// Whether the type `name` of module `uri` is one object: a native
    /// singleton, or a library file that says `pragma Singleton`.
    pub fn is_singleton_type(&self, uri: &str, name: &str) -> bool {
        self.is_native_singleton(uri, name)
            || self.qml_type_key(uri, name).is_some_and(|key| {
                self.load_file(&key)
                    .is_ok_and(|file| file.ir.pragmas.iter().any(|p| p.name == "Singleton"))
            })
    }

    /// The engine's one `T`, made on first use. A `T` that is one for
    /// each window has none: ask [`Engine::singleton_in`].
    pub fn singleton<T: QmlSingleton>(&self) -> Result<ObjectId, EngineError> {
        let meta = T::static_meta();
        if T::SCOPE == SingletonScope::Window {
            return Err(EngineError::Load(format!(
                "{} is one for each window: ask for the one of a window with singleton_in",
                meta.name
            )));
        }
        let ty = self.singleton_type::<T>()?;
        self.singleton_object(&ty, None)
            .ok_or_else(|| EngineError::Load(format!("{}: the singleton was not made", meta.name)))
    }

    /// The `T` that `obj` reads: for a type that is one for each window,
    /// the object of the window `obj` is under, made now if it was not;
    /// for one of the engine, [`Engine::singleton`]. `obj` is a root the
    /// host made or anything under one.
    ///
    /// ```
    /// use ljos_engine::{Engine, QmlObject, Version, qml_singleton};
    ///
    /// #[derive(Default, QmlObject)]
    /// #[qml(module = "Example", name = "Document")]
    /// #[qml_singleton(scope = "window")]
    /// pub struct Document {
    ///     #[qml(notify)]
    ///     title: String,
    /// }
    ///
    /// let engine = Engine::new();
    /// engine.register_singleton::<Document>("Example", Version::new(1, 0));
    /// // There is no one `Document`: each root the host makes has its own.
    /// assert!(engine.singleton::<Document>().is_err());
    /// ```
    pub fn singleton_in<T: QmlSingleton>(&self, obj: ObjectId) -> Result<ObjectId, EngineError> {
        if T::SCOPE != SingletonScope::Window {
            return self.singleton::<T>();
        }
        let meta = T::static_meta();
        let ctx = self.context_of(obj).ok_or(EngineError::DeadObject(obj))?;
        if self.singleton_scope_at(ctx).is_none() {
            return Err(EngineError::Load(format!(
                "{} is one for each window, and {obj:?} is under none",
                meta.name
            )));
        }
        let ty = self.singleton_type::<T>()?;
        self.singleton_object(&ty, Some(ctx))
            .ok_or_else(|| EngineError::Load(format!("{}: the singleton was not made", meta.name)))
    }

    /// Calls `f` when an object of a type that is one for each window is
    /// made for a window, and when its window goes (while the object
    /// still stands). For as long as the engine lives.
    pub fn on_scoped_singleton(&self, f: impl Fn(&Engine, &ScopedSingleton) + 'static) {
        let all = self.extension::<Singletons>();
        all.listeners.borrow_mut().push(Rc::new(f));
    }

    /// The root that owns the window `obj` is under: whose objects of the
    /// types that are one for each window `obj` reads.
    pub fn singleton_scope_root(&self, obj: ObjectId) -> Option<ObjectId> {
        let scope = self.singleton_scope_at(self.context_of(obj)?)?;
        let all = self.extension::<Singletons>();
        let scopes = all.scopes.borrow();
        scopes.owners.get(&scope).and_then(|o| o.root)
    }

    /// The objects of the window `root` owns, of the types that are one
    /// for each window, as they were made.
    pub fn scoped_singletons(&self, root: ObjectId) -> Vec<ObjectId> {
        let all = self.extension::<Singletons>();
        let scopes = all.scopes.borrow();
        let Some(owner) = scopes.owners.values().find(|o| o.root == Some(root)) else {
            return Vec::new();
        };
        let entries = all.entries.borrow();
        owner
            .entries
            .iter()
            .filter_map(|&i| match entries[i].state {
                State::Made { obj, .. } => Some(obj),
                _ => None,
            })
            .collect()
    }

    fn singleton_type<T: QmlSingleton>(&self) -> Result<TypeRef, EngineError> {
        let meta = T::static_meta();
        let all = self.extension::<Singletons>();
        let natives = all.natives.borrow();
        natives
            .iter()
            .find(|(_, n)| std::ptr::eq(n.meta, meta))
            .map(|((module, name), _)| TypeRef::Native {
                module: module.clone(),
                name: name.clone(),
            })
            .ok_or_else(|| EngineError::UnknownType {
                uri: String::new(),
                name: meta.name.to_string(),
            })
    }

    /// The object of the singleton type `name` of the module `uri`, made
    /// on first use; `None` when the engine has no such singleton, or it
    /// is one for each window. With an empty `uri`, `name` is the key of
    /// a file of the application that says `pragma Singleton` (`"Theme"`,
    /// `"look/Tints"`).
    pub fn singleton_by_name(&self, uri: &str, name: &str) -> Option<ObjectId> {
        if !uri.is_empty() {
            let ty = TypeRef::Native {
                module: uri.to_owned(),
                name: name.to_owned(),
            };
            let handle = self.singleton_handle(&ty, None);
            if self.extension::<Singletons>().entries.borrow()[handle].window {
                return None;
            }
            return self.singleton_made(handle);
        }
        // Only a file that is one: any other would be made as one here.
        let file = self.load_file(name).ok()?;
        if !file.ir.pragmas.iter().any(|p| p.name == "Singleton") {
            return None;
        }
        self.singleton_object(
            &TypeRef::Component {
                file: name.to_owned(),
                inline: None,
            },
            None,
        )
    }

    /// A native singleton by its name alone, for a script of `ctx` whose
    /// compiler did not know the type: the name was left to be looked up
    /// when the program runs.
    pub(crate) fn singleton_named(&self, name: &str, ctx: ContextId) -> Option<ObjectId> {
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
        self.singleton_object(&ty, Some(ctx))
    }

    /// The singleton `name` is to a script no compiler saw, run by a tool
    /// in `ctx` (none for the global scope) as code written in the
    /// application's files: the type a loaded file resolved the name to,
    /// the file `from` first; else a native singleton of the name; else a
    /// file that says `pragma Singleton`, named `name` in the directory of
    /// a loaded file. A singleton for each window needs `ctx`.
    pub(crate) fn singleton_for_tool(
        &self,
        name: &str,
        ctx: Option<ContextId>,
        from: Option<&str>,
    ) -> Option<ObjectId> {
        fn singleton(arg: &Arg) -> Option<&TypeRef> {
            match arg {
                Arg::Singleton(t) => Some(t),
                Arg::Context {
                    fallback: Some(f), ..
                } => singleton(f),
                _ => None,
            }
        }
        // A type's name: anything else is a name of the script's own.
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let mut files: Vec<_> = self.inner.loader.borrow().files.values().cloned().collect();
        files.sort_by(|a, b| {
            (Some(a.key.as_str()) != from, &a.key).cmp(&(Some(b.key.as_str()) != from, &b.key))
        });
        let resolved = files.iter().find_map(|f| {
            f.ir.exprs
                .iter()
                .flat_map(|b| &b.params)
                .find_map(|p| (p.name == name).then(|| singleton(&p.arg)).flatten())
                .cloned()
        });
        let ty = resolved.or_else(|| {
            let all = self.extension::<Singletons>();
            let natives = all.natives.borrow();
            let (module, _) = natives.keys().find(|(_, n)| n == name)?;
            Some(TypeRef::Native {
                module: module.clone(),
                name: name.to_owned(),
            })
        });
        if let Some(ty) = ty {
            let handle = self.singleton_handle(&ty, ctx);
            if ctx.is_none() && self.extension::<Singletons>().entries.borrow()[handle].window {
                return None;
            }
            return self.singleton_made(handle);
        }
        let source = self.inner.loader.borrow().source.clone()?;
        let mut seen = Vec::new();
        files.iter().find_map(|f| {
            let key = match f.key.rsplit_once('/') {
                Some((dir, _)) => format!("{dir}/{name}"),
                None => name.to_owned(),
            };
            if seen.contains(&key) || !source.contains(&key) {
                return None;
            }
            let found = self.singleton_by_name("", &key);
            seen.push(key);
            found
        })
    }

    /// The files the engine loads from changed: a QML singleton is looked
    /// at again when it is next read, and made again if its file is not
    /// the one it was made from.
    pub(crate) fn singletons_sources_changed(&self) {
        let all = self.extension::<Singletons>();
        all.sources.set(all.sources.get() + 1);
    }

    /// The singletons made from a file a reload edited in place are made
    /// of its new form: `now` gives it for the old one.
    pub(crate) fn singletons_edited(
        &self,
        now: impl Fn(&Rc<LoadedFile>) -> Option<Rc<LoadedFile>>,
    ) {
        let all = self.extension::<Singletons>();
        let sources = all.sources.get();
        for e in all.entries.borrow_mut().iter_mut() {
            if let State::Made {
                file: Some(file),
                seen,
                ..
            } = &mut e.state
                && let Some(new) = now(file)
            {
                *file = new;
                *seen = sources;
            }
        }
    }

    /// What stands for the singleton `ty`, read by a script of `ctx`, in
    /// that script: the number `__ljos_single` is called with.
    pub(crate) fn singleton_handle(&self, ty: &TypeRef, ctx: Option<ContextId>) -> usize {
        let all = self.extension::<Singletons>();
        let found = all.index.borrow().get(ty).copied();
        let base = match found {
            Some(i) => i,
            None => {
                let window = self.singleton_windowed(ty, &mut Vec::new());
                let mut entries = all.entries.borrow_mut();
                entries.push(Entry {
                    ty: ty.clone(),
                    scope: None,
                    window,
                    given: false,
                    state: State::Unmade,
                });
                let i = entries.len() - 1;
                all.index.borrow_mut().insert(ty.clone(), i);
                i
            }
        };
        if !all.entries.borrow()[base].window {
            return base;
        }
        let Some(scope) = ctx.and_then(|c| self.singleton_scope_at(c)) else {
            return base;
        };
        self.scoped_handle(base, scope)
    }

    /// Whether `ty` is one for each window: a native type registered so,
    /// or a QML singleton whose own scripts read one, directly or through
    /// another QML singleton. Qt makes every QML singleton again for each
    /// engine, and an application wanting one of each for every window
    /// makes an engine for each; here only those that read a window's
    /// object are, so the rest stay shared.
    fn singleton_windowed(&self, ty: &TypeRef, seen: &mut Vec<String>) -> bool {
        match self.singleton_kind(ty) {
            Kind::Native(native) => native.scope == SingletonScope::Window,
            Kind::File(key) => {
                if seen.contains(&key) {
                    return false;
                }
                seen.push(key.clone());
                let Ok(file) = self.load_file(&key) else {
                    return false;
                };
                file.ir
                    .exprs
                    .iter()
                    .flat_map(|b| &b.params)
                    .any(|p| match singleton_read(&p.arg) {
                        Some(Read::Type(t)) => self.singleton_windowed(t, seen),
                        // A name the compiler left to the run time
                        // ([`Engine::singleton_named`]).
                        Some(Read::Named(name)) => {
                            let all = self.extension::<Singletons>();
                            let natives = all.natives.borrow();
                            natives.iter().any(|((_, n), native)| {
                                n == name && native.scope == SingletonScope::Window
                            })
                        }
                        None => false,
                    })
            }
            Kind::Keys(_) => false,
        }
    }

    /// The entry of the type whose entry without a scope is `base`, in
    /// `scope`.
    fn scoped_handle(&self, base: usize, scope: ScopeId) -> usize {
        let all = self.extension::<Singletons>();
        if let Some(i) = all.scoped.borrow().get(&(base, scope)) {
            return *i;
        }
        let mut entries = all.entries.borrow_mut();
        let ty = entries[base].ty.clone();
        entries.push(Entry {
            ty,
            scope: Some(scope),
            window: true,
            given: false,
            state: State::Unmade,
        });
        let i = entries.len() - 1;
        all.scoped.borrow_mut().insert((base, scope), i);
        if let Some(o) = all.scopes.borrow_mut().owners.get_mut(&scope) {
            o.entries.push(i);
        }
        i
    }

    /// `__ljos_single(handle)`: the object, or for a type with none the
    /// keys of its enums.
    pub(crate) fn singleton_js(&self, handle: usize) -> JsValue {
        if self.extension::<Singletons>().entries.borrow().len() <= handle {
            return JsValue::Undefined;
        }
        self.singleton_value_at(handle)
    }

    /// The singleton `ty`, read by a script of `ctx`, as the script has it
    /// once it has asked.
    pub(crate) fn singleton_value_js(&self, ty: &TypeRef, ctx: ContextId) -> JsValue {
        let handle = self.singleton_handle(ty, Some(ctx));
        self.singleton_value_at(handle)
    }

    fn singleton_value_at(&self, handle: usize) -> JsValue {
        if let Some(o) = self.singleton_made(handle) {
            return self.to_js(&Value::Object(o));
        }
        let all = self.extension::<Singletons>();
        let entries = all.entries.borrow();
        match &entries[handle].state {
            State::Keys(Some(meta)) => enum_keys(meta),
            // What a read of a type with no object gives a script that
            // tests for one first (`Document ? Document.title : ""`).
            State::Outside => JsValue::Null,
            _ => JsValue::Undefined,
        }
    }

    /// The native singleton type `ty` names.
    fn singleton_native(&self, ty: &TypeRef) -> Option<Native> {
        let TypeRef::Native { module, name } = ty else {
            return None;
        };
        let all = self.extension::<Singletons>();
        let natives = all.natives.borrow();
        natives
            .get(&(module.clone(), name.clone()))
            .or_else(|| {
                // The compiler may know the type by another module than
                // the one it was registered in.
                natives
                    .iter()
                    .find_map(|((_, n), m)| (n == name).then_some(m))
            })
            .cloned()
    }

    fn singleton_kind(&self, ty: &TypeRef) -> Kind {
        match ty {
            TypeRef::Component { file, inline: None } => Kind::File(file.clone()),
            TypeRef::Native { module, name } => {
                if let Some(native) = self.singleton_native(ty) {
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

    /// The object the singleton `ty` is for a script of `ctx`, made now if
    /// it was not. `None` when the engine has no object for the type, when
    /// it failed to load (reported once) and for a type that is one for
    /// each window where there is none. While it is being made its own
    /// bindings get the object as far as it is made, which is what Qt
    /// gives them.
    pub(crate) fn singleton_object(
        &self,
        ty: &TypeRef,
        ctx: Option<ContextId>,
    ) -> Option<ObjectId> {
        let handle = self.singleton_handle(ty, ctx);
        self.singleton_made(handle)
    }

    /// The object of the entry `handle`, made now if it was not.
    fn singleton_made(&self, handle: usize) -> Option<ObjectId> {
        let all = self.extension::<Singletons>();
        let sources = all.sources.get();
        // The object as it stands, and whether its file has to be looked
        // at again.
        let (ty, scope) = {
            let entries = all.entries.borrow();
            let e = &entries[handle];
            let stands = match &e.state {
                State::Made { obj, file, seen } if self.is_alive(*obj) => {
                    if file.is_none() || *seen == sources {
                        return Some(*obj);
                    }
                    Some((*obj, file.clone()))
                }
                State::Making(obj) => return *obj,
                State::Failed | State::Keys(_) | State::Outside | State::Gone => return None,
                State::Made { .. } | State::Unmade => None,
            };
            if e.window && e.scope.is_none() {
                let name = type_name(&e.ty).to_owned();
                drop(entries);
                self.warn(format!(
                    "the singleton {name} is one for each window, and nothing here is under one"
                ));
                all.entries.borrow_mut()[handle].state = State::Outside;
                return None;
            }
            ((e.ty.clone(), stands), e.scope)
        };
        let (ty, stands) = ty;
        let kind = self.singleton_kind(&ty);
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
                    // A window's own: what its scripts read is the window's.
                    if let Some(s) = scope
                        && let Some(c) = self.context_of(obj)
                    {
                        let mut scopes = all.scopes.borrow_mut();
                        scopes.by_context.insert(c, s);
                        scopes.members.insert(obj, (c, s));
                    }
                };
                // Not a root of a window, whoever reads it first.
                all.unscoped.set(all.unscoped.get() + 1);
                let made = self.create_from_template_noting(&template, None, Vec::new(), allocated);
                all.unscoped.set(all.unscoped.get() - 1);
                made.map(|obj| (obj, Some(file)))
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
                    type_name(&ty)
                ));
                (State::Failed, None)
            }
        };
        all.entries.borrow_mut()[handle].state = state;
        if let (Some(obj), Some(scope), Kind::Native(native)) = (out, scope, &kind) {
            let root = all.scopes.borrow().owners.get(&scope).and_then(|o| o.root);
            if let Some(root) = root {
                self.tell_scoped(&ScopedSingleton {
                    event: ScopedEvent::Made,
                    root,
                    object: obj,
                    meta: native.meta,
                });
            }
        }
        out
    }

    fn tell_scoped(&self, what: &ScopedSingleton) {
        let listeners = self.extension::<Singletons>().listeners.borrow().clone();
        for l in listeners {
            l(self, what);
        }
    }

    /// The scope a script of `ctx` reads in, if any.
    fn singleton_scope_at(&self, ctx: ContextId) -> Option<ScopeId> {
        let all = self.extension::<Singletons>();
        let scopes = all.scopes.borrow();
        if scopes.by_context.is_empty() {
            return None;
        }
        let mut at = Some(ctx);
        while let Some(c) = at {
            if let Some(s) = scopes.by_context.get(&c) {
                // A root a script made outlives the window it was made in;
                // it reads in none after that.
                return scopes.owners.contains_key(s).then_some(*s);
            }
            at = self.context_parent(c);
        }
        None
    }

    /// Runs `f`, a creation the host asked for: the root it makes with no
    /// parent object opens a scope.
    pub(crate) fn as_host_root<R>(&self, f: impl FnOnce() -> R) -> R {
        let all = self.extension::<Singletons>();
        let before = all.host.replace(true);
        let r = f();
        all.host.set(before);
        r
    }

    /// The instance `inst` of a template, made under `parent`, now has
    /// all its objects: its root takes its place in a scope, when it is a
    /// root of a file in no other instance.
    pub(super) fn singleton_scope_enter(&self, inst: &Instance, parent: Option<ObjectId>) {
        let all = self.extension::<Singletons>();
        let host = all.host.replace(false);
        if all.unscoped.get() > 0 || self.context_parent(inst.context) != Some(self.root_context())
        {
            return;
        }
        let Some(&root) = inst.objects.first() else {
            return;
        };
        let scope = match parent {
            Some(p) => self.context_of(p).and_then(|c| self.singleton_scope_at(c)),
            None if !host => match self.calling_context() {
                Some(c) => self.singleton_scope_at(c),
                None => Some(self.singleton_scope_open(Some(root))),
            },
            None => match all.adopt.take() {
                Some(s) => {
                    self.singleton_scope_taken(s, root);
                    Some(s)
                }
                None => Some(self.singleton_scope_open(Some(root))),
            },
        };
        if let Some(s) = scope {
            let mut scopes = all.scopes.borrow_mut();
            scopes.by_context.insert(inst.context, s);
            scopes.members.insert(root, (inst.context, s));
        }
    }

    fn singleton_scope_open(&self, root: Option<ObjectId>) -> ScopeId {
        let all = self.extension::<Singletons>();
        let mut scopes = all.scopes.borrow_mut();
        let s = scopes.next;
        scopes.next += 1;
        scopes.owners.insert(
            s,
            Owner {
                root,
                entries: Vec::new(),
            },
        );
        s
    }

    /// A reload is about to make `old` again: the root it makes opens no
    /// scope of its own but takes over the one `old` owns.
    pub(super) fn singleton_scope_adopt(&self, old: ObjectId) {
        let all = self.extension::<Singletons>();
        let scope = {
            let scopes = all.scopes.borrow();
            scopes
                .members
                .get(&old)
                .map(|(_, s)| *s)
                .filter(|s| scopes.owners.get(s).is_some_and(|o| o.root == Some(old)))
        };
        all.adopt.set(scope);
    }

    /// The reload has made its roots, or failed to.
    pub(super) fn singleton_scope_adopt_none(&self) {
        self.extension::<Singletons>().adopt.set(None);
    }

    /// The reload is done with a root: the scope is the new root's.
    pub(super) fn singleton_scope_moved(&self, old: ObjectId, new: ObjectId) {
        let all = self.extension::<Singletons>();
        let mut scopes = all.scopes.borrow_mut();
        for o in scopes.owners.values_mut() {
            if o.root == Some(old) {
                o.root = Some(new);
            }
        }
    }

    /// `order` is being destroyed, and still stands: the scopes owned by
    /// any of them end. Their objects are told to the listeners now and
    /// given back, to be destroyed once `order` is gone.
    pub(crate) fn singleton_scopes_end(&self, order: &[ObjectId]) -> Vec<ObjectId> {
        let all = self.extension::<Singletons>();
        let ended: Vec<(Option<ObjectId>, Owner)> = {
            let mut scopes = all.scopes.borrow_mut();
            if scopes.members.is_empty() {
                return Vec::new();
            }
            let mut ended = Vec::new();
            for id in order {
                let Some((ctx, s)) = scopes.members.remove(id) else {
                    continue;
                };
                scopes.by_context.remove(&ctx);
                if scopes.owners.get(&s).is_some_and(|o| o.root == Some(*id)) {
                    let owner = scopes.owners.remove(&s).expect("looked up above");
                    ended.push((Some(*id), owner));
                }
            }
            ended
        };
        let (objects, told) = self.singleton_scopes_close(&ended);
        for (root, obj, ty) in told {
            if let Some(meta) = self.singleton_native(&ty).map(|n| n.meta) {
                self.tell_scoped(&ScopedSingleton {
                    event: ScopedEvent::Dropped,
                    root,
                    object: obj,
                    meta,
                });
            }
        }
        objects
    }

    /// Marks the entries of `ended` gone. Gives back the objects to destroy
    /// (those the engine made) and those to tell the listeners of (those
    /// of a scope a root owned).
    #[allow(clippy::type_complexity)]
    fn singleton_scopes_close(
        &self,
        ended: &[(Option<ObjectId>, Owner)],
    ) -> (Vec<ObjectId>, Vec<(ObjectId, ObjectId, TypeRef)>) {
        let all = self.extension::<Singletons>();
        let mut objects = Vec::new();
        let mut told = Vec::new();
        let mut entries = all.entries.borrow_mut();
        let mut scoped = all.scoped.borrow_mut();
        for (root, owner) in ended {
            for &i in &owner.entries {
                let e = &mut entries[i];
                if let State::Made { obj, .. } = e.state
                    && self.is_alive(obj)
                {
                    if !e.given {
                        objects.push(obj);
                    }
                    if let Some(root) = root {
                        told.push((*root, obj, e.ty.clone()));
                    }
                }
                e.state = State::Gone;
            }
            scoped.retain(|_, i| !owner.entries.contains(i));
        }
        (objects, told)
    }

    /// A root took the scope `s`: if the host opened it, the root owns it
    /// now, and the listeners hear of what the scope made before it.
    fn singleton_scope_taken(&self, s: ScopeId, root: ObjectId) {
        let all = self.extension::<Singletons>();
        let made: Vec<(ObjectId, TypeRef)> = {
            let mut scopes = all.scopes.borrow_mut();
            let Some(owner) = scopes.owners.get_mut(&s) else {
                return;
            };
            if owner.root.is_some() {
                return;
            }
            owner.root = Some(root);
            let entries = all.entries.borrow();
            owner
                .entries
                .iter()
                .filter_map(|&i| match entries[i].state {
                    State::Made { obj, .. } if !entries[i].given => {
                        Some((obj, entries[i].ty.clone()))
                    }
                    _ => None,
                })
                .collect()
        };
        for (object, ty) in made {
            if let Some(meta) = self.singleton_native(&ty).map(|n| n.meta) {
                self.tell_scoped(&ScopedSingleton {
                    event: ScopedEvent::Made,
                    root,
                    object,
                    meta,
                });
            }
        }
    }

    /// Opens a window's scope before its root, for a host that makes the
    /// window's objects of [`SingletonScope::Window`] types itself
    /// ([`Engine::set_singleton_in`]), or reads them with no QML at all
    /// ([`Engine::singleton_in_scope`]). The first root the host makes
    /// inside [`Engine::in_window_scope`] owns it from then on, and its
    /// end ends the scope; until then [`Engine::close_window_scope`] does.
    ///
    /// ```
    /// use ljos_engine::{Engine, QmlObject, Version, qml_singleton};
    ///
    /// #[derive(Default, QmlObject)]
    /// #[qml(module = "Example", name = "Document")]
    /// #[qml_singleton(scope = "window")]
    /// pub struct Document {
    ///     #[qml(notify)]
    ///     title: String,
    /// }
    ///
    /// let engine = Engine::new();
    /// engine.register_singleton::<Document>("Example", Version::new(1, 0));
    /// let made = engine
    ///     .create_native(Document { title: "first".into() }, None)
    ///     .unwrap();
    /// let window = engine.window_scope();
    /// engine.set_singleton_in::<Document>(window, made).unwrap();
    /// assert_eq!(engine.singleton_in_scope::<Document>(window), Ok(made));
    /// // A root made in `engine.in_window_scope(window, || ...)` reads `made`.
    /// ```
    pub fn window_scope(&self) -> WindowScope {
        WindowScope(self.singleton_scope_open(None))
    }

    /// Gives the window `scope` the host's own `obj` as its `T`, which
    /// everything under the window reads by the type's name. The window
    /// does not destroy it when it goes; the host does. An error when `T`
    /// is not one for each window, `obj` is not a `T`, the scope has ended,
    /// or it already has another `T`.
    pub fn set_singleton_in<T: QmlSingleton>(
        &self,
        scope: WindowScope,
        obj: ObjectId,
    ) -> Result<(), EngineError> {
        let meta = T::static_meta();
        if T::SCOPE != SingletonScope::Window {
            return Err(EngineError::Load(format!(
                "{} is one for the engine, not for each window",
                meta.name
            )));
        }
        if self.with::<T, _>(obj, |_| ()).is_err() {
            return Err(EngineError::Load(format!("{obj:?} is not a {}", meta.name)));
        }
        let handle = self.scope_handle::<T>(scope)?;
        let all = self.extension::<Singletons>();
        let mut entries = all.entries.borrow_mut();
        let e = &mut entries[handle];
        match e.state {
            State::Made { obj: now, .. } if now == obj => return Ok(()),
            State::Made { obj: now, .. } if self.is_alive(now) => {
                return Err(EngineError::Load(format!(
                    "the window already has its {}",
                    meta.name
                )));
            }
            State::Making(_) => {
                return Err(EngineError::Load(format!(
                    "the window's {} is being made",
                    meta.name
                )));
            }
            _ => {}
        }
        e.given = true;
        e.state = State::Made {
            obj,
            file: None,
            seen: all.sources.get(),
        };
        Ok(())
    }

    /// The `T` of the window `scope`: the object the host gave it, or one
    /// made now. For a window with no QML, such as in a test.
    pub fn singleton_in_scope<T: QmlSingleton>(
        &self,
        scope: WindowScope,
    ) -> Result<ObjectId, EngineError> {
        if T::SCOPE != SingletonScope::Window {
            return self.singleton::<T>();
        }
        let handle = self.scope_handle::<T>(scope)?;
        self.singleton_made(handle).ok_or_else(|| {
            EngineError::Load(format!(
                "{}: the singleton was not made",
                T::static_meta().name
            ))
        })
    }

    /// Runs `f`, in which the host makes the root of the window `scope`:
    /// the first root it makes with no parent takes the scope instead of
    /// opening one of its own. Any creation call can be in `f`.
    pub fn in_window_scope<R>(&self, scope: WindowScope, f: impl FnOnce() -> R) -> R {
        let all = self.extension::<Singletons>();
        let before = all.adopt.replace(Some(scope.0));
        let r = f();
        all.adopt.set(before);
        r
    }

    /// Ends a window's scope that no root has taken: the objects it made
    /// are destroyed and those the host gave are left standing. A scope a
    /// root owns ends with the root.
    pub fn close_window_scope(&self, scope: WindowScope) -> Result<(), EngineError> {
        let all = self.extension::<Singletons>();
        let owner = {
            let mut scopes = all.scopes.borrow_mut();
            match scopes.owners.get(&scope.0) {
                None => return Ok(()),
                Some(o) if o.root.is_some() => {
                    return Err(EngineError::Load(
                        "a root owns the window: destroy the root".into(),
                    ));
                }
                Some(_) => scopes.owners.remove(&scope.0).expect("looked up above"),
            }
        };
        let (objects, _) = self.singleton_scopes_close(&[(None, owner)]);
        for obj in objects {
            self.destroy(obj);
        }
        Ok(())
    }

    /// The entry of `T` in the window `scope`, which stands.
    fn scope_handle<T: QmlSingleton>(&self, scope: WindowScope) -> Result<usize, EngineError> {
        let all = self.extension::<Singletons>();
        if !all.scopes.borrow().owners.contains_key(&scope.0) {
            return Err(EngineError::Load("the window has gone".into()));
        }
        let ty = self.singleton_type::<T>()?;
        let base = self.singleton_handle(&ty, None);
        Ok(self.scoped_handle(base, scope.0))
    }
}

/// What may be a singleton in what a script is given.
enum Read<'a> {
    Type(&'a TypeRef),
    Named(&'a str),
}

/// The singleton type `arg` gives a script, or the name it is looked up
/// by when the program runs.
fn singleton_read(arg: &ljos_ir::Arg) -> Option<Read<'_>> {
    match arg {
        ljos_ir::Arg::Singleton(t) => Some(Read::Type(t)),
        ljos_ir::Arg::Dynamic(name) => Some(Read::Named(name)),
        ljos_ir::Arg::Context {
            fallback: Some(f), ..
        } => singleton_read(f),
        _ => None,
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
