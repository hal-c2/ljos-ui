//! The Ljós backend (ARCHITECTURE.md §18.3): the scenario runs in-process on
//! a real [`Engine`] with `QtQuick` registered, under a [`QuickScene`], on a
//! virtual clock.
//!
//! It mirrors the Qt oracle's setup so that one scenario means the same on
//! both: every loaded file is created as a child of a 400×400 driver item,
//! objects are found by `objectName` below the loaded roots, and Qt-style
//! messages are collected from the engine and the scene. Unlike the oracle,
//! nothing is replayed: each step acts on the live engine, then the scene is
//! settled (frames run until nothing is dirty), as a host would before the
//! user could see anything. Time is virtual: "1 second passes" fires each
//! timer at its own deadline without sleeping.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc, time::Duration};

use ljos_a11y::{NodeKey, Tree, role_word};
use ljos_compiler::{Project, Severity, compile_file};
use ljos_engine::{Engine, MapSource, ObjectId, Value as QmlValue};
use ljos_quick::{KeyEvent, Modifiers, MouseButtons, PointerEvent, QuickScene, keys::key};
use ljos_render::{FrameView, RasterConfig, VelloCpuReference, kurbo};
use ljos_runtime::{Clock as _, Time, VirtualClock};
use qml_front::QtTypeOracle;
use serde_json::{Map, Number, Value, json};

use super::{A11yNode, Backend, BackendError, LogEntry, LogLevel, PointerStep, QmlFile, Result};

mod reload;

/// The driver's file key. Not a valid QML type name for a scenario author
/// to collide with by accident.
const DRIVER: &str = "__LjosBddDriver";

/// Same size as the Qt oracle's driver item, so `parent.width` agrees.
const VIEWPORT: f64 = 400.0;

/// Helpers that need JavaScript semantics. `bindWith` is how `Qt.binding`
/// gets a function with the target's properties in scope; the function
/// itself is made by a sloppy-mode `eval` (QML modules are strict and do
/// not allow `with`).
const DRIVER_SOURCE: &str = r#"import QtQuick
Item {
    width: 400
    height: 400
    function bindWith(target, prop, maker) {
        var parts = prop.split(".")
        var o = target
        for (var i = 0; i < parts.length - 1; ++i) o = o[parts[i]]
        o[parts[parts.length - 1]] = Qt.binding(maker(target))
    }
    function toJson(v) {
        try { return JSON.stringify(v) } catch (e) { return JSON.stringify(String(v)) }
    }
}
"#;

/// Frames run per settle before the scene is reported as never idle.
const MAX_SETTLE_FRAMES: usize = 64;
/// Timer firings per `advance` before a zero-interval repeating timer is
/// reported instead of hanging the run.
const MAX_TIMER_STEPS: usize = 100_000;

/// The modifier keys in the order a chord presses them.
const MODIFIER_KEYS: [(Modifiers, i32); 4] = [
    (Modifiers::CONTROL, key::CONTROL),
    (Modifiers::SHIFT, key::SHIFT),
    (Modifiers::ALT, key::ALT),
    (Modifiers::META, key::META),
];

/// One key with the modifiers held around it, as a step names it
/// (`"Ctrl+Shift+S"`, `"Tab"`, `"Ctrl++"`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Chord {
    modifiers: Modifiers,
    key: i32,
    /// What the key types with those modifiers.
    text: String,
}

impl Chord {
    fn parse(text: &str) -> std::result::Result<Self, String> {
        let (mods, name) = match text.strip_suffix("++") {
            Some(mods) => (mods, "+"),
            None => match text.rsplit_once('+') {
                Some((mods, name)) if !name.is_empty() => (mods, name),
                _ => ("", text),
            },
        };
        let mut modifiers = Modifiers::empty();
        for m in mods.split('+').filter(|m| !m.is_empty()) {
            modifiers |= match m.to_ascii_lowercase().as_str() {
                "shift" => Modifiers::SHIFT,
                "ctrl" => Modifiers::CONTROL,
                "alt" => Modifiers::ALT,
                "meta" => Modifiers::META,
                _ => return Err(format!("unknown modifier {m:?} in the chord {text:?}")),
            };
        }
        // The spellings the Qt oracle's keyboard also takes.
        let name = match name {
            n if n.eq_ignore_ascii_case("PageUp") => "PgUp",
            n if n.eq_ignore_ascii_case("PageDown") => "PgDown",
            n => n,
        };
        let code = key::from_name(name)
            .ok_or_else(|| format!("unknown key {name:?} in the chord {text:?}"))?;
        let shift = modifiers.contains(Modifiers::SHIFT);
        // A desktop sends Shift+Tab as the Backtab key.
        let code = if code == key::TAB && shift {
            key::BACKTAB
        } else {
            code
        };
        let commands = Modifiers::CONTROL | Modifiers::ALT | Modifiers::META;
        let mut chars = name.chars();
        let typed = match (chars.next(), chars.next()) {
            _ if modifiers.intersects(commands) => String::new(),
            (Some(c), None) if shift => c.to_uppercase().collect(),
            (Some(c), None) => c.to_lowercase().collect(),
            _ if code == key::SPACE => " ".to_owned(),
            // A Qt key event for Tab carries the tab; a `TextEdit` types it.
            _ if code == key::TAB && !shift => "\t".to_owned(),
            _ => String::new(),
        };
        Ok(Self {
            modifiers,
            key: code,
            text: typed,
        })
    }
}

/// The live parts, made by the first `load`.
struct Live {
    engine: Engine,
    clock: Arc<VirtualClock>,
    scene: QuickScene,
    driver: ObjectId,
    /// What each `load` created, in order; lookups start here.
    roots: Vec<ObjectId>,
    /// Shared with the engine's and the scene's log handlers, so the order
    /// of messages across both is kept.
    log: Rc<RefCell<Vec<LogEntry>>>,
    /// The staged files the component source was last built from.
    compiled_from: BTreeMap<String, String>,
    /// When the scene last said it next needs a frame (a running
    /// animation's next tick). `advance` wakes it then, as a host would.
    wake: Option<Time>,
    /// Whether a pointer step pressed the button and none released it.
    pointer_down: bool,
    /// The reload session of a scenario that changes files while it runs.
    reloads: Option<reload::Rig>,
}

