//! Compiled code in the engine (`crate::aot`): which file's code is
//! whose, the [`Frame`] the code runs with, and the cross-check.
//!
//! # Matched by content
//!
//! A loaded file is matched to compiled code once, the first time one of
//! its expressions runs: by [`ljos_ir::aot::content_key`] of its IR, so
//! code is never run for any file but the one it was made from.
//!
//! # Looked up, then run
//!
//! The code names objects by where they are in the file and members by
//! name. Each of those is looked up through the frame before the code does
//! anything, and a member is checked to be of the kind the build took it
//! to be, once per type (an inline cache per site). What is not as the
//! build saw it stops the code with nothing done, and the dynamic tiers run
//! the expression whole.
//!
//! # The cross-check
//!
//! A binding is compared by its value. A handler has effects, which cannot
//! be made twice: it is run dry first, its writes and emits held and
//! listed, and then dynamically with those of the script spied on. The
//! compiled ones have to be among the dynamic ones, in their order (the
//! dynamic list also has what the handlers of an emitted signal did). A
//! dry run that reads after a write it held back is not compared.

use std::cell::{Cell, OnceCell, RefCell};
use std::fmt::Write as _;
use std::sync::OnceLock;

use ljos_ir::{BindingKind, CompIdx, ExprId, ObjIdx, ObjRef};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::WriteKind;
use crate::aot::{File, Kind, Stop};
use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex, SignalIndex};
use crate::incubator::Instance;
use crate::incubator::tier1::{Noted, js_text, normal, same};
use crate::loader::LoadedFile;
use crate::meta::{MetaRef, PropertyFlags};
use crate::types::TypeTag;
use crate::value::Value;

/// What compiled code has done in one engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AotStats {
    /// Evaluations compiled code completed.
    pub runs: u64,
    /// Evaluations it began and gave to the dynamic tiers, having done
    /// nothing.
    pub fallbacks: u64,
    /// Evaluations it stopped after an effect: as a script that throws.
    pub thrown: u64,
    /// Completed evaluations compared with the dynamic tiers.
    pub checked: u64,
    /// Of those, the ones where the two differed.
    pub mismatches: u64,
    /// Dry runs of the cross-check that could not be compared.
    pub inconclusive: u64,
}

struct Env {
    enabled: bool,
    check: Option<bool>,
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
            enabled: flag("LJOS_AOT").unwrap_or(true),
            check: flag("LJOS_AOT_CHECK"),
        }
    })
}

/// A write or an emit, as the cross-check compares them.
#[derive(Clone, Debug)]
pub(crate) enum Effect {
    Write(ObjectId, PropIndex, Value),
    Emit(ObjectId, SignalIndex, Vec<Value>),
}

/// The engine's compiled code and its switches.
pub(crate) struct State {
    enabled: Cell<bool>,
    /// Enabled, with code registered: the one look an expression of an
    /// application without compiled code pays.
    active: Cell<bool>,
    check: Cell<bool>,
    table: RefCell<FxHashMap<[u8; 32], &'static File>>,
    /// The cross-check asking for the dynamic tiers: as tier 1's.
    skip: RefCell<Vec<(usize, u32)>>,
    stats: Cell<AotStats>,
    spy: RefCell<Option<Vec<Effect>>>,
    spying: Cell<bool>,
}

impl Default for State {
    fn default() -> Self {
        let env = env();
        State {
            enabled: Cell::new(env.enabled),
            active: Cell::new(false),
            check: Cell::new(env.check.unwrap_or(cfg!(debug_assertions))),
            table: RefCell::default(),
            skip: RefCell::default(),
            stats: Cell::default(),
            spy: RefCell::default(),
            spying: Cell::new(false),
        }
    }
}

impl State {
    fn update_active(&self) {
        self.active
            .set(self.enabled.get() && !self.table.borrow().is_empty());
    }

    fn count(&self, f: impl FnOnce(&mut AotStats)) {
        let mut s = self.stats.get();
        f(&mut s);
        self.stats.set(s);
    }
}

/// What one site was found to be on the types it was met on.
type Looks<T> = SmallVec<[(MetaRef, Option<T>); 2]>;

/// The types a site is remembered for.
const LOOKED_TYPES: usize = 4;

/// A loaded file's compiled code, with its sites' caches.
pub(crate) struct Linked {
    file: &'static File,
    props: Box<[RefCell<Looks<PropIndex>>]>,
    signals: Box<[RefCell<Looks<SignalIndex>>]>,
}

