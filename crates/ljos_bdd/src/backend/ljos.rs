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

use ljos_compiler::{Project, Severity, compile_file};
use ljos_engine::{Engine, MapSource, ObjectId, Value as QmlValue};
use ljos_quick::{PointerEvent, QuickScene};
use ljos_render::{FrameView, RasterConfig, VelloCpuReference, kurbo};
use ljos_runtime::{Clock as _, VirtualClock};
use qml_front::QtTypeOracle;
use serde_json::{Map, Number, Value, json};

use super::{A11yNode, Backend, BackendError, LogEntry, LogLevel, QmlFile, Result};

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
}

#[derive(Default)]
pub struct LjosBackend {
    live: Option<Live>,
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
        };
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
            let out = self.scene.frame(&self.engine, self.clock.now());
            if !self.scene.needs_frame() && out.next_wake != Some(self.clock.now()) {
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
            let source = compile(&wanted)?;
            self.engine.set_component_source(source);
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
    /// object is `bar` of that object.
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
            match self.engine.read_by_name(obj, head).as_object() {
                Some(next) => {
                    obj = next;
                    rest = tail;
                }
                None => return Ok((obj, rest.to_owned())),
            }
        }
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
            QmlValue::Font(_) | QmlValue::Js(_) | QmlValue::Object(_) => self.js_json(v),
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
        let end = self.clock.now() + Duration::from_millis(ms);
        // Each timer fires at its own deadline, and the scene is settled in
        // between, as a host woken at each deadline would.
        for _ in 0..MAX_TIMER_STEPS {
            match self.engine.next_timer_deadline() {
                Some(t) if t <= end => {
                    if t > self.clock.now() {
                        self.clock.set(t);
                    }
                    self.engine.advance_timers(self.clock.now());
                    self.settle()?;
                }
                _ => {
                    self.clock.set(end);
                    self.engine.advance_timers(end);
                    return self.settle();
                }
            }
        }
        Err(failed(format!(
            "timers fired more than {MAX_TIMER_STEPS} times in {ms} ms"
        )))
    }

    fn tap(&mut self, x: f64, y: f64) -> Result<()> {
        let now = self.clock.now();
        self.scene
            .pointer_event(&self.engine, &PointerEvent::press(x, y, now));
        self.scene
            .pointer_event(&self.engine, &PointerEvent::release(x, y, now));
        self.settle()
    }

    /// The first item, depth first under the roots, whose accessible role
    /// and name match.
    fn find_accessible(&self, role: &str, name: &str) -> Option<ObjectId> {
        let e = &self.engine;
        let mut stack: Vec<ObjectId> = self.roots.iter().rev().copied().collect();
        while let Some(o) = stack.pop() {
            if !e.is_alive(o) {
                continue;
            }
            if let Ok(info) = ljos_quick::accessible(e, o)
                && info
                    .role
                    .as_ref()
                    .is_some_and(|r| r.as_str().eq_ignore_ascii_case(role))
                && info.name.as_ref().is_some_and(|n| n.as_str() == name)
            {
                return Some(o);
            }
            let mut kids = e.children(o);
            kids.reverse();
            stack.extend(kids);
        }
        None
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
            self.live = Some(Self::start()?);
        }
        self.live()?.load(files, entry)
    }

    fn set(&mut self, object: &str, property: &str, value: &Value) -> Result<()> {
        let live = self.live()?;
        let obj = live.find(object)?;
        let (owner, prop) = live.owner(obj, property)?;
        let v = live.to_qml(value)?;
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
                .try_fold(whole, |v, m| v.get(m).cloned())
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
        let target = live
            .find_accessible(role, name)
            .ok_or_else(|| failed(format!("no {role:?} named {name:?}")))?;
        let w = live.engine.read_by_name(target, "width").to_number();
        let h = live.engine.read_by_name(target, "height").to_number();
        let centre =
            ljos_quick::scene_transform(&live.engine, target) * kurbo::Point::new(w / 2.0, h / 2.0);
        live.tap(centre.x, centre.y)
    }

    fn key(&mut self, _: &str) -> Result<()> {
        Err(BackendError::NotImplemented("key input"))
    }

    fn touch(&mut self, x: f64, y: f64) -> Result<()> {
        self.live()?.tap(x, y)
    }

    fn a11y_tree(&mut self) -> Result<A11yNode> {
        Err(BackendError::NotImplemented("the accessibility tree"))
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

    #[test]
    fn unbuilt_capabilities_stay_pending() {
        let mut b = loaded(BINDING);
        assert!(matches!(
            b.a11y_tree(),
            Err(BackendError::NotImplemented(_))
        ));
        assert!(matches!(
            b.key("Ctrl+S"),
            Err(BackendError::NotImplemented(_))
        ));
    }
}