#[derive(Default)]
pub struct LjosBackend {
    live: Option<Live>,
    /// Catalogues given before anything was loaded; the engine gets them
    /// when it is made.
    catalogs: Vec<ljos_i18n::Catalog>,
    /// Picture files given before anything was loaded, by URL.
    resources: Vec<(String, Vec<u8>)>,
    /// Whether the scenario changes files while it runs.
    watching: bool,
}

impl std::fmt::Debug for LjosBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LjosBackend")
            .field("loaded", &self.live.as_ref().map_or(0, |l| l.roots.len()))
            .finish()
    }
}

fn failed(e: impl std::fmt::Display) -> BackendError {
    BackendError::Failed(e.to_string())
}

fn not_loaded() -> BackendError {
    BackendError::Failed("nothing is loaded yet".into())
}

/// `Scenario.qml` → `Scenario`, the engine's file key.
fn key_of(name: &str) -> &str {
    name.strip_suffix(".qml").unwrap_or(name)
}

impl LjosBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn live(&mut self) -> Result<&mut Live> {
        self.live.as_mut().ok_or_else(not_loaded)
    }

    fn start() -> Result<Live> {
        let engine = Engine::new();
        // Qt evaluates a binding when it is installed, which is when it
        // reports a binding loop; lazy evaluation would stay silent.
        engine.set_eager_bindings(true);
        ljos_quick::register(&engine);
        ljos_controls::register(&engine);
        let clock = Arc::new(VirtualClock::new());
        engine.set_clock(Rc::new(clock.clone()));
        let log: Rc<RefCell<Vec<LogEntry>>> = Rc::default();
        let sink = log.clone();
        engine.set_log_handler(move |e| {
            sink.borrow_mut().push(LogEntry {
                level: level(e.level),
                message: e.message.clone(),
            });
        });
        let mut compiled = BTreeMap::new();
        compiled.insert(format!("{DRIVER}.qml"), DRIVER_SOURCE.to_owned());
        engine.set_component_source(compile(&compiled)?);
        let driver = engine
            .create_from_url(DRIVER, None, Vec::new())
            .map_err(|e| failed(format!("the test driver does not load: {e}")))?;
        let mut scene = QuickScene::new(kurbo::Size::new(VIEWPORT, VIEWPORT));
        let sink = log.clone();
        scene.log().set_handler(move |e| {
            sink.borrow_mut().push(LogEntry {
                level: LogLevel::Warning,
                message: e.message.clone(),
            });
        });
        scene.set_root(&engine, driver).map_err(failed)?;
        let mut live = Live {
            engine,
            clock,
            scene,
            driver,
            roots: Vec::new(),
            log,
            compiled_from: compiled,
            wake: None,
            pointer_down: false,
            reloads: None,
        };
        // A decode takes as long as it takes; scenarios see its result when
        // time is advanced and not before, whatever the machine's speed.
        ljos_quick::ImageStore::of(&live.engine).set_held(true);
        live.settle()?;
        Ok(live)
    }
}

fn level(l: ljos_engine::LogLevel) -> LogLevel {
    match l {
        ljos_engine::LogLevel::Debug => LogLevel::Debug,
        ljos_engine::LogLevel::Info => LogLevel::Info,
        ljos_engine::LogLevel::Warning => LogLevel::Warning,
        ljos_engine::LogLevel::Critical => LogLevel::Critical,
    }
}

/// Compiles every file against every other, as `qml6` sees one directory.
/// A file with errors fails the whole load, as `Qt.createComponent` would
/// for the file that uses it.
fn compile(files: &BTreeMap<String, String>) -> Result<MapSource> {
    let mut project = Project::new(QtTypeOracle);
    for (name, source) in files {
        project.add(key_of(name), source.as_str());
    }
    let mut out = MapSource::new();
    for name in files.keys() {
        let key = key_of(name);
        let compiled = compile_file(&project, key).map_err(|d| {
            let lines: Vec<String> = d.iter().map(ToString::to_string).collect();
            failed(format!("{name}: {}", lines.join("; ")))
        })?;
        if compiled.has_errors() {
            let lines: Vec<String> = compiled
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(ToString::to_string)
                .collect();
            return Err(failed(format!("{name}: {}", lines.join("; "))));
        }
        out.insert(key, compiled.file);
    }
    Ok(out)
}

impl Live {
    /// Runs frames until the scene is idle: polish (positioners, text),
    /// anchors, deferred deletes and sync, as a host does before the user
    /// sees anything.
    fn settle(&mut self) -> Result<()> {
        for _ in 0..MAX_SETTLE_FRAMES {
            let now = self.clock.now();
            let out = self.scene.frame(&self.engine, now);
            if !self.scene.needs_frame() && out.next_wake.is_none_or(|t| t > now) {
                self.wake = out.next_wake;
                return Ok(());
            }
        }
        Err(failed(format!(
            "the scene did not settle in {MAX_SETTLE_FRAMES} frames"
        )))
    }

    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()> {
        let mut wanted: BTreeMap<String, String> = files
            .iter()
            .map(|f| (f.name.clone(), f.source.clone()))
            .collect();
        wanted.insert(format!("{DRIVER}.qml"), DRIVER_SOURCE.to_owned());
        if wanted != self.compiled_from {
            // A scenario that reloads compiles through its reload session.
            if !self.reload_compile(&wanted)? {
                let source = compile(&wanted)?;
                self.engine.set_component_source(source);
            }
            self.compiled_from = wanted;
        }
        let obj = self
            .engine
            .create_from_url(key_of(entry), Some(self.driver), Vec::new())
            .map_err(|e| failed(format!("cannot load {entry}: {e}")))?;
        // `createObject(driver)`: an item becomes a visual child.
        ljos_quick::add_data(&self.engine, self.driver, obj).map_err(failed)?;
        self.roots.push(obj);
        self.settle()
    }

