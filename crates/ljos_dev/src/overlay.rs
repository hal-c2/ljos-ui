//! What the error overlay says. Painting it is the host's: this is the
//! text, so every host says the same.

use ljos_engine::{Engine, FaultReport, ReloadFault};

/// A reload error, or a fault no boundary contained, as a developer reads
/// it: where, what, and what happens next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub title: String,
    /// `Main.qml:12:5`, `Main.qml`, or empty when the fault is in no one
    /// file.
    pub location: String,
    pub message: String,
    /// What the developer can count on.
    pub hint: String,
}

impl Overlay {
    /// The overlay for `fault`; `more` errors came with it.
    #[must_use]
    pub fn of(fault: &ReloadFault, more: usize) -> Self {
        let location = match (fault.file.is_empty(), fault.line) {
            (true, _) => String::new(),
            (false, 0) => fault.file.clone(),
            (false, line) if fault.column == 0 => format!("{}:{line}", fault.file),
            (false, line) => format!("{}:{line}:{}", fault.file, fault.column),
        };
        let hint = match more {
            0 => "The application runs as it did before this change. Save the file to try again."
                .to_owned(),
            1 => "And 1 more error. The application runs as it did before this change.".to_owned(),
            n => format!("And {n} more errors. The application runs as it did before this change."),
        };
        Self {
            title: "Reload failed".to_owned(),
            location,
            message: fault.message.clone(),
            hint,
        }
    }

    /// The overlay for a fault that reached the application because no
    /// boundary around it took it.
    #[must_use]
    pub fn of_fault(report: &FaultReport) -> Self {
        let location = match (&report.file, report.line) {
            (None, _) => String::new(),
            (Some(file), 0) => file.clone(),
            (Some(file), line) => format!("{file}:{line}"),
        };
        Self {
            title: format!("Unhandled fault: {}", report.kind()),
            location,
            message: report.message(),
            hint: "No ErrorBoundary contains this, so nothing was made again: what the failed \
                   code left half done stays. Save a file to reload."
                .to_owned(),
        }
    }

    /// What there is to show: the engine's last reload error, else the
    /// fault no boundary contained. None when neither, and then nothing is
    /// shown. A reload error comes first because it is the one the
    /// developer's next save answers.
    #[must_use]
    pub fn current(engine: &Engine, more: usize) -> Option<Self> {
        engine
            .last_reload_error()
            .map(|f| Self::of(&f, more))
            .or_else(|| engine.application_fault().map(|r| Self::of_fault(&r)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fault(file: &str, line: u32, column: u32) -> ReloadFault {
        ReloadFault {
            file: file.to_owned(),
            line,
            column,
            message: "Expected token `}`".to_owned(),
        }
    }

    #[test]
    fn it_says_where_and_what() {
        let o = Overlay::of(&fault("ui/Main.qml", 12, 5), 0);
        assert_eq!(o.location, "ui/Main.qml:12:5");
        assert_eq!(o.message, "Expected token `}`");
        assert!(o.hint.contains("runs as it did"));
    }

    #[test]
    fn a_place_is_as_exact_as_it_is_known() {
        assert_eq!(
            Overlay::of(&fault("Main.qml", 0, 0), 0).location,
            "Main.qml"
        );
        assert_eq!(
            Overlay::of(&fault("Main.qml", 3, 0), 0).location,
            "Main.qml:3"
        );
        assert_eq!(Overlay::of(&fault("", 0, 0), 0).location, "");
    }

    #[test]
    fn more_errors_are_counted() {
        assert!(
            Overlay::of(&fault("Main.qml", 1, 1), 1)
                .hint
                .starts_with("And 1 more error.")
        );
        assert!(
            Overlay::of(&fault("Main.qml", 1, 1), 4)
                .hint
                .starts_with("And 4 more errors.")
        );
    }

    #[test]
    fn nothing_is_shown_when_nothing_is_wrong() {
        let engine = Engine::new();
        assert_eq!(Overlay::current(&engine, 0), None);
        engine.set_last_reload_error(Some(fault("Main.qml", 2, 1)));
        assert_eq!(
            Overlay::current(&engine, 0).unwrap().location,
            "Main.qml:2:1"
        );
    }

    #[test]
    fn a_fault_no_boundary_took_is_shown_until_it_is_cleared() {
        let engine = Engine::new();
        engine.report_fault(
            None,
            ljos_runtime::Fault::Panic("index out of range".into()),
        );
        engine.deliver_faults();
        let o = Overlay::current(&engine, 0).unwrap();
        assert_eq!(o.title, "Unhandled fault: Panic");
        assert_eq!(o.message, "index out of range");
        assert_eq!(o.location, "");
        // The reload error is the one the next save answers.
        engine.set_last_reload_error(Some(fault("Main.qml", 2, 1)));
        assert_eq!(Overlay::current(&engine, 0).unwrap().title, "Reload failed");
        engine.set_last_reload_error(None);
        engine.clear_application_fault();
        assert_eq!(Overlay::current(&engine, 0), None);
    }
}
