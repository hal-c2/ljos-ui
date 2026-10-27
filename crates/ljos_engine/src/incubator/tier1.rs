//! Tier 1: bindings as register bytecode, run in Rust (ARCHITECTURE.md
//! §5.4).
//!
//! The compiler writes a [`Tier1`] program beside the JavaScript function
//! of every expression that is wholly inside the subset (`ljos_ir::tier1`).
//! [`Engine::tier1_eval`] runs it between tier 0 (a bare property path) and
//! tier 2 (the JS function): [`vm`] is the machine, [`ops`] the operators
//! with JavaScript's coercions, [`builtins`] the functions it has in Rust.
//!
//! # Exact or not at all
//!
//! A program means what its JS function means. Where the machine cannot be
//! sure of doing exactly what JavaScript would (an object where a
//! coercion is needed, a member the object does not declare, a builtin
//! given something it would throw for), it stops and the expression runs
//! as JavaScript, whole, as if tier 1 had not been tried. That is safe
//! because a program has no effect before its end: what it read, tier 2
//! reads again in the same order, and the one write a program may make is
//! handed back to be made after the program is done.
//!
//! # Dependencies
//!
//! The machine reads through the same engine calls the JS host does
//! ([`Engine::try_read`], [`Engine::visual_parent`], the context lookups),
//! so a binding depends on exactly what it would in tier 2: a branch not
//! taken reads nothing.
//!
//! # The cross-check
//!
//! With the check on (debug builds, or `LJOS_TIER1_CHECK=1`) every
//! evaluation that tier 1 completes is run in tier 2 as well. The two
//! values, and the two writes, are compared; a difference is reported as
//! a warning with the expression and both, and tier 2's result is the one
//! used. `LJOS_TIER1=0` (or [`Engine::set_tier1`]) turns tier 1 off.
//!
//! # What it takes for granted
//!
//! That the globals a program's builtins stand for are the engine's: a
//! script that assigns `Math.max` or `qsTr` is not followed.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::rc::{Rc, Weak};
use std::sync::OnceLock;

use ljos_ir::{Binding, CompiledFile, Const, ExprId, Op, Reg, Tier1};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::WriteKind;
use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{MethodIndex, ObjectId, PropIndex};
use crate::incubator::Instance;
use crate::loader::LoadedFile;
use crate::meta::MetaRef;
use crate::types::TypeTag;
use crate::value::Value;

mod builtins;
mod ops;
mod vm;

pub(crate) use builtins::round;
pub(crate) use ops::{compare, pow, utf16_len};
pub(crate) use vm::field as member_of;

/// What tier 1 has done in one engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tier1Stats {
    /// Evaluations tier 1 completed.
    pub runs: u64,
    /// Evaluations it began and gave to tier 2.
    pub bails: u64,
    /// Completed evaluations that were run in tier 2 as well and compared.
    pub checked: u64,
    /// Of those, the ones where the two tiers differed.
    pub mismatches: u64,
}

/// The environment's say, read once: tests set these for a whole run.
struct Env {
    enabled: bool,
    check: Option<bool>,
    log: Option<std::path::PathBuf>,
}

fn env() -> &'static Env {
    static ENV: OnceLock<Env> = OnceLock::new();
    ENV.get_or_init(|| {
        let flag = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| !matches!(v.trim(), "0" | "off" | "false" | "no" | ""))
        };
        Env {
            enabled: flag("LJOS_TIER1").unwrap_or(true),
            check: flag("LJOS_TIER1_CHECK"),
            log: std::env::var_os("LJOS_TIER1_LOG")
                .filter(|p| !p.is_empty())
                .map(Into::into),
        }
    })
}

/// A line for the file `LJOS_TIER1_LOG` names, which collects what the
/// cross-check found over a run of many engines.
fn log_line(line: &str) {
    use std::io::Write as _;
    let Some(path) = &env().log else {
        return;
    };
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        // One write, so lines of engines on other threads stay whole.
        let _ = f.write_all(format!("{line}\n").as_bytes());
    }
}

/// A write a JS function made, as the cross-check sees it.
type Written = (ObjectId, PropIndex, Value);

/// The engine's tier-1 state.
pub(crate) struct State {
    enabled: Cell<bool>,
    check: Cell<bool>,
    /// The cross-check runs an expression again for tier 2: the one on
    /// top is to skip tier 1 when it comes by.
    skip: RefCell<Vec<(usize, u32)>>,
    stats: Cell<Tier1Stats>,
    /// The functions programs have inlined, by the object and method they
    /// were installed as: what [`Op::Guard`] asks.
    methods: RefCell<FxHashMap<(ObjectId, MethodIndex), MethodRecord>>,
    /// The size of `methods` at which records of dead closures are next
    /// dropped.
    sweep_at: Cell<usize>,
    /// While the cross-check runs tier 2: the writes its JS made.
    spy: RefCell<Option<Vec<Written>>>,
    spying: Cell<bool>,
    /// How many programs are running inside one another ([`Op::Invoke`]).
    invoking: Cell<u8>,
}

