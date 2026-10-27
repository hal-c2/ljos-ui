//! The `QtQml.XmlListModel` module for Ljós: `XmlListModel` and
//! `XmlListModelRole`, a read-only list model over an XML document.
//!
//! This is an opt-in module crate written against the public `ljos`
//! facade alone. The `ljos` command has it in `run`, `render` and
//! `test`; any other host registers it like any native module:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_xml::register(&engine);
//! ```
//!
//! QML then says `import QtQml.XmlListModel` and uses the model the
//! Qt 6.11 documentation describes:
//!
//! ```qml
//! XmlListModel {
//!     source: "https://www.qt.io/blog/rss.xml"
//!     query: "/rss/channel/item"
//!     XmlListModelRole { name: "title"; elementName: "title" }
//!     XmlListModelRole { name: "link"; elementName: "link" }
//! }
//! ```
//!
//! ## The surface, and what is not in it
//!
//! Implemented, matching Qt 6.11.2 as probed on `qml6` (the scenarios
//! in `features/models/xml-list-model.feature`): `source`, `query`,
//! `roles` (the default property), `count`, `progress`, `status` with
//! `XmlListModel.{Null,Ready,Loading,Error}` (0–3), `reload()` and
//! `errorString()`; on the role, `name`, `elementName` and
//! `attributeName`. Qt 6.11 has no `xml` property and no `get()`
//! method on this type (both are Qt 5 spellings), so neither does
//! this crate.
//!
//! ## Queries
//!
//! A query is a slash-separated path of element names starting with
//! `/` — `/rss/channel/item` makes a row of each `item` at exactly
//! that path. That is the whole grammar, as in Qt: no XPath, no
//! predicates, no `//` (an empty segment simply matches nothing).
//! A query that does not start with `/` is refused with Qt's warning
//! and the model stays `Loading`; an empty query never loads. A
//! role's `elementName` is the same kind of path below the row's
//! element (`info/pages`), optionally with one `attributeName`;
//! empty `elementName` with an attribute reads the row element's own
//! attribute. Names match by local name, so `content` reads a
//! `<media:content>` element. A role's value is all the text inside
//! its element (descendants included, entities and CDATA resolved,
//! whitespace kept), the last of repeated elements wins, and
//! anything missing reads as `""`.
//!
//! ## Loading
//!
//! Every load is asynchronous, local files included: `status` says
//! `Loading` while the document is read on the compute pool (a
//! local file directly, a remote one through the platform's
//! fetcher), then the rows replace the old ones and `status` says
//! `Ready`. Setting `source` or `query`, or calling `reload()`,
//! loads again; clearing `source` empties the model back to `Null`.
//! A local file that cannot be opened reads as an empty document
//! (Ready, no rows — what Qt does); a fetch that fails is `Error`
//! with `errorString()` saying why. A document that breaks part-way
//! keeps the rows already read, logs a query error and is `Ready`.
//! DECISION: documents are limited to 16 MiB for files, fetches and
//! registered resources. Parsing runs on the loading worker, subject to
//! the depth, row, cell, value, copied-text and event limits in `parse`.
//! A resource limit sets Error with errorString(), keeping the old rows.
//! Nothing polls: work happens only when a load is asked for and
//! when its answer arrives.

mod parse;

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::*;
use parse::{Parsed, RoleSpec};

const CATEGORY: &str = "qml";

mod status {
    pub const NULL: i32 = 0;
    pub const READY: i32 = 1;
    pub const LOADING: i32 = 2;
    pub const ERROR: i32 = 3;
}

fn xml_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
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

/// Registers `XmlListModel` and `XmlListModelRole`
/// (`QtQml.XmlListModel`) on `engine`: the one seam a host calls.
pub fn register(engine: &ljos::Engine) {
    engine.register_module(
        ljos::module!("QtQml.XmlListModel" 1.0 => [XmlListModel, XmlListModelRole]),
    );
}

