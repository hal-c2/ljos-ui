//! The JS bridge (ARCHITECTURE.md §9.2): the engine's [`HostObjects`],
//! value conversion, and the globals every QML file sees (`console`, `Qt`,
//! `qsTr`, `String.prototype.arg`).
//!
//! The host is made on first use, so an engine that never runs JS (a test,
//! a tier-0-only UI) never starts QuickJS. Every JS run goes through
//! [`Engine::run_js`], which turns a failure into a Qt-style warning instead
//! of an error the caller has to route: a binding that throws keeps its
//! value, a handler that throws is skipped, as in Qt.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use ljos_ir::CompiledFile;
use ljos_js::rquickjs;
use ljos_js::{
    FnId, HostError, HostFn, HostObjects, HostTypeId, JsConfig, JsFault, JsHost, JsRef, JsValue,
    MethodDef, PropDef, Scope, SignalDef, Slice, TypeDef,
};
use slotmap::{Key, KeyData};

use crate::engine::{Engine, Inner};
use crate::error::EngineError;
use crate::ids::{ConnectionId, MethodIndex, ObjectId, PropIndex, SignalIndex};
use crate::log::LogLevel;
use crate::meta::{Declared, MetaRef, PropertyFlags};
use crate::types::TypeTag;
use crate::value::{Color, Font, JsHandle, Value};
use crate::value_faces;

/// `fileName` and `lineNumber` of an error, as Qt's engine gives them: the
/// file and line it was made at, read from its stack when asked. Not an
/// error's own, so `Object.keys` leaves them out; what a script assigns
/// is its own.
const ERROR_PLACE: &str = r#"(() => {
  const place = globalThis.__ljos_errorPlace;
  const at = (name, index) => Object.defineProperty(Error.prototype, name, {
    get() {
      const p = this instanceof Error ? place(String(this.stack)) : null;
      return p ? p[index] : undefined;
    },
    set(value) {
      Object.defineProperty(this, name, { value, writable: true, enumerable: true, configurable: true });
    },
    configurable: true,
  });
  at("fileName", 0);
  at("lineNumber", 1);
})();"#;

/// Property id of the synthetic `parent` (the visual parent) on types that
/// do not declare one.
const PARENT_ID: u32 = u32::MAX - 1;
/// Method id of `destroy()`.
const DESTROY_ID: u32 = u32::MAX;

pub(crate) fn obj_to_u64(o: ObjectId) -> u64 {
    o.data().as_ffi()
}

pub(crate) fn u64_to_obj(u: u64) -> ObjectId {
    KeyData::from_ffi(u).into()
}

/// A JS function connected to a signal from JS (`sig.connect(f)`), kept so
/// `sig.disconnect(f)` can find it.
struct JsConn {
    sender: ObjectId,
    signal: SignalIndex,
    func: JsRef,
    id: ConnectionId,
}

/// QML line of each generated function, per module, for error messages.
#[derive(Default)]
pub(crate) struct LineTable {
    modules: HashMap<String, ModuleLines>,
}

/// A module loaded for a file ([`JsState::loaded_module`]).
struct LoadedModule {
    source: String,
    name: String,
    fns: Vec<Option<FnId>>,
}

/// One module's place in its QML file.
struct ModuleLines {
    url: String,
    /// (First JS line of a function, its QML line).
    lines: Vec<(u32, u32)>,
    /// The file its source is in. A file loaded again after a reload lets
    /// the earlier one go: its lines stay, its source does not.
    file: Option<Source>,
}

enum Source {
    Qml(Arc<CompiledFile>),
    /// A script file's text as it runs, which keeps the file's lines.
    Script(Rc<str>),
}

impl LineTable {
    /// Reads the `// File.qml:L:C` comment the compiler writes above every
    /// function.
    pub(crate) fn add(&mut self, module: &str, url: &str, file: &Arc<CompiledFile>) {
        let source = &file.js.source;
        let mut lines = Vec::new();
        let all: Vec<&str> = source.lines().collect();
        for (i, l) in all.iter().enumerate() {
            let Some(rest) = l.strip_prefix("// ") else {
                continue;
            };
            let Some(pos) = rest.split_whitespace().next() else {
                continue;
            };
            let mut parts = pos.rsplitn(3, ':');
            let (Some(_col), Some(line), Some(_file)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if let Ok(line) = line.parse::<u32>() {
                // An expression is wrapped as `return (…);` on the line after
                // `export function …`; a block body keeps its own lines, its
                // `{` sharing the export line. Lines are 1-based.
                let wrapped = all
                    .get(i + 2)
                    .is_some_and(|b| b.trim_start().starts_with("return ("))
                    && all.get(i + 3).is_some_and(|c| c.trim() == "}");
                // A method (`// File.qml:L:C Type.name()`) is always
                // wrapped as `return (function name(…) {`, on however
                // many lines: its first line is the `return` line.
                let method = rest.ends_with("()")
                    && all
                        .get(i + 2)
                        .is_some_and(|b| b.trim_start().starts_with("return (function "));
                let first = if wrapped || method { i + 3 } else { i + 2 };
                // QuickJS may place a throw in a wrapped expression on
                // the `export function` line itself (`null.foo()`): that
                // line is the expression's too.
                if first != i + 2 {
                    lines.push(((i + 2) as u32, line));
                }
                lines.push((first as u32, line));
            }
        }
        for m in self.modules.values_mut() {
            if m.url == url {
                m.file = None;
            }
        }
        self.modules.insert(
            module.to_owned(),
            ModuleLines {
                url: url.to_owned(),
                lines,
                file: Some(Source::Qml(file.clone())),
            },
        );
    }

    /// A script file, run under its own address: its lines are the file's.
    pub(crate) fn add_script(&mut self, url: &str, source: &str) {
        if self
            .modules
            .get(url)
            .and_then(|m| m.file.as_ref())
            .is_some_and(|f| matches!(f, Source::Script(s) if **s == *source))
        {
            return;
        }
        self.modules.insert(
            url.to_owned(),
            ModuleLines {
                url: url.to_owned(),
                lines: vec![(1, 1)],
                file: Some(Source::Script(source.into())),
            },
        );
    }

    pub(crate) fn map(&self, module: &str, js_line: u32) -> Option<(String, u32)> {
        let ModuleLines { url, lines, .. } = self.modules.get(module)?;
        let i = lines.partition_point(|(js, _)| *js <= js_line);
        let (js, qml) = lines.get(i.checked_sub(1)?)?;
        Some((url.clone(), qml + js_line.saturating_sub(*js)))
    }

    /// Line `js_line` (1-based) of a module's source.
    pub(crate) fn line(&self, module: &str, js_line: u32) -> Option<String> {
        let source = match self.modules.get(module)?.file.as_ref()? {
            Source::Qml(file) => &*file.js.source,
            Source::Script(text) => text,
        };
        let i = usize::try_from(js_line.checked_sub(1)?).ok()?;
        source.lines().nth(i).map(str::to_owned)
    }
}

/// The engine's JS state: the host, the prototypes made for each type, and
/// connections made from JS.
pub(crate) struct JsState {
    pub(crate) host: JsHost,
    /// Prototype per type description; the `MetaRef` keeps the pointer the
    /// key is made from alive.
    types: RefCell<rustc_hash::FxHashMap<usize, (MetaRef, HostTypeId)>>,
    /// The number each type QML declares prints with, by description.
    numbers: RefCell<rustc_hash::FxHashMap<usize, usize>>,
    /// How many numbers were given: one let go of is not given again.
    numbered: Cell<usize>,
    conns: RefCell<Vec<JsConn>>,
    pub(crate) lines: Rc<RefCell<LineTable>>,
    /// The modules loaded for each file, by their source: (source, module
    /// name, exported functions).
    modules: RefCell<HashMap<String, Vec<LoadedModule>>>,
    /// The budget of one dispatch when the host asked for another than the
    /// configured one ([`Engine::set_js_dispatch_budget`]).
    budget: Cell<Option<Slice>>,
    /// The component instance whose script is running, for what a script
    /// makes in the place it runs in (incubator/dynamic.rs).
    pub(crate) calling: Cell<Option<crate::incubator::dynamic::Calling>>,
    /// The faces of `list<T>` properties scripts hold (list_faces.rs).
    pub(crate) lists: crate::list_faces::ListFaces,
    /// Whether the names devtools scripts use are defined: only once a
    /// tool runs one (devtools.rs), so an application no tool looks at
    /// does not pay for them.
    pub(crate) devtools: Cell<bool>,
}

impl JsState {
    /// The budget of one dispatch into JS.
    fn slice(&self) -> Slice {
        self.budget.get().unwrap_or(self.host.config().slice)
    }

