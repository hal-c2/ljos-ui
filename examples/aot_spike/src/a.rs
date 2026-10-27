//! Back end A: the cases scriptc compiled to native code (`scriptc/cases.ts`),
//! called through its library ABI.
//!
//! The compiled code reaches the engine only through the callbacks here,
//! which read and write the real object store, so a read inside a binding
//! is tracked like any other. Three things the ABI imposes shape this file:
//!
//! * a callback cannot return a string, so the strings a case reads are
//!   read here before the call and passed in;
//! * the library may not be entered again from a callback, so emits are
//!   queued and delivered after the export returns (a compiled handler of
//!   the emitted signal would otherwise enter it again);
//! * a string result lives in the library's arena until the next export,
//!   so it is copied out at once.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ljos_engine::{Atom, Engine, MethodIndex, ObjectId, PropIndex, SignalIndex, Value};

use crate::Export;
use crate::rt::{self, Missing};

unsafe extern "C" {
    fn aot_init();
    fn aot_set_panic_sink(f: extern "C" fn(*mut c_void, *const u8, usize, u64), ctx: *mut c_void);
    fn aot_set_callback(name: *const u8, f: *const c_void, ctx: *mut c_void) -> i32;
    fn aot_collect();
    fn aot_progress() -> f64;
    fn aot_on_clicked() -> f64;
    fn aot_on_submitted(p: *const u8, n: usize) -> f64;
    fn aot_status_text(p: *const u8, n: usize, out: *mut *const u8, out_len: *mut usize);
    fn aot_run_text(p: *const u8, n: usize, out: *mut *const u8, out_len: *mut usize);
    fn aot_on_picked(p: *const u8, n: usize, q: *const u8, m: usize) -> f64;
    fn aot_on_key_pressed(key: f64, mods: f64, p: *const u8, n: usize) -> f64;
    fn aot_on_items_changed() -> f64;
    fn aot_read_many(n: f64) -> f64;
    fn aot_write_many(n: f64) -> f64;
    fn aot_emit_many(n: f64) -> f64;
    fn aot_nothing() -> f64;
}

/// The slots of `cases.ts`, in its order: (object, property).
const PROPS: &[(&str, &str)] = &[
    ("root", "total"),
    ("root", "done"),
    ("bar", "width"),
    ("root", "stopMode"),
    ("root", "lastMode"),
    ("root", "changes"),
    ("root", "health"),
    ("root", "selected"),
    ("root", "ran"),
    ("root", "cursor"),
    ("root", "depth"),
    ("root", "accepted"),
    ("root", "items"),
    ("root", "syncs"),
];
const SIGNALS: &[(&str, &str)] = &[("root", "dispatched"), ("editor", "submitted")];
const SEP: char = '\u{1f}';

/// What the callbacks reach: the world's objects and members, resolved once.
struct World {
    props: Vec<(ObjectId, PropIndex)>,
    signals: Vec<(ObjectId, SignalIndex)>,
    cards: ObjectId,
    count: PropIndex,
    get: MethodIndex,
    remove: MethodIndex,
    insert: MethodIndex,
    mv: MethodIndex,
}

/// The engine of the call in progress and what it left to do.
struct Call {
    engine: *const Engine,
    world: *const World,
    emits: Vec<(usize, Atom)>,
    /// The list a `listLen` read, for the `listNum`/`listBool` that follow.
    list: Option<(usize, Vec<Value>)>,
}

