//! The runner: which test cases of a file run, in what order, and what
//! happens around each of their functions
//! (doc.qt.io/qt-6/qml-qttest-testcase.html, "Introduction" and
//! "Data-driven Tests"; doc.qt.io/qt-6/qtquicktest-index.html).
//!
//! The runner is given files that are compiled already. Whoever calls it
//! makes one engine per test file, with the modules the file may import
//! and its component source in place, and gives it to
//! [`Session::run_file`].

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos_engine::{Engine, EngineError, ObjectId, QmlType as _, Value};
use ljos_quick::{Item, QuickScene, is_item};
use ljos_render::kurbo;
use ljos_runtime::{Clock as _, VirtualClock};

use super::case::TestCase;
use super::host::{FAIL_MARKER, Host, SKIP_MARKER, path_of, split_place};
use super::report::{LineSink, Message, MessageKind, Outcome, Report, Reporter};

/// The window a test file is shown in when its root has no size.
const DEFAULT_WINDOW: (f64, f64) = (800.0, 600.0);

/// How much of the application's time may pass while a test case waits
/// for `when` before the case is given up on. Qt's runner waits for ever.
const WHEN_TIMEOUT: Duration = Duration::from_secs(300);

/// The time one call into JS may take. A test function waits for timers
/// and animations inside one call, so the budget of a frame-driven host
/// (milliseconds) does not fit; this one only ends a loop that never
/// would.
const JS_BUDGET: Duration = Duration::from_secs(300);

const LENGTH: &str = "(function (rows) { return Array.isArray(rows) ? rows.length : -1; })";
const ELEMENT: &str = "(function (rows, i) { return rows[i]; })";

/// One `--filter`: which functions of a run are run.
///
/// `Case::function` and `Case::function:tag` are what Qt's runner takes.
/// A bare name is a test case (all of its functions) or a function (in
/// every test case), with an optional `:tag`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    case: Option<String>,
    /// A bare name: a case or a function.
    name: String,
    tag: Option<String>,
}

impl Filter {
    pub fn parse(text: &str) -> Filter {
        let (case, rest) = match text.split_once("::") {
            Some((case, rest)) => (Some(case.to_owned()), rest),
            None => (None, text),
        };
        let (name, tag) = match rest.split_once(':') {
            Some((name, tag)) => (name, Some(tag.to_owned())),
            None => (rest, None),
        };
        Filter {
            case,
            name: name.to_owned(),
            tag,
        }
    }

    /// Whether the filter names the whole of `case`.
    fn names_whole_case(&self, case: &str) -> bool {
        self.case.is_none() && self.tag.is_none() && !case.is_empty() && self.name == case
    }

    fn takes_function(&self, case: &str, function: &str) -> bool {
        match &self.case {
            Some(c) => c == case && self.name == function,
            None => self.names_whole_case(case) || self.name == function,
        }
    }

    fn takes_row(&self, case: &str, function: &str, tag: Option<&str>) -> bool {
        self.takes_function(case, function)
            && (self.names_whole_case(case) || self.tag.is_none() || self.tag.as_deref() == tag)
    }
}

/// How a run is done.
#[derive(Clone, Debug)]
pub struct Options {
    /// With none, everything runs.
    pub filters: Vec<Filter>,
    /// Where `save` puts a picture whose path is relative.
    pub save_dir: PathBuf,
    /// The name in the first and the last line of the report.
    pub program: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            filters: Vec::new(),
            save_dir: PathBuf::from("."),
            program: "ljos test".to_owned(),
        }
    }
}

/// One run over any number of test files: one report.
pub struct Session {
    options: Options,
    reporter: Rc<RefCell<Reporter>>,
    diagnostics: Rc<RefCell<LineSink>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

/// Takes the test state out of the engine when the file is done, however
/// it ends: the log handler holds the engine.
struct Installed<'a> {
    engine: &'a Engine,
    host: Rc<Host>,
}

impl Drop for Installed<'_> {
    fn drop(&mut self) {
        self.host.uninstall();
        self.engine.clear_log_handler();
    }
}

