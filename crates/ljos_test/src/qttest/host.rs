//! What the test cases of one engine share while a test file runs: the
//! scene and the virtual clock they drive, the report they write to, and
//! what the running test function has said it expects.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos_engine::{Engine, EngineError, LogEntry, LogLevel, ObjectId, Value};
use ljos_platform::Platform;
use ljos_quick::{DOUBLE_CLICK_INTERVAL, ImageStore, MouseButtons, QuickScene};
use ljos_runtime::{Clock as _, Time, VirtualClock};

use super::report::{LineSink, Message, MessageKind, Reporter};

/// What a failed check throws to leave the test function, and what `skip`
/// throws. They are Qt's words, so a test that catches an exception and
/// looks at its message sees what it saw there.
pub(crate) const FAIL_MARKER: &str = "QtQuickTest::fail";
pub(crate) const SKIP_MARKER: &str = "QtQuickTest::skip";

/// How the engine's message about a property an object's type does not
/// have begins.
const REFUSED: &str = "Cannot assign to non-existent property";

/// Frames a scene may need to come to rest, and timer firings one wait may
/// see, before something is wrong with the test.
const MAX_SETTLE_FRAMES: usize = 64;
const MAX_TIMER_STEPS: usize = 1_000_000;

/// The scene of the test file and the clock it runs on.
pub(crate) struct Rig {
    pub(crate) clock: Arc<VirtualClock>,
    pub(crate) scene: QuickScene,
    /// When the scene last said it next needs a frame.
    wake: Option<Time>,
    /// How many frames showed something new.
    pub(crate) frames: u64,
    /// How far the time on pointer events is ahead of the clock. Presses
    /// are told apart by their time, and two clicks a test makes one after
    /// the other are not a double click in Qt; see `input.rs`.
    pub(crate) lead: Duration,
    /// The mouse buttons a test holds down.
    pub(crate) held: MouseButtons,
}

impl Rig {
    /// The time an event made now carries.
    pub(crate) fn event_time(&self) -> Time {
        self.clock.now() + self.lead
    }

    /// The time a mouse event made now carries. qmltestrunner stamps each
    /// mouse event a millisecond after its delay: moves 16 ms apart are 17
    /// ms apart to what measures their speed (a fling).
    pub(crate) fn mouse_time(&mut self) -> Time {
        let t = self.event_time();
        self.lead += Duration::from_millis(1);
        t
    }

    /// The next press is too late to be the second of a double click.
    pub(crate) fn fresh_press(&mut self) {
        self.lead += DOUBLE_CLICK_INTERVAL + Duration::from_millis(1);
    }
}

/// What `expectFail` said about the next check.
pub(crate) struct Expect {
    pub(crate) message: String,
    /// Whether the test function ends at the expected failure.
    pub(crate) abort: bool,
}

/// An exception that left a test function.
pub(crate) struct Uncaught {
    pub(crate) message: String,
    pub(crate) place: Option<(String, u32)>,
}

#[derive(Default)]
pub(crate) struct State {
    reporter: Option<Rc<RefCell<Reporter>>>,
    /// Where what the file says while it loads goes: it is no part of the
    /// report, as in Qt.
    diagnostics: Option<Rc<RefCell<LineSink>>>,
    pub(crate) loading: bool,
    /// The tag of the row the running function was given.
    pub(crate) tag: Option<String>,
    pub(crate) expect: Option<Expect>,
    /// A check failed (or `skip` was called) in JS whose exception was not
    /// the caller's to see: a signal handler, a `tryVerify` function.
    pub(crate) marked: bool,
    /// The runner is inside a function it called.
    pub(crate) top_level: bool,
    /// A function a check called (`tryVerify`) is running: what it throws
    /// is the test function's, although JS is still running around it.
    capture: bool,
    pub(crate) uncaught: Option<Uncaught>,
    /// The file was made although something written in it cannot be: the
    /// first such thing.
    refused: Option<Uncaught>,
    /// Objects made by `createTemporaryObject` and its kin: by the running
    /// test function, and by `initTestCase`.
    pub(crate) temporaries: Vec<ObjectId>,
    pub(crate) case_temporaries: Vec<ObjectId>,
    /// Whether what is made now lives until the case ends.
    pub(crate) in_case_scope: bool,
    /// Where `save` puts a picture whose path is relative.
    pub(crate) save_dir: PathBuf,
}

