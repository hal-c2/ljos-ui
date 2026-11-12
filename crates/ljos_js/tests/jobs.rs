//! The host's cue that promise jobs wait ([`HostObjects::jobs_queued`]):
//! nothing runs them unless the host schedules a pump, and a host asleep
//! must be woken only when there is one.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ljos_js::{HostError, HostObjects, HostTypeId, JsConfig, JsHost, JsValue, Scope};

/// A host with no objects that counts its cues.
#[derive(Default)]
struct Cues(Cell<u32>);

impl HostObjects for Cues {
    fn type_of(&self, _obj: u64) -> Option<HostTypeId> {
        None
    }

    fn get<'js>(&self, _scope: &Scope<'js>, _obj: u64, _prop: u32) -> Result<JsValue, HostError> {
        Ok(JsValue::Undefined)
    }

    fn set<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _prop: u32,
        _value: JsValue,
    ) -> Result<(), HostError> {
        Ok(())
    }

    fn call<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _method: u32,
        _args: Vec<JsValue>,
    ) -> Result<JsValue, HostError> {
        Ok(JsValue::Undefined)
    }

    fn jobs_queued(&self) {
        self.0.set(self.0.get() + 1);
    }
}

fn host() -> (JsHost, Rc<Cues>) {
    let host = JsHost::new(JsConfig::default()).unwrap();
    let cues = Rc::new(Cues::default());
    host.set_host_objects(cues.clone());
    (host, cues)
}

fn run(host: &JsHost, src: &str) {
    host.enter("t", host.config().slice, |s| s.eval::<(), _>(src))
        .unwrap();
}

#[test]
fn a_run_that_queues_no_job_gives_no_cue() {
    let (host, cues) = host();
    run(&host, "globalThis.n = 1 + 1");
    assert_eq!(cues.0.get(), 0);
}

#[test]
fn a_run_that_queues_a_job_gives_one_cue_when_it_ends() {
    let (host, cues) = host();
    // A nested run is part of the outer one: its jobs are told once, when
    // the outer one ends.
    host.enter("outer", host.config().slice, |_| {
        host.enter("inner", host.config().slice, |s| {
            s.eval::<(), _>("Promise.resolve().then(() => globalThis.hit = 1)")
        })
        .unwrap();
        assert_eq!(cues.0.get(), 0);
        Ok(())
    })
    .unwrap();
    assert_eq!(cues.0.get(), 1);
    assert!(host.has_pending_jobs());
    let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(1));
    assert!(pumped.faults.is_empty());
    assert!(!host.has_pending_jobs());
    // Drained: no cue for a pump that leaves nothing.
    assert_eq!(cues.0.get(), 1);
}

#[test]
fn a_pump_out_of_time_gives_a_cue_for_what_is_left() {
    let (host, cues) = host();
    run(&host, "Promise.resolve().then(() => {})");
    assert_eq!(cues.0.get(), 1);
    // A deadline already passed runs nothing and leaves the job.
    assert_eq!(host.pump_jobs(Instant::now()).ran, 0);
    assert_eq!(cues.0.get(), 2);
}
