//! Value types a module brings: `coordinate` and its like.
//!
//! The engine has the value types of `QtQml` and `QtQuick` built in
//! ([`Value::Point`], [`Value::Font`] and the rest). A module crate that
//! has its own describes each with a [`CustomType`], registers it with
//! [`crate::Engine::register_value_type`], and a value of it is a
//! [`Value::Custom`]: the type and the fields it keeps.
//!
//! What a value does is worked out in Rust, by its type. To a script it is
//! a frozen object, one per distinct value as the built-in ones are
//! (value_faces.rs), whose members are all the type's members; a write to
//! one, its text and its methods call back into Rust
//! (`__ljos_custom_with`, `__ljos_custom_text`, `__ljos_custom_call`).
//!
//! `property coordinate c` finds the type by its name. Names are looked up
//! in one table for the thread, as [`TypeTag::parse`] has no engine at
//! hand; an engine and the types it registers stay on one thread.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use ljos_js::{HostError, HostFn, JsValue, Scope};

use crate::atom::Atom;
use crate::error::EngineError;
use crate::js::JsState;
use crate::types::TypeTag;
use crate::value::Value;

/// One member of a [`CustomType`].
#[derive(Clone, Copy, Debug)]
pub struct CustomMember {
    pub name: &'static str,
    /// The QML type of the member, as a property names it (`real`).
    pub ty: &'static str,
    /// Kept in the value's fields, in the order the members list them, and
    /// written one at a time (`c.latitude = 10`); the others are worked out
    /// from those ([`CustomType::computed`]).
    pub stored: bool,
    /// What a script finds when it lists the value's keys
    /// (`JSON.stringify`); a hidden member is still read by its name.
    pub shown: bool,
}

impl CustomMember {
    /// A member kept in the value, which a script sees.
    pub const fn stored(name: &'static str, ty: &'static str) -> Self {
        Self {
            name,
            ty,
            stored: true,
            shown: true,
        }
    }

    /// A member worked out from the stored ones.
    pub const fn computed(name: &'static str, ty: &'static str) -> Self {
        Self {
            name,
            ty,
            stored: false,
            shown: true,
        }
    }

    /// Out of sight when a script lists the keys.
    pub const fn hidden(self) -> Self {
        Self {
            shown: false,
            ..self
        }
    }
}

/// A value type of a module. See the module docs.
pub trait CustomType: Send + Sync + 'static {
    /// The name a property is declared with (`coordinate`).
    fn name(&self) -> &'static str;
    /// The module that has the type (`QtPositioning`).
    fn module(&self) -> &'static str;
    /// Every member; the stored ones are the value's fields, in this order.
    fn members(&self) -> &'static [CustomMember];
    /// What a script may call on a value ([`CustomType::call`]).
    fn methods(&self) -> &'static [&'static str] {
        &[]
    }
    /// The fields of the value a property of the type starts with.
    fn default_fields(&self) -> Vec<Value>;
    /// A member that is not stored.
    fn computed(&self, _v: &CustomValue, _name: &str) -> Value {
        Value::Undefined
    }
    /// The value as text: what `toString()` and a `string` property make
    /// of it.
    fn text(&self, v: &CustomValue) -> String;
    /// `v` with the member `name` given `to`; `None` when it has no such
    /// member to write. By default a stored member takes `to` as its type
    /// does.
    fn with(&self, v: &CustomValue, name: &str, to: &Value) -> Option<CustomValue> {
        let i = stored_index(self.members(), name)?;
        let ty = TypeTag::parse(self.members().iter().find(|m| m.name == name)?.ty);
        let mut fields = v.fields.clone();
        fields[i] = ty.coerce(to).ok()?;
        Some(CustomValue { ty: v.ty, fields })
    }
    /// The method `method` of `v`, called with `args`; an error is thrown
    /// to the script.
    fn call(&self, _v: &CustomValue, method: &str, _args: &[Value]) -> Result<Value, String> {
        Err(format!("{method} is not a function"))
    }
    /// What a property of the type makes of a value that is not one of
    /// it; `None` refuses it.
    fn coerce(&self, _v: &Value) -> Option<Value> {
        None
    }
}

