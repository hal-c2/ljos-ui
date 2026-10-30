//! The `Ljos.Models` module: `JsonListModel`, a read-only list model whose
//! rows come from JSON.
//!
//! This is an opt-in module crate written against the public `ljos` facade
//! alone. The `ljos` command has it in `run`, `render` and `test`; any other
//! host registers it like any native module:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_jsonmodel::register(&engine);
//! ```
//!
//! ```qml
//! import Ljos.Models
//! ListView {
//!     model: JsonListModel { source: "rows.json" }
//!     delegate: Text { required property string name; text: name }
//! }
//! ```
//!
//! ## The surface
//!
//! `json` (text) and `source` (a file or URL, relative to the QML file; it
//! wins when both are set), `count`, `roles` (the role names), `status`
//! (`JsonListModel.{Null,Ready,Loading,Error}`), `error` (empty, or why the
//! last read failed), `get(i)` (the row as a plain object, `undefined` beyond
//! the rows), `reload()`, and the row signals a `ListModel` has, with the
//! same arguments. `dataChanged`'s roles are numbers, as a `ListModel`'s are:
//! positions in `roles`.
//!
//! ## The data
//!
//! The document is an array of objects. The roles are the union of the keys
//! of all rows, in first-seen order. A key a row does not have reads as
//! `undefined`; a JSON `null` reads as `null`. A key said twice in an object
//! is the last one said, as `JSON.parse` takes it.
//!
//! Roles only grow while keys are still in use, since a view shaped its
//! delegates by them: a new key is added at the end (`rolesChanged`). When a
//! non-empty document no longer has a key, the model resets, with the roles
//! read afresh. A document of no rows keeps the roles, having nothing to say
//! of them.
//!
//! ## Changes
//!
//! See [`diff`]: only what differs is said, so a delegate of a row that
//! stayed is kept. A row keeps its identity for a view while it stays in the
//! model, moved along by what is inserted or removed before it.
//!
//! ## Failure
//!
//! JSON that cannot be read, a top level that is no array, a row that is no
//! object, a file that is not there: the rows stay as they were, `status` is
//! `Error`, `error` says why (with file, line and column when the text is
//! what is broken) and the same is logged as a warning. The next good read
//! clears it.
//!
//! ## Files that change
//!
//! A local file is followed: a development session (`ljos run --watch` and
//! any host of `ljos_dev`) reads it again when it is saved, by the kernel's
//! file events. A shipped application has no watcher, and `reload()` reads
//! again when it says so. A re-read of a saved file does not pass through
//! `Loading`, so nothing that shows the status flickers. Nothing polls.

mod diff;
mod parse;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use ljos::prelude::*;
use parse::{Fault, Node, Row};

const CATEGORY: &str = "qml";

mod status {
    pub const NULL: i32 = 0;
    pub const READY: i32 = 1;
    pub const LOADING: i32 = 2;
    pub const ERROR: i32 = 3;
}

fn json_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Status",
        vec![
            ("Null", i64::from(status::NULL)),
            ("Ready", i64::from(status::READY)),
            ("Loading", i64::from(status::LOADING)),
            ("Error", i64::from(status::ERROR)),
        ],
    )]
}

/// Registers `JsonListModel` (`Ljos.Models`) on `engine`: the one seam a
/// host calls.
pub fn register(engine: &ljos::Engine) {
    engine.register_module(ljos::module!("Ljos.Models" 1.0 => [JsonListModel]));
}

/// A row and what a view knows it by.
#[derive(Debug)]
struct Kept {
    key: RowKey,
    row: Row,
}

/// The rows a model shows, shared between the QML object and the
/// `QmlListModel` a view reads.
#[derive(Debug, Default)]
struct Shared {
    role_names: Vec<Atom>,
    rows: Vec<Kept>,
    next_key: RowKey,
    signals: Option<ModelSignals>,
}

impl Shared {
    fn kept(&mut self, row: Row) -> Kept {
        let key = self.next_key;
        self.next_key += 1;
        Kept { key, row }
    }
}

struct Rows(Rc<RefCell<Shared>>);

impl QmlListModel for Rows {
    fn row_count(&self) -> usize {
        self.0.borrow().rows.len()
    }