    /// `"list/row"` finds `list` anywhere below a root, then `row` anywhere
    /// below it, depth first in declaration order.
    fn find(&self, path: &str) -> Result<ObjectId> {
        let mut cur: Option<ObjectId> = None;
        for part in path.split('/') {
            let starts = match cur {
                Some(c) => vec![c],
                None => self.roots.clone(),
            };
            cur = starts.iter().find_map(|r| self.deep(*r, part));
            if cur.is_none() {
                return Err(failed(format!("no object named \"{part}\"")));
            }
        }
        cur.ok_or_else(|| failed("empty object path"))
    }

    fn deep(&self, root: ObjectId, name: &str) -> Option<ObjectId> {
        let e = &self.engine;
        let mut stack = vec![root];
        while let Some(o) = stack.pop() {
            if !e.is_alive(o) {
                continue;
            }
            if let QmlValue::String(s) = e.read_by_name(o, "objectName")
                && s.as_str() == name
            {
                return Some(o);
            }
            let mut kids = e.children(o);
            kids.reverse();
            stack.extend(kids);
        }
        None
    }

    /// The object and property a dotted path names: `anchors.margins` is
    /// one (flattened) property of the item; `foo.bar` where `foo` holds an
    /// object is `bar` of that object; `Accessible.name` is `name` of the
    /// object that type attaches.
    fn owner(&self, obj: ObjectId, prop: &str) -> Result<(ObjectId, String)> {
        let mut obj = obj;
        let mut rest = prop;
        loop {
            if self.engine.property_index(obj, rest).is_ok() {
                return Ok((obj, rest.to_owned()));
            }
            let Some((head, tail)) = rest.split_once('.') else {
                return Ok((obj, rest.to_owned()));
            };
            let next = match self.engine.read_by_name(obj, head).as_object() {
                Some(next) => Some(next),
                None => self.attached(obj, head),
            };
            match next {
                Some(next) => {
                    obj = next;
                    rest = tail;
                }
                None => match self.further(obj, rest) {
                    Some((next, tail)) => {
                        obj = next;
                        rest = tail;
                    }
                    None => return Ok((obj, rest.to_owned())),
                },
            }
        }
    }

