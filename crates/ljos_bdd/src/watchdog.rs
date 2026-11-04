//! A deadline for every scenario, so that a run never hangs its job.
//!
//! Scenarios run one at a time on the main thread, and a step that blocks
//! (a wait that never ends, a lock never let go, a process that never
//! answers) blocks the whole run with it. Nothing on that thread can notice,
//! so a thread of its own keeps the time: when a scenario runs past
//! `LJOS_BDD_SCENARIO_DEADLINE` seconds it says which scenario and which step
//! on stderr and ends the process with [`OVERRUN_EXIT`]. It cannot fail just
//! that scenario and go on, as the step that blocks never gives the thread
//! back. The end of the run (web pages closing, CEF shutting down) has a
//! deadline of its own.
//!
//! The step that runs is told by the steps themselves: [`watched_steps`]
//! registers every step through one function that tells the watchdog the
//! step's line before it calls the step. (cucumber's writer is no use for
//! this: it is handed a scenario's step events only once the scenario has
//! finished.)

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, PoisonError},
    time::{Duration, Instant},
};

use cucumber::{
    codegen::{LocalBoxFuture, StepConstructor as _, WorldInventory},
    step,
};

/// What a scenario that runs past its deadline ends the process with, as
/// `timeout(1)` does.
pub const OVERRUN_EXIT: i32 = 124;

/// The variable that sets the deadline, in seconds; `0` sets none.
pub const TIMEOUT_VAR: &str = "LJOS_BDD_SCENARIO_DEADLINE";

/// The slowest scenarios of the ledger take seconds, so this is far past
/// any that is merely slow: a deadline is for a run that hangs.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// The deadline `LJOS_BDD_SCENARIO_DEADLINE` asks for, `None` for none.
///
/// # Errors
/// When the variable is set to something other than whole seconds.
pub fn timeout_from_env() -> Result<Option<Duration>, String> {
    parse_timeout(std::env::var(TIMEOUT_VAR).ok().as_deref())
}

fn parse_timeout(value: Option<&str>) -> Result<Option<Duration>, String> {
    match value.map(str::trim) {
        None | Some("") => Ok(Some(DEFAULT_TIMEOUT)),
        Some(text) => match text.parse::<u64>() {
            Ok(0) => Ok(None),
            Ok(secs) => Ok(Some(Duration::from_secs(secs))),
            Err(_) => Err(format!(
                "{TIMEOUT_VAR}={text:?} is not a whole number of seconds"
            )),
        },
    }
}

/// One step of a scenario, as the report names it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    line: usize,
    keyword: String,
    text: String,
}

/// What runs now.
#[derive(Clone, Debug)]
enum Running {
    Scenario {
        feature: PathBuf,
        line: usize,
        keyword: String,
        name: String,
        /// Backgrounds first, then the scenario's own.
        steps: Vec<Step>,
        /// The index of the step that runs or ran last; `steps.len()`
        /// before the first.
        at: usize,
        /// Whether that step still runs.
        in_step: bool,
    },
    /// Something between or after scenarios, such as closing web pages.
    Phase(String),
}

#[derive(Default)]
struct State {
    running: Option<(Running, Instant, Duration)>,
    closed: bool,
}

type Overrun = Box<dyn Fn(&str) + Send + Sync>;

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    root: PathBuf,
    deadline: Duration,
    overrun: Overrun,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Keeps the time of every scenario. Cloning it shares the same clock.
#[derive(Clone)]
pub struct Watchdog {
    shared: Option<Arc<Shared>>,
}

impl std::fmt::Debug for Watchdog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watchdog")
            .field("deadline", &self.shared.as_ref().map(|s| s.deadline))
            .finish_non_exhaustive()
    }
}

impl Watchdog {
    /// A watchdog that never fires.
    #[must_use]
    pub fn off() -> Self {
        Self { shared: None }
    }

    /// Fires `deadline` after a scenario or phase starts, if it has not
    /// finished: prints which on stderr and ends the process with
    /// [`OVERRUN_EXIT`]. Paths are shown relative to `root`.
    #[must_use]
    pub fn start(deadline: Duration, root: &Path) -> Self {
        Self::with_overrun(
            deadline,
            root,
            Box::new(|report| {
                eprintln!("{report}");
                exit_now(OVERRUN_EXIT);
            }),
        )
    }

