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
//! the compiled program is the same for both kinds. A reload makes the
//! root again in the scope the old one owned, so the window keeps its
//! objects.

use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_ir::TypeRef;
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
    root: ObjectId,
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

type Make = Rc<dyn Fn(&Engine) -> Box<dyn QmlObject>>;

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
        let meta = T::static_meta();
        self.register_type(uri, version, MetaRef::Static(meta));
        let all = self.extension::<Singletons>();
        let make: Make = Rc::new(move |engine| Box::new(make(engine)));
        all.natives.borrow_mut().insert(
            (uri.to_owned(), meta.name.to_string()),
            Native {
                meta,
                make,
                scope: T::SCOPE,
            },
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
    pub fn is_native_singleton(&self, uri: &str, name: &str) -> bool {
        let all = self.extension::<Singletons>();
        let natives = all.natives.borrow();
        natives.contains_key(&(uri.to_owned(), name.to_owned()))
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
        scopes.owners.get(&scope).map(|o| o.root)
    }

    /// The objects of the window `root` owns, of the types that are one
    /// for each window, as they were made.
    pub fn scoped_singletons(&self, root: ObjectId) -> Vec<ObjectId> {
        let all = self.extension::<Singletons>();
        let scopes = all.scopes.borrow();
        let Some(owner) = scopes.owners.values().find(|o| o.root == root) else {
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

    /// The files the engine loads from changed: a QML singleton is looked
    /// at again when it is next read, and made again if its file is not
    /// the one it was made from.
    pub(crate) fn singletons_sources_changed(&self) {
        let all = self.extension::<Singletons>();
        all.sources.set(all.sources.get() + 1);
    }

    /// What stands for the singleton `ty`, read by a script of `ctx`, in
    /// that script: the number `__ljos_single` is called with.
    pub(crate) fn singleton_handle(&self, ty: &TypeRef, ctx: Option<ContextId>) -> usize {
        let all = self.extension::<Singletons>();
        let found = all.index.borrow().get(ty).copied();
        let base = match found {
            Some(i) => i,
            None => {
                let window =
                    self.singleton_native(ty).map(|n| n.scope) == Some(SingletonScope::Window);
                let mut entries = all.entries.borrow_mut();
                entries.push(Entry {
                    ty: ty.clone(),
                    scope: None,
                    window,
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
        if let Some(i) = all.scoped.borrow().get(&(base, scope)) {
            return *i;
        }
        let mut entries = all.entries.borrow_mut();
        entries.push(Entry {
            ty: ty.clone(),
            scope: Some(scope),
            window: true,
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
                };
                // The engine's, whoever reads it first.
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
            let root = all.scopes.borrow().owners.get(&scope).map(|o| o.root);
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
                None => Some(self.singleton_scope_open(root)),
            },
            None => match all.adopt.take() {
                Some(s) => Some(s),
                None => Some(self.singleton_scope_open(root)),
            },
        };
        if let Some(s) = scope {
            let mut scopes = all.scopes.borrow_mut();
            scopes.by_context.insert(inst.context, s);
            scopes.members.insert(root, (inst.context, s));
        }
    }

    fn singleton_scope_open(&self, root: ObjectId) -> ScopeId {
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
                .filter(|s| scopes.owners.get(s).is_some_and(|o| o.root == old))
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
            if o.root == old {
                o.root = new;
            }
        }
    }

    /// `order` is being destroyed, and still stands: the scopes owned by
    /// any of them end. Their objects are told to the listeners now and
    /// given back, to be destroyed once `order` is gone.
    pub(crate) fn singleton_scopes_end(&self, order: &[ObjectId]) -> Vec<ObjectId> {
        let all = self.extension::<Singletons>();
        let ended: Vec<(ObjectId, Owner)> = {
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
                if scopes.owners.get(&s).is_some_and(|o| o.root == *id) {
                    let owner = scopes.owners.remove(&s).expect("looked up above");
                    ended.push((*id, owner));
                }
            }
            ended
        };
        let mut objects = Vec::new();
        let mut told = Vec::new();
        {
            let mut entries = all.entries.borrow_mut();
            let mut scoped = all.scoped.borrow_mut();
            for (root, owner) in &ended {
                for &i in &owner.entries {
                    let e = &mut entries[i];
                    if let State::Made { obj, .. } = e.state
                        && self.is_alive(obj)
                    {
                        objects.push(obj);
                        told.push((*root, obj, e.ty.clone()));
                    }
                    e.state = State::Gone;
                }
                scoped.retain(|_, i| !owner.entries.contains(i));
            }
        }
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