    /// The object further along `rest` than its first name: the one a
    /// member of a group holds (`up.indicator.x`), or an element of a list
    /// by its number (`children.0.visible`), and the path that is left.
    fn further<'p>(&self, obj: ObjectId, rest: &'p str) -> Option<(ObjectId, &'p str)> {
        for (at, _) in rest.match_indices('.') {
            let (head, tail) = (&rest[..at], &rest[at + 1..]);
            if self.engine.property_index(obj, head).is_err() {
                continue;
            }
            match self.engine.read_by_name(obj, head) {
                QmlValue::List(items) => {
                    let (index, after) = tail.split_once('.')?;
                    let next = items.get(index.parse::<usize>().ok()?)?.as_object()?;
                    return Some((next, after));
                }
                held => {
                    if let Some(next) = held.as_object() {
                        return Some((next, tail));
                    }
                }
            }
        }
        None
    }

    /// A write to a member of a value type (`font.pixelSize`) as the write
    /// of the whole value with that member changed, which is what QML does.
    /// Anything else comes back as it was.
    fn member_write(&self, owner: ObjectId, prop: String, v: QmlValue) -> (String, QmlValue) {
        if self.engine.property_index(owner, &prop).is_ok() {
            return (prop, v);
        }
        let whole = prop.split_once('.').and_then(|(head, member)| {
            let meta = self.engine.meta_of(owner)?;
            let ty = meta.property(meta.property_index(head)?)?.ty.clone();
            let changed = self
                .engine
                .read_by_name(owner, head)
                .with_member(&ty, member, &v)?;
            Some((head.to_owned(), changed))
        });
        whole.unwrap_or((prop, v))
    }

    /// The object the type `name` attaches to `obj`, made on first use as
    /// QML makes it. Type names are capitalised; properties are not.
    fn attached(&self, obj: ObjectId, name: &str) -> Option<ObjectId> {
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let attacher = self.engine.lookup_type_any(name)?;
        self.engine.attached_object(obj, &attacher).ok()
    }

    fn to_qml(&self, v: &Value) -> Result<QmlValue> {
        Ok(match v {
            Value::Null => QmlValue::Null,
            Value::Bool(b) => QmlValue::Bool(*b),
            Value::Number(n) => match n.as_i64().and_then(|i| i32::try_from(i).ok()) {
                Some(i) => QmlValue::Int(i),
                None => QmlValue::Real(n.as_f64().unwrap_or(f64::NAN)),
            },
            Value::String(s) => QmlValue::string(s.as_str()),
            Value::Array(items) => QmlValue::list(
                items
                    .iter()
                    .map(|i| self.to_qml(i))
                    .collect::<Result<_>>()?,
            ),
            // A JS object literal, made by JS so that it is one.
            Value::Object(_) => self.engine.eval_js(&format!("({v})")).map_err(failed)?,
        })
    }

    /// As the Qt driver's `ser`: what `JSON.stringify` would give, colours
    /// as their `#rrggbb` text.
    fn to_json(&self, v: &QmlValue) -> Value {
        // Whole numbers print as integers, as `JSON.stringify` does.
        let num = |n: f64| {
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                json!(n as i64)
            } else {
                Number::from_f64(n).map_or_else(|| Value::String(n.to_string()), Value::Number)
            }
        };
        match v {
            QmlValue::Undefined | QmlValue::Null => Value::Null,
            QmlValue::Bool(b) => Value::Bool(*b),
            QmlValue::Int(i) => json!(i),
            QmlValue::Real(r) => num(*r),
            QmlValue::String(s) | QmlValue::Url(s) => Value::String(s.to_string()),
            QmlValue::Color(c) => Value::String(c.to_string()),
            QmlValue::Enum(e) => json!(e.value),
            QmlValue::Point(p) => json!({"x": num(p.x), "y": num(p.y)}),
            QmlValue::Size(s) => json!({"width": num(s.width), "height": num(s.height)}),
            QmlValue::Rect(r) => json!({
                "x": num(r.x), "y": num(r.y), "width": num(r.width), "height": num(r.height)
            }),
            QmlValue::List(items) => Value::Array(items.iter().map(|i| self.to_json(i)).collect()),
            QmlValue::Font(_)
            | QmlValue::Vector2d(_)
            | QmlValue::Vector3d(_)
            | QmlValue::Vector4d(_)
            | QmlValue::Quaternion(_)
            | QmlValue::Js(_)
            | QmlValue::Object(_) => self.js_json(v),
        }
    }

    /// Values only JavaScript can describe, through `JSON.stringify`.
    fn js_json(&self, v: &QmlValue) -> Value {
        match self
            .engine
            .invoke_by_name(self.driver, "toJson", std::slice::from_ref(v))
        {
            Ok(QmlValue::String(s)) => {
                serde_json::from_str(s.as_str()).unwrap_or(Value::Object(Map::new()))
            }
            _ => Value::String(self.engine.to_js_string(v).to_string()),
        }
    }

    fn advance(&mut self, ms: u64) -> Result<()> {
        let images = ljos_quick::ImageStore::of(&self.engine);
        // Pictures asked for before now have arrived by the time any time
        // has passed: waited for, not slept for.
        images.finish(&self.engine);
        let end = self.clock.now() + Duration::from_millis(ms);
        // Each timer fires at its own deadline, and the scene is settled in
        // between, as a host woken at each deadline would. The scene's own
        // wakes (the frames of a running animation) are kept the same way.
        for _ in 0..MAX_TIMER_STEPS {
            let next = [
                self.engine.next_timer_deadline(),
                self.wake,
                self.reload_due(),
            ]
            .into_iter()
            .flatten()
            .min();
            match next {
                Some(t) if t <= end => {
                    if t > self.clock.now() {
                        self.clock.set(t);
                    }
                    self.engine.advance_timers(self.clock.now());
                    self.reload_if_due()?;
                    self.settle()?;
                }
                _ => {
                    self.clock.set(end);
                    self.engine.advance_timers(end);
                    // And so have those a timer asked for on the way.
                    images.finish(&self.engine);
                    return self.settle();
                }
            }
        }
        Err(failed(format!(
            "timers fired more than {MAX_TIMER_STEPS} times in {ms} ms"
        )))
    }

    /// Types a chord as a keyboard does: the modifier keys go down one by
    /// one, then the key, held until it has repeated `repeats` times, and
    /// everything comes up in reverse.
    fn type_chord(&mut self, chord: &Chord, repeats: u32) -> Result<()> {
        let mut held = Modifiers::empty();
        for (modifier, code) in MODIFIER_KEYS {
            if chord.modifiers.contains(modifier) {
                self.scene
                    .key_event(&self.engine, &KeyEvent::press(code).with_modifiers(held));
                held |= modifier;
            }
        }
        let press = KeyEvent::press(chord.key)
            .with_modifiers(held)
            .with_text(chord.text.clone());
        self.scene.key_event(&self.engine, &press);
        for _ in 0..repeats {
            self.scene
                .key_event(&self.engine, &press.clone().repeated());
        }
        let release = KeyEvent::release(chord.key)
            .with_modifiers(held)
            .with_text(chord.text.clone());
        self.scene.key_event(&self.engine, &release);
        for (modifier, code) in MODIFIER_KEYS.into_iter().rev() {
            if chord.modifiers.contains(modifier) {
                held -= modifier;
                self.scene
                    .key_event(&self.engine, &KeyEvent::release(code).with_modifiers(held));
            }
        }
        self.settle()
    }

    /// Types `text`, one key per character, as a keyboard with a key for
    /// each would.
    fn type_text(&mut self, text: &str) -> Result<()> {
        for c in text.chars() {
            let code = key::from_char(c).unwrap_or(0);
            let held = if c.is_uppercase() {
                Modifiers::SHIFT
            } else {
                Modifiers::empty()
            };
            let typed = c.to_string();
            let press = KeyEvent::press(code)
                .with_modifiers(held)
                .with_text(typed.clone());
            self.scene.key_event(&self.engine, &press);
            let release = KeyEvent::release(code)
                .with_modifiers(held)
                .with_text(typed);
            self.scene.key_event(&self.engine, &release);
        }
        self.settle()
    }

    /// Press at `from`, move to `to` in steps a hand would make, release.
    fn drag(&mut self, from: (f64, f64), to: (f64, f64)) -> Result<()> {
        const STEP: f64 = 8.0;
        let now = self.clock.now();
        self.scene
            .pointer_event(&self.engine, &PointerEvent::press(from.0, from.1, now));
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let steps = (dx.hypot(dy) / STEP).ceil().max(1.0) as u32;
        for i in 1..=steps {
            let t = f64::from(i) / f64::from(steps);
            let (x, y) = (from.0 + dx * t, from.1 + dy * t);
            self.scene.pointer_event(
                &self.engine,
                &PointerEvent::moved(x, y, MouseButtons::LEFT, now),
            );
        }
        self.scene
            .pointer_event(&self.engine, &PointerEvent::release(to.0, to.1, now));
        self.settle()
    }

    fn tap(&mut self, x: f64, y: f64) -> Result<()> {
        let now = self.clock.now();
        self.scene
            .pointer_event(&self.engine, &PointerEvent::press(x, y, now));
        self.scene
            .pointer_event(&self.engine, &PointerEvent::release(x, y, now));
        self.settle()
    }

    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()> {
        let now = self.clock.now();
        let event = match step {
            PointerStep::Press => {
                self.pointer_down = true;
                PointerEvent::press(x, y, now)
            }
            PointerStep::Move => {
                let held = if self.pointer_down {
                    MouseButtons::LEFT
                } else {
                    MouseButtons::empty()
                };
                PointerEvent::moved(x, y, held, now)
            }
            PointerStep::Release => {
                self.pointer_down = false;
                PointerEvent::release(x, y, now)
            }
        };
        self.scene.pointer_event(&self.engine, &event);
        self.settle()
    }

    fn wheel(&mut self, at: (f64, f64), by: super::WheelBy) -> Result<()> {
        let now = self.clock.now();
        let event = match by {
            super::WheelBy::Notches(x, y) => ljos_quick::WheelEvent::notches(at.0, at.1, x, y, now),
            super::WheelBy::Pixels(x, y) => ljos_quick::WheelEvent::pixels(at.0, at.1, x, y, now),
        };
        self.scene.wheel_event(&self.engine, &event);
        self.settle()
    }

    /// The scene builds its accessibility tree only while something reads
    /// it. The first question about the tree starts that, as an assistive
    /// technology starting up would; from then on the tree follows changes.
    fn listen(&mut self) -> Result<()> {
        if !self.scene.accessibility_active() {
            self.scene.set_accessibility_active(&self.engine, true);
            self.settle()?;
        }
        Ok(())
    }

    /// The tree as steps see it: role words, names and nesting.
    fn a11y(&self) -> A11yNode {
        fn below(tree: &Tree, key: NodeKey, focus: Option<NodeKey>) -> Vec<A11yNode> {
            let node = |k: &NodeKey| {
                let d = tree.get(*k)?;
                Some(A11yNode {
                    role: role_word(d.role),
                    name: d.name.clone(),
                    description: d.description.clone(),
                    object_name: d.author_id.clone(),
                    focused: focus == Some(*k),
                    value: d.text.as_ref().map(|t| t.value.clone()).unwrap_or_default(),
                    selected: d
                        .text
                        .as_ref()
                        .and_then(|t| {
                            let (a, b) = (t.anchor.min(t.cursor), t.anchor.max(t.cursor));
                            t.value.get(a..b).map(str::to_owned)
                        })
                        .unwrap_or_default(),
                    children: below(tree, *k, focus),
                })
            };
            tree.children(key).iter().filter_map(node).collect()
        }
        let focus = self.scene.accessible_focus_node();
        A11yNode {
            role: "window".into(),
            // With no node to name, the window is what has the keyboard.
            focused: focus.is_none(),
            children: below(self.scene.accessibility_tree(), NodeKey::ROOT, focus),
            ..A11yNode::default()
        }
    }

    /// The first node, in tree order, with this role and name.
    fn accessible_key(&self, role: &str, name: &str) -> Option<NodeKey> {
        let mut found = None;
        self.scene.accessibility_tree().walk(|key, _, d| {
            if found.is_none() && role_word(d.role) == role && d.name == name {
                found = Some(key);
            }
        });
        found
    }

    /// Where the first node, in tree order, with this role and name is.
    fn accessible_centre(&self, role: &str, name: &str) -> Option<(f64, f64)> {
        let mut found = None;
        self.scene.accessibility_tree().walk(|_, _, d| {
            if found.is_none() && role_word(d.role) == role && d.name == name {
                found = Some(d.bounds.center());
            }
        });
        found
    }

    fn grab(&mut self) -> Result<Vec<u8>> {
        self.settle()?;
        let out = self.scene.frame(&self.engine, self.clock.now());
        let mut r = VelloCpuReference::new(RasterConfig::default());
        let image = r
            .render(&FrameView::new(&out.snapshot, 1.0))
            .map_err(failed)?;
        image.to_png().map_err(failed)
    }
}