    /// Whether a script may hold `obj`.
    pub(crate) fn seen(&self, obj: ObjectId) -> bool {
        self.host.has_wrapper(obj_to_u64(obj))
    }

    /// Forgets destroyed objects: their wrappers and JS connections.
    pub(crate) fn forget(&self, objects: &[ObjectId]) {
        for o in objects {
            self.host.drop_wrapper(obj_to_u64(*o));
        }
        self.conns
            .borrow_mut()
            .retain(|c| !objects.contains(&c.sender));
        self.lists.forget(objects);
    }

    /// The class an object of `meta` prints as, named as Qt's engine names
    /// it: the C++ class of a type the toolkit gives, `File_QMLTYPE_n` for
    /// a file's type, the name of an inline component, and
    /// `Base_QML_n` for an object that declares members of its own.
    fn printed(&self, meta: &MetaRef) -> String {
        match (meta.declared, &meta.base) {
            (Some(Declared::Inline), _) => meta.name.to_string(),
            (Some(Declared::File), _) => format!("{}_QMLTYPE_{}", meta.name, self.number(meta)),
            (Some(Declared::Object), Some(base)) => {
                format!("{}_QML_{}", self.printed(base), self.number(meta))
            }
            _ => qt_class(meta),
        }
    }

    /// A number of the type's own, for the names Qt's engine numbers.
    fn number(&self, meta: &MetaRef) -> usize {
        let key = std::ptr::from_ref::<crate::meta::MetaObject>(meta) as usize;
        let mut numbers = self.numbers.borrow_mut();
        let next = self.numbered.get();
        *numbers.entry(key).or_insert_with(|| {
            self.numbered.set(next + 1);
            next
        })
    }

    /// The module loaded for the file `key` when its script was `source`.
    pub(crate) fn loaded_module(
        &self,
        key: &str,
        source: &str,
    ) -> Option<(String, Vec<Option<FnId>>)> {
        let modules = self.modules.borrow();
        let found = modules.get(key)?.iter().find(|m| m.source == source)?;
        Some((found.name.clone(), found.fns.clone()))
    }

    /// Remembers the module loaded for `key` with `source`.
    pub(crate) fn keep_module(&self, key: &str, source: &str, name: &str, fns: &[Option<FnId>]) {
        self.modules
            .borrow_mut()
            .entry(key.to_owned())
            .or_default()
            .push(LoadedModule {
                source: source.to_owned(),
                name: name.to_owned(),
                fns: fns.to_vec(),
            });
    }

    /// Lets go of the prototypes of the type descriptions only this cache
    /// still holds: those of the files a hot reload replaced. An object
    /// holds its description, so no object is left to be given one of
    /// them, and its address may be another description's next.
    pub(crate) fn release_unused_types(&self) {
        loop {
            let mut gone = Vec::new();
            // A description holds its base: the base goes in a later round.
            self.types.borrow_mut().retain(|key, (meta, id)| {
                let held = match meta {
                    MetaRef::Static(_) => true,
                    MetaRef::Shared(m) => Arc::strong_count(m) > 1,
                };
                if !held {
                    gone.push((*key, *id));
                }
                held
            });
            if gone.is_empty() {
                return;
            }
            let mut numbers = self.numbers.borrow_mut();
            for (key, id) in gone {
                self.host.unregister_type(id);
                numbers.remove(&key);
            }
        }
    }

    fn type_id(&self, engine: &Engine, meta: &MetaRef) -> Result<HostTypeId, JsFault> {
        let key = std::ptr::from_ref::<crate::meta::MetaObject>(meta) as usize;
        if let Some((_, id)) = self.types.borrow().get(&key) {
            return Ok(*id);
        }
        let mut def = type_def(engine, meta);
        def.name = self.printed(meta);
        let id = self.host.register_type(&def)?;
        self.types.borrow_mut().insert(key, (meta.clone(), id));
        Ok(id)
    }
}

impl Engine {
    /// The class an object of `meta` prints as ([`JsState::printed`]).
    pub(crate) fn printed_class(&self, meta: &MetaRef) -> String {
        match self.inner.js.borrow().as_ref() {
            Some(js) => js.printed(meta),
            None => qt_class(meta),
        }
    }
}

/// The C++ class Qt's engine gives a type of its toolkit, by the rule its
/// classes follow, `QQuick` or `QQml` before the QML name, and the few
/// that do not. A type of no Qt module is its own name.
fn qt_class(meta: &crate::meta::MetaObject) -> String {
    let name = meta.name.as_str();
    let module = meta.module.as_ref().map_or("", |m| m.as_str());
    match name {
        "QtObject" => return "QObject".into(),
        "Window" if module.starts_with("QtQuick") => return "QQuickWindowQmlImpl".into(),
        "Binding" if module == "QtQml" => return "QQmlBind".into(),
        "WorkerScript" => return "QQuickWorkerScript".into(),
        _ => {}
    }
    if module.starts_with("QtQuick") {
        format!("QQuick{name}")
    } else if module.starts_with("QtQml") {
        format!("QQml{name}")
    } else {
        name.to_owned()
    }
}

/// The JS shape of a type. Every property is an accessor, dotted grouped
/// leaves included (`obj["border.width"]`), so the group proxy can reach
/// them; a later property of the same name shadows an earlier one.
fn type_def(_engine: &Engine, meta: &MetaRef) -> TypeDef {
    let mut props: Vec<PropDef> = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (i, p) in meta.properties.iter().enumerate() {
        let def = PropDef {
            name: p.name.to_string(),
            id: i as u32,
            writable: !p.flags.contains(PropertyFlags::GROUP),
        };
        match seen.get(p.name.as_str()) {
            Some(at) => props[*at] = def,
            None => {
                seen.insert(p.name.as_str(), props.len());
                props.push(def);
            }
        }
    }
    let is_item = {
        let mut cur = Some(meta);
        let mut found = false;
        while let Some(m) = cur {
            if m.name == "Item" {
                found = true;
                break;
            }
            cur = m.base.as_ref();
        }
        found
    };
    if !seen.contains_key("parent") && is_item {
        props.push(PropDef {
            name: "parent".into(),
            id: PARENT_ID,
            writable: false,
        });
    }
    let mut methods: Vec<MethodDef> = Vec::new();
    let mut seen_m: HashMap<&str, usize> = HashMap::new();
    for (i, m) in meta.methods.iter().enumerate() {
        let def = MethodDef {
            name: m.name.to_string(),
            id: i as u32,
        };
        match seen_m.get(m.name.as_str()) {
            Some(at) => methods[*at] = def,
            None => {
                seen_m.insert(m.name.as_str(), methods.len());
                methods.push(def);
            }
        }
    }
    if !seen_m.contains_key("destroy") {
        methods.push(MethodDef {
            name: "destroy".into(),
            id: DESTROY_ID,
        });
    }
    let mut signals: Vec<SignalDef> = Vec::new();
    let mut seen_s: HashMap<&str, usize> = HashMap::new();
    for (i, s) in meta.signals.iter().enumerate() {
        if seen.contains_key(s.name.as_str()) || seen_m.contains_key(s.name.as_str()) {
            continue;
        }
        let def = SignalDef {
            name: s.name.to_string(),
            id: i as u32,
        };
        match seen_s.get(s.name.as_str()) {
            Some(at) => signals[*at] = def,
            None => {
                seen_s.insert(s.name.as_str(), signals.len());
                signals.push(def);
            }
        }
    }
    TypeDef {
        name: meta.name.to_string(),
        props,
        methods,
        signals,
        plain: meta.plain_object,
    }
}

/// The engine side of the host-object seam. Holds the engine weakly: the
/// engine owns the host that owns this.
struct EngineHost {
    inner: Weak<Inner>,
}

impl EngineHost {
    fn engine(&self) -> Result<Engine, HostError> {
        self.inner
            .upgrade()
            .map(Engine::from_inner)
            .ok_or_else(|| HostError::msg("the engine is gone"))
    }
}

fn host_err(e: EngineError) -> HostError {
    HostError::msg(e.to_string())
}

impl HostObjects for EngineHost {
    /// A `var` property holds the script's own array (script_arrays.rs).
    fn arrays_by_reference(&self) -> bool {
        true
    }

