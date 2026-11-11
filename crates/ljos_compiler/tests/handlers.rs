//! A handler of a signal whose arguments are not all named, in a file of
//! Ljós's API: the table leaves an argument unnamed where Qt's handler
//! reaches it only by declaring it (`Tap.tapped(eventPoint, button)`), and
//! a check of such a handler once never ended (#394).

mod oracle;

use std::{sync::mpsc, thread, time::Duration};

use ljos_compiler::{Code, Project, Severity, compile_file};
use oracle::Runtime;

/// A check of a file this small takes milliseconds; one that runs past
/// this does not end, and the test fails instead of hanging the run.
const DEADLINE: Duration = Duration::from_secs(20);

/// The errors the checker finds in `source`, or a failure when the check
/// does not end.
fn errors(source: &'static str) -> Vec<(Code, String)> {
    let (said, heard) = mpsc::channel();
    // A check that never ends keeps its thread, which ends with the test
    // process.
    thread::spawn(move || {
        let mut project = Project::new(Runtime::new());
        project.add("Main", source);
        let diagnostics = match compile_file(&project, "Main") {
            Ok(compiled) => compiled.diagnostics,
            Err(diagnostics) => diagnostics,
        };
        let errors: Vec<_> = diagnostics
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| (d.code, d.message))
            .collect();
        let _ = said.send(errors);
    });
    heard
        .recv_timeout(DEADLINE)
        .unwrap_or_else(|e| panic!("the check of the file did not end in {DEADLINE:?}: {e}"))
}

#[test]
fn an_empty_handler_of_a_signal_with_an_unnamed_argument_is_checked() {
    // The file starts with a word, as every file that starts with
    // `import` does.
    assert_eq!(
        errors("import Ljos.Ui\nItem {\n    Tap { onTapped: {} }\n}\n"),
        []
    );
}

#[test]
fn a_handler_expression_of_a_signal_with_an_unnamed_argument_is_checked() {
    assert_eq!(
        errors(
            "import Ljos.Ui\nItem {\n    property int n: 0\n    Tap { onTapped: n = n + 1 }\n}\n"
        ),
        []
    );
}

#[test]
fn a_handler_that_declares_the_unnamed_argument_is_checked() {
    assert_eq!(
        errors("import Ljos.Ui\nItem {\n    Tap { onTapped: (eventPoint, button) => {} }\n}\n"),
        []
    );
}

#[test]
fn an_unnamed_argument_is_in_scope_by_no_name() {
    // `button` is the item's id, not the argument the table leaves unnamed.
    assert_eq!(
        errors(
            "import Ljos.Ui\nItem {\n    id: button\n    property int n: 0\n    Tap { onTapped: button.n = button.n + 1 }\n}\n"
        ),
        []
    );
}