/// The per-engine test state.
#[derive(Default)]
pub(crate) struct Host {
    rig: RefCell<Option<Rig>>,
    pub(crate) state: RefCell<State>,
    helpers: RefCell<Vec<(&'static str, Value)>>,
}

fn invoke_error(text: impl Into<String>) -> EngineError {
    EngineError::Invoke(text.into())
}

impl Host {
    pub(crate) fn of(engine: &Engine) -> Rc<Host> {
        engine.extension::<Host>()
    }

    // ----------------------------------------------------------- set-up

    pub(crate) fn install(
        &self,
        clock: Arc<VirtualClock>,
        scene: QuickScene,
        reporter: Rc<RefCell<Reporter>>,
        diagnostics: Rc<RefCell<LineSink>>,
        save_dir: PathBuf,
    ) {
        *self.rig.borrow_mut() = Some(Rig {
            clock,
            scene,
            wake: None,
            frames: 0,
            lead: Duration::ZERO,
            held: MouseButtons::empty(),
        });
        let mut state = self.state.borrow_mut();
        *state = State {
            reporter: Some(reporter),
            diagnostics: Some(diagnostics),
            loading: true,
            save_dir,
            ..State::default()
        };
    }

    /// Lets go of everything that keeps the engine alive: the scene, the
    /// JS functions, the report.
    pub(crate) fn uninstall(&self) {
        self.rig.borrow_mut().take();
        self.helpers.borrow_mut().clear();
        *self.state.borrow_mut() = State::default();
    }

    // ------------------------------------------------------------ scene

    /// Works on the scene. An error when no test file is running, and when
    /// the scene is in the middle of an event or a frame: a handler that
    /// sends input from inside the delivery of input.
    pub(crate) fn with_rig<R>(&self, f: impl FnOnce(&mut Rig) -> R) -> Result<R, EngineError> {
        let mut rig = self.rig.try_borrow_mut().map_err(|_| {
            invoke_error("a test cannot send input or wait from inside a frame or an event")
        })?;
        let rig = rig
            .as_mut()
            .ok_or_else(|| invoke_error("no test run is in progress: use `ljos test`"))?;
        Ok(f(rig))
    }

    pub(crate) fn clock(&self) -> Result<Arc<VirtualClock>, EngineError> {
        self.with_rig(|rig| rig.clock.clone())
    }

    /// Frames until the scene is at rest: bindings, layout, deferred
    /// deletes, what `Qt.callLater` put off.
    pub(crate) fn settle(&self, engine: &Engine) -> Result<(), EngineError> {
        for _ in 0..MAX_SETTLE_FRAMES {
            if engine.has_due_calls() {
                engine.run_pending_calls();
            }
            let idle = self.with_rig(|rig| {
                let now = rig.clock.now();
                let out = rig.scene.frame(engine, now);
                if out.changed {
                    rig.frames += 1;
                }
                let idle = !rig.scene.needs_frame() && out.next_wake.is_none_or(|t| t > now);
                if idle {
                    rig.wake = out.next_wake;
                }
                idle
            })?;
            if idle && !engine.has_due_calls() {
                return Ok(());
            }
        }
        Err(invoke_error(format!(
            "the scene did not come to rest in {MAX_SETTLE_FRAMES} frames"
        )))
    }

    /// How many frames have shown something new so far.
    pub(crate) fn frames(&self) -> Result<u64, EngineError> {
        self.with_rig(|rig| rig.frames)
    }

    /// Lets `ms` of the application's time pass: every timer fires at its
    /// own time and every frame an animation asks for is made, as a host
    /// woken at each of them would do it.
    pub(crate) fn advance(&self, engine: &Engine, ms: u64) -> Result<(), EngineError> {
        self.advance_while(engine, ms, &mut || Ok(false))
            .map(|_| ())
    }

    /// Lets up to `ms` pass, looking after every timer and frame whether
    /// `done` holds. Whether it came to hold.
    pub(crate) fn advance_until(
        &self,
        engine: &Engine,
        ms: u64,
        mut done: impl FnMut() -> Result<bool, EngineError>,
    ) -> Result<bool, EngineError> {
        self.settle(engine)?;
        if done()? {
            return Ok(true);
        }
        self.advance_while(engine, ms, &mut done)
    }

