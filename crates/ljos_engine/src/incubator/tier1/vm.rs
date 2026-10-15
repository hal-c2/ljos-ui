//! The register machine of tier 1.
//!
//! One run of one program: a frame of registers, the instructions in
//! order, forward jumps. Every instruction either does exactly what the
//! JavaScript function would do at that point or ends the run with
//! `None`, and the expression runs as JavaScript instead. That is safe
//! because nothing here has an effect JavaScript would not repeat: reads
//! are tracked reads of the same properties in the same order, and the one
//! write a program may make is only noted and handed back.

use std::rc::{Rc, Weak};

use ljos_ir::{Arg, Binding, BindingKind, ExprId, Op, Tier1};
use smallvec::SmallVec;

use super::{MethodFn, Program, builtins, has_js, normal, ops, passed};
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{MethodIndex, ObjectId, PropIndex};
use crate::incubator::Instance;
use crate::meta::PropertyFlags;
use crate::value::{Color, Value};

/// What a program came to.
pub(super) struct Done {
    pub(super) value: Value,
    /// The write it ends with, not made yet.
    pub(super) write: Option<(ObjectId, PropIndex, Value)>,
}

pub(super) struct Run<'a> {
    pub(super) e: &'a Engine,
    pub(super) inst: &'a Instance,
    pub(super) b: &'a Binding,
    pub(super) t: &'a Tier1,
    pub(super) p: &'a Program,
    /// The arguments of the call (a signal's, a method's).
    pub(super) args: &'a [Value],
    /// The arguments are registers of the program that made the call, so
    /// an array among them is the caller's own.
    pub(super) called: bool,
}

/// What a member of an object is, found while the store is borrowed and
/// read after.
enum Member {
    Read(PropIndex),
    /// The member of a grouped property (`border.width`), which is a
    /// property of the object under both names.
    Leaf(PropIndex),
    /// The Item parent, which objects have without declaring it.
    Parent,
    /// A grouped property: the object itself stands for it.
    Group,
}

