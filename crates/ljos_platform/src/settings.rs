//! `Settings` (QtCore, doc.qt.io/qt-6/qml-qtcore-settings.html): the
//! properties declared on it, and values set by key, kept in a file.
//!
//! The file is read when the first `Settings` of it is complete, and kept in
//! memory for every `Settings` of the engine that names it, so they agree at
//! once. A change is written half a second after the last one, or at
//! `sync()`, by a thread that does nothing else: the Engine thread only
//! hands it the new text. That thread sleeps until there is something to
//! write, so nothing polls.
//!
//! The file is an INI file as QSettings writes one on Linux: a `[General]`
//! section for keys without a category, and one per category. A list or an
//! object is written as `@Json(...)`, which is Ljós's own; QSettings writes
//! `@Variant(...)` there, which only Qt reads.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, LogLevel, ObjectId, PropIndex, QmlObject, QmlType,
    QtObject, Timer, TypeTag, Url, Value, WriteKind, number_to_string, qml_impl, string_to_number,
};

use crate::paths::url_path;
use crate::platform::{CATEGORY, Platform, Reply};

/// How long after the last change a file is written.
const WRITE_DELAY_MS: i32 = 500;

/// The section of keys that have no category.
const GENERAL: &str = "General";

/// `Settings` (QtCore).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtCore", name = "Settings", extends = QtObject)]
pub struct Settings {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub category: Atom,
    #[qml(notify)]
    pub location: Url,
}

#[qml_impl]
impl Settings {
    /// The value stored under `key`, or `default` (undefined when not
    /// given) when nothing is.
    #[qml(invokable)]
    fn value(cx: &Cx, key: Value, default: Value) -> Value {
        let Some(bound) = bound(cx.engine, cx.this) else {
            return default;
        };
        let key = key.to_js_string();
        bound
            .file
            .get(&bound.section, &key)
            .map_or(default, |stored| stored.value(cx.engine))
    }

    #[qml(invokable, name = "setValue")]
    fn set_value(cx: &Cx, key: Value, value: Value) {
        if let Some(bound) = bound(cx.engine, cx.this) {
            let key = key.to_js_string().to_string();
            bound.file.put(&bound.section, key, Stored::Value(value));
            changed(cx.engine, &bound);
        }
    }

    /// Writes what changed now, rather than half a second from now.
    #[qml(invokable)]
    fn sync(cx: &Cx) {
        if let Some(bound) = bound(cx.engine, cx.this) {
            write_now(cx.engine, &bound.file);
            bound.file.as_written(cx.engine);
        }
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        bind(cx.engine, cx.this)
    }
}

/// A value as it is kept: as the file had it, or as the application set it.
#[derive(Clone, Debug)]
enum Stored {
    /// The text of the file, not yet given a type.
    Text(String),
    Value(Value),
}

impl Stored {
    /// What a script reads: text from the file reads as a string, as
    /// QSettings gives it, except what was written as JSON.
    fn value(&self, engine: &Engine) -> Value {
        match self {
            Stored::Value(v) => v.clone(),
            Stored::Text(t) => match t.strip_prefix("@Json(").and_then(|r| r.strip_suffix(')')) {
                Some(json) => from_json(engine, json),
                None => Value::from(t.as_str()),
            },
        }
    }

    /// As the type of the property it is read into.
    fn typed(&self, engine: &Engine, ty: &TypeTag) -> Option<Value> {
        let Stored::Text(t) = self else {
            return Some(self.value(engine));
        };
        Some(match ty {
            TypeTag::Bool => Value::Bool(t == "true"),
            TypeTag::Int | TypeTag::Enum(_) | TypeTag::Flags(_) => {
                let n = string_to_number(t);
                if n.is_nan() {
                    return None;
                }
                Value::Int(n as i32)
            }
            TypeTag::Real => {
                let n = string_to_number(t);
                if n.is_nan() {
                    return None;
                }
                Value::Real(n)
            }
            TypeTag::String => Value::from(t.as_str()),
            TypeTag::Url => Value::Url(Atom::from(t.as_str())),
            TypeTag::Color => Value::Color(Color::parse(t)?),
            _ => self.value(engine),
        })
    }

    /// The text of the file.
    fn text(&self, engine: &Engine) -> String {
        match self {
            Stored::Text(t) => t.clone(),
            Stored::Value(v) => match v {
                Value::Bool(b) => b.to_string(),
                Value::Int(i) => i.to_string(),
                Value::Real(r) => number_to_string(*r),
                Value::String(s) | Value::Url(s) => s.to_string(),
                Value::Color(c) => c.to_string(),
                Value::Undefined | Value::Null => String::new(),
                Value::Enum(e) => e.value.to_string(),
                other => match to_json(engine, other) {
                    Some(json) => format!("@Json({json})"),
                    None => other.to_js_string().to_string(),
                },
            },
        }
    }
}

