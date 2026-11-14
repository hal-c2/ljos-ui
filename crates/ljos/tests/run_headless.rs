//! An application built with `App` runs with no window and no display,
//! as `ljos run --headless` runs a file: the same builder, the same
//! setup, the shell's headless loop.

use std::cell::RefCell;
use std::rc::Rc;

const MAIN: &str = r#"
import QtQuick
Item {
    id: root
    width: 10; height: 10
    property string platform: Qt.platform.pluginName
    property int fired: 0
    Timer {
        interval: 10; running: true
        onTriggered: { root.fired = 1; Qt.quit() }
    }
}
"#;

fn compile(qml: &str) -> ljos::MapSource {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", qml);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "Main: {:?}", c.diagnostics);
    let mut source = ljos::MapSource::new();
    source.insert("Main", c.file);
    source
}

/// Runs `run` on its own thread and gives back what it returns, or fails
/// the test if the run has not ended within a limit: a run that never
/// quits would hang the test instead of failing it.
fn within_limit<T: Send + 'static>(run: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run());
    });
    rx.recv_timeout(std::time::Duration::from_secs(60))
        .expect("the headless run ends when the application quits")
}

#[test]
fn an_app_runs_headless_until_it_quits() {
    let (platform, fired) = within_limit(|| {
        // The shell's Engine and the root, kept past the run: read once
        // the run has returned, after the timer fired on the real clock.
        let kept = Rc::new(RefCell::new(None));
        let out = Rc::clone(&kept);
        ljos::App::new("Headless")
            .size(64.0, 48.0)
            .source(compile(MAIN))
            .run_headless_with(move |setup| {
                let root = setup
                    .engine()
                    .create_from_url("Main", None, Vec::new())
                    .map_err(|e| -> ljos::SetupError { e.to_string().into() })?;
                *out.borrow_mut() = Some((Rc::clone(setup.engine()), root));
                Ok(root)
            })
            .expect("the headless run");
        let (engine, root) = kept.borrow_mut().take().expect("the setup ran");
        let platform = engine
            .read_by_name(root, "platform")
            .to_js_string()
            .to_string();
        (platform, engine.read_by_name(root, "fired").to_number())
    });
    assert_eq!(platform, "offscreen", "Qt.platform.pluginName headless");
    assert_eq!(fired, 1.0);
}

#[test]
fn an_app_with_a_root_runs_headless() {
    within_limit(|| {
        ljos::App::new("Headless")
            .source(compile(MAIN))
            .root("Main")
            .run_headless()
            .expect("the headless run");
    });
}
