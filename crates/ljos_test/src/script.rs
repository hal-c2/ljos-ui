//! Ljos.Test (docs/adr/0005-ljos-test.md): tests written in TypeScript
//! that drive an application as an agent does.
//!
//! A test file runs in a QuickJS context of its own, never in the
//! application's engine. It imports `test` and `expect` from `ljos:test`
//! (the module in `script/prelude.js`) and acts on the application only
//! through the inspector's tools, which a [`TestHost`] answers. The runner
//! knows nothing of how: `ljos test` gives it a host that starts the
//! application in this process on a virtual clock, and erases the files'
//! types with the compiler, which this crate does not link (CLAUDE.md
//! rule 5).
//!
//! Every call into the host is synchronous, so a test's promises settle
//! when the context's jobs are run out; nothing waits on a wall clock. The
//! one wall clock is a guard: a test whose own script runs longer than
//! [`Options::script_limit`] is stopped and fails.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

use ljos_js::rquickjs::{
    self, CatchResultExt, CaughtError, Context, Ctx, Function, Object, Runtime, Value,
    context::EvalOptions,
};
use ljos_render::RgbaImage;
use serde_json::{Value as Json, json};

/// The name a test file imports the API by.
pub const MODULE: &str = "ljos:test";

/// What a test file's name ends with.
pub const SUFFIX: &str = ".test.ts";

/// The directory beside a test file its pictures are kept in.
pub const PICTURES: &str = "__pictures__";

/// How far a channel of a pixel may be from the reference's before the
/// pixel counts as different: what antialiasing on another machine moves.
const DEFAULT_TOLERANCE: u8 = 2;

const PRELUDE: &str = include_str!("script/prelude.js");

/// The types of [`MODULE`], for `tsc` and an editor: what an application
/// puts beside its tests as `ljos-test.d.ts`.
pub const DECLARATIONS: &str = include_str!("script/ljos-test.d.ts");

/// A module with its types erased, as the host's compiler makes it.
#[derive(Debug, Clone)]
pub struct Module {
    /// `(function (__ljos_imports) { … })`, line for line the file's.
    pub source: String,
    /// The keys of what it imports, in the order `__ljos_imports` holds
    /// them: [`MODULE`], or another file's key.
    pub imports: Vec<String>,
}

/// Why a module cannot be loaded, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    pub key: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

/// A tool's refusal: its message, and the inspector's code for it when it
/// gave one (`not_found`, `ambiguous`, …), which a script reads as
/// `error.code`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: Option<String>,
    pub message: String,
    /// What the refusal says beside its message, such as how many items an
    /// `ambiguous` one found (`{ "count": 3 }`): a script's `error.details`.
    pub details: Option<Json>,
}

impl From<String> for Refusal {
    fn from(message: String) -> Self {
        Refusal {
            code: None,
            message,
            details: None,
        }
    }
}

impl From<&str> for Refusal {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

/// What a test's application is: the runner asks it to start one and to
/// answer the inspector's tools on it.
pub trait TestHost {
    /// The module at `key` (a file's key from the suite's root), erased.
    ///
    /// # Errors
    ///
    /// The file cannot be read, or does not erase.
    fn load(&mut self, key: &str) -> Result<Module, LoadError>;

    /// Starts the application at `app`, a path from the test file `from`,
    /// in a window of the size given (the root's own when `None`). Gives
    /// back a number the other calls name it by.
    ///
    /// # Errors
    ///
    /// The application does not load; the message says why.
    fn launch(
        &mut self,
        from: &str,
        app: &str,
        width: Option<f64>,
        height: Option<f64>,
    ) -> Result<u32, String>;

    /// The answer of the inspector's `tool` on the application `app`:
    /// `{ "structured": …, "text": "…" }`.
    ///
    /// # Errors
    ///
    /// The tool refused; the message and the code are its own.
    fn call(&mut self, app: u32, tool: &str, args: Json) -> Result<Json, Refusal>;

    /// The application's clock, in milliseconds.
    ///
    /// # Errors
    ///
    /// There is no such application.
    fn now(&mut self, app: u32) -> Result<f64, String>;

    /// A PNG of the application's window, or of the one item `target`
    /// finds, as the inspector's `app_screenshot` gives it.
    ///
    /// # Errors
    ///
    /// The inspector refused; the message is its own.
    fn picture(&mut self, app: u32, target: Option<&str>) -> Result<Vec<u8>, String>;