/// `XmlListModelRole` (QtQml.XmlListModel): one value to read out of
/// each row's element.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQml.XmlListModel",
    name = "XmlListModelRole",
    extends = QtObject
)]
pub struct XmlListModelRole {
    #[qml(base)]
    pub object: QtObject,
    /// The role name a delegate sees.
    #[qml(notify)]
    pub name: Atom,
    /// The element below the row's element to read, or a path to it
    /// joined with `/`; empty is the row's element itself.
    #[qml(notify)]
    pub element_name: Atom,
    /// The attribute of that element to read instead of its text.
    #[qml(notify)]
    pub attribute_name: Atom,
}

/// The rows a model currently shows, and how it reports changes.
/// Shared between the QML object and the `QmlListModel` a view reads,
/// so a finished load replaces the rows under the same signals.
#[derive(Debug, Default)]
struct Shared {
    role_names: Vec<Atom>,
    rows: Vec<Vec<String>>,
    signals: Option<ModelSignals>,
    error: String,
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
        let Some(col) = shared.role_names.iter().position(|n| n.as_str() == role) else {
            return Value::Undefined;
        };
        match shared.rows.get(row).and_then(|r| r.get(col)) {
            Some(text) => Value::from(text.as_str()),
            None => Value::Undefined,
        }
    }
}

/// `XmlListModel` (QtQml.XmlListModel): a read-only model of an XML
/// document at `source`, one row per element the `query` picks.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQml.XmlListModel",
    name = "XmlListModel",
    extends = QtObject,
    enums = xml_enums,
    default = "roles"
)]
pub struct XmlListModel {
    #[qml(base)]
    pub object: QtObject,
    /// The location of the XML data source.
    #[qml(notify)]
    pub source: Url,
    /// The base path for creating model items, starting with `/`.
    #[qml(notify)]
    pub query: Atom,
    /// The roles to make available for this model.
    #[qml(list, readonly, append = roles_append)]
    pub roles: QmlList,
    /// The number of data entries in the model.
    #[qml(notify, readonly)]
    pub count: i32,
    /// How much of the document has been read, 0.0 to 1.0. A local
    /// document is read at once, so this is 1.0 while it loads.
    #[qml(notify, readonly)]
    pub progress: f64,
    /// [`status`]: Null, Ready, Loading or Error.
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    /// Bumped by every load, so an answer to an older one is dropped.
    #[qml(skip)]
    generation: u64,
    #[qml(skip)]
    shared: Rc<RefCell<Shared>>,
}

impl Default for XmlListModel {
    fn default() -> Self {
        XmlListModel {
            object: QtObject::default(),
            source: Url::default(),
            query: Atom::EMPTY,
            roles: QmlList::default(),
            count: 0,
            progress: 0.0,
            status: status::NULL,
            generation: 0,
            shared: Rc::default(),
        }
    }
}

/// Gives the object its model face, once. This must happen before a
/// view first resolves `model: <this object>`: a view resolves it at
/// its own completion, completion hooks run last-made-first, and a
/// view declared after the model would otherwise take the object
/// itself for its one row and never look again. The creator appends
/// each role as it is made — before any sibling view exists — so the
/// append hook below attaches here; completion attaches again for a
/// model declared without roles.
fn attach(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let shared = e.with(me, |m: &XmlListModel| Rc::clone(&m.shared))?;
    if shared.borrow().signals.is_some() {
        return Ok(());
    }
    let signals = e.set_list_model(me, Rc::new(Rows(Rc::clone(&shared))))?;
    shared.borrow_mut().signals = Some(signals);
    Ok(())
}

fn roles_append(cx: &Cx, item: ObjectId) -> Result<(), EngineError> {
    attach(cx.engine, cx.this)?;
    cx.engine
        .list_push(cx.this, XmlListModel::ROLES.index, item)
}

#[qml_impl]
impl XmlListModel {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        attach(e, me)?;
        e.watch(me, XmlListModel::SOURCE.index, move |e, _| load(e, me))?;
        e.watch(me, XmlListModel::QUERY.index, move |e, _| load(e, me))?;
        load(e, me);
        Ok(())
    }

    /// Reloads the model: reads the document again.
    #[qml(invokable)]
    fn reload(cx: &Cx) {
        load(cx.engine, cx.this);
    }

    /// A description of the last error, when `status` is `Error`.
    #[qml(invokable, name = "errorString")]
    fn error_string(&self) -> Atom {
        Atom::from(self.shared.borrow().error.as_str())
    }
}