    fn type_of(&self, obj: u64) -> Option<HostTypeId> {
        let engine = self.engine().ok()?;
        let meta = engine.meta_of(u64_to_obj(obj))?;
        let js = engine.inner.js.borrow().clone()?;
        js.type_id(&engine, &meta).ok()
    }

    fn get<'js>(&self, scope: &Scope<'js>, obj: u64, prop: u32) -> Result<JsValue, HostError> {
        let engine = self.engine()?;
        let (id, prop) = forwarded_prop(&engine, u64_to_obj(obj), prop);
        if prop == PARENT_ID {
            return Ok(match engine.visual_parent(id) {
                Some(p) => engine.to_js(&Value::Object(p)),
                None => JsValue::Null,
            });
        }
        let p = PropIndex(prop);
        if let Some((owner, name)) = engine.group_at(id, p) {
            return group_proxy(scope, &engine, owner, &name).map_err(HostError::from);
        }
        let read = engine.try_read(id, p);
        // A `list<T>` declared in QML is read as the property's own list
        // (list_faces.rs). The read above is what a binding follows.
        if read.is_ok()
            && let Some((owner, list)) = engine.script_list(id, p)
            && let Some(js) = engine.inner.js.borrow().clone()
        {
            return engine
                .list_face(scope, &js, owner, list)
                .map_err(HostError::from);
        }
        match read {
            Ok(v) => Ok(engine.to_js(&v)),
            Err(EngineError::DeadObject(_)) => Ok(JsValue::Undefined),
            Err(e) => Err(host_err(e)),
        }
    }

    fn set<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        prop: u32,
        value: JsValue,
    ) -> Result<(), HostError> {
        let engine = self.engine()?;
        let (id, prop) = forwarded_prop(&engine, u64_to_obj(obj), prop);
        let p = PropIndex(prop);
        let ty = {
            let store = engine.inner.store.borrow();
            let h = store
                .objects
                .get(id)
                .ok_or_else(|| HostError::msg("object no longer exists"))?;
            h.meta
                .property(p)
                .map(|i| i.ty.clone())
                .unwrap_or(TypeTag::Var)
        };
        if let JsValue::Handle(r) = &value
            && let Ok(JsValue::Handle(f)) = scope.get_field(r, "__ljosBinding")
            && scope.is_function(&f)
        {
            // `x = Qt.binding(f)`: a new binding with `this` the object.
            let (target, target_prop) = engine.redirect(id, p);
            let this = JsValue::Object(obj_to_u64(target));
            let hint = ty.clone();
            return engine
                .bind_fallible_checked(target, target_prop, move |e| {
                    e.run_js("Qt.binding", |s| {
                        let out = s.call_ref(&f, &this, &[])?;
                        Ok(e.js_to_value_kept(s, out, &hint))
                    })
                })
                .map_err(host_err);
        }
        // What Qt's engine calls the value when it is refused, where that
        // is not what the conversion calls it.
        let from = match &value {
            JsValue::Number(n) if n.fract() == 0.0 && n.abs() <= f64::from(i32::MAX) => Some("int"),
            JsValue::Null => Some("std::nullptr_t"),
            JsValue::Object(_) => Some("QObject*"),
            // No property but a var takes a function.
            JsValue::Handle(r)
                if !matches!(ty, TypeTag::Var | TypeTag::Data) && scope.is_function(r) =>
            {
                return Err(HostError::msg(format!(
                    "Cannot assign JavaScript function to {}",
                    engine.qt_type(&ty)
                )));
            }
            _ => None,
        };
        let v = engine.js_to_value_kept(scope, value, &ty);
        if engine.tier1_spying() {
            // Tier 1 would write a list where the script wrote its array.
            engine.tier1_spy(id, p, &engine.plain(&v));
        }
        engine
            .write(id, p, v, crate::WriteKind::Imperative)
            .map_err(|e| match e {
                // What a script catches, as Qt's engine throws it.
                EngineError::ReadOnly(name) => {
                    HostError::type_error(format!("Cannot assign to read-only property \"{name}\""))
                }
                // A binding's warning says "null" where a script is told
                // std::nullptr_t (Qt 6.11.2).
                EngineError::Convert(c) => HostError::msg(format!(
                    "Cannot assign {} to {}",
                    from.unwrap_or(&c.from),
                    // An enum is named as its type declares it.
                    match ty {
                        TypeTag::Enum(_) | TypeTag::Flags(_) => c.to,
                        _ => engine.qt_type(&ty),
                    }
                )),
                e => host_err(e),
            })?;
        if engine.inner.reload.tracking.get() {
            engine.reload_note_assigned(id, p);
        }
        Ok(())
    }

    fn call<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        method: u32,
        args: &[JsValue],
    ) -> Result<JsValue, HostError> {
        let engine = self.engine()?;
        let (id, method) = forwarded_method(&engine, u64_to_obj(obj), method);
        if method == DESTROY_ID {
            engine.destroy_later(
                id,
                args.first().and_then(|a| match a {
                    JsValue::Number(n) => Some(*n),
                    _ => None,
                }),
            );
            return Ok(JsValue::Undefined);
        }
        // A function declared in QML gets the caller's array; a native
        // method gets a list of it (`Engine::invoke`), and a copy of a
        // plain object where it takes data.
        let meta = engine.meta_of(id);
        let params = meta
            .as_ref()
            .and_then(|m| m.method(MethodIndex(method)))
            .map_or(&[][..], |m| &m.params[..]);
        let vals: Vec<Value> = args
            .iter()
            .enumerate()
            .map(|(i, a)| match params.get(i) {
                Some((_, ty @ TypeTag::Data)) => engine.js_to_value(scope, a.clone(), ty),
                _ => engine.js_to_value_kept(scope, a.clone(), &TypeTag::Var),
            })
            .collect();
        // An exception in a QML function called from JS is the caller's to
        // catch: it is left pending instead of being logged where it left
        // the function (see `Engine::eval_expr`).
        let faults = &engine.inner.faults;
        faults.thrown.set(false);
        faults.propagate.set(true);
        let out = engine.invoke(id, MethodIndex(method), &vals);
        faults.propagate.set(false);
        if faults.thrown.take() {
            return Err(HostError::Js(rquickjs::Error::Exception));
        }
        Ok(engine.to_js(&out.map_err(host_err)?))
    }

    fn emit<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        signal: u32,
        args: &[JsValue],
    ) -> Result<(), HostError> {
        let engine = self.engine()?;
        let id = u64_to_obj(obj);
        // A signal declared in QML carries the script's array to its
        // handlers; a native one tells native code, which reads lists.
        let script = engine
            .meta_of(id)
            .is_some_and(|m| m.signal_is_script(SignalIndex(signal)));
        let vals: Vec<Value> = args
            .iter()
            .map(|a| {
                if script {
                    // A list property sent with a signal is the entries
                    // it had (list_faces.rs).
                    match a {
                        JsValue::Handle(r) if engine.is_face(r) => {
                            engine.js_to_value(scope, a.clone(), &TypeTag::Var)
                        }
                        _ => engine.js_to_value_kept(scope, a.clone(), &TypeTag::Var),
                    }
                } else {
                    engine.js_to_value(scope, a.clone(), &TypeTag::Var)
                }
            })
            .collect();
        engine
            .emit(id, SignalIndex(signal), &vals)
            .map_err(host_err)
    }

    fn connect<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        signal: u32,
        args: &[JsValue],
    ) -> Result<(), HostError> {
        let engine = self.engine()?;
        let (this, func) = connect_args(scope, args)?;
        let sender = u64_to_obj(obj);
        let sig = SignalIndex(signal);
        let receiver = match &this {
            JsValue::Object(o) => Some(u64_to_obj(*o)),
            _ => None,
        };
        let f = func.clone();
        let handler = move |e: &Engine, args: &[Value]| {
            let args: Vec<JsValue> = args.iter().map(|a| e.to_js(a)).collect();
            // A fault in it is one of the object it was connected for.
            let _origin = e.origin(receiver.or(Some(sender)));
            e.run_js("signal handler", |s| s.call_ref(&f, &this, &args));
        };
        let id = match receiver {
            Some(r) => engine.connect_to(sender, sig, r, handler),
            None => engine.connect(sender, sig, handler),
        }
        .map_err(host_err)?;
        let js = engine.js().map_err(host_err)?;
        js.conns.borrow_mut().push(JsConn {
            sender,
            signal: sig,
            func,
            id,
        });
        Ok(())
    }

    fn disconnect<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        signal: u32,
        args: &[JsValue],
    ) -> Result<(), HostError> {
        let engine = self.engine()?;
        let (_, func) = connect_args(scope, args)?;
        let sender = u64_to_obj(obj);
        let js = engine.js().map_err(host_err)?;
        let found = {
            let conns = js.conns.borrow();
            conns
                .iter()
                .position(|c| {
                    c.sender == sender
                        && c.signal == SignalIndex(signal)
                        && scope.same_value(&c.func, &func)
                })
                .map(|i| (i, conns[i].id))
        };
        if let Some((i, id)) = found {
            js.conns.borrow_mut().remove(i);
            engine.disconnect(id);
        }
        Ok(())
    }

    fn released(&self, obj: u64) {
        self.released_many(&[obj]);
    }

    fn released_many(&self, objs: &[u64]) {
        if let Ok(engine) = self.engine() {
            engine.scripts_let_go(objs);
            engine.reload_let_go(objs);
        }
    }
}