impl Backend for LjosBackend {
    fn name(&self) -> &'static str {
        "ljos"
    }

    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()> {
        if self.live.is_none() {
            let live = Self::start()?;
            for catalog in self.catalogs.drain(..) {
                live.engine.install_catalog(Box::new(catalog));
            }
            let images = ljos_quick::ImageStore::of(&live.engine);
            for (url, bytes) in self.resources.drain(..) {
                images.register_resource(url, bytes);
            }
            self.live = Some(live);
            // Before the scenario's first object: a reload knows only
            // what was made while it was noted.
            if self.watching
                && let Some(live) = self.live.as_mut()
            {
                reload::Rig::install(live);
            }
        }
        self.live()?.load(files, entry)
    }

    fn set(&mut self, object: &str, property: &str, value: &Value) -> Result<()> {
        let live = self.live()?;
        let obj = live.find(object)?;
        let (owner, prop) = live.owner(obj, property)?;
        let v = live.to_qml(value)?;
        let (prop, v) = live.member_write(owner, prop, v);
        live.engine
            .write_by_name(owner, &prop, v)
            .map_err(|e| failed(format!("cannot set {property}: {e}")))?;
        live.settle()
    }

    fn bind(&mut self, object: &str, property: &str, expression: &str) -> Result<()> {
        let live = self.live()?;
        let obj = live.find(object)?;
        // Sloppy mode, so `with` can put the object's properties in scope,
        // exactly as a binding written inside the object would see them.
        // The host evaluates scripts in strict mode, so the sloppy function
        // comes from the `Function` constructor.
        let body = format!("return function () {{ with (self) {{ return ({expression}); }} }};");
        let body = Value::String(body).to_string();
        let maker = live
            .engine
            .eval_js(&format!("new Function(\"self\", {body})"))
            .map_err(|e| failed(format!("cannot compile {expression:?}: {e}")))?;
        live.engine
            .invoke_by_name(
                live.driver,
                "bindWith",
                &[QmlValue::Object(obj), QmlValue::string(property), maker],
            )
            .map_err(failed)?;
        live.settle()
    }

    fn get(&mut self, object: &str, property: &str) -> Result<Value> {
        let live = self.live()?;
        let obj = live.find(object)?;
        let (owner, prop) = live.owner(obj, property)?;
        if live.engine.property_index(owner, &prop).is_err()
            && let Some((head, members)) = prop.split_once('.')
        {
            // A member of a value type (`childrenRect.x`, `font.pixelSize`):
            // the value is a copy, so read it whole and pick the member out.
            let whole = live.to_json(&live.engine.read_by_name(owner, head));
            return Ok(members
                .split('.')
                .try_fold(whole, |v, m| match (&v, m) {
                    // A list has a length, as it has in a binding.
                    (Value::Array(items), "length") => Some(Value::from(items.len())),
                    _ => v.get(m).cloned(),
                })
                .unwrap_or(Value::Null));
        }
        let v = live.engine.read_by_name(owner, &prop);
        Ok(live.to_json(&v))
    }

    fn call(&mut self, object: &str, method: &str, args: &[Value]) -> Result<Value> {
        let live = self.live()?;
        let obj = live.find(object)?;
        let args = args
            .iter()
            .map(|a| live.to_qml(a))
            .collect::<Result<Vec<_>>>()?;
        let out = live
            .engine
            .invoke_by_name(obj, method, &args)
            .map_err(|e| failed(format!("cannot call {method}: {e}")))?;
        let out = live.to_json(&out);
        live.settle()?;
        Ok(out)
    }

    fn exists(&mut self, object: &str) -> Result<bool> {
        let live = self.live()?;
        Ok(live.find(object).is_ok())
    }

    fn advance(&mut self, ms: u64) -> Result<()> {
        self.live()?.advance(ms)
    }

    fn click(&mut self, role: &str, name: &str) -> Result<()> {
        let live = self.live()?;
        live.listen()?;
        // A user's click: a pointer in the middle of the control, so
        // whatever covers it there takes the click, as on screen.
        let (x, y) = live
            .accessible_centre(role, name)
            .ok_or_else(|| failed(format!("no {role:?} named {name:?}")))?;
        live.tap(x, y)
    }

    fn key(&mut self, chord: &str) -> Result<()> {
        let chord = Chord::parse(chord).map_err(failed)?;
        self.live()?.type_chord(&chord, 0)
    }

    fn key_hold(&mut self, chord: &str, repeats: u32) -> Result<()> {
        let chord = Chord::parse(chord).map_err(failed)?;
        self.live()?.type_chord(&chord, repeats)
    }

    fn window_active(&mut self, active: bool) -> Result<()> {
        let live = self.live()?;
        live.scene.set_window_active(&live.engine, active);
        live.settle()
    }

    fn touch(&mut self, x: f64, y: f64) -> Result<()> {
        self.live()?.tap(x, y)
    }

    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()> {
        self.live()?.pointer(step, x, y)
    }

    fn wheel(&mut self, at: (f64, f64), by: super::WheelBy) -> Result<()> {
        self.live()?.wheel(at, by)
    }

    fn type_text(&mut self, text: &str) -> Result<()> {
        self.live()?.type_text(text)
    }

    fn double_click(&mut self, x: f64, y: f64) -> Result<()> {
        let live = self.live()?;
        live.tap(x, y)?;
        live.tap(x, y)
    }

    fn drag(&mut self, from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.live()?.drag(from, to)
    }

    fn set_clipboard(&mut self, text: &str) -> Result<()> {
        let live = self.live()?;
        ljos_quick::Clipboard::of(&live.engine).set_text(&live.engine, text);
        live.settle()
    }

    fn clipboard(&mut self) -> Result<String> {
        let live = self.live()?;
        Ok(ljos_quick::Clipboard::of(&live.engine).text())
    }

    fn compose(&mut self, text: &str) -> Result<()> {
        let live = self.live()?;
        // With no editor in focus the input method has nowhere to compose;
        // the scenario says what it expects of that.
        live.scene.ime_preedit(&live.engine, text, None);
        live.settle()
    }

    fn commit(&mut self, text: &str) -> Result<()> {
        let live = self.live()?;
        live.scene.ime_commit(&live.engine, text);
        live.settle()
    }

    fn a11y_tree(&mut self) -> Result<A11yNode> {
        let live = self.live()?;
        live.listen()?;
        Ok(live.a11y())
    }

    fn a11y_focus(&mut self, role: &str, name: &str) -> Result<()> {
        let live = self.live()?;
        live.listen()?;
        let key = live
            .accessible_key(role, name)
            .ok_or_else(|| failed(format!("no {role:?} named {name:?}")))?;
        // A node may refuse; the scenario says what it expects of that.
        live.scene.accessible_focus(&live.engine, key);
        live.settle()
    }

    fn a11y_act(&mut self, role: &str, name: &str, action: super::A11yAction) -> Result<()> {
        use ljos_quick::AccessibleAction as To;
        let live = self.live()?;
        live.listen()?;
        let key = live
            .accessible_key(role, name)
            .ok_or_else(|| failed(format!("no {role:?} named {name:?}")))?;
        let action = match action {
            super::A11yAction::Increase => To::Increase,
            super::A11yAction::Decrease => To::Decrease,
            super::A11yAction::SetValue(to) => To::SetValue(to),
            super::A11yAction::Scroll { dx, dy, page } => match (dx, dy) {
                (0, 1..) => To::ScrollDown { page },
                (0, _) => To::ScrollUp { page },
                (1.., _) => To::ScrollRight { page },
                _ => To::ScrollLeft { page },
            },
        };
        // A node may refuse; the scenario says what it expects of that.
        live.scene.accessible_action(&live.engine, key, action);
        live.settle()
    }

    fn a11y_text(&mut self, role: &str, name: &str, action: super::A11yTextAction) -> Result<()> {
        use ljos_quick::AccessibleTextAction as To;
        let live = self.live()?;
        live.listen()?;
        let key = live
            .accessible_key(role, name)
            .ok_or_else(|| failed(format!("no {role:?} named {name:?}")))?;
        let value = live
            .scene
            .accessibility_tree()
            .get(key)
            .and_then(|d| d.text.as_ref())
            .map(|t| t.value.clone())
            .unwrap_or_default();
        // The scenario counts characters; the tree counts bytes.
        let byte = |n: usize| {
            value
                .char_indices()
                .nth(n)
                .map_or(value.len(), |(at, _)| at)
        };
        let action = match action {
            super::A11yTextAction::Select(from, to) => To::Select {
                anchor: byte(from),
                cursor: byte(to),
            },
            super::A11yTextAction::ReplaceSelection(text) => To::ReplaceSelection(text),
            super::A11yTextAction::SetValue(text) => To::SetValue(text),
        };
        // A node may refuse; the scenario says what it expects of that.
        live.scene.accessible_text_action(&live.engine, key, action);
        live.settle()
    }

    fn log(&mut self) -> Result<Vec<LogEntry>> {
        // A load that failed before anything was made has an empty log.
        Ok(self
            .live
            .as_ref()
            .map(|l| l.log.borrow().clone())
            .unwrap_or_default())
    }

    fn grab(&mut self) -> Result<Vec<u8>> {
        self.live()?.grab()
    }

    // The `.ts` file is read as it is, the way a development build reads
    // one; a shipped application would have compiled it to `.ljl`.
    fn translation(&mut self, name: &str, ts: &str) -> Result<()> {
        let catalog = ljos_i18n::ts::TsFile::read(ts)
            .map_err(|e| failed(format!("{name}: {e}")))?
            .catalog();
        match self.live.as_mut() {
            Some(live) => {
                live.engine.install_catalog(Box::new(catalog));
                live.settle()
            }
            None => {
                self.catalogs.push(catalog);
                Ok(())
            }
        }
    }

    fn set_ui_language(&mut self, language: &str) -> Result<()> {
        let live = self.live()?;
        live.engine.set_ui_language(language);
        live.settle()
    }

    // Scenario files live at `file:///<name>`, so a picture beside them
    // does too; nothing is written to disk.
    fn resource(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let url = format!("file:///{name}");
        match self.live.as_ref() {
            Some(live) => {
                ljos_quick::ImageStore::of(&live.engine).register_resource(url, bytes.to_vec());
            }
            None => self.resources.push((url, bytes.to_vec())),
        }
        Ok(())
    }

    fn watch_files(&mut self) {
        self.watching = true;
    }

    fn change_file(&mut self, name: &str, source: Option<&str>) -> Result<()> {
        self.live()?.change_file(name, source)
    }

    fn reload(&mut self) -> Result<()> {
        self.live()?.reload_flush()
    }

    fn reload_error(&mut self) -> Result<Option<super::ReloadFailure>> {
        let fault = self.live()?.engine.last_reload_error();
        Ok(fault.map(|f| super::ReloadFailure {
            file: f.file,
            line: f.line,
            message: f.message,
        }))
    }

    fn reload_count(&mut self) -> Result<u64> {
        self.live()?.reload_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, source: &str) -> QmlFile {
        QmlFile {
            name: name.into(),
            source: source.into(),
        }
    }

    fn loaded(source: &str) -> LjosBackend {
        let mut b = LjosBackend::new();
        b.load(&[file("Main.qml", source)], "Main.qml")
            .expect("Main.qml loads");
        b
    }

    const BINDING: &str = r#"import QtQuick
