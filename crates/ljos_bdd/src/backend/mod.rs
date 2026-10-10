//! What a step can ask of a running QML application, independent of who runs it.

use std::fmt;

use serde_json::Value;

pub mod ljos;
pub mod qt;

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
    /// The author's `objectName`, when set.
    pub object_name: String,
    pub children: Vec<A11yNode>,
}

impl A11yNode {
    /// Depth-first search for `role` + `name`.
    #[must_use]
    pub fn find(&self, role: &str, name: &str) -> Option<&A11yNode> {
        if self.role == role && self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(role, name))
    }
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
    fn key(&mut self, chord: &str) -> Result<()>;
    fn touch(&mut self, x: f64, y: f64) -> Result<()>;
    fn a11y_tree(&mut self) -> Result<A11yNode>;
    fn log(&mut self) -> Result<Vec<LogEntry>>;
    /// PNG bytes of the window.
    fn grab(&mut self) -> Result<Vec<u8>>;
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
        assert!(tree.find("button", "Open").is_none());
    }
}