    /// [`Watchdog::start`] that calls `overrun` with the report instead of
    /// ending the process. After it returns, the scenario no longer counts.
    #[must_use]
    pub fn with_overrun(deadline: Duration, root: &Path, overrun: Overrun) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            root: root.to_path_buf(),
            deadline,
            overrun,
        });
        let keeper = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("ljos-bdd-watchdog".to_owned())
            .spawn(move || keep_time(&keeper))
            .expect("start the scenario watchdog");
        Self {
            shared: Some(shared),
        }
    }

    fn set(&self, running: Option<Running>, deadline: Option<Duration>) {
        let Some(shared) = &self.shared else { return };
        let mut state = shared.lock();
        state.running = running.map(|r| (r, Instant::now(), deadline.unwrap_or(shared.deadline)));
        shared.changed.notify_all();
    }

    /// A scenario starts: its clock starts now.
    pub fn scenario(
        &self,
        feature: &gherkin::Feature,
        rule: Option<&gherkin::Rule>,
        scenario: &gherkin::Scenario,
    ) {
        if self.shared.is_none() {
            return;
        }
        let step = |s: &gherkin::Step| Step {
            line: s.position.line,
            keyword: s.keyword.trim().to_owned(),
            text: s.value.clone(),
        };
        let steps: Vec<Step> = [
            feature.background.as_ref(),
            rule.and_then(|r| r.background.as_ref()),
        ]
        .into_iter()
        .flatten()
        .flat_map(|b| &b.steps)
        .chain(&scenario.steps)
        .map(step)
        .collect();
        self.set(
            Some(Running::Scenario {
                feature: feature.path.clone().unwrap_or_default(),
                line: scenario.position.line,
                keyword: scenario.keyword.clone(),
                name: scenario.name.clone(),
                at: steps.len(),
                in_step: false,
                steps,
            }),
            None,
        );
    }

    /// Something other than a scenario starts, with a deadline of its own.
    pub fn phase(&self, what: &str, deadline: Duration) {
        self.set(Some(Running::Phase(what.to_owned())), Some(deadline));
    }

    /// Nothing runs now.
    pub fn idle(&self) {
        self.set(None, None);
    }

    /// The step at `line` of the scenario that runs has started.
    fn step_started(&self, line: usize) {
        let Some(shared) = &self.shared else { return };
        let mut state = shared.lock();
        if let Some((
            Running::Scenario {
                steps, at, in_step, ..
            },
            _,
            _,
        )) = &mut state.running
        {
            *in_step = true;
            // Several steps on one line cannot be told apart, so the next
            // one on it after the last is taken.
            let after = if *at < steps.len() { *at + 1 } else { 0 };
            if let Some(i) = (after..steps.len())
                .chain(0..after)
                .find(|&i| steps[i].line == line)
            {
                *at = i;
            }
        }
    }

    /// The step that ran has returned.
    fn step_finished(&self) {
        let Some(shared) = &self.shared else { return };
        if let Some((Running::Scenario { in_step, .. }, _, _)) = &mut shared.lock().running {
            *in_step = false;
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        // The last one stops the thread. (One kept by `watched_steps` lives
        // as long as its thread, and so does the watchdog's: it sleeps.)
        if let Some(shared) = &self.shared
            && Arc::strong_count(shared) == 2
        {
            shared.lock().closed = true;
            shared.changed.notify_all();
        }
    }
}

fn keep_time(shared: &Shared) {
    let mut state = shared.lock();
    loop {
        if state.closed {
            return;
        }
        let Some((running, since, deadline)) = &state.running else {
            state = shared
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
            continue;
        };
        let elapsed = since.elapsed();
        if elapsed < *deadline {
            let left = *deadline - elapsed;
            state = shared
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
            continue;
        }
        let report = report(running, *deadline, &shared.root);
        state.running = None;
        drop(state);
        (shared.overrun)(&report);
        state = shared.lock();
    }
}