/// A script that holds an object a dev reload replaced (a closure an
/// `onCompleted` handed to a singleton) reaches the object that went on
/// for it, its members named as the old type named them.
fn forwarded_prop(engine: &Engine, id: ObjectId, prop: u32) -> (ObjectId, u32) {
    let Some((new, names)) = engine.reload_forwarded(id) else {
        return (id, prop);
    };
    if prop >= PARENT_ID {
        return (new, prop);
    }
    let name = names.props.get(prop as usize);
    match name.and_then(|n| engine.property_index(new, n.as_str()).ok()) {
        Some(p) => (new, p.0),
        None => (id, prop),
    }
}

fn forwarded_method(engine: &Engine, id: ObjectId, method: u32) -> (ObjectId, u32) {
    let Some((new, names)) = engine.reload_forwarded(id) else {
        return (id, method);
    };
    if method >= PARENT_ID {
        return (new, method);
    }
    let name = names.methods.get(method as usize);
    match name.and_then(|n| engine.method_index(new, n.as_str()).ok()) {
        Some(m) => (new, m.0),
        None => (id, method),
    }
}

/// `connect(f)` or `connect(target, f)`.
fn connect_args(scope: &Scope<'_>, args: &[JsValue]) -> Result<(JsValue, JsRef), HostError> {
    let (this, f) = match args {
        [JsValue::Handle(f)] => (JsValue::Undefined, f),
        [this, JsValue::Handle(f), ..] => (this.clone(), f),
        _ => return Err(HostError::msg("connect: expected a function")),
    };
    if !scope.is_function(f) {
        return Err(HostError::msg("connect: expected a function"));
    }
    Ok((this, f.clone()))
}