fn to_json(engine: &Engine, v: &Value) -> Option<String> {
    let stringify = engine.eval_js("JSON.stringify").ok()?;
    match engine.call_js(&stringify, std::slice::from_ref(v))? {
        Value::String(s) => Some(s.to_string()),
        _ => None,
    }
}

fn from_json(engine: &Engine, json: &str) -> Value {
    engine
        .eval_js("JSON.parse")
        .ok()
        .and_then(|parse| engine.call_js(&parse, &[Value::from(json)]))
        .unwrap_or(Value::Undefined)
}

/// One settings file, as the engine holds it.
#[derive(Debug)]
struct SettingsFile {
    path: PathBuf,
    sections: RefCell<BTreeMap<String, BTreeMap<String, Stored>>>,
    /// Changed since it was last handed to the writer.
    dirty: Cell<bool>,
    /// The text as of the last change, so that it can still be written
    /// when the engine is gone and nothing can be rendered.
    pending_text: RefCell<Option<String>>,
}

impl SettingsFile {
    fn load(path: PathBuf) -> SettingsFile {
        // Read here, on the Engine thread, as QSettings reads: the file is
        // small, and the properties must start from it.
        let sections = std::fs::read_to_string(&path)
            .map(|t| parse(&t))
            .unwrap_or_default();
        SettingsFile {
            path,
            sections: RefCell::new(sections),
            dirty: Cell::new(false),
            pending_text: RefCell::new(None),
        }
    }

    fn get(&self, section: &str, key: &str) -> Option<Stored> {
        self.sections.borrow().get(section)?.get(key).cloned()
    }

    fn put(&self, section: &str, key: String, value: Stored) {
        self.sections
            .borrow_mut()
            .entry(section.to_owned())
            .or_default()
            .insert(key, value);
        self.dirty.set(true);
    }

    /// What a sync leaves: every value as the file has it, so that a later
    /// read gives the string QSettings reads back (Qt 6.11.2 gives "9" for
    /// an int 9 written and synced).
    fn as_written(&self, engine: &Engine) {
        for keys in self.sections.borrow_mut().values_mut() {
            for stored in keys.values_mut() {
                if let Stored::Value(_) = stored {
                    *stored = Stored::Text(stored.text(engine));
                }
            }
        }
    }

    fn render(&self, engine: &Engine) -> String {
        let mut out = String::new();
        for (section, keys) in self.sections.borrow().iter() {
            if keys.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{section}]\n"));
            for (key, stored) in keys {
                out.push_str(&format!("{key}={}\n", quote(&stored.text(engine))));
            }
        }
        out
    }
}

