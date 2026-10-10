//! The JS bridge (ARCHITECTURE.md §9.2): the engine's [`HostObjects`],
//! value conversion, and the globals every QML file sees (`console`, `Qt`,
//! `qsTr`, `String.prototype.arg`).
//!
//! The host is made on first use, so an engine that never runs JS (a test,
//! a tier-0-only UI) never starts QuickJS. Every JS run goes through
//! [`Engine::run_js`], which turns a failure into a Qt-style warning instead
//! of an error the caller has to route: a binding that throws keeps its
//! value, a handler that throws is skipped, as in Qt.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use ljos_js::rquickjs;
use ljos_js::{
    HostError, HostFn, HostObjects, HostTypeId, JsConfig, JsFault, JsHost, JsRef, JsValue,
    MethodDef, PropDef, Scope, SignalDef, TypeDef,
};
use slotmap::{Key, KeyData};

use crate::engine::{Engine, Inner};
use crate::error::EngineError;
use crate::ids::{ConnectionId, MethodIndex, ObjectId, PropIndex, SignalIndex};
use crate::log::LogLevel;
use crate::meta::{MetaRef, PropertyFlags};
use crate::types::TypeTag;
use crate::value::{Color, Font, JsHandle, Value};

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
    /// Module name to (QML url, [(first JS line of a function, its QML
    /// line)]).
    modules: HashMap<String, (String, Vec<(u32, u32)>)>,
}

impl LineTable {
    /// Reads the `// File.qml:L:C` comment the compiler writes above every
    /// function.
    pub(crate) fn add(&mut self, module: &str, url: &str, source: &str) {
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
                let first = if wrapped { i + 3 } else { i + 2 };
                lines.push((first as u32, line));
            }
        }
        self.modules
            .insert(module.to_owned(), (url.to_owned(), lines));
    }

    fn map(&self, module: &str, js_line: u32) -> Option<(String, u32)> {
        let (url, lines) = self.modules.get(module)?;
        let i = lines.partition_point(|(js, _)| *js <= js_line);
        let (js, qml) = lines.get(i.checked_sub(1)?)?;
        Some((url.clone(), qml + js_line.saturating_sub(*js)))
    }
}

/// The engine's JS state: the host, the prototypes made for each type, and
/// connections made from JS.
pub(crate) struct JsState {
    pub(crate) host: JsHost,
    /// Prototype per type description; the `MetaRef` keeps the pointer the
    /// key is made from alive.
    types: RefCell<HashMap<usize, (MetaRef, HostTypeId)>>,
    conns: RefCell<Vec<JsConn>>,
    pub(crate) lines: Rc<RefCell<LineTable>>,
}

impl JsState {
    /// Forgets destroyed objects: their wrappers and JS connections.
    pub(crate) fn forget(&self, objects: &[ObjectId]) {
        for o in objects {
            self.host.drop_wrapper(obj_to_u64(*o));
        }
        self.conns
            .borrow_mut()
            .retain(|c| !objects.contains(&c.sender));
    }

