mod common;

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use common::*;
use ljos_js::{Bytecode, BytecodeOptions, JsConfig, JsFault, JsHost, JsValue, Slice, Timing};

fn plain() -> JsHost {
    JsHost::new(JsConfig::default()).unwrap()
}

fn eval_num(host: &JsHost, src: &str) -> Result<f64, JsFault> {
    host.enter("eval", host.config().slice, |s| s.eval::<f64, _>(src))
}

#[test]
fn eval_through_enter() {
    let host = plain();
    assert_eq!(eval_num(&host, "1 + 1").unwrap(), 2.0);
}

#[test]
fn infinite_loop_is_interrupted_and_context_survives() {
    let host = plain();
    let slice = Slice {
        soft: Duration::from_millis(2),
        hard: Duration::from_millis(20),
    };
    let t = Instant::now();
    let err = host
        .enter("spin", slice, |s| s.eval::<(), _>("while (true) {}"))
        .unwrap_err();
    let took = t.elapsed();
    assert!(matches!(err, JsFault::JsInterrupted { .. }), "{err:?}");
    assert!(took < Duration::from_millis(200), "interrupt took {took:?}");
    // The interrupt must not be catchable by script.
    let err = host
        .enter("spin2", slice, |s| {
            s.eval::<(), _>("try { while (true) {} } catch (e) {}")
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsInterrupted { .. }), "{err:?}");
    assert_eq!(eval_num(&host, "1 + 1").unwrap(), 2.0);
}

/// A debug build runs scripts several times slower, so the default slice
/// lets a dispatch that an optimised build finishes in time finish there
/// too: a handler that builds a board of objects is not cut off half way.
#[cfg(debug_assertions)]
#[test]
fn a_debug_build_does_not_interrupt_what_an_optimised_build_finishes() {
    let host = plain();
    let out = host.enter("handler", host.config().slice, |s| {
        s.eval::<f64, _>("const t = Date.now(); while (Date.now() - t < 150) {} 1")
    });
    assert_eq!(out.unwrap(), 1.0);
}

/// The limit read back is the one in force, also once it was changed.
#[test]
fn the_memory_limit_read_is_the_one_put_last() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    assert_eq!(host.memory_limit(), Some(16 * 1024 * 1024));
    assert!(host.set_memory_limit(Some(32 * 1024 * 1024)));
    assert_eq!(host.memory_limit(), Some(32 * 1024 * 1024));
    assert!(host.set_memory_limit(None));
    assert_eq!(host.memory_limit(), None);
}

/// `throw null` with room to spare stays an exception. With no room at all,
/// the run fails before the script throws anything, and that is running
/// out of memory.
#[test]
fn a_run_with_no_room_to_compile_is_running_out_of_memory() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    let err = eval_num(&host, "throw null").unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    // Not a byte to spare: a kilobyte more holds what `throw null` compiles
    // to, and the script's own null is then its exception.
    assert!(host.set_memory_limit(Some(held)));
    let err = eval_num(&host, "throw null").unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    // With room again, scripts run.
    assert!(host.set_memory_limit(None));
    assert_eq!(eval_num(&host, "40 + 2").unwrap(), 42.0);
}