/// What the watchdog prints: the first line says what happened, the next
/// say where, as `path:line` so that an editor or CI log links them.
fn report(running: &Running, deadline: Duration, root: &Path) -> String {
    let secs = deadline.as_secs();
    match running {
        Running::Scenario {
            feature,
            line,
            keyword,
            name,
            steps,
            at,
            in_step,
        } => {
            let path = feature.strip_prefix(root).unwrap_or(feature).display();
            let mut out = format!(
                "ljos-bdd: a scenario ran past its deadline of {secs} s ({TIMEOUT_VAR}) and failed; \
                 the run ends here\n  scenario: {path}:{line}: {keyword}: {name}\n"
            );
            let step = |s: &Step| {
                format!(
                    "{} of {}: {path}:{}: {} {}",
                    at + 1,
                    steps.len(),
                    s.line,
                    s.keyword,
                    s.text
                )
            };
            match (steps.get(*at), in_step) {
                (Some(s), true) => out.push_str(&format!("  step {}", step(s))),
                (Some(s), false) => out.push_str(&format!(
                    "  after step {}\n  (no step runs: its World is being dropped, \
                     or the next scenario's made)",
                    step(s)
                )),
                (None, _) => out.push_str("  before its first step (its backends starting)"),
            }
            out
        }
        Running::Phase(what) => {
            format!("ljos-bdd: {what} ran past its deadline of {secs} s; the run ends here")
        }
    }
}

/// Ends the process at once: exit handlers are not run, as one that waits
/// on what hangs (a web engine's threads) would hang the exit as well.
fn exit_now(code: i32) -> ! {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    #[cfg(unix)]
    // SAFETY: `_exit` takes any status and does not return.
    unsafe {
        libc::_exit(code)
    }
    #[cfg(not(unix))]
    std::process::exit(code)
}

thread_local! {
    /// The steps as registered, by World, for [`watched`] to call; and the
    /// watchdog it tells.
    static WATCHED: RefCell<HashMap<TypeId, Box<dyn Any>>> = RefCell::new(HashMap::new());
}

struct Watched<W> {
    dog: Watchdog,
    steps: step::Collection<W>,
    /// The step each text found, as finding one tries every expression.
    found: HashMap<(u8, String), step::Step<W>>,
}

/// Every step of `W`, each registered through [`watched`], which tells
/// `dog` that it starts and then calls it, on this thread. A step matches
/// as it would have, ambiguities and all: the expressions are the same.
#[must_use]
pub fn watched_steps<W: WorldInventory + std::fmt::Debug + 'static>(
    dog: &Watchdog,
) -> step::Collection<W> {
    let mut out = step::Collection::new();
    for s in cucumber::codegen::inventory::iter::<W::Given> {
        let (loc, regex, _) = s.inner();
        out = out.given(Some(loc), regex(), watched::<W>);
    }
    for s in cucumber::codegen::inventory::iter::<W::When> {
        let (loc, regex, _) = s.inner();
        out = out.when(Some(loc), regex(), watched::<W>);
    }
    for s in cucumber::codegen::inventory::iter::<W::Then> {
        let (loc, regex, _) = s.inner();
        out = out.then(Some(loc), regex(), watched::<W>);
    }
    let watched = Watched {
        dog: dog.clone(),
        steps: W::collection(),
        found: HashMap::new(),
    };
    WATCHED.with(|w| w.borrow_mut().insert(TypeId::of::<W>(), Box::new(watched)));
    out
}