impl Default for State {
    fn default() -> Self {
        let env = env();
        State {
            enabled: Cell::new(env.enabled),
            check: Cell::new(env.check.unwrap_or(cfg!(debug_assertions))),
            skip: RefCell::new(Vec::new()),
            stats: Cell::new(Tier1Stats::default()),
            methods: RefCell::new(FxHashMap::default()),
            sweep_at: Cell::new(64),
            spy: RefCell::new(None),
            spying: Cell::new(false),
            invoking: Cell::new(0),
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        let s = self.stats.get();
        if s != Tier1Stats::default() {
            log_line(&format!(
                "stats runs={} bails={} checked={} mismatches={}",
                s.runs, s.bails, s.checked, s.mismatches
            ));
        }
    }
}

impl State {
    fn count(&self, f: impl FnOnce(&mut Tier1Stats)) {
        let mut s = self.stats.get();
        f(&mut s);
        self.stats.set(s);
    }
}

/// What a [`crate::object::ScriptMethod`] points to.
type MethodFn = dyn Fn(&Engine, ObjectId, &[Value]) -> Result<Value, EngineError>;

/// A function as it was installed on an object.
struct MethodRecord {
    /// The closure the object calls. Weak: the record does not keep the
    /// instance alive, and a closure that is gone matches nothing.
    f: Weak<MethodFn>,
    /// The instance the closure runs the function in. Only compared: the
    /// closure owns it, so while the closure lives the address is its.
    inst: *const Instance,
    file: *const LoadedFile,
    expr: ExprId,
    /// The same instance, for a program of another file that runs the
    /// function's own ([`Op::Invoke`]).
    weak: Weak<Instance>,
}

/// A function about to be installed that some program has inlined, or
/// that a program of another file may run.
pub(crate) struct Noted {
    inst: *const Instance,
    file: *const LoadedFile,
    expr: ExprId,
    weak: Weak<Instance>,
}

impl Noted {
    /// A record of `expr` installed for `inst`, whoever inlined it.
    pub(crate) fn new(inst: &Rc<Instance>, expr: ExprId) -> Noted {
        Noted {
            inst: Rc::as_ptr(inst),
            file: Rc::as_ptr(&inst.file),
            expr,
            weak: Rc::downgrade(inst),
        }
    }

    /// `Some` when a program of the file guards `expr`, or `expr` has a
    /// program another may run in place: only those are worth a record.
    pub(crate) fn of(inst: &Rc<Instance>, expr: ExprId) -> Option<Noted> {
        let programs = &inst.file.tier1;
        let worth = programs.guarded.contains(&expr)
            || programs
                .by_expr
                .get(expr.index())
                .and_then(Option::as_ref)
                .is_some_and(|p| p.pure);
        worth.then(|| Noted::new(inst, expr))
    }
}

/// The programs of a file, ready to run.
pub(crate) struct Programs {
    by_expr: Vec<Option<Program>>,
    /// The functions some program has inlined.
    guarded: HashSet<ExprId>,
}

impl Programs {
    pub(crate) fn new(ir: &CompiledFile) -> Self {
        let by_expr: Vec<Option<Program>> = ir
            .exprs
            .iter()
            .map(|b| {
                let t = b.tier1.as_ref()?;
                Program::new(b, t, ir.exprs.len())
            })
            .collect();
        let mut guarded = HashSet::new();
        for (b, p) in ir.exprs.iter().zip(&by_expr) {
            if let (Some(t), Some(_)) = (&b.tier1, p) {
                for op in &t.code {
                    if let Op::Guard { expr, .. } = op {
                        guarded.insert(*expr);
                    }
                }
            }
        }
        Programs { by_expr, guarded }
    }
}

/// What the machine needs of one program besides its code, worked out
/// when the file is loaded.
pub(crate) struct Program {
    consts: Vec<Value>,
    /// `names[i].names[i + 1]`: the name of the property a member of a
    /// grouped property is.
    pairs: Vec<Box<str>>,
    /// Per [`Op::Set`] that may write to a group's member, by the index
    /// of the instruction: the name of the property that is.
    sets: Vec<(u32, Box<str>)>,
    /// The value of each `Qt.<key>` the program reads, by the index of the
    /// name. `None`: not a key this knows, which may be anything in JS.
    qt: Vec<(u16, Option<f64>)>,
    /// It writes nothing: a program may run it in the middle of its own.
    pure: bool,
    oks: Cell<u32>,
    bails: Cell<u32>,
    /// Per name, what it was found to be on the last types it was looked
    /// up on: a lookup by name costs more than the read it is for.
    looked: Box<[RefCell<Looks>]>,
}

/// The types one name was last looked up on, and what it was on each: a
/// name is mostly read off one or two types.
type Looks = SmallVec<[(MetaRef, Key, Looked); 2]>;

/// What a name was looked up as.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    /// A member, with or without a property it is grouped with.
    Member(bool),
    Method,
    /// The property a write of the name goes to.
    Write,
}