impl Session {
    /// `out` is given the lines of the report as they happen;
    /// `diagnostics` what a file says while it is loaded, which is no part
    /// of the report.
    pub fn new(options: Options, out: LineSink, diagnostics: LineSink) -> Self {
        let reporter = Reporter::new(options.program.clone(), out);
        Session {
            options,
            reporter: Rc::new(RefCell::new(reporter)),
            diagnostics: Rc::new(RefCell::new(diagnostics)),
        }
    }

    /// A test file that could not be compiled: a failed test of its own,
    /// named after the file (`stem` is `tst_x` for `tst_x.qml`).
    pub fn compile_failure(&mut self, stem: &str, message: &str, place: Option<(String, u32)>) {
        self.reporter.borrow_mut().failure(
            stem,
            "compile",
            Message::new(MessageKind::Fail, message).at(place),
        );
    }

    /// Runs the test cases of one file. `engine` is the file's own, with
    /// `QtQuick` (and whatever else the file imports) registered and its
    /// component source set; `url` is what the source knows the file by,
    /// `stem` its name without `.qml`.
    pub fn run_file(&mut self, engine: &Engine, url: &str, stem: &str) {
        super::register(engine);
        let clock = Arc::new(VirtualClock::new());
        engine.set_clock(Rc::new(clock.clone()));
        let host = Host::of(engine);
        let scene = QuickScene::new(kurbo::Size::new(DEFAULT_WINDOW.0, DEFAULT_WINDOW.1));
        host.install(
            clock,
            scene,
            self.reporter.clone(),
            self.diagnostics.clone(),
            self.options.save_dir.clone(),
        );
        let _installed = Installed {
            engine,
            host: host.clone(),
        };
        let (log_host, log_engine) = (host.clone(), engine.clone());
        engine.set_log_handler(move |entry| log_host.on_log(&log_engine, entry));

        let root = match engine.create_from_url(url, None, Vec::new()) {
            Ok(root) => root,
            Err(e) => {
                let text = e.to_string();
                let first = text.lines().next().unwrap_or_default();
                let (message, place) = match split_place(first) {
                    Some((file, line, message)) => (message, Some((path_of(file), line))),
                    None => (first, None),
                };
                self.compile_failure(stem, message, place);
                return;
            }
        };
        if let Some(refused) = host.refused() {
            self.compile_failure(stem, &refused.message, refused.place);
            return;
        }
        // The budget is the JS host's, which the file has started by now
        // if it has any JS at all.
        let _ = engine.set_js_dispatch_budget(JS_BUDGET, JS_BUDGET);
        if let Err(e) = self.show(engine, &host, root) {
            self.compile_failure(stem, &e.to_string(), None);
            return;
        }
        host.loaded();
        if let Err(e) = self.run_cases(engine, &host, root) {
            // The scene itself gave up (it never came to rest): nothing
            // more of this file can be run.
            self.reporter.borrow_mut().failure(
                stem,
                "run",
                Message::new(MessageKind::Fail, e.to_string()),
            );
        }
    }

    /// The `Totals:` line, and the results of every file run so far.
    pub fn finish(self) -> Report {
        self.reporter.borrow_mut().finish()
    }

    // ------------------------------------------------------------- a file

    fn show(&self, engine: &Engine, host: &Host, root: ObjectId) -> Result<(), EngineError> {
        if !is_item(engine, root) {
            // Nothing to show: the cases below it still run, without
            // input.
            return Ok(());
        }
        let width = engine.get(root, Item::WIDTH)?;
        let height = engine.get(root, Item::HEIGHT)?;
        host.with_rig(|rig| {
            if width > 0.0 && height > 0.0 {
                rig.scene
                    .set_viewport(kurbo::Rect::new(0.0, 0.0, width, height));
            }
            rig.scene.set_root(engine, root)?;
            rig.scene.set_window_active(engine, true);
            Ok::<(), EngineError>(())
        })??;
        host.settle(engine)
    }

