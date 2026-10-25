//! The tools that act, run on the UI thread.
//!
//! **No back door.** Input is made into the events a device sends
//! (`PointerEvent`, `KeyEvent`, `WheelEvent`) and given to the scene's
//! router, so a click lands on what is on top, a key goes to the item with
//! focus and a field gets its text through the text-input path, exactly as
//! for a person. A property is assigned and a method called through the
//! engine the way a script does it; script runs in the engine's JS host
//! with a time limit.
//!
//! Every act answers once the frame has settled, with what changed: the
//! difference between the outline before the act and the one after it.

use std::rc::Rc;
use std::time::{Duration, Instant};

use ljos_drive::{
    Chord, DriveError, DriveState, Driver, MAX_TIMER_STEPS, RealClock, Refusal, SceneClock, Wheel,
};
use ljos_engine::{
    Engine, EngineError, ObjectId, OverrideContent, PropIndex, RestoreMode, TypeTag,
    Value as QmlValue,
};
use ljos_mcp_core::{ErrorCode, RefTable, SurfaceError, ToolReply, delta};
use ljos_quick::{MouseButtons, QuickScene};
use ljos_render::kurbo::{self, Point};
use ljos_runtime::Clock;
use serde_json::{Map, Value, json};

use super::args::{
    Act, AdvanceArgs, AppearanceArgs, Button, ClickArgs, EvalArgs, InvokeArgs, PressArgs,
    ResizeArgs, Scheme, ScrollArgs, SetPropertyArgs, TouchAction, TouchArgs, TypeArgs,
};
use super::capture::{self, Capture, REDACTED};
use super::edits::{Edit, Edits};
use super::redact::{mask_data, mask_error, mask_json, mask_text};
use super::surface::Answer;
use super::tools::{Look, TYPED, Target, clip, flag, num, visible_to_the_top, window_box};
use super::values::number;
use super::{Inspector, MAX_DELTA_LINES, View};

/// Chords one `app_press` may name.
const MAX_CHORDS: usize = 16;
/// The time a script may run unless asked for less or more.
const DEFAULT_EVAL: Duration = Duration::from_millis(1000);

// ----- keys ------------------------------------------------------------------

/// The chords of one `app_press`, in order (`Ctrl+Shift+S Tab`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Chords(Vec<Chord>);

impl Chords {
    /// Reads `keys`: chords separated by spaces. An agent's `Enter` is the
    /// Return key a desktop sends for it, which `Keys.onReturnPressed`
    /// hears; the driver reads `Enter` as the keypad's, as Qt names it.
    pub(super) fn parse(keys: &str) -> Result<Chords, String> {
        let chords: Vec<Chord> = keys
            .split_whitespace()
            .map(|text| Chord::parse(&return_for_enter(text)))
            .collect::<Result<_, _>>()?;
        if chords.is_empty() {
            return Err("`keys` names no key; give `Enter`, `Tab`, `Ctrl+A`, …".into());
        }
        if chords.len() > MAX_CHORDS {
            return Err(format!(
                "`keys` names {} keys and one call presses at most {MAX_CHORDS}",
                chords.len()
            ));
        }
        Ok(Chords(chords))
    }
}

/// `text` with a key named `Enter` named `Return`.
fn return_for_enter(text: &str) -> String {
    let (mods, name) = match text.rsplit_once('+') {
        Some((mods, name)) if !name.is_empty() => (Some(mods), name),
        _ => (None, text),
    };
    if !name.eq_ignore_ascii_case("Enter") {
        return text.to_owned();
    }
    match mods {
        Some(mods) => format!("{mods}+Return"),
        None => "Return".to_owned(),
    }
}

// ----- errors ----------------------------------------------------------------

fn stale(what: &str) -> SurfaceError {
    SurfaceError::new(
        ErrorCode::StaleRef,
        format!(
            "`{what}` is gone: the object was destroyed. Take a new `app_snapshot` for current \
             references"
        ),
    )
}

fn not_actionable(why: impl Into<String>) -> SurfaceError {
    SurfaceError::new(ErrorCode::NotActionable, why)
}

/// The newest fault on record and whether faults wait: what to compare with
/// after a call to see whether the call added one.
type FaultMark = (bool, Option<(u64, String)>);

fn last_fault(engine: &Engine) -> FaultMark {
    let last = engine
        .faults()
        .last()
        .map(|f| (f.line.into(), f.fault.to_string()));
    (engine.has_pending_faults(), last)
}

/// The fault a call added since `before`, as text.
fn new_fault(engine: &Engine, before: FaultMark) -> Option<String> {
    let (was_pending, was_last) = before;
    let (pending, last) = last_fault(engine);
    // The same text twice in a row is still new when it queued up again.
    ((pending && !was_pending) || last != was_last)
        .then(|| last.map(|(_, text)| text))
        .flatten()
}

