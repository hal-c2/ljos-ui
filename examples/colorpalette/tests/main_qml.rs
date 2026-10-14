//! The example's own QML, whole: every file of
//! `corpus/qtdoc/examples/demos/colorpaletteclient` compiled the
//! way the gallery compiles it (`Project::load` over the example
//! directory, the harness shape of
//! `crates/ljos_controls/tests/navigation.rs`), then created
//! against an Engine this crate's types are registered on. No
//! network is touched: nothing fetches until a server is picked,
//! and this test never picks one.
//!
//! Two toolkit gaps stand between this backend and a whole
//! `Main.qml` today, and the tests pin both exactly instead of
//! hiding them:
//!
//! - The front end cannot know native types yet (docs/MIGRATION.md
//!   §6: they "do not reach the compiler's table of types"), so
//!   the compile half allows only the four "`X` is not a type of
//!   anything the file imports" for this crate's three types in
//!   Main.qml. Any other error — a missing member, a bad
//!   binding — fails the test now; when the front end learns
//!   registered types, those four go to zero and it still passes.
//! - `QtQuick.Effects` (`MultiEffect`, used by ColorView.qml and
//!   UserMenu.qml for avatar masks) is not implemented in Ljós
//!   at all, so creating Main stops there. Until it lands, the
//!   first screen — the real `ServerSelection.qml`, which needs
//!   no Effects — is created with this backend wired to it the
//!   way Main.qml:21-57 wires it, which is the object tree the
//!   user actually sees first. When Effects arrives, the Main
//!   creation in the first test starts succeeding and takes over.

use std::path::PathBuf;

use ljos::prelude::*;
use ljos_compiler::{Project, Severity, compile_file};

fn example_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/qtdoc/examples/demos/colorpaletteclient")
}

/// The corpus is a submodule the check job does not check out; without
/// it a test says on the terminal that it was skipped and why, and
/// passes (the convention of `crates/ljos_compiler/tests/corpus.rs`).
fn corpus_present(test: &str) -> bool {
    let dir = example_dir();
    if dir.is_dir() {
        return true;
    }
    use std::io::Write as _;
    // To the stream itself: the harness swallows what a passing test
    // prints.
    let _ = writeln!(
        std::io::stderr(),
        "SKIPPED {test}: no corpus at {} (`git submodule update --init corpus/qtdoc`)",
        dir.display()
    );
    false
}

/// Compiles the whole example (plus the in-memory `Backend`
/// wiring file below) and returns an Engine holding the compiled
/// files and this crate's types.
fn engine() -> Engine {
    let dir = example_dir();
    let mut project = Project::load(&dir, qml_front::QtTypeOracle)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    // Main.qml's own wiring of the backend (its lines 39-57),
    // alone, so the first screen can be given the very objects
    // Main would give it while Main itself stops at Effects.
    project.add(
        "ColorPalette/Backend",
        r#"
import QtQuick
RestService {
    PaginatedResource { path: "users" }
    PaginatedResource { path: "unknown" }
    BasicLogin { loginPath: "login"; logoutPath: "logout" }
}
"#,
    );

    let mut source = ljos::MapSource::new();
    let mut unexpected = Vec::new();
    let mut native_type_errors = 0;
    for key in project.files().map(str::to_owned).collect::<Vec<_>>() {
        let compiled = compile_file(&project, &key).unwrap_or_else(|d| panic!("{key}: {d:?}"));
        for d in &compiled.diagnostics {
            if d.severity != Severity::Error {
                continue;
            }
            let ours = ["RestService", "PaginatedResource", "BasicLogin"]
                .iter()
                .any(|t| d.message.contains(&format!("`{t}` is not a type")));
            if ours && key == "ColorPalette/Main" {
                native_type_errors += 1;
            } else if !ours || key != "ColorPalette/Backend" {
                unexpected.push(format!("{key}: {d}"));
            }
        }
        source.insert(&key, compiled.file);
    }
    assert!(
        unexpected.is_empty(),
        "errors beyond the native-type gap:\n{}",
        unexpected.join("\n")
    );
    // Four today (RestService once, PaginatedResource twice,
    // BasicLogin once); zero once the front end knows them.
    assert!(native_type_errors == 0 || native_type_errors == 4);

    let engine = ljos::engine();
    ljos_example_colorpalette::register(&engine);
    engine.set_component_source(source);
    engine
}

fn walk(e: &Engine, root: ObjectId) -> Vec<ObjectId> {
    let mut all = Vec::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        all.push(o);
        stack.extend(e.children(o));
    }
    all
}