fn small(limit_mib: usize) -> JsHost {
    JsHost::new(JsConfig {
        memory_limit: Some(limit_mib * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap()
}

const MIB: usize = 1024 * 1024;

/// Keeps about `mib` MiB alive in `globalThis.kept`, a run at a time.
fn keep(host: &JsHost, mib: usize) {
    keep_bytes(host, mib * MIB, 64);
}

/// Keeps `bytes` alive in `globalThis.kept`, `arrays` 1000-element arrays
/// (about 16 KiB each) a run.
fn keep_bytes(host: &JsHost, bytes: usize, arrays: usize) {
    host.enter("keep", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.kept = []")
    })
    .unwrap();
    let run = format!("for (let i = 0; i < {arrays}; i++) kept.push(new Array(1000).fill(i))");
    while host.heap_bytes() < bytes {
        host.enter("keep", host.config().slice, |s| {
            s.eval::<(), _>(run.as_str())
        })
        .unwrap();
    }
}

/// QuickJS collects when the heap is half again what lived after the last
/// collection. Past two thirds of the limit that is past the limit, and
/// garbage alone used to fill the heap until an allocation failed with
/// most of it free (the 96 MiB the HAL-C2 sidebar ran out of). The
/// collector now also runs halfway to the limit.
#[test]
fn garbage_past_two_thirds_of_the_limit_is_collected_before_the_heap_runs_out() {
    let host = small(32);
    keep(&host, 22);
    host.collect_now();
    assert!(host.heap_bytes() > 21 * MIB, "{}", host.heap_bytes());
    let slice = Slice {
        soft: Duration::from_secs(5),
        hard: Duration::from_secs(20),
    };
    // About 50 MiB of arrays made and dropped in one run, each holding
    // itself: refcounts free what holds nothing, and only the collector
    // frees a cycle, as closures and QML's back-references make.
    let out = host.enter("churn", slice, |s| {
        s.eval::<f64, _>(
            "(() => { let n = 0; for (let i = 0; i < 3000; i++) { const a = new Array(1000).fill(i); a.self = a; n += a.length; } return n; })()",
        )
    });
    assert_eq!(out.unwrap(), 3_000_000.0);
}

/// The collector runs at least a 64th of the limit apart: a heap that
/// lives within that of the limit runs out, rather than collecting every
/// few KiB until the slice stops the run.
#[test]
fn garbage_churned_on_a_nearly_full_heap_runs_out_rather_than_collecting_each_step() {
    let host = small(32);
    keep_bytes(&host, 32 * MIB - MIB / 4, 4);
    host.collect_now();
    assert!(
        host.heap_bytes() > 32 * MIB - MIB / 2,
        "{}",
        host.heap_bytes()
    );
    let slice = Slice {
        soft: Duration::from_secs(1),
        hard: Duration::from_secs(2),
    };
    let err = host
        .enter("churn", slice, |s| {
            s.eval::<(), _>("for (;;) { const a = { b: [1, 2, 3] }; a.self = a }")
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
}

/// A heap that lives at 94% of the limit still has room for garbage: the
/// collector runs halfway to the limit, a MiB on, not only past the limit
/// as QuickJS would, nor every few KiB.
#[test]
fn garbage_churned_on_a_heap_at_94_percent_is_collected_as_it_goes() {
    let host = small(32);
    keep(&host, 30);
    host.collect_now();
    let live = host.heap_bytes();
    assert!(live > 29 * MIB && live < 31 * MIB, "{live}");
    let slice = Slice {
        soft: Duration::from_secs(5),
        hard: Duration::from_secs(20),
    };
    // About 16 MiB of self-holding arrays, eight times the room left.
    let out = host.enter("churn", slice, |s| {
        s.eval::<f64, _>(
            "(() => { let n = 0; for (let i = 0; i < 1000; i++) { const a = new Array(1000).fill(i); a.self = a; n += a.length; } return n; })()",
        )
    });
    assert_eq!(out.unwrap(), 1_000_000.0);
}

/// A run that found no room leaves its garbage collected, so the next run
/// does not meet the same full heap, whoever it belongs to.
#[test]
fn a_run_that_runs_out_of_heap_leaves_its_garbage_collected() {
    let host = small(8);
    let err = host
        .enter("hog", host.config().slice, |s| {
            s.eval::<(), _>(
                "(function(){ const a = []; a.self = a; for (;;) a.push(new Array(1000).fill(1)); })()",
            )
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    assert!(host.heap_bytes() < 4 * MIB, "{}", host.heap_bytes());
}

/// What lived when the heap was last collected and was let go of since as
/// cycles is garbage the heap is no bigger for. A run that finds no room
/// for a string (which never asks QuickJS to collect) still leaves it
/// collected, so the next run has room.
#[test]
fn a_run_that_runs_out_after_cycles_were_let_go_leaves_them_collected() {
    let host = small(16);
    keep(&host, 9);
    host.enter("tie", host.config().slice, |s| {
        s.eval::<(), _>("kept.self = kept")
    })
    .unwrap();
    assert!(host.collect_now());
    let held = host.heap_bytes();
    host.enter("drop", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.kept = null")
    })
    .unwrap();
    assert!(host.heap_bytes() >= held, "the cycles are still there");
    let fill = "globalThis.s = 'x'.repeat(8 * 1024 * 1024)";
    let err = host
        .enter("fill", host.config().slice, |s| s.eval::<(), _>(fill))
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    host.enter("fill", host.config().slice, |s| s.eval::<(), _>(fill))
        .unwrap();
}

/// A run that only drops the last reference to cycles frees nothing and
/// ends at its peak, but what it let go of is garbage: a run that then
/// finds no room still leaves it collected, so the next run has room.
#[test]
fn a_run_out_after_a_run_that_only_dropped_cycles_collects_them() {
    let host = small(16);
    keep(&host, 9);
    host.enter("tie", host.config().slice, |s| {
        s.eval::<(), _>("kept.self = kept; globalThis.drop = () => { globalThis.kept = null }")
    })
    .unwrap();
    assert!(host.collect_now());
    let held = host.heap_bytes();
    let collections = host.collections();
    // A call, not an eval: compiling one frees what the compiler used.
    host.enter("drop", host.config().slice, |s| {
        s.globals()
            .get::<_, ljos_js::rquickjs::Function>("drop")?
            .call::<_, ()>(())
    })
    .unwrap();
    assert_eq!(host.heap_bytes(), held, "it freed nothing");
    assert!(host.may_hold_garbage());
    let fill = "globalThis.s = 'x'.repeat(8 * 1024 * 1024)";
    let err = host
        .enter("fill", host.config().slice, |s| s.eval::<(), _>(fill))
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    assert_eq!(host.collections(), collections + 1);
    assert!(host.heap_bytes() < held / 2, "{}", host.heap_bytes());
    host.enter("fill", host.config().slice, |s| s.eval::<(), _>(fill))
        .unwrap();
}

/// The same of a promise job: one that only drops the last reference to
/// cycles frees nothing, but a run that then finds no room still leaves
/// them collected.
#[test]
fn a_run_out_after_a_job_that_only_dropped_cycles_collects_them() {
    let host = small(16);
    keep(&host, 9);
    host.enter("tie", host.config().slice, |s| {
        s.eval::<(), _>("kept.self = kept; queueMicrotask(() => { globalThis.kept = null })")
    })
    .unwrap();
    // The collection reviews what the run let go of: nothing is owed
    // until the job runs.
    assert!(host.collect_now());
    assert!(!host.may_hold_garbage());
    let held = host.heap_bytes();
    let collections = host.collections();
    let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(5));
    assert_eq!(pumped.ran, 1);
    assert!(pumped.faults.is_empty(), "{:?}", pumped.faults);
    assert!(host.may_hold_garbage(), "what the job let go of");
    let fill = "globalThis.s = 'x'.repeat(8 * 1024 * 1024)";
    let err = host
        .enter("fill", host.config().slice, |s| s.eval::<(), _>(fill))
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    assert_eq!(host.collections(), collections + 1);
    assert!(host.heap_bytes() < held / 2, "{}", host.heap_bytes());
    host.enter("fill", host.config().slice, |s| s.eval::<(), _>(fill))
        .unwrap();
}

/// A run can make a cycle unreachable and leave every object's count as
/// it was, by rewiring references. The digest the host compares cannot
/// tell, but the run ran out, so a collection is owed for it all the same:
/// the host wants one before it rests, and that one frees the cycle.
#[test]
fn a_run_out_after_rewiring_that_kept_every_count_is_still_collected_for() {
    let host = small(16);
    host.enter("tie", host.config().slice, |s| {
        s.eval::<(), _>(
            "globalThis.a = { next: { next: null, big: 'y'.repeat(9 * 1024 * 1024) } };\n\
             globalThis.rewire = () => {\n\
                 a.next.next = a.next; a.next = a; globalThis.a = null;\n\
                 globalThis.s = 'x'.repeat(8 * 1024 * 1024);\n\
             }",
        )
    })
    .unwrap();
    assert!(host.collect_now());
    let held = host.heap_bytes();
    // A call, not an eval: compiling one leaves what the compiler used.
    let err = host
        .enter("rewire", host.config().slice, |s| {
            s.globals()
                .get::<_, ljos_js::rquickjs::Function>("rewire")?
                .call::<_, ()>(())
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    assert!(host.heap_bytes() >= held, "the cycles are still there");
    assert!(
        !host.may_hold_garbage(),
        "every count is as it was: the digest cannot tell"
    );
    assert!(!host.collected_since_ran_out());
    assert!(host.wants_idle_collect(), "a collection is owed");
    assert!(host.collect_if_idle());
    assert!(host.collected_since_ran_out());
    assert!(!host.wants_idle_collect(), "and paid");
    assert!(host.heap_bytes() < held / 2, "{}", host.heap_bytes());
    host.enter("fill", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.s = 'x'.repeat(8 * 1024 * 1024)")
    })
    .unwrap();
}

/// A run that lets go of the root of a cycle and then fails a big
/// allocation may have let go of something: the host owes a collection
/// for it, paid when it rests, so the next run has the room.
#[test]
fn a_run_that_drops_a_cycle_and_then_runs_out_leaves_it_collected() {
    let host = small(16);
    keep(&host, 9);
    host.enter("tie", host.config().slice, |s| {
        s.eval::<(), _>(
            "kept.self = kept;\n\
             globalThis.dropAndFill = () => {\n\
                 globalThis.kept = null;\n\
                 globalThis.s = 'x'.repeat(8 * 1024 * 1024);\n\
             }",
        )
    })
    .unwrap();
    assert!(host.collect_now());
    let held = host.heap_bytes();
    let collections = host.collections();
    let err = host
        .enter("dropAndFill", host.config().slice, |s| {
            s.globals()
                .get::<_, ljos_js::rquickjs::Function>("dropAndFill")?
                .call::<_, ()>(())
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    assert!(host.wants_idle_collect(), "a collection is owed");
    assert!(host.collect_if_idle());
    assert_eq!(host.collections(), collections + 1, "collected after it");
    assert!(!host.wants_idle_collect(), "and paid");
    assert!(host.heap_bytes() < held / 2, "{}", host.heap_bytes());
    host.enter("fill", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.s = 'x'.repeat(8 * 1024 * 1024)")
    })
    .unwrap();
}

/// The fault of a run that found no room names what grew the heap.
#[test]
fn running_out_of_heap_names_what_grew_it() {
    let host = small(8);
    let err = host
        .enter_naming(
            "binding",
            || "Sidebar.qml:12".to_owned(),
            host.config().slice,
            |s| {
                s.eval::<(), _>(
                    "(function(){ globalThis.a = []; for (;;) a.push(new Array(1000).fill(1)); })()",
                )
            },
        )
        .unwrap_err();
    let said = err.to_string();
    assert!(said.contains(" at Sidebar.qml:12"), "{said}");
    assert!(
        said.contains("grew it most since it was last collected: Sidebar.qml:12 ("),
        "{said}"
    );
}

/// Past three quarters of the limit after collecting, the host says so
/// once and names what grew the heap; below five eighths it may say so
/// again.
#[test]
fn a_heap_past_three_quarters_is_warned_of_once_and_named() {
    let host = small(32);
    assert!(!host.heap_pressure().warned);
    keep(&host, 25);
    let p = host.heap_pressure();
    assert!(p.warned, "{p:?}");
    assert_eq!(p.limit, Some(32 * MIB));
    host.enter("drop", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.kept = null")
    })
    .unwrap();
    host.collect_now();
    assert!(host.heap_pressure().growers.is_empty());
    host.enter("after", host.config().slice, |s| s.eval::<(), _>("1"))
        .unwrap();
    assert!(!host.heap_pressure().warned);
}

/// An explicit collection that leaves the heap past three quarters warns
/// as the one after a run does: every collection follows one rule.
#[test]
fn an_explicit_collection_past_three_quarters_is_warned_of() {
    let host = small(64);
    keep(&host, 25);
    assert!(!host.heap_pressure().warned, "{:?}", host.heap_pressure());
    assert!(host.set_memory_limit(Some(32 * MIB)));
    assert!(host.collect_now());
    let p = host.heap_pressure();
    assert!(p.warned, "{p:?}");
    assert!(p.growers.is_empty(), "{p:?}");
}

/// Promise jobs grow the heap as runs do: what a pump grew is named, and
/// past three quarters it collects and warns, with no run after it.
#[test]
fn promise_jobs_are_held_to_the_heap_as_runs_are() {
    let host = small(32);
    keep(&host, 20);
    host.collect_now();
    assert!(!host.heap_pressure().warned, "{:?}", host.heap_pressure());
    let collections = host.collections();
    let grow = |arrays: usize| {
        host.enter("setup", host.config().slice, |s| {
            s.eval::<(), _>(
                format!(
                    "queueMicrotask(() => {{ for (let i = 0; i < {arrays}; i++) kept.push(new Array(1000).fill(i)) }})"
                )
                .as_str(),
            )
        })
        .unwrap();
        let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(5));
        assert_eq!(pumped.ran, 1);
        assert!(pumped.faults.is_empty(), "{:?}", pumped.faults);
    };

    // About 1 MiB, short of three quarters: named, not collected.
    grow(64);
    let p = host.heap_pressure();
    assert!(
        p.growers
            .iter()
            .any(|(name, by)| name == "promise job" && *by > 0),
        "{p:?}"
    );
    assert_eq!(host.collections(), collections);

    // About 8 MiB more, past three quarters: collected and warned of.
    grow(512);
    let p = host.heap_pressure();
    assert!(host.collections() > collections, "{p:?}");
    assert!(p.warned, "{p:?}");
}

/// A heap past three quarters that a collection would bring back under
/// it is not warned of: the warning follows a collection.
#[test]
fn garbage_past_three_quarters_with_no_collection_is_not_warned_of() {
    let host = small(32);
    keep(&host, 22);
    host.collect_now();
    let collections = host.collections();
    let high = 24 * MIB;
    // Cycles only the collector frees, a little at a time, until the
    // heap ends a run past three quarters, short of the growth that
    // collects.
    for _ in 0..200 {
        if host.heap_bytes() >= high {
            break;
        }
        host.enter("litter", host.config().slice, |s| {
            s.eval::<(), _>(
                "for (let i = 0; i < 16; i++) { const a = new Array(1000).fill(i); a.self = a }",
            )
        })
        .unwrap();
    }
    assert!(host.heap_bytes() >= high, "{}", host.heap_bytes());
    assert_eq!(host.collections(), collections, "nothing collected");
    assert!(!host.heap_pressure().warned, "{:?}", host.heap_pressure());
    host.collect_now();
    assert!(
        host.heap_bytes() < high,
        "it was garbage: {}",
        host.heap_bytes()
    );
}

/// A run that let go of nothing, as a binding that only reads, leaves
/// nothing to collect: a run that then finds a heap full of what lives
/// does not collect it all again.
#[test]
fn a_run_out_of_a_live_heap_after_an_ordinary_run_does_not_collect() {
    let host = small(32);
    keep(&host, 28);
    host.enter("read", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.read = () => kept.length")
    })
    .unwrap();
    host.collect_now();
    let collections = host.collections();
    // A call, not an eval: compiling one frees what the compiler used.
    host.enter("read", host.config().slice, |s| {
        s.globals()
            .get::<_, ljos_js::rquickjs::Function>("read")?
            .call::<_, f64>(())
    })
    .unwrap();
    assert!(!host.may_hold_garbage());
    // One allocation bigger than the room left: it fails without growing
    // the heap.
    let err = host
        .enter("fill", host.config().slice, |s| {
            s.eval::<usize, _>("'x'.repeat(16 * 1024 * 1024).length")
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    assert_eq!(host.collections(), collections, "nothing collected");
}

/// What grew the heap is told per run, the most first, and the table
/// stays small.
#[test]
fn heap_growers_are_kept_by_name_and_bounded() {
    let host = small(64);
    for i in 0..12 {
        let name = format!("Row{i}.qml:1");
        host.enter_naming(&name, || name.clone(), host.config().slice, |s| {
            s.eval::<(), _>(format!(
                "globalThis.k{i} = []; for (let j = 0; j < {}; j++) k{i}.push(new Array(1000).fill(j))",
                40 + i * 10
            ))
        })
        .unwrap();
    }
    let p = host.heap_pressure();
    assert!(p.growers.len() <= 8, "{p:?}");
    assert_eq!(p.growers[0].0, "Row11.qml:1", "{p:?}");
    assert!(p.growers.windows(2).all(|w| w[0].1 >= w[1].1), "{p:?}");
}

/// The default limit follows the machine, within bounds.
#[test]
fn the_default_heap_limit_is_between_96_and_512_mib() {
    let l = ljos_js::default_memory_limit();
    assert!((96 * MIB..=512 * MIB).contains(&l), "{l}");
}

/// A heap at its limit with no room left for its own message:
/// QuickJS throws an `InternalError` with no words, which is the heap
/// running out (#670).
#[test]
fn an_unworded_internal_error_on_a_full_heap_is_running_out_of_memory() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    let err = eval_num(&host, "throw new InternalError()").unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 1024)));
    let err = eval_num(&host, "throw new InternalError()").unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    assert!(host.set_memory_limit(None));
    assert_eq!(eval_num(&host, "40 + 2").unwrap(), 42.0);
}

/// A string read out of a full heap has no room for its copy: rquickjs
/// calls that an unknown error, and it is the heap running out (#670).
#[test]
fn a_string_that_cannot_be_read_out_of_a_full_heap_is_running_out_of_memory() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    let read = |host: &JsHost| {
        host.enter("read", host.config().slice, |s| {
            s.eval::<String, _>("globalThis.wide")
        })
    };
    host.enter("make", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.wide = '\\u00e9'.repeat(64 * 1024)")
    })
    .unwrap();
    assert_eq!(read(&host).unwrap().chars().count(), 64 * 1024);
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 16 * 1024)));
    let err = read(&host).unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    assert!(host.set_memory_limit(None));
    assert_eq!(read(&host).unwrap().chars().count(), 64 * 1024);
}