/// How an engine failure is told to an agent.
fn engine_error(what: &str, e: EngineError) -> SurfaceError {
    match e {
        EngineError::DeadObject(_) => stale(what),
        EngineError::UnknownProperty { .. }
        | EngineError::UnknownSignal { .. }
        | EngineError::UnknownMethod { .. } => SurfaceError::not_found(e.to_string()),
        EngineError::ReadOnly(_) | EngineError::GroupProperty(_) => not_actionable(e.to_string()),
        EngineError::Convert(_) | EngineError::ArgumentCount { .. } => {
            SurfaceError::invalid_argument(e.to_string())
        }
        // A method that throws is the application's fault, not the agent's.
        other => SurfaceError::new(ErrorCode::AppFaulted, other.to_string()),
    }
}

// ----- values ----------------------------------------------------------------

/// Text given for a property that holds an enum, as an agent reads it back:
/// `Text.AlignRight`, `AlignRight`, `Qt.LeftButton | Qt.RightButton`, or a
/// number. The engine takes a key without its type, as a QML string
/// assignment does, so the type in front is dropped here, and a number
/// written as text is the number.
fn enum_input(engine: &Engine, obj: ObjectId, prop: PropIndex, v: QmlValue) -> QmlValue {
    let QmlValue::String(text) = &v else {
        return v;
    };
    let is_enum = engine.meta_of(obj).is_some_and(|m| {
        m.property(prop)
            .is_some_and(|p| matches!(p.ty, TypeTag::Enum(_) | TypeTag::Flags(_)))
    });
    if !is_enum {
        return v;
    }
    if let Ok(n) = text.trim().parse::<i32>() {
        return QmlValue::Int(n);
    }
    let keys: Vec<&str> = text
        .split('|')
        .map(|part| {
            let part = part.trim();
            part.rsplit_once('.').map_or(part, |(_, key)| key)
        })
        .collect();
    QmlValue::string(keys.join("|").as_str())
}

/// A JSON value as the engine holds one: a whole number that fits is an
/// `int`, an array a list, and an object made by JS so that it is one.
fn to_qml(engine: &Engine, v: &Value) -> Result<QmlValue, SurfaceError> {
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
                .map(|i| to_qml(engine, i))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(_) => engine
            .eval_js(&format!("({v})"))
            .map_err(|e| SurfaceError::invalid_argument(format!("that value is no object: {e}")))?,
    })
}

/// An engine value as JSON: what a script's `JSON.stringify` would show for
/// the plain kinds, and the text JS gives for the rest.
pub(crate) fn to_json(engine: &Engine, v: &QmlValue, depth: u8) -> Value {
    match v {
        QmlValue::Undefined | QmlValue::Null => Value::Null,
        QmlValue::Bool(b) => json!(b),
        QmlValue::Int(i) => json!(i),
        QmlValue::Real(r) => number(engine, *r),
        QmlValue::String(s) | QmlValue::Url(s) => json!(s.to_string()),
        QmlValue::Color(c) => json!(c.to_string()),
        QmlValue::Enum(e) => json!(e.value),
        QmlValue::Point(p) => json!({ "x": number(engine, p.x), "y": number(engine, p.y) }),
        QmlValue::Size(s) => {
            json!({ "width": number(engine, s.width), "height": number(engine, s.height) })
        }
        QmlValue::Rect(r) => json!({
            "x": number(engine, r.x), "y": number(engine, r.y),
            "width": number(engine, r.width), "height": number(engine, r.height)
        }),
        QmlValue::List(items) if depth < 4 => Value::Array(
            items
                .iter()
                .take(100)
                .map(|i| to_json(engine, i, depth + 1))
                .collect(),
        ),
        other => {
            let text = engine
                .js_text(other)
                .unwrap_or_else(|| engine.to_js_string(other).to_string());
            json!(clip(&text, 200))
        }
    }
}

/// The secrets of every capture given, longest first. An act may have
/// changed or cleared a field, so what it held before is a secret too.
fn secrets_of_all(captures: &[&Capture]) -> Vec<String> {
    let mut all: Vec<String> = captures
        .iter()
        .flat_map(|c| c.secrets.iter().cloned())
        .collect();
    all.sort_unstable();
    all.dedup();
    all.sort_by_key(|s| std::cmp::Reverse(s.len()));
    all
}

/// Masks what a script, a method or a set answered, keys and all: it is
/// the application's, not the answer's own (a step of a batch too).
fn mask_answered(fields: &mut Map<String, Value>, secrets: &[String]) {
    for key in ["value", "previous"] {
        if let Some(answered) = fields.get_mut(key) {
            mask_data(answered, secrets);
        }
    }
    if let Some(Value::Array(steps)) = fields.get_mut("steps") {
        for step in steps {
            if let Some(Value::Object(result)) = step.get_mut("result") {
                mask_answered(result, secrets);
            }
        }
    }
}

fn secrets_of(capture: &Capture) -> Vec<String> {
    capture.masks()
}

// ----- the hands -------------------------------------------------------------

/// What an act reports about itself, before what changed is added.
struct Done {
    fields: Map<String, Value>,
    summary: String,
}

impl Done {
    fn new(summary: impl Into<String>) -> Self {
        Done {
            fields: Map::new(),
            summary: summary.into(),
        }
    }

    fn with(mut self, key: &str, value: Value) -> Self {
        self.fields.insert(key.to_string(), value);
        self
    }
}

