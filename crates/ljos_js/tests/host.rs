mod common;

use std::time::{Duration, Instant};

use common::*;
use ljos_js::{Bytecode, BytecodeOptions, JsConfig, JsFault, JsHost, JsValue, Slice};

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
fn nested_enter_keeps_the_outer_deadline() {
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
    let ran = host.pump_jobs(Instant::now() + Duration::from_millis(50));
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
        host.pump_jobs(Instant::now() + Duration::from_millis(50)),
        2
    );
    // A deadline in the past runs nothing.
    host.enter("setup2", host.config().slice, |s| {
        s.eval::<(), _>("Promise.resolve().then(() => {})")
    })
    .unwrap();
    assert_eq!(host.pump_jobs(Instant::now()), 0);
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
    assert!(host.collect_if_idle());
    let after = host.memory_stats().unwrap().malloc_size;
    assert!(after < before, "{after} !< {before}");
    // Nothing new allocated: the next call is a no-op.
    assert!(!host.collect_if_idle());
}

#[test]
fn memory_stats_unavailable_inside_a_run() {
    let host = plain();
    host.enter("x", host.config().slice, |_| {
        assert!(host.memory_stats().is_none());
        assert!(!host.collect_now());
        assert_eq!(host.pump_jobs(Instant::now() + Duration::from_secs(1)), 0);
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
fn faults_convert_for_the_supervisor() {
    let f = ljos_js::JsFault::JsOutOfMemory {
        label: "binding".into(),
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
