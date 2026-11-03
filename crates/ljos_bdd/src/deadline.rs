//! A deadline for each scenario, and for the runner's shutdown after the last one.
//!
//! Scenarios run one at a time on one thread, and a step that blocks (a child
//! that never answers, a library that never returns) cannot be interrupted
//! from inside. Without a deadline such a run sits silent until whatever
//! started it gives up, and nothing says which scenario it was. A thread of
//! its own watches the clock instead; when a scenario outlives its deadline
//! it names the scenario's file and line and ends the run with a failure.
//!
//! `LJOS_BDD_SCENARIO_DEADLINE` is the deadline in seconds; `0` turns it off.

use std::{
    io::Write as _,
    sync::{Arc, Condvar, Mutex, PoisonError},
    thread,
    time::{Duration, Instant},
};

/// When `LJOS_BDD_SCENARIO_DEADLINE` is not set. The slowest scenarios of the
/// ledger take seconds, so this is far past any that is merely slow.
pub const DEFAULT: Duration = Duration::from_secs(300);

#[derive(Default)]
struct State {
    /// What is running now, and since when.
    running: Option<(String, Instant)>,
    /// Counts every change, so the watcher can tell a new scenario under
    /// the same name from the one it was timing.
    generation: u64,
}

/// What a deadline does when it passes. The runner exits; a test records.
pub type OnExpiry = Box<dyn Fn(&str, Duration) + Send + 'static>;

/// Times what runs now against one deadline.
#[derive(Clone)]
pub struct Watchdog {
    shared: Arc<(Mutex<State>, Condvar)>,
}

impl Watchdog {
    /// The deadline `LJOS_BDD_SCENARIO_DEADLINE` asks for, or [`DEFAULT`].
    /// `None` when it is `0`.
    ///
    /// # Panics
    /// When the variable is set to something that is not a whole number of seconds.
    #[must_use]
    pub fn deadline_from_env() -> Option<Duration> {
        match std::env::var("LJOS_BDD_SCENARIO_DEADLINE") {
            Err(_) => Some(DEFAULT),
            Ok(v) => match v.trim().parse::<u64>() {
                Ok(0) => None,
                Ok(secs) => Some(Duration::from_secs(secs)),
                Err(_) => panic!("LJOS_BDD_SCENARIO_DEADLINE is seconds, not {v:?}"),
            },
        }
    }

    /// A watchdog that aborts the process, naming what ran past `deadline`.
    #[must_use]
    pub fn exiting(deadline: Duration) -> Self {
        Self::new(
            deadline,
            Box::new(|what, deadline| {
                // The scenario's own output is still buffered on stdout; it
                // comes first, so the report is the last thing the log shows.
                let _ = std::io::stdout().flush();
                eprintln!(
                    "\nscenario deadline: {what} has run for more than {} s; \
                     stopping the run (LJOS_BDD_SCENARIO_DEADLINE)",
                    deadline.as_secs()
                );
                // Not `exit`: what hangs may be a handler that `exit` would
                // run again, as a web engine's shutdown is.
                std::process::abort();
            }),
        )
    }

    /// A watchdog that calls `on_expiry` once for each thing that outlives `deadline`.
    #[must_use]
    pub fn new(deadline: Duration, on_expiry: OnExpiry) -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let watched = Arc::clone(&shared);
        thread::Builder::new()
            .name("scenario-deadline".into())
            .spawn(move || watch(&watched, deadline, &on_expiry))
            .expect("start the scenario deadline thread");
        Self { shared }
    }

    /// `what` starts now; its deadline replaces any other.
    pub fn start(&self, what: String) {
        self.set(Some((what, Instant::now())));
    }

    /// Nothing is timed until the next [`Watchdog::start`].
    pub fn stop(&self) {
        self.set(None);
    }

    fn set(&self, running: Option<(String, Instant)>) {
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap_or_else(PoisonError::into_inner);
        state.running = running;
        state.generation += 1;
        changed.notify_all();
    }
}

impl std::fmt::Debug for Watchdog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        f.debug_struct("Watchdog")
            .field("running", &state.running.as_ref().map(|(what, _)| what))
            .finish()
    }
}

fn watch(shared: &(Mutex<State>, Condvar), deadline: Duration, on_expiry: &OnExpiry) {
    let (lock, changed) = shared;
    let mut state = lock.lock().unwrap_or_else(PoisonError::into_inner);
    // The generation already reported, so one overrun is reported once.
    let mut reported = None;
    loop {
        let timed = match &state.running {
            Some((what, since)) if reported != Some(state.generation) => {
                Some((what.clone(), *since, state.generation))
            }
            _ => None,
        };
        match timed {
            None => {
                state = changed.wait(state).unwrap_or_else(PoisonError::into_inner);
            }
            Some((what, since, generation)) => {
                let left = deadline.saturating_sub(since.elapsed());
                if left.is_zero() {
                    reported = Some(generation);
                    drop(state);
                    on_expiry(&what, deadline);
                    state = lock.lock().unwrap_or_else(PoisonError::into_inner);
                } else {
                    state = changed
                        .wait_timeout(state, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn recording(deadline: Duration) -> (Watchdog, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let dog = Watchdog::new(
            deadline,
            Box::new(move |what, _| {
                let _ = tx.lock().unwrap().send(what.to_owned());
            }),
        );
        (dog, rx)
    }

    #[test]
    fn a_scenario_past_its_deadline_is_named() {
        let (dog, rx) = recording(Duration::from_millis(50));
        dog.start("features/a.feature:7 Hangs".into());
        let named = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(named, "features/a.feature:7 Hangs");
        // Once, not on every tick after.
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn scenarios_that_finish_in_time_are_not_named() {
        // Far longer than each scenario, so a loaded machine cannot make one late.
        let (dog, rx) = recording(Duration::from_secs(2));
        for i in 0..5 {
            dog.start(format!("features/a.feature:{i}"));
            thread::sleep(Duration::from_millis(20));
            dog.stop();
        }
        assert!(rx.recv_timeout(Duration::from_millis(2500)).is_err());
    }

    #[test]
    fn each_scenario_gets_its_own_deadline() {
        let (dog, rx) = recording(Duration::from_secs(2));
        // Together these outlive the deadline; neither comes near it alone.
        dog.start("features/a.feature:1".into());
        thread::sleep(Duration::from_millis(1200));
        dog.start("features/a.feature:9".into());
        thread::sleep(Duration::from_millis(1200));
        dog.stop();
        assert!(rx.recv_timeout(Duration::from_millis(2500)).is_err());
    }
}