Item {
    objectName: "root"
    property int a: 1
    property int b: a + 1
    function twice(n) { return n * 2 }
    Item { objectName: "kid"; width: parent.width / 2 }
}
"#;

    #[test]
    fn nothing_works_before_a_load() {
        let mut b = LjosBackend::new();
        assert!(matches!(b.get("root", "a"), Err(BackendError::Failed(_))));
        assert!(b.log().unwrap().is_empty());
    }

    #[test]
    fn bindings_follow_and_break() {
        let mut b = loaded(BINDING);
        assert_eq!(b.get("root", "b").unwrap(), json!(2));
        b.set("root", "a", &json!(5)).unwrap();
        assert_eq!(b.get("root", "b").unwrap(), json!(6));
        b.set("root", "b", &json!(100)).unwrap();
        b.set("root", "a", &json!(8)).unwrap();
        assert_eq!(b.get("root", "b").unwrap(), json!(100));
    }

    #[test]
    fn bind_installs_a_binding_over_the_objects_properties() {
        let mut b = loaded(BINDING);
        b.bind("root", "b", "a * 10").unwrap();
        b.set("root", "a", &json!(3)).unwrap();
        assert_eq!(b.get("root", "b").unwrap(), json!(30));
    }

    #[test]
    fn methods_are_called_with_arguments() {
        let mut b = loaded(BINDING);
        assert_eq!(b.call("root", "twice", &[json!(21)]).unwrap(), json!(42));
    }

    #[test]
    fn roots_are_children_of_a_400_pixel_driver() {
        let mut b = loaded(BINDING);
        assert_eq!(b.get("root", "parent.width").unwrap(), json!(400));
        assert!(b.exists("root/kid").unwrap());
        assert!(!b.exists("root/nobody").unwrap());
    }

    #[test]
    fn unknown_properties_are_rejected() {
        let mut b = loaded(BINDING);
        assert!(matches!(
            b.set("root", "nothing", &json!(1)),
            Err(BackendError::Failed(_))
        ));
    }

    #[test]
    fn syntax_errors_reject_the_load() {
        let mut b = LjosBackend::new();
        let r = b.load(
            &[file(
                "Broken.qml",
                "import QtQuick\nItem { property int p: }\n",
            )],
            "Broken.qml",
        );
        assert!(matches!(r, Err(BackendError::Failed(_))), "{r:?}");
    }

    #[test]
    fn colours_read_as_lower_case_hex() {
        let mut b = loaded("import QtQuick\nRectangle { objectName: \"r\"; color: \"#0000FF\" }\n");
        assert_eq!(b.get("r", "color").unwrap(), json!("#0000ff"));
    }

    #[test]
    fn members_of_value_types_are_read_from_a_copy() {
        let mut b = loaded(
            r#"import QtQuick
Item {
    objectName: "root"
    property rect area: Qt.rect(1, 2, 30, 40)
}
"#,
        );
        assert_eq!(b.get("root", "area.width").unwrap(), json!(30));
        assert_eq!(b.get("root", "area.nothing").unwrap(), Value::Null);
    }

    #[test]
    fn time_is_virtual_and_fires_timers_at_their_deadlines() {
        let mut b = loaded(
            r#"import QtQuick
Item {
    objectName: "root"
    property int ticks: 0
    Timer { interval: 100; repeat: true; running: true; onTriggered: root.ticks++ }
    id: root
}
"#,
        );
        b.advance(350).unwrap();
        assert_eq!(b.get("root", "ticks").unwrap(), json!(3));
    }

    #[test]
    fn binding_loops_reach_the_log_as_warnings() {
        let mut b =
            loaded("import QtQuick\nItem { property int p: q + 1; property int q: p + 1 }\n");
        let log = b.log().unwrap();
        assert!(
            log.iter().any(
                |e| e.level == LogLevel::Warning && e.message.contains("Binding loop detected")
            ),
            "{log:?}"
        );
    }

    #[test]
    fn touches_reach_mouse_areas() {
        // The MouseArea is the root, so the test does not depend on how
        // declared children join the item tree.
        let mut b = loaded(
            r#"import QtQuick
MouseArea {
    objectName: "root"
    width: 100; height: 100
    property int clicks: 0
    onClicked: clicks++
}
"#,
        );
        b.touch(10.0, 10.0).unwrap();
        b.touch(150.0, 150.0).unwrap();
        assert_eq!(b.get("root", "clicks").unwrap(), json!(1));
    }

    #[test]
    fn grab_renders_the_window() {
        let mut b =
            loaded("import QtQuick\nRectangle { width: 10; height: 10; color: \"#ff0000\" }\n");
        let png = b.grab().unwrap();
        let image = ljos_render::RgbaImage::from_png(&png).unwrap();
        assert_eq!((image.width, image.height), (400, 400));
        assert_eq!(image.pixel(5, 5), [255, 0, 0, 255]);
        assert_eq!(image.pixel(50, 50)[3], 0);
    }

    const TOOLS: &str = r#"import QtQuick