    /// Where the picture `name` of the test file `from` is kept: in
    /// [`PICTURES`] beside the file.
    fn picture_path(&self, from: &str, name: &str) -> PathBuf;

    /// Ends every application started for the test that just ran.
    fn close_all(&mut self);
}

/// How a run goes.
#[derive(Debug, Clone)]
pub struct Options {
    /// Run only the tests whose full name (`file > describe > test`)
    /// contains one of these; all when empty.
    pub filters: Vec<String>,
    /// How long a test's own script may run, on the wall clock.
    pub script_limit: Duration,
    /// Write every picture a test compares as its new reference, rather
    /// than compare it.
    pub update_pictures: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            filters: Vec::new(),
            script_limit: Duration::from_secs(60),
            update_pictures: false,
        }
    }
}

/// How a test went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Skip,
    Fail {
        message: String,
        /// `file:line` of the test file where it failed.
        at: Option<String>,
    },
}

/// One test's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestResult {
    /// The test file's key.
    pub file: String,
    /// The names of the `describe` blocks it is in, then its own; empty
    /// for a file that did not load.
    pub name: Vec<String>,
    pub outcome: Outcome,
    /// What it printed with `console.log`.
    pub log: Vec<String>,
}

impl TestResult {
    /// `file > describe > test`.
    #[must_use]
    pub fn full_name(&self) -> String {
        let mut out = self.file.clone();
        for part in &self.name {
            out.push_str(" > ");
            out.push_str(part);
        }
        out
    }
}

/// Runs the test file `key`, telling `each` each result as it comes.
pub fn run_file(
    host: &Rc<RefCell<dyn TestHost>>,
    key: &str,
    options: &Options,
    each: &mut dyn FnMut(&TestResult),
) {
    match File::open(host, key, options) {
        Ok(file) => file.run(options, each),
        Err(error) => {
            let result = TestResult {
                file: key.to_string(),
                name: Vec::new(),
                outcome: Outcome::Fail {
                    message: error.message,
                    at: Some(format!("{}:{}:{}", error.key, error.line, error.column)),
                },
                log: Vec::new(),
            };
            each(&result);
        }
    }
    // What the file started ends with it, however it ended: a load that
    // failed, a list that could not be made, or tests that all were
    // skipped, none of which reach the close after each test.
    host.borrow_mut().close_all();
}

/// A test file in its own context.
struct File {
    key: String,
    // Holds the interrupt guard for as long as the context lives.
    _runtime: Runtime,
    context: Context,
    host: Rc<RefCell<dyn TestHost>>,
    /// When the running test's script is stopped.
    deadline: Rc<Cell<Option<Instant>>>,
    stopped: Rc<Cell<bool>>,
    log: Rc<RefCell<Vec<String>>>,
    update_pictures: bool,
}

fn load_error(key: &str, message: impl Into<String>) -> LoadError {
    LoadError {
        key: key.to_string(),
        line: 1,
        column: 1,
        message: message.into(),
    }
}

impl File {
    fn open(
        host: &Rc<RefCell<dyn TestHost>>,
        key: &str,
        options: &Options,
    ) -> Result<File, LoadError> {
        let runtime = Runtime::new().map_err(|e| load_error(key, e.to_string()))?;
        let deadline: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
        let stopped = Rc::new(Cell::new(false));
        {
            let (deadline, stopped) = (deadline.clone(), stopped.clone());
            runtime.set_interrupt_handler(Some(Box::new(move || {
                let over = deadline.get().is_some_and(|d| Instant::now() >= d);
                if over {
                    stopped.set(true);
                }
                over
            })));
        }
        let context = Context::full(&runtime).map_err(|e| load_error(key, e.to_string()))?;
        let file = File {
            key: key.to_string(),
            _runtime: runtime,
            context,
            host: host.clone(),
            deadline,
            stopped,
            log: Rc::new(RefCell::new(Vec::new())),
            update_pictures: options.update_pictures,
        };
        // Loading runs the file's top level, which the guard covers too.
        file.deadline
            .set(Some(Instant::now() + options.script_limit));
        let loaded = file.load();
        file.deadline.set(None);
        loaded.map(|()| file)
    }

