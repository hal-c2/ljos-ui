//! What the code back end B generates calls besides the engine: the parts
//! of JavaScript's semantics the typed subset keeps (NaN in `Math.min`,
//! UTF-16 lengths, `ToInt32` in `&`) and the conversions to and from
//! [`Value`].

use std::collections::HashSet;
use std::sync::Arc;

use ljos_engine::{
    Atom, Engine, EngineError, MethodIndex, ObjectId, PropIndex, SignalIndex, Value, WriteKind,
    number_to_string, to_int32,
};

#[derive(Debug, thiserror::Error)]
pub enum Missing {
    #[error("no object named {0}")]
    Object(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// The first object (depth first from `top`) whose objectName is `name`:
/// the generated code names objects by their QML id, which the files also
/// give as objectName.
pub fn find(e: &Engine, top: ObjectId, name: &str) -> Result<ObjectId, Missing> {
    let mut stack = vec![top];
    while let Some(o) = stack.pop() {
        if e.read_by_name(o, "objectName").to_js_string().as_str() == name {
            return Ok(o);
        }
        let mut kids = e.children(o);
        kids.reverse();
        stack.extend(kids);
    }
    Err(Missing::Object(name.into()))
}

pub trait IntoValue {
    fn into_value(self) -> Value;
}

impl IntoValue for f64 {
    fn into_value(self) -> Value {
        Value::Real(self)
    }
}

impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}

impl IntoValue for Atom {
    fn into_value(self) -> Value {
        Value::String(self)
    }
}

impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self) -> Value {
        Value::List(Arc::new(
            self.into_iter().map(IntoValue::into_value).collect(),
        ))
    }
}

pub fn val(v: impl IntoValue) -> Value {
    v.into_value()
}

/// What a script would see of a failed write, emit or call: a warning,
/// and the script goes on.
fn warn<T>(e: &Engine, what: &str, r: Result<T, EngineError>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(err) => {
            let _ = e;
            eprintln!("aot: {what}: {err}");
            None
        }
    }
}

pub fn write(e: &Engine, obj: ObjectId, prop: PropIndex, v: Value) {
    warn(e, "write", e.write(obj, prop, v, WriteKind::Imperative));
}

pub fn emit(e: &Engine, obj: ObjectId, sig: SignalIndex, args: &[Value]) {
    warn(e, "emit", e.emit(obj, sig, args));
}

pub fn invoke(e: &Engine, obj: ObjectId, m: MethodIndex, args: &[Value]) -> Value {
    warn(e, "call", e.invoke(obj, m, args)).unwrap_or(Value::Undefined)
}

pub fn sig_num(a: &[Value], i: usize) -> f64 {
    a.get(i).map_or(f64::NAN, Value::to_number)
}

pub fn sig_str(a: &[Value], i: usize) -> Atom {
    a.get(i)
        .map_or_else(|| Atom::from("undefined"), Value::to_js_string)
}

pub fn sig_bool(a: &[Value], i: usize) -> bool {
    a.get(i).is_some_and(Value::to_boolean)
}

fn items(e: &Engine, v: &Value) -> Vec<Value> {
    match e.plain(v) {
        Value::List(l) => l.as_ref().clone(),
        _ => Vec::new(),
    }
}

pub fn list_num(e: &Engine, v: &Value) -> Vec<f64> {
    items(e, v).iter().map(Value::to_number).collect()
}

pub fn list_bool(e: &Engine, v: &Value) -> Vec<bool> {
    items(e, v).iter().map(Value::to_boolean).collect()
}

pub fn list_str(e: &Engine, v: &Value) -> Vec<Atom> {
    items(e, v).iter().map(Value::to_js_string).collect()
}

/// `list[i]` of a typed list: out of range is what `undefined` converts to.
pub fn at_num(l: &[f64], i: f64) -> f64 {
    index(i).and_then(|i| l.get(i).copied()).unwrap_or(f64::NAN)
}

pub fn at_bool(l: &[bool], i: f64) -> bool {
    index(i).and_then(|i| l.get(i).copied()).unwrap_or(false)
}

pub fn at_str(l: &[Atom], i: f64) -> Atom {
    index(i)
        .and_then(|i| l.get(i).cloned())
        .unwrap_or_else(|| Atom::from("undefined"))
}

fn index(i: f64) -> Option<usize> {
    (i >= 0.0 && i.fract() == 0.0).then_some(i as usize)
}

pub fn row_num(e: &Engine, row: &Value, field: &str) -> f64 {
    match row {
        Value::Object(o) => e.read_by_name(*o, field).to_number(),
        _ => f64::NAN,
    }
}

pub fn row_str(e: &Engine, row: &Value, field: &str) -> Atom {
    match row {
        Value::Object(o) => e.read_by_name(*o, field).to_js_string(),
        _ => Atom::from("undefined"),
    }
}

pub fn row_bool(e: &Engine, row: &Value, field: &str) -> bool {
    match row {
        Value::Object(o) => e.read_by_name(*o, field).to_boolean(),
        _ => false,
    }
}

/// `Math.min` of two: NaN if either is.
pub fn math_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        // min(0, -0) is -0.
        if a.is_sign_negative() { a } else { b }
    } else {
        a.min(b)
    }
}

pub fn math_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        if a.is_sign_positive() { a } else { b }
    } else {
        a.max(b)
    }
}

pub fn bitand(a: f64, b: f64) -> f64 {
    f64::from(to_int32(a) & to_int32(b))
}

pub fn utf16_len(s: &Atom) -> f64 {
    s.as_str().encode_utf16().count() as f64
}

pub fn concat(a: &Atom, b: &Atom) -> Atom {
    Atom::from(format!("{}{}", a.as_str(), b.as_str()))
}

/// `qsTr` in the file the cases came from: the engine's catalogue lookup,
/// which tracks the language inside a binding as the dynamic tiers do.
pub fn qs_tr(e: &Engine, source: &str) -> Atom {
    Atom::from(e.translate("World", source, "", None))
}

/// `String.prototype.arg` of a number. The cases' templates have no `%L`,
/// so the plain form stands for both.
pub fn arg_num(template: &Atom, n: f64) -> Atom {
    let s = number_to_string(n);
    ljos_i18n::arg::arg(template.as_str(), s.as_str(), s.as_str())
        .map_or_else(|| template.clone(), Atom::from)
}

pub fn arg_str(template: &Atom, s: Atom) -> Atom {
    ljos_i18n::arg::arg(template.as_str(), s.as_str(), s.as_str())
        .map_or_else(|| template.clone(), Atom::from)
}

/// A `Set` of numbers: SameValueZero, so NaN is in it once and -0 is 0.
pub struct NumSet(HashSet<u64>);

impl NumSet {
    pub fn new(items: &[f64]) -> Self {
        NumSet(items.iter().map(|n| key(*n)).collect())
    }
    pub fn has(&self, n: f64) -> bool {
        self.0.contains(&key(n))
    }
}

fn key(n: f64) -> u64 {
    if n.is_nan() {
        f64::NAN.to_bits()
    } else if n == 0.0 {
        0
    } else {
        n.to_bits()
    }
}