impl Run<'_> {
    /// Runs the program. `None`: this is one for JavaScript.
    pub(super) fn run(&self) -> Option<Done> {
        let code = &self.t.code;
        let mut frame: SmallVec<[Value; 16]> = SmallVec::new();
        frame.resize(usize::from(self.t.registers), Value::Undefined);
        // The slice once: every instruction indexes it.
        let r = frame.as_mut_slice();
        let consts = self.p.consts.as_slice();
        let mut write = None;
        let mut pc = 0usize;
        loop {
            let at = pc;
            let op = *code.get(at)?;
            pc += 1;
            match op {
                Op::Const { dst, k } => {
                    r[usize::from(dst)] = match &consts[usize::from(k)] {
                        // Most constants are numbers: nothing to count.
                        Value::Real(x) => Value::Real(*x),
                        other => other.clone(),
                    };
                }
                Op::Move { dst, src } => r[usize::from(dst)] = r[usize::from(src)].clone(),
                Op::Pass { reg } => {
                    let v = std::mem::take(&mut r[usize::from(reg)]);
                    r[usize::from(reg)] = passed(self.e, v);
                }
                Op::Param { dst, param } => r[usize::from(dst)] = self.param(param)?,
                Op::Local { dst, index } => {
                    r[usize::from(dst)] =
                        self.args
                            .get(usize::from(index))
                            .map_or(Value::Undefined, |v| {
                                if self.called {
                                    passed(self.e, v.clone())
                                } else {
                                    normal(self.e, v.clone())
                                }
                            });
                }
                Op::This { dst } => {
                    r[usize::from(dst)] = match self.inst.resolve(self.b.scope) {
                        Some(o) if self.e.is_alive(o) => Value::Object(o),
                        Some(_) => Value::Null,
                        None => Value::Undefined,
                    };
                }
                Op::Get {
                    dst,
                    obj,
                    path,
                    len,
                } => {
                    let v = self.get(&r[usize::from(obj)], usize::from(path), usize::from(len))?;
                    r[usize::from(dst)] = v;
                }
                Op::Index { dst, obj, index } => {
                    let (of, at) = (&r[usize::from(obj)], &r[usize::from(index)]);
                    let v = match of {
                        Value::Js(h) if h.0.is_array() => self.entry(&h.0, at)?,
                        _ => element(of, at)?,
                    };
                    r[usize::from(dst)] = v;
                }
                Op::EnumKey { dst, param, key } => {
                    r[usize::from(dst)] = self.enum_key(param, self.name(key))?;
                }
                Op::QtKey { dst, key } => r[usize::from(dst)] = Value::Real(self.p.qt_key(key)?),
                Op::Binary { op, dst, a, b } => {
                    let v = match (&r[usize::from(a)], &r[usize::from(b)]) {
                        // Two numbers, which is what most bindings compute
                        // with: no coercion to go through.
                        (Value::Real(x), Value::Real(y)) => match ops::numbers(op, *x, *y) {
                            Some(v) => v,
                            None => ops::binary(op, &r[usize::from(a)], &r[usize::from(b)])?,
                        },
                        (x, y) => ops::binary(op, x, y)?,
                    };
                    r[usize::from(dst)] = v;
                }
                Op::Unary { op, dst, src } => {
                    let v = ops::unary(op, &r[usize::from(src)])?;
                    r[usize::from(dst)] = v;
                }
                Op::Jump { to } => pc = to as usize,
                Op::JumpIf { cond, to } => {
                    if r[usize::from(cond)].to_boolean() {
                        pc = to as usize;
                    }
                }
                Op::JumpIfNot { cond, to } => {
                    if !r[usize::from(cond)].to_boolean() {
                        pc = to as usize;
                    }
                }
                Op::JumpIfNullish { src, to } => {
                    if r[usize::from(src)].is_nullish() {
                        pc = to as usize;
                    }
                }
                Op::Concat { dst, base, count } => {
                    let from = usize::from(base);
                    let v = ops::concat(&r[from..from + usize::from(count)])?;
                    r[usize::from(dst)] = v;
                }
                Op::Array { dst, base, count } => {
                    let from = usize::from(base);
                    let v = Value::list(r[from..from + usize::from(count)].to_vec());
                    r[usize::from(dst)] = v;
                }
                Op::Call {
                    dst,
                    f,
                    base,
                    count,
                } => {
                    let from = usize::from(base);
                    let v = builtins::call(self.e, f, &r[from..from + usize::from(count)])?;
                    r[usize::from(dst)] = v;
                }
                Op::Check { src, f } => {
                    if !builtins::check(f, &r[usize::from(src)]) {
                        return None;
                    }
                }
                Op::Guard { obj, name, expr } => {
                    let Value::Object(o) = r[usize::from(obj)] else {
                        return None;
                    };
                    self.guard(o, self.name(name), expr)?;
                }
                Op::Target {
                    dst,
                    grouped,
                    obj,
                    name,
                } => {
                    let Value::Object(o) = r[usize::from(obj)] else {
                        return None;
                    };
                    let (target, group) = self.target(o, self.name(name))?;
                    r[usize::from(dst)] = Value::Object(target);
                    r[usize::from(grouped)] = Value::Bool(group);
                }
                Op::Set {
                    obj,
                    name,
                    val,
                    group,
                } => {
                    if write.is_some() {
                        return None;
                    }
                    let Value::Object(o) = r[usize::from(obj)] else {
                        // A write to a member of `null` throws; one to a
                        // member of a value is the script's to do.
                        return None;
                    };
                    let name = match group {
                        Some((flag, _)) if matches!(r[usize::from(flag)], Value::Bool(true)) => {
                            self.p.grouped(at)?
                        }
                        _ => self.name(name),
                    };
                    let p = self.writable(o, name)?;
                    let v = r[usize::from(val)].clone();
                    if has_js(&v) {
                        return None;
                    }
                    write = Some((o, p, v));
                }
                Op::Invoke {
                    dst,
                    obj,
                    name,
                    base,
                    count,
                } => {
                    let Value::Object(o) = r[usize::from(obj)] else {
                        return None;
                    };
                    let from = usize::from(base);
                    let v = self.invoke(o, self.name(name), &r[from..from + usize::from(count)])?;
                    r[usize::from(dst)] = v;
                }
                Op::Return { src } => {
                    let value = std::mem::take(&mut r[usize::from(src)]);
                    // A JS value keeps its identity through the JS call
                    // and back, in a way a register does not vouch for.
                    if has_js(&value) {
                        return None;
                    }
                    return Some(Done { value, write });
                }
            }
        }
    }

    fn name(&self, index: u16) -> &str {
        &self.t.names[usize::from(index)]
    }

    /// `array[index]` for the array a script holds. Only an index that is
    /// one: anything else names a property a script may have given it.
    fn entry(&self, array: &ljos_js::JsRef, index: &Value) -> Option<Value> {
        let Value::Real(n) = index else {
            return None;
        };
        if n.fract() != 0.0 || *n < 0.0 || *n > f64::from(u32::MAX) {
            return None;
        }
        // What the script reads is the entry as it is, a `-0` too: it did
        // not go through a property on its way.
        match self.e.array_entry(array, *n as usize)? {
            // What a script is given for an object that is gone is not
            // decided here.
            Value::Object(o) if !self.e.is_alive(o) => None,
            v => Some(v),
        }
    }

    /// The parameter `n`, as the JS function would be passed it.
    fn param(&self, n: u16) -> Option<Value> {
        let (inst, arg) = self.param_arg(n)?;
        self.arg(inst, arg)
    }

    /// The parameter `n` and the instance it is resolved from: the
    /// binding's own, or past those the ones of inlined functions.
    fn param_arg(&self, n: u16) -> Option<(&Instance, &Arg)> {
        let n = usize::from(n);
        let own = self.b.params.len();
        if n < own {
            return Some((self.inst, &self.b.params[n].arg));
        }
        let a = self.t.args.get(n - own)?;
        Some((self.inst.find(a.component)?, &a.arg))
    }

    /// Mirrors `Engine::arg_js`, for what has a value in a register.
    fn arg(&self, inst: &Instance, arg: &Arg) -> Option<Value> {
        match arg {
            Arg::Property { object, name } => {
                let v = self.e.arg_value(inst, arg);
                if matches!(v, Some(Value::List(_)))
                    && let Some(o) = inst.resolve(*object)
                    && let Ok(p) = self.e.property_index(o, name)
                    && self.hands_on(o, p)
                {
                    return None;
                }
                Some(v.map_or(Value::Undefined, |v| normal(self.e, v)))
            }
            Arg::Object(_) | Arg::Attached { .. } => Some(
                self.e
                    .arg_value(inst, arg)
                    .map_or(Value::Undefined, |v| normal(self.e, v)),
            ),
            Arg::Context {
                component,
                name,
                fallback,
            } => match self.e.context_arg(inst, *component, name) {
                Some(v) => Some(normal(self.e, v)),
                // Without a fallback JavaScript looks at its globals.
                None => self.arg(inst, fallback.as_deref()?),
            },
            Arg::Dynamic(name) => self
                .e
                .context_name(inst.context, name)
                .map(|v| normal(self.e, v)),
            // Made here if this is the first read.
            Arg::Singleton(ty) => self.e.singleton_object(ty).map(Value::Object),
            Arg::Type(_) | Arg::Namespace { .. } | Arg::Holder { .. } => None,
        }
    }

    /// `Type.Key`: the last key of that name among the type's enums, as
    /// the object JavaScript is given has it.
    fn enum_key(&self, param: u16, key: &str) -> Option<Value> {
        let (inst, arg) = self.param_arg(param)?;
        let (Arg::Type(ty) | Arg::Singleton(ty)) = arg else {
            return None;
        };
        let meta = self.e.type_meta(&inst.file, ty).ok()?;
        Some(
            meta.enums
                .iter()
                .flat_map(|e| e.keys.iter())
                .rev()
                .find(|(k, _)| k.as_str() == key)
                .map_or(Value::Undefined, |(_, v)| Value::Real(*v as f64)),
        )
    }

    /// What the member `name` of the object `o` is. `None` for what only
    /// JavaScript answers: a method, a signal, a name the object lacks.
    /// For a grouped property `pair` is the name of the property its
    /// member is (`border.width`).
    fn member(&self, o: ObjectId, name: &str, pair: Option<&str>) -> Option<Member> {
        let store = self.e.inner.store.borrow();
        let meta = &store.objects.get(o)?.meta;
        if meta.method_index(name).is_some() || name == "toString" || name == "destroy" {
            return None;
        }
        let Some(p) = meta.property_index(name) else {
            return (name == "parent").then_some(Member::Parent);
        };
        if !meta.property(p)?.flags.contains(PropertyFlags::GROUP) {
            return Some(Member::Read(p));
        }
        let Some(pair) = pair else {
            return Some(Member::Group);
        };
        let leaf = meta.property_index(pair)?;
        if meta.property(leaf)?.flags.contains(PropertyFlags::GROUP) {
            return None;
        }
        Some(Member::Leaf(leaf))
    }

    /// A property of a live object, read as the JS host reads it.
    fn read(&self, o: ObjectId, p: PropIndex) -> Option<Value> {
        match self.e.try_read(o, p) {
            Ok(Value::List(_)) if self.hands_on(o, p) => None,
            Ok(v) => Some(v),
            // Destroyed by something the read ran.
            Err(EngineError::DeadObject(_)) => Some(Value::Undefined),
            Err(_) => None,
        }
    }

    /// Whether what is read is a list a function may hand on. To a
    /// script a `list<T>` declared in QML is the property's own list
    /// (list_faces.rs), which what the function returns must be then,
    /// where a register holds a copy of the entries.
    fn hands_on(&self, o: ObjectId, p: PropIndex) -> bool {
        self.b.kind == BindingKind::Function && self.e.script_list(o, p).is_some()
    }

    /// `base.a.b.c`.
    fn get(&self, base: &Value, path: usize, len: usize) -> Option<Value> {
        let names = self.t.names.get(path..path + len)?;
        let mut cur = base.clone();
        let mut i = 0;
        while i < names.len() {
            let name = names[i].as_str();
            cur = match &cur {
                Value::Object(o) => {
                    let pair = (i + 1 < names.len())
                        .then(|| self.p.pairs.get(path + i).map(|s| &**s))
                        .flatten();
                    match self.member(*o, name, pair)? {
                        Member::Read(p) => self.read(*o, p)?,
                        Member::Leaf(p) => {
                            // Two names of the path.
                            i += 1;
                            self.read(*o, p)?
                        }
                        Member::Parent => {
                            self.e.visual_parent(*o).map_or(Value::Null, Value::Object)
                        }
                        // A group on its own is a JS object the engine
                        // makes for the occasion.
                        Member::Group => return None,
                    }
                }
                // The array a script holds: how long it is now.
                Value::Js(h) if h.0.is_array() && name == "length" => {
                    Value::Real(self.e.array_len(&h.0)? as f64)
                }
                other => field(other, name)?,
            };
            i += 1;
        }
        Some(normal(self.e, cur))
    }

    /// The object a write to `o.name.member` goes to, and whether `name`
    /// is a group of `o` (then the object is `o` itself).
    fn target(&self, o: ObjectId, name: &str) -> Option<(ObjectId, bool)> {
        match self.member(o, name, None)? {
            Member::Group => Some((o, true)),
            Member::Read(p) | Member::Leaf(p) => match self.e.try_read(o, p) {
                // A value type is written back whole, which JavaScript
                // does through a stand-in object; `null` throws.
                Ok(Value::Object(target)) => Some((target, false)),
                _ => None,
            },
            Member::Parent => Some((self.e.visual_parent(o)?, false)),
        }
    }

    /// The property a write to `o.name` goes to.
    fn writable(&self, o: ObjectId, name: &str) -> Option<PropIndex> {
        let store = self.e.inner.store.borrow();
        let meta = &store.objects.get(o)?.meta;
        if meta.method_index(name).is_some() {
            return None;
        }
        let p = meta.property_index(name)?;
        (!meta.property(p)?.flags.contains(PropertyFlags::GROUP)).then_some(p)
    }

    /// The function of a script `o` has by `name`, as it is installed.
    fn script_method(&self, o: ObjectId, name: &str) -> Option<(MethodIndex, Rc<MethodFn>)> {
        let store = self.e.inner.store.borrow();
        let h = store.objects.get(o)?;
        if h.meta.property_index(name).is_some() {
            return None;
        }
        let m = h.meta.method_index(name)?;
        if h.meta.method(m)?.thunk.is_some() {
            return None;
        }
        let f = h
            .ext
            .as_ref()?
            .methods
            .iter()
            .find(|(i, _)| *i == m)
            .map(|(_, f)| f.clone())?;
        Some((m, f))
    }

    /// `o.name(args)` where that is a function with a program of its own
    /// that writes nothing: the program's result. It reads what the JS
    /// function would read, in its order, so stopping in it is stopping
    /// here.
    fn invoke(&self, o: ObjectId, name: &str, args: &[Value]) -> Option<Value> {
        /// A function that calls itself goes on until JavaScript says so.
        const MAX_DEPTH: u8 = 8;
        if args.iter().any(has_js) {
            return None;
        }
        let (m, f) = self.script_method(o, name)?;
        let (inst, expr) = {
            let methods = self.e.inner.tier1.methods.borrow();
            let rec = methods.get(&(o, m))?;
            if !std::ptr::addr_eq(Weak::as_ptr(&rec.f), Rc::as_ptr(&f)) {
                return None;
            }
            (rec.weak.upgrade()?, rec.expr)
        };
        let file = &inst.file;
        let p = file.tier1.by_expr.get(expr.index())?.as_ref()?;
        if !p.pure || p.gave_up() {
            return None;
        }
        let b = file.ir.expr(expr);
        let t = b.tier1.as_ref()?;
        let depth = &self.e.inner.tier1.invoking;
        if depth.get() >= MAX_DEPTH {
            return None;
        }
        depth.set(depth.get() + 1);
        let done = Run {
            e: self.e,
            inst: &inst,
            b,
            t,
            p,
            args,
            called: true,
        }
        .run();
        depth.set(depth.get() - 1);
        let done = done?;
        // A program that writes is not run: `pure` says it has no write.
        done.write.is_none().then_some(done.value)
    }

    /// Whether the function `o` has by `name` is `expr` of this file, the
    /// one whose body the program has in place of the call.
    fn guard(&self, o: ObjectId, name: &str, expr: ExprId) -> Option<()> {
        let (m, f) = self.script_method(o, name)?;
        let methods = self.e.inner.tier1.methods.borrow();
        let rec = methods.get(&(o, m))?;
        let file = &self.inst.file;
        // The instance the function's own parameters are resolved from
        // when it is called, which is where the inlined ones are too.
        let callee = self.inst.find(file.ir.expr(expr).scope.component)?;
        let same = std::ptr::addr_eq(Weak::as_ptr(&rec.f), Rc::as_ptr(&f))
            && std::ptr::eq(rec.file, Rc::as_ptr(file))
            && rec.expr == expr
            && std::ptr::eq(rec.inst, callee);
        same.then_some(())
    }
}