/// `rect.border` from JS: an object whose accessors reach the flattened
/// `border.*` properties, so `rect.border.width = 2` writes through.
fn group_proxy<'js>(
    scope: &Scope<'js>,
    engine: &Engine,
    obj: ObjectId,
    name: &str,
) -> rquickjs::Result<JsValue> {
    let prefix = format!("{name}.");
    let leaves: Vec<JsValue> = engine
        .meta_of(obj)
        .map(|m| {
            m.properties
                .iter()
                .filter_map(|p| p.name.strip_prefix(prefix.as_str()))
                .map(|l| JsValue::String(l.to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let make: rquickjs::Function = scope.globals().get("__ljos_group")?;
    let wrapper = scope.wrapper(obj_to_u64(obj))?;
    let leaves = scope.to_value(&JsValue::Array(leaves))?;
    let v: rquickjs::Value = make.call((wrapper, prefix, leaves))?;
    scope.from_value(&v)
}

/// Whether `v` is or holds a script's own value.
pub(crate) fn holds_js(v: &Value) -> bool {
    match v {
        Value::Js(_) => true,
        Value::List(items) => items.iter().any(holds_js),
        Value::Map(fields) => fields.iter().any(|(_, v)| holds_js(v)),
        _ => false,
    }
}

/// How deep [`Engine::deep_copy`] goes: past it an object is left to the
/// script, which is where a cycle ends.
const DATA_DEPTH: usize = 64;

impl Engine {
    /// Makes `font`'s family, sizes, weight, italics and style name those
    /// of every font made from now on, on this engine's thread, over the
    /// process's ([`Font::set_application_font`]); with `None` the
    /// process's again. A `Qt.font({...})` and a new object start from it;
    /// what exists keeps its font. Held until the engine goes.
    pub fn set_default_font(&self, font: Option<&Font>) {
        self.inner.default_font.set(font);
        let js = self.inner.js.borrow().clone();
        if let Some(js) = js
            && let Err(e) = value_faces::font_base_changed(&js.host)
        {
            self.warn(format!("the font Qt.font starts from: {e}"));
        }
    }

    /// The JS host, started on first use.
    pub(crate) fn js(&self) -> Result<Rc<JsState>, EngineError> {
        if let Some(js) = self.inner.js.borrow().clone() {
            return Ok(js);
        }
        let host = JsHost::new(JsConfig::default()).map_err(|e| EngineError::Js(e.to_string()))?;
        host.set_host_objects(Rc::new(EngineHost {
            inner: Rc::downgrade(&self.inner),
        }));
        let lines = Rc::new(RefCell::new(LineTable::default()));
        let table = lines.clone();
        host.set_source_mapper(Some(Box::new(move |module, line| {
            table.borrow().map(module, line)
        })));
        let table = lines.clone();
        host.set_source_text(Some(Box::new(move |module, line| {
            table.borrow().line(module, line)
        })));
        let state = Rc::new(JsState {
            host,
            types: RefCell::default(),
            numbers: RefCell::default(),
            numbered: Cell::new(0),
            modules: RefCell::new(HashMap::new()),
            conns: RefCell::new(Vec::new()),
            lines,
            budget: Cell::new(None),
            calling: Cell::new(None),
            lists: crate::list_faces::ListFaces::default(),
            devtools: Cell::new(false),
        });
        *self.inner.js.borrow_mut() = Some(state.clone());
        self.install_globals(&state)?;
        self.run_js_started();
        Ok(state)
    }

    /// The time one dispatch into JS may take from now on: over `soft` it
    /// is logged, over `hard` the script is interrupted. A frame-driven
    /// host keeps the default; a test runner whose test functions wait for
    /// timers and animations inside one dispatch asks for more.
    pub fn set_js_dispatch_budget(
        &self,
        soft: std::time::Duration,
        hard: std::time::Duration,
    ) -> Result<(), EngineError> {
        self.js()?.budget.set(Some(Slice { soft, hard }));
        Ok(())
    }

    /// Whether the JS host has been started.
    pub fn js_started(&self) -> bool {
        self.inner.js.borrow().is_some()
    }

    /// Runs JS; a failure is logged as Qt logs it and gives `None`.
    pub(crate) fn run_js<R>(
        &self,
        label: &str,
        f: impl for<'js> FnOnce(&Scope<'js>) -> rquickjs::Result<R>,
    ) -> Option<R> {
        self.run_js_naming(label, String::new, f)
    }

    /// [`Engine::run_js`], with `what` naming the code for the warning a
    /// dispatch over its soft budget gives (`ljos_js::Host::enter_naming`).
    pub(crate) fn run_js_naming<R>(
        &self,
        label: &str,
        what: impl FnOnce() -> String,
        f: impl for<'js> FnOnce(&Scope<'js>) -> rquickjs::Result<R>,
    ) -> Option<R> {
        let js = match self.js() {
            Ok(js) => js,
            Err(e) => {
                self.warn(e.to_string());
                return None;
            }
        };
        let slice = js.slice();
        match js.host.enter_naming(label, what, slice, f) {
            Ok(v) => Some(v),
            // Nested in a dispatch that is now over: the dispatch is the
            // fault, and is reported when it unwinds.
            Err(JsFault::JsInterrupted { .. }) if js.host.interrupted() => None,
            Err(fault) => {
                self.report_js_fault(&fault);
                None
            }
        }
    }

    /// Whether the dispatch running now was interrupted inside a nested run.
    pub(crate) fn js_interrupted(&self) -> bool {
        self.js().is_ok_and(|js| js.host.interrupted())
    }

    /// Runs JS and gives its failure back, for a caller that reports it
    /// itself; nothing is logged.
    pub(crate) fn try_run_js<R>(
        &self,
        label: &str,
        f: impl for<'js> FnOnce(&Scope<'js>) -> rquickjs::Result<R>,
    ) -> Result<R, crate::JsError> {
        let js = self
            .js()
            .map_err(|e| crate::JsError::Failed(e.to_string()))?;
        let slice = js.slice();
        js.host
            .enter_naming(label, String::new, slice, f)
            .map_err(|fault| match fault {
                JsFault::JsException {
                    message,
                    stack,
                    file,
                    line,
                    thrown,
                } => crate::JsError::Thrown {
                    stack: self.js_trace(&stack).1,
                    value: thrown
                        .and_then(|v| {
                            self.run_js("thrown", |s| {
                                Ok(self.js_to_value_kept(s, v, &crate::TypeTag::Var))
                            })
                        })
                        .unwrap_or_default(),
                    message,
                    file,
                    line,
                },
                other => crate::JsError::Failed(other.to_string()),
            })
    }

    /// Evaluates a classic script in the global scope, for hosts and tests.
    /// Returns its completion value.
    pub fn eval_js(&self, source: &str) -> Result<Value, EngineError> {
        let js = self.js()?;
        let slice = js.slice();
        js.host
            .enter("eval", slice, |s| {
                let v: rquickjs::Value = s.ctx().eval(source)?;
                let v = s.from_value(&v)?;
                Ok(self.js_to_value(s, v, &TypeTag::Var))
            })
            .map_err(|e| EngineError::Js(e.to_string()))
    }

    /// An engine value as JS sees it. URLs are strings; a colour or another
    /// value type is the one object there is for that value, so it compares
    /// by value (see value_faces.rs); a destroyed object is `null`.
    pub(crate) fn to_js(&self, v: &Value) -> JsValue {
        match v {
            Value::Undefined => JsValue::Undefined,
            Value::Null => JsValue::Null,
            Value::Bool(b) => JsValue::Bool(*b),
            Value::Int(i) => JsValue::Number(f64::from(*i)),
            Value::Real(r) => JsValue::Number(*r),
            Value::String(s) | Value::Url(s) => JsValue::String(s.to_string()),
            Value::Color(c) => color_to_js(*c),
            Value::Point(_)
            | Value::IntPoint(_)
            | Value::Size(_)
            | Value::Rect(_)
            | Value::Font(_)
            | Value::Vector2d(_)
            | Value::Vector3d(_)
            | Value::Vector4d(_)
            | Value::Quaternion(_) => value_faces::to_js(v).unwrap_or_default(),
            Value::Object(id) if self.is_alive(*id) => JsValue::Object(obj_to_u64(*id)),
            Value::Object(_) => JsValue::Null,
            Value::List(items) => JsValue::Array(items.iter().map(|i| self.to_js(i)).collect()),
            Value::Map(fields) => {
                let make = || {
                    fields
                        .iter()
                        .map(|(k, v)| (k.to_string(), self.to_js(v)))
                        .collect()
                };
                match self
                    .extension::<crate::records::Records>()
                    .map(fields, make)
                {
                    Some(kept) => kept,
                    None => JsValue::Map(
                        fields
                            .iter()
                            .map(|(k, v)| (k.to_string(), self.to_js(v)))
                            .collect(),
                    ),
                }
            }
            Value::Enum(e) => JsValue::Number(f64::from(e.value)),
            Value::Js(h) => JsValue::Handle(h.0.clone()),
        }
    }

    /// A JS value as an engine value. `ty` is the type it is going to, so a
    /// plain `{x, y}` can become a `point`.
    pub(crate) fn js_to_value(&self, scope: &Scope<'_>, v: JsValue, ty: &TypeTag) -> Value {
        if matches!(ty, TypeTag::Data) {
            return self.js_to_data(scope, v, 0);
        }
        // A real takes what JavaScript makes a number of an object, as
        // qml6's does; a function is refused where it is written.
        if matches!(ty, TypeTag::Real) {
            match &v {
                JsValue::Object(_) | JsValue::Typed(..) => return Value::Real(f64::NAN),
                JsValue::Handle(r) if !scope.is_function(r) && !self.is_face(r) => {
                    return Value::Real(scope.number(&v));
                }
                _ => {}
            }
        }
        match v {
            JsValue::Undefined => Value::Undefined,
            JsValue::Null => Value::Null,
            JsValue::Bool(b) => Value::Bool(b),
            JsValue::Number(n) => Value::Real(n),
            JsValue::String(s) => Value::String(s.into()),
            JsValue::Array(items) => {
                let inner = match ty {
                    TypeTag::List(Some(_)) => TypeTag::Object(None),
                    _ => TypeTag::Var,
                };
                Value::list(
                    items
                        .into_iter()
                        .map(|i| self.js_to_value(scope, i, &inner))
                        .collect(),
                )
            }
            JsValue::Object(id) => Value::Object(u64_to_obj(id)),
            v @ (JsValue::Color(_) | JsValue::InvalidColor(_)) => Value::Color(color_from_js(&v)),
            // Whatever it is going to: a `var` keeps the value, and a
            // property of another type refuses it by its name.
            JsValue::Typed(kind, fields) => value_faces::from_js(kind, &fields),
            // The script's own array, going where a list is kept.
            JsValue::Handle(r) if r.is_array() => {
                let items = scope.array_items(&r).unwrap_or_default();
                self.js_to_value(scope, JsValue::Array(items), ty)
            }
            JsValue::Handle(r) => {
                // The face of a list property: the list it stands for.
                if let Some(list) = self.face_list(&r) {
                    return list;
                }
                let num = |k: &str| match scope.get_field(&r, k) {
                    Ok(JsValue::Number(n)) => n,
                    _ => 0.0,
                };
                match ty {
                    TypeTag::Point => Value::point(num("x"), num("y")),
                    TypeTag::Size => Value::size(num("width"), num("height")),
                    TypeTag::Rect => Value::rect(num("x"), num("y"), num("width"), num("height")),
                    TypeTag::Vector2d => Value::vector2d(num("x") as f32, num("y") as f32),
                    TypeTag::Vector3d => {
                        Value::vector3d(num("x") as f32, num("y") as f32, num("z") as f32)
                    }
                    TypeTag::Vector4d => Value::vector4d(
                        num("x") as f32,
                        num("y") as f32,
                        num("z") as f32,
                        num("w") as f32,
                    ),
                    TypeTag::Color => {
                        // A colour object (`{r, g, b, a}`) from a JS helper.
                        let a = match scope.get_field(&r, "a") {
                            Ok(JsValue::Number(n)) => n,
                            _ => 1.0,
                        };
                        Value::Color(Color::from_rgba_f(num("r"), num("g"), num("b"), a))
                    }
                    // A Date is its text; any other object a string refuses.
                    TypeTag::String => match scope.date_text(&r) {
                        Some(t) => Value::String(t.into()),
                        None => Value::Js(JsHandle(r)),
                    },
                    // A script's `URL` object: the address it holds.
                    TypeTag::Url => match scope.get_field(&r, "href") {
                        Ok(JsValue::String(s)) => Value::Url(s.into()),
                        _ => Value::Js(JsHandle(r)),
                    },
                    TypeTag::Font => {
                        let mut f = Font::default();
                        if let Ok(JsValue::String(s)) = scope.get_field(&r, "family") {
                            f.family = s.into();
                        }
                        if let Ok(JsValue::Number(n)) = scope.get_field(&r, "pointSize") {
                            f.point_size = n;
                        }
                        if let Ok(JsValue::Number(n)) = scope.get_field(&r, "pixelSize")
                            && n >= 1.0
                        {
                            f.pixel_size = n as i32;
                            f.point_size = -1.0;
                        }
                        if let Ok(JsValue::Bool(b)) = scope.get_field(&r, "bold") {
                            f.weight = if b { 700 } else { 400 };
                        }
                        if let Ok(JsValue::Bool(b)) = scope.get_field(&r, "italic") {
                            f.italic = b;
                        }
                        Value::Font(Arc::new(f))
                    }
                    _ => Value::Js(JsHandle(r)),
                }
            }
            JsValue::Map(fields) => {
                // Only made on the Rust side; keep it as a JS object.
                match scope
                    .to_value(&JsValue::Map(fields))
                    .and_then(|v| scope.from_value(&v))
                {
                    Ok(JsValue::Handle(r)) => Value::Js(JsHandle(r)),
                    _ => Value::Undefined,
                }
            }
        }
    }

    /// A JS value as data ([`TypeTag::Data`]): a plain object is copied
    /// into a [`Value::Map`] and an array into a [`Value::List`], all the
    /// way down. A function, and what lies deeper than [`DATA_DEPTH`] (a
    /// cycle), stays the script's own.
    fn js_to_data(&self, scope: &Scope<'_>, v: JsValue, depth: usize) -> Value {
        let deeper = |v| self.js_to_data(scope, v, depth + 1);
        match v {
            JsValue::Handle(r) if depth < DATA_DEPTH && !scope.is_function(&r) => {
                if let Some(list) = self.face_list(&r) {
                    return list;
                }
                if r.is_array() {
                    let items = scope.array_items(&r).unwrap_or_default();
                    Value::list(items.into_iter().map(deeper).collect())
                } else {
                    let fields = scope.entries(&r).unwrap_or_default();
                    Value::map(fields.into_iter().map(|(k, v)| (k, deeper(v))))
                }
            }
            JsValue::Array(items) => Value::list(items.into_iter().map(deeper).collect()),
            JsValue::Map(fields) => Value::map(fields.into_iter().map(|(k, v)| (k, deeper(v)))),
            v => self.js_to_value(scope, v, &TypeTag::Var),
        }
    }

    /// A copy of `v` as data: a script's plain objects become
    /// [`Value::Map`]s and its arrays [`Value::List`]s, all the way down,
    /// as an argument or property of a data type ([`TypeTag::Data`]:
    /// `Vec<T>`, `HashMap<String, T>`, `serde_json::Value`) receives them.
    /// For a `var` a Rust method was handed, which holds the script's own
    /// object.
    pub fn deep_copy(&self, v: &Value) -> Value {
        if !holds_js(v) {
            return v.clone();
        }
        match v {
            Value::Js(_) => {}
            Value::List(items) => {
                return Value::list(items.iter().map(|i| self.deep_copy(i)).collect());
            }
            Value::Map(fields) => {
                return Value::map(fields.iter().map(|(k, v)| (k.clone(), self.deep_copy(v))));
            }
            _ => return v.clone(),
        }
        let js = self.to_js(v);
        self.run_js("deep copy", |s| Ok(self.js_to_data(s, js, 0)))
            .unwrap_or_else(|| v.clone())
    }

    /// [`Engine::js_to_value`] for a value going where a script's array is
    /// kept as that array: a `var` property, an argument or the result of
    /// a function declared in QML (script_arrays.rs). A `var` of a native
    /// type is given a list of it when it is stored
    /// ([`Engine::for_slot`]).
    pub(crate) fn js_to_value_kept(&self, scope: &Scope<'_>, v: JsValue, ty: &TypeTag) -> Value {
        match v {
            JsValue::Handle(r)
                if matches!(ty, TypeTag::Var) && (r.is_array() || self.is_face(&r)) =>
            {
                Value::Js(JsHandle(r))
            }
            v => self.js_to_value(scope, v, ty),
        }
    }

    fn install_globals(&self, js: &JsState) -> Result<(), EngineError> {
        let weak = Rc::downgrade(&self.inner);
        let w = weak.clone();
        let log: HostFn = Rc::new(move |scope, args| {
            let Some(inner) = w.upgrade() else {
                return Ok(JsValue::Undefined);
            };
            let engine = Engine::from_inner(inner);
            let level = match args.first() {
                Some(JsValue::Number(n)) if *n >= 3.0 => LogLevel::Critical,
                Some(JsValue::Number(n)) if *n >= 2.0 => LogLevel::Warning,
                Some(JsValue::Number(n)) if *n >= 1.0 => LogLevel::Info,
                _ => LogLevel::Debug,
            };
            let text = args.get(1).map(|a| scope.display(a)).unwrap_or_default();
            engine.log(level, "qml", text);
            Ok(JsValue::Undefined)
        });
        let w = weak.clone();
        let call_later: HostFn = Rc::new(move |scope, args| {
            let Some(inner) = w.upgrade() else {
                return Ok(JsValue::Undefined);
            };
            let engine = Engine::from_inner(inner);
            let Some(JsValue::Handle(f)) = args.first() else {
                return Err(HostError::msg("Qt.callLater: expected a function"));
            };
            if !scope.is_function(f) {
                return Err(HostError::msg("Qt.callLater: expected a function"));
            }
            let rest = match args.get(1) {
                Some(JsValue::Array(a)) => a.clone(),
                _ => Vec::new(),
            };
            engine.call_later(scope, f.clone(), rest);
            Ok(JsValue::Undefined)
        });
        let w = weak.clone();
        let quit: HostFn = Rc::new(move |_scope, _args| {
            if let Some(inner) = w.upgrade() {
                Engine::from_inner(inner).request_quit();
            }
            Ok(JsValue::Undefined)
        });
        // The prelude has already brought the channels to sixteen bits.
        let color: HostFn = Rc::new(|_scope, args| {
            let ch = |i: usize| match args.get(i) {
                Some(JsValue::Number(n)) => n.clamp(0.0, 65535.0) as u16,
                _ => 0,
            };
            let channels = [ch(0), ch(1), ch(2), ch(3)];
            Ok(match args.get(4) {
                Some(JsValue::Bool(false)) => JsValue::InvalidColor(channels),
                _ => JsValue::Color(channels),
            })
        });
        // A colour by its name or `#rrggbb`; `null` for what is not one.
        let color_of: HostFn = Rc::new(|_scope, args| {
            Ok(match args.first() {
                Some(JsValue::String(s)) => Color::parse(s).map_or(JsValue::Null, color_to_js),
                _ => JsValue::Null,
            })
        });
        let w = weak;
        // What a type attached to an object, made on first use; `undefined`
        // for what is not an object of the engine's or a type that attaches.
        let attached: HostFn = Rc::new(move |_scope, args| {
            let (
                Some(inner),
                [
                    JsValue::Object(o),
                    JsValue::String(module),
                    JsValue::String(name),
                ],
            ) = (w.upgrade(), args)
            else {
                return Ok(JsValue::Undefined);
            };
            let engine = Engine::from_inner(inner);
            Ok(engine
                .attached_by_name(u64_to_obj(*o), module, name)
                .map_or(JsValue::Undefined, |a| engine.to_js(&Value::Object(a))))
        });
        let w = Rc::downgrade(&self.inner);
        // The object the name of a singleton is, made on the first call:
        // the script is given a number that stands for the type.
        let single: HostFn = Rc::new(move |_scope, args| {
            Ok(match (w.upgrade(), args.first()) {
                (Some(inner), Some(JsValue::Number(n))) => {
                    Engine::from_inner(inner).singleton_js(*n as usize)
                }
                (_, Some(other)) => other.clone(),
                _ => JsValue::Undefined,
            })
        });
        // The pinned wall clock, or `undefined` for the system's.
        let w = Rc::downgrade(&self.inner);
        let wall_now: HostFn = Rc::new(move |_scope, _args| {
            Ok(w.upgrade()
                .and_then(|inner| Engine::from_inner(inner).wall_clock_ms())
                .map_or(JsValue::Undefined, JsValue::Number))
        });
        // A type of a module a script imports under a name
        // (`.import QtQuick as QQ`): a singleton's object
        // (`Sql.LocalStorage`), else its enums, or nothing.
        let w = Rc::downgrade(&self.inner);
        let type_of: HostFn = Rc::new(move |scope, args| {
            let (Some(inner), [JsValue::String(module), JsValue::String(name)]) =
                (w.upgrade(), args)
            else {
                return Ok(JsValue::Undefined);
            };
            let engine = Engine::from_inner(inner);
            if let Some(o) = engine.singleton_by_name(module, name) {
                return Ok(engine.to_js(&Value::Object(o)));
            }
            // `QtQuick` sees `QtQml`.
            Ok(engine
                .lookup_type(module, name)
                .or_else(|| engine.lookup_type("QtQml", name))
                .map_or(JsValue::Undefined, |m| engine.type_face(scope, &m)))
        });
        // `value instanceof Type`, asked by what stands for the type
        // (incubator/types.rs).
        let w = Rc::downgrade(&self.inner);
        let instance_of: HostFn = Rc::new(move |_scope, args| {
            let (Some(inner), [value, JsValue::Number(index)]) = (w.upgrade(), args) else {
                return Ok(JsValue::Bool(false));
            };
            Ok(JsValue::Bool(
                Engine::from_inner(inner).instance_of(value, *index as usize),
            ))
        });
        // The file and line of the first frame of a stack that is a place
        // in a file, as `[url, line]`: where an error was made.
        let table = js.lines.clone();
        let error_place: HostFn = Rc::new(move |_scope, args| {
            let Some(JsValue::String(stack)) = args.first() else {
                return Ok(JsValue::Null);
            };
            let lines = table.borrow();
            let place = stack.lines().find_map(|frame| {
                let (module, line) = crate::introspect::frame_place(frame.trim())?;
                lines.map(module, line)
            });
            Ok(match place {
                Some((url, line)) => {
                    JsValue::Array(vec![JsValue::String(url), JsValue::Number(f64::from(line))])
                }
                None => JsValue::Null,
            })
        });
        let err = |e: JsFault| EngineError::Js(e.to_string());
        js.host
            .set_global_function("__ljos_errorPlace", error_place)
            .map_err(err)?;
        js.host
            .eval_script("error-place", ERROR_PLACE)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_type", type_of)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_instanceOf", instance_of)
            .map_err(err)?;
        js.host
            .eval_script("types", crate::incubator::types::TYPE_FACES)
            .map_err(err)?;
        js.host
            .eval_script(
                "modules",
                "globalThis.__ljos_module = (module) => new Proxy({}, {\n\
                   get: (_, name) => typeof name === 'string' ? __ljos_type(module, name) : undefined\n\
                 });",
            )
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_wallNow", wall_now)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_attached", attached)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_single", single)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_color", color)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_colorOf", color_of)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_log", log)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_callLater", call_later)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_quit", quit)
            .map_err(err)?;
        // The value types first: the prelude puts their makers on `Qt`.
        value_faces::define(&js.host).map_err(err)?;
        js.host.eval_script("prelude", PRELUDE).map_err(err)?;
        js.host
            .eval_script("colors", value_faces::COLORS)
            .map_err(err)?;
        // `qsTr` and its family, `String.prototype.arg`, `Qt.uiLanguage`,
        // `Qt.locale()`: see i18n.rs.
        self.install_i18n_globals(js)?;
        // `Qt.createComponent`, `Qt.createQmlObject`, `Qt.resolvedUrl`:
        // see incubator/dynamic.rs.
        self.install_dynamic_globals(js)?;
        // What stands for a `list<T>` property: see list_faces.rs.
        self.install_list_globals(js)?;
        // The `URL` and `URLSearchParams` globals: see url_global.rs.
        crate::url_global::install(js)?;
        // The enums by name first: a key the older tables also have is
        // written again with the same number.
        let keys: Vec<String> = crate::qt_enums::QT_ENUMS
            .iter()
            .flat_map(|e| e.keys.iter())
            .chain(QT_ENUM_KEYS)
            .chain(crate::qt_keys::QT_KEY_ENUMS)
            .map(|(k, v)| format!("{k}:{v}"))
            .collect();
        js.host
            .eval_script(
                "qt-enums",
                &format!("Object.assign(Qt, {{{}}});", keys.join(",")),
            )
            .map_err(err)?;
        Ok(())
    }
}