    fn type_id(&self, engine: &Engine, meta: &MetaRef) -> Result<HostTypeId, JsFault> {
        let key = std::ptr::from_ref::<crate::meta::MetaObject>(meta) as usize;
        if let Some((_, id)) = self.types.borrow().get(&key) {
            return Ok(*id);
        }
        let def = type_def(engine, meta);
        let id = self.host.register_type(&def)?;
        self.types.borrow_mut().insert(key, (meta.clone(), id));
        Ok(id)
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
    if !seen.contains_key("parent") {
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
    fn type_of(&self, obj: u64) -> Option<HostTypeId> {
        let engine = self.engine().ok()?;
        let meta = engine.meta_of(u64_to_obj(obj))?;
        let js = engine.inner.js.borrow().clone()?;
        js.type_id(&engine, &meta).ok()
    }

    fn get<'js>(&self, scope: &Scope<'js>, obj: u64, prop: u32) -> Result<JsValue, HostError> {
        let engine = self.engine()?;
        let id = u64_to_obj(obj);
        if prop == PARENT_ID {
            return Ok(match engine.visual_parent(id) {
                Some(p) => engine.to_js(&Value::Object(p)),
                None => JsValue::Null,
            });
        }
        let p = PropIndex(prop);
        let group = {
            let store = engine.inner.store.borrow();
            store.objects.get(id).and_then(|h| {
                let info = h.meta.property(p)?;
                info.flags
                    .contains(PropertyFlags::GROUP)
                    .then(|| info.name.clone())
            })
        };
        if let Some(name) = group {
            return group_proxy(scope, &engine, id, &name).map_err(HostError::from);
        }
        match engine.try_read(id, p) {
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
        let id = u64_to_obj(obj);
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
                        Ok(e.js_to_value(s, out, &hint))
                    })
                })
                .map_err(host_err);
        }
        let v = engine.js_to_value(scope, value, &ty);
        engine
            .write(id, p, v, crate::WriteKind::Imperative)
            .map_err(host_err)
    }

    fn call<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        method: u32,
        args: &[JsValue],
    ) -> Result<JsValue, HostError> {
        let engine = self.engine()?;
        let id = u64_to_obj(obj);
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
        let vals: Vec<Value> = args
            .iter()
            .map(|a| engine.js_to_value(scope, a.clone(), &TypeTag::Var))
            .collect();
        let out = engine
            .invoke(id, MethodIndex(method), &vals)
            .map_err(host_err)?;
        Ok(engine.to_js(&out))
    }

    fn emit<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        signal: u32,
        args: &[JsValue],
    ) -> Result<(), HostError> {
        let engine = self.engine()?;
        let vals: Vec<Value> = args
            .iter()
            .map(|a| engine.js_to_value(scope, a.clone(), &TypeTag::Var))
            .collect();
        engine
            .emit(u64_to_obj(obj), SignalIndex(signal), &vals)
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
        if let Ok(engine) = self.engine() {
            engine.destroy(u64_to_obj(obj));
        }
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

impl Engine {
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
        let state = Rc::new(JsState {
            host,
            types: RefCell::new(HashMap::new()),
            conns: RefCell::new(Vec::new()),
            lines,
        });
        *self.inner.js.borrow_mut() = Some(state.clone());
        self.install_globals(&state)?;
        Ok(state)
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
        let js = match self.js() {
            Ok(js) => js,
            Err(e) => {
                self.warn(e.to_string());
                return None;
            }
        };
        let slice = js.host.config().slice;
        match js.host.enter(label, slice, f) {
            Ok(v) => Some(v),
            Err(fault) => {
                self.report_js_fault(&fault);
                None
            }
        }
    }

    pub(crate) fn report_js_fault(&self, fault: &JsFault) {
        let msg = match fault {
            JsFault::JsException {
                message,
                file: Some(file),
                line: Some(line),
                ..
            } => format!("{file}:{line}: {message}"),
            JsFault::JsException { message, .. } => message.clone(),
            other => other.to_string(),
        };
        self.warn(msg);
    }

    /// Evaluates a classic script in the global scope, for hosts and tests.
    /// Returns its completion value.
    pub fn eval_js(&self, source: &str) -> Result<Value, EngineError> {
        let js = self.js()?;
        let slice = js.host.config().slice;
        js.host
            .enter("eval", slice, |s| {
                let v: rquickjs::Value = s.ctx().eval(source)?;
                let v = s.from_value(&v)?;
                Ok(self.js_to_value(s, v, &TypeTag::Var))
            })
            .map_err(|e| EngineError::Js(e.to_string()))
    }

    /// An engine value as JS sees it. Colours and URLs are strings; value
    /// types are fresh plain objects; a destroyed object is `null`.
    pub(crate) fn to_js(&self, v: &Value) -> JsValue {
        match v {
            Value::Undefined => JsValue::Undefined,
            Value::Null => JsValue::Null,
            Value::Bool(b) => JsValue::Bool(*b),
            Value::Int(i) => JsValue::Number(f64::from(*i)),
            Value::Real(r) => JsValue::Number(*r),
            Value::String(s) | Value::Url(s) => JsValue::String(s.to_string()),
            Value::Color(_) => JsValue::String(v.to_js_string().to_string()),
            Value::Point(p) => map(&[("x", p.x), ("y", p.y)]),
            Value::Size(s) => map(&[("width", s.width), ("height", s.height)]),
            Value::Rect(r) => map(&[
                ("x", r.x),
                ("y", r.y),
                ("width", r.width),
                ("height", r.height),
            ]),
            Value::Font(f) => JsValue::Map(vec![
                ("family".into(), JsValue::String(f.family.to_string())),
                ("pointSize".into(), JsValue::Number(f.point_size)),
                ("pixelSize".into(), JsValue::Number(f64::from(f.pixel_size))),
                ("weight".into(), JsValue::Number(f64::from(f.weight))),
                ("bold".into(), JsValue::Bool(f.bold())),
                ("italic".into(), JsValue::Bool(f.italic)),
                ("underline".into(), JsValue::Bool(f.underline)),
                ("strikeout".into(), JsValue::Bool(f.strikeout)),
                ("letterSpacing".into(), JsValue::Number(f.letter_spacing)),
                ("wordSpacing".into(), JsValue::Number(f.word_spacing)),
            ]),
            Value::Object(id) if self.is_alive(*id) => JsValue::Object(obj_to_u64(*id)),
            Value::Object(_) => JsValue::Null,
            Value::List(items) => JsValue::Array(items.iter().map(|i| self.to_js(i)).collect()),
            Value::Enum(e) => JsValue::Number(f64::from(e.value)),
            Value::Js(h) => JsValue::Handle(h.0.clone()),
        }
    }

    /// A JS value as an engine value. `ty` is the type it is going to, so a
    /// plain `{x, y}` can become a `point`.
    pub(crate) fn js_to_value(&self, scope: &Scope<'_>, v: JsValue, ty: &TypeTag) -> Value {
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
            JsValue::Handle(r) => {
                let num = |k: &str| match scope.get_field(&r, k) {
                    Ok(JsValue::Number(n)) => n,
                    _ => 0.0,
                };
                match ty {
                    TypeTag::Point => Value::point(num("x"), num("y")),
                    TypeTag::Size => Value::size(num("width"), num("height")),
                    TypeTag::Rect => Value::rect(num("x"), num("y"), num("width"), num("height")),
                    TypeTag::Color => {
                        // A colour object (`{r, g, b, a}`) from a JS helper.
                        let a = match scope.get_field(&r, "a") {
                            Ok(JsValue::Number(n)) => n,
                            _ => 1.0,
                        };
                        Value::Color(Color::from_rgba_f(num("r"), num("g"), num("b"), a))
                    }
                    TypeTag::Font => {
                        let mut f = Font::default();
                        if let Ok(JsValue::String(s)) = scope.get_field(&r, "family") {
                            f.family = s.into();
                        }
                        if let Ok(JsValue::Number(n)) = scope.get_field(&r, "pointSize") {
                            f.point_size = n;
                        }
                        if let Ok(JsValue::Number(n)) = scope.get_field(&r, "pixelSize") {
                            f.pixel_size = n as i32;
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
        let tr: HostFn = Rc::new(move |_scope, _args| {
            if let Some(inner) = w.upgrade() {
                Engine::from_inner(inner).track_translation();
            }
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
        let w = weak;
        let quit: HostFn = Rc::new(move |_scope, _args| {
            if let Some(inner) = w.upgrade() {
                Engine::from_inner(inner).request_quit();
            }
            Ok(JsValue::Undefined)
        });
        let err = |e: JsFault| EngineError::Js(e.to_string());
        js.host
            .set_global_function("__ljos_log", log)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_trTrack", tr)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_callLater", call_later)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_quit", quit)
            .map_err(err)?;
        js.host.eval_script("prelude", PRELUDE).map_err(err)?;
        let keys: Vec<String> = QT_ENUM_KEYS
            .iter()
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

fn map(fields: &[(&str, f64)]) -> JsValue {
    JsValue::Map(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), JsValue::Number(*v)))
            .collect(),
    )
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
];

/// The value of `Qt.<key>`, when it is an enum key.
pub(crate) fn qt_enum_key(key: &str) -> Option<i64> {
    QT_ENUM_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

/// The globals every QML file sees, written in JS because they are mostly
/// string work and the host calls are few.
const PRELUDE: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const hostLog = g.__ljos_log;
  const show = (a) => {
    if (typeof a === "string") return a;
    if (Array.isArray(a)) return a.map(show).join(",");
    try { return String(a); } catch (e) { return "[object]"; }
  };
  const text = (args) => Array.prototype.map.call(args, show).join(" ");
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

  const clamp = (v) => Math.max(0, Math.min(1, Number(v)));
  const hex2 = (v) => {
    const n = Math.round(clamp(v) * 255);
    return (n < 16 ? "0" : "") + n.toString(16);
  };
  const rgba = (r, gr, b, a) => {
    if (a === undefined) a = 1;
    const rgb = hex2(r) + hex2(gr) + hex2(b);
    return clamp(a) >= 1 ? "#" + rgb : "#" + hex2(a) + rgb;
  };
  const hue = (p, q, t) => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  const hsla = (h, s, l, a) => {
    h = Number(h); s = clamp(s); l = clamp(l);
    if (s === 0) return rgba(l, l, l, a);
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    return rgba(hue(p, q, h + 1 / 3), hue(p, q, h), hue(p, q, h - 1 / 3), a);
  };
  const hsva = (h, s, v, a) => {
    h = Number(h); s = clamp(s); v = clamp(v);
    const l = v * (1 - s / 2);
    const sl = l === 0 || l === 1 ? 0 : (v - l) / Math.min(l, 1 - l);
    return hsla(h, sl, l, a);
  };
  const pad = (n, w) => String(n).padStart(w, "0");
  const MONTHS = ["January", "February", "March", "April", "May", "June", "July",
    "August", "September", "October", "November", "December"];
  const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
  const formatDateTime = (d, f) => {
    if (!(d instanceof Date)) d = new Date(d);
    if (f === undefined || typeof f !== "string") return d.toString();
    return f.replace(/yyyy|yy|MMMM|MMM|MM|M|dddd|ddd|dd|d|hh|h|HH|H|mm|m|ss|s|zzz|AP|ap|'[^']*'/g, (t) => {
      switch (t) {
        case "yyyy": return pad(d.getFullYear(), 4);
        case "yy": return pad(d.getFullYear() % 100, 2);
        case "MMMM": return MONTHS[d.getMonth()];
        case "MMM": return MONTHS[d.getMonth()].slice(0, 3);
        case "MM": return pad(d.getMonth() + 1, 2);
        case "M": return String(d.getMonth() + 1);
        case "dddd": return DAYS[d.getDay()];
        case "ddd": return DAYS[d.getDay()].slice(0, 3);
        case "dd": return pad(d.getDate(), 2);
        case "d": return String(d.getDate());
        case "HH": case "hh": return pad(d.getHours(), 2);
        case "H": case "h": return String(d.getHours());
        case "mm": return pad(d.getMinutes(), 2);
        case "m": return String(d.getMinutes());
        case "ss": return pad(d.getSeconds(), 2);
        case "s": return String(d.getSeconds());
        case "zzz": return pad(d.getMilliseconds(), 3);
        case "AP": return d.getHours() < 12 ? "AM" : "PM";
        case "ap": return d.getHours() < 12 ? "am" : "pm";
        default: return t.slice(1, -1);
      }
    });
  };
  const Qt = {
    rgba, hsla, hsva,
    rgb: (r, gr, b) => rgba(r, gr, b, 1),
    point: (x, y) => ({ x: Number(x), y: Number(y) }),
    size: (width, height) => ({ width: Number(width), height: Number(height) }),
    rect: (x, y, width, height) => ({ x: Number(x), y: Number(y), width: Number(width), height: Number(height) }),
    binding(f) {
      if (typeof f !== "function") throw new TypeError("Qt.binding(): argument must be a function");
      return Object.freeze({ __ljosBinding: f });
    },
    callLater(f) { g.__ljos_callLater(f, Array.prototype.slice.call(arguments, 1)); },
    quit() { g.__ljos_quit(); },
    formatDateTime,
    formatDate: (d, f) => formatDateTime(d, f === undefined ? "yyyy-MM-dd" : f),
    formatTime: (d, f) => formatDateTime(d, f === undefined ? "HH:mm:ss" : f),
    isQtObject: (o) => o !== null && typeof o === "object" && typeof o.destroy === "function",
    transparent: "#00000000", black: "#000000", white: "#ffffff",
    red: "#ff0000", green: "#008000", blue: "#0000ff",
  };
  Object.defineProperty(g, "Qt", { value: Qt, writable: true, configurable: true });

  // String.prototype.arg: replaces the lowest-numbered %N, as QString::arg.
  Object.defineProperty(String.prototype, "arg", {
    value: function (v) {
      const s = String(this);
      let min = 100;
      s.replace(/%(\d\d?)/g, (m, n) => { n = Number(n); if (n > 0 && n < min) min = n; return m; });
      if (min === 100) return s;
      const re = new RegExp("%0?" + min + "(?!\\d)", "g");
      const rep = String(v);
      return s.replace(re, () => rep);
    },
    writable: true, configurable: true,
  });

  const plural = (s, n) => (n === undefined ? s : s.replace(/%n/g, String(n)));
  const defs = {
    qsTr(s, disambiguation, n) { g.__ljos_trTrack(); return plural(String(s), n); },
    qsTranslate(ctx, s, disambiguation, n) { g.__ljos_trTrack(); return plural(String(s), n); },
    qsTrId(id, n) { g.__ljos_trTrack(); return plural(String(id), n); },
    QT_TR_NOOP: (s) => s,
    QT_TRANSLATE_NOOP: (ctx, s) => s,
    QT_TRID_NOOP: (id) => id,
  };
  for (const k of Object.keys(defs)) {
    Object.defineProperty(g, k, { value: defs[k], writable: true, configurable: true });
  }
})();
"##;