    /// Erases the file and what it imports, runs their top levels, and
    /// keeps the API's functions as globals the runner calls.
    fn load(&self) -> Result<(), LoadError> {
        let mut modules: Vec<(String, Module)> = Vec::new();
        self.gather(&self.key, &mut modules, &mut Vec::new())?;
        let host = self.host.clone();
        let key = self.key.clone();
        let update = self.update_pictures;
        self.context.with(|ctx| -> Result<(), LoadError> {
            let thrown = |e: CaughtError<'_>, at: &str| error_at(&e, at);
            let bridge = Function::new(ctx.clone(), move |op: String, arg: String| -> String {
                answer(&host, &op, &arg, update)
            })
            .map_err(|e| load_error(&key, e.to_string()))?;
            install_console(&ctx, self.log.clone()).map_err(|e| load_error(&key, e.to_string()))?;
            let prelude: Function = eval(&ctx, PRELUDE, MODULE)
                .catch(&ctx)
                .map_err(|e| thrown(e, MODULE))?;
            let made: Object = prelude
                .call((bridge, key.clone()))
                .catch(&ctx)
                .map_err(|e| thrown(e, MODULE))?;
            let api: Value = made
                .get("api")
                .map_err(|e| load_error(&key, e.to_string()))?;
            let globals = ctx.globals();
            for name in ["list", "start", "result"] {
                let f: Function = made
                    .get(name)
                    .map_err(|e| load_error(&key, e.to_string()))?;
                globals
                    .set(format!("__ljos_test_{name}"), f)
                    .map_err(|e| load_error(&key, e.to_string()))?;
            }
            // Each module once, after what it imports.
            let mut exports: HashMap<String, Value<'_>> = HashMap::new();
            exports.insert(MODULE.to_string(), api);
            for (module_key, module) in &modules {
                let made: Function = eval(&ctx, &module.source, module_key)
                    .catch(&ctx)
                    .map_err(|e| thrown(e, module_key))?;
                let imports = rquickjs::Array::new(ctx.clone())
                    .map_err(|e| load_error(module_key, e.to_string()))?;
                for (i, import) in module.imports.iter().enumerate() {
                    let value = exports.get(import).cloned().ok_or_else(|| {
                        load_error(module_key, format!("`{import}` was not loaded"))
                    })?;
                    imports
                        .set(i, value)
                        .map_err(|e| load_error(module_key, e.to_string()))?;
                }
                let gives: Value = made
                    .call((imports,))
                    .catch(&ctx)
                    .map_err(|e| thrown(e, module_key))?;
                exports.insert(module_key.clone(), gives);
            }
            Ok(())
        })
    }

    /// `key` and every file it reaches, each after what it imports.
    fn gather(
        &self,
        key: &str,
        out: &mut Vec<(String, Module)>,
        visiting: &mut Vec<String>,
    ) -> Result<(), LoadError> {
        if key == MODULE || out.iter().any(|(k, _)| k == key) {
            return Ok(());
        }
        if let Some(at) = visiting.iter().position(|k| k == key) {
            let mut cycle = visiting[at..].to_vec();
            cycle.push(key.to_string());
            return Err(load_error(
                &visiting[at],
                format!(
                    "the modules import each other ({}): a module runs after what it imports",
                    cycle.join(" > ")
                ),
            ));
        }
        let module = self.host.borrow_mut().load(key)?;
        visiting.push(key.to_string());
        for import in &module.imports {
            self.gather(import, out, visiting)?;
        }
        visiting.pop();
        out.push((key.to_string(), module));
        Ok(())
    }