thread_local! {
    static CALL: RefCell<Option<Call>> = const { RefCell::new(None) };
    static READY: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f`, a call into the library, with `e` and `w` reachable from the
/// callbacks; then delivers the emits it queued.
fn enter<R>(e: &Engine, w: &World, f: impl FnOnce() -> R) -> R {
    ready();
    CALL.with(|c| {
        let mut c = c.borrow_mut();
        // scriptc would trap; this says which way in it was.
        assert!(
            c.is_none(),
            "back end A entered again from one of its own callbacks"
        );
        *c = Some(Call {
            engine: e,
            world: w,
            emits: Vec::new(),
            list: None,
        });
    });
    let r = f();
    let call = CALL.with(|c| c.borrow_mut().take()).expect("the call");
    for (sig, arg) in call.emits {
        let (o, s) = w.signals[sig];
        rt::emit(e, o, s, &[Value::String(arg)]);
    }
    r
}

fn with<R>(f: impl FnOnce(&Engine, &World, &mut Call) -> R) -> R {
    CALL.with(|c| {
        let mut c = c.borrow_mut();
        let call = c.as_mut().expect("a callback outside a call");
        // SAFETY: `enter` holds both borrows for as long as `CALL` is set.
        let (e, w) = unsafe { (&*call.engine, &*call.world) };
        f(e, w, call)
    })
}

fn str_arg<'a>(p: *const u8, n: usize) -> &'a str {
    // SAFETY: scriptc passes UTF-8 it owns for the length of the callback.
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(p, n)) }
}

extern "C" fn read_num(_: *mut c_void, slot: f64) -> f64 {
    with(|e, w, _| {
        let (o, p) = w.props[slot as usize];
        e.read_number(o, p)
    })
}

extern "C" fn read_bool(_: *mut c_void, slot: f64) -> u8 {
    with(|e, w, _| {
        let (o, p) = w.props[slot as usize];
        u8::from(e.read(o, p).to_boolean())
    })
}

extern "C" fn write_num(_: *mut c_void, slot: f64, v: f64) {
    with(|e, w, _| {
        let (o, p) = w.props[slot as usize];
        rt::write(e, o, p, Value::Real(v));
    });
}

extern "C" fn write_bool(_: *mut c_void, slot: f64, v: u8) {
    with(|e, w, _| {
        let (o, p) = w.props[slot as usize];
        rt::write(e, o, p, Value::Bool(v != 0));
    });
}

extern "C" fn write_str(_: *mut c_void, slot: f64, s: *const u8, n: usize) {
    let s = Atom::from(str_arg(s, n));
    with(|e, w, _| {
        let (o, p) = w.props[slot as usize];
        rt::write(e, o, p, Value::String(s));
    });
}

extern "C" fn write_str_list(_: *mut c_void, slot: f64, s: *const u8, n: usize) {
    let s = str_arg(s, n);
    let items: Vec<Value> = if s.is_empty() {
        Vec::new()
    } else {
        s.split(SEP).map(|x| Value::String(x.into())).collect()
    };
    with(|e, w, _| {
        let (o, p) = w.props[slot as usize];
        rt::write(e, o, p, Value::List(Arc::new(items)));
    });
}

extern "C" fn emit_str(_: *mut c_void, sig: f64, s: *const u8, n: usize) {
    let s = Atom::from(str_arg(s, n));
    with(|_, _, call| call.emits.push((sig as usize, s)));
}

extern "C" fn list_len(_: *mut c_void, slot: f64) -> f64 {
    with(|e, w, call| {
        let (o, p) = w.props[slot as usize];
        let items = match e.plain(&e.read(o, p)) {
            Value::List(l) => l.as_ref().clone(),
            _ => Vec::new(),
        };
        let n = items.len();
        call.list = Some((slot as usize, items));
        n as f64
    })
}

fn list_at(call: &Call, slot: f64, i: f64) -> Option<&Value> {
    let (s, items) = call.list.as_ref()?;
    (*s == slot as usize).then_some(())?;
    items.get(i as usize)
}

extern "C" fn list_num(_: *mut c_void, slot: f64, i: f64) -> f64 {
    with(|_, _, call| list_at(call, slot, i).map_or(f64::NAN, Value::to_number))
}

extern "C" fn list_bool(_: *mut c_void, slot: f64, i: f64) -> u8 {
    with(|_, _, call| u8::from(list_at(call, slot, i).is_some_and(Value::to_boolean)))
}

extern "C" fn model_count(_: *mut c_void) -> f64 {
    with(|e, w, _| e.read_number(w.cards, w.count))
}

extern "C" fn model_row_num(_: *mut c_void, i: f64) -> f64 {
    with(|e, w, _| {
        let row = rt::invoke(e, w.cards, w.get, &[Value::Real(i)]);
        rt::row_num(e, &row, "toastId")
    })
}

extern "C" fn model_remove(_: *mut c_void, i: f64) {
    with(|e, w, _| {
        rt::invoke(e, w.cards, w.remove, &[Value::Real(i)]);
    });
}

extern "C" fn model_insert_num(_: *mut c_void, i: f64, id: f64) {
    with(|e, w, _| {
        let row = Value::Map(Arc::new(vec![(Atom::from("toastId"), Value::Real(id))]));
        rt::invoke(e, w.cards, w.insert, &[Value::Real(i), row]);
    });
}

extern "C" fn model_move(_: *mut c_void, from: f64, to: f64, n: f64) {
    with(|e, w, _| {
        rt::invoke(
            e,
            w.cards,
            w.mv,
            &[Value::Real(from), Value::Real(to), Value::Real(n)],
        );
    });
}

extern "C" fn panic_sink(_: *mut c_void, p: *const u8, n: usize, code: u64) {
    // SAFETY: the diagnostic bytes scriptc passes, valid for this call.
    let msg = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, n) });
    eprintln!("aot: back end A trapped ({code}): {msg}");
    std::process::abort();
}

/// Initialises the library on this thread and registers the callbacks,
/// once. The callbacks find the engine through thread-local state.
fn ready() {
    if READY.get() {
        return;
    }
    macro_rules! reg {
        ($name:literal, $f:expr) => {{
            // SAFETY: the C ABI casts every callback to `void (*)(void)`;
            // the library calls it with the profile's signature.
            let r = unsafe {
                aot_set_callback(
                    concat!($name, "\0").as_ptr(),
                    $f as *const c_void,
                    std::ptr::null_mut(),
                )
            };
            assert_eq!(r, 0, "callback {}", $name);
        }};
    }
    // SAFETY: the library's own entry points, on the thread that uses it.
    unsafe {
        aot_set_panic_sink(panic_sink, std::ptr::null_mut());
        aot_init();
    }
    reg!(
        "readNum",
        read_num as extern "C" fn(*mut c_void, f64) -> f64
    );
    reg!(
        "readBool",
        read_bool as extern "C" fn(*mut c_void, f64) -> u8
    );
    reg!(
        "writeNum",
        write_num as extern "C" fn(*mut c_void, f64, f64)
    );
    reg!(
        "writeBool",
        write_bool as extern "C" fn(*mut c_void, f64, u8)
    );
    reg!(
        "writeStr",
        write_str as extern "C" fn(*mut c_void, f64, *const u8, usize)
    );
    reg!(
        "writeStrList",
        write_str_list as extern "C" fn(*mut c_void, f64, *const u8, usize)
    );
    reg!(
        "emitStr",
        emit_str as extern "C" fn(*mut c_void, f64, *const u8, usize)
    );
    reg!(
        "listLen",
        list_len as extern "C" fn(*mut c_void, f64) -> f64
    );
    reg!(
        "listNum",
        list_num as extern "C" fn(*mut c_void, f64, f64) -> f64
    );
    reg!(
        "listBool",
        list_bool as extern "C" fn(*mut c_void, f64, f64) -> u8
    );
    reg!(
        "modelCount",
        model_count as extern "C" fn(*mut c_void) -> f64
    );
    reg!(
        "modelRowNum",
        model_row_num as extern "C" fn(*mut c_void, f64) -> f64
    );
    reg!(
        "modelRemove",
        model_remove as extern "C" fn(*mut c_void, f64)
    );
    reg!(
        "modelInsertNum",
        model_insert_num as extern "C" fn(*mut c_void, f64, f64)
    );
    reg!(
        "modelMove",
        model_move as extern "C" fn(*mut c_void, f64, f64, f64)
    );
    READY.set(true);
}

fn string_result(f: impl FnOnce(*mut *const u8, *mut usize)) -> Atom {
    let mut p: *const u8 = std::ptr::null();
    let mut n = 0usize;
    f(&mut p, &mut n);
    if p.is_null() {
        return Atom::from("");
    }
    // SAFETY: the result arena holds it until the next call into the library.
    Atom::from(str_arg(p, n))
}

/// The world as back end A reaches it; shared by its exports.
pub struct Linked {
    world: std::rc::Rc<World>,
    status: PropIndex,
    run_status: PropIndex,
    selected: PropIndex,
    field: ObjectId,
    text: PropIndex,
    pub bar: ObjectId,
}

pub fn link(e: &Engine, root: ObjectId) -> Result<Linked, Missing> {
    let obj = |n: &str| rt::find(e, root, n);
    let mut props = Vec::new();
    for (o, p) in PROPS {
        let o = obj(o)?;
        props.push((o, e.property_index(o, p)?));
    }
    let mut signals = Vec::new();
    for (o, s) in SIGNALS {
        let o = obj(o)?;
        signals.push((o, e.signal_index(o, s)?));
    }
    let cards = obj("cards")?;
    let field = obj("field")?;
    let world = World {
        props,
        signals,
        cards,
        count: e.property_index(cards, "count")?,
        get: e.method_index(cards, "get")?,
        remove: e.method_index(cards, "remove")?,
        insert: e.method_index(cards, "insert")?,
        mv: e.method_index(cards, "move")?,
    };
    Ok(Linked {
        world: std::rc::Rc::new(world),
        status: e.property_index(root, "status")?,
        run_status: e.property_index(root, "runStatus")?,
        selected: e.property_index(root, "selected")?,
        field,
        text: e.property_index(field, "text")?,
        bar: obj("bar")?,
    })
}

/// Every case as the binding or handler the dynamic world has, by the
/// export names back end B uses.
pub fn exports(e: &Engine, root: ObjectId) -> Result<Vec<(&'static str, Export)>, Missing> {
    let l = link(e, root)?;
    let w = || l.world.clone();
    let root_idx = |n: &str| e.property_index(root, n);
    let sig = |o: ObjectId, n: &str| e.signal_index(o, n);
    let editor = rt::find(e, root, "editor")?;
    let mut out: Vec<(&'static str, Export)> = Vec::new();

    let ww = w();
    out.push((
        "root__progress",
        Export::Binding(
            root,
            root_idx("progress")?,
            Box::new(move |e| Value::Real(enter(e, &ww, || unsafe { aot_progress() }))),
        ),
    ));
    let ww = w();
    out.push((
        "root__onClicked",
        Export::Handler(
            root,
            sig(root, "clicked")?,
            std::rc::Rc::new(move |e, _| {
                enter(e, &ww, || unsafe { aot_on_clicked() });
            }),
        ),
    ));
    let ww = w();
    out.push((
        "editor__onSubmitted",
        Export::Handler(
            editor,
            sig(editor, "submitted")?,
            std::rc::Rc::new(move |e, a| {
                let mode = rt::sig_str(a, 0);
                enter(e, &ww, || unsafe {
                    aot_on_submitted(mode.as_str().as_ptr(), mode.as_str().len())
                });
            }),
        ),
    ));
    let (ww, status) = (w(), l.status);
    out.push((
        "root__statusText",
        Export::Binding(
            root,
            root_idx("statusText")?,
            Box::new(move |e| {
                let s = e.read(root, status).to_js_string();
                Value::String(enter(e, &ww, || {
                    string_result(|p, n| unsafe {
                        aot_status_text(s.as_str().as_ptr(), s.as_str().len(), p, n)
                    })
                }))
            }),
        ),
    ));
    let (ww, run_status) = (w(), l.run_status);
    out.push((
        "root__runText",
        Export::Binding(
            root,
            root_idx("runText")?,
            Box::new(move |e| {
                let s = e.read(root, run_status).to_js_string();
                Value::String(enter(e, &ww, || {
                    string_result(|p, n| unsafe {
                        aot_run_text(s.as_str().as_ptr(), s.as_str().len(), p, n)
                    })
                }))
            }),
        ),
    ));
    let (ww, selected) = (w(), l.selected);
    out.push((
        "root__onPicked",
        Export::Handler(
            root,
            sig(root, "picked")?,
            std::rc::Rc::new(move |e, a| {
                let value = rt::sig_str(a, 0);
                let list = rt::list_str(e, &e.read(root, selected));
                let joined = list
                    .iter()
                    .map(Atom::as_str)
                    .collect::<Vec<_>>()
                    .join(&SEP.to_string());
                enter(e, &ww, || unsafe {
                    aot_on_picked(
                        joined.as_ptr(),
                        joined.len(),
                        value.as_str().as_ptr(),
                        value.as_str().len(),
                    )
                });
            }),
        ),
    ));
    let (ww, field, text) = (w(), l.field, l.text);
    out.push((
        "root__onKeyPressed",
        Export::Handler(
            root,
            sig(root, "keyPressed")?,
            std::rc::Rc::new(move |e, a| {
                let (key, mods) = (rt::sig_num(a, 0), rt::sig_num(a, 1));
                let t = e.read(field, text).to_js_string();
                enter(e, &ww, || unsafe {
                    aot_on_key_pressed(key, mods, t.as_str().as_ptr(), t.as_str().len())
                });
            }),
        ),
    ));
    let ww = w();
    out.push((
        "root__onItemsChanged",
        Export::Handler(
            root,
            sig(root, "itemsChanged")?,
            std::rc::Rc::new(move |e, _| {
                enter(e, &ww, || unsafe { aot_on_items_changed() });
            }),
        ),
    ));
    Ok(out)
}

/// The boundary probes: `n` tracked reads of `done`, writes of `cursor`,
/// or emits of `dispatched`, in one call; and a call that does nothing.
pub enum Probe {
    Read,
    Write,
    Emit,
    Nothing,
}

pub fn probe(e: &Engine, l: &Linked, p: &Probe, n: f64) -> f64 {
    enter(e, &l.world, || unsafe {
        match p {
            Probe::Read => aot_read_many(n),
            Probe::Write => aot_write_many(n),
            Probe::Emit => aot_emit_many(n),
            Probe::Nothing => aot_nothing(),
        }
    })
}

/// A full cycle collection of the library's heap, timed.
pub fn collect() -> Duration {
    ready();
    let t = Instant::now();
    // SAFETY: not inside a call, which `enter` would have set.
    unsafe { aot_collect() };
    t.elapsed()
}
