//! The module as a whole, without a compiler: what it registers, and a
//! run over a file that is not there. Runs over real test files are in
//! `ljos_cli`, which is what compiles them.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{Engine, MapSource, QmlType as _};

use super::*;

fn lines() -> (Rc<RefCell<Vec<String>>>, LineSink) {
    let lines = Rc::new(RefCell::new(Vec::new()));
    let out = lines.clone();
    (
        lines,
        Box::new(move |l| out.borrow_mut().push(l.to_owned())),
    )
}

#[test]
fn the_module_describes_its_types_for_a_compiler() {
    let types = describe();
    let case = types.iter().find(|t| t.name == "TestCase").unwrap();
    assert_eq!(case.base.as_deref(), Some("Item"));
    assert!(case.creatable);
    for property in [
        "name",
        "when",
        "windowShown",
        "completed",
        "running",
        "optional",
    ] {
        assert!(
            case.properties.iter().any(|p| p.name == property),
            "TestCase.{property}"
        );
    }
    let read_only = |name: &str| {
        case.properties
            .iter()
            .find(|p| p.name == name)
            .unwrap()
            .readonly
    };
    assert!(read_only("windowShown") && read_only("completed") && read_only("running"));
    assert!(!read_only("when"));
    for method in [
        "compare",
        "verify",
        "fuzzyCompare",
        "fail",
        "skip",
        "expectFail",
        "tryCompare",
        "tryVerify",
        "wait",
        "sleep",
        "createTemporaryObject",
        "createTemporaryQmlObject",
        "findChild",
        "waitForRendering",
        "waitForItemPolished",
        "mouseClick",
        "mousePress",
        "mouseRelease",
        "mouseMove",
        "mouseDoubleClickSequence",
        "mouseDrag",
        "mouseWheel",
        "keyClick",
        "keyPress",
        "keyRelease",
        "keySequence",
        "touchEvent",
        "grabImage",
    ] {
        assert!(
            case.methods.iter().any(|m| m.name == method),
            "TestCase.{method}"
        );
    }

    let spy = types.iter().find(|t| t.name == "SignalSpy").unwrap();
    for property in ["target", "signalName", "count", "signalArguments", "valid"] {
        assert!(
            spy.properties.iter().any(|p| p.name == property),
            "SignalSpy.{property}"
        );
    }
    for method in ["clear", "wait"] {
        assert!(
            spy.methods.iter().any(|m| m.name == method),
            "SignalSpy.{method}"
        );
    }
    let touch = types
        .iter()
        .find(|t| t.name == "TouchEventSequence")
        .unwrap();
    assert!(!touch.creatable);
    for method in ["press", "move", "release", "stationary", "commit"] {
        assert!(
            touch.methods.iter().any(|m| m.name == method),
            "touch.{method}"
        );
    }
    let image = types.iter().find(|t| t.name == "GrabbedImage").unwrap();
    for method in ["pixel", "red", "green", "blue", "alpha", "equals", "save"] {
        assert!(
            image.methods.iter().any(|m| m.name == method),
            "image.{method}"
        );
    }
}

#[test]
fn registering_twice_is_registering_once() {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    register(&engine);
    register(&engine);
    let modules = engine.type_dump().modules;
    assert_eq!(modules.iter().filter(|m| m.uri == URI).count(), 1);
}

#[test]
fn a_test_case_shows_nothing_and_is_ready_at_once() {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    register(&engine);
    let case = engine
        .create_native_incomplete(TestCase::default(), None)
        .unwrap();
    assert!(engine.get(case, TestCase::WHEN).unwrap());
    assert!(!engine.get(case, TestCase::COMPLETED).unwrap());
    assert!(!engine.get(case, ljos_quick::Item::VISIBLE).unwrap());
    assert!(
        engine
            .meta_of(case)
            .unwrap()
            .inherits(ljos_quick::Item::static_meta())
    );
}

#[test]
fn a_check_outside_a_run_is_an_error_and_not_a_crash() {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    register(&engine);
    let case = engine
        .create_native_incomplete(TestCase::default(), None)
        .unwrap();
    // No run is in progress: there is no scene to wait on.
    let waited = engine.invoke_by_name(case, "wait", &[ljos_engine::Value::Int(10)]);
    assert!(waited.unwrap_err().to_string().contains("no test run"));
    // A check still checks.
    let ok = engine.invoke_by_name(case, "verify", &[ljos_engine::Value::Bool(true)]);
    assert!(ok.is_ok());
    let failed = engine.invoke_by_name(case, "verify", &[ljos_engine::Value::Bool(false)]);
    assert!(failed.is_err());
}

#[test]
fn a_file_that_cannot_be_made_is_a_failed_test_named_after_it() {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    engine.set_component_source(MapSource::new());
    let (out, sink) = lines();
    let (_, diagnostics) = lines();
    let mut session = Session::new(Options::default(), sink, diagnostics);
    session.run_file(&engine, "tst_missing", "tst_missing");
    let report = session.finish();
    assert_eq!(report.totals.failed, 1);
    assert_eq!(report.exit_code(), 1);
    assert_eq!(report.tests[0].case, "tst_missing");
    assert_eq!(report.tests[0].function, "compile");
    let out = out.borrow();
    assert!(
        out.iter()
            .any(|l| l.starts_with("FAIL!  : tst_missing::compile() ")),
        "{out:#?}"
    );
    assert!(
        out.iter()
            .any(|l| l.starts_with("Totals: 0 passed, 1 failed, 0 skipped"))
    );
}

#[test]
fn a_run_of_nothing_reports_nothing_and_succeeds() {
    let (out, sink) = lines();
    let (_, diagnostics) = lines();
    let session = Session::new(Options::default(), sink, diagnostics);
    let report = session.finish();
    assert!(report.passed());
    assert!(out.borrow().is_empty());
    assert_eq!(report.to_json()["totals"]["passed"], 0);
}