/// `obj[index]` for an array or a string and a number.
fn element(obj: &Value, index: &Value) -> Option<Value> {
    let Value::Real(n) = index else {
        // A string index names a member, and may name a method.
        return None;
    };
    let at = |len: usize| {
        // An index is an integer; anything else names a property neither
        // an array nor a string has.
        (n.fract() == 0.0 && *n >= 0.0 && *n < len as f64).then_some(*n as usize)
    };
    match obj {
        Value::List(items) => Some(at(items.len()).map_or(Value::Undefined, |i| items[i].clone())),
        Value::String(s) => {
            if s.is_ascii() {
                return Some(at(s.len()).map_or(Value::Undefined, |i| Value::string(&s[i..=i])));
            }
            let Some(i) = at(ops::utf16_len(s)) else {
                return Some(Value::Undefined);
            };
            let unit = s.encode_utf16().nth(i)?;
            // Half of a pair is not a string Rust has.
            let c = char::from_u32(u32::from(unit))?;
            Some(Value::string(c.to_string()))
        }
        _ => None,
    }
}

/// A member of a value that is not an object.
fn field(v: &Value, name: &str) -> Option<Value> {
    let d = |x: f64| Some(Value::Real(x + 0.0));
    let s = |x: f32| Some(Value::Real(f64::from(x + 0.0)));
    match v {
        Value::String(text) | Value::Url(text) => {
            (name == "length").then(|| Value::Real(ops::utf16_len(text) as f64))
        }
        Value::List(items) => (name == "length").then(|| Value::Real(items.len() as f64)),
        Value::Point(p) => match name {
            "x" => d(p.x),
            "y" => d(p.y),
            _ => None,
        },
        Value::IntPoint(p) => match name {
            "x" => d(f64::from(p.x)),
            "y" => d(f64::from(p.y)),
            _ => None,
        },
        Value::Size(z) => match name {
            "width" => d(z.width),
            "height" => d(z.height),
            _ => None,
        },
        Value::Rect(q) => {
            let (x, y, width, height) = (q.x + 0.0, q.y + 0.0, q.width + 0.0, q.height + 0.0);
            match name {
                "x" | "left" => d(x),
                "y" | "top" => d(y),
                "width" => d(width),
                "height" => d(height),
                "right" => Some(Value::Real(x + width)),
                "bottom" => Some(Value::Real(y + height)),
                _ => None,
            }
        }
        Value::Vector2d(q) => match name {
            "x" => s(q.x),
            "y" => s(q.y),
            _ => None,
        },
        Value::Vector3d(q) => match name {
            "x" => s(q.x),
            "y" => s(q.y),
            "z" => s(q.z),
            _ => None,
        },
        Value::Vector4d(q) => match name {
            "x" => s(q.x),
            "y" => s(q.y),
            "z" => s(q.z),
            "w" => s(q.w),
            _ => None,
        },
        Value::Quaternion(q) => match name {
            "scalar" => s(q.scalar),
            "x" => s(q.x),
            "y" => s(q.y),
            "z" => s(q.z),
            _ => None,
        },
        Value::Font(f) => match name {
            "family" => Some(Value::String(f.family.clone())),
            "bold" => Some(Value::Bool(f.bold())),
            "weight" => Some(Value::Real(f64::from(f.weight))),
            "italic" => Some(Value::Bool(f.italic)),
            "underline" => Some(Value::Bool(f.underline)),
            "strikeout" => Some(Value::Bool(f.strikeout)),
            "pointSize" => d(f.point_size),
            "pixelSize" => Some(Value::Real(f64::from(f.pixel_size))),
            "letterSpacing" => d(f.letter_spacing),
            "wordSpacing" => d(f.word_spacing),
            "capitalization" => Some(Value::Real(f64::from(f.capitalization))),
            "hintingPreference" => Some(Value::Real(f64::from(f.hinting_preference))),
            "scaleWithSystem" => Some(Value::Bool(f.scale_with_system)),
            "styleName" => Some(Value::String(f.style_name.clone())),
            "overline" => Some(Value::Bool(f.overline)),
            "kerning" => Some(Value::Bool(f.kerning)),
            "preferShaping" => Some(Value::Bool(f.prefer_shaping)),
            "contextFontMerging" => Some(Value::Bool(f.context_font_merging)),
            "preferTypoLineMetrics" => Some(Value::Bool(f.prefer_typo_line_metrics)),
            "features" if matches!(f.features, Value::Js(_)) => Some(f.features.clone()),
            "variableAxes" if matches!(f.variable_axes, Value::Js(_)) => {
                Some(f.variable_axes.clone())
            }
            _ => None,
        },
        Value::Color(c) => color_field(*c, name),
        // `null.x` throws; a number's or a boolean's members are methods.
        _ => None,
    }
}

/// A channel of a colour is a single-precision fraction, as the JS colour
/// object has it.
fn color_field(c: Color, name: &str) -> Option<Value> {
    let part = |ch: u16| Some(Value::Real(f64::from((f64::from(ch) / 65535.0) as f32)));
    match name {
        "r" => part(c.r),
        "g" => part(c.g),
        "b" => part(c.b),
        "a" => part(c.a),
        "valid" => Some(Value::Bool(true)),
        _ => None,
    }
}