    fn role_names(&self) -> Vec<Atom> {
        self.0.borrow().role_names.clone()
    }

    fn data(&self, row: usize, role: &str) -> Value {
        let shared = self.0.borrow();
        shared
            .rows
            .get(row)
            .and_then(|r| r.row.field(role))
            .map_or(Value::Undefined, value_of)
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.0
            .borrow()
            .rows
            .get(row)
            .map_or(row as RowKey, |r| r.key)
    }
}

/// Rows are converted when read, so a large document costs values only for
/// what a view shows.
fn value_of(node: &Node) -> Value {
    match node {
        Node::Null => Value::Null,
        Node::Bool(b) => Value::Bool(*b),
        Node::Number(n) => number(*n),
        Node::Text(t) => Value::from(t.as_ref()),
        Node::List(items) => Value::list(items.iter().map(value_of).collect()),
        Node::Map(fields) => object(fields),
    }
}

fn number(n: f64) -> Value {
    let whole = n as i32;
    // -0 is a number of its own to a script, and an int cannot hold it.
    if f64::from(whole) == n && !(n == 0.0 && n.is_sign_negative()) {
        Value::Int(whole)
    } else {
        Value::Real(n)
    }
}

fn object(fields: &[(Box<str>, Node)]) -> Value {
    Value::map(
        fields
            .iter()
            .map(|(k, v)| (Atom::from(k.as_ref()), value_of(v))),
    )
}

/// `JsonListModel` (Ljos.Models): a read-only list model of the array of
/// objects in `json` or at `source`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "Ljos.Models",
    name = "JsonListModel",
    extends = QtObject,
    enums = json_enums
)]
pub struct JsonListModel {
    #[qml(base)]
    pub object: QtObject,
    /// The rows as JSON text; `source` wins when it is set too.
    #[qml(notify)]
    pub json: Atom,
    /// The file or URL to read the rows from, relative to the QML file.
    #[qml(notify)]
    pub source: Url,
    /// The number of rows.
    #[qml(notify, readonly)]
    pub count: i32,
    /// The role names: the keys of the rows, in first-seen order.
    #[qml(notify, readonly)]
    pub roles: Value,
    /// [`status`]: Null, Ready, Loading or Error.
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    /// Why the last read failed; empty when it did not.
    #[qml(notify, readonly)]
    pub error: Atom,
    /// Bumped by every read, so an answer to an older one is dropped.
    #[qml(skip)]
    generation: u64,
    /// The file being followed, so that reading it again does not ask again.
    #[qml(skip)]
    following: Option<PathBuf>,
    #[qml(skip)]
    shared: Rc<RefCell<Shared>>,
}

impl Default for JsonListModel {
    fn default() -> Self {
        JsonListModel {
            object: QtObject::default(),
            json: Atom::EMPTY,
            source: Url::default(),
            count: 0,
            roles: Value::list(Vec::new()),
            status: status::NULL,
            error: Atom::EMPTY,
            generation: 0,
            following: None,
            shared: Rc::default(),
        }
    }
}

/// Gives the object its model face, once, before a view declared after it
/// resolves `model:` at its own completion (see `ljos_xml`).
fn attach(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let shared = e.with(me, |m: &JsonListModel| Rc::clone(&m.shared))?;
    if shared.borrow().signals.is_some() {
        return Ok(());
    }
    let signals = e.set_list_model(me, Rc::new(Rows(Rc::clone(&shared))))?;
    shared.borrow_mut().signals = Some(signals);
    Ok(())
}