    fn run(self, options: &Options, each: &mut dyn FnMut(&TestResult)) {
        let listed = self.context.with(|ctx| -> Result<Vec<Listed>, String> {
            let list: Function = ctx
                .globals()
                .get("__ljos_test_list")
                .map_err(|e| e.to_string())?;
            let text: String = list.call(()).catch(&ctx).map_err(|e| e.to_string())?;
            Listed::all(&text)
        });
        let listed = match listed {
            Ok(listed) => listed,
            Err(why) => {
                // A file whose tests cannot be listed fails as a whole,
                // rather than passing with none.
                each(&TestResult {
                    file: self.key.clone(),
                    name: Vec::new(),
                    outcome: Outcome::Fail {
                        message: format!("the tests of {} cannot be listed: {why}", self.key),
                        at: None,
                    },
                    log: Vec::new(),
                });
                return;
            }
        };
        for (index, test) in listed.into_iter().enumerate() {
            let mut result = TestResult {
                file: self.key.clone(),
                name: test.name,
                outcome: Outcome::Skip,
                log: Vec::new(),
            };
            let name = result.full_name();
            if !options.filters.is_empty() && !options.filters.iter().any(|f| name.contains(f)) {
                continue;
            }
            if !test.skip {
                result.outcome = self.run_one(index, options);
                result.log = std::mem::take(&mut *self.log.borrow_mut());
            }
            each(&result);
        }
    }

    fn run_one(&self, index: usize, options: &Options) -> Outcome {
        self.stopped.set(false);
        self.deadline
            .set(Some(Instant::now() + options.script_limit));
        let outcome = self.context.with(|ctx| {
            let globals = ctx.globals();
            let start: Function = match globals.get("__ljos_test_start") {
                Ok(f) => f,
                Err(e) => return fail(e.to_string(), None),
            };
            if let Err(e) = start.call::<_, ()>((index,)).catch(&ctx) {
                return self.thrown(&e);
            }
            // The test's promises settle as its jobs run: every act it
            // awaits was done when it was called.
            while ctx.execute_pending_job() {}
            let result: Function = match globals.get("__ljos_test_result") {
                Ok(f) => f,
                Err(e) => return fail(e.to_string(), None),
            };
            let state: Json = result
                .call::<_, String>((index,))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or(Json::Null);
            if self.stopped.get() {
                return self.overran(options);
            }
            if state["done"] != json!(true) {
                return fail(
                    "the test did not finish: it awaits a promise nothing settles",
                    None,
                );
            }
            match state.get("error") {
                Some(error) => fail(
                    error["message"].as_str().unwrap_or("the test failed"),
                    error["at"].as_str().map(str::to_string),
                ),
                None => Outcome::Pass,
            }
        });
        self.deadline.set(None);
        self.host.borrow_mut().close_all();
        outcome
    }

    fn thrown(&self, e: &CaughtError<'_>) -> Outcome {
        if self.stopped.get() {
            return fail("the test's script was stopped", None);
        }
        let error = error_at(e, &self.key);
        fail(error.message, Some(format!("{}:{}", error.key, error.line)))
    }

    fn overran(&self, options: &Options) -> Outcome {
        fail(
            format!(
                "the test's own script ran for more than {} s and was stopped",
                options.script_limit.as_secs()
            ),
            None,
        )
    }
}

fn fail(message: impl Into<String>, at: Option<String>) -> Outcome {
    Outcome::Fail {
        message: message.into(),
        at,
    }
}

struct Listed {
    name: Vec<String>,
    skip: bool,
}