#[derive(Clone, Copy)]
enum Looked {
    Member(Option<vm::Member>),
    Method(Option<MethodIndex>),
    Write(Option<PropIndex>),
}

/// The types a name is remembered for: a program is shared by the
/// instances of its file, which a derived type's objects are too.
const LOOKED_TYPES: usize = 4;

impl Program {
    /// `None` when the bytecode is not sound (a register past the frame, a
    /// jump backwards): it is data read from a file, and the machine
    /// indexes by it.
    fn new(b: &Binding, t: &Tier1, exprs: usize) -> Option<Program> {
        let registers = usize::from(t.registers);
        let reg = |r: Reg| usize::from(r) < registers;
        let span = |base: Reg, count: usize| usize::from(base) + count <= registers;
        let name = |n: u16| usize::from(n) < t.names.len();
        let param = |n: u16| usize::from(n) < b.params.len() + t.args.len();
        let mut sets = Vec::new();
        let mut qt: Vec<(u16, Option<f64>)> = Vec::new();
        for (at, op) in t.code.iter().enumerate() {
            let jump = |to: u32| (to as usize) > at && (to as usize) < t.code.len();
            let sound = match *op {
                Op::Const { dst, k } => reg(dst) && usize::from(k) < t.consts.len(),
                Op::Move { dst, src } => reg(dst) && reg(src),
                Op::Param { dst, param: n } => reg(dst) && param(n),
                Op::Local { dst, .. } | Op::This { dst } | Op::Pass { reg: dst } => reg(dst),
                Op::Get {
                    dst,
                    obj,
                    path,
                    len,
                } => {
                    reg(dst)
                        && reg(obj)
                        && len > 0
                        && usize::from(path) + usize::from(len) <= t.names.len()
                }
                Op::Index { dst, obj, index } => reg(dst) && reg(obj) && reg(index),
                Op::EnumKey { dst, param: n, key } => reg(dst) && param(n) && name(key),
                Op::QtKey { dst, key } => {
                    if name(key) && !qt.iter().any(|(k, _)| *k == key) {
                        qt.push((key, qt_key(&t.names[usize::from(key)])));
                    }
                    reg(dst) && name(key)
                }
                Op::Binary { dst, a, b, .. } => reg(dst) && reg(a) && reg(b),
                Op::Unary { dst, src, .. } => reg(dst) && reg(src),
                Op::Jump { to } => jump(to),
                Op::JumpIf { cond, to } | Op::JumpIfNot { cond, to } => reg(cond) && jump(to),
                Op::JumpIfNullish { src, to } => reg(src) && jump(to),
                Op::Concat { dst, base, count } | Op::Array { dst, base, count } => {
                    reg(dst) && span(base, usize::from(count))
                }
                Op::Call {
                    dst, base, count, ..
                } => reg(dst) && span(base, usize::from(count)),
                Op::Check { src, .. } => reg(src),
                Op::Guard { obj, name: n, expr } => reg(obj) && name(n) && expr.index() < exprs,
                Op::Target {
                    dst,
                    grouped,
                    obj,
                    name: n,
                } => reg(dst) && reg(grouped) && reg(obj) && name(n),
                Op::Set {
                    obj,
                    name: n,
                    val,
                    group,
                } => {
                    let group_ok = match group {
                        Some((flag, through)) => {
                            if name(n) && name(through) {
                                sets.push((
                                    at as u32,
                                    format!(
                                        "{}.{}",
                                        t.names[usize::from(through)],
                                        t.names[usize::from(n)]
                                    )
                                    .into_boxed_str(),
                                ));
                            }
                            reg(flag) && name(through)
                        }
                        None => true,
                    };
                    reg(obj) && name(n) && reg(val) && group_ok
                }
                Op::Return { src } => reg(src),
                Op::Invoke {
                    dst,
                    obj,
                    name: n,
                    base,
                    count,
                } => reg(dst) && reg(obj) && name(n) && span(base, usize::from(count)),
            };
            if !sound {
                return None;
            }
        }
        let consts = t
            .consts
            .iter()
            .map(|c| match c {
                Const::Undefined => Value::Undefined,
                Const::Null => Value::Null,
                Const::Bool(b) => Value::Bool(*b),
                Const::Number(bits) => Value::Real(f64::from_bits(*bits)),
                Const::String(s) => Value::String(Atom::new(s)),
            })
            .collect();
        let pairs = t
            .names
            .windows(2)
            .map(|w| format!("{}.{}", w[0], w[1]).into_boxed_str())
            .collect();
        Some(Program {
            consts,
            pairs,
            sets,
            qt,
            pure: !t.writes(),
            oks: Cell::new(0),
            bails: Cell::new(0),
            looked: t.names.iter().map(|_| RefCell::default()).collect(),
        })
    }