    fn run_cases(&self, engine: &Engine, host: &Host, root: ObjectId) -> Result<(), EngineError> {
        let mut pending = test_cases(engine, root);
        let all = pending.clone();
        let clock = host.clock()?;
        let mut shown = false;
        let mut waiting_since = clock.now();
        loop {
            host.settle(engine)?;
            let ready: Vec<ObjectId> = pending
                .iter()
                .copied()
                .filter(|c| engine.get(*c, TestCase::WHEN).unwrap_or(false))
                .collect();
            if !shown {
                // The cases that were ready before the window was shown go
                // first, as in Qt, and all of them see it shown.
                shown = true;
                for case in &all {
                    engine.set_internal(*case, TestCase::WINDOW_SHOWN, true)?;
                }
                if ready.is_empty() {
                    continue;
                }
            }
            if ready.is_empty() {
                let required = pending
                    .iter()
                    .any(|c| !engine.get(*c, TestCase::OPTIONAL).unwrap_or(false));
                let waited = clock.now() - waiting_since;
                if !required || waited >= WHEN_TIMEOUT || !host.advance_to_next(engine)? {
                    break;
                }
                continue;
            }
            // The last one first: the order Qt runs the cases of a file in.
            for case in ready.into_iter().rev() {
                pending.retain(|c| *c != case);
                if engine.is_alive(case) {
                    self.run_case(engine, host, case)?;
                }
            }
            waiting_since = clock.now();
        }
        for case in pending {
            if !engine.is_alive(case) || engine.get(case, TestCase::OPTIONAL)? {
                continue;
            }
            let name = engine.get(case, TestCase::NAME)?.to_string();
            if !self.runs_any(engine, case, &name) {
                continue;
            }
            // `file:line:column: `.
            let place = engine.declared_at(case).and_then(|at| {
                let mut parts = at.trim_end().trim_end_matches(':').rsplitn(3, ':');
                let _column = parts.next()?;
                let line = parts.next()?.parse().ok()?;
                Some((path_of(parts.next()?), line))
            });
            self.reporter.borrow_mut().failure(
                &name,
                "initTestCase",
                Message::new(MessageKind::Fail, "'when' never became true").at(place),
            );
        }
        Ok(())
    }

    // ------------------------------------------------------------- a case

    fn takes_function(&self, case: &str, function: &str) -> bool {
        let filters = &self.options.filters;
        filters.is_empty() || filters.iter().any(|f| f.takes_function(case, function))
    }

    fn takes_row(&self, case: &str, function: &str, tag: Option<&str>) -> bool {
        let filters = &self.options.filters;
        filters.is_empty() || filters.iter().any(|f| f.takes_row(case, function, tag))
    }

    fn runs_any(&self, engine: &Engine, case: ObjectId, name: &str) -> bool {
        self.options.filters.is_empty()
            || test_functions(engine, case)
                .iter()
                .any(|f| self.takes_function(name, f))
    }

    fn run_case(&self, engine: &Engine, host: &Host, case: ObjectId) -> Result<(), EngineError> {
        let name = engine.get(case, TestCase::NAME)?.to_string();
        let functions: Vec<String> = test_functions(engine, case)
            .into_iter()
            .filter(|f| self.takes_function(&name, f))
            .collect();
        // A case none of whose functions was asked for is not reported at
        // all, as in Qt.
        if !self.options.filters.is_empty() && functions.is_empty() {
            return engine.set_internal(case, TestCase::COMPLETED, true);
        }
        engine.set_internal(case, TestCase::RUNNING, true)?;
        let run = Run {
            session: self,
            engine,
            host,
            case,
            name: &name,
        };

        host.state.borrow_mut().in_case_scope = true;
        run.begin("initTestCase", None);
        run.call("initTestCase", &[])?;
        let started = run.end();
        host.state.borrow_mut().in_case_scope = false;

        if started == Outcome::Pass {
            for function in &functions {
                if !engine.is_alive(case) {
                    break;
                }
                run.function(function)?;
            }
        }

        if engine.is_alive(case) {
            run.begin("cleanupTestCase", None);
            run.call("cleanupTestCase", &[])?;
            run.end();
        }
        host.drop_temporaries(engine, true);
        host.settle(engine)?;
        if engine.is_alive(case) {
            engine.set_internal(case, TestCase::RUNNING, false)?;
            engine.set_internal(case, TestCase::COMPLETED, true)?;
        }
        Ok(())
    }
}

