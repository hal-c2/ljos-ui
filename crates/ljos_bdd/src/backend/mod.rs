//! What a step can ask of a running QML application, independent of who runs it.

use std::fmt;

use serde_json::Value;

pub mod ljos;
pub mod qt;
mod rfb;

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The backend exists but the capability does not yet. Reported as pending.
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    /// The oracle cannot do this (e.g. Qt cannot read default accessible names).
    #[error("unsupported on the {backend} backend: {what}")]
    Unsupported {
        backend: &'static str,
        what: &'static str,
    },
    #[error("{0}")]
    Failed(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BackendError>;

/// A QML source file staged by a `Given` step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QmlFile {
    pub name: String,
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Critical,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "error",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
}

/// One node of the accessibility tree, as an assistive technology sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct A11yNode {
    pub role: String,
    pub name: String,
    pub description: String,
    /// The author's `objectName`, when set.
    pub object_name: String,
    /// Whether this is the one node the keyboard is at: what a screen
    /// reader reads out. The top node when nothing below it has it.
    pub focused: bool,
    pub children: Vec<A11yNode>,
}

impl A11yNode {
    /// Depth-first search for `role` + `name`.
    #[must_use]
    pub fn find(&self, role: &str, name: &str) -> Option<&A11yNode> {
        if self.is(role, name) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(role, name))
    }

    #[must_use]
    pub fn is(&self, role: &str, name: &str) -> bool {
        self.role == role && self.name == name
    }

    /// The node the keyboard is at.
    #[must_use]
    pub fn focus(&self) -> Option<&A11yNode> {
        if self.focused {
            return Some(self);
        }
        self.children.iter().find_map(A11yNode::focus)
    }

    /// The node that has the first `role` + `name` as a child.
    #[must_use]
    pub fn parent_of(&self, role: &str, name: &str) -> Option<&A11yNode> {
        if self.children.iter().any(|c| c.is(role, name)) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.parent_of(role, name))
    }
}

/// What a pointer does in one step of a press, drag and release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerStep {
    Press,
    /// With the button held if it was pressed and not yet released.
    Move,
    Release,
}

/// The surface the steps drive. `!Send` is fine: cucumber 0.23 does not need `Send`.
pub trait Backend: fmt::Debug {
    fn name(&self) -> &'static str;
    /// Stage `files` and instantiate `entry` (one of them). May be called repeatedly.
    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()>;
    fn set(&mut self, object: &str, property: &str, value: &Value) -> Result<()>;
    /// Replace the property by a binding to `expression`, as `Qt.binding` does.
    fn bind(&mut self, object: &str, property: &str, expression: &str) -> Result<()>;
    fn get(&mut self, object: &str, property: &str) -> Result<Value>;
    fn call(&mut self, object: &str, method: &str, args: &[Value]) -> Result<Value>;
    fn exists(&mut self, object: &str) -> Result<bool>;
    /// Let `ms` of application time pass (virtual on Ljós, real on Qt).
    fn advance(&mut self, ms: u64) -> Result<()>;
    fn click(&mut self, role: &str, name: &str) -> Result<()>;
    /// Press and release a key chord (`"Ctrl+S"`, `"Tab"`), as a keyboard does.
    fn key(&mut self, chord: &str) -> Result<()>;
    /// Hold a chord down until the keyboard has repeated it `repeats` times,
    /// then let go.
    fn key_hold(&mut self, chord: &str, repeats: u32) -> Result<()>;
    /// The window becomes, or stops being, the one the keyboard types into.
    fn window_active(&mut self, active: bool) -> Result<()>;
    fn touch(&mut self, x: f64, y: f64) -> Result<()>;
    /// One half of a click, or a move between the two: what a held or
    /// dragged pointer is made of.
    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()>;
    fn a11y_tree(&mut self) -> Result<A11yNode>;
    /// What a screen reader's "move the focus here" does to the node with
    /// this role and name. A node that refuses is not an error.
    fn a11y_focus(&mut self, role: &str, name: &str) -> Result<()>;
    fn log(&mut self) -> Result<Vec<LogEntry>>;
    /// PNG bytes of the window.
    fn grab(&mut self) -> Result<Vec<u8>>;
    /// Make the translations of a Qt Linguist `.ts` file available for the
    /// language its `language` attribute names. Nothing is shown in that
    /// language until it is the UI language.
    fn translation(&mut self, name: &str, ts: &str) -> Result<()>;
    /// What assigning `Qt.uiLanguage` does.
    fn set_ui_language(&mut self, language: &str) -> Result<()>;
    /// Put a file that is not QML (an image) beside the QML files, where a
    /// relative URL in them finds it.
    fn resource(&mut self, name: &str, bytes: &[u8]) -> Result<()>;
}

/// Which backends run a scenario (`LJOS_BDD_BACKEND=qt|ljos|both`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendMode {
    Qt,
    Ljos,
    Both,
}

impl BackendMode {
    /// Default is `ljos`: the ledger is about Ljós, Qt is the oracle.
    #[must_use]
    pub fn from_env() -> Self {
        std::env::var("LJOS_BDD_BACKEND").map_or(Self::Ljos, |v| {
            Self::parse(&v)
                .unwrap_or_else(|| panic!("LJOS_BDD_BACKEND must be qt, ljos or both (got {v:?})"))
        })
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "qt" => Some(Self::Qt),
            "ljos" | "" => Some(Self::Ljos),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    /// Backends for a scenario. Only `@qt-parity` scenarios reach Qt.
    #[must_use]
    pub fn backends_for(self, qt_parity: bool) -> Vec<Box<dyn Backend>> {
        let mut out: Vec<Box<dyn Backend>> = Vec::new();
        if matches!(self, Self::Qt | Self::Both) && qt_parity {
            out.push(Box::new(qt::QtBackend::new()));
        }
        if matches!(self, Self::Ljos | Self::Both) {
            out.push(Box::new(ljos::LjosBackend::new()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parsing() {
        assert_eq!(BackendMode::parse("QT"), Some(BackendMode::Qt));
        assert_eq!(BackendMode::parse("both"), Some(BackendMode::Both));
        assert_eq!(BackendMode::parse("nope"), None);
    }

    #[test]
    fn only_parity_scenarios_reach_qt() {
        let names = |m: BackendMode, p| -> Vec<&'static str> {
            m.backends_for(p).iter().map(|b| b.name()).collect()
        };
        assert_eq!(names(BackendMode::Both, true), ["qt", "ljos"]);
        assert_eq!(names(BackendMode::Both, false), ["ljos"]);
        assert!(names(BackendMode::Qt, false).is_empty());
    }

    #[test]
    fn a11y_find_is_depth_first() {
        let tree = A11yNode {
            role: "window".into(),
            children: vec![A11yNode {
                role: "button".into(),
                name: "Save".into(),
                ..A11yNode::default()
            }],
            ..A11yNode::default()
        };
        assert!(tree.find("button", "Save").is_some());
        assert!(tree.focus().is_none());
        assert!(tree.find("button", "Open").is_none());
        assert_eq!(tree.parent_of("button", "Save").unwrap().role, "window");
        assert!(tree.parent_of("window", "").is_none());
    }
}