fn stored_index(members: &[CustomMember], name: &str) -> Option<usize> {
    members
        .iter()
        .filter(|m| m.stored)
        .position(|m| m.name == name)
}

/// A value of a [`CustomType`].
#[derive(Clone)]
pub struct CustomValue {
    ty: &'static dyn CustomType,
    fields: Vec<Value>,
}

impl CustomValue {
    /// A value of `ty` that keeps `fields`, one for each stored member;
    /// missing ones are the type's defaults.
    pub fn new(ty: &'static dyn CustomType, mut fields: Vec<Value>) -> Self {
        let stored = ty.members().iter().filter(|m| m.stored).count();
        if fields.len() != stored {
            let defaults = ty.default_fields();
            fields.truncate(stored);
            fields.extend(defaults.into_iter().skip(fields.len()));
        }
        Self { ty, fields }
    }

    /// The value a property of `ty` starts with.
    pub fn default_of(ty: &'static dyn CustomType) -> Self {
        Self::new(ty, ty.default_fields())
    }

    pub fn ty(&self) -> &'static dyn CustomType {
        self.ty
    }

    /// Whether this is a value of `ty`.
    pub fn is(&self, ty: &dyn CustomType) -> bool {
        self.ty.name() == ty.name() && self.ty.module() == ty.module()
    }

    /// The stored members, in the order the type lists them.
    pub fn fields(&self) -> &[Value] {
        &self.fields
    }

    /// The stored member `i`; `undefined` past the last.
    pub fn field(&self, i: usize) -> &Value {
        const NONE: &Value = &Value::Undefined;
        self.fields.get(i).unwrap_or(NONE)
    }

    /// The member `name`, stored or worked out; `None` when the type has
    /// no such member.
    pub fn member(&self, name: &str) -> Option<Value> {
        let members = self.ty.members();
        let m = members.iter().find(|m| m.name == name)?;
        Some(match m.stored {
            true => self.field(stored_index(members, name)?).clone(),
            false => self.ty.computed(self, name),
        })
    }

    /// Every member, in the order the type lists them: how it crosses to
    /// a script.
    pub(crate) fn all_members(&self) -> Vec<Value> {
        let mut stored = self.fields.iter();
        self.ty
            .members()
            .iter()
            .map(|m| match m.stored {
                true => stored.next().cloned().unwrap_or_default(),
                false => self.ty.computed(self, m.name),
            })
            .collect()
    }

    /// From every member, as [`CustomValue::all_members`] gave them.
    pub(crate) fn from_members(ty: &'static dyn CustomType, all: Vec<Value>) -> Self {
        let fields = ty
            .members()
            .iter()
            .zip(all)
            .filter(|(m, _)| m.stored)
            .map(|(_, v)| v)
            .collect();
        Self::new(ty, fields)
    }

    pub fn text(&self) -> String {
        self.ty.text(self)
    }

    /// Same type and the same fields. A list in a field is compared by
    /// what it holds, which a script cannot tell from one made again.
    pub(crate) fn same(&self, other: &CustomValue) -> bool {
        self.is(other.ty)
            && self.fields.len() == other.fields.len()
            && self
                .fields
                .iter()
                .zip(&other.fields)
                .all(|(a, b)| deep_equals(a, b))
    }
}

fn deep_equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| deep_equals(a, b))
        }
        _ => a.change_equals(b),
    }
}

impl fmt::Debug for CustomValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{:?}", self.ty.name(), self.fields)
    }
}

impl From<CustomValue> for Value {
    fn from(v: CustomValue) -> Self {
        Value::Custom(Arc::new(v))
    }
}

thread_local! {
    static TYPES: RefCell<Vec<&'static dyn CustomType>> = const { RefCell::new(Vec::new()) };
}