/// Quotes a value the way an INI reader needs: when it has room for doubt.
fn quote(text: &str) -> String {
    let doubtful = text.is_empty()
        || text.starts_with(char::is_whitespace)
        || text.ends_with(char::is_whitespace)
        || text.contains([',', ';', '"', '\\', '\n', '\r', '#']);
    if !doubtful || text.is_empty() {
        return text.to_owned();
    }
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn unquote(text: &str) -> String {
    let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) else {
        return text.to_owned();
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn parse(text: &str) -> BTreeMap<String, BTreeMap<String, Stored>> {
    let mut sections: BTreeMap<String, BTreeMap<String, Stored>> = BTreeMap::new();
    let mut section = GENERAL.to_owned();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with([';', '#']) {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_owned();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(section.clone())
                .or_default()
                .insert(key.trim().to_owned(), Stored::Text(unquote(value.trim())));
        }
    }
    sections
}

/// What one `Settings` object is bound to.
#[derive(Debug, Clone)]
struct Bound {
    file: Rc<SettingsFile>,
    section: String,
    /// Restarted by each change; writes when it fires.
    timer: Option<ObjectId>,
}

/// The settings files of one engine and the `Settings` objects bound to
/// them.
#[derive(Debug, Default)]
struct SettingsFiles {
    files: RefCell<HashMap<PathBuf, Rc<SettingsFile>>>,
    bound: RefCell<HashMap<ObjectId, Bound>>,
}

impl Drop for SettingsFiles {
    fn drop(&mut self) {
        // The engine is going: what is still waiting for its half second
        // is written now. The text was rendered at each change.
        for file in self.files.borrow().values() {
            if file.dirty.get()
                && let Some(text) = file.pending_text.borrow_mut().take()
            {
                writer().post(file.path.clone(), text, None);
            }
        }
    }
}

fn bound(engine: &Engine, obj: ObjectId) -> Option<Bound> {
    engine
        .extension::<SettingsFiles>()
        .bound
        .borrow()
        .get(&obj)
        .cloned()
}

/// The file a `Settings` with no location keeps: the application's, in
/// the user's configuration directory.
fn default_path(engine: &Engine) -> std::io::Result<PathBuf> {
    let platform = Platform::of(engine);
    let (org, app) = platform.application(engine);
    let env = platform.env();
    let mut path = env.config_home();
    if !org.is_empty() {
        path.push(&org);
    }
    path.push(format!("{app}.conf"));
    env.real(&path)
}

fn file_for(engine: &Engine, obj: ObjectId) -> std::io::Result<PathBuf> {
    let location = engine
        .untrack(|| engine.get(obj, Settings::LOCATION))
        .map(|u| u.0.to_string())
        .unwrap_or_default();
    if location.is_empty() {
        return default_path(engine);
    }
    let resolved = engine.resolve_url(obj, &location);
    let path = url_path(&resolved).unwrap_or_else(|| PathBuf::from(location));
    // A sandboxed run (a test) keeps every file under its root.
    Platform::of(engine).env().real(&path)
}

/// The properties a `Settings` declares in QML, which it keeps.
fn kept_properties(engine: &Engine, obj: ObjectId) -> Vec<(PropIndex, String, TypeTag)> {
    let own = Settings::static_meta().properties.len();
    let Some(meta) = engine.meta_of(obj) else {
        return Vec::new();
    };
    meta.properties
        .iter()
        .enumerate()
        .skip(own)
        .filter(|(_, p)| !p.is_readonly())
        .map(|(i, p)| (PropIndex(i as u32), p.name.to_string(), p.ty.clone()))
        .collect()
}

fn bind(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let files = engine.extension::<SettingsFiles>();
    let path = match file_for(engine, obj) {
        Ok(path) => path,
        Err(err) => {
            engine.log(
                ljos_engine::LogLevel::Warning,
                "settings",
                format!("Settings: cannot resolve file: {err}"),
            );
            return Ok(());
        }
    };
    let file = Rc::clone(
        files
            .files
            .borrow_mut()
            .entry(path.clone())
            .or_insert_with(|| Rc::new(SettingsFile::load(path))),
    );
    let category = engine
        .untrack(|| engine.get(obj, Settings::CATEGORY))
        .map(|c| c.to_string())
        .unwrap_or_default();
    let section = if category.is_empty() {
        GENERAL.to_owned()
    } else {
        category
    };
    let props = kept_properties(engine, obj);
    // The file's values first, then the watches: what was read is not a
    // change to write back.
    for (index, name, ty) in &props {
        if let Some(v) = file.get(&section, name).and_then(|s| s.typed(engine, ty)) {
            engine.write(obj, *index, v, WriteKind::Imperative)?;
        }
    }
    let timer = if props.is_empty() {
        None
    } else {
        Some(make_timer(engine, obj, Rc::clone(&file))?)
    };
    files.bound.borrow_mut().insert(
        obj,
        Bound {
            file,
            section,
            timer,
        },
    );
    for (index, name, _) in props {
        engine.watch(obj, index, move |e, v| {
            if let Some(bound) = bound(e, obj) {
                bound
                    .file
                    .put(&bound.section, name.clone(), Stored::Value(v.clone()));
                changed(e, &bound);
            }
        })?;
    }
    Ok(())
}

fn make_timer(
    engine: &Engine,
    obj: ObjectId,
    file: Rc<SettingsFile>,
) -> Result<ObjectId, EngineError> {
    let quiet = Timer {
        interval: WRITE_DELAY_MS,
        ..Default::default()
    };
    let timer = engine.create_native_incomplete(quiet, Some(obj))?;
    engine.complete(timer)?;
    engine.connect_by_name(timer, "triggered", move |e, _| write_now(e, &file))?;
    Ok(timer)
}

/// Something changed: it is written once nothing has for a while.
fn changed(engine: &Engine, bound: &Bound) {
    *bound.file.pending_text.borrow_mut() = Some(bound.file.render(engine));
    let restarted = match bound.timer {
        Some(t) if engine.is_alive(t) => engine.invoke_by_name(t, "restart", &[]).map(|_| ()),
        // A Settings with no properties of its own has no timer: what it
        // sets by key is written at once, as it is rare.
        _ => {
            write_now(engine, &bound.file);
            Ok(())
        }
    };
    if let Err(e) = restarted {
        engine.log(LogLevel::Warning, CATEGORY, format!("Settings: {e}"));
        write_now(engine, &bound.file);
    }
}

/// Hands the file's text to the writer.
fn write_now(engine: &Engine, file: &SettingsFile) {
    if !file.dirty.replace(false) {
        return;
    }
    let rendered = file.pending_text.borrow_mut().take();
    let text = rendered.unwrap_or_else(|| file.render(engine));
    let reply = Platform::of(engine).expect_for("Settings write", None);
    writer().post(file.path.clone(), text, Some(reply));
}

/// Waits until every settings file handed to the writer is on disk. For
/// tests and for a host that is about to exit.
pub fn flush_writes() {
    writer().flush();
}

/// The thread that writes settings files.
#[derive(Default)]
struct Writer {
    state: Mutex<WriterState>,
    changed: Condvar,
}

#[derive(Default)]
struct WriterState {
    /// The newest text of each file not written yet, and who to tell if
    /// writing it fails.
    pending: HashMap<PathBuf, (String, Option<Reply>)>,
    busy: bool,
    started: bool,
}

fn writer() -> &'static Arc<Writer> {
    static WRITER: OnceLock<Arc<Writer>> = OnceLock::new();
    WRITER.get_or_init(|| Arc::new(Writer::default()))
}