#[qml_impl]
impl JsonListModel {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        attach(e, me)?;
        e.watch(me, JsonListModel::JSON.index, move |e, _| {
            read(e, me, Show::Loading);
        })?;
        e.watch(me, JsonListModel::SOURCE.index, move |e, _| {
            read(e, me, Show::Loading);
        })?;
        read(e, me, Show::Loading);
        Ok(())
    }

    /// Reads `source` again, or `json` when there is no source.
    #[qml(invokable)]
    fn reload(cx: &Cx) {
        read(cx.engine, cx.this, Show::Loading);
    }

    /// The row at `index` as a plain object; `undefined` beyond the rows.
    #[qml(invokable)]
    fn get(&self, index: Value) -> Value {
        let Some(i) = index
            .as_number()
            .filter(|n| *n >= 0.0 && n.fract() == 0.0)
            .map(|n| n as usize)
        else {
            return Value::Undefined;
        };
        self.shared
            .borrow()
            .rows
            .get(i)
            .map_or(Value::Undefined, |r| object(&r.row.fields))
    }

    #[qml(signal, name = "dataChanged")]
    fn data_changed(top_left: Value, bottom_right: Value, roles: Value);

    #[qml(signal, name = "modelAboutToBeReset")]
    fn model_about_to_be_reset();

    #[qml(signal, name = "modelReset")]
    fn model_reset();

    #[qml(signal, name = "rowsAboutToBeInserted")]
    fn rows_about_to_be_inserted(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "rowsInserted")]
    fn rows_inserted(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "rowsAboutToBeRemoved")]
    fn rows_about_to_be_removed(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "rowsRemoved")]
    fn rows_removed(parent: Value, first: i32, last: i32);
}

/// Whether a read shows as `Loading`: an asked one does; one that follows a
/// save does not, so that a save does not flicker what shows the status.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Show {
    Loading,
    Quietly,
}

fn current(e: &Engine, me: ObjectId, generation: u64) -> bool {
    e.is_alive(me)
        && e.with(me, |m: &JsonListModel| m.generation == generation)
            .unwrap_or(false)
}

/// Starts a read of what the model is set to now.
fn read(e: &Engine, me: ObjectId, show: Show) {
    let Ok(generation) = e.with_mut(me, |m: &mut JsonListModel| {
        m.generation += 1;
        m.generation
    }) else {
        return;
    };
    let source = e.untrack(|| e.read(me, JsonListModel::SOURCE.index).to_js_string());
    if source.is_empty() {
        unfollow(e, me);
        let text = e.untrack(|| e.read(me, JsonListModel::JSON.index).to_js_string());
        if text.is_empty() {
            apply(e, me, Vec::new(), status::NULL);
        } else {
            match parse::rows(text.as_bytes()) {
                Ok(rows) => apply(e, me, rows, status::READY),
                Err(fault) => failed(e, me, describe("json", &fault)),
            }
        }
        return;
    }
    if show == Show::Loading {
        let _ = Cx::new(e, me).set(JsonListModel::STATUS, status::LOADING);
    }
    let url = e.resolve_url(me, &source);
    let platform = Platform::of(e);
    if let Some(bytes) = platform.resource(&url) {
        unfollow(e, me);
        start_worker(e, me, generation, url, Input::Bytes(bytes.to_vec()));
    } else if is_remote(&url) {
        unfollow(e, me);
        let name = url.clone();
        platform.fetch_for(Some(me), &url, move |e, got| match got {
            Ok(bytes) => start_worker(e, me, generation, name, Input::Bytes(bytes)),
            Err(err) => answered(e, me, generation, Err(format!("{name}: {err}"))),
        });
    } else {
        let real = url_path(&url).and_then(|p| platform.env().real(&p).ok());
        if let Some(path) = &real {
            follow(e, me, path);
        }
        start_worker(e, me, generation, url, Input::File(real));
    }
}

/// The file is watched before it is read, so that no save after the read
/// goes unseen.
fn follow(e: &Engine, me: ObjectId, path: &std::path::Path) {
    let same = e
        .with(me, |m: &JsonListModel| m.following.as_deref() == Some(path))
        .unwrap_or(true);
    if same {
        return;
    }
    unfollow(e, me);
    let _ = e.with_mut(me, |m: &mut JsonListModel| {
        m.following = Some(path.to_owned());
    });
    FollowedFiles::of(e).follow(e, me, path, move |e| read(e, me, Show::Quietly));
}

fn unfollow(e: &Engine, me: ObjectId) {
    let was = e
        .with_mut(me, |m: &mut JsonListModel| m.following.take())
        .ok()
        .flatten();
    if was.is_some() {
        FollowedFiles::of(e).unfollow(me);
    }
}

enum Input {
    Bytes(Vec<u8>),
    File(Option<PathBuf>),
}

/// What is said of a document: where, then what.
fn describe(name: &str, fault: &Fault) -> String {
    match fault.at {
        Some((line, column)) => format!("{name}:{line}:{column}: {}", fault.message),
        None => format!("{name}: {}", fault.message),
    }
}