impl Listed {
    fn all(text: &str) -> Result<Vec<Listed>, String> {
        let Ok(Json::Array(tests)) = serde_json::from_str::<Json>(text) else {
            return Err(format!("the list is not an array of tests: {text}"));
        };
        Ok(tests
            .iter()
            .map(|t| Listed {
                name: t["name"]
                    .as_array()
                    .map(|parts| {
                        parts
                            .iter()
                            .map(|p| p.as_str().unwrap_or_default().to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
                skip: t["skip"] == json!(true),
            })
            .collect())
    }
}

fn eval<'js, V: rquickjs::FromJs<'js>>(
    ctx: &Ctx<'js>,
    source: &str,
    filename: &str,
) -> rquickjs::Result<V> {
    let mut options = EvalOptions::default();
    options.global = true;
    options.strict = true;
    options.filename = Some(filename.to_string());
    ctx.eval_with_options(source, options)
}

/// What a caught error says, at the first place in `key`'s files its stack
/// names, else at the top of `fallback`.
fn error_at(e: &CaughtError<'_>, fallback: &str) -> LoadError {
    let (message, stack) = match e {
        CaughtError::Exception(ex) => {
            let name: String = ex
                .get::<_, String>("name")
                .unwrap_or_else(|_| "Error".into());
            let message = ex.message().unwrap_or_default();
            let message = if name == "TestError" {
                message
            } else {
                format!("{name}: {message}")
            };
            (message, ex.stack().unwrap_or_default())
        }
        CaughtError::Value(v) => (format!("{v:?}"), String::new()),
        CaughtError::Error(e) => (e.to_string(), String::new()),
    };
    for line in stack.lines() {
        let line = line.trim().trim_end_matches(')');
        let Some(place) = line.rsplit(['(', ' ']).next() else {
            continue;
        };
        let mut parts = place.rsplitn(3, ':');
        let (Some(last), Some(middle)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (file, line_no, column) = match parts.next() {
            Some(file) => (file, middle, last),
            None => (middle, last, "1"),
        };
        if file == MODULE || file == "<native>" {
            continue;
        }
        if let (Ok(line), Ok(column)) = (line_no.parse(), column.parse()) {
            return LoadError {
                key: file.to_string(),
                line,
                column,
                message,
            };
        }
    }
    load_error(fallback, message)
}

/// `console.log` and its kin, into the running test's log.
fn install_console(ctx: &Ctx<'_>, log: Rc<RefCell<Vec<String>>>) -> rquickjs::Result<()> {
    let console = Object::new(ctx.clone())?;
    for name in ["log", "info", "warn", "error", "debug"] {
        let log = log.clone();
        let print = Function::new(
            ctx.clone(),
            move |args: rquickjs::function::Rest<rquickjs::convert::Coerced<String>>| {
                let line: Vec<String> = args.0.into_iter().map(|a| a.0).collect();
                log.borrow_mut().push(line.join(" "));
            },
        )?;
        console.set(name, print)?;
    }
    ctx.globals().set("console", console)
}

/// The host's answer to one call of the prelude, as JSON.
fn answer(host: &Rc<RefCell<dyn TestHost>>, op: &str, arg: &str, update: bool) -> String {
    let arg: Json = serde_json::from_str(arg).unwrap_or(Json::Null);
    let mut host = host.borrow_mut();
    let result = match op {
        "launch" => host
            .launch(
                arg["from"].as_str().unwrap_or_default(),
                arg["app"].as_str().unwrap_or_default(),
                arg["width"].as_f64(),
                arg["height"].as_f64(),
            )
            .map(Json::from),
        "call" => {
            let tool = arg["tool"].as_str().unwrap_or_default();
            match host.call(app_of(&arg), tool, arg["args"].clone()) {
                Ok(value) => Ok(value),
                Err(Refusal {
                    code: Some(code),
                    message,
                    details,
                }) => {
                    return json!({ "error": message, "code": code, "details": details })
                        .to_string();
                }
                Err(refusal) => Err(refusal.message),
            }
        }
        "now" => host.now(app_of(&arg)).map(Json::from),
        "picture" => match_picture(&mut *host, &arg, update),
        other => Err(format!("the runner has no `{other}`")),
    };
    match result {
        Ok(value) => json!({ "ok": value }).to_string(),
        Err(error) => json!({ "error": error }).to_string(),
    }
}

/// Compares the picture a test asked for with the one kept for it, and
/// writes it where there is none or the run was asked to.
fn match_picture(host: &mut dyn TestHost, arg: &Json, update: bool) -> Result<Json, String> {
    let name = arg["name"].as_str().unwrap_or_default();
    // `x.actual.png` is where a failed match of `x.png` is written, so it
    // names no reference: a reference by that name would be overwritten.
    let plain = Path::new(name)
        .file_name()
        .is_some_and(|f| f == name && name.ends_with(".png") && !name.ends_with(".actual.png"));
    if !plain {
        return Err(format!(
            "a picture is named by a file name that ends in `.png` but not `.actual.png`, with no directory: not {}",
            Json::from(name)
        ));
    }
    let from = arg["from"].as_str().unwrap_or_default();
    let png = host.picture(app_of(arg), arg["target"].as_str())?;
    let path = host.picture_path(from, name);
    let kept = format!("{PICTURES}/{name}");
    let seen_path = path.with_extension("actual.png");
    let stem = name.strip_suffix(".png").unwrap_or(name);
    let seen = format!("{PICTURES}/{stem}.actual.png");
    let write = |to: &Path| -> Result<(), String> {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("{} cannot be made: {e}", dir.display()))?;
        }
        std::fs::write(to, &png).map_err(|e| format!("{} cannot be written: {e}", to.display()))
    };
    if update {
        write(&path)?;
        // What a failed run left beside it is the reference now.
        let _ = std::fs::remove_file(&seen_path);
        return Ok(Json::Null);
    }
    let old = match std::fs::read(&path) {
        Ok(old) => old,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            write(&path)?;
            return Err(format!(
                "there was no {kept}; this run's picture is written there: look at it, and run again"
            ));
        }
        // A reference that is there but cannot be read is not replaced.
        Err(e) => return Err(format!("{kept} cannot be read: {e}")),
    };
    let now = RgbaImage::from_png(&png).map_err(|e| format!("the picture cannot be read: {e}"))?;
    let reference = RgbaImage::from_png(&old)
        .map_err(|e| format!("{kept} is not a PNG this runner reads: {e}"))?;
    let tolerance = arg["tolerance"]
        .as_u64()
        .map_or(DEFAULT_TOLERANCE, |t| u8::try_from(t).unwrap_or(u8::MAX));
    let allowed = arg["maxDiffPixels"].as_u64().unwrap_or(0);
    let why = if (now.width, now.height) == (reference.width, reference.height) {
        let differing = differing_pixels(&now, &reference, tolerance);
        if differing <= allowed {
            let _ = std::fs::remove_file(&seen_path);
            return Ok(Json::Null);
        }
        format!(
            "the picture differs from {kept} in {differing} pixel{} (at most {allowed} may)",
            if differing == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "the picture is {}x{}; {kept} is {}x{}",
            now.width, now.height, reference.width, reference.height
        )
    };
    write(&seen_path)?;
    Err(format!(
        "{why}. What this run saw is in {seen}; `ljos test --update-pictures` keeps it"
    ))
}

/// The pixels where a channel is more than `tolerance` from the other
/// picture's.
fn differing_pixels(a: &RgbaImage, b: &RgbaImage, tolerance: u8) -> u64 {
    let far = |p: &[u8; 4], q: &[u8; 4]| p.iter().zip(q).any(|(x, y)| x.abs_diff(*y) > tolerance);
    let count = a
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.data.as_chunks::<4>().0)
        .filter(|(p, q)| far(p, q))
        .count();
    u64::try_from(count).unwrap_or(u64::MAX)
}