fn watched<W: 'static>(world: &mut W, ctx: step::Context) -> LocalBoxFuture<'_, ()> {
    let kind = match ctx.step.ty {
        gherkin::StepType::Given => 0,
        gherkin::StepType::When => 1,
        gherkin::StepType::Then => 2,
    };
    let found = WATCHED.with(|w| {
        let mut w = w.borrow_mut();
        let w = w
            .get_mut(&TypeId::of::<W>())?
            .downcast_mut::<Watched<W>>()?;
        w.dog.step_started(ctx.step.position.line);
        let key = (kind, ctx.step.value.clone());
        if let Some(f) = w.found.get(&key) {
            return Some((*f, w.dog.clone()));
        }
        let f = *w.steps.find(&ctx.step).ok()??.0;
        w.found.insert(key, f);
        Some((f, w.dog.clone()))
    });
    match found {
        Some((f, dog)) => Box::pin(async move {
            f(world, ctx).await;
            dog.step_finished();
        }),
        // Only a step that matched reaches here, and the same expressions
        // found it.
        None => panic!("no step matches {:?}", ctx.step.value),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn scenario(steps: &[&str]) -> Running {
        Running::Scenario {
            feature: PathBuf::from("/w/features/pdf/views.feature"),
            line: 12,
            keyword: "Scenario".into(),
            name: "A view opens".into(),
            steps: steps
                .iter()
                .enumerate()
                .map(|(i, s)| Step {
                    line: 13 + i,
                    keyword: "Given".into(),
                    text: (*s).into(),
                })
                .collect(),
            at: steps.len(),
            in_step: false,
        }
    }

    #[test]
    fn the_deadline_is_five_minutes_unless_set() {
        assert_eq!(parse_timeout(None), Ok(Some(DEFAULT_TIMEOUT)));
        assert_eq!(parse_timeout(Some("")), Ok(Some(DEFAULT_TIMEOUT)));
        assert_eq!(
            parse_timeout(Some(" 30 ")),
            Ok(Some(Duration::from_secs(30)))
        );
        assert_eq!(parse_timeout(Some("0")), Ok(None));
        assert!(parse_timeout(Some("5m")).is_err());
    }

    fn at(running: &mut Running, step: usize, running_now: bool) {
        if let Running::Scenario { at, in_step, .. } = running {
            *at = step;
            *in_step = running_now;
        }
    }

    #[test]
    fn the_report_names_the_file_line_scenario_and_step() {
        let mut running = scenario(&["a", "b", "c"]);
        at(&mut running, 1, true);
        let text = report(&running, Duration::from_secs(600), Path::new("/w"));
        assert_eq!(
            text,
            "ljos-bdd: a scenario ran past its deadline of 600 s (LJOS_BDD_SCENARIO_DEADLINE) and \
             failed; the run ends here\n  scenario: features/pdf/views.feature:12: Scenario: A \
             view opens\n  step 2 of 3: features/pdf/views.feature:14: Given b"
        );
    }

    #[test]
    fn a_scenario_between_steps_says_after_which() {
        let mut running = scenario(&["a"]);
        at(&mut running, 0, false);
        let text = report(&running, Duration::from_secs(1), Path::new("/w"));
        assert!(
            text.ends_with(
                "  after step 1 of 1: features/pdf/views.feature:13: Given a\n  (no step runs: \
                 its World is being dropped, or the next scenario's made)"
            ),
            "{text}"
        );
        let text = report(&scenario(&["a"]), Duration::from_secs(1), Path::new("/w"));
        assert!(
            text.ends_with("  before its first step (its backends starting)"),
            "{text}"
        );
    }

    fn firing(deadline: Duration) -> (Watchdog, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let dog = Watchdog::with_overrun(
            deadline,
            Path::new("/w"),
            Box::new(move |r| {
                let _ = tx.lock().unwrap().send(r.to_owned());
            }),
        );
        (dog, rx)
    }

    #[test]
    fn a_phase_past_its_deadline_fires_once() {
        let (dog, rx) = firing(Duration::from_secs(3600));
        dog.phase("closing web pages", Duration::from_millis(20));
        let report = rx.recv_timeout(Duration::from_secs(30)).expect("it fires");
        assert_eq!(
            report,
            "ljos-bdd: closing web pages ran past its deadline of 0 s; the run ends here"
        );
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn the_next_scenario_starts_a_new_clock() {
        let (dog, rx) = firing(Duration::from_millis(300));
        dog.set(Some(scenario(&["a"])), None);
        std::thread::sleep(Duration::from_millis(200));
        dog.set(Some(scenario(&["b"])), None);
        std::thread::sleep(Duration::from_millis(200));
        dog.idle();
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn the_step_that_runs_is_told_by_its_line() {
        let (dog, rx) = firing(Duration::from_millis(50));
        dog.set(Some(scenario(&["a", "b", "c"])), None);
        dog.step_started(13);
        dog.step_finished();
        dog.step_started(15);
        let report = rx.recv_timeout(Duration::from_secs(30)).expect("it fires");
        assert!(
            report.ends_with("step 3 of 3: features/pdf/views.feature:15: Given c"),
            "{report}"
        );
    }
}