    fn advance_while(
        &self,
        engine: &Engine,
        ms: u64,
        done: &mut dyn FnMut() -> Result<bool, EngineError>,
    ) -> Result<bool, EngineError> {
        let clock = self.clock()?;
        let images = ImageStore::of(engine);
        let end = clock.now() + Duration::from_millis(ms);
        let platform = Platform::of(engine);
        for _ in 0..MAX_TIMER_STEPS {
            // Pictures that were being read, and what was fetched, have
            // arrived, whatever the disk or the network took: the
            // application's time is not the machine's.
            images.finish(engine);
            if platform.pending() > 0 {
                platform.finish(engine);
                self.settle(engine)?;
            }
            let wake = self.with_rig(|rig| rig.wake)?;
            let next = [engine.next_timer_deadline(), wake]
                .into_iter()
                .flatten()
                .min();
            let last = match next {
                Some(t) if t <= end => {
                    if t > clock.now() {
                        clock.set(t);
                    }
                    false
                }
                _ => {
                    clock.set(end);
                    true
                }
            };
            engine.advance_timers(clock.now());
            images.finish(engine);
            self.settle(engine)?;
            if done()? {
                return Ok(true);
            }
            if last {
                return Ok(false);
            }
        }
        Err(invoke_error(format!(
            "timers fired more than {MAX_TIMER_STEPS} times in {ms} ms"
        )))
    }

    // ----------------------------------------------------------- report

    /// Where the JS that called into the test module was written: a path
    /// and a line.
    pub(crate) fn place(&self, engine: &Engine) -> Option<(String, u32)> {
        let at = engine.js_origin()?;
        Some((path_of(&at.file), at.line))
    }

    /// Says `message` about the running test function. Without a run
    /// (a `TestCase` in a program that is not being tested) it is a
    /// warning of the engine.
    pub(crate) fn say(&self, engine: &Engine, message: Message) {
        let reporter = self.state.borrow().reporter.clone();
        match reporter {
            Some(reporter) => reporter.borrow_mut().say(message),
            None => engine.log(LogLevel::Warning, "qml", message.message),
        }
    }

    /// One check of a test: `ok` is whether it held. A failed check is
    /// reported and ends the test function by throwing; `expectFail`
    /// turns the next one around.
    pub(crate) fn check(
        &self,
        engine: &Engine,
        ok: bool,
        failure: impl FnOnce() -> Message,
        unexpected: impl FnOnce() -> String,
    ) -> Result<(), EngineError> {
        let place = self.place(engine);
        let expect = self.state.borrow_mut().expect.take();
        match (expect, ok) {
            (None, true) => Ok(()),
            (None, false) => {
                self.say(engine, failure().at(place));
                Err(invoke_error(FAIL_MARKER))
            }
            (Some(expect), false) => {
                self.say(
                    engine,
                    Message::new(MessageKind::ExpectedFail, expect.message).at(place),
                );
                if expect.abort {
                    Err(invoke_error(FAIL_MARKER))
                } else {
                    Ok(())
                }
            }
            (Some(_), true) => {
                self.say(
                    engine,
                    Message::new(MessageKind::UnexpectedPass, unexpected()).at(place),
                );
                Err(invoke_error(FAIL_MARKER))
            }
        }
    }

    /// A check that says `text` did not hold, in the words of Qt's
    /// `QVERIFY`.
    pub(crate) fn verify(&self, engine: &Engine, ok: bool, text: &str) -> Result<(), EngineError> {
        self.check(
            engine,
            ok,
            || Message::new(MessageKind::Fail, format!("'{text}' returned FALSE. ()")),
            || format!("'{text}' returned TRUE unexpectedly. ()"),
        )
    }