/// The object whose `path` property reads `path`: one of the two
/// resources, found by what the QML wrote on it.
fn resource(e: &Engine, roots: &[ObjectId], path: &str) -> ObjectId {
    roots
        .iter()
        .flat_map(|&r| walk(e, r))
        .find(|&o| e.read_by_name(o, "path").to_js_string().as_str() == path)
        .unwrap_or_else(|| panic!("no resource with path {path}"))
}

/// What holds of the backend however it was created: the state
/// the first screen reads before any server is picked.
fn assert_backend(e: &Engine, roots: &[ObjectId]) {
    let all: Vec<ObjectId> = roots.iter().flat_map(|&r| walk(e, r)).collect();

    // The RestService: nothing picked yet, and no TLS to offer.
    let service = all
        .iter()
        .copied()
        .find(|&o| matches!(e.read_by_name(o, "sslSupported"), Value::Bool(false)))
        .expect("no RestService");
    assert_eq!(e.read_by_name(service, "url").to_js_string().as_str(), "");

    // Both resources, on their first page, with no rows yet: the
    // list the views bind exists and is empty, not undefined.
    for path in ["users", "unknown"] {
        let r = resource(e, roots, path);
        assert_eq!(e.read_by_name(r, "page").to_number(), 1.0, "{path}");
        assert_eq!(e.read_by_name(r, "pages").to_number(), 0.0, "{path}");
        match e.read_by_name(r, "data") {
            Value::List(rows) => assert!(rows.is_empty(), "{path}"),
            other => panic!("{path}: data is {other:?}, not a list"),
        }
    }

    // The login: logged out, nobody's session.
    let login = all
        .iter()
        .copied()
        .find(|&o| e.read_by_name(o, "loginPath").to_js_string().as_str() == "login")
        .expect("no BasicLogin");
    assert_eq!(
        e.read_by_name(login, "logoutPath").to_js_string().as_str(),
        "logout"
    );
    assert!(!e.read_by_name(login, "loggedIn").to_boolean());
    assert_eq!(e.read_by_name(login, "user").to_js_string().as_str(), "");

    // Asking for a page before a server is picked is a logged
    // no-op, not an error or a panic; so is logging out.
    let users = resource(e, roots, "users");
    e.invoke_by_name(users, "refreshCurrentPage", &[])
        .expect("refreshCurrentPage");
    assert_eq!(e.read_by_name(users, "page").to_number(), 1.0);
    e.invoke_by_name(login, "logout", &[]).expect("logout");
    assert!(!e.read_by_name(login, "loggedIn").to_boolean());
}

#[test]
fn main_qml_creates_or_stops_only_at_the_missing_effects_module() {
    if !corpus_present("main_qml_creates_or_stops_only_at_the_missing_effects_module") {
        return;
    }
    let e = engine();
    match e.create_from_url("ColorPalette/Main", None, Vec::new()) {
        Ok(root) => assert_backend(&e, &[root]),
        Err(err) => assert!(
            err.to_string()
                .contains("module \"QtQuick.Effects\" is not installed"),
            "Main stopped somewhere else: {err}"
        ),
    }
}

#[test]
fn the_first_screen_creates_with_the_backend_wired_as_main_wires_it() {
    if !corpus_present("the_first_screen_creates_with_the_backend_wired_as_main_wires_it") {
        return;
    }
    let e = engine();
    let backend = e
        .create_from_url("ColorPalette/Backend", None, Vec::new())
        .unwrap_or_else(|err| panic!("Backend: {err}"));
    let colors = resource(&e, &[backend], "unknown");
    let users = resource(&e, &[backend], "users");

    // ServerSelection's required properties, given exactly what
    // Main.qml:21-27 gives them.
    let screen = e
        .create_from_url(
            "ColorPalette/ServerSelection",
            None,
            vec![
                ("colorResources".to_owned(), Value::Object(colors)),
                ("colorUsers".to_owned(), Value::Object(users)),
                ("restPalette".to_owned(), Value::Object(backend)),
            ],
        )
        .unwrap_or_else(|err| panic!("ServerSelection: {err}"));

    // The picker is up: something in its tree is showing.
    assert!(
        walk(&e, screen)
            .iter()
            .any(|&o| e.read_by_name(o, "visible").to_boolean()),
        "nothing visible in the first screen's tree"
    );
    assert_backend(&e, &[backend, screen]);
}