fn set_status(e: &Engine, me: ObjectId, status: i32) {
    let cx = Cx::new(e, me);
    let _ = cx.set(XmlListModel::STATUS, status);
}

fn current(e: &Engine, me: ObjectId, generation: u64) -> bool {
    e.is_alive(me)
        && e.with(me, |m: &XmlListModel| m.generation == generation)
            .unwrap_or(false)
}

/// What one load asks its thread for and what the answer carries.
enum Outcome {
    /// The bounded rows, parsed on the worker.
    Parsed(Parsed),
    /// A local file Qt would call an empty document: not openable.
    Unopenable(String),
    /// A fetch that failed, with what `errorString()` should say.
    Failed(String),
}

/// Worker input owns everything needed off the Engine thread.
enum Input {
    Bytes(Vec<u8>),
    Local {
        real: Option<std::path::PathBuf>,
        url: String,
    },
}

/// Starts (or restarts) reading the model's document. The status
/// says `Loading` now; the answer lands in [`loaded`].
fn load(e: &Engine, me: ObjectId) {
    let Ok(generation) = e.with_mut(me, |m: &mut XmlListModel| {
        m.generation += 1;
        m.generation
    }) else {
        return;
    };
    let source = e.untrack(|| e.read(me, XmlListModel::SOURCE.index).to_js_string());
    let query = e.untrack(|| e.read(me, XmlListModel::QUERY.index).to_js_string());
    let roles = role_specs(e, me);
    // The role names are known before any row is: a view shapes its
    // delegates by them, so publish them at load start, not on arrival.
    publish_roles(e, me, &roles);
    let cx = Cx::new(e, me);
    let platform = Platform::of(e);

    if source.is_empty() {
        // Nothing to read: Loading now, Null and empty when the
        // answer lands, as every other load answers off this call.
        set_status(e, me, status::LOADING);
        platform
            .expect_for("XmlListModel source", Some(me))
            .send(move |e| {
                if current(e, me, generation) {
                    apply(e, me, &roles, Parsed::default(), status::NULL, 1.0);
                }
            });
        return;
    }
    if query.is_empty() {
        set_status(e, me, status::LOADING);
        return;
    }
    let Some(path) = parse::query_path(&query) else {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            "QML XmlListModel: An XmlListModel query must start with '/'".to_owned(),
        );
        set_status(e, me, status::LOADING);
        return;
    };

    let url = e.resolve_url(me, &source);
    let remote = is_remote(&url);
    e.batch(|| {
        let _ = cx.set(XmlListModel::PROGRESS, if remote { 0.0 } else { 1.0 });
        set_status(e, me, status::LOADING);
    });

    let resource = platform.resource(&url);
    if resource
        .as_ref()
        .is_some_and(|b| b.len() > parse::DOCUMENT_BYTES)
    {
        platform
            .expect_for("XmlListModel source", Some(me))
            .send(move |e| {
                loaded(
                    e,
                    me,
                    generation,
                    &roles,
                    Outcome::Failed("XML document byte limit exceeded".into()),
                );
            });
        return;
    }
    if let Some(bytes) = resource {
        start_worker(e, me, generation, path, roles, Input::Bytes(bytes.to_vec()));
        return;
    }
    if remote {
        platform.fetch_for(Some(me), &url, move |e, got| match got {
            Ok(bytes) => start_worker(e, me, generation, path, roles, Input::Bytes(bytes)),
            Err(err) => loaded(e, me, generation, &roles, Outcome::Failed(err.to_string())),
        });
        return;
    }
    // A local file, read and parsed off the Engine thread like a fetch,
    // so a load never blocks it and `Loading` is what a reader sees first.
    let real = url_path(&url).and_then(|p| platform.env().real(&p).ok());
    start_worker(e, me, generation, path, roles, Input::Local { real, url });
}

