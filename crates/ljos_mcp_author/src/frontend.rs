use std::path::PathBuf;

use ljos_mcp_core::SurfaceError;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct Diagnostic {
    /// Project-relative path.
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub severity: Severity,
    pub code: String,
    pub message: String,
    /// For a design or declarative rule: what to write instead, in the rule's words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    /// For a design or declarative rule: its guideline, `docs/design/<page>#<code>` or `docs/declarative.md#<code>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guide: Option<String>,
}

/// A QML type as the type registry describes it. `properties`, `signals`,
/// `methods` and `enums` list only what the type declares itself; what it
/// inherits is in the types of `base_chain`, each of which can be looked up in
/// turn (that keeps an answer small enough to read).
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct TypeInfo {
    pub module: String,
    pub name: String,
    /// What the type inherits from, nearest first.
    pub base_chain: Vec<String>,
    /// Inherited too: where objects written inside one of the type go.
    pub default_property: Option<String>,
    /// `name: type`, with `list`, `readonly` or `required` when they apply.
    pub properties: Vec<String>,
    /// `name(argument, ...)`: a handler is `on` plus the name, capitalised.
    pub signals: Vec<String>,
    pub methods: Vec<String>,
    /// `Name: Key, Key`; flags are marked `(flags)`.
    pub enums: Vec<String>,
    /// The type `Name.` on another object stands for, if the type attaches
    /// one: `Layout.fillWidth` is a property of `Layout`'s.
    pub attached: Option<String>,
}

/// The types a module exports, by the name a QML file writes them with.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct ModuleInfo {
    pub module: String,
    /// Sorted. Each can be looked up with `type_lookup`.
    pub types: Vec<String>,
}

/// What the authoring server needs from the QML front end. The real one is
/// [`QmlFrontEnd`](crate::QmlFrontEnd), over `qml_syntax`, `qml_front` and
/// `qt_types`. The trait stays as the seam, so the tools and the protocol
/// tests can run against [`StubFrontEnd`] or a fake without a parser or a Qt
/// type table.
pub trait FrontEnd: Send + Sync + 'static {
    /// False for the placeholder, so `project_info` can tell agents up front.
    fn wired(&self) -> bool {
        true
    }
    /// Parse and resolve already-confined, canonical files.
    fn check(&self, files: &[PathBuf]) -> Result<Vec<Diagnostic>, SurfaceError>;
    /// Lints, including the accessibility audit.
    fn lint(&self, files: &[PathBuf]) -> Result<Vec<Diagnostic>, SurfaceError>;
    fn type_lookup(&self, module: &str, name: &str) -> Result<TypeInfo, SurfaceError>;
    /// The types of one module. A front end that cannot list them says so
    /// instead of returning an empty list an agent would believe.
    fn module_types(&self, _module: &str) -> Result<ModuleInfo, SurfaceError> {
        Err(SurfaceError::unavailable(
            "this front end does not list the types of a module",
        ))
    }
}

/// A front end that is not there. Failing loudly beats returning an empty
/// (and therefore "clean") diagnostic list that an agent would trust.
#[derive(Clone, Copy, Debug, Default)]
pub struct StubFrontEnd;

const NOT_WIRED: &str =
    "front end not wired yet: the QML parser and type registry are not linked into this build";

impl FrontEnd for StubFrontEnd {
    fn wired(&self) -> bool {
        false
    }
    fn check(&self, _: &[PathBuf]) -> Result<Vec<Diagnostic>, SurfaceError> {
        Err(SurfaceError::unavailable(NOT_WIRED))
    }
    fn lint(&self, _: &[PathBuf]) -> Result<Vec<Diagnostic>, SurfaceError> {
        Err(SurfaceError::unavailable(NOT_WIRED))
    }
    fn type_lookup(&self, _: &str, _: &str) -> Result<TypeInfo, SurfaceError> {
        Err(SurfaceError::unavailable(NOT_WIRED))
    }
    fn module_types(&self, _: &str) -> Result<ModuleInfo, SurfaceError> {
        Err(SurfaceError::unavailable(NOT_WIRED))
    }
}