    /// What the engine and the program log while a test file runs.
    pub(crate) fn on_log(&self, engine: &Engine, entry: &LogEntry) {
        let first = entry.message.lines().next().unwrap_or_default();
        // The exception a failed check threw was caught by nobody: the
        // failure itself has been reported already.
        if entry.level == LogLevel::Warning
            && (first.contains(FAIL_MARKER) || first.contains(SKIP_MARKER))
        {
            self.state.borrow_mut().marked = true;
            return;
        }
        let (loading, top_level, capture, diagnostics) = {
            let state = self.state.borrow();
            (
                state.loading,
                state.top_level,
                state.capture,
                state.diagnostics.clone(),
            )
        };
        if loading {
            // The engine makes a compiled file without a property its
            // type does not have, and says so. Qt refuses such a file,
            // and a test that ran without what it wrote would mislead.
            if entry.level == LogLevel::Warning
                && let Some((file, line, message)) = split_place(first)
                && message.starts_with(REFUSED)
            {
                self.state.borrow_mut().refused.get_or_insert(Uncaught {
                    message: message.to_owned(),
                    place: Some((path_of(file), line)),
                });
                return;
            }
            if let Some(sink) = diagnostics {
                (sink.borrow_mut())(&entry.to_string());
            }
            return;
        }
        // An exception is logged where it stops. With no JS running that
        // is the function the runner called, so the function threw it.
        let thrown = entry.level == LogLevel::Warning
            && (capture || (top_level && engine.js_origin().is_none()));
        if thrown && let Some((file, line, message)) = split_place(first) {
            self.state.borrow_mut().uncaught = Some(Uncaught {
                message: exception_message(message).to_owned(),
                place: Some((path_of(file), line)),
            });
            return;
        }
        let kind = match entry.level {
            LogLevel::Debug => MessageKind::Debug,
            LogLevel::Info => MessageKind::Info,
            LogLevel::Warning => MessageKind::Warn,
            LogLevel::Critical => MessageKind::Critical,
        };
        // Qt prints what `console` wrote under its category and what the
        // engine warns of, which starts with its place, as it is.
        let text = if entry.category != "qml" {
            entry.to_string()
        } else if split_place(first).is_some() {
            entry.message.clone()
        } else {
            format!("qml: {}", entry.message)
        };
        self.say(engine, Message::new(kind, text));
    }

    // --------------------------------------------------------------- JS

    /// A function written in JS, made once per engine.
    fn js(&self, engine: &Engine, source: &'static str) -> Result<Value, EngineError> {
        let known = self
            .helpers
            .borrow()
            .iter()
            .find(|(s, _)| *s == source)
            .map(|(_, f)| f.clone());
        if let Some(f) = known {
            return Ok(f);
        }
        let f = engine.eval_js(source)?;
        self.helpers.borrow_mut().push((source, f.clone()));
        Ok(f)
    }

    /// Calls the JS function `source` is. `None` when it threw.
    pub(crate) fn call(
        &self,
        engine: &Engine,
        source: &'static str,
        args: &[Value],
    ) -> Option<Value> {
        let f = self.js(engine, source).ok()?;
        engine.call_js(&f, args)
    }

    /// Calls a function a test gave to a check. What it throws ends the
    /// test function: a failed check in it has been reported already, and
    /// anything else is reported by the runner as the exception it is.
    pub(crate) fn called(&self, engine: &Engine, function: &Value) -> Result<Value, EngineError> {
        {
            let mut state = self.state.borrow_mut();
            state.marked = false;
            state.capture = true;
        }
        let out = engine.call_js(function, &[]);
        self.state.borrow_mut().capture = false;
        out.ok_or_else(|| invoke_error(FAIL_MARKER))
    }

    // ------------------------------------------------------------ runner

    /// What made the file one Qt would not have loaded, if anything did.
    pub(crate) fn refused(&self) -> Option<Uncaught> {
        self.state.borrow_mut().refused.take()
    }

    /// The file has been made: what is logged from now on is a test's.
    pub(crate) fn loaded(&self) {
        self.state.borrow_mut().loading = false;
    }

    /// Lets the application's time pass up to the next thing that is due:
    /// a timer, a frame of an animation. `false` when nothing is.
    pub(crate) fn advance_to_next(&self, engine: &Engine) -> Result<bool, EngineError> {
        let clock = self.clock()?;
        let images = ImageStore::of(engine);
        images.finish(engine);
        let wake = self.with_rig(|rig| rig.wake)?;
        let next = [engine.next_timer_deadline(), wake]
            .into_iter()
            .flatten()
            .min();
        let Some(next) = next else {
            return Ok(false);
        };
        clock.set(next);
        engine.advance_timers(clock.now());
        images.finish(engine);
        self.settle(engine)?;
        Ok(true)
    }