/// What a function that ran out held in its locals is freed as the
/// exception leaves it, before the fault is told: the heap has room by
/// then, and the error thrown while it had none still says it ran out.
#[test]
fn running_out_of_heap_is_told_after_its_locals_go() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    // A chain no cycle holds: freed as soon as `fill` returns.
    let err = eval_num(
        &host,
        "function fill() { let h = null; for (;;) h = { n: h } } fill()",
    )
    .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    // A script's own `throw null` in the next run, with room, is its own.
    let err = eval_num(&host, "throw null").unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
}

/// A script may throw `null` itself on a heap within a sixteenth of its
/// limit, held or just let go: QuickJS never throws `null`, so however
/// full the heap is, it is the script's exception.
#[test]
fn a_null_a_script_throws_on_a_nearly_full_heap_is_its_own() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    let limit = held + 4 * 1024 * 1024;
    assert!(host.set_memory_limit(Some(limit)));
    // Room for the buffer, and less than a sixteenth of the limit left.
    let bytes = 4 * 1024 * 1024 - 128 * 1024;
    eval_num(&host, &format!("var kept = new ArrayBuffer({bytes}); 0")).unwrap();
    let now = host.memory_stats().unwrap().malloc_size;
    assert!(now + limit / 16 >= limit, "nearly full: {now} of {limit}");
    let err = eval_num(&host, "throw null").unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "held: {err:?}");
    // Filled, let go and thrown in one run.
    let err = eval_num(
        &host,
        &format!("kept = null; let b = new ArrayBuffer({bytes}); b = null; throw null"),
    )
    .unwrap_err();
    assert!(
        matches!(err, JsFault::JsException { .. }),
        "let go: {err:?}"
    );
    assert_eq!(eval_num(&host, "40 + 2").unwrap(), 42.0);
}

/// What QuickJS throws for want of heap is out of memory however it reaches
/// the host: caught and thrown again by the script, too, and a script
/// cannot make it read as anything else.
#[test]
fn the_error_thrown_for_want_of_heap_and_thrown_again_is_running_out_of_memory() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    let err = eval_num(
        &host,
        "function fill() { let h = null; for (;;) h = { n: h } }\n\
         try { fill() } catch (e) {\n\
           if (e.message !== 'out of memory') throw new Error('not out of memory: ' + e);\n\
           e.message = 'mine';\n\
           throw e\n\
         }",
    )
    .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
}

/// A `catch` may write on what it caught before it throws it again, as
/// error reporting does. On the error every throw for want of room shares,
/// the write neither throws, which would put a `TypeError` in the place of
/// running out, nor shows in the next throw. What says whether a write
/// took says it did not.
#[test]
fn writing_on_the_error_thrown_for_want_of_heap_neither_throws_nor_changes_it() {
    for write in [
        "if (e instanceof Error) e.context = 'load'",
        "e.message = 'load'",
        "e.name = 'Load'",
        "e['context'] = 'load'",
        "e.stack = 'load'",
        "e.stack = undefined",
        "e.stack = 42",
        "if (Reflect.set(e, 'context', 'load') !== false) throw new Error('set')",
        "if (Reflect.set(e, 'stack', 'load') !== false) throw new Error('stack set')",
        "if (Reflect.set(e, 'stack', undefined) !== false) throw new Error('stack set')",
        "if (Reflect.defineProperty(e, 'message', { value: 'load' }) !== false) throw new Error('defined')",
        "if (Reflect.setPrototypeOf(e, null) !== false) throw new Error('prototype')",
        "if (Reflect.deleteProperty(e, 'message') !== false) throw new Error('deleted')",
        "if (delete e.message !== false) throw new Error('deleted')",
        "Object.assign(e, { context: 'load', message: 'load' })",
        "Object.defineProperty(e, 'context', { value: 'load' })",
        "Object.defineProperty(e, 'message', { value: 'load' })",
        "delete e.message",
        "Object.setPrototypeOf(e, null)",
    ] {
        let host = small(16);
        host.collect_now();
        let held = host.memory_stats().unwrap().malloc_size;
        assert!(host.set_memory_limit(Some(held + 4 * MIB)));
        let err = eval_num(
            &host,
            &format!(
                "'use strict';\n\
                 function fill() {{ let h = null; for (;;) h = {{ n: h }} }}\n\
                 try {{ fill() }} catch (e) {{\n\
                   globalThis.kept = e;\n\
                   {write};\n\
                   throw e\n\
                 }}"
            ),
        )
        .unwrap_err();
        assert!(
            matches!(err, JsFault::JsOutOfMemory { .. }),
            "{write}: {err:?}"
        );
        // Also while a script still holds the one it wrote on.
        let unchanged = eval_num(
            &host,
            "function again() { let h = null; for (;;) h = { n: h } }\n\
             let same = 0;\n\
             try { again() } catch (e) {\n\
               same = e === kept && e instanceof Error && e.name === 'InternalError'\n\
                 && e.message === 'out of memory' && e.stack === ''\n\
                 && e.context === undefined ? 1 : 0\n\
             }\n\
             same",
        );
        assert_eq!(unchanged, Ok(1.0), "{write}");
    }
}

/// With no room left for "interrupted", an interrupt is still one no
/// `catch` takes, and it leaves the error thrown for want of room as it
/// was: the next run that runs out catches it, and an async function that
/// runs out rejects.
#[test]
fn an_interrupt_on_a_full_heap_is_uncatchable_and_leaves_running_out_catchable() {
    let host = small(16);
    let slice = host.config().slice;
    // Made beforehand, so nothing between running out and the interrupt
    // allocates.
    host.enter("setup", slice, |s| {
        s.eval::<(), _>(
            "globalThis.full = false; globalThis.caught = null; globalThis.h = null;\n\
             globalThis.fill = function () { for (;;) h = { n: h } };\n\
             globalThis.go = function () {\n\
               try { fill() } catch (e) { full = e.stack === '' }\n\
               try { for (;;) {} } catch (e) { caught = e }\n\
             };\n\
             globalThis.chain = function () { let t = null; for (;;) t = { n: t } };\n\
             undefined",
        )
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + MIB)));
    let spin = Slice {
        soft: Duration::from_secs(1),
        hard: Duration::from_secs(2),
    };
    let out = host.enter("go", spin, |s| s.eval::<(), _>("go()"));
    assert!(host.set_memory_limit(None));
    // What the catch took, read once there is room to read it.
    let took = host.enter("took", slice, |s| {
        s.eval::<String, _>(
            "caught === null ? 'nothing' : `${caught.name}: ${caught.message} \
             (stack ${JSON.stringify(caught.stack)}, frozen ${Object.isFrozen(caught)})`",
        )
    });
    assert!(
        matches!(out, Err(JsFault::JsInterrupted { .. })),
        "{out:?}, and the catch took {took:?}"
    );
    assert_eq!(
        eval_num(&host, "full ? 1 : 0"),
        Ok(1.0),
        "the heap was not full when the interrupt fired"
    );
    assert_eq!(eval_num(&host, "caught === null ? 0 : 1"), Ok(0.0));

    assert_eq!(eval_num(&host, "h = null; 0"), Ok(0.0));
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * MIB)));
    let later = eval_num(
        &host,
        "let got = 0; try { chain() } catch (e) { got = e.stack === '' ? 1 : 2 }; got",
    );
    assert_eq!(later, Ok(1.0));
    host.enter("reject", slice, |s| {
        s.eval::<(), _>(
            "globalThis.rejected = false;\n\
             (async () => chain())().catch(() => { rejected = true });\n\
             undefined",
        )
    })
    .unwrap();
    let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(5));
    assert!(pumped.faults.is_empty(), "{:?}", pumped.faults);
    assert_eq!(eval_num(&host, "rejected ? 1 : 0"), Ok(1.0));
}

/// A run that catches running out and swallows it has room again; a
/// `null` it then throws itself is its own exception, not the heap
/// running out, so the host does not wait for room and run it again.
#[test]
fn a_null_thrown_after_running_out_was_swallowed_in_the_same_run_is_its_own() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    let err = eval_num(
        &host,
        "function fill() { let h = null; for (;;) h = { n: h } }\n\
         let ran_out = false;\n\
         try { fill() } catch (e) { ran_out = true }\n\
         if (!ran_out) throw new Error('did not run out');\n\
         throw null",
    )
    .unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
}