/// What the acts work with while one request is run.
struct Hands<'a> {
    engine: &'a Engine,
    scene: &'a mut QuickScene,
    /// The test clock, when the application runs on one, and else the
    /// system's, which `app_advance` cannot move.
    clock: &'a dyn SceneClock,
    refs: &'a mut RefTable,
    edits: &'a mut Edits,
    redact_all: bool,
    /// The scene as it was after the last act; replaced by [`refresh`](Self::refresh).
    capture: Rc<Capture>,
    per_copy: usize,
    drive: DriveState,
    unsettled: bool,
}

impl Hands<'_> {
    fn driver(&mut self) -> Driver<'_> {
        Driver::new(self.engine, self.scene, self.clock, &mut self.drive)
    }

    /// What the driver did, for an agent: a scene that will not settle is
    /// reported with the answer, not failed.
    fn drove<R: Default>(&mut self, done: Result<R, DriveError>) -> Result<R, SurfaceError> {
        match done {
            Ok(r) => Ok(r),
            Err(DriveError::Unsettled { .. }) => {
                self.unsettled = true;
                Ok(R::default())
            }
            Err(DriveError::RealClock) => Err(SurfaceError::unavailable(
                "this application runs on the system clock, which only moves with time. \
                 `app_advance` is for an application on a virtual clock (a test); \
                 `app_status` says which clock there is",
            )),
            Err(DriveError::Timers { ms }) => Err(SurfaceError::new(
                ErrorCode::AppFaulted,
                format!(
                    "timers fired more than {MAX_TIMER_STEPS} times in {ms} ms; a timer with no \
                     interval never lets time pass"
                ),
            )),
            Err(e) => Err(SurfaceError::new(ErrorCode::Internal, e.to_string())),
        }
    }

    /// Runs frames until the scene is idle, as a host does before the user
    /// sees anything.
    fn settle(&mut self) {
        let done = self.driver().settle();
        let _ = self.drove(done);
    }

    /// Copies the scene again, so that the next act and the answer see what
    /// the last one did.
    fn refresh(&mut self) {
        let pointer = self.capture.pointer;
        self.capture = Rc::new(capture::capture(
            self.engine,
            self.scene,
            self.redact_all,
            pointer,
        ));
    }

    fn look(&mut self) -> Look<'_> {
        Look {
            engine: self.engine,
            scene: self.scene,
            capture: &self.capture,
            refs: self.refs,
        }
    }

    /// The object `what` names.
    fn object(&mut self, what: &str, tool: &str) -> Result<ObjectId, SurfaceError> {
        let target = self.look().resolve(what, tool)?;
        let obj = match target {
            Target::Node(index) => self.capture.items[index],
            Target::Unlisted(obj) => Some(obj),
        };
        match obj {
            Some(o) if self.engine.is_alive(o) => Ok(o),
            Some(_) => Err(stale(what)),
            None => Err(not_actionable(format!(
                "`{what}` is the window, which has no QML object of its own. Name an item in it"
            ))),
        }
    }

    /// The point in the window an input for `what` is aimed at: its centre,
    /// provided that it can be reached.
    fn aim(&mut self, what: &str, tool: &str) -> Result<Point, SurfaceError> {
        let target = self.look().resolve(what, tool)?;
        let engine = self.engine;
        let (bounds, obj, listed_disabled) = match target {
            Target::Node(index) => {
                let node = self.capture.node(index);
                (
                    node.bounds,
                    self.capture.items[index],
                    node.states.contains(&"disabled"),
                )
            }
            Target::Unlisted(obj) => {
                if !engine.is_alive(obj) {
                    return Err(stale(what));
                }
                if !engine.untrack(|| visible_to_the_top(engine, obj)) {
                    return Err(not_actionable(format!(
                        "`{what}` is not shown (it, or an item above it, has `visible: false`), \
                         so nothing can be clicked there"
                    )));
                }
                (engine.untrack(|| window_box(engine, obj)), Some(obj), false)
            }
        };
        let own_disabled =
            obj.is_some_and(|o| engine.untrack(|| flag(engine, o, "enabled")) == Some(false));
        let [x, y, w, h] = bounds;
        let bounds = kurbo::Rect::new(x, y, x + w, y + h);
        ljos_drive::aim(
            bounds,
            listed_disabled || own_disabled,
            self.scene.viewport(),
        )
        .map_err(|why| match why {
            Refusal::Disabled => not_actionable(format!(
                "`{what}` is disabled (`enabled: false`, or something above it is), so it \
                     takes no input. `app_set_property` can enable it"
            )),
            Refusal::Hidden => not_actionable(format!(
                "`{what}` is not shown (it, or an item above it, has `visible: false`), so \
                     nothing can be clicked there"
            )),
            Refusal::NoSize { width, height } => not_actionable(format!(
                "`{what}` has no size ({width}x{height}), so there is nowhere on it to put \
                     the pointer"
            )),
            Refusal::Outside { at, window } => not_actionable(format!(
                "`{what}` is outside the window: its centre is at {},{} in a window of \
                     {}x{}. Scroll it into view, or make the window larger with `app_resize`",
                num(at.x),
                num(at.y),
                num(window.width),
                num(window.height)
            )),
        })
    }

    /// A move, then `count` presses and releases of `button` at the point.
    fn click_at(
        &mut self,
        at: Point,
        button: MouseButtons,
        count: u32,
    ) -> Result<(), SurfaceError> {
        let done = self.driver().click(at, button, count);
        self.drove(done)
    }

    // ----- the acts -----

    fn click(&mut self, a: &ClickArgs) -> Result<Done, SurfaceError> {
        let at = self.aim(&a.target, "app_click")?;
        let button = match a.button {
            Button::Left => MouseButtons::LEFT,
            Button::Right => MouseButtons::RIGHT,
            Button::Middle => MouseButtons::MIDDLE,
        };
        let count = a.count.unwrap_or(1);
        self.click_at(at, button, count)?;
        Ok(Done::new(format!(
            "clicked `{}` at {},{}",
            a.target,
            num(at.x),
            num(at.y)
        ))
        .with("at", json!([num(at.x), num(at.y)])))
    }

    /// The item that has the keyboard, or why nothing does.
    fn keyboard(&mut self, tool: &str) -> Result<ObjectId, SurfaceError> {
        let engine = self.engine;
        let focus = self.scene.active_focus_item(engine);
        let nowhere = || {
            not_actionable(format!(
                "nothing has keyboard focus, so `{tool}` has nowhere to send keys. Give `target` \
                 to click an item first, or press `Tab`"
            ))
        };
        let obj = focus.ok_or_else(nowhere)?;
        // The scene's root holds the keyboard when no item asked for it: keys
        // go there and nothing listens.
        if Some(obj) == self.scene.root()
            && engine.untrack(|| flag(engine, obj, "focus")) != Some(true)
        {
            return Err(nowhere());
        }
        Ok(obj)
    }

    fn type_text(&mut self, a: &TypeArgs) -> Result<Done, SurfaceError> {
        if let Some(target) = &a.target {
            let at = self.aim(target, "app_type")?;
            self.click_at(at, MouseButtons::LEFT, 1)?;
        }
        let focus = self.keyboard("app_type")?;
        let hidden = capture::is_secret(self.engine, focus, self.redact_all);
        let done = self.driver().type_text(&a.text);
        self.drove(done)?;
        // What a field that hides its text was given is not said again.
        let typed = if hidden {
            REDACTED.to_string()
        } else {
            a.text.clone()
        };
        let summary = format!("typed {} characters", a.text.chars().count());
        Ok(Done::new(summary)
            .with("typed", json!(typed))
            .with("characters", json!(a.text.chars().count())))
    }

    fn press(&mut self, a: &PressArgs) -> Result<Done, SurfaceError> {
        let chords = Chords::parse(&a.keys).map_err(SurfaceError::invalid_argument)?;
        if let Some(target) = &a.target {
            let at = self.aim(target, "app_press")?;
            self.click_at(at, MouseButtons::LEFT, 1)?;
        }
        let repeat = a.repeat.unwrap_or(1);
        for _ in 0..repeat {
            for c in &chords.0 {
                let done = self.driver().chord(c, 0);
                self.drove(done)?;
            }
        }
        Ok(Done::new(format!("pressed {} × {repeat}", a.keys))
            .with("pressed", json!(a.keys))
            .with("repeat", json!(repeat)))
    }

    /// The middle of the window, or of `target`.
    fn spot(&mut self, target: &Option<String>, tool: &str) -> Result<Point, SurfaceError> {
        match target {
            Some(what) => self.aim(what, tool),
            None => {
                let view = self.scene.viewport();
                Ok(Point::new(view.width() / 2.0, view.height() / 2.0))
            }
        }
    }

    fn scroll(&mut self, a: &ScrollArgs) -> Result<Done, SurfaceError> {
        let at = self.spot(&a.target, "app_scroll")?;
        let (dx, dy) = (a.dx.unwrap_or(0.0), a.dy.unwrap_or(0.0));
        let done = self.driver().wheel(at, Wheel::Notches(dx, dy));
        self.drove(done)?;
        Ok(Done::new(format!(
            "turned the wheel by {},{} notches at {},{}",
            num(dx),
            num(dy),
            num(at.x),
            num(at.y)
        )))
    }

    fn touch(&mut self, a: &TouchArgs) -> Result<Done, SurfaceError> {
        if a.action == TouchAction::Pinch {
            return Err(SurfaceError::unavailable(
                "`pinch` is not supported yet: the scene routes one finger as a pointer with an \
                 identity and has no gesture of two fingers. `tap` and `swipe` are",
            ));
        }
        let at = self.spot(&a.target, "app_touch")?;
        let by = match a.action {
            TouchAction::Swipe => kurbo::Vec2::new(a.dx.unwrap_or(0.0), a.dy.unwrap_or(0.0)),
            _ => kurbo::Vec2::ZERO,
        };
        let done = self.driver().touch(at, by);
        self.drove(done)?;
        let what = if a.action == TouchAction::Swipe {
            "swiped"
        } else {
            "tapped"
        };
        Ok(
            Done::new(format!("{what} at {},{}", num(at.x), num(at.y))).with(
                "gesture",
                json!(if a.action == TouchAction::Swipe {
                    "swipe"
                } else {
                    "tap"
                }),
            ),
        )
    }

    fn advance(&mut self, a: &AdvanceArgs) -> Result<Done, SurfaceError> {
        let done = self.driver().advance(a.ms);
        self.drove(done)?;
        Ok(Done::new(format!("moved the clock by {} ms", a.ms)).with("advanced_ms", json!(a.ms)))
    }

    fn resize(&mut self, a: &ResizeArgs) -> Result<Done, SurfaceError> {
        self.scene.set_viewport(kurbo::Rect::new(
            0.0,
            0.0,
            f64::from(a.width),
            f64::from(a.height),
        ));
        if let Some(scale) = a.scale {
            self.scene.set_scale_factor(scale as f32);
        }
        Ok(
            Done::new(format!("resized the window to {}x{}", a.width, a.height))
                .with("width", json!(a.width))
                .with("height", json!(a.height))
                .with("scale", num(f64::from(self.scene.scale_factor()))),
        )
    }

    fn appearance(&mut self, a: &AppearanceArgs) -> Result<Done, SurfaceError> {
        ljos_quick::update_system_prefs(self.engine, |p| {
            match a.color_scheme {
                Some(Scheme::Light) => p.color_scheme = ljos_a11y::ColorScheme::Light,
                Some(Scheme::Dark) => p.color_scheme = ljos_a11y::ColorScheme::Dark,
                None => {}
            }
            if let Some(r) = a.reduced_motion {
                p.reduced_motion = r;
            }
            if let Some(s) = a.font_scale {
                p.font_scale = s;
            }
        });
        let now = ljos_quick::system_prefs(self.engine);
        let scheme = match now.color_scheme {
            ljos_a11y::ColorScheme::Unknown => "unknown",
            ljos_a11y::ColorScheme::Light => "light",
            ljos_a11y::ColorScheme::Dark => "dark",
        };
        Ok(Done::new(format!(
            "appearance: {scheme} colour scheme, reduced motion {}, font scale {}",
            if now.reduced_motion { "on" } else { "off" },
            num(now.font_scale)
        ))
        .with(
            "appearance",
            json!({
                "color_scheme": scheme,
                "reduced_motion": now.reduced_motion,
                "font_scale": num(now.font_scale),
            }),
        ))
    }

    fn set_property(&mut self, a: &SetPropertyArgs) -> Result<Done, SurfaceError> {
        let obj = self.object(&a.target, "app_set_property")?;
        let engine = self.engine;
        if let Some(row) = a.row {
            let value = to_qml(engine, a.value.as_ref().unwrap_or(&Value::Null))?;
            let (before, now) =
                super::models::write_cell(engine, obj, &a.target, row, &a.property, &value)?;
            return Ok(Done::new(format!(
                "set `{}` of row {row} of `{}` to {now}",
                a.property, a.target
            ))
            .with("row", json!(row))
            .with("property", json!(a.property))
            .with("value", now)
            .with("previous", before));
        }
        if let Err(e) = engine.property_index(obj, &a.property) {
            return Err(match e {
                EngineError::UnknownProperty { .. } => SurfaceError::not_found(format!(
                    "{e}. `app_inspect` lists the properties of `{}`",
                    a.target
                )),
                other => engine_error(&a.target, other),
            });
        }
        let prop = engine
            .property_index(obj, &a.property)
            .map_err(|e| engine_error(&a.target, e))?;
        if a.revert {
            return self.revert_property(obj, prop, a);
        }
        if let Some(source) = &a.binding {
            self.may_run_script_in(obj, &a.target)?;
            // Refused before anything is taken down, so that an expression
            // that does not compile changes nothing.
            engine
                .check_expression(source)
                .map_err(|e| SurfaceError::invalid_argument(format!("`binding` {e}")))?;
        }
        let value = match &a.binding {
            Some(_) => QmlValue::Undefined,
            None => enum_input(
                engine,
                obj,
                prop,
                to_qml(engine, a.value.as_ref().unwrap_or(&Value::Null))?,
            ),
        };
        let previous = engine.read_by_name(obj, &a.property);
        let earlier = self.edits.take(obj, &a.property);
        let had = earlier.is_some();
        let mut cover = earlier.as_ref().and_then(|e| e.cover);
        let wrote = match &a.binding {
            Some(source) => {
                let covering = cover.is_some() || (a.keep_binding && engine.is_bound(obj, prop));
                if covering {
                    // The binding takes the place of the layer devtools put
                    // there before, so that a revert still goes under both.
                    let old = cover.take();
                    let gone = old.map_or(Ok(()), |id| {
                        engine.remove_override(obj, prop, id, RestoreMode::BindingOrValue)
                    });
                    gone.and_then(|()| engine.cover_with_expression(obj, prop, source))
                        .map(|id| cover = Some(id))
                } else {
                    engine.bind_expression(obj, prop, source)
                }
            }
            None if cover.is_none() && a.keep_binding && engine.is_bound(obj, prop) => engine
                .push_override(obj, prop, OverrideContent::Value(value))
                .map(|id| cover = Some(id)),
            // With a cover in place this writes the cover, which is the
            // top layer: the binding under it stays.
            None => engine.write_by_name(obj, &a.property, value),
        };
        let before = earlier.map_or_else(|| previous.clone(), |e| e.before);
        let expression = a.binding.clone();
        if let Err(e) = wrote {
            if had {
                self.edits.put(
                    obj,
                    &a.property,
                    Edit {
                        before,
                        cover,
                        expression: None,
                    },
                );
            }
            return Err(engine_error(&a.target, e));
        }
        self.edits.put(
            obj,
            &a.property,
            Edit {
                before,
                cover,
                expression,
            },
        );
        let now = engine.read_by_name(obj, &a.property);
        // What is typed into a field that hides it is not said back.
        let hidden = TYPED.contains(&a.property.as_str())
            && capture::is_secret(engine, obj, self.redact_all);
        let show = |v: &QmlValue| {
            if hidden {
                json!(REDACTED)
            } else {
                to_json(engine, v, 0)
            }
        };
        let done = match (cover.is_some(), &a.binding) {
            (true, Some(source)) => format!(
                "covered the binding of `{}` of `{}` with `{source}`",
                a.property, a.target
            ),
            (false, Some(source)) => {
                format!("bound `{}` of `{}` to `{source}`", a.property, a.target)
            }
            (true, None) => format!("covered the binding of `{}` of `{}`", a.property, a.target),
            (false, None) => format!("assigned `{}` of `{}`", a.property, a.target),
        };
        let mut done = Done::new(done);
        if let Some(key) = engine.key_text(obj, prop, &now).filter(|_| !hidden) {
            done = done.with("key", json!(key));
        }
        Ok(done
            .with("property", json!(a.property))
            .with("value", show(&now))
            .with("previous", show(&previous))
            .with("bound", json!(engine.is_bound(obj, prop)))
            .with("binding_kept", json!(cover.is_some())))
    }

    /// Takes back what devtools did to a property: the layer over its
    /// binding goes, or the value from before the first edit comes back.
    fn revert_property(
        &mut self,
        obj: ObjectId,
        prop: PropIndex,
        a: &SetPropertyArgs,
    ) -> Result<Done, SurfaceError> {
        let engine = self.engine;
        let Some(edit) = self.edits.take(obj, &a.property) else {
            return Err(not_actionable(format!(
                "devtools did not change `{}` of `{}`, so there is nothing to revert",
                a.property, a.target
            )));
        };
        let previous = engine.read_by_name(obj, &a.property);
        let restored = match edit.cover {
            Some(id) => {
                engine
                    .remove_override(obj, prop, id, RestoreMode::BindingOrValue)
                    .map_err(|e| engine_error(&a.target, e))?;
                if engine.is_bound(obj, prop) {
                    "binding"
                } else {
                    "value"
                }
            }
            None => {
                engine
                    .write_by_name(obj, &a.property, edit.before)
                    .map_err(|e| engine_error(&a.target, e))?;
                "value"
            }
        };
        let now = engine.read_by_name(obj, &a.property);
        let hidden = TYPED.contains(&a.property.as_str())
            && capture::is_secret(engine, obj, self.redact_all);
        let show = |v: &QmlValue| {
            if hidden {
                json!(REDACTED)
            } else {
                to_json(engine, v, 0)
            }
        };
        Ok(Done::new(format!(
            "reverted `{}` of `{}` to its {restored}",
            a.property, a.target
        ))
        .with("property", json!(a.property))
        .with("value", show(&now))
        .with("previous", show(&previous))
        .with("restored", json!(restored)))
    }

    fn invoke(&mut self, a: &InvokeArgs) -> Result<Done, SurfaceError> {
        // A method or a signal handler is script the application wrote: it
        // may read a masked value and answer or throw it, changed or not.
        if self.redact_all {
            return Err(Self::masked_everywhere());
        }
        let obj = self.object(&a.target, "app_invoke")?;
        let engine = self.engine;
        let args: Vec<QmlValue> = a
            .args
            .iter()
            .map(|v| to_qml(engine, v))
            .collect::<Result<_, _>>()?;
        if engine.method_index(obj, &a.name).is_ok() {
            let before = last_fault(engine);
            let result = engine
                .invoke_by_name(obj, &a.name, &args)
                .map_err(|e| engine_error(&a.target, e))?;
            // A function that throws is logged and answers `undefined`; to an
            // agent that is a failure, so it is told as one.
            if let Some(fault) = new_fault(engine, before) {
                return Err(SurfaceError::new(
                    ErrorCode::AppFaulted,
                    format!("`{}` of `{}` threw: {fault}", a.name, a.target),
                ));
            }
            let result = if capture::is_secret(engine, obj, self.redact_all) {
                json!(REDACTED)
            } else {
                to_json(engine, &result, 0)
            };
            return Ok(
                Done::new(format!("called `{}` of `{}`", a.name, a.target)).with("result", result)
            );
        }
        if engine.signal_index(obj, &a.name).is_ok() {
            engine
                .emit_by_name(obj, &a.name, &args)
                .map_err(|e| engine_error(&a.target, e))?;
            return Ok(Done::new(format!("sent `{}` of `{}`", a.name, a.target))
                .with("result", Value::Null)
                .with("emitted", json!(true)));
        }
        Err(SurfaceError::not_found(format!(
            "`{}` has no method or signal `{}`. `app_inspect` lists its properties; a method is \
             one declared as `function {}(…)` or by its type",
            a.target, a.name, a.name
        )))
    }

    /// Refuses script run in `obj` that could read around the mask: in a
    /// field that hides its text, or anywhere when every value is masked.
    fn may_run_script_in(&self, obj: ObjectId, what: &str) -> Result<(), SurfaceError> {
        if self.redact_all {
            return Err(Self::masked_everywhere());
        }
        if capture::is_secret(self.engine, obj, self.redact_all) {
            return Err(SurfaceError::new(
                ErrorCode::CapabilityDenied,
                format!(
                    "`{what}` hides its text (`echoMode`), and a script run in it reads \
                     the text around the mask. Run the script in another object"
                ),
            ));
        }
        Ok(())
    }

    fn masked_everywhere() -> SurfaceError {
        SurfaceError::new(
            ErrorCode::CapabilityDenied,
            "script, and a method or signal of the application, is refused while this \
             inspector masks every editable value (`--mcp-redact`): a script reads values \
             around the mask, and cannot be held to it. Start without `--mcp-redact` to run \
             script",
        )
    }

    fn eval(&mut self, a: &EvalArgs) -> Result<Done, SurfaceError> {
        if self.redact_all {
            return Err(Self::masked_everywhere());
        }
        let this = match &a.target {
            None => None,
            Some(what) => {
                let obj = self.object(what, "app_eval")?;
                self.may_run_script_in(obj, what)?;
                Some(obj)
            }
        };
        let limit = a.timeout_ms.map_or(DEFAULT_EVAL, Duration::from_millis);
        let budget = (self.per_copy / 4).clamp(2000, 16_000) as u32;
        let out = self
            .engine
            .eval_js_in(this, &a.source, limit, [6, 100, 2000, 2000, budget]);
        let text = match out {
            Ok(text) => text,
            Err(e) => {
                let said = e.to_string();
                let lower = said.to_lowercase();
                return Err(if lower.contains("interrupt") || lower.contains("time") {
                    SurfaceError::new(
                        ErrorCode::Timeout,
                        format!(
                            "the script did not finish in {} ms and was stopped; the \
                             application goes on. Give `timeout_ms` more (at most {}), or write \
                             it to end",
                            limit.as_millis(),
                            ljos_engine::MAX_EVAL_TIME.as_millis()
                        ),
                    )
                } else {
                    SurfaceError::new(
                        ErrorCode::AppFaulted,
                        format!("the script host failed: {said}"),
                    )
                });
            }
        };
        let parsed: Value = serde_json::from_str(&text).map_err(|e| {
            SurfaceError::new(
                ErrorCode::Internal,
                format!("the script's answer is no JSON: {e}"),
            )
        })?;
        if let Some(message) = parsed.get("error") {
            let name = parsed
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Error");
            return Err(SurfaceError::invalid_argument(format!(
                "the script threw {name}: {}",
                clip(message.as_str().unwrap_or_default(), 500)
            )));
        }
        let mut value = parsed.get("value").cloned().unwrap_or(Value::Null);
        let mut truncated = parsed
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // A script may have changed what the fields hold.
        self.settle();
        self.refresh();
        mask_data(&mut value, &secrets_of(&self.capture));
        // The serialiser bounded what it wrote; this is for the sum of it
        // with the escaping of both copies.
        if value.to_string().len() > self.per_copy / 2 {
            value = json!("[too large to give; return less]");
            truncated = true;
        }
        Ok(Done::new(format!(
            "{}{}",
            clip(&value.to_string(), 2000),
            if truncated { " (cut)" } else { "" }
        ))
        .with("value", value)
        .with("truncated", json!(truncated)))
    }

    /// One act, then the frame settled and the scene copied again.
    fn step(&mut self, act: &Act) -> Result<Done, SurfaceError> {
        let done = match act {
            Act::Click(a) => self.click(a),
            Act::Type(a) => self.type_text(a),
            Act::Press(a) => self.press(a),
            Act::Scroll(a) => self.scroll(a),
            Act::Touch(a) => self.touch(a),
            Act::Advance(a) => self.advance(a),
            Act::Resize(a) => self.resize(a),
            Act::Appearance(a) => self.appearance(a),
            Act::SetProperty(a) => self.set_property(a),
            Act::Invoke(a) => self.invoke(a),
            Act::Eval(a) => self.eval(a),
            Act::Reload | Act::Batch(_) => Err(SurfaceError::new(
                ErrorCode::Internal,
                "this act is not run by itself",
            )),
        };
        self.settle();
        self.refresh();
        done
    }
}

