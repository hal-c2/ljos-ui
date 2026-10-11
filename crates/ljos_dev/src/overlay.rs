//! What the error overlay says. Painting it is the host's: this is the
//! text, so every host says the same.

use ljos_engine::{Engine, ReloadFault};

/// The reload error as a developer reads it: where, what, and what
/// happens next.
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

    /// The overlay for the engine's last reload error; None when the last
    /// reload went well, and then nothing is shown.
    #[must_use]
    pub fn current(engine: &Engine, more: usize) -> Option<Self> {
        engine.last_reload_error().map(|f| Self::of(&f, more))
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
}