    /// The property a [`Op::Set`] at `at` writes when its target is a
    /// group.
    fn grouped(&self, at: usize) -> Option<&str> {
        let i = self
            .sets
            .binary_search_by_key(&(at as u32), |(pc, _)| *pc)
            .ok()?;
        Some(&self.sets[i].1)
    }

    /// What the name `name` was found to be on objects of the type `meta`.
    fn looked(&self, name: u16, meta: &MetaRef, key: Key) -> Option<Looked> {
        let seen = self.looked.get(usize::from(name))?.borrow();
        seen.iter()
            .find(|(m, k, _)| *k == key && MetaRef::ptr_eq(m, meta))
            .map(|(_, _, l)| *l)
    }

    fn keep(&self, name: u16, meta: &MetaRef, key: Key, looked: Looked) {
        let Some(seen) = self.looked.get(usize::from(name)) else {
            return;
        };
        let mut seen = seen.borrow_mut();
        if seen.len() >= LOOKED_TYPES {
            seen.remove(0);
        }
        seen.push((meta.clone(), key, looked));
    }

    fn qt_key(&self, key: u16) -> Option<f64> {
        self.qt.iter().find(|(k, _)| *k == key)?.1
    }

    /// A program that keeps stopping costs its reads twice each time: it
    /// is left to tier 2 once that is what it mostly does.
    fn gave_up(&self) -> bool {
        let bails = self.bails.get();
        bails >= 8 && bails > 2 * self.oks.get()
    }

    fn bailed(&self) {
        self.bails.set(self.bails.get().saturating_add(1));
    }

    fn done(&self) {
        self.oks.set(self.oks.get().saturating_add(1));
    }
}

/// `Qt.<key>` when it is a number: an enum key, or one of the colours Qt
/// names by number. Where two share a name the later is the one the `Qt`
/// object ends up with, and the keys are assigned after the colours.
fn qt_key(key: &str) -> Option<f64> {
    const COLORS: &[&str] = &[
        "color0",
        "color1",
        "black",
        "white",
        "darkGray",
        "gray",
        "lightGray",
        "red",
        "green",
        "blue",
        "cyan",
        "magenta",
        "yellow",
        "darkRed",
        "darkGreen",
        "darkBlue",
        "darkCyan",
        "darkMagenta",
        "darkYellow",
        "transparent",
    ];
    crate::js::QT_ENUM_KEYS
        .iter()
        .chain(crate::qt_keys::QT_KEY_ENUMS)
        .rev()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v as f64)
        .or_else(|| COLORS.iter().position(|c| *c == key).map(|i| i as f64))
}

/// Whether a field is the zero JavaScript's `x + 0` does away with.
fn negative_zero(x: f64) -> bool {
    x == 0.0 && x.is_sign_negative()
}

/// A value as a register holds it: what it is once it has gone to
/// JavaScript and come back as a `var`. Numbers are doubles, a URL is its
/// string, a destroyed object is `null`, an array is a new array, and a
/// zero has lost its sign, by itself or as the field of a value type.
pub(super) fn normal(e: &Engine, v: Value) -> Value {
    match v {
        Value::Undefined
        | Value::Null
        | Value::Bool(_)
        | Value::String(_)
        | Value::Color(_)
        | Value::IntPoint(_)
        | Value::Custom(_)
        | Value::Js(_) => v,
        // A number that is an integer goes to JavaScript as one, and an
        // integer zero has no sign.
        Value::Real(r) => Value::Real(r + 0.0),
        Value::Int(i) => Value::Real(f64::from(i)),
        Value::Enum(x) => Value::Real(f64::from(x.value)),
        Value::Url(s) => Value::String(s),
        Value::Object(o) => {
            if e.is_alive(o) {
                v
            } else {
                Value::Null
            }
        }
        Value::List(items) => Value::list(items.iter().map(|i| normal(e, i.clone())).collect()),
        Value::Map(fields) => Value::map(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), normal(e, v.clone()))),
        ),
        Value::Point(p) => {
            if negative_zero(p.x) || negative_zero(p.y) {
                Value::point(p.x + 0.0, p.y + 0.0)
            } else {
                Value::Point(p)
            }
        }
        Value::Size(s) => {
            if negative_zero(s.width) || negative_zero(s.height) {
                Value::size(s.width + 0.0, s.height + 0.0)
            } else {
                Value::Size(s)
            }
        }
        Value::Rect(r) => {
            if [r.x, r.y, r.width, r.height].into_iter().any(negative_zero) {
                Value::rect(r.x + 0.0, r.y + 0.0, r.width + 0.0, r.height + 0.0)
            } else {
                Value::Rect(r)
            }
        }
        Value::Font(f) => {
            if [f.point_size, f.letter_spacing, f.word_spacing]
                .into_iter()
                .any(negative_zero)
            {
                let mut font = (*f).clone();
                font.point_size += 0.0;
                font.letter_spacing += 0.0;
                font.word_spacing += 0.0;
                Value::Font(std::sync::Arc::new(font))
            } else {
                Value::Font(f)
            }
        }
        Value::Vector2d(q) => Value::vector2d(q.x + 0.0, q.y + 0.0),
        Value::Vector3d(q) => Value::vector3d(q.x + 0.0, q.y + 0.0, q.z + 0.0),
        Value::Vector4d(q) => {
            if [q.x, q.y, q.z, q.w]
                .into_iter()
                .any(|x| negative_zero(f64::from(x)))
            {
                Value::vector4d(q.x + 0.0, q.y + 0.0, q.z + 0.0, q.w + 0.0)
            } else {
                Value::Vector4d(q)
            }
        }
        Value::Quaternion(q) => {
            if [q.scalar, q.x, q.y, q.z]
                .into_iter()
                .any(|x| negative_zero(f64::from(x)))
            {
                Value::quaternion(q.scalar + 0.0, q.x + 0.0, q.y + 0.0, q.z + 0.0)
            } else {
                Value::Quaternion(q)
            }
        }
    }
}