// ----- the pipeline ----------------------------------------------------------

fn error_json(e: &SurfaceError) -> Value {
    json!({ "code": e.code.as_str(), "message": e.message })
}

impl Inspector {
    /// Runs `act` and answers with the capture to go on with: the scene as it
    /// is after the act.
    pub(super) fn run_act(
        &mut self,
        engine: &Engine,
        scene: &mut QuickScene,
        clock: &dyn Clock,
        capture: Rc<Capture>,
        act: &Act,
        per_copy: usize,
    ) -> (Answer, Rc<Capture>) {
        let before = capture.clone();
        let started = Instant::now();
        let real = RealClock(clock);
        let clock: &dyn SceneClock = match &self.vclock {
            Some(test) => test,
            None => &real,
        };
        let mut hands = Hands {
            engine,
            scene,
            clock,
            refs: &mut self.refs,
            edits: &mut self.edits,
            redact_all: self.cfg.redact_all,
            capture,
            per_copy,
            drive: DriveState::default(),
            unsettled: false,
        };
        let body = match act {
            Act::Batch(steps) => Ok(hands.batch(steps)),
            one => hands.step(one),
        };
        let Hands {
            capture: after,
            unsettled,
            scene,
            ..
        } = hands;
        // The host paints what the act did.
        scene.request_frame();
        let secrets = secrets_of_all(&[&before, &after]);
        let done = match body {
            Ok(done) => done,
            Err(mut e) => {
                mask_error(&mut e, &secrets);
                return (Err(e), after);
            }
        };
        // What changed: the outline before this act against the one now.
        let since = self.kept.back().map_or(0, |k| k.epoch);
        let changes = delta(
            &before.tree,
            &after.tree,
            since,
            &mut self.refs,
            MAX_DELTA_LINES,
        );
        let epoch = self.next_epoch();
        self.keep(epoch, after.clone(), View::whole(&after));
        let Done {
            mut fields,
            summary,
        } = done;
        fields.insert("epoch".into(), json!(epoch));
        fields.insert("changed".into(), json!(changes.changed));
        fields.insert("added".into(), json!(changes.added));
        fields.insert("removed".into(), json!(changes.removed));
        fields.insert("delta".into(), json!(changes.text));
        fields.insert("settled".into(), json!(!unsettled));
        mask_answered(&mut fields, &secrets);
        let mut value = Value::Object(fields);
        mask_json(&mut value, &secrets);
        let mut text = summary;
        mask_text(&mut text, &secrets);
        if !changes.text.is_empty() {
            text.push('\n');
            text.push_str(&changes.text);
        }
        let _ = std::fmt::Write::write_fmt(
            &mut text,
            format_args!(
                "\n(snapshot {epoch}: {} changed, {} added, {} removed{}; {} ms)",
                changes.changed,
                changes.added,
                changes.removed,
                if unsettled {
                    "; the scene had not settled"
                } else {
                    ""
                },
                started.elapsed().as_millis()
            ),
        );
        let mut reply = ToolReply::from(value);
        reply = reply.with_text(text);
        (Ok(reply), after)
    }
}