fn app_of(arg: &Json) -> u32 {
    arg["app"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(u32::MAX)
}

// ------------------------------------------------------------- reports

/// The TAP version 14 report, written as results come.
#[derive(Debug, Default)]
pub struct Tap {
    count: usize,
    passed: usize,
    failed: usize,
    skipped: usize,
}

impl Tap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn header(&self) -> &'static str {
        "TAP version 14"
    }

    /// The lines of one result.
    pub fn result(&mut self, result: &TestResult) -> Vec<String> {
        self.count += 1;
        // Every line of a log entry is a comment, or a TAP reader would
        // take the second line of an entry for a test point.
        let mut lines: Vec<String> = result
            .log
            .iter()
            .flat_map(|l| l.split('\n'))
            .map(|l| format!("# {}", l.trim_end_matches('\r')))
            .collect();
        let name = tap_description(&result.full_name());
        match &result.outcome {
            Outcome::Pass => {
                self.passed += 1;
                lines.push(format!("ok {} - {name}", self.count));
            }
            Outcome::Skip => {
                self.skipped += 1;
                lines.push(format!("ok {} - {name} # SKIP", self.count));
            }
            Outcome::Fail { message, at } => {
                self.failed += 1;
                lines.push(format!("not ok {} - {name}", self.count));
                lines.push("  ---".into());
                lines.push(format!("  message: {}", yaml_scalar(message)));
                if let Some(at) = at {
                    lines.push(format!("  at: {at}"));
                }
                lines.push("  ...".into());
            }
        }
        lines
    }

    /// The plan and the totals.
    #[must_use]
    pub fn footer(&self) -> Vec<String> {
        vec![
            format!("1..{}", self.count),
            format!("# pass {}", self.passed),
            format!("# fail {}", self.failed),
            format!("# skip {}", self.skipped),
        ]
    }

    #[must_use]
    pub fn failed(&self) -> usize {
        self.failed
    }
}