/// The enum keys of the `Qt` namespace this runtime knows.
///
/// One table for both readers: JS reads them off the `Qt` global, and a
/// compiled literal (`layoutDirection: Qt.RightToLeft`) never reaches JS,
/// so the incubator looks the key up here.
pub(crate) const QT_ENUM_KEYS: &[(&str, i64)] = &[
    ("AlignLeft", 0x1),
    ("AlignRight", 0x2),
    ("AlignHCenter", 0x4),
    ("AlignJustify", 0x8),
    ("AlignTop", 0x20),
    ("AlignBottom", 0x40),
    ("AlignVCenter", 0x80),
    ("AlignCenter", 0x84),
    ("AlignBaseline", 0x100),
    ("Horizontal", 1),
    ("Vertical", 2),
    ("LeftToRight", 0),
    ("RightToLeft", 1),
    ("LayoutDirectionAuto", 2),
    ("ElideLeft", 0),
    ("ElideRight", 1),
    ("ElideMiddle", 2),
    ("ElideNone", 3),
    ("NoButton", 0),
    ("LeftButton", 1),
    ("RightButton", 2),
    ("MiddleButton", 4),
    ("BackButton", 8),
    ("ForwardButton", 0x10),
    ("AllButtons", 0x07ff_ffff),
    ("NoModifier", 0),
    ("ShiftModifier", 0x0200_0000),
    ("ControlModifier", 0x0400_0000),
    ("AltModifier", 0x0800_0000),
    ("MetaModifier", 0x1000_0000),
    ("Unchecked", 0),
    ("PartiallyChecked", 1),
    ("Checked", 2),
    // The modes of `Component.incubateObject`.
    ("Asynchronous", 0),
    ("Synchronous", 1),
    // Qt::MatchFlag, for `ComboBox.find`.
    ("MatchExactly", 0),
    ("MatchContains", 1),
    ("MatchStartsWith", 2),
    ("MatchEndsWith", 3),
    ("MatchRegularExpression", 4),
    ("MatchWildcard", 5),
    ("MatchFixedString", 8),
    ("MatchCaseSensitive", 16),
    ("MatchWrap", 32),
    ("MatchRecursive", 64),
    // Qt::InputMethodHint, for `inputMethodHints`.
    ("ImhNone", 0),
    ("ImhHiddenText", 0x1),
    ("ImhSensitiveData", 0x2),
    ("ImhNoAutoUppercase", 0x4),
    ("ImhPreferNumbers", 0x8),
    ("ImhPreferUppercase", 0x10),
    ("ImhPreferLowercase", 0x20),
    ("ImhNoPredictiveText", 0x40),
    ("ImhDate", 0x80),
    ("ImhTime", 0x100),
    ("ImhPreferLatin", 0x200),
    ("ImhMultiLine", 0x400),
    ("ImhNoEditMenu", 0x800),
    ("ImhNoTextHandles", 0x1000),
    ("ImhDigitsOnly", 0x1_0000),
    ("ImhFormattedNumbersOnly", 0x2_0000),
    ("ImhUppercaseOnly", 0x4_0000),
    ("ImhLowercaseOnly", 0x8_0000),
    ("ImhDialableCharactersOnly", 0x10_0000),
    ("ImhEmailCharactersOnly", 0x20_0000),
    ("ImhUrlCharactersOnly", 0x40_0000),
    ("ImhLatinOnly", 0x80_0000),
    // Read as a 32-bit int in QML, so negative.
    ("ImhExclusiveInputMask", -0x1_0000),
];