impl Hands<'_> {
    /// The steps one after the other. The first that fails stops the rest.
    /// Each says what it cost; what changed is told once, by the caller.
    fn batch(&mut self, steps: &[(&'static str, Act)]) -> Done {
        let mut results = Vec::new();
        let mut failed = Value::Null;
        for (i, (tool, act)) in steps.iter().enumerate() {
            let frames = self.drive.frames;
            let started = Instant::now();
            let outcome = self.step(act);
            let cost = json!({
                "frames": self.drive.frames - frames,
                "ms": started.elapsed().as_millis() as u64,
            });
            match outcome {
                Ok(done) => results.push(json!({
                    "tool": tool,
                    "ok": true,
                    "result": Value::Object(done.fields),
                    "summary": done.summary,
                    "cost": cost,
                })),
                Err(e) => {
                    results.push(json!({
                        "tool": tool,
                        "ok": false,
                        "error": error_json(&e),
                        "cost": cost,
                    }));
                    failed = json!({ "step": i, "code": e.code.as_str(), "message": e.message });
                    break;
                }
            }
        }
        let completed = results.iter().filter(|r| r["ok"] == json!(true)).count();
        let summary = match failed.get("step") {
            None => format!("{completed} of {} steps done", steps.len()),
            Some(step) => format!(
                "{completed} of {} steps done; step {step} failed ({}): {}",
                steps.len(),
                failed["code"].as_str().unwrap_or_default(),
                failed["message"].as_str().unwrap_or_default()
            ),
        };
        Done::new(summary)
            .with("completed", json!(completed))
            .with("failed", failed)
            .with("steps", Value::Array(results))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_masked_with_the_secrets_of_before_and_after() {
        let secrets = vec!["hunter22".to_string(), "hunter2".to_string()];
        let mut e = SurfaceError::new(ErrorCode::AppFaulted, "threw: hunter22 then hunter2")
            .with_details(json!({ "said": ["hunter2"], "n": 1 }));
        mask_error(&mut e, &secrets);
        assert_eq!(e.message, "threw: <redacted> then <redacted>");
        assert_eq!(e.details, Some(json!({ "said": ["<redacted>"], "n": 1 })));
    }

    #[test]
    fn a_whole_secret_text_is_masked_once() {
        let mut s = "hunter2".to_string();
        mask_text(&mut s, &["hunter2".to_string()]);
        assert_eq!(s, REDACTED);
    }
}