/// A test point's description: on one line, with `#` and `\` escaped as
/// TAP 14 asks, so a name never reads as a directive.
fn tap_description(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '#' => out.push_str("\\#"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// A YAML scalar for `text`: single-quoted when every character may stand
/// as it is, and double-quoted, with escapes, when one may not: a line
/// break, or a character YAML does not print (an ANSI colour's ESC).
fn yaml_scalar(text: &str) -> String {
    let printable = |c: char| {
        matches!(c, '\t' | ' '..='~' | '\u{85}')
            || (c >= '\u{A0}' && c != '\u{FEFF}' && c != '\u{FFFE}' && c != '\u{FFFF}')
    };
    if text.chars().all(printable) {
        return format!("'{}'", text.replace('\'', "''"));
    }
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if printable(c) => out.push(c),
            c => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
        }
    }
    out.push('"');
    out
}

/// The results as JUnit XML, one `testsuite` per file.
#[must_use]
pub fn junit(results: &[TestResult]) -> String {
    let mut files: Vec<&str> = Vec::new();
    for r in results {
        if !files.contains(&r.file.as_str()) {
            files.push(&r.file);
        }
    }
    let failures = |of: &[&TestResult]| {
        of.iter()
            .filter(|r| matches!(r.outcome, Outcome::Fail { .. }))
            .count()
    };
    let skips = |of: &[&TestResult]| of.iter().filter(|r| r.outcome == Outcome::Skip).count();
    let all: Vec<&TestResult> = results.iter().collect();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites tests=\"{}\" failures=\"{}\" skipped=\"{}\">",
        all.len(),
        failures(&all),
        skips(&all)
    );
    for file in files {
        let of: Vec<&TestResult> = results.iter().filter(|r| r.file == file).collect();
        let _ = writeln!(
            out,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" skipped=\"{}\">",
            xml(file),
            of.len(),
            failures(&of),
            skips(&of)
        );
        for r in of {
            let name = r.name.join(" > ");
            let _ = write!(
                out,
                "    <testcase classname=\"{}\" name=\"{}\"",
                xml(file),
                xml(&name)
            );
            match &r.outcome {
                Outcome::Pass => out.push_str("/>\n"),
                Outcome::Skip => out.push_str(">\n      <skipped/>\n    </testcase>\n"),
                Outcome::Fail { message, at } => {
                    let first = message.lines().next().unwrap_or_default();
                    let _ = writeln!(
                        out,
                        ">\n      <failure message=\"{}\">{}{}</failure>\n    </testcase>",
                        xml(first),
                        xml(message),
                        at.as_deref()
                            .map(|a| format!("\nat {}", xml(a)))
                            .unwrap_or_default()
                    );
                }
            }
        }
        out.push_str("  </testsuite>\n");
    }
    out.push_str("</testsuites>\n");
    out
}

fn xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // XML 1.0 has no way to write these, even escaped.
            '\t' | '\n' | '\r' => out.push(c),
            c if u32::from(c) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    out
}

/// The results as data:
///
/// ```json
/// { "totals": { "passed": 1, "failed": 1, "skipped": 0 },
///   "tests": [ { "file": "a.test.ts", "name": ["group", "test"],
///                "outcome": "fail", "message": "…", "at": "a.test.ts:4" } ] }
/// ```
#[must_use]
pub fn to_json(results: &[TestResult]) -> Json {
    let count = |want: fn(&Outcome) -> bool| results.iter().filter(|r| want(&r.outcome)).count();
    let tests: Vec<Json> = results
        .iter()
        .map(|r| {
            let mut o = json!({ "file": r.file, "name": r.name });
            match &r.outcome {
                Outcome::Pass => o["outcome"] = json!("pass"),
                Outcome::Skip => o["outcome"] = json!("skip"),
                Outcome::Fail { message, at } => {
                    o["outcome"] = json!("fail");
                    o["message"] = json!(message);
                    if let Some(at) = at {
                        o["at"] = json!(at);
                    }
                }
            }
            if !r.log.is_empty() {
                o["log"] = json!(r.log);
            }
            o
        })
        .collect();
    json!({
        "totals": {
            "passed": count(|o| *o == Outcome::Pass),
            "failed": count(|o| matches!(o, Outcome::Fail { .. })),
            "skipped": count(|o| *o == Outcome::Skip),
        },
        "tests": tests,
    })
}

#[cfg(test)]
mod tests;