/// The value of `Qt.<key>`, when it is an enum key.
pub(crate) fn qt_enum_key(key: &str) -> Option<i64> {
    QT_ENUM_KEYS
        .iter()
        .chain(crate::qt_keys::QT_KEY_ENUMS)
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .or_else(|| crate::qt_enums::key(key))
}

/// The globals every QML file sees, written in JS because they are mostly
/// string work and the host calls are few.
const PRELUDE: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const hostLog = g.__ljos_log;
  // An array prints in brackets, as Qt's console prints one; one that
  // holds itself says so where it does.
  const show = (a, seen) => {
    if (typeof a === "string") return a;
    if (Array.isArray(a)) {
      seen = seen || [];
      if (seen.includes(a)) return "[Circular]";
      seen.push(a);
      const inner = a.map((v) => show(v, seen)).join(",");
      seen.pop();
      return "[" + inner + "]";
    }
    try { return String(a); } catch (e) { return "[object]"; }
  };
  const text = (args) => Array.prototype.map.call(args, (a) => show(a)).join(" ");
  const console = {
    log() { hostLog(0, text(arguments)); },
    debug() { hostLog(0, text(arguments)); },
    info() { hostLog(1, text(arguments)); },
    warn() { hostLog(2, text(arguments)); },
    error() { hostLog(3, text(arguments)); },
    assert(cond) {
      if (!cond) hostLog(3, "Assertion failed: " + text(Array.prototype.slice.call(arguments, 1)));
    },
    trace() { hostLog(0, text(arguments)); },
  };
  Object.defineProperty(g, "console", { value: console, writable: true, configurable: true });
  Object.defineProperty(g, "print", { value: console.log, writable: true, configurable: true });
  // Qt's `gc()` asks for a collection; the engine collects when it must,
  // and a script only needs the name to exist.
  Object.defineProperty(g, "gc", { value() {}, writable: true, configurable: true });

  // `new Date()`, `Date()` and `Date.now()` read the host's wall clock
  // when it is pinned; everything else about Date is the engine's own.
  // A Proxy keeps `instanceof Date`, the statics and the prototype.
  const wallNow = g.__ljos_wallNow;
  const RealDate = g.Date;
  g.Date = new Proxy(RealDate, {
    construct(target, args, newTarget) {
      if (args.length === 0) {
        const ms = wallNow();
        if (ms !== undefined) return Reflect.construct(target, [ms], newTarget);
      }
      return Reflect.construct(target, args, newTarget);
    },
    apply(target, thisArg, args) {
      const ms = wallNow();
      return ms === undefined ? target() : new target(ms).toString();
    },
    get(target, key, receiver) {
      if (key === "now") {
        return function now() {
          const ms = wallNow();
          return ms === undefined ? target.now() : Math.floor(ms);
        };
      }
      return Reflect.get(target, key, receiver);
    },
  });

  // A grouped property (`rect.border`): accessors over the flat
  // `border.width` names.
  g.__ljos_group = function (obj, prefix, leaves) {
    const o = {};
    for (const leaf of leaves) {
      Object.defineProperty(o, leaf, {
        get() { return obj[prefix + leaf]; },
        set(v) { obj[prefix + leaf] = v; },
        enumerable: true,
      });
    }
    return o;
  };

  // `other.Layout`, compiled: what the type attached to an object. For
  // anything else it is the member the script wrote.
  const hostAttached = g.__ljos_attached;
  g.__ljos_att = function (o, module, name) {
    const a = hostAttached(o, module, name);
    return a === undefined ? o[name] : a;
  };

  // A name nothing in sight declares, compiled: what the engine found for
  // it when the function was called. Where it found nothing it gave this
  // object, and reading the name throws; asking for its type does not.
  const nowhere = Object.freeze({});
  g.__ljos_nowhere = nowhere;
  g.__ljos_named = function (v, name) {
    if (v === nowhere) throw new ReferenceError(name + " is not defined");
    return v;
  };
  g.__ljos_known = function (v) { return v === nowhere ? undefined : v; };
  // What a script that assigns to such a name is given to assign through.
  g.__ljos_unheld = function (name) {
    return Object.defineProperty({}, name, {
      get() { throw new ReferenceError(name + " is not defined"); },
      set() { throw new Error("Invalid write to global property \"" + name + "\""); },
    });
  };

  // Colours come from the host: there is one object per colour, so that two
  // equal colours are also `===`.
  const hostColor = g.__ljos_color;
  const f32 = Math.fround;
  const num = (v) => { v = Number(v); return v !== v ? 0 : v; };
  const unit = (v) => Math.max(0, Math.min(1, num(v)));
  const alpha = (a) => (a === undefined ? 1 : num(a));
  // Qt takes these numbers in single precision and keeps sixteen bits of
  // each, and a hue in hundredths of a degree where a full turn is zero.
  // Rounding the same way first gives the channels it gives.
  const to16 = (v) => Math.round(f32(f32(unit(v)) * 65535));
  const turn = (h) => (Math.round(f32(f32(unit(h)) * 36000)) % 36000) / 36000;
  const ch = (v) => Math.round(unit(v) * 65535);
  const overloads = (name, n) => {
    if (n > 4) throw new Error("Too many arguments");
    if (n < 3) {
      throw new Error("Unable to determine callable overload.  Candidates are:\n    "
        + name + "(double,double,double)\n    " + name + "(double,double,double,double)");
    }
  };
  function rgba(r, gr, b, a) {
    overloads("rgba", arguments.length);
    return hostColor(to16(r), to16(gr), to16(b), to16(alpha(a)));
  }
  const hue = (p, q, t) => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  // Qt.hsla in Qt never gives a channel of one sixteen-bit step: where the
  // arithmetic says one, it gives none. Without this a fully saturated
  // colour at half lightness would not be a pure one.
  const hslCh = (v) => { const n = ch(v); return n === 1 ? 0 : n; };
  function hsla(h, s, l, a) {
    overloads("hsla", arguments.length);
    h = turn(h); s = to16(s) / 65535; l = to16(l) / 65535; a = to16(alpha(a));
    if (s === 0) return hostColor(ch(l), ch(l), ch(l), a);
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    return hostColor(hslCh(hue(p, q, h + 1 / 3)), hslCh(hue(p, q, h)), hslCh(hue(p, q, h - 1 / 3)), a);
  }
  function hsva(h, s, v, a) {
    overloads("hsva", arguments.length);
    h = turn(h) * 6; s = to16(s) / 65535; v = to16(v) / 65535; a = to16(alpha(a));
    const i = Math.floor(h);
    const f = h - i;
    const p = ch(v * (1 - s));
    const q = ch(v * (1 - s * f));
    const t = ch(v * (1 - s * (1 - f)));
    v = ch(v);
    switch (i) {
      case 0: return hostColor(v, t, p, a);
      case 1: return hostColor(q, v, p, a);
      case 2: return hostColor(p, v, t, a);
      case 3: return hostColor(p, q, v, a);
      case 4: return hostColor(t, p, v, a);
      default: return hostColor(v, p, q, a);
    }
  }
  const Qt = {
    rgba, hsla, hsva,
    rgb: (r, gr, b) => hostColor(to16(r), to16(gr), to16(b), 65535),
    // point, size, rect, the vectors, quaternion, font.
    ...g.__ljos_faces.qt,
    binding(f) {
      if (typeof f !== "function") throw new TypeError("Qt.binding(): argument must be a function");
      return Object.freeze({ __ljosBinding: f });
    },
    callLater(f) { g.__ljos_callLater(f, Array.prototype.slice.call(arguments, 1)); },
    quit() { g.__ljos_quit(); },
    // formatDateTime, formatDate and formatTime write with a locale's
    // formats, so the localisation prelude (i18n.rs) defines them.
    isQtObject: (o) => o !== null && typeof o === "object" && typeof o.destroy === "function",
    // The colours Qt names are numbers in QML, not colours.
    color0: 0, color1: 1, black: 2, white: 3, darkGray: 4, gray: 5, lightGray: 6, red: 7,
    green: 8, blue: 9, cyan: 10, magenta: 11, yellow: 12, darkRed: 13, darkGreen: 14,
    darkBlue: 15, darkCyan: 16, darkMagenta: 17, darkYellow: 18, transparent: 19,
  };
  Object.defineProperty(g, "Qt", { value: Qt, writable: true, configurable: true });
})();
"##;

/// A colour as it crosses into JS.
pub(crate) fn color_to_js(c: Color) -> JsValue {
    let channels = [c.r, c.g, c.b, c.a];
    if c.is_valid() {
        JsValue::Color(channels)
    } else {
        JsValue::InvalidColor(channels)
    }
}

/// A colour from JS; [`Color::TRANSPARENT`] for what is not one.
pub(crate) fn color_from_js(v: &JsValue) -> Color {
    match *v {
        JsValue::Color([r, g, b, a]) => Color::rgba16(r, g, b, a),
        JsValue::InvalidColor([r, g, b, a]) => Color::rgba16(r, g, b, a).invalidated(),
        _ => Color::TRANSPARENT,
    }
}