/// Reads and parses one source on the compute pool, then answers on
/// Engine. A load that panics there is a fault of the engine and ends
/// in `Error`, not in a `Loading` that never changes.
fn start_worker(
    e: &Engine,
    me: ObjectId,
    generation: u64,
    path: Vec<String>,
    roles: Vec<RoleSpec>,
    input: Input,
) {
    let what = match &input {
        Input::Bytes(_) => "XmlListModel source".to_owned(),
        Input::Local { url, .. } => format!("read {url}"),
    };
    let queued = ljos::task::spawn_for(
        e,
        what,
        Some(me),
        move || {
            let bytes = match input {
                Input::Bytes(bytes) => Ok(bytes),
                Input::Local { real, url } => {
                    if let Some(p) = real {
                        match std::fs::File::open(&p) {
                            Ok(file) => read_document(file).map_err(Outcome::Failed),
                            Err(err) => Err(Outcome::Unopenable(format!(
                                "Failed to open file {}: {err}",
                                p.display()
                            ))),
                        }
                    } else {
                        Err(Outcome::Unopenable(format!("Failed to open file {url}")))
                    }
                }
            };
            // A test's way to make the work panic, as a bug in it would.
            #[cfg(test)]
            if bytes.as_deref().is_ok_and(|b| b == tests::PANICS) {
                panic!("a load that panics");
            }
            let outcome = match bytes {
                Ok(bytes) => match parse::parse(&bytes, &path, &roles) {
                    Ok(parsed) => Outcome::Parsed(parsed),
                    Err(err) => Outcome::Failed(err),
                },
                Err(outcome) => outcome,
            };
            // The roles ride back with the answer rather than being
            // copied for it.
            (roles, outcome)
        },
        move |e, answer| match answer {
            Ok((roles, outcome)) => loaded(e, me, generation, &roles, outcome),
            // An error answer reads no roles.
            Err(panic) => loaded(e, me, generation, &[], Outcome::Failed(panic.to_string())),
        },
    );
    if let Err(full) = queued {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("QML XmlListModel: no worker to read with: {full}"),
        );
        let cx = Cx::new(e, me);
        let _ = cx.with_mut(|m: &mut XmlListModel| {
            m.shared.borrow_mut().error = format!("no worker to read with: {full}");
        });
        set_status(e, me, status::ERROR);
    }
}

/// Makes `roles` the names a delegate is shaped by, saying so when
/// they changed (the first load grows them from none).
fn publish_roles(e: &Engine, me: ObjectId, roles: &[RoleSpec]) {
    let cx = Cx::new(e, me);
    let Ok(shared) = cx.with(|m: &XmlListModel| Rc::clone(&m.shared)) else {
        return;
    };
    let names: Vec<Atom> = roles.iter().map(|r| Atom::from(r.name.as_str())).collect();
    let signals = {
        let mut s = shared.borrow_mut();
        if s.role_names == names {
            return;
        }
        s.role_names = names;
        s.signals.clone()
    };
    if let Some(signals) = signals {
        signals.roles_changed(e);
    }
}

/// The roles as the load starting now reads them: name, element path
/// and attribute of each `XmlListModelRole` in `roles`.
fn role_specs(e: &Engine, me: ObjectId) -> Vec<RoleSpec> {
    let objects = e
        .untrack(|| e.with(me, |m: &XmlListModel| m.roles.objects()))
        .unwrap_or_default();
    objects
        .into_iter()
        .filter_map(|o| {
            e.untrack(|| {
                e.with(o, |r: &XmlListModelRole| RoleSpec {
                    name: r.name.to_string(),
                    element: parse::element_path(&r.element_name),
                    attribute: r.attribute_name.to_string(),
                })
            })
            .ok()
        })
        .collect()
}

/// An answer has arrived: rows, an empty document, or an error.
fn loaded(e: &Engine, me: ObjectId, generation: u64, roles: &[RoleSpec], outcome: Outcome) {
    if !current(e, me, generation) {
        return;
    }
    match outcome {
        Outcome::Parsed(parsed) => {
            apply(e, me, roles, parsed, status::READY, 1.0);
        }
        Outcome::Unopenable(message) => {
            e.log(LogLevel::Warning, CATEGORY, message);
            apply(e, me, roles, Parsed::default(), status::READY, 1.0);
        }
        Outcome::Failed(message) => {
            let cx = Cx::new(e, me);
            let _ = cx.with_mut(|m: &mut XmlListModel| {
                m.shared.borrow_mut().error = message;
            });
            set_status(e, me, status::ERROR);
            e.request_wake();
        }
    }
}

