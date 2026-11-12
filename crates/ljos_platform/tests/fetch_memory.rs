//! What a `fetch` leaves behind on the signal it was given. A component
//! gives one signal of its own to every fetch it makes, and aborts it only
//! when it goes: a request may hold nothing on that signal once it is gone
//! (docs/DIALECT.md §4.19).

use ljos_engine::Engine;

fn engine() -> Engine {
    use ljos_compiler::{Project, compile_file};
    let e = Engine::new();
    ljos_quick::register(&e);
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", "import QtQuick\nItem {}\n");
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"));
    e.eval_js(
        "globalThis.kept = new AbortController();\n\
         globalThis.make = (n) => {\n\
             for (let i = 0; i < n; i++) new Request(\"https://example.org/\" + i, { signal: kept.signal });\n\
         };",
    )
    .expect("the script runs");
    e
}

fn heap_after_making(e: &Engine, n: usize) -> usize {
    e.eval_js(&format!("make({n})"))
        .expect("the requests are made");
    assert!(e.collect_garbage(), "the collector runs");
    e.script_heap_bytes().expect("a script ran")
}

#[test]
fn requests_given_one_signal_hold_nothing_on_it_once_they_are_gone() {
    let e = engine();
    let before = heap_after_making(&e, 1_000);
    let after = heap_after_making(&e, 20_000);
    // A signal following the given one for each request is some hundred
    // bytes: twenty thousand of them would be megabytes.
    assert!(
        after.saturating_sub(before) < 256 * 1024,
        "the heap grew from {before} to {after} bytes"
    );
}

#[test]
fn a_signal_of_signals_made_by_any_follows_what_they_were_made_of() {
    let e = engine();
    let got = e
        .eval_js(
            "const c = new AbortController();\n\
             const inner = AbortSignal.any([c.signal]);\n\
             const outer = AbortSignal.any([inner, AbortSignal.any([inner])]);\n\
             const heard = [];\n\
             outer.onabort = () => heard.push(\"outer \" + inner.aborted);\n\
             c.abort(\"gone\");\n\
             [outer.aborted, outer.reason, heard.join()].join(\" \")",
        )
        .expect("the script runs");
    assert_eq!(got.to_js_string().as_str(), "true gone outer true");
}

#[test]
fn a_request_s_signal_follows_the_given_one_when_it_is_read() {
    let e = engine();
    let got = e
        .eval_js(
            "const c = new AbortController();\n\
             const r = new Request(\"https://example.org/\", { signal: c.signal });\n\
             const s = r.signal;\n\
             const copied = new Request(r).signal;\n\
             const same = r.signal === s;\n\
             const before = s.aborted;\n\
             c.abort(\"gone\");\n\
             [same, before, s.aborted, s.reason, copied.aborted, copied.reason,\n\
              new Request(\"https://example.org/\").signal.aborted].join(\" \")",
        )
        .expect("the script runs");
    assert_eq!(
        got.to_js_string().as_str(),
        "true false true gone true gone false"
    );
}