    // -------------------------------------------------------- temporaries

    /// `obj` lives until the running test function is over (until the case
    /// is, for one made in `initTestCase`).
    pub(crate) fn keep_temporary(&self, obj: ObjectId) {
        let mut state = self.state.borrow_mut();
        if state.in_case_scope {
            state.case_temporaries.push(obj);
        } else {
            state.temporaries.push(obj);
        }
    }

    /// `obj` lives until the case is over: a test may keep a picture from
    /// one function to compare it in the next.
    pub(crate) fn keep_for_case(&self, obj: ObjectId) {
        self.state.borrow_mut().case_temporaries.push(obj);
    }

    /// Destroys what the test function made (and what the case made, when
    /// the case is over).
    pub(crate) fn drop_temporaries(&self, engine: &Engine, case_too: bool) {
        let gone = {
            let mut state = self.state.borrow_mut();
            let mut gone = std::mem::take(&mut state.temporaries);
            if case_too {
                gone.append(&mut state.case_temporaries);
            }
            gone
        };
        for obj in gone {
            if engine.is_alive(obj) {
                engine.destroy_now(obj);
            }
        }
    }
}

/// A file URL as the path a report names.
pub(crate) fn path_of(url: &str) -> String {
    url.strip_prefix("file://").unwrap_or(url).to_owned()
}

/// Splits `file:line: message`, the shape the engine logs an exception
/// and its own warnings in. The file may have colons of its own (a URL).
pub(crate) fn split_place(text: &str) -> Option<(&str, u32, &str)> {
    let mut from = 0;
    while let Some(at) = text[from..].find(':') {
        let colon = from + at;
        let rest = &text[colon + 1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && colon > 0 {
            // A column may follow the line (`file:5:3: message`).
            let after = &rest[digits..];
            let after = match after.strip_prefix(':') {
                Some(column) => match column.bytes().take_while(u8::is_ascii_digit).count() {
                    0 => after,
                    n => &column[n..],
                },
                None => after,
            };
            if let Some(message) = after.strip_prefix(": ")
                && let Ok(line) = rest[..digits].parse()
            {
                return Some((&text[..colon], line, message));
            }
        }
        from = colon + 1;
    }
    None
}

/// The message of an exception without the name of its kind in front:
/// `Error: boom` is `boom`, as `e.message` is.
pub(crate) fn exception_message(text: &str) -> &str {
    match text.split_once(": ") {
        Some((kind, message))
            if kind.ends_with("Error") && kind.chars().all(|c| c.is_ascii_alphabetic()) =>
        {
            message
        }
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_logged_exception_is_split_into_its_place_and_its_message() {
        assert_eq!(
            split_place("file:///dir/tst_a.qml:12: Error: boom"),
            Some(("file:///dir/tst_a.qml", 12, "Error: boom"))
        );
        assert_eq!(
            split_place("tst_a.qml:3: a: b: c"),
            Some(("tst_a.qml", 3, "a: b: c"))
        );
        assert_eq!(
            split_place("file:///dir/tst_a.qml:5:3: Cannot assign"),
            Some(("file:///dir/tst_a.qml", 5, "Cannot assign")),
            "a column after the line is not the line"
        );
        assert_eq!(split_place("just words: 12"), None);
        assert_eq!(split_place(":3: nothing in front"), None);
        assert_eq!(split_place(""), None);
    }

    #[test]
    fn the_kind_of_an_exception_is_not_part_of_its_message() {
        assert_eq!(exception_message("Error: boom"), "boom");
        assert_eq!(
            exception_message("ReferenceError: nothing is not defined"),
            "nothing is not defined"
        );
        assert_eq!(exception_message("the answer: 42"), "the answer: 42");
        assert_eq!(exception_message("plain"), "plain");
    }

    #[test]
    fn a_file_url_is_reported_as_a_path() {
        assert_eq!(path_of("file:///dir/tst_a.qml"), "/dir/tst_a.qml");
        assert_eq!(path_of("tst_a.qml"), "tst_a.qml");
    }
}