/// A value on the other side of a call from one function of a script to
/// another: as [`normal`], but that an array is the caller's own array,
/// holding what it held.
pub(super) fn passed(e: &Engine, v: Value) -> Value {
    match v {
        Value::List(_) => v,
        other => normal(e, other),
    }
}

/// Whether `v` holds a `-0`, itself or as a field. A method called from a
/// script may be a script function of its own (`direct_methods`), which
/// is given the zero with its sign; a register crossing the engine would
/// lose it, so a run with one gives way to JavaScript.
pub(super) fn signed_zero(v: &Value) -> bool {
    match v {
        Value::Real(r) => negative_zero(*r),
        Value::Map(fields) => fields.iter().any(|(_, v)| signed_zero(v)),
        Value::Point(p) => negative_zero(p.x) || negative_zero(p.y),
        Value::Size(s) => negative_zero(s.width) || negative_zero(s.height),
        Value::Rect(r) => [r.x, r.y, r.width, r.height].into_iter().any(negative_zero),
        Value::Font(f) => [f.point_size, f.letter_spacing, f.word_spacing]
            .into_iter()
            .any(negative_zero),
        Value::Vector2d(q) => [q.x, q.y].into_iter().any(|x| negative_zero(f64::from(x))),
        Value::Vector3d(q) => [q.x, q.y, q.z]
            .into_iter()
            .any(|x| negative_zero(f64::from(x))),
        Value::Vector4d(q) => [q.x, q.y, q.z, q.w]
            .into_iter()
            .any(|x| negative_zero(f64::from(x))),
        Value::Quaternion(q) => [q.scalar, q.x, q.y, q.z]
            .into_iter()
            .any(|x| negative_zero(f64::from(x))),
        _ => false,
    }
}

/// Whether there is a JS value in `v`, itself or in a list.
pub(super) fn has_js(v: &Value) -> bool {
    match v {
        Value::Js(_) => true,
        Value::List(items) => items.iter().any(has_js),
        _ => false,
    }
}

/// Whether there is a record in `v`, itself or in a list or record.
fn holds_map(v: &Value) -> bool {
    match v {
        Value::Map(_) => true,
        Value::List(items) => items.iter().any(holds_map),
        _ => false,
    }
}