/// Telling that the heap ran out runs no script: an accessor on the error's
/// prototype would be given a heap with no room, and what it made would
/// read as garbage to collect.
#[test]
fn telling_that_the_heap_ran_out_runs_no_script() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.enter("setup", host.config().slice, |s| {
        s.eval::<(), _>(
            "globalThis.read = 0;\n\
             Object.defineProperty(Error.prototype, 'fileName', { get() { read++; return '' } });\n\
             globalThis.fill = function () { let h = null; for (;;) h = { n: h } }",
        )
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    let err = eval_num(&host, "fill()").unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    host.collect_now();
    assert_eq!(eval_num(&host, "read").unwrap(), 0.0);
}

/// A job that ran out and caught it leaves the next job's own `throw null`
/// its exception.
#[test]
fn a_null_a_job_throws_after_another_caught_running_out_is_its_own() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    host.enter("setup", host.config().slice, |s| {
        s.eval::<(), _>(
            "function fill() { let h = null; for (;;) h = { n: h } }\n\
             queueMicrotask(() => { try { fill() } catch (e) { if (e.message !== 'out of memory') throw new Error('not out of memory: ' + e) } });\n\
             queueMicrotask(() => { throw null })",
        )
    })
    .unwrap();
    let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(5));
    assert_eq!(pumped.ran, 2);
    assert!(
        matches!(&pumped.faults[..], [JsFault::JsException { .. }]),
        "{:?}",
        pumped.faults
    );
}

/// A nested run does not change what the run around it threw: the error
/// QuickJS threw in the outer run before it, caught and thrown again after,
/// is out of memory.
#[test]
fn the_error_thrown_for_want_of_heap_before_a_nested_run_is_still_out_of_memory_after() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    let slice = host.config().slice;
    let err = host
        .enter("outer", slice, |s| {
            s.eval::<(), _>(
                "function fill() { let h = null; for (;;) h = { n: h } }\n\
                 try { fill() } catch (e) { if (e.message !== 'out of memory') throw new Error('not out of memory: ' + e); globalThis.caught = e }",
            )?;
            let inner = host.enter("inner", slice, |s| s.eval::<f64, _>("40 + 2"));
            assert_eq!(inner, Ok(42.0));
            s.eval::<(), _>("throw caught")
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
}

/// QuickJS carves small objects out of arenas and lets an arena go only
/// once all of it is free. Objects that stay among the freed ones keep
/// their arenas held, so what the system gave back says little about
/// what was freed, and nothing about whether the run ran out: the error
/// thrown while it had no room says that.
#[test]
fn running_out_of_heap_is_told_with_arenas_still_in_use() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    // One object kept for every four of a chain no cycle holds: the
    // chain is freed when `fill` returns, the arenas stay.
    let err = eval_num(
        &host,
        "var kept = { n: null };\n\
         function fill() { let h = null; for (let i = 0; ; i++) { h = { n: h }; if (i % 4 == 0) kept = { n: kept } } }\n\
         fill()",
    )
    .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    let left = host.memory_stats().unwrap().malloc_size;
    assert!(
        left + 1024 * 1024 < held + 4 * 1024 * 1024,
        "the chain was freed: {left} of {}",
        held + 4 * 1024 * 1024
    );
    // A script's own `throw null` in the next run, with room, is its own.
    let err = eval_num(&host, "throw null").unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
}

/// A nested run that ran out leaves a `null` thrown by the next nested
/// run its own: an outer run can last as long as a test function that
/// waits, with the event loop nested inside it.
#[test]
fn a_null_thrown_in_a_nested_run_after_another_ran_out_is_its_own() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    let slice = host.config().slice;
    let (filled, thrown) = host
        .enter("outer", slice, |_| {
            let filled = host.enter("fill", slice, |s| {
                s.eval::<f64, _>("function fill() { let h = null; for (;;) h = { n: h } } fill()")
            });
            let thrown = host.enter("throw", slice, |s| s.eval::<f64, _>("throw null"));
            Ok((filled, thrown))
        })
        .unwrap();
    assert!(
        matches!(filled, Err(JsFault::JsOutOfMemory { .. })),
        "{filled:?}"
    );
    assert!(
        matches!(thrown, Err(JsFault::JsException { .. })),
        "{thrown:?}"
    );
}

/// Each job a pump runs is told by what it threw itself: a job before it
/// that ran out does not make its `null` running out.
#[test]
fn a_null_thrown_by_a_job_after_another_job_ran_out_is_its_own() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    host.enter("setup", host.config().slice, |s| {
        s.eval::<(), _>(
            "queueMicrotask(function fill() { let h = null; for (;;) h = { n: h } });\n\
             queueMicrotask(() => { throw null })",
        )
    })
    .unwrap();
    let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(5));
    assert_eq!(pumped.ran, 2);
    assert!(
        matches!(
            &pumped.faults[..],
            [JsFault::JsOutOfMemory { .. }, JsFault::JsException { .. }]
        ),
        "{:?}",
        pumped.faults
    );
}

/// Turning a string into Rust text allocates (non-ASCII text grows as
/// UTF-8). On a full heap QuickJS gives no string back, which rquickjs
/// words as an unknown error; the heap ran out all the same.
#[test]
fn a_string_too_big_for_the_heap_left_is_running_out_of_memory() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 3 * 1024 * 1024)));
    let slice = host.config().slice;
    // 1.2 MB as JS holds it, 2.4 MB as UTF-8.
    let err = host
        .enter("text", slice, |s| {
            s.eval::<String, _>("'\\u00e9'.repeat(1200000)")
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    // With room, the same text comes through, and nothing is left pending.
    assert!(host.set_memory_limit(None));
    let text = host
        .enter("text", slice, |s| {
            s.eval::<String, _>("'\\u00e9'.repeat(1200000)")
        })
        .unwrap();
    assert_eq!(text.chars().count(), 1_200_000);
    assert_eq!(eval_num(&host, "40 + 2").unwrap(), 42.0);
}

#[test]
fn out_of_memory_is_reported_and_context_survives() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(8 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    let err = host
        .enter(
            "hog",
            Slice {
                soft: Duration::from_secs(5),
                hard: Duration::from_secs(10),
            },
            |s| {
                s.eval::<(), _>(
                    "(function(){ const a = []; for (;;) a.push(new Array(1000).fill(1)); })()",
                )
            },
        )
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    host.collect_now();
    assert_eq!(eval_num(&host, "40 + 2").unwrap(), 42.0);
    // The garbage is gone again.
    assert!(host.memory_stats().unwrap().malloc_size < 4 * 1024 * 1024);
}

/// A dispatch that keeps all it allocates is stopped once it took its
/// share of the room under the limit, and told as the heap running out:
/// the rest of the application keeps the room left, instead of every
/// script after it running out too.
#[test]
fn a_dispatch_that_keeps_allocating_leaves_room_for_the_rest() {
    let limit = 16 * 1024 * 1024;
    let host = JsHost::new(JsConfig {
        memory_limit: Some(limit),
        ..JsConfig::default()
    })
    .unwrap();
    let slice = Slice {
        soft: Duration::from_secs(5),
        hard: Duration::from_secs(10),
    };
    let err = host
        .enter("hog", slice, |s| {
            s.eval::<(), _>("globalThis.hog = []; for (;;) hog.push(new Array(100).fill(1))")
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    // What it kept stays, but a quarter of the room is someone else's.
    let held = host.heap_bytes();
    assert!(held < limit / 8 * 7, "the hog holds {held} of {limit}");
    let other = host.enter("other", slice, |s| {
        s.eval::<f64, _>("'x'.repeat(2 * 1024 * 1024).length")
    });
    assert_eq!(other.unwrap(), (2 * 1024 * 1024) as f64);
}

#[test]
fn a_promise_job_that_keeps_allocating_leaves_room_for_the_rest() {
    let limit = 16 * 1024 * 1024;
    let host = JsHost::new(JsConfig {
        memory_limit: Some(limit),
        ..JsConfig::default()
    })
    .unwrap();
    let slice = Slice {
        soft: Duration::from_secs(5),
        hard: Duration::from_secs(10),
    };
    host.enter("queue", slice, |s| {
        s.eval::<(), _>(
            "globalThis.hog = []; globalThis.why = undefined; \
             Promise.resolve() \
                 .then(() => { for (;;) hog.push(new Array(100).fill(1)) }) \
                 .catch(e => { why = String(e) })",
        )
    })
    .unwrap();
    let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(5));
    assert!(pumped.faults.is_empty(), "{:?}", pumped.faults);
    // A callback that throws rejects its promise rather than failing the
    // job, so the refusal reaches the script, not the pump.
    let why = host
        .enter("why", slice, |s| s.eval::<String, _>("String(why)"))
        .unwrap();
    assert!(why.contains("out of memory") || why == "null", "{why}");
    let held = host.heap_bytes();
    assert!(held < limit / 8 * 7, "the job holds {held} of {limit}");
    let other = host.enter("other", slice, |s| {
        s.eval::<f64, _>("'x'.repeat(2 * 1024 * 1024).length")
    });
    assert_eq!(other.unwrap(), (2 * 1024 * 1024) as f64);
}

#[test]
fn an_error_thrown_after_a_caught_refusal_is_the_scripts_own() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    let slice = Slice {
        soft: Duration::from_secs(5),
        hard: Duration::from_secs(10),
    };
    let err = host
        .enter("caught", slice, |s| {
            s.eval::<(), _>(
                "let hog = []; \
                 try { for (;;) hog.push(new Array(100).fill(1)) } catch (e) {} \
                 hog = null; \
                 throw new TypeError('load failed')",
            )
        })
        .unwrap_err();
    match err {
        JsFault::JsException { message, .. } => {
            assert!(message.contains("load failed"), "{message}")
        }
        other => panic!("told as {other:?}"),
    }
}

/// A script that calls itself `n` deep: `n`, or -1 once a RangeError
/// says the stack ran out.
fn recurse(n: u32) -> String {
    format!(
        "(function () {{
            function f(n) {{ return n == 0 ? 0 : 1 + f(n - 1) }}
            try {{ return f({n}) }} catch (e) {{ return e instanceof RangeError ? -1 : -2 }}
        }})()"
    )
}