/// One test case being run.
struct Run<'a> {
    session: &'a Session,
    engine: &'a Engine,
    host: &'a Host,
    case: ObjectId,
    name: &'a str,
}

impl Run<'_> {
    fn begin(&self, function: &str, tag: Option<&str>) {
        let mut state = self.host.state.borrow_mut();
        state.tag = tag.map(str::to_owned);
        state.expect = None;
        drop(state);
        self.session
            .reporter
            .borrow_mut()
            .begin(self.name, function, tag);
    }

    fn end(&self) -> Outcome {
        self.host.state.borrow_mut().expect = None;
        self.session.reporter.borrow_mut().end()
    }

    fn has_failed(&self) -> bool {
        self.session.reporter.borrow().has_failed()
    }

    fn has(&self, function: &str) -> bool {
        self.engine.method_index(self.case, function).is_ok()
    }

    fn fail(&self, text: impl Into<String>, place: Option<(String, u32)>) {
        self.host
            .say(self.engine, Message::new(MessageKind::Fail, text).at(place));
    }

    /// Calls a function of the case, when it has it. Whatever the function
    /// does, the next one can run: a failed check has been reported where
    /// it failed, and an exception is reported here.
    fn call(&self, function: &str, args: &[Value]) -> Result<Value, EngineError> {
        let Ok(method) = self.engine.method_index(self.case, function) else {
            return Ok(Value::Undefined);
        };
        {
            let mut state = self.host.state.borrow_mut();
            state.top_level = true;
            state.uncaught = None;
            state.marked = false;
        }
        let out = self.engine.invoke(self.case, method, args);
        let uncaught = {
            let mut state = self.host.state.borrow_mut();
            state.top_level = false;
            state.uncaught.take()
        };
        if let Some(thrown) = uncaught {
            self.fail(
                format!("Uncaught exception: {}", thrown.message),
                thrown.place,
            );
        }
        let value = match out {
            Ok(value) => value,
            Err(e) => {
                let text = e.to_string();
                if !text.contains(FAIL_MARKER) && !text.contains(SKIP_MARKER) {
                    self.fail(text, None);
                }
                Value::Undefined
            }
        };
        // What the function set in motion comes to rest before the next
        // one starts.
        self.host.settle(self.engine)?;
        Ok(value)
    }

    /// The rows of a function: its own `_data` function's, else
    /// `init_data`'s. `None` for a function without data.
    fn rows(&self, function: &str) -> Result<Option<(String, Vec<Value>)>, EngineError> {
        let own = format!("{function}_data");
        let source = if self.has(&own) {
            own
        } else if self.has("init_data") {
            "init_data".to_owned()
        } else {
            return Ok(None);
        };
        let rows = match self.call(&source, &[])? {
            Value::List(items) => items.as_ref().clone(),
            rows @ Value::Js(_) => {
                let length = self
                    .host
                    .call(self.engine, LENGTH, std::slice::from_ref(&rows))
                    .and_then(|n| n.as_number())
                    .unwrap_or(-1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let length = length.max(0.0) as u32;
                (0..length)
                    .filter_map(|i| {
                        #[allow(clippy::cast_possible_wrap)]
                        let i = Value::Int(i as i32);
                        self.host.call(self.engine, ELEMENT, &[rows.clone(), i])
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        Ok(Some((source, rows)))
    }

    fn function(&self, function: &str) -> Result<(), EngineError> {
        // What the data function says and does wrong is the function's.
        self.begin(function, None);
        let rows = self.rows(function)?;
        if self.has_failed() {
            self.end();
            return Ok(());
        }
        let Some((source, rows)) = rows else {
            if self.session.takes_row(self.name, function, None) {
                self.row(function, None, &[])?;
            } else {
                self.session.reporter.borrow_mut().discard();
            }
            return Ok(());
        };
        if rows.is_empty() {
            self.host.say(
                self.engine,
                Message::new(
                    MessageKind::Warning,
                    format!("no data supplied for {function}() by {source}()"),
                ),
            );
        }
        self.session.reporter.borrow_mut().discard();
        for (n, row) in rows.into_iter().enumerate() {
            if !self.engine.is_alive(self.case) {
                break;
            }
            let tag = self
                .engine
                .js_member(&row, "tag")
                .filter(|t| !t.is_nullish())
                .map_or_else(
                    || format!("row {n}"),
                    |t| self.engine.to_js_string(&t).to_string(),
                );
            if self.session.takes_row(self.name, function, Some(&tag)) {
                self.begin(function, Some(&tag));
                self.row(function, Some(&tag), std::slice::from_ref(&row))?;
            }
        }
        Ok(())
    }

    /// One run of a test function, between `init` and `cleanup`. The
    /// report has been told that it begins.
    fn row(&self, function: &str, tag: Option<&str>, args: &[Value]) -> Result<(), EngineError> {
        self.host.state.borrow_mut().tag = tag.map(str::to_owned);
        // The function runs after a failing `init` too, as in Qt.
        self.call("init", &[])?;
        self.call(function, args)?;
        let pending = self.host.state.borrow_mut().expect.take().is_some();
        if pending && !self.has_failed() {
            self.fail(
                "QEXPECT_FAIL was called without any subsequent verification statements",
                None,
            );
        }
        self.call("cleanup", &[])?;
        self.host.drop_temporaries(self.engine, false);
        self.host.settle(self.engine)?;
        self.end();
        Ok(())
    }
}

/// The test cases below `root`, `root` itself included, in the order they
/// are written in.
fn test_cases(engine: &Engine, root: ObjectId) -> Vec<ObjectId> {
    fn walk(engine: &Engine, obj: ObjectId, out: &mut Vec<ObjectId>) {
        let is_case = engine
            .meta_of(obj)
            .is_some_and(|m| m.inherits(TestCase::static_meta()));
        if is_case {
            out.push(obj);
        }
        for child in engine.children(obj) {
            walk(engine, child, out);
        }
    }
    let mut out = Vec::new();
    walk(engine, root, &mut out);
    out
}

/// The test functions of a case: the functions written in QML whose names
/// start with `test_`, without the ones that give data, in the order of
/// their names.
fn test_functions(engine: &Engine, case: ObjectId) -> Vec<String> {
    let Some(meta) = engine.meta_of(case) else {
        return Vec::new();
    };
    let mut names: Vec<String> = meta
        .chain()
        .flat_map(|m| m.own_methods())
        .filter(|m| m.thunk.is_none())
        .map(|m| m.name.to_string())
        .filter(|n| n.starts_with("test_") && !n.ends_with("_data"))
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_names_a_function_of_a_case_and_perhaps_a_row() {
        let f = Filter::parse("One::test_b:high");
        assert!(f.takes_function("One", "test_b"));
        assert!(!f.takes_function("Two", "test_b"));
        assert!(!f.takes_function("One", "test_a"));
        assert!(f.takes_row("One", "test_b", Some("high")));
        assert!(!f.takes_row("One", "test_b", Some("low")));
        assert!(!f.takes_row("One", "test_b", None));

        let f = Filter::parse("One::test_a");
        assert!(f.takes_row("One", "test_a", None));
        assert!(f.takes_row("One", "test_a", Some("any")));
    }

    #[test]
    fn a_bare_name_is_a_case_or_a_function() {
        let f = Filter::parse("Two");
        assert!(f.takes_function("Two", "test_a"));
        assert!(f.takes_row("Two", "test_a", Some("any")));
        assert!(!f.takes_function("One", "test_a"));

        let f = Filter::parse("test_a");
        assert!(f.takes_function("One", "test_a"));
        assert!(f.takes_function("Two", "test_a"));
        assert!(!f.takes_function("Two", "test_b"));

        let f = Filter::parse("test_b:high");
        assert!(f.takes_row("One", "test_b", Some("high")));
        assert!(!f.takes_row("One", "test_b", Some("low")));
    }
}