/// Whether the two tiers gave the same value: the same to the bit, but
/// that every NaN is one NaN. Stricter than `===` (`-0` is not `+0`, a
/// number is not the string it prints as) and deep where `===` is by
/// reference, since each tier builds its own arrays and records.
pub(super) fn same(a: &Value, b: &Value) -> bool {
    fn d(a: f64, b: f64) -> bool {
        a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
    }
    fn s(a: f32, b: f32) -> bool {
        a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
    }
    match (a, b) {
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Real(x), Value::Real(y)) => d(*x, *y),
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Color(x), Value::Color(y)) => x == y,
        (Value::Object(x), Value::Object(y)) => x == y,
        (Value::Point(x), Value::Point(y)) => d(x.x, y.x) && d(x.y, y.y),
        (Value::IntPoint(x), Value::IntPoint(y)) => x == y,
        (Value::Size(x), Value::Size(y)) => d(x.width, y.width) && d(x.height, y.height),
        (Value::Rect(x), Value::Rect(y)) => {
            d(x.x, y.x) && d(x.y, y.y) && d(x.width, y.width) && d(x.height, y.height)
        }
        (Value::Font(x), Value::Font(y)) => {
            x.family == y.family
                && d(x.point_size, y.point_size)
                && x.pixel_size == y.pixel_size
                && x.weight == y.weight
                && x.italic == y.italic
                && x.underline == y.underline
                && x.strikeout == y.strikeout
                && d(x.letter_spacing, y.letter_spacing)
                && d(x.word_spacing, y.word_spacing)
                && x.capitalization == y.capitalization
                && x.hinting_preference == y.hinting_preference
                && x.scale_with_system == y.scale_with_system
                && x.style_name == y.style_name
                && x.overline == y.overline
                && x.kerning == y.kerning
                && x.prefer_shaping == y.prefer_shaping
                && x.context_font_merging == y.context_font_merging
                && x.prefer_typo_line_metrics == y.prefer_typo_line_metrics
                && x.features == y.features
                && x.variable_axes == y.variable_axes
        }
        (Value::Vector2d(x), Value::Vector2d(y)) => s(x.x, y.x) && s(x.y, y.y),
        (Value::Vector3d(x), Value::Vector3d(y)) => s(x.x, y.x) && s(x.y, y.y) && s(x.z, y.z),
        (Value::Vector4d(x), Value::Vector4d(y)) => {
            s(x.x, y.x) && s(x.y, y.y) && s(x.z, y.z) && s(x.w, y.w)
        }
        (Value::Quaternion(x), Value::Quaternion(y)) => {
            s(x.scalar, y.scalar) && s(x.x, y.x) && s(x.y, y.y) && s(x.z, y.z)
        }
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(x, y)| same(x, y))
        }
        // By their fields, whatever order each tier made them in.
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.iter().any(|(l, w)| k == l && same(v, w)))
        }
        _ => false,
    }
}