// Where the end of the stack is unknown, the default host is kept to
// FALLBACK_MAX_STACK too, so the two hosts give the same.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
))]
#[test]
fn a_call_that_goes_deep_uses_the_threads_stack() {
    // A thread of 8 MiB, far more than the old fixed limit, and a call
    // that needs more than it: kept to that limit, the call runs out. A
    // level takes near 2 KiB, so the limit stops a call some 300 deep and
    // the thread one some 4 000 deep.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let deep = recurse(2_000);
            let capped = JsHost::new(JsConfig {
                max_stack: Some(ljos_js::FALLBACK_MAX_STACK),
                ..JsConfig::default()
            })
            .unwrap();
            assert_eq!(
                eval_num(&capped, &deep).unwrap(),
                -1.0,
                "the call needs more than FALLBACK_MAX_STACK"
            );
            assert_eq!(eval_num(&plain(), &deep).unwrap(), 2_000.0);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn a_smaller_max_stack_is_kept_to() {
    let host = JsHost::new(JsConfig {
        max_stack: Some(16 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    assert_eq!(eval_num(&host, &recurse(200)).unwrap(), -1.0);
    assert_eq!(eval_num(&host, "40 + 2").unwrap(), 42.0);
}

#[test]
fn exception_carries_message_stack_and_position() {
    let host = plain();
    let m = host
        .load_module(
            "button.js",
            "export function boom(x) {\n  return x.nope.deeper;\n}\n",
        )
        .unwrap();
    let err = host
        .call(m.function("boom").unwrap(), &[JsValue::Null])
        .unwrap_err();
    match err {
        JsFault::JsException {
            message,
            stack,
            file,
            line,
            ..
        } => {
            assert!(message.contains("null"), "{message}");
            assert!(stack.contains("boom"), "{stack}");
            assert_eq!(file.as_deref(), Some("button.js"));
            assert_eq!(line, Some(2));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn source_mapper_rewrites_position() {
    let host = plain();
    host.set_source_mapper(Some(Box::new(|file, line| {
        (file == "button.js").then(|| ("Button.qml".to_owned(), line + 10))
    })));
    let m = host
        .load_module("button.js", "export function f() { throw new Error('x'); }")
        .unwrap();
    let JsFault::JsException { file, line, .. } =
        host.call(m.function("f").unwrap(), &[]).unwrap_err()
    else {
        panic!()
    };
    assert_eq!(file.as_deref(), Some("Button.qml"));
    assert_eq!(line, Some(11));
}

#[test]
fn syntax_error_is_a_js_exception() {
    let host = plain();
    let err = host.load_module("bad.js", "export function (").unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
}

#[test]
fn thrown_non_error_value_is_reported() {
    let host = plain();
    let err = host
        .enter("t", host.config().slice, |s| {
            s.eval::<(), _>("throw 'plain string'")
        })
        .unwrap_err();
    let JsFault::JsException { message, .. } = err else {
        panic!("{err:?}")
    };
    assert_eq!(message, "plain string");
}

#[test]
fn panic_in_closure_is_a_fault_and_host_survives() {
    let host = plain();
    let err = host
        .enter(
            "p",
            host.config().slice,
            |_| -> rquickjs_result::Result<()> { panic!("kaput") },
        )
        .unwrap_err();
    assert!(
        matches!(&err, JsFault::Panic { message, .. } if message == "kaput"),
        "{err:?}"
    );
    assert_eq!(eval_num(&host, "2 * 3").unwrap(), 6.0);
}

mod rquickjs_result {
    pub use ljos_js::rquickjs::Result;
}

#[test]
fn nested_enter_reuses_the_active_context() {
    let host = plain();
    let inner = host
        .enter("outer", host.config().slice, |s| {
            s.eval::<(), _>("globalThis.shared = 20;")?;
            Ok(host.enter("inner", host.config().slice, |s| {
                s.eval::<f64, _>("shared + 22")
            }))
        })
        .unwrap();
    assert_eq!(inner.unwrap(), 42.0);
    assert!(
        host.memory_stats().is_some(),
        "entered flag must be cleared"
    );
}

#[test]
fn nested_enter_failure_is_contained() {
    let host = plain();
    let (inner, after) = host
        .enter("outer", host.config().slice, |s| {
            let inner = host.enter("inner", host.config().slice, |s| {
                s.eval::<(), _>("throw new Error('inner broke')")
            });
            Ok((inner, s.eval::<f64, _>("1 + 1")?))
        })
        .unwrap();
    assert!(
        matches!(&inner, Err(JsFault::JsException { message, .. }) if message == "Error: inner broke"),
        "{inner:?}"
    );
    assert_eq!(after, 2.0);
}

#[test]
fn nested_runs_are_held_to_the_dispatch_cap() {
    let host = plain();
    let slice = Slice {
        soft: Duration::from_millis(5),
        hard: Duration::from_millis(30),
    };
    let long = Slice {
        soft: Duration::from_secs(10),
        hard: Duration::from_secs(10),
    };
    let out = host.enter("outer", slice, |s| {
        let inner = host.enter("inner", long, |s| s.eval::<(), _>("for(;;){}"));
        assert!(
            matches!(inner, Err(JsFault::JsInterrupted { .. })),
            "{inner:?}"
        );
        s.eval::<(), _>("for(;;){}")
    });
    assert!(matches!(out, Err(JsFault::JsInterrupted { .. })), "{out:?}");
}

/// A clock that moves only when it is polled (a millisecond a poll) or
/// moved by hand, so a test of the limits does not depend on the machine.
fn fake_clock(host: &JsHost) -> Rc<Cell<Instant>> {
    let now = Rc::new(Cell::new(Instant::now()));
    let at = now.clone();
    host.set_clock(Some(Rc::new(move |t| {
        if t == Timing::Poll {
            at.set(at.get() + Duration::from_millis(1));
        }
        at.get()
    })));
    now
}

const TENTH: Slice = Slice {
    soft: Duration::from_secs(10),
    hard: Duration::from_millis(100),
};

/// About 30 polls: well inside a slice of a tenth of a second.
const SHORT: &str =
    "(() => { let s = 0; for (let i = 0; i < 300000; i++) s += i; return s > 0 })()";

/// The real clocks: a sleep is time the thread does not have the processor,
/// however it comes about.
#[test]
fn time_off_the_processor_is_not_the_scripts() {
    let host = JsHost::new(JsConfig::default()).unwrap();
    let nap: ljos_js::HostFn = Rc::new(|_, _| {
        std::thread::sleep(Duration::from_millis(300));
        Ok(JsValue::Undefined)
    });
    host.set_global_function("nap", nap).unwrap();
    // Sleeps through its limit before the first poll and after it.
    let out = host.enter("run", TENTH, |s| {
        s.eval::<bool, _>(format!("nap(); {SHORT}; nap(); {SHORT}"))
    });
    assert_eq!(out, Ok(true));
}

/// Time off the processor is excused and not more: a script that keeps
/// sleeping and running is stopped when what it ran comes to its limit.
#[test]
fn a_script_that_also_sleeps_is_stopped_at_its_own_time() {
    let host = JsHost::new(JsConfig::default()).unwrap();
    let nap: ljos_js::HostFn = Rc::new(|_, _| {
        std::thread::sleep(Duration::from_millis(1));
        Ok(JsValue::Undefined)
    });
    host.set_global_function("nap", nap).unwrap();
    let out = host.enter("run", TENTH, |s| {
        s.eval::<(), _>("for (;;) { nap(); for (let i = 0; i < 1000; i++) ; }")
    });
    assert!(matches!(out, Err(JsFault::JsInterrupted { .. })), "{out:?}");
}

#[test]
fn native_work_is_not_the_scripts_time() {
    let host = plain();
    let now = fake_clock(&host);
    let out = host.enter("outer", TENTH, |s| {
        // Under the ceiling of 50 slices.
        host.native(|| now.set(now.get() + Duration::from_secs(2)));
        s.eval::<bool, _>(SHORT)
    });
    assert_eq!(out, Ok(true));
}

#[test]
fn a_nested_run_has_a_slice_of_its_own() {
    let host = plain();
    let now = fake_clock(&host);
    let out = host.enter("outer", TENTH, |s| {
        // The outer run has used most of its slice, then makes an object
        // whose own script runs nested; then it goes on.
        now.set(now.get() + Duration::from_millis(90));
        let inner = host.native(|| host.enter("inner", TENTH, |s| s.eval::<bool, _>(SHORT)));
        assert_eq!(inner, Ok(true));
        s.eval::<bool, _>(
            "(() => { let t = 0; for (let i = 0; i < 30000; i++) t += i; return t > 0 })()",
        )
    });
    assert_eq!(out, Ok(true));
}

#[test]
fn an_interrupt_in_a_nested_run_names_it_and_the_dispatch() {
    let host = plain();
    let _now = fake_clock(&host);
    let out: Result<(), _> = host.enter_naming(
        "outer",
        || "Main.qml:7:5 (onUrlChanged)".into(),
        TENTH,
        |_| {
            let inner = host.enter_naming(
                "inner",
                || "Late.qml:3:5 (Component.onCompleted)".into(),
                TENTH,
                |s| s.eval::<(), _>("for(;;){}"),
            );
            assert!(
                matches!(inner, Err(JsFault::JsInterrupted { .. })),
                "{inner:?}"
            );
            // As the callback that ran it would: the dispatch is over.
            assert!(host.interrupted());
            Err(ljos_js::rquickjs::Error::Exception)
        },
    );
    let Err(fault) = out else { panic!("{out:?}") };
    let text = fault.to_string();
    assert!(
        text.contains(
            "at Main.qml:7:5 (onUrlChanged), in `inner` at Late.qml:3:5 (Component.onCompleted)"
        ),
        "{text}"
    );
}

#[test]
fn a_dispatch_of_native_work_is_stopped_at_its_ceiling() {
    let host = plain();
    let now = fake_clock(&host);
    let out = host.enter("outer", TENTH, |s| {
        s.eval::<(), _>("for(;;){}")?;
        Ok(())
    });
    assert!(matches!(out, Err(JsFault::JsInterrupted { .. })), "{out:?}");
    let start = now.get();
    let out = host.enter("outer", TENTH, |s| {
        for _ in 0..100 {
            host.native(|| now.set(now.get() + Duration::from_secs(1)));
            s.eval::<bool, _>(
                "(() => { let x = 0; for (let i = 0; i < 1000; i++) x += i; return x > 0 })()",
            )?;
        }
        s.eval::<bool, _>(SHORT)
    });
    assert!(matches!(out, Err(JsFault::JsInterrupted { .. })), "{out:?}");
    assert!(now.get() - start < Duration::from_secs(100));
}

#[test]
fn module_exports_become_callable_handles() {
    let host = plain();
    let m = host
        .load_module(
            "calc.js",
            "export function add(a, b) { return a + b; }\n\
             export function names(list) { return list.map(s => s.toUpperCase()); }\n\
             export const notAFunction = 3;\n\
             export function nothing() {}",
        )
        .unwrap();
    assert_eq!(m.exports.len(), 3);
    assert_eq!(m.function("notAFunction"), None);
    let add = m.function("add").unwrap();
    assert_eq!(
        host.call(add, &[2.into(), 40.into()]).unwrap(),
        JsValue::Number(42.0)
    );
    let names = m.function("names").unwrap();
    assert_eq!(
        host.call(names, &[vec!["a", "b"].into()]).unwrap(),
        JsValue::Array(vec!["A".into(), "B".into()])
    );
    assert_eq!(
        host.call(m.function("nothing").unwrap(), &[]).unwrap(),
        JsValue::Undefined
    );
    assert!(m.function_at(0).is_some());

    host.unload_module(&m);
    assert!(matches!(host.call(add, &[]), Err(JsFault::Internal(_))));
}

#[test]
fn value_bridge_round_trips() {
    let host = plain();
    let m = host
        .load_module("id.js", "export function id(x) { return x; }")
        .unwrap();
    let id = m.function("id").unwrap();
    for v in [
        JsValue::Undefined,
        JsValue::Null,
        JsValue::Bool(true),
        JsValue::Number(1.5),
        JsValue::Number(-3.0),
        JsValue::String("héllo".into()),
        JsValue::Array(vec![
            JsValue::Null,
            vec![JsValue::from(1)].into(),
            "x".into(),
        ]),
    ] {
        assert_eq!(host.call(id, std::slice::from_ref(&v)).unwrap(), v);
    }
}

#[test]
fn bytecode_round_trip() {
    let src =
        "export function sq(x) { return x * x; }\nexport function boom() { throw new Error('b'); }";
    let bc = Bytecode::compile("sq.js", src, BytecodeOptions::default()).unwrap();
    assert!(!bc.as_bytes().is_empty());

    let host = plain();
    let m = host.load_module_bytecode(&bc).unwrap();
    assert_eq!(m.name, "sq.js");
    assert_eq!(
        host.call(m.function("sq").unwrap(), &[7.into()]).unwrap(),
        JsValue::Number(49.0)
    );
    // Line tables are kept by default, so diagnostics still work.
    let JsFault::JsException { line, .. } =
        host.call(m.function("boom").unwrap(), &[]).unwrap_err()
    else {
        panic!()
    };
    assert_eq!(line, Some(2));
}

#[test]
fn bytecode_compile_error_is_a_fault() {
    let err = Bytecode::compile("bad.js", "export {", BytecodeOptions::default()).unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
}

#[test]
fn bytecode_is_send_and_buffers_outlive_unloaded_modules() {
    fn assert_send<T: Send>() {}
    assert_send::<Bytecode>();
    let host = plain();
    let m = {
        let bc = Bytecode::compile(
            "a.js",
            "export function f() { return 'still here'; }",
            BytecodeOptions::default(),
        )
        .unwrap();
        host.load_module_bytecode(&bc).unwrap()
        // `bc` dropped here; the host must have kept its own reference.
    };
    host.collect_now();
    assert_eq!(
        host.call(m.function("f").unwrap(), &[]).unwrap(),
        JsValue::from("still here")
    );
}

#[test]
fn promise_jobs_are_pumped_under_a_deadline() {
    let host = plain();
    let m = host
        .load_module(
            "p.js",
            "export let n = 0;\nexport function go() { Promise.resolve().then(() => { globalThis.hit = (globalThis.hit || 0) + 1; }).then(() => { globalThis.hit += 10; }); }\nexport function hit() { return globalThis.hit; }",
        )
        .unwrap();
    host.call(m.function("go").unwrap(), &[]).unwrap();
    assert!(host.has_pending_jobs());
    let ran = host
        .pump_jobs(Instant::now() + Duration::from_millis(50))
        .ran;
    assert_eq!(ran, 2);
    assert!(!host.has_pending_jobs());
    assert_eq!(
        host.call(m.function("hit").unwrap(), &[]).unwrap(),
        JsValue::Number(11.0)
    );
}

#[test]
fn pump_jobs_respects_the_deadline_and_survives_throwing_jobs() {
    let host = plain();
    host.enter("setup", host.config().slice, |s| {
        s.eval::<(), _>("Promise.resolve().then(() => { throw new Error('job'); }); Promise.resolve().then(() => { globalThis.ok = 1; });")
    })
    .unwrap();
    assert_eq!(
        host.pump_jobs(Instant::now() + Duration::from_millis(50))
            .ran,
        2
    );
    // A deadline in the past runs nothing.
    host.enter("setup2", host.config().slice, |s| {
        s.eval::<(), _>("Promise.resolve().then(() => {})")
    })
    .unwrap();
    assert_eq!(host.pump_jobs(Instant::now()).ran, 0);
    assert!(host.has_pending_jobs());
}

#[test]
fn collect_if_idle_runs_only_after_growth() {
    let host = JsHost::new(JsConfig {
        idle_collect_min_growth: 64 * 1024,
        gc_threshold: usize::MAX / 2, // keep automatic GC out of the way
        ..JsConfig::default()
    })
    .unwrap();
    host.enter("alloc", host.config().slice, |s| {
        // A reference cycle only the cycle collector can free.
        s.eval::<(), _>("for (let i = 0; i < 2000; i++) { const a = {}; const b = { a }; a.b = b; a.pad = new Array(50).fill(i); }")
    })
    .unwrap();
    let before = host.memory_stats().unwrap().malloc_size;
    assert!(host.wants_idle_collect());
    assert!(host.collect_if_idle());
    let after = host.memory_stats().unwrap().malloc_size;
    assert!(after < before, "{after} !< {before}");
    // Nothing new allocated: the next call is a no-op, and a window at
    // rest has no reason to wake for one.
    assert!(!host.wants_idle_collect());
    assert!(!host.collect_if_idle());
}

// Inside a run the collector cannot run, so it is not wanted then.
#[test]
fn an_idle_collect_is_not_wanted_inside_a_run() {
    let host = JsHost::new(JsConfig {
        idle_collect_min_growth: 64 * 1024,
        gc_threshold: usize::MAX / 2,
        ..JsConfig::default()
    })
    .unwrap();
    host.enter("alloc", host.config().slice, |s| {
        s.eval::<(), _>("for (let i = 0; i < 2000; i++) { const a = {}; a.self = a; a.pad = new Array(50).fill(i); }")
    })
    .unwrap();
    let inside = host
        .enter(
            "ask",
            host.config().slice,
            |_| Ok(host.wants_idle_collect()),
        )
        .unwrap();
    assert!(!inside);
    assert!(host.wants_idle_collect());
}

// Whether to collect is asked each time a window rests after a script ran,
// so it reads a count the allocator keeps rather than walking the heap.
#[test]
fn heap_bytes_follow_what_scripts_hold_without_a_walk() {
    let host = JsHost::new(JsConfig {
        gc_threshold: usize::MAX / 2,
        ..JsConfig::default()
    })
    .unwrap();
    let start = host.heap_bytes();
    assert!(start > 0, "the runtime itself is on the heap");
    // QuickJS's own count, which only the walk reads out, is of what it
    // gave out; the system's also has what its arena has yet to give.
    let walked = host.memory_stats().unwrap().malloc_size;
    assert!(
        start >= walked / 2 && start <= walked * 2,
        "{start} against {walked}"
    );
    host.enter("alloc", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.keep = []; for (let i = 0; i < 2000; i++) keep.push(new Array(50).fill(i));")
    })
    .unwrap();
    let grown = host
        .enter("inside", host.config().slice, |_| Ok(host.heap_bytes()))
        .unwrap();
    assert!(grown > start + 400 * 1024, "{grown} after {start}");
    host.enter("drop", host.config().slice, |s| {
        s.eval::<(), _>("globalThis.keep = null;")
    })
    .unwrap();
    host.collect_now();
    assert!(
        host.heap_bytes() < grown - 400 * 1024,
        "{} after {grown}",
        host.heap_bytes()
    );
}

#[test]
fn memory_stats_unavailable_inside_a_run() {
    let host = plain();
    host.enter("x", host.config().slice, |_| {
        assert!(host.memory_stats().is_none());
        assert!(!host.collect_now());
        assert_eq!(
            host.pump_jobs(Instant::now() + Duration::from_secs(1)).ran,
            0
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn dropping_a_host_with_live_handles_does_not_abort() {
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 2.0);
    let m = host
        .load_module("m.js", "export function f(o) { return o.width; }")
        .unwrap();
    host.call(m.function("f").unwrap(), &[JsValue::Object(1)])
        .unwrap();
    assert_eq!(host.wrapper_count(), 1);
    drop(host); // QuickJS asserts on leaked objects at teardown.
}

// ---- host objects

#[test]
fn accessor_reads_through_the_trait() {
    let (host, store) = host_with_store();
    store.insert(1, 21.0, 5.0);
    let m = host
        .load_module("a.js", "export function double(o) { return o.width * 2; }")
        .unwrap();
    let f = m.function("double").unwrap();
    assert_eq!(
        host.call(f, &[JsValue::Object(1)]).unwrap(),
        JsValue::Number(42.0)
    );
    assert_eq!(store.gets.get(), 1);
    // Live: a change on the Rust side is visible on the next read.
    store
        .objects
        .borrow_mut()
        .get_mut(&1)
        .unwrap()
        .props
        .insert(WIDTH, JsValue::Number(4.0));
    assert_eq!(
        host.call(f, &[JsValue::Object(1)]).unwrap(),
        JsValue::Number(8.0)
    );
}

#[test]
fn accessor_write_calls_set() {
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 1.0);
    let m = host
        .load_module(
            "w.js",
            "export function grow(o) { o.width = o.height + 9; o.height += 1; }",
        )
        .unwrap();
    host.call(m.function("grow").unwrap(), &[JsValue::Object(1)])
        .unwrap();
    assert_eq!(
        *store.log.borrow(),
        vec![
            "set 1.0 = Number(10.0)".to_string(),
            "set 1.1 = Number(2.0)".to_string()
        ]
    );
}

#[test]
fn the_wrapper_answers_its_properties_with_no_accessor_call() {
    let (host, store) = host_with_store();
    store.insert(1, 3.0, 4.0);
    let m = host
        .load_module(
            "own.js",
            "export function f(o) { o.width = o.height + 1; return o.width; }\n\
             export function g(o) { var d = Object.getOwnPropertyDescriptor(\
             Object.getPrototypeOf(o), 'width'); var c = Object.create(o); var e; \
             try { o.mark = 7; } catch (x) { e = x.message; } c.mark = 8; \
             return [d.get.call(o), e, o.mark, c.mark, 'width' in c]; }",
        )
        .unwrap();
    let before = JsHost::accessor_calls();
    assert_eq!(
        host.call(m.function("f").unwrap(), &[JsValue::Object(1)])
            .unwrap(),
        JsValue::Number(5.0)
    );
    assert_eq!(JsHost::accessor_calls(), before);
    // What asks the prototype still finds the accessors. A name of no
    // property cannot be given to the wrapper, but can to an object made
    // over it.
    assert_eq!(
        host.call(m.function("g").unwrap(), &[JsValue::Object(1)])
            .unwrap(),
        JsValue::Array(vec![
            JsValue::Number(5.0),
            JsValue::String("Cannot assign to non-existent property \"mark\"".into()),
            JsValue::Undefined,
            JsValue::Number(8.0),
            JsValue::Bool(true)
        ])
    );
    assert_eq!(JsHost::accessor_calls(), before + 1);
}

#[test]
fn a_method_of_a_property_name_hides_the_property() {
    let host = JsHost::new(ljos_js::JsConfig::default()).unwrap();
    let store = std::rc::Rc::new(Store::default());
    host.set_host_objects(store.clone());
    let mut def = type_def();
    def.methods[0].name = "width".into();
    let ty = host.register_type(&def).unwrap();
    *store.ty.borrow_mut() = Some(ty);
    store.insert(1, 3.0, 4.0);
    let m = host
        .load_module("hide.js", "export function f(o) { return typeof o.width; }")
        .unwrap();
    assert_eq!(
        host.call(m.function("f").unwrap(), &[JsValue::Object(1)])
            .unwrap(),
        JsValue::String("function".into())
    );
}

#[test]
fn read_only_property_rejects_writes_in_strict_code() {
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 1.0);
    let m = host
        .load_module("ro.js", "export function f(o) { o.ro = 1; }")
        .unwrap();
    let err = host
        .call(m.function("f").unwrap(), &[JsValue::Object(1)])
        .unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
}

#[test]
fn methods_stay_bound_when_read_off_the_object() {
    // Qt.callLater(obj.method) and signal.connect(obj.method) pass the
    // method without its object, so it must keep its `this`.
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 1.0);
    store.insert(2, 1.0, 1.0);
    let m = host
        .load_module(
            "bound.js",
            "export function f(a, b) { var g = a.add; \
             return [a.add === a.add, a.add !== b.add, g(4, 5)]; }",
        )
        .unwrap();
    let out = host
        .call(
            m.function("f").unwrap(),
            &[JsValue::Object(1), JsValue::Object(2)],
        )
        .unwrap();
    assert_eq!(
        out,
        JsValue::Array(vec![
            JsValue::Bool(true),
            JsValue::Bool(true),
            JsValue::Number(9.0)
        ])
    );
    assert!(store.log.borrow().iter().any(|l| l.starts_with("call 1.")));
}

#[test]
fn methods_call_the_trait() {
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 1.0);
    let m = host
        .load_module(
            "c.js",
            "export function f(o) { return [o.add(1, 2, 3), o.echo('a', null, [1])]; }",
        )
        .unwrap();
    let out = host
        .call(m.function("f").unwrap(), &[JsValue::Object(1)])
        .unwrap();
    assert_eq!(
        out,
        JsValue::Array(vec![
            JsValue::Number(6.0),
            JsValue::Array(vec![
                "a".into(),
                JsValue::Null,
                vec![JsValue::from(1)].into()
            ]),
        ])
    );
    assert!(store.log.borrow()[0].starts_with("call 1.0("));
}

#[test]
fn host_errors_become_js_exceptions() {
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 1.0);
    let m = host.load_module("e.js", "export function f(o) { return o.fail; }\nexport function g(o) { try { return o.fail; } catch (e) { return 'caught ' + e.message; } }").unwrap();
    let err = host
        .call(m.function("f").unwrap(), &[JsValue::Object(1)])
        .unwrap_err();
    assert!(
        matches!(&err, JsFault::JsException { message, .. } if message == "Error: boom from getter"),
        "{err:?}"
    );
    assert_eq!(
        host.call(m.function("g").unwrap(), &[JsValue::Object(1)])
            .unwrap(),
        JsValue::from("caught boom from getter")
    );
}

#[test]
fn getter_can_call_back_into_js_without_deadlock() {
    let (host, store) = host_with_store();
    store.insert(1, 3.0, 4.0);
    let m = host
        .load_module(
            "r.js",
            "export function area(o) { return o.width * o.height; }\n\
             export function viaGetter(o) { return o.derived + 1; }",
        )
        .unwrap();
    // `derived` calls `area(o)` from inside the getter, which itself goes
    // through two more accessor reads.
    *store.reenter.borrow_mut() = m.function("area");
    assert_eq!(
        host.call(m.function("viaGetter").unwrap(), &[JsValue::Object(1)])
            .unwrap(),
        JsValue::Number(13.0)
    );
}

#[test]
fn getter_returning_an_object_gives_the_same_wrapper() {
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 1.0);
    store.insert(2, 7.0, 1.0);
    let m = host
        .load_module("i.js", "export function f(o) { return [o.child === o.child, o.child.width, o.child !== o]; }\nexport function same(a, b) { return a === b; }")
        .unwrap();
    let out = host
        .call(m.function("f").unwrap(), &[JsValue::Object(1)])
        .unwrap();
    assert_eq!(
        out,
        JsValue::Array(vec![true.into(), 7.0.into(), true.into()])
    );
    let same = m.function("same").unwrap();
    assert_eq!(
        host.call(same, &[JsValue::Object(2), JsValue::Object(2)])
            .unwrap(),
        true.into()
    );
    assert_eq!(host.wrapper_count(), 2);
}

#[test]
fn wrappers_are_lazy_and_drop_wrapper_forgets_them() {
    let (host, store) = host_with_store();
    store.insert(1, 5.0, 1.0);
    let m = host
        .load_module("l.js", "export function w(o) { return o.width; }")
        .unwrap();
    assert_eq!(host.wrapper_count(), 0);
    let w = m.function("w").unwrap();
    host.call(w, &[JsValue::Object(1)]).unwrap();
    assert_eq!(host.wrapper_count(), 1);
    assert!(host.drop_wrapper(1));
    assert!(!host.drop_wrapper(1));
    // Recreated on demand.
    host.call(w, &[JsValue::Object(1)]).unwrap();
    assert_eq!(host.wrapper_count(), 1);
    // A destroyed object cannot get a wrapper.
    store.objects.borrow_mut().remove(&1);
    host.drop_wrapper(1);
    let err = host.call(w, &[JsValue::Object(1)]).unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
}

#[test]
fn js_owned_wrappers_notify_when_collected() {
    let (host, store) = host_with_store();
    store.insert(9, 1.0, 1.0);
    host.enter("adopt", host.config().slice, |s| {
        let o = s.new_owned(9)?;
        assert_eq!(o.get::<_, f64>("width")?, 1.0);
        Ok(())
    })
    .unwrap();
    // No cycle, so reference counting frees the wrapper as soon as the run
    // ends and the notification is delivered at the end of that run.
    assert_eq!(*store.released.borrow(), vec![9]);
    assert_eq!(
        host.wrapper_count(),
        0,
        "JS-owned wrappers are never cached"
    );
}

#[test]
fn plain_objects_and_functions_cross_by_reference() {
    let host = plain();
    let m = host
        .load_module(
            "o.js",
            "export function f() { return { a: 1, b: 'x' }; }\n\
             export function g() { return function (n) { return this.k + n; }; }\n\
             export function same(a, b) { return a === b; }",
        )
        .unwrap();
    let JsValue::Handle(obj) = host.call(m.function("f").unwrap(), &[]).unwrap() else {
        panic!("expected a handle");
    };
    let entries = host
        .enter("entries", host.config().slice, |s| s.entries(&obj))
        .unwrap();
    assert_eq!(
        entries,
        vec![
            ("a".to_owned(), JsValue::Number(1.0)),
            ("b".to_owned(), JsValue::String("x".into()))
        ]
    );
    let JsValue::Handle(func) = host.call(m.function("g").unwrap(), &[]).unwrap() else {
        panic!("expected a handle");
    };
    let this = JsValue::Map(vec![("k".into(), JsValue::Number(40.0))]);
    assert_eq!(
        host.call_ref(&func, &this, &[2.into()]).unwrap(),
        JsValue::Number(42.0)
    );
    // The same JS value goes back in, so identity survives the round trip.
    let same = m.function("same").unwrap();
    assert_eq!(
        host.call(
            same,
            &[JsValue::Handle(obj.clone()), JsValue::Handle(obj.clone())]
        )
        .unwrap(),
        JsValue::Bool(true)
    );
}

#[test]
fn dropped_refs_are_released_on_the_next_run() {
    let host = plain();
    let m = host
        .load_module("r.js", "export function f() { return {}; }")
        .unwrap();
    let f = m.function("f").unwrap();
    let before = host.memory_stats().unwrap().object_count;
    let refs: Vec<_> = (0..100).map(|_| host.call(f, &[]).unwrap()).collect();
    drop(refs);
    host.flush_released();
    host.collect_now();
    let after = host.memory_stats().unwrap().object_count;
    assert!(after <= before + 2, "before {before}, after {after}");
}

/// A run can last as long as a test function that waits, where everything
/// else runs nested inside it: what Rust lets go of meanwhile is freed
/// there, not only once the outermost run ends.
#[test]
fn refs_dropped_inside_a_long_run_are_freed_by_nested_runs() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    let slice = host.config().slice;
    // About 2 MB each, 60 MB in all: over the limit unless freed as it goes.
    let make =
        "(() => { const a = []; for (let i = 0; i < 25000; i++) a.push({ i }); return a; })()";
    host.enter("outer", slice, |s| {
        for _ in 0..30 {
            let v: ljos_js::rquickjs::Value = s.eval(make)?;
            drop(s.keep(v));
            host.enter("nested", slice, |_| Ok(())).map_err(|e| {
                ljos_js::rquickjs::Error::new_from_js_message("nested", "run", e.to_string())
            })?;
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn global_functions_and_scripts() {
    let host = plain();
    host.set_global_function(
        "twice",
        std::rc::Rc::new(
            |_s: &ljos_js::Scope<'_>, args: &[JsValue]| match args.first() {
                Some(JsValue::Number(n)) => Ok(JsValue::Number(n * 2.0)),
                _ => Err(ljos_js::HostError::msg("twice needs a number")),
            },
        ),
    )
    .unwrap();
    assert_eq!(
        host.eval_script("t.js", "twice(21)").unwrap(),
        JsValue::Number(42.0)
    );
    let err = host.eval_script("t.js", "twice('a')").unwrap_err();
    assert!(
        matches!(&err, JsFault::JsException { message, .. } if message == "Error: twice needs a number"),
        "{err:?}"
    );
}

#[test]
fn soft_budget_overrun_does_not_fail_the_call() {
    let host = plain();
    let slice = Slice {
        soft: Duration::ZERO,
        hard: Duration::from_secs(1),
    };
    assert_eq!(
        host.enter("slow", slice, |s| s.eval::<f64, _>("1"))
            .unwrap(),
        1.0
    );
}

#[test]
fn the_overrun_is_named_and_only_then() {
    let host = plain();
    let named = std::cell::Cell::new(0);
    let what = || {
        named.set(named.get() + 1);
        "Main.qml:3:5".to_owned()
    };
    let roomy = Slice {
        soft: Duration::from_secs(1),
        hard: Duration::from_secs(2),
    };
    host.enter_naming("binding", what, roomy, |s| s.eval::<f64, _>("1"))
        .unwrap();
    assert_eq!(named.get(), 0);
    let tight = Slice {
        soft: Duration::ZERO,
        hard: Duration::from_secs(1),
    };
    host.enter_naming("binding", what, tight, |s| s.eval::<f64, _>("1"))
        .unwrap();
    assert_eq!(named.get(), 1);
}

#[test]
fn panic_inside_a_host_callback_is_a_fault() {
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 1.0);
    let m = host
        .load_module(
            "pn.js",
            "export function f(o) { return o.echo(PANIC); }\nvar PANIC = 'panic';",
        )
        .unwrap();
    let err = host
        .call(m.function("f").unwrap(), &[JsValue::Object(1)])
        .unwrap_err();
    assert!(matches!(err, JsFault::Panic { .. }), "{err:?}");
    assert_eq!(eval_num(&host, "1 + 2").unwrap(), 3.0);
}

#[test]
fn a_panic_in_a_promise_job_is_a_fault_of_the_pump() {
    let (host, store) = host_with_store();
    store.insert(1, 1.0, 1.0);
    let m = host
        .load_module(
            "pj.js",
            "export function go(o) { Promise.resolve().then(() => o.echo('panic')); Promise.resolve().then(() => { globalThis.later = 1; }); }",
        )
        .unwrap();
    host.call(m.function("go").unwrap(), &[JsValue::Object(1)])
        .unwrap();
    let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(1));
    assert_eq!(pumped.ran, 1);
    assert!(
        matches!(&pumped.faults[..], [JsFault::Panic { label, .. }] if label == "promise job"),
        "{:?}",
        pumped.faults
    );
    // The panic is not kept for the next dispatch, and the job after it
    // still runs on the next pump.
    assert_eq!(eval_num(&host, "1 + 2").unwrap(), 3.0);
    let pumped = host.pump_jobs(Instant::now() + Duration::from_secs(1));
    assert_eq!(pumped.ran, 1);
    assert!(pumped.faults.is_empty(), "{:?}", pumped.faults);
    assert_eq!(eval_num(&host, "globalThis.later").unwrap(), 1.0);
}

#[test]
fn a_promise_job_that_never_ends_is_interrupted_at_the_deadline() {
    let host = plain();
    host.enter("setup", host.config().slice, |s| {
        s.eval::<(), _>("Promise.resolve().then(() => { for (;;) {} })")
    })
    .unwrap();
    let pumped = host.pump_jobs(Instant::now() + Duration::from_millis(20));
    assert_eq!(pumped.ran, 1);
    assert!(
        matches!(&pumped.faults[..], [JsFault::JsInterrupted { label, .. }] if label == "promise job"),
        "{:?}",
        pumped.faults
    );
    assert!(!host.has_pending_jobs());
}

#[test]
fn faults_convert_for_the_supervisor() {
    let f = ljos_js::JsFault::JsOutOfMemory {
        label: "binding".into(),
        what: String::new(),
    };
    assert_eq!(
        ljos_runtime::Fault::from(f),
        ljos_runtime::Fault::JsOutOfMemory
    );
    let f = ljos_js::JsFault::JsException {
        message: "x is not defined".into(),
        stack: String::new(),
        file: Some("Main.qml".into()),
        line: Some(7),
        thrown: None,
    };
    let ljos_runtime::Fault::JsException(info) = ljos_runtime::Fault::from(f) else {
        panic!("expected an exception");
    };
    assert_eq!(info.to_string(), "Main.qml:7:0: x is not defined");
    assert_eq!(info.stack, None);
}

/// A call QuickJS words as unknown, with a script's exception pending, is
/// that exception, and keeps the value thrown as a thrown one does. A
/// script cannot reach this on purpose: the run's own code stands in for
/// the QuickJS call that left it.
#[test]
fn an_exception_pending_behind_an_unknown_error_keeps_its_value() {
    let host = plain();
    let err = host
        .enter("unknown", host.config().slice, |s| {
            let v = s.eval::<ljos_js::rquickjs::Value, _>("new TypeError('mine')")?;
            let _ = s.ctx().throw(v);
            Err::<(), _>(ljos_js::rquickjs::Error::Unknown)
        })
        .unwrap_err();
    let JsFault::JsException {
        message, thrown, ..
    } = err
    else {
        panic!("{err:?}")
    };
    assert!(message.contains("mine"), "{message}");
    assert!(thrown.is_some(), "the value thrown is kept");
}

/// rquickjs's unknown error with nothing pending is a call into QuickJS
/// that gave nothing back. With no limit, or with room to spare, that is
/// a bug in the engine, not the heap running out.
#[test]
fn an_unknown_error_with_nothing_pending_and_room_to_spare_is_internal() {
    // The default limit (96 MiB), with room to spare, and no limit at all.
    let unlimited = JsHost::new(JsConfig {
        memory_limit: None,
        ..JsConfig::default()
    })
    .unwrap();
    for host in [plain(), unlimited] {
        let err = host
            .enter("unknown", host.config().slice, |s| {
                assert!(!s.ctx().has_exception());
                Err::<(), _>(ljos_js::rquickjs::Error::Unknown)
            })
            .unwrap_err();
        assert!(
            matches!(err, JsFault::Internal(_)),
            "{:?}: {err:?}",
            host.config().memory_limit
        );
    }
}

/// With nothing pending the heap is judged by the most the run held, as
/// for a thrown `null`: the call that failed may have freed what it took.
/// So a run that came near its limit and let go, then met an unknown
/// error that has nothing to do with the heap, is told as running out.
/// Kept on purpose: a Loader told of it collects and loads again a
/// counted number of times (`Again::Counted`), where taking a full heap
/// for a bug would leave it stopped for good.
#[test]
fn an_unknown_error_with_nothing_pending_after_the_heap_came_near_its_limit_is_running_out() {
    let host = JsHost::new(JsConfig {
        memory_limit: Some(16 * 1024 * 1024),
        ..JsConfig::default()
    })
    .unwrap();
    host.collect_now();
    let held = host.memory_stats().unwrap().malloc_size;
    assert!(host.set_memory_limit(Some(held + 4 * 1024 * 1024)));
    let err = host
        .enter("unknown", host.config().slice, |s| {
            // A chain no cycle holds: freed as the catch leaves the block.
            let filled = s.eval::<bool, _>(
                "(() => { try { let h = null; for (;;) h = { n: h } } catch (e) { return true } })()",
            )?;
            assert!(filled);
            assert!(!s.ctx().has_exception());
            Err::<(), _>(ljos_js::rquickjs::Error::Unknown)
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::JsOutOfMemory { .. }), "{err:?}");
    // The next run's peak is its own: with room, the same error is a bug.
    let err = host
        .enter("unknown", host.config().slice, |_| {
            Err::<(), _>(ljos_js::rquickjs::Error::Unknown)
        })
        .unwrap_err();
    assert!(matches!(err, JsFault::Internal(_)), "{err:?}");
    assert_eq!(eval_num(&host, "40 + 2").unwrap(), 42.0);
}

/// What a value reaches is counted for the first group that holds it, a
/// getter is not run to find it, and the walk stops at its budget.
#[test]
fn what_each_group_holds_is_counted_once_without_running_getters() {
    let host = plain();
    let (big, small) = host
        .enter("make", host.config().slice, |s| {
            let big = s.eval::<rquickjs::Value, _>(
                "globalThis.shared = []; \
                 for (let i = 0; i < 10000; i++) shared.push({ i, s: 'row ' + i }); \
                 ({ rows: shared })",
            )?;
            let small = s.eval::<rquickjs::Value, _>(
                "globalThis.ran = false; \
                 ({ rows: shared, get spy() { globalThis.ran = true; return [] } })",
            )?;
            Ok((s.keep(big), s.keep(small)))
        })
        .unwrap();

    let held = host
        .enter("walk", host.config().slice, |s| {
            Ok(s.held_by([vec![&big], vec![&small]], usize::MAX))
        })
        .unwrap();
    assert!(!held.truncated);
    assert!(held.values > 20_000, "{held:?}");
    assert!(held.bytes[0] > 10_000 * 64, "{held:?}");
    assert!(
        held.bytes[1] < 1024,
        "the rows were the first group's: {held:?}"
    );
    assert_eq!(
        eval_num(&host, "ran ? 1 : 0").unwrap(),
        0.0,
        "no getter ran"
    );

    let cut = host
        .enter("walk", host.config().slice, |s| {
            Ok(s.held_by([vec![&big]], 100))
        })
        .unwrap();
    assert!(cut.truncated);
    assert_eq!(cut.values, 100);
}