impl Linked {
    fn new(file: &'static File) -> Linked {
        Linked {
            file,
            props: file.props.iter().map(|_| RefCell::default()).collect(),
            signals: file.signals.iter().map(|_| RefCell::default()).collect(),
        }
    }

    /// Whether code of the file has the function `expr` inlined.
    fn inlines(&self, expr: ExprId) -> bool {
        self.file.methods.iter().any(|m| m.expr == expr.0)
    }
}

/// Where loaded files keep what they were matched to.
pub(crate) type LinkedCell = OnceCell<Option<Linked>>;

fn cached<T: Copy>(
    cell: &RefCell<Looks<T>>,
    meta: &MetaRef,
    find: impl FnOnce() -> Option<T>,
) -> Option<T> {
    if let Some((_, found)) = cell.borrow().iter().find(|(m, _)| MetaRef::ptr_eq(m, meta)) {
        return *found;
    }
    let found = find();
    let mut seen = cell.borrow_mut();
    if seen.len() >= LOOKED_TYPES {
        seen.remove(0);
    }
    seen.push((meta.clone(), found));
    found
}

/// An object of the file, found for the code.
#[derive(Clone, Copy, Debug)]
pub struct Obj(ObjectId);

/// A property of an object, checked to be of the kind the code takes it
/// to be.
#[derive(Clone, Copy, Debug)]
pub struct Prop(PropIndex);

/// A signal of an object, declared in QML.
#[derive(Clone, Copy, Debug)]
pub struct Sig(SignalIndex);

/// What compiled code reads, writes and emits through: one evaluation of
/// one expression.
pub struct Frame<'a> {
    e: &'a Engine,
    inst: &'a Instance,
    linked: &'a Linked,
    args: &'a [Value],
    /// Whether it has written or emitted.
    effects: Cell<bool>,
    /// A dry run of the cross-check: the effects, held.
    held: Option<RefCell<Vec<Effect>>>,
}

impl std::fmt::Debug for Frame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("file", &self.linked.file.name)
            .field("args", &self.args)
            .field("effects", &self.effects.get())
            .field("dry", &self.held.is_some())
            .finish_non_exhaustive()
    }
}