/// Reads and parses on the compute pool, then answers on the Engine thread.
/// A panic there is a fault of the engine and ends in `Error`, not in a
/// `Loading` that never changes.
fn start_worker(e: &Engine, me: ObjectId, generation: u64, name: String, input: Input) {
    let what = format!("read {name}");
    let queued = ljos::task::spawn_for(
        e,
        what,
        Some(me),
        move || {
            let bytes = match input {
                Input::Bytes(bytes) => bytes,
                Input::File(None) => return Err(format!("{name}: cannot be read here")),
                Input::File(Some(path)) => {
                    read_file(&path).map_err(|err| format!("{name}: {err}"))?
                }
            };
            parse::rows(&bytes).map_err(|fault| describe(&name, &fault))
        },
        move |e, answer| {
            let answer = answer.unwrap_or_else(|panic| Err(panic.to_string()));
            answered(e, me, generation, answer);
        },
    );
    if let Err(full) = queued {
        failed(e, me, format!("no worker to read with: {full}"));
    }
}

// One byte over the limit tells a whole document from a cut one, and a
// file that grows without end costs no more than that to refuse.
fn read_file(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(parse::DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn answered(e: &Engine, me: ObjectId, generation: u64, answer: Result<Vec<Row>, String>) {
    if !current(e, me, generation) {
        return;
    }
    match answer {
        Ok(rows) => apply(e, me, rows, status::READY),
        Err(message) => failed(e, me, message),
    }
    e.request_wake();
}

/// The rows stay; everything else says why.
fn failed(e: &Engine, me: ObjectId, message: String) {
    e.log(
        LogLevel::Warning,
        CATEGORY,
        format!("QML JsonListModel: {message}"),
    );
    let cx = Cx::new(e, me);
    e.batch(|| {
        let _ = cx.set(JsonListModel::ERROR, Atom::from(message.as_str()));
        let _ = cx.set(JsonListModel::STATUS, status::ERROR);
    });
}

/// The keys of `rows`, in first-seen order.
fn union_of(rows: &[Row]) -> Vec<Atom> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    for row in rows {
        for (key, _) in &row.fields {
            if seen.insert(key.as_ref()) {
                names.push(Atom::from(key.as_ref()));
            }
        }
    }
    names
}

fn roles_value(names: &[Atom]) -> Value {
    Value::list(names.iter().map(|n| Value::String(n.clone())).collect())
}

/// Makes `rows` the model's rows and says what differs: the roles first, on
/// their own, so that whoever hears of the rows can read them by the roles
/// they have; then the rows, the count and the status, in one batch.
fn apply(e: &Engine, me: ObjectId, rows: Vec<Row>, new_status: i32) {
    let cx = Cx::new(e, me);
    let Ok(shared) = cx.with(|m: &JsonListModel| Rc::clone(&m.shared)) else {
        return;
    };
    let union = union_of(&rows);
    let known = shared.borrow().role_names.clone();
    let gone = !rows.is_empty() && known.iter().any(|n| !union.contains(n));
    let signals = shared.borrow().signals.clone();
    let told = e.any_connection(me);
    let mut rows = Some(rows);
    let names = if gone {
        // A role is gone: what a view shaped its delegates by no longer
        // holds, so the rows go in with the roles and all is said as one.
        if told {
            let _ = JsonListModel::emit_model_about_to_be_reset(&cx);
        }
        let mut s = shared.borrow_mut();
        let fresh = rows.take().unwrap_or_default();
        let kept: Vec<Kept> = fresh.into_iter().map(|row| s.kept(row)).collect();
        s.rows = kept;
        union
    } else {
        let mut names = known.clone();
        names.extend(union.into_iter().filter(|n| !known.contains(n)));
        names
    };
    if names != known {
        let value = roles_value(&names);
        shared.borrow_mut().role_names = names;
        let _ = cx.set(JsonListModel::ROLES, value);
        if !gone && let Some(signals) = &signals {
            signals.roles_changed(e);
        }
    }
    e.batch(|| {
        match rows {
            Some(rows) => change(e, me, &shared, rows),
            None => {
                if let Some(signals) = &signals {
                    signals.model_reset(e);
                }
                if told {
                    let _ = JsonListModel::emit_model_reset(&cx);
                }
            }
        }
        let count = shared.borrow().rows.len() as i32;
        let _ = cx.set(JsonListModel::COUNT, count);
        let _ = cx.set(JsonListModel::ERROR, Atom::EMPTY);
        let _ = cx.set(JsonListModel::STATUS, new_status);
    });
}

/// Says the [`diff::Plan`] between the rows and `rows`. No borrow of the
/// rows is held while anything is told: a listener reads them.
fn change(e: &Engine, me: ObjectId, shared: &Rc<RefCell<Shared>>, rows: Vec<Row>) {
    let plan = {
        let s = shared.borrow();
        let old: Vec<&Row> = s.rows.iter().map(|k| &k.row).collect();
        diff::plan_refs(&old, &rows)
    };
    let cx = Cx::new(e, me);
    let told = e.any_connection(me);
    let signals = shared.borrow().signals.clone();
    let mut incoming = rows.into_iter().skip(plan.first);

    // The rows in place, then what differs in them, adjacent rows that
    // differ in the same roles as one change.
    let mut groups: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    {
        let mut s = shared.borrow_mut();
        let names = s.role_names.clone();
        for row in plan.first..plan.first + plan.changed {
            let Some(new) = incoming.next() else { break };
            let differing: Vec<usize> = names
                .iter()
                .enumerate()
                .filter(|(_, name)| {
                    s.rows[row].row.field(name.as_str()) != new.field(name.as_str())
                })
                .map(|(i, _)| i)
                .collect();
            s.rows[row].row = new;
            // A row between two changes that is the same, or whose keys
            // only came in another order, has nothing to say.
            if differing.is_empty() {
                continue;
            }
            match groups.last_mut() {
                Some((first, count, roles)) if *first + *count == row && *roles == differing => {
                    *count += 1;
                }
                _ => groups.push((row, 1, differing)),
            }
        }
    }
    for (first, count, roles) in groups {
        let names: Vec<Atom> = {
            let s = shared.borrow();
            roles.iter().map(|i| s.role_names[*i].clone()).collect()
        };
        let names: Vec<&str> = names.iter().map(Atom::as_str).collect();
        if let Some(signals) = &signals {
            signals.data_changed(e, first, count, &names);
        }
        if told {
            let _ = JsonListModel::emit_data_changed(
                &cx,
                model_index(e, me, first as i32),
                model_index(e, me, (first + count) as i32 - 1),
                Value::list(roles.iter().map(|i| Value::Int(*i as i32)).collect()),
            );
        }
    }

    let at = plan.first + plan.changed;
    if plan.removed > 0 {
        let last = (at + plan.removed) as i32 - 1;
        if let Some(signals) = &signals {
            signals.rows_about_to_be_removed(e, at, plan.removed);
        }
        if told {
            let _ = JsonListModel::emit_rows_about_to_be_removed(
                &cx,
                invalid_model_index(e),
                at as i32,
                last,
            );
        }
        shared.borrow_mut().rows.drain(at..at + plan.removed);
        if let Some(signals) = &signals {
            signals.rows_removed(e, at, plan.removed);
        }
        if told {
            let _ = JsonListModel::emit_rows_removed(&cx, invalid_model_index(e), at as i32, last);
        }
    }
    if plan.inserted > 0 {
        let last = (at + plan.inserted) as i32 - 1;
        if let Some(signals) = &signals {
            signals.rows_about_to_be_inserted(e, at, plan.inserted);
        }
        if told {
            let _ = JsonListModel::emit_rows_about_to_be_inserted(
                &cx,
                invalid_model_index(e),
                at as i32,
                last,
            );
        }
        {
            let mut s = shared.borrow_mut();
            let fresh: Vec<Kept> = incoming
                .by_ref()
                .take(plan.inserted)
                .map(|row| s.kept(row))
                .collect();
            s.rows.splice(at..at, fresh);
        }
        if let Some(signals) = &signals {
            signals.rows_inserted(e, at, plan.inserted);
        }
        if told {
            let _ = JsonListModel::emit_rows_inserted(&cx, invalid_model_index(e), at as i32, last);
        }
    }
}