/// Puts rows in the model and says so: warnings first, then the
/// rows (a delegate made for them is there before `Ready` is), then
/// count, progress and status, in one batch.
fn apply(e: &Engine, me: ObjectId, roles: &[RoleSpec], parsed: Parsed, status: i32, progress: f64) {
    for warning in &parsed.warnings {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("QML XmlListModel: {warning}"),
        );
    }
    let cx = Cx::new(e, me);
    let shared = match cx.with(|m: &XmlListModel| Rc::clone(&m.shared)) {
        Ok(shared) => shared,
        Err(_) => return,
    };
    let (signals, count) = {
        let mut s = shared.borrow_mut();
        s.role_names = roles.iter().map(|r| Atom::from(r.name.as_str())).collect();
        s.rows = parsed.rows;
        s.error = String::new();
        (s.signals.clone(), s.rows.len() as i32)
    };
    if let Some(signals) = signals {
        signals.model_reset(e);
    }
    e.batch(|| {
        let _ = cx.set(XmlListModel::COUNT, count);
        let _ = cx.set(XmlListModel::PROGRESS, progress);
        set_status(e, me, status);
    });
    e.request_wake();
}

// Read one extra byte to distinguish a complete document from a truncated one.
fn read_document(reader: impl std::io::Read) -> Result<Vec<u8>, String> {
    read_document_with_limit(reader, parse::DOCUMENT_BYTES)
}

fn read_document_with_limit(reader: impl std::io::Read, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    if bytes.len() > limit {
        return Err("XML document byte limit exceeded".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// The document whose load panics on the worker.
    pub(super) const PANICS: &[u8] = b"<ljos-test-panics/>";

    #[test]
    fn a_load_that_panics_ends_in_error_not_loading() {
        let e = ljos::engine();
        register(&e);
        let platform = Platform::of(&e);
        platform.register_resource("file:///boom.xml", PANICS.to_vec());
        let mut project = ljos_compiler::Project::new(ljos::dev::EngineOracle::from_engine(&e));
        project.add(
            "Main",
            r#"
            import QtQml.XmlListModel
            XmlListModel {
                source: "boom.xml"
                query: "/a"
            }
            "#,
        );
        let compiled = ljos_compiler::compile_file(&project, "Main").expect("compiles");
        let mut source = ljos::MapSource::new();
        source.insert("Main", compiled.file);
        e.set_component_source(source);
        let m = e
            .create_from_url("Main", None, Vec::new())
            .expect("created");
        e.take_log();
        platform.finish(&e);
        assert_eq!(
            e.read_by_name(m, "status").to_number(),
            f64::from(status::ERROR)
        );
        let said = e
            .invoke_by_name(m, "errorString", &[])
            .expect("errorString");
        assert!(
            said.to_js_string().contains("a load that panics"),
            "errorString was {:?}",
            said.to_js_string()
        );
        // The panic is a fault of the engine, as one on its own thread is.
        assert!(
            e.faults()
                .iter()
                .any(|f| format!("{:?}", f.fault).contains("a load that panics")),
            "faults: {:?}",
            e.faults()
        );
    }

    #[test]
    fn document_reader_rejects_overflow_without_truncating() {
        let exact = std::io::repeat(b' ').take(8);
        assert_eq!(read_document_with_limit(exact, 8).unwrap().len(), 8);
        let oversized = std::io::repeat(b' ').take(9);
        assert!(
            read_document_with_limit(oversized, 8)
                .unwrap_err()
                .contains("document byte limit")
        );
        assert_eq!(read_document_with_limit(&b"<a/>"[..], 8).unwrap(), b"<a/>");
        let oversized = vec![b' '; parse::DOCUMENT_BYTES + 1];
        assert!(
            parse::parse(&oversized, &[], &[])
                .unwrap_err()
                .contains("document byte limit")
        );
    }
}