impl Frame<'_> {
    fn stop(&self) -> Stop {
        match (self.effects.get(), self.held.is_some()) {
            (false, _) => Stop::Fallback,
            (true, true) => Stop::Inconclusive,
            (true, false) => Stop::Thrown,
        }
    }

    fn meta(&self, o: ObjectId) -> Result<MetaRef, Stop> {
        let store = self.e.inner.store.borrow();
        match store.objects.get(o) {
            Some(h) => Ok(h.meta.clone()),
            None => Err(self.stop()),
        }
    }

    /// The object at `object` of the component `component` of the file.
    pub fn object(&self, component: u32, object: u32) -> Result<Obj, Stop> {
        let r = ObjRef {
            component: CompIdx(component),
            object: ObjIdx(object),
        };
        match self.inst.resolve(r) {
            Some(o) if self.e.is_alive(o) => Ok(Obj(o)),
            _ => Err(self.stop()),
        }
    }

    /// The property site `site` on `o`.
    pub fn prop(&self, o: Obj, site: usize) -> Result<Prop, Stop> {
        let s = self.linked.file.props.get(site).ok_or(Stop::Fallback)?;
        let meta = self.meta(o.0)?;
        let found = cached(&self.linked.props[site], &meta, || {
            // As a script finds a member: a method of the name hides it.
            if meta.method_index(s.name).is_some() {
                return None;
            }
            let p = meta.property_index(s.name)?;
            let info = meta.property(p)?;
            let kind = match info.ty {
                TypeTag::Real | TypeTag::Int => Kind::Number,
                TypeTag::Bool => Kind::Bool,
                TypeTag::String => Kind::String,
                _ => return None,
            };
            let usable = kind == s.kind
                && !info.flags.contains(PropertyFlags::GROUP)
                && !(s.write && info.flags.contains(PropertyFlags::READONLY));
            usable.then_some(p)
        });
        found.map(Prop).ok_or_else(|| self.stop())
    }

    /// The signal site `site` on `o`.
    pub fn signal(&self, o: Obj, site: usize) -> Result<Sig, Stop> {
        let name = *self.linked.file.signals.get(site).ok_or(Stop::Fallback)?;
        let meta = self.meta(o.0)?;
        let found = cached(&self.linked.signals[site], &meta, || {
            let s = meta.signal_index(name)?;
            meta.signal_is_script(s).then_some(s)
        });
        found.map(Sig).ok_or_else(|| self.stop())
    }

    /// That `o` still has the function the code has inlined.
    pub fn guard(&self, o: Obj, site: usize) -> Result<(), Stop> {
        let m = self.linked.file.methods.get(site).ok_or(Stop::Fallback)?;
        if self.e.runs_inlined(self.inst, o.0, m.name, ExprId(m.expr)) {
            Ok(())
        } else {
            Err(self.stop())
        }
    }

    fn arg(&self, i: usize) -> Result<&Value, Stop> {
        self.args.get(i).ok_or_else(|| self.stop())
    }

    pub fn arg_num(&self, i: usize) -> Result<f64, Stop> {
        number(self.arg(i)?).ok_or_else(|| self.stop())
    }

    pub fn arg_bool(&self, i: usize) -> Result<bool, Stop> {
        match self.arg(i)? {
            Value::Bool(b) => Ok(*b),
            _ => Err(self.stop()),
        }
    }

    pub fn arg_str(&self, i: usize) -> Result<Atom, Stop> {
        match self.arg(i)? {
            Value::String(s) => Ok(s.clone()),
            _ => Err(self.stop()),
        }
    }

    /// A read, tracked as the binding's dependency as any other. In a dry
    /// run, a read after a held write may not see what it would.
    fn read(&self, o: Obj, p: Prop) -> Result<Value, Stop> {
        if self.held.is_some() && self.effects.get() {
            return Err(Stop::Inconclusive);
        }
        self.e.try_read(o.0, p.0).map_err(|_| self.stop())
    }

    pub fn num(&self, o: Obj, p: Prop) -> Result<f64, Stop> {
        let v = self.read(o, p)?;
        number(&v).ok_or_else(|| self.stop())
    }

    pub fn bool(&self, o: Obj, p: Prop) -> Result<bool, Stop> {
        match self.read(o, p)? {
            Value::Bool(b) => Ok(b),
            _ => Err(self.stop()),
        }
    }

    pub fn str(&self, o: Obj, p: Prop) -> Result<Atom, Stop> {
        match self.read(o, p)? {
            Value::String(s) => Ok(s),
            _ => Err(self.stop()),
        }
    }

    /// `o.p = v`, made as a script's write is.
    pub fn set(&self, o: Obj, p: Prop, v: Value) -> Result<(), Stop> {
        if let Some(held) = &self.held {
            held.borrow_mut().push(Effect::Write(o.0, p.0, v));
            self.effects.set(true);
            return Ok(());
        }
        self.e.tier1_spy(o.0, p.0, &v);
        let plain = self.e.plain_script_write(o.0, p.0, &v);
        if !matches!(plain, Ok(true))
            && (plain.is_err() || self.e.write(o.0, p.0, v, WriteKind::Imperative).is_err())
        {
            return Err(self.stop());
        }
        if self.e.inner.reload.tracking.get() {
            self.e.reload_note_assigned(o.0, p.0);
        }
        self.effects.set(true);
        Ok(())
    }

    /// `o.s(args)`: its handlers run now.
    pub fn emit(&self, o: Obj, s: Sig, args: &[Value]) -> Result<(), Stop> {
        if let Some(held) = &self.held {
            held.borrow_mut()
                .push(Effect::Emit(o.0, s.0, args.to_vec()));
            self.effects.set(true);
            return Ok(());
        }
        self.e.aot_spy_emit(o.0, s.0, args);
        let done = self.e.emit(o.0, s.0, args);
        // The handlers may have run, and that is an effect, also when
        // one of them failed.
        let stop = self.stop();
        self.effects.set(true);
        match done {
            Ok(()) => Ok(()),
            Err(EngineError::DeadObject(_)) => Err(stop),
            Err(_) => Err(self.stop()),
        }
    }

    /// `qsTr` of the text `at`, in the file's context.
    pub fn tr(&self, at: usize) -> Atom {
        let file = self.linked.file;
        let source = file.texts.get(at).copied().unwrap_or("");
        Atom::from(self.e.translate(file.context, source, "", None))
    }
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Real(r) => Some(*r),
        Value::Int(i) => Some(f64::from(*i)),
        _ => None,
    }
}