Item {
    objectName: "root"
    width: 200; height: 100
    property int clicks: 0
    Item {
        objectName: "tools"
        x: 10; y: 10; width: 180; height: 40
        Accessible.role: Accessible.ToolBar
        Accessible.name: "Tools"
        Rectangle {
            objectName: "save"
            x: 5; y: 5; width: 80; height: 30
            Accessible.role: Accessible.Button
            Accessible.name: "Save"
            Accessible.description: "Writes the file"
            MouseArea { anchors.fill: parent; onClicked: root.clicks++ }
        }
    }
    id: root
}
"#;

    #[test]
    fn the_accessibility_tree_has_roles_names_and_nesting() {
        let mut b = loaded(TOOLS);
        let tree = b.a11y_tree().unwrap();
        assert_eq!(tree.role, "window");
        let save = tree.find("button", "Save").expect("the button");
        assert_eq!(save.object_name, "save");
        assert_eq!(save.description, "Writes the file");
        assert!(
            tree.parent_of("button", "Save")
                .unwrap()
                .is("toolbar", "Tools")
        );
        // The plain root item is not a node.
        assert_eq!(tree.children.len(), 1);
    }

    #[test]
    fn attached_properties_are_set_and_read_by_their_type_name() {
        let mut b = loaded(TOOLS);
        assert_eq!(b.get("save", "Accessible.name").unwrap(), json!("Save"));
        b.set("save", "Accessible.name", &json!("Store")).unwrap();
        let tree = b.a11y_tree().unwrap();
        assert!(tree.find("button", "Store").is_some());
        assert!(tree.find("button", "Save").is_none());
        // Once read, the tree follows later changes too.
        b.set("save", "Accessible.ignored", &json!(true)).unwrap();
        assert!(b.a11y_tree().unwrap().find("button", "Store").is_none());
    }

    #[test]
    fn clicks_find_the_control_by_role_and_name() {
        let mut b = loaded(TOOLS);
        b.click("button", "Save").unwrap();
        assert_eq!(b.get("root", "clicks").unwrap(), json!(1));
        assert!(matches!(
            b.click("button", "Open"),
            Err(BackendError::Failed(_))
        ));
    }

    #[test]
    fn chords_name_modifiers_then_a_key() {
        let c = Chord::parse("Ctrl+Shift+S").unwrap();
        assert_eq!(c.modifiers, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!((c.key, c.text.as_str()), (0x53, ""));
        let c = Chord::parse("a").unwrap();
        assert_eq!((c.key, c.text.as_str()), (key::A, "a"));
        let c = Chord::parse("Shift+A").unwrap();
        assert_eq!((c.key, c.text.as_str()), (key::A, "A"));
        let c = Chord::parse("Ctrl++").unwrap();
        assert_eq!((c.modifiers, c.key), (Modifiers::CONTROL, key::PLUS));
        assert_eq!(Chord::parse("+").unwrap().key, key::PLUS);
        assert_eq!(Chord::parse("Shift+Tab").unwrap().key, key::BACKTAB);
        assert_eq!(Chord::parse("PageUp").unwrap().key, key::PAGE_UP);
        assert_eq!(Chord::parse("Space").unwrap().text, " ");
        assert_eq!(Chord::parse("F12").unwrap().text, "");
        assert!(Chord::parse("Hyper+A").is_err());
        assert!(Chord::parse("Nonsense").is_err());
    }

    const KEYS: &str = r#"import QtQuick
Item {
    objectName: "root"
    focus: true
    property string log: ""
    property bool lit: activeFocus
    Keys.onPressed: (event) => log += event.key + (event.isAutoRepeat ? "r " : " ")
    Keys.onReleased: (event) => log += "up "
}
"#;

    #[test]
    fn keys_reach_the_focus_item() {
        let mut b = loaded(KEYS);
        b.key("A").unwrap();
        assert_eq!(b.get("root", "log").unwrap(), json!("65 up "));
        b.key_hold("B", 2).unwrap();
        assert_eq!(b.get("root", "log").unwrap(), json!("65 up 66 66r 66r up "));
    }

    #[test]
    fn an_inactive_window_gets_no_keys() {
        let mut b = loaded(KEYS);
        assert_eq!(b.get("root", "lit").unwrap(), json!(true));
        b.window_active(false).unwrap();
        assert_eq!(b.get("root", "lit").unwrap(), json!(false));
        b.key("A").unwrap();
        assert_eq!(b.get("root", "log").unwrap(), json!(""));
        b.window_active(true).unwrap();
        b.key("A").unwrap();
        assert_eq!(b.get("root", "log").unwrap(), json!("65 up "));
    }
}