/// Puts `ty` in the thread's table; again does nothing.
pub(crate) fn register(ty: &'static dyn CustomType) -> bool {
    TYPES.with(|types| {
        let mut types = types.borrow_mut();
        if types.iter().any(|t| t.name() == ty.name()) {
            return false;
        }
        types.push(ty);
        true
    })
}

/// The type a property names, when a module has registered one.
pub(crate) fn find(name: &str) -> Option<&'static dyn CustomType> {
    TYPES.with(|types| types.borrow().iter().find(|t| t.name() == name).copied())
}

/// The value a property of the type `name` starts with.
pub(crate) fn default_of(name: &Atom) -> Value {
    find(name).map_or(Value::Undefined, |ty| CustomValue::default_of(ty).into())
}

/// `v` for a property of the type `name`.
pub(crate) fn coerce(name: &Atom, v: &Value) -> Option<Value> {
    let ty = find(name)?;
    match v {
        Value::Custom(c) if c.is(ty) => Some(v.clone()),
        _ => ty.coerce(v),
    }
}

/// The text that defines the JS face of `ty` (`__ljos_faces.custom`).
pub(crate) fn face_source(ty: &dyn CustomType) -> String {
    let names = |it: &mut dyn Iterator<Item = &str>| {
        it.map(|n| format!("{n:?}")).collect::<Vec<_>>().join(",")
    };
    let members = ty.members();
    format!(
        "__ljos_faces.custom([{}], [{}], [{}])",
        names(&mut members.iter().map(|m| m.name)),
        members
            .iter()
            .map(|m| if m.shown { "true" } else { "false" })
            .collect::<Vec<_>>()
            .join(","),
        names(&mut ty.methods().iter().copied()),
    )
}

impl crate::Engine {
    /// Makes `ty` a value type of this engine: a property may be declared
    /// with it, and a value of it crosses to a script and back. Again does
    /// nothing.
    pub fn register_value_type(&self, ty: &'static dyn CustomType) {
        register(ty);
        {
            let mut types = self.inner.value_types.borrow_mut();
            if types.iter().any(|t| t.name() == ty.name()) {
                return;
            }
            types.push(ty);
        }
        let js = self.inner.js.borrow().clone();
        if let Some(js) = js
            && let Err(e) = define_face(&js, ty)
        {
            self.warn(format!("the value type {}: {e}", ty.name()));
        }
    }