impl Writer {
    fn post(self: &Arc<Self>, path: PathBuf, text: String, reply: Option<Reply>) {
        let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        // A newer text replaces one not written yet; its reply is dropped,
        // which counts as answered.
        st.pending.insert(path, (text, reply));
        if !st.started {
            let me = Arc::clone(self);
            let spawned = std::thread::Builder::new()
                .name("ljos-settings".to_owned())
                .spawn(move || me.run());
            st.started = spawned.is_ok();
            if !st.started {
                // No thread to write with: write here rather than lose it.
                let jobs: Vec<_> = st.pending.drain().collect();
                drop(st);
                for (path, (text, reply)) in jobs {
                    finish(&path, &text, reply);
                }
                return;
            }
        }
        self.changed.notify_all();
    }

    fn run(&self) {
        loop {
            let (path, (text, reply)) = {
                let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
                st.busy = false;
                self.changed.notify_all();
                loop {
                    let next = st.pending.keys().next().cloned();
                    if let Some(path) = next {
                        let job = st.pending.remove(&path).expect("the key was just read");
                        st.busy = true;
                        break (path, job);
                    }
                    st = self
                        .changed
                        .wait(st)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            };
            finish(&path, &text, reply);
        }
    }

    fn flush(&self) {
        let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while st.started && (st.busy || !st.pending.is_empty()) {
            st = self
                .changed
                .wait(st)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

fn finish(path: &Path, text: &str, reply: Option<Reply>) {
    if let Err(e) = write_file(path, text) {
        let message = format!("Settings: cannot write {}: {e}", path.display());
        match reply {
            Some(reply) => {
                reply.send(move |engine| engine.log(LogLevel::Warning, CATEGORY, message))
            }
            None => eprintln!("{message}"),
        }
    }
}

/// Writes beside the file and renames, so that a reader never sees half of
/// it.
fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".ljos-new");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ini_file_reads_by_section() {
        let s = parse("[General]\nwidth=7\nname = \"a, b\"\n; note\n[window]\nwidth=640\n");
        let get = |sec: &str, key: &str| match s.get(sec).and_then(|k| k.get(key)) {
            Some(Stored::Text(t)) => t.clone(),
            _ => String::from("-"),
        };
        assert_eq!(get("General", "width"), "7");
        assert_eq!(get("General", "name"), "a, b");
        assert_eq!(get("window", "width"), "640");
    }

    #[test]
    fn doubtful_values_are_quoted_and_read_back() {
        for v in [
            "plain",
            "a, b",
            " lead",
            "q\"uote",
            "back\\slash",
            "two\nlines",
        ] {
            assert_eq!(unquote(&quote(v)), v, "{v}");
        }
        assert_eq!(quote("Ada"), "Ada");
    }

    #[test]
    fn the_writer_writes_the_newest_text_and_says_when_it_is_done() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("deep/app.conf");
        writer().post(path.clone(), "[General]\na=1\n".into(), None);
        writer().post(path.clone(), "[General]\na=2\n".into(), None);
        flush_writes();
        assert_eq!(
            std::fs::read_to_string(&path).expect("written"),
            "[General]\na=2\n"
        );
    }
}