/// Whether the compiled effects are among the dynamic ones, in order.
fn among(e: &Engine, compiled: &[Effect], dynamic: &[Effect]) -> bool {
    let like = |a: &Value, b: &Value| same(&normal(e, a.clone()), &normal(e, b.clone()));
    let matches = |c: &Effect, d: &Effect| match (c, d) {
        (Effect::Write(o, p, v), Effect::Write(q, r, w)) => o == q && p == r && like(v, w),
        (Effect::Emit(o, s, a), Effect::Emit(q, t, b)) => {
            o == q && s == t && a.len() == b.len() && a.iter().zip(b).all(|(x, y)| like(x, y))
        }
        _ => false,
    };
    let mut rest = dynamic.iter();
    compiled.iter().all(|c| rest.any(|d| matches(c, d)))
}

impl Engine {
    /// Gives the engine an application's compiled code (`ljos_aot`): the
    /// table its build generated. Files loaded after this whose content is
    /// one the code was made from run it; a file is matched once, when it
    /// first runs an expression.
    pub fn register_compiled(&self, files: &'static [&'static File]) {
        let st = &self.inner.aot;
        let mut table = st.table.borrow_mut();
        for f in files {
            table.insert(f.content, f);
        }
        drop(table);
        st.update_active();
    }

    /// Turns compiled code on or off. On unless `LJOS_AOT=0`.
    pub fn set_aot(&self, on: bool) {
        self.inner.aot.enabled.set(on);
        self.inner.aot.update_active();
    }

    pub fn aot(&self) -> bool {
        self.inner.aot.enabled.get()
    }

    /// Turns the cross-check of compiled code on or off. On in debug
    /// builds; `LJOS_AOT_CHECK` (`1` or `0`) sets it for every engine of a
    /// process.
    pub fn set_aot_check(&self, on: bool) {
        self.inner.aot.check.set(on);
    }

    pub fn aot_check(&self) -> bool {
        self.inner.aot.check.get()
    }

    pub fn aot_stats(&self) -> AotStats {
        self.inner.aot.stats.get()
    }

    /// Whether the file `url` (as the engine names it) runs compiled code.
    /// `None` when it has not run an expression yet.
    pub fn aot_file(&self, url: &str) -> Option<bool> {
        let loader = self.inner.loader.borrow();
        let file = loader.files.values().find(|f| &*f.url == url)?;
        Some(file.aot.get()?.is_some())
    }

    fn aot_linked<'f>(&self, file: &'f LoadedFile) -> Option<&'f Linked> {
        file.aot
            .get_or_init(|| {
                let table = self.inner.aot.table.borrow();
                if table.is_empty() {
                    return None;
                }
                let key = ljos_ir::aot::content_key(&file.ir);
                let found = table.get(&key)?;
                (found.exprs.len() == file.ir.exprs.len()).then(|| Linked::new(found))
            })
            .as_ref()
    }

    /// A function about to be installed that compiled code has inlined.
    pub(super) fn aot_noted(&self, inst: &std::rc::Rc<Instance>, expr: ExprId) -> Option<Noted> {
        if !self.inner.aot.active.get() {
            return None;
        }
        self.aot_linked(&inst.file)?
            .inlines(expr)
            .then(|| Noted::new(inst, expr))
    }

    #[inline]
    pub(crate) fn aot_spying(&self) -> bool {
        self.inner.aot.spying.get()
    }

    #[inline]
    pub(crate) fn aot_spy_write(&self, o: ObjectId, p: PropIndex, v: &Value) {
        let st = &self.inner.aot;
        if st.spying.get()
            && let Some(list) = st.spy.borrow_mut().as_mut()
        {
            list.push(Effect::Write(o, p, v.clone()));
        }
    }

    #[inline]
    pub(crate) fn aot_spy_emit(&self, o: ObjectId, s: SignalIndex, args: &[Value]) {
        let st = &self.inner.aot;
        if st.spying.get()
            && let Some(list) = st.spy.borrow_mut().as_mut()
        {
            list.push(Effect::Emit(o, s, args.to_vec()));
        }
    }

    /// Runs `expr` as compiled code. `None`: it has none, or the code
    /// stopped with nothing done; the dynamic tiers are to run it.
    pub(super) fn aot_eval(
        &self,
        inst: &Instance,
        expr: ExprId,
        hint: &TypeTag,
        args: &[Value],
        kept: bool,
    ) -> Option<Option<Value>> {
        let st = &self.inner.aot;
        if !st.active.get() {
            return None;
        }
        let linked = self.aot_linked(&inst.file)?;
        let code = (*linked.file.exprs.get(expr.index())?)?;
        let check = st.check.get();
        let key = (std::ptr::from_ref(inst) as usize, expr.0);
        if check {
            let mut skip = st.skip.borrow_mut();
            if skip.last() == Some(&key) {
                skip.pop();
                return None;
            }
        }
        let b = inst.file.ir.expr(expr);
        let handler = b.kind == BindingKind::Handler;
        let mut frame = Frame {
            e: self,
            inst,
            linked,
            args,
            effects: Cell::new(false),
            held: check.then(RefCell::default),
        };
        let done = code(&frame);
        if !check {
            return match done {
                Ok(v) => {
                    st.count(|s| s.runs += 1);
                    Some(Some(if handler { Value::Undefined } else { v }))
                }
                Err(Stop::Fallback | Stop::Inconclusive) => {
                    st.count(|s| s.fallbacks += 1);
                    None
                }
                Err(Stop::Thrown) => {
                    st.count(|s| s.thrown += 1);
                    self.warn(format!(
                        "{}:{}:{}: compiled code stopped after a write or an emit",
                        inst.url(),
                        b.span.line,
                        b.span.column
                    ));
                    Some(None)
                }
            };
        }
        let held = frame
            .held
            .take()
            .map(RefCell::into_inner)
            .unwrap_or_default();
        let value = match done {
            Ok(v) => v,
            Err(Stop::Fallback) => {
                st.count(|s| s.fallbacks += 1);
                return None;
            }
            Err(Stop::Thrown | Stop::Inconclusive) => {
                st.count(|s| s.inconclusive += 1);
                return None;
            }
        };

        // The same in the dynamic tiers, which make the effects.
        let depth = {
            let mut skip = st.skip.borrow_mut();
            skip.push(key);
            skip.len() - 1
        };
        let outer = st.spy.replace(Some(Vec::new()));
        st.spying.set(true);
        let js = self.eval_expr_as(inst, expr, hint, args, false, kept);
        let js_plain = js.as_ref().map(|v| self.plain(v));
        st.skip.borrow_mut().truncate(depth);
        let seen = st.spy.replace(outer).unwrap_or_default();
        {
            let mut spy = st.spy.borrow_mut();
            match spy.as_mut() {
                Some(list) => list.extend(seen.iter().cloned()),
                None => st.spying.set(false),
            }
        }
        let agree = match &js_plain {
            Some(v) => {
                (handler || same(&normal(self, value.clone()), &normal(self, v.clone())))
                    && among(self, &held, &seen)
            }
            // A script that threw at an effect has made those before it.
            None => self.js_interrupted() || (!held.is_empty() && among(self, &held, &seen)),
        };
        st.count(|s| {
            s.runs += 1;
            s.checked += 1;
            if !agree {
                s.mismatches += 1;
            }
        });
        if !agree {
            let mut msg = format!(
                "{}:{}:{}: compiled code and the dynamic tiers differ on `{}`: compiled gave {value:?}",
                inst.url(),
                b.span.line,
                b.span.column,
                js_text(&inst.file.ir, b),
            );
            for h in &held {
                let _ = write!(msg, " and would make {}", self.effect_text(h));
            }
            match &js {
                Some(v) => {
                    let _ = write!(msg, "; dynamic gave {v:?}");
                }
                None => msg.push_str("; dynamic threw"),
            }
            for s in &seen {
                let _ = write!(msg, " and made {}", self.effect_text(s));
            }
            self.warn(msg);
        }
        Some(js)
    }

    fn effect_text(&self, effect: &Effect) -> String {
        match effect {
            Effect::Write(o, p, v) => {
                let (ty, name) = self.describe(*o, *p);
                format!("{v:?} to {ty}.{name}")
            }
            Effect::Emit(o, s, args) => {
                let name = self
                    .meta_of(*o)
                    .and_then(|m| m.signal(*s).map(|i| i.name.to_string()))
                    .unwrap_or_else(|| format!("#{}", s.0));
                format!("{name}{args:?}")
            }
        }
    }
}