    /// The value types modules registered with this engine, in that order.
    pub fn value_types(&self) -> Vec<&'static dyn CustomType> {
        self.inner.value_types.borrow().clone()
    }

    /// Defines the faces of the types registered before the host started,
    /// and the functions a face calls back.
    pub(crate) fn install_custom_values(&self, js: &JsState) -> Result<(), EngineError> {
        for ty in self.value_types() {
            define_face(js, ty).map_err(EngineError::js)?;
        }
        let weak = Rc::downgrade(&self.inner);
        let w = weak.clone();
        let text: HostFn = Rc::new(move |scope, args| {
            let engine = engine_of(&w)?;
            let v = engine.custom_arg(scope, args.first())?;
            Ok(JsValue::String(v.text()))
        });
        let w = weak.clone();
        let with: HostFn = Rc::new(move |scope, args| {
            let engine = engine_of(&w)?;
            let v = engine.custom_arg(scope, args.first())?;
            let member = args.get(1).map(|m| scope.display(m)).unwrap_or_default();
            let to = engine.custom_member_arg(scope, args.get(2));
            Ok(match v.ty().with(&v, &member, &to) {
                Some(next) => engine.custom_to_js(&next),
                None => JsValue::Undefined,
            })
        });
        let w = weak;
        let call: HostFn = Rc::new(move |scope, args| {
            let engine = engine_of(&w)?;
            let v = engine.custom_arg(scope, args.first())?;
            let method = args.get(1).map(|m| scope.display(m)).unwrap_or_default();
            let given: Vec<Value> = match args.get(2) {
                Some(JsValue::Array(items)) => items
                    .iter()
                    .map(|a| engine.custom_member_arg(scope, Some(a)))
                    .collect(),
                Some(JsValue::Handle(r)) if r.is_array() => scope
                    .array_items(r)
                    .unwrap_or_default()
                    .iter()
                    .map(|a| engine.custom_member_arg(scope, Some(a)))
                    .collect(),
                _ => Vec::new(),
            };
            match v.ty().call(&v, &method, &given) {
                Ok(out) => Ok(engine.to_js(&out)),
                Err(e) => Err(HostError::Message(e)),
            }
        });
        for (name, f) in [
            ("__ljos_custom_text", text),
            ("__ljos_custom_with", with),
            ("__ljos_custom_call", call),
        ] {
            js.host
                .set_global_function(name, f)
                .map_err(EngineError::js)?;
        }
        Ok(())
    }

    /// A value of a module's type as JS sees it: the face of its kind with
    /// every member.
    pub(crate) fn custom_to_js(&self, v: &CustomValue) -> JsValue {
        let js = self.inner.js.borrow().clone();
        let kind = js.and_then(|js| {
            js.customs
                .borrow()
                .iter()
                .find(|(ty, _)| v.is(*ty))
                .map(|(_, kind)| *kind)
        });
        match kind {
            Some(kind) => JsValue::Typed(
                kind,
                v.all_members().iter().map(|m| self.to_js(m)).collect(),
            ),
            None => JsValue::Undefined,
        }
    }

    /// What a [`JsValue::Typed`] that came back from JS holds: one of the
    /// engine's own value types, or a module's.
    pub(crate) fn typed_from_js(&self, kind: ljos_js::ValueKind, fields: Vec<JsValue>) -> Value {
        let js = self.inner.js.borrow().clone();
        let ty = js.and_then(|js| {
            js.customs
                .borrow()
                .iter()
                .find(|(_, k)| *k == kind)
                .map(|(ty, _)| *ty)
        });
        let Some(ty) = ty else {
            return crate::value_faces::from_js(kind, &fields);
        };
        let members = ty.members();
        let all = fields
            .into_iter()
            .zip(members)
            .map(|(f, m)| match m.stored {
                true => self.plain_from_js(f),
                false => Value::Undefined,
            })
            .collect();
        CustomValue::from_members(ty, all).into()
    }

    /// A field of a value back from JS: what the engine made of it going
    /// out, so nothing in it needs a scope.
    fn plain_from_js(&self, v: JsValue) -> Value {
        match v {
            JsValue::Undefined => Value::Undefined,
            JsValue::Null => Value::Null,
            JsValue::Bool(b) => Value::Bool(b),
            JsValue::Number(n) => Value::Real(n),
            JsValue::String(s) => Value::String(s.into()),
            JsValue::Array(items) => {
                Value::list(items.into_iter().map(|i| self.plain_from_js(i)).collect())
            }
            JsValue::Typed(kind, fields) => self.typed_from_js(kind, fields),
            v @ (JsValue::Color(_) | JsValue::InvalidColor(_)) => {
                Value::Color(crate::js::color_from_js(&v))
            }
            JsValue::Object(id) => Value::Object(crate::js::u64_to_obj(id)),
            JsValue::Handle(r) => Value::Js(crate::value::JsHandle(r)),
            _ => Value::Undefined,
        }
    }

    fn custom_member_arg(&self, scope: &Scope<'_>, v: Option<&JsValue>) -> Value {
        match v {
            Some(v) => self.js_to_value(scope, v.clone(), &TypeTag::Var),
            None => Value::Undefined,
        }
    }

    fn custom_arg(&self, scope: &Scope<'_>, v: Option<&JsValue>) -> Result<CustomValue, HostError> {
        match self.custom_member_arg(scope, v) {
            Value::Custom(c) => Ok((*c).clone()),
            _ => Err(HostError::Type("not a value of a module's type".into())),
        }
    }
}