/// The text of a binding's JS function, for a report.
pub(crate) fn js_text(ir: &CompiledFile, b: &Binding) -> String {
    const MOST: usize = 240;
    let Some(function) = ir.js.functions.get(b.js_fn.index()) else {
        return String::new();
    };
    let head = format!("export function {}(", function.export);
    let Some(start) = ir.js.source.find(&head) else {
        return function.export.clone();
    };
    let rest = &ir.js.source[start + "export ".len()..];
    let end = rest.find("\nexport function ").unwrap_or(rest.len());
    let mut out = String::new();
    for word in rest[..end].split_whitespace() {
        if out.len() + word.len() > MOST {
            out.push_str(" ...");
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

impl Engine {
    /// Turns tier 1 on or off: off, every expression that is not a bare
    /// property path runs as JavaScript. On unless `LJOS_TIER1=0`. For
    /// telling a fault of tier 1 from one of anything else.
    pub fn set_tier1(&self, on: bool) {
        self.inner.tier1.enabled.set(on);
    }

    pub fn tier1(&self) -> bool {
        self.inner.tier1.enabled.get()
    }

    /// Turns the cross-check on or off: on, every evaluation tier 1
    /// completes is run in tier 2 as well, a difference is a warning, and
    /// tier 2's result is used. On in debug builds; `LJOS_TIER1_CHECK`
    /// (`1` or `0`) sets it for every engine of a process.
    pub fn set_tier1_check(&self, on: bool) {
        self.inner.tier1.check.set(on);
    }

    pub fn tier1_check(&self) -> bool {
        self.inner.tier1.check.get()
    }

    /// What tier 1 has done in this engine so far.
    pub fn tier1_stats(&self) -> Tier1Stats {
        self.inner.tier1.stats.get()
    }

    /// Runs `expr` in tier 1. `None`: it has no program, or the program
    /// stopped; tier 2 is to run it. `Some`: the result, as
    /// `Engine::eval_expr` gives it.
    pub(super) fn tier1_eval(
        &self,
        inst: &Instance,
        expr: ExprId,
        hint: &TypeTag,
        args: &[Value],
        kept: bool,
    ) -> Option<Option<Value>> {
        let st = &self.inner.tier1;
        if !st.enabled.get() {
            return None;
        }
        let file = &inst.file;
        let p = file.tier1.by_expr.get(expr.index())?.as_ref()?;
        if p.gave_up() {
            return None;
        }
        let check = st.check.get();
        let key = (std::ptr::from_ref(inst) as usize, expr.0);
        if check {
            let mut skip = st.skip.borrow_mut();
            if skip.last() == Some(&key) {
                // The cross-check, asking for tier 2.
                skip.pop();
                return None;
            }
        }
        let b = file.ir.expr(expr);
        let t = b.tier1.as_ref()?;
        let run = vm::Run {
            e: self,
            inst,
            b,
            t,
            p,
            args,
            called: false,
        };
        let Some(done) = run.run() else {
            p.bailed();
            st.count(|s| s.bails += 1);
            return None;
        };
        if !check {
            if let Some((o, prop, v)) = done.write {
                self.aot_spy_write(o, prop, &v);
                // Most handlers write a number to a plain property, which
                // needs none of the looks the long way makes.
                let plain = self.plain_script_write(o, prop, &v);
                if !matches!(plain, Ok(true))
                    && (plain.is_err() || self.write(o, prop, v, WriteKind::Imperative).is_err())
                {
                    // Refused (read-only, the wrong type): JavaScript
                    // throws for it, with the message tier 2 logs.
                    p.bailed();
                    st.count(|s| s.bails += 1);
                    return None;
                }
                if self.inner.reload.tracking.get() {
                    self.reload_note_assigned(o, prop);
                }
            }
            p.done();
            st.count(|s| s.runs += 1);
            return Some(Some(done.value));
        }

        // The same again in tier 2, which also makes the write.
        let depth = {
            let mut skip = st.skip.borrow_mut();
            skip.push(key);
            skip.len() - 1
        };
        let outer = st.spy.replace(Some(Vec::new()));
        st.spying.set(true);
        let js = self.eval_expr_as(inst, expr, hint, args, false, kept);
        // Tier 1 builds a list where the script builds an array, and holds
        // a record where the script holds the object made of it.
        let js_plain = js.as_ref().map(|v| {
            if holds_map(&done.value) {
                self.deep_copy(v)
            } else {
                self.plain(v)
            }
        });
        // Tier 0 may have answered before the key was taken off.
        st.skip.borrow_mut().truncate(depth);
        let seen = st.spy.replace(outer).unwrap_or_default();
        {
            // A check further out sees these writes too: they were made
            // inside its own tier-2 run.
            let mut spy = st.spy.borrow_mut();
            match spy.as_mut() {
                Some(list) => list.extend(seen.iter().cloned()),
                None => st.spying.set(false),
            }
        }
        // A record tier 1 would write is, written by the script, the
        // object made of it, as with the result above.
        let wrote = |w: &Written| {
            seen.iter().any(|s| {
                s.0 == w.0
                    && s.1 == w.1
                    && if holds_map(&w.2) {
                        same(&self.deep_copy(&s.2), &w.2)
                    } else {
                        same(&s.2, &w.2)
                    }
            })
        };
        let agree = match (&js_plain, &done.write) {
            (Some(v), None) => same(&done.value, v) && seen.is_empty(),
            (Some(v), Some(w)) => same(&done.value, v) && wrote(w),
            // JavaScript threw at the write: unchecked, the write is
            // refused here too and tier 2 takes over.
            (None, Some(w)) => wrote(w),
            // Tier 2 was cut off by the interrupt that ends the dispatch:
            // unchecked, not a failure of its own.
            (None, None) => self.js_interrupted(),
        };
        p.done();
        st.count(|s| {
            s.runs += 1;
            s.checked += 1;
            if !agree {
                s.mismatches += 1;
            }
        });
        if !agree {
            self.tier1_mismatch(inst, b, &done, js.as_ref(), &seen);
        }
        Some(js)
    }

    fn tier1_mismatch(
        &self,
        inst: &Instance,
        b: &Binding,
        tier1: &vm::Done,
        tier2: Option<&Value>,
        seen: &[Written],
    ) {
        let place = |(o, p, v): &Written| {
            let (ty, name) = self.describe(*o, *p);
            format!("{v:?} to {ty}.{name}")
        };
        let mut msg = format!(
            "{}:{}:{}: tier 1 and tier 2 differ on `{}`: tier 1 gave {:?}",
            inst.url(),
            b.span.line,
            b.span.column,
            js_text(&inst.file.ir, b),
            tier1.value
        );
        if let Some(w) = &tier1.write {
            let _ = write!(msg, " and would write {}", place(w));
        }
        match tier2 {
            Some(v) => {
                let _ = write!(msg, "; tier 2 gave {v:?}");
            }
            None => msg.push_str("; tier 2 threw"),
        }
        for w in seen {
            let _ = write!(msg, " and wrote {}", place(w));
        }
        log_line(&format!("mismatch {msg}"));
        self.warn(msg);
    }

    /// A function was installed as the method `m` of `o`: when programs
    /// have it inlined, this is what they check the object against.
    pub(super) fn tier1_note_method(&self, o: ObjectId, m: MethodIndex, noted: Option<Noted>) {
        // Nothing to do for a function nobody inlined, also when it takes
        // the place of one that was: the record of that one names a
        // closure the object no longer has.
        let Some(noted) = noted else {
            return;
        };
        let f = {
            let store = self.inner.store.borrow();
            store
                .objects
                .get(o)
                .and_then(|h| h.ext.as_ref())
                .and_then(|ext| ext.methods.iter().find(|(i, _)| *i == m))
                .map(|(_, f)| Rc::downgrade(f))
        };
        let Some(f) = f else {
            return;
        };
        let st = &self.inner.tier1;
        let mut methods = st.methods.borrow_mut();
        methods.insert(
            (o, m),
            MethodRecord {
                f,
                inst: noted.inst,
                file: noted.file,
                expr: noted.expr,
                weak: noted.weak,
            },
        );
        if methods.len() >= st.sweep_at.get() {
            // The closures of destroyed objects.
            methods.retain(|_, r| r.f.strong_count() > 0);
            st.sweep_at.set((methods.len() * 2).max(64));
        }
    }

    /// Whether the function `o` has by `name` is still `expr` of `inst`'s
    /// file, installed for the instance its parameters are resolved from:
    /// what code with its body inlined runs on, as [`Op::Guard`] asks.
    pub(crate) fn runs_inlined(
        &self,
        inst: &Instance,
        o: ObjectId,
        name: &str,
        expr: ExprId,
    ) -> bool {
        let f = {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(o) else {
                return false;
            };
            let Some(m) = vm::script_method_of(&h.meta, name) else {
                return false;
            };
            let Some(f) = h
                .ext
                .as_ref()
                .and_then(|ext| ext.methods.iter().find(|(i, _)| *i == m))
                .map(|(_, f)| f.clone())
            else {
                return false;
            };
            (m, f)
        };
        let methods = self.inner.tier1.methods.borrow();
        let Some(rec) = methods.get(&(o, f.0)) else {
            return false;
        };
        let file = &inst.file;
        let Some(callee) = inst.find(file.ir.expr(expr).scope.component) else {
            return false;
        };
        std::ptr::addr_eq(Weak::as_ptr(&rec.f), Rc::as_ptr(&f.1))
            && std::ptr::eq(rec.file, Rc::as_ptr(file))
            && rec.expr == expr
            && std::ptr::eq(rec.inst, callee)
    }

    /// A JS function is about to write `v` to a property: the cross-check
    /// compares it with the write tier 1 would have made.
    /// The cross-check of compiled code (aot.rs) spies on the same
    /// writes.
    pub(crate) fn tier1_spying(&self) -> bool {
        self.inner.tier1.spying.get() || self.aot_spying()
    }

    pub(crate) fn tier1_spy(&self, o: ObjectId, p: PropIndex, v: &Value) {
        let st = &self.inner.tier1;
        if st.spying.get()
            && let Some(list) = st.spy.borrow_mut().as_mut()
        {
            list.push((o, p, v.clone()));
        }
        self.aot_spy_write(o, p, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_the_same_by_their_fields_alone() {
        let record = |fields: Vec<(&str, Value)>| {
            Value::map(fields.into_iter().map(|(k, v)| (Atom::intern(k), v)))
        };
        let kind = || ("kind", Value::string("home"));
        let at = || ("at", Value::Real(2.0));
        assert!(same(
            &record(vec![kind(), at()]),
            &record(vec![at(), kind()])
        ));
        assert!(!same(
            &record(vec![kind()]),
            &record(vec![("kind", Value::string("away"))])
        ));
        assert!(!same(&record(vec![kind()]), &record(vec![kind(), at()])));
        assert!(!same(&record(vec![kind(), at()]), &record(vec![kind()])));
        assert!(!same(
            &record(vec![at()]),
            &record(vec![("at", Value::Real(-0.0))])
        ));
        assert!(!same(&record(vec![kind()]), &Value::Null));
    }

    #[test]
    fn a_register_value_is_what_javascript_gives_back() {
        let e = Engine::new();
        assert!(same(&normal(&e, Value::Int(3)), &Value::Real(3.0)));
        assert!(same(
            &normal(&e, Value::Url(Atom::new("a.png"))),
            &Value::string("a.png")
        ));
        let p = normal(&e, Value::point(-0.0, 1.0));
        let Value::Point(p) = p else {
            panic!("not a point");
        };
        assert!(p.x.is_sign_positive());
        let list = normal(&e, Value::list(vec![Value::Int(1), Value::Null]));
        assert!(same(
            &list,
            &Value::list(vec![Value::Real(1.0), Value::Null])
        ));
    }

    #[test]
    fn the_cross_check_tells_zeros_and_types_apart() {
        assert!(same(&Value::Real(f64::NAN), &Value::Real(-f64::NAN)));
        assert!(!same(&Value::Real(0.0), &Value::Real(-0.0)));
        assert!(!same(&Value::Real(1.0), &Value::string("1")));
        assert!(!same(&Value::Undefined, &Value::Null));
        assert!(same(&Value::point(1.0, 2.0), &Value::point(1.0, 2.0)));
    }

    #[test]
    fn qt_keys_are_the_numbers_the_qt_object_has() {
        assert_eq!(qt_key("AlignLeft"), Some(1.0));
        assert_eq!(qt_key("red"), Some(7.0));
        assert_eq!(qt_key("transparent"), Some(19.0));
        assert_eq!(qt_key("platform"), None);
    }
}