fn engine_of(w: &std::rc::Weak<crate::engine::Inner>) -> Result<crate::Engine, HostError> {
    w.upgrade()
        .map(crate::Engine::from_inner)
        .ok_or_else(|| HostError::Message("the engine is gone".into()))
}

/// Defines the face of `ty` on the host, once.
fn define_face(js: &JsState, ty: &'static dyn CustomType) -> Result<(), ljos_js::JsFault> {
    if js
        .customs
        .borrow()
        .iter()
        .any(|(t, _)| t.name() == ty.name())
    {
        return Ok(());
    }
    let kind = js.host.define_value_kind(&face_source(ty))?;
    js.customs.borrow_mut().push((ty, kind));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    /// A pair of numbers with a sum worked out from them and a hidden tag.
    #[derive(Debug)]
    struct PairType;
    static PAIR: PairType = PairType;

    impl CustomType for PairType {
        fn name(&self) -> &'static str {
            "testPair"
        }
        fn module(&self) -> &'static str {
            "TestModule"
        }
        fn members(&self) -> &'static [CustomMember] {
            const M: &[CustomMember] = &[
                CustomMember::stored("a", "real"),
                CustomMember::stored("b", "real"),
                CustomMember::computed("sum", "real"),
                CustomMember::stored("tag", "var").hidden(),
            ];
            M
        }
        fn methods(&self) -> &'static [&'static str] {
            &["scaled"]
        }
        fn default_fields(&self) -> Vec<Value> {
            vec![Value::Real(1.0), Value::Real(2.0), Value::Null]
        }
        fn computed(&self, v: &CustomValue, _name: &str) -> Value {
            Value::Real(v.field(0).to_number() + v.field(1).to_number())
        }
        fn text(&self, v: &CustomValue) -> String {
            format!(
                "Pair({}, {})",
                v.field(0).to_number(),
                v.field(1).to_number()
            )
        }
        fn call(&self, v: &CustomValue, method: &str, args: &[Value]) -> Result<Value, String> {
            let k = args.first().map_or(1.0, Value::to_number);
            match method {
                "scaled" => Ok(CustomValue::new(
                    &PAIR,
                    vec![
                        Value::Real(v.field(0).to_number() * k),
                        Value::Real(v.field(1).to_number() * k),
                    ],
                )
                .into()),
                _ => Err(format!("{method} is not a function")),
            }
        }
    }

    fn pair(engine: &Engine, script: &str) -> Value {
        let f = engine
            .eval_js(&format!("(function (p) {{ return {script}; }})"))
            .unwrap();
        engine
            .call_js(&f, &[CustomValue::default_of(&PAIR).into()])
            .unwrap()
    }

    #[test]
    fn a_modules_value_crosses_to_a_script_and_back() {
        let engine = Engine::new();
        engine.register_value_type(&PAIR);
        assert!(TypeTag::parse("testPair") == TypeTag::Custom(Atom::intern("testPair")));
        assert_eq!(
            pair(&engine, "[p.a, p.b, p.sum].join()"),
            Value::String("1,2,3".into())
        );
        assert_eq!(
            pair(&engine, "String(p)"),
            Value::String("Pair(1, 2)".into())
        );
        assert_eq!(
            pair(&engine, "Object.keys(p).join()"),
            Value::String("a,b,sum".into())
        );
        assert_eq!(pair(&engine, "p.scaled(2).sum").to_number(), 6.0);
        assert_eq!(pair(&engine, "Object.isFrozen(p)"), Value::Bool(true));
        match pair(&engine, "p") {
            Value::Custom(c) => {
                assert!(c.is(&PAIR));
                assert_eq!(c.member("sum"), Some(Value::Real(3.0)));
                assert_eq!(c.field(2), &Value::Null);
            }
            other => panic!("not a pair: {other:?}"),
        }
    }
}
