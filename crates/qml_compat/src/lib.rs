//! Qt's QML lowered to Ljós's own API (docs/DIALECT.md,
//! docs/adr/0004-compat-lowering-before-the-ir.md).
//!
//! [`lower`] reads a file written against `QtQuick` and gives the edits that
//! say the same against `Ljos.Ui`: the import, the types Ljós renames, the
//! members it renames on the types that have them, and `var` declarations as
//! `unknown`. A project makes them as it loads the file, so the compiler, the
//! IR and the runtime see only Ljós's names; `ljos migrate` writes the same
//! edits back to the file.
//!
//! A file is lowered whole or not at all. Where the pass cannot tell what a
//! name is (`parent.color`: the parent may be a `Rectangle`, whose `color`
//! is `fill`), renaming it could be wrong and leaving it could be wrong, so
//! the file is [kept](Outcome::Kept) as written and says why. An edit never
//! adds or removes a line: what the compiler says of a line of the lowered
//! text is said of the same line of the file.
//!
//! Files that name one another are lowered together or not at all
//! ([`group`]): what one sets on another is then called the same in both.

mod arrange;
mod given;
mod group;
mod renames;
mod rows;
mod tables;

use std::{cell::RefCell, collections::HashMap, ops::Range};

use oxc_ast::ast::{
    BindingIdentifier, CallExpression, ComputedMemberExpression, Expression, IdentifierReference,
    StaticMemberExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;
use qml_edit::Change;
use qml_syntax::{
    Allocator,
    ast::{
        QmlBinding, QmlBindingValue, QmlDocument, QmlImportSource, QmlMember, QmlObject,
        QmlQualifiedName, QmlTypeName,
    },
};
use qt_types::Type;
use rows::{Delegate, Owns, Rows};

pub use group::{Alone, References, group, references};
pub use renames::{member_renames, type_renames};

/// The module a lowered file imports.
pub const MODULE: &str = "Ljos.Ui";

/// The module a lowered file imports its controls from.
pub const CONTROLS: &str = "Ljos.Controls";

/// The modules of Qt's controls lowered to [`CONTROLS`]: the Basic style,
/// and `QtQuick.Controls` where the program starts with Basic
/// ([`Locals::controls_style`]). A style's own module,
/// `QtQuick.Controls.Material`, keeps its file: its attached API is the
/// style's.
pub const QT_CONTROLS: &[&str] = &["QtQuick.Controls", "QtQuick.Controls.Basic"];

/// The style [`CONTROLS`] draws.
pub const CONTROLS_STYLE: &str = "Basic";

/// The module of Ljós's models, which a lowered file imports when it makes
/// one of [`MODELS`].
pub const MODELS_MODULE: &str = "Ljos.Models";

/// The types of `QtQml.Models` that are `Ljos.Models`' as they are.
pub const MODELS: &[&str] = &["ListModel", "ListElement", "Instantiator"];

/// The module a lowered file imports too when it names what is not an
/// item: a `Timer`, a `Connections`, a `Component`.
pub const CORE: &str = "Ljos.Core";

/// Qt's dialogs, whose file and folder dialogs [`CONTROLS`] has as they are.
pub const QT_DIALOGS: &str = "QtQuick.Dialogs";

/// What a lowered file says of its scripts: they are Qt's JavaScript, which
/// the checker of Ljós's API does not hold to its rules. A file is typed by
/// taking the pragma out, and `ljos check` then says what to change
/// (docs/DIALECT.md §8).
pub const JAVASCRIPT: &str = "pragma Scripts: JavaScript";

/// What [`lower`] made of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It does not import `QtQuick`: there is nothing to lower.
    Unchanged,
    /// The file in Ljós's API.
    Lowered(Lowered),
    /// Not lowered, for the reason given.
    Kept(Kept),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lowered {
    pub text: String,
    /// In order, not overlapping, none across a line break.
    pub changes: Vec<Change>,
    /// The modules the lowered text imports that the pass writes, each
    /// with its name beside the file (`Main.rows.ts`) and its text: the row
    /// types of its constant `ListModel`s (docs/DIALECT.md §4.8).
    pub modules: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    /// What of the file the pass cannot lower: "`Image` is not in Ljos.Ui
    /// yet".
    pub reason: String,
    /// Where, as a byte offset into the file.
    pub at: usize,
}

/// The root object of a file of the project, or of an inline component,
/// as it was written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Root {
    /// The type it is written as.
    pub type_name: String,
    /// The names it binds, with those of its groups (`anchors.left` for
    /// `anchors { left: … }`): what it says of how it is placed.
    pub bindings: Vec<String>,
    /// The properties it declares: a `model` it declares is its own, not a
    /// delegate's row.
    pub properties: Vec<String>,
}

/// What the pass needs to know of the project the file is in: for a type a
/// file of the project is (`SimpleButton` for `SimpleButton.qml`), its root
/// object, and what the modules and directories the file imports are.
pub trait Locals {
    /// The root object of the file `name` beside this one.
    fn root(&self, name: &str) -> Option<Root>;

    /// The style `import QtQuick.Controls` is where the file runs, as the
    /// program chooses it when it starts; none when that is not known, and
    /// a file that imports it is kept.
    fn controls_style(&self) -> Option<&str> {
        None
    }

    /// What the module `uri` is to the project, as its loader finds it on
    /// the import path: none for one the project does not have.
    fn module(&self, _uri: &str) -> Option<ProjectModule> {
        None
    }

    /// The root object of the type `name` that `from` brings: a file of a
    /// module of the project, or of a directory imported by path.
    fn imported(&self, _from: Imported<'_>, _name: &str) -> Option<Root> {
        None
    }
}

/// The project's files, in a program that starts with the controls style
/// `style`.
pub struct Styled<'a> {
    pub locals: &'a dyn Locals,
    pub style: &'a str,
}

impl std::fmt::Debug for Styled<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Styled")
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

impl Locals for Styled<'_> {
    fn root(&self, name: &str) -> Option<Root> {
        self.locals.root(name)
    }

    fn module(&self, uri: &str) -> Option<ProjectModule> {
        self.locals.module(uri)
    }

    fn imported(&self, from: Imported<'_>, name: &str) -> Option<Root> {
        self.locals.imported(from, name)
    }

    fn controls_style(&self) -> Option<&str> {
        Some(self.style)
    }
}

impl<F: Fn(&str) -> Option<Root>> Locals for F {
    fn root(&self, name: &str) -> Option<Root> {
        self(name)
    }
}

/// A module the project has (a `qmldir` or a build describes it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectModule {
    /// Its types are QML files of the project: they are lowered with the
    /// files that import them.
    Files,
    /// None of its types is a QML file: they are native, and not Qt's or
    /// Ljós's, so what a file sets on them is not known.
    Native,
}

/// What an import that is not Qt's brings a file's types from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Imported<'a> {
    /// A module of the project: `import My.Controls`.
    Module(&'a str),
    /// A directory, as written: `import "parts"`.
    Dir(&'a str),
}

/// An import of the file, in the order written, that decides what a type
/// name is.
#[derive(Clone, Debug)]
enum Import {
    /// `QtQuick` (with `QtQml`'s types), or `QtQuick.Layouts`.
    Qt {
        layouts: bool,
    },
    /// `QtQml`, whose types a later import of the project shadows as it
    /// shadows `QtQuick`'s.
    Qml,
    /// `QtQuick.Controls` or one of its styles' modules.
    Controls,
    /// `QtQuick.Shapes`.
    Shapes,
    Module(String),
    Dir(String),
}

impl Import {
    fn imported(&self) -> Option<Imported<'_>> {
        match self {
            Import::Qt { .. } | Import::Qml | Import::Controls | Import::Shapes => None,
            Import::Module(uri) => Some(Imported::Module(uri)),
            Import::Dir(path) => Some(Imported::Dir(path)),
        }
    }
}

/// No files beside this one.
pub fn no_locals(_: &str) -> Option<Root> {
    None
}

/// `source` in Ljós's API, or why it stays as it is. Its constant
/// `ListModel`s stay as they are: what writes them as lists writes a module
/// beside the file, and names it for the file ([`lower_as`]).
pub fn lower(source: &str, locals: &dyn Locals) -> Outcome {
    lower_file(source, None, locals)
}

/// [`lower`] for the file `stem.qml`, whose constant `ListModel`s are
/// written as typed lists with their row types in the module
/// `stem.rows.ts` beside it ([`Lowered::modules`]).
pub fn lower_as(source: &str, stem: &str, locals: &dyn Locals) -> Outcome {
    lower_file(source, Some(stem), locals)
}

fn lower_file(source: &str, stem: Option<&str>, locals: &dyn Locals) -> Outcome {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        // The compiler says what is wrong with the file as written.
        return kept("it does not parse", 0);
    }
    let Some(ui) = qt_types::module(MODULE) else {
        return kept("this build has no description of Ljos.Ui", 0);
    };
    let Some(core) = qt_types::module(CORE) else {
        return kept("this build has no description of Ljos.Core", 0);
    };
    let Some(quick) = qt_types::module("QtQuick") else {
        return kept("this build has no description of QtQuick", 0);
    };
    let mut pass = Pass {
        source,
        ui,
        core,
        cores: std::cell::Cell::new(false),
        uis: std::cell::Cell::new(false),
        quick,
        layouts: None,
        controls: None,
        shapes: None,
        dialogs: None,
        locals,
        imports: Vec::new(),
        imported: RefCell::default(),
        ids: Vec::new(),
        inline: Vec::new(),
        changes: Vec::new(),
        listed: Vec::new(),
        imports_models: false,
        makes_models: std::cell::Cell::new(false),
        given: None,
        view_rows: None,
        delegate: None,
        by_id: Vec::new(),
        loose: false,
        scope_owns: Owns::NONE,
        root_owns: Owns::NONE,
        stem,
        tables: Vec::new(),
    };
    match pass.document(&parsed.document) {
        Ok(false) => Outcome::Unchanged,
        Ok(true) => {
            let mut changes = pass.changes;
            changes.sort_by_key(|c| c.range.start);
            changes.dedup();
            let text = qml_edit::splice(source, &changes);
            let again = qml_syntax::parse(&allocator, allocator.alloc_str(&text));
            if !again.is_ok() {
                return kept("the lowered text does not parse", 0);
            }
            let modules = match (stem, pass.tables.is_empty()) {
                (Some(stem), false) => vec![(
                    rows_module(stem),
                    tables::module(&format!("{stem}.qml"), &pass.tables),
                )],
                _ => Vec::new(),
            };
            Outcome::Lowered(Lowered {
                text,
                changes,
                modules,
            })
        }
        Err(kept) => Outcome::Kept(kept),
    }
}

/// The module of the row types of the file `stem.qml`.
pub fn rows_module(stem: &str) -> String {
    format!("{stem}.rows.ts")
}

/// The root object of `source`: what a file of the project is, for
/// [`Locals`].
pub fn root(source: &str) -> Option<Root> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    let root = Root::of(&parsed.document.root);
    (!parsed.panicked && !root.type_name.is_empty()).then_some(root)
}

impl Root {
    fn of(object: &QmlObject<'_>) -> Root {
        Root {
            type_name: object.type_name.to_string(),
            bindings: arrange::binding_names(object),
            properties: object
                .members
                .iter()
                .filter_map(|member| match member {
                    QmlMember::Property(property) => Some(property.name.name.to_string()),
                    _ => None,
                })
                .collect(),
        }
    }
}

fn kept(reason: impl Into<String>, at: usize) -> Outcome {
    Outcome::Kept(Kept {
        reason: reason.into(),
        at,
    })
}

/// What the pass knows of what a name or an expression is.
#[derive(Clone, Copy, Debug)]
enum Ty {
    /// This object, of this type: an id, or the object a binding is on.
    Exact(&'static Type),
    /// Of this type or one derived from it.
    Derived(&'static Type),
    /// A value with no members the pass renames: a number, a string, a
    /// `color`, a `font`.
    Plain,
    /// The pass does not know.
    Unknown,
}

struct Pass<'s> {
    source: &'s str,
    ui: &'static qt_types::Module,
    core: &'static qt_types::Module,
    /// Whether the file names a type of `Ljos.Core`, which it imports
    /// then.
    cores: std::cell::Cell<bool>,
    /// Whether the file names a type of `Ljos.Ui`: a file that imports
    /// `QtQml` and not `QtQuick` imports it then too.
    uis: std::cell::Cell<bool>,
    quick: &'static qt_types::Module,
    /// QtQuick.Layouts, when the file imports it.
    layouts: Option<&'static qt_types::Module>,
    /// Qt's controls and [`CONTROLS`], when the file imports Qt's.
    controls: Option<Controls>,
    /// QtQuick.Shapes, when the file imports it.
    shapes: Option<&'static qt_types::Module>,
    /// Qt's dialogs and [`CONTROLS`], when the file imports them.
    dialogs: Option<Controls>,
    locals: &'s dyn Locals,
    /// The imports that name types, in the order written: the last that
    /// has a name is the one it is.
    imports: Vec<Import>,
    /// What [`imported_root`](Self::imported_root) found, by name.
    imported: RefCell<HashMap<String, Option<(usize, Root)>>>,
    ids: Vec<(String, Ty)>,
    inline: Vec<(String, Root)>,
    changes: Vec<Change>,
    /// The `ListModel`s with ids, and the rows they give.
    listed: Vec<(String, Rows)>,
    /// Whether the file imports `QtQml.Models`, which is lowered to
    /// `Ljos.Models`.
    imports_models: bool,
    /// Whether it makes one of [`MODELS`].
    makes_models: std::cell::Cell<bool>,
    /// The rows of the view whose delegate the next object is.
    given: Option<Rows>,
    /// The rows of the view the pass is in, which a `DelegateChoice` or a
    /// `Component` hands its delegate.
    view_rows: Option<Rows>,
    /// The delegate the pass is in.
    delegate: Option<Delegate>,
    /// The `Component`s views are given by id alone, and those views' rows.
    by_id: Vec<(String, Rows)>,
    /// Whether the pass is in a `Component` that is no view's delegate the
    /// file shows.
    loose: bool,
    /// What the object whose members an unqualified name is first has.
    scope_owns: Owns,
    /// What the root of the file, or of the inline component, has.
    root_owns: Owns,
    /// The file's name without `.qml`, when the pass may write a module
    /// beside it.
    stem: Option<&'s str>,
    /// Its constant `ListModel`s, written as lists.
    tables: Vec<tables::Table>,
}

type Step = Result<(), Kept>;

/// Qt's description of the controls a file imports, and Ljós's of those
/// it has taken.
#[derive(Clone, Copy)]
struct Controls {
    qt: &'static qt_types::Module,
    ljos: &'static qt_types::Module,
}

impl<'s> Pass<'s> {
    /// Whether the file imports `QtQuick` and so has something to lower.
    fn document(&mut self, document: &QmlDocument<'_>) -> Result<bool, Kept> {
        let mut quick = None;
        let mut qml = None;
        let mut layouts = None;
        let mut controls = Vec::new();
        let mut shapes = None;
        for import in &document.imports {
            let at = import.span.start as usize;
            let name = match &import.source {
                QmlImportSource::Module(name) => name,
                // A directory's types are files of the project; one
                // imported under a name is only ever named through it.
                QmlImportSource::Path(path) => {
                    if import.alias.is_none() && !group::is_script(path) {
                        self.imports.push(Import::Dir((*path).to_string()));
                    }
                    continue;
                }
            };
            let uri = name.to_string();
            if import.alias.is_some()
                && (matches!(
                    uri.as_str(),
                    "QtQuick" | "QtQml" | "QtQml.Models" | "QtQuick.Shapes"
                ) || uri == QT_DIALOGS
                    || QT_CONTROLS.contains(&uri.as_str()))
            {
                return Err(self.no(format!("it imports `{uri}` under a name"), at));
            }
            match uri.as_str() {
                "QtQuick" => {
                    quick = Some(import.span);
                    self.imports.push(Import::Qt { layouts: false });
                }
                "QtQml" => {
                    qml = Some(import.span);
                    self.imports.push(Import::Qml);
                }
                // Its models are `Ljos.Models`'.
                "QtQml.Models" => {
                    self.imports_models = true;
                    let span = import.span;
                    self.change(
                        span.start as usize..span.end as usize,
                        format!("import {MODELS_MODULE}"),
                    );
                }
                // Its line layouts are lowered to Ljós's arrangement
                // ([`arrange`]); its other types keep the file.
                "QtQuick.Layouts" if import.alias.is_none() => {
                    self.layouts = qt_types::module("QtQuick.Layouts");
                    layouts = Some(import.span);
                    self.imports.push(Import::Qt { layouts: true });
                }
                // `Ljos.Ui` has its types as they are.
                "QtQuick.Shapes" => {
                    self.shapes = qt_types::module("QtQuick.Shapes");
                    shapes = Some(import.span);
                    self.imports.push(Import::Shapes);
                }
                // Ljós's controls are Basic's; `QtQuick.Controls` is the
                // style the program starts with, which may be another.
                "QtQuick.Controls" if self.locals.controls_style() != Some(CONTROLS_STYLE) => {
                    let reason = match self.locals.controls_style() {
                        Some(style) => format!(
                            "it imports `QtQuick.Controls`, which is the {style} style where it runs, and {CONTROLS} is {CONTROLS_STYLE}"
                        ),
                        None => "it imports `QtQuick.Controls`, whose style is chosen as the program starts".to_owned(),
                    };
                    return Err(self.no(reason, at));
                }
                uri if uri == QT_DIALOGS || QT_CONTROLS.contains(&uri) => {
                    let (Some(qt), Some(ljos)) =
                        (qt_types::module(uri), qt_types::module(CONTROLS))
                    else {
                        return Err(self.no(
                            format!("this build has no description of `{uri}` or {CONTROLS}"),
                            at,
                        ));
                    };
                    let found = if uri == QT_DIALOGS {
                        &mut self.dialogs
                    } else {
                        &mut self.controls
                    };
                    found.get_or_insert(Controls { qt, ljos });
                    // The dialogs are in Ljos.Controls, so their import is
                    // one of the controls'.
                    controls.push(import.span);
                    self.imports.push(Import::Controls);
                }
                other => match self.locals.module(other) {
                    // Files of the project, lowered with this one
                    // ([`group`]); the line stays as it is.
                    Some(ProjectModule::Files) if import.alias.is_none() => {
                        self.imports.push(Import::Module(uri));
                    }
                    Some(ProjectModule::Files) => {
                        return Err(self.no(format!("it imports `{other}` under a name"), at));
                    }
                    Some(ProjectModule::Native) => {
                        return Err(self.no(
                            format!(
                                "it imports `{other}`, a module of the project whose types \
                                 are native: they are not lowered"
                            ),
                            at,
                        ));
                    }
                    // A module that builds on QtQuick names its types, and
                    // those are Qt's until Ljós has its own
                    // (docs/DIALECT.md).
                    None => {
                        return Err(self.no(
                            format!("it imports `{other}`, which is not lowered yet"),
                            at,
                        ));
                    }
                },
            }
        }
        if quick.is_none() && qml.is_none() {
            return Ok(false);
        }
        for span in [layouts, shapes].into_iter().flatten() {
            // `Ljos.Ui` has `Layout` and the shapes; the line stays, empty.
            self.change(span.start as usize..span.end as usize, "");
        }
        for (i, span) in controls.into_iter().enumerate() {
            // A second import of Qt's controls is the first again.
            let import = if i == 0 {
                format!("import {CONTROLS}")
            } else {
                String::new()
            };
            self.change(span.start as usize..span.end as usize, import);
        }
        if self.stem.is_some() {
            let quick = self.quick;
            self.tables = tables::tables(self.source, &document.root, &|name| {
                quick.type_named(name).is_some()
            });
        }
        self.collect(&document.root);
        let by_id = given::by_id(
            &document.root,
            &|o| self.is_view(o),
            &self.listed,
            &self.table_starts(),
        )
        .into_iter()
        .filter(|(id, _)| matches!(self.id(id), Some(Ty::Exact(t)) if t.name == "Component"))
        .collect();
        self.by_id = by_id;
        let root = self.object_type(&document.root)?;
        self.root_owns = self.owns(&document.root, root);
        self.object(&document.root, root, root, true)?;
        // On the import's line, so every line stays where it was. What
        // imported `QtQml` imports `Ljos.Core` in its place; what has it
        // from `QtQuick` imports it beside `Ljos.Ui` when it names one of
        // its types. A file that makes a model imports `Ljos.Models` on the
        // same line.
        let models = if self.makes_models.get() && !self.imports_models {
            format!("; import {MODELS_MODULE}")
        } else {
            String::new()
        };
        // A file whose constant tables are typed lists imports their rows
        // module on the line that imports Ljós's models.
        let rows = match (self.stem, self.tables.is_empty()) {
            (Some(stem), false) => {
                format!("; import \"{}\" as {}", rows_module(stem), tables::ALIAS)
            }
            _ => String::new(),
        };
        let span = |s: qml_syntax::Span| s.start as usize..s.end as usize;
        match (quick, qml) {
            (Some(quick), Some(qml)) => {
                self.change(
                    span(quick),
                    format!("{JAVASCRIPT}; import {MODULE}{models}{rows}"),
                );
                self.change(span(qml), format!("import {CORE}"));
            }
            (Some(quick), None) => {
                let mut import = format!("{JAVASCRIPT}; import {MODULE}");
                if self.cores.get() {
                    import.push_str(&format!("; import {CORE}"));
                }
                import.push_str(&models);
                import.push_str(&rows);
                self.change(span(quick), import);
            }
            (None, Some(qml)) => {
                let ui = if self.uis.get() {
                    format!("; import {MODULE}")
                } else {
                    String::new()
                };
                self.change(
                    span(qml),
                    format!("{JAVASCRIPT}{ui}; import {CORE}{models}{rows}"),
                );
            }
            (None, None) => unreachable!("returned above"),
        }
        let edits: Vec<Change> = self
            .tables
            .iter()
            .flat_map(|t| t.changes.iter().cloned())
            .collect();
        self.changes.extend(edits);
        Ok(true)
    }

    /// Whether `object` is of a type that shows a model with a delegate.
    fn is_view(&self, object: &QmlObject<'_>) -> bool {
        let Some(name) = object.type_name.as_simple() else {
            return false;
        };
        matches!(
            self.named_type(name, 0, 0),
            Ok(Ty::Exact(t) | Ty::Derived(t))
                if t.property("model").is_some() && t.property("delegate").is_some()
        )
    }

    fn no(&self, reason: String, at: usize) -> Kept {
        Kept { reason, at }
    }

    fn change(&mut self, range: Range<usize>, replacement: impl Into<String>) {
        self.changes.push(Change {
            range,
            replacement: replacement.into(),
        });
    }

    /// The ids and inline components, before anything refers to them.
    fn collect(&mut self, object: &QmlObject<'_>) {
        for member in &object.members {
            match member {
                QmlMember::Binding(binding) => {
                    if binding.name.as_simple() == Some("id")
                        && let QmlBindingValue::Expression(Expression::Identifier(id)) =
                            &binding.value
                    {
                        let ty = self.object_type(object).unwrap_or(Ty::Unknown);
                        self.ids.push((id.name.to_string(), ty));
                    }
                    self.collect_value(&binding.value);
                }
                QmlMember::Property(property) => {
                    if let Some(value) = &property.value {
                        self.collect_value(value);
                    }
                }
                QmlMember::Object(child) if self.is_table(child) => {
                    // A list of objects, which is not looked into.
                    if let Some(id) = object_id(child) {
                        self.ids.push((id.clone(), Ty::Plain));
                        self.listed.push((id, Rows::Roles));
                    }
                }
                QmlMember::Object(child) => {
                    if child.type_name.as_simple() == Some("ListModel")
                        && let Some(id) = object_id(child)
                    {
                        let rows = rows::of_roles(rows::listed_roles(child), true);
                        self.listed.push((id, rows));
                    }
                    self.collect(child);
                }
                QmlMember::InlineComponent(component) => {
                    self.inline
                        .push((component.name.name.to_string(), Root::of(&component.object)));
                    self.collect(&component.object);
                }
                QmlMember::Signal(_) | QmlMember::Function(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn collect_value(&mut self, value: &QmlBindingValue<'_>) {
        match value {
            QmlBindingValue::Object(object) if self.is_table(object) => {}
            QmlBindingValue::Object(object) => self.collect(object),
            QmlBindingValue::Objects(objects) => objects.iter().for_each(|o| self.collect(o)),
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
        }
    }

    /// Whether `object` is a constant `ListModel` the pass writes as a list.
    fn is_table(&self, object: &QmlObject<'_>) -> bool {
        self.tables.iter().any(|t| t.at == object.span.start)
    }

    /// Where the tables written as lists start.
    fn table_starts(&self) -> Vec<u32> {
        self.tables.iter().map(|t| t.at).collect()
    }

    fn id(&self, name: &str) -> Option<Ty> {
        self.ids
            .iter()
            .find(|(id, _)| id == name)
            .map(|(_, ty)| *ty)
    }

    /// The type an object is made as, renaming it in the text when Ljós
    /// calls it otherwise. A grouped property (`font { }`) is no type.
    fn object_type(&mut self, object: &QmlObject<'_>) -> Result<Ty, Kept> {
        let Some(name) = object.type_name.as_simple() else {
            return Err(self.no(
                format!(
                    "`{}` is a type of a module named in the file",
                    object.type_name
                ),
                object.type_name.span.start as usize,
            ));
        };
        if name.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Ok(Ty::Plain);
        }
        let at = object.type_name.span;
        if let Some(new) = renamed_type(name) {
            self.change(at.start as usize..at.end as usize, new);
        }
        self.named_type(name, at.start as usize, 0)
    }

    /// The type `name` is in this file: Qt's (and then one Ljos.Ui has),
    /// an inline component, or a file of the project.
    fn named_type(&self, name: &str, at: usize, depth: usize) -> Result<Ty, Kept> {
        // Of an import after Qt's that has the name too, the type is that
        // import's, as the loader takes the last.
        let imported = self.imported_root(name);
        let after =
            |qt: Option<usize>| matches!((&imported, qt), (Some((at, _)), Some(qt)) if *at > qt);
        let shadowed = |layouts: bool| {
            after(
                self.imports
                    .iter()
                    .rposition(|i| matches!(i, Import::Qt { layouts: l } if *l == layouts)),
            )
        };
        let shadowed_layout = shadowed(true);
        let shadowed_quick = shadowed(false);
        // `QtQuick` brings `QtQml`'s types too: they are shadowed by what
        // is imported after the last of the two.
        let shadowed_qml = after(
            self.imports
                .iter()
                .rposition(|i| matches!(i, Import::Qt { layouts: false } | Import::Qml)),
        );
        let controls_at = self
            .imports
            .iter()
            .rposition(|i| matches!(i, Import::Controls));
        let shadowed_controls =
            matches!((&imported, controls_at), (Some((at, _)), Some(c)) if *at > c);
        let shapes_at = self
            .imports
            .iter()
            .rposition(|i| matches!(i, Import::Shapes));
        let shadowed_shapes = matches!((&imported, shapes_at), (Some((at, _)), Some(s)) if *at > s);
        if let Some(ty) = self
            .layouts
            .filter(|_| !shadowed_layout)
            .and_then(|m| m.type_named(name))
        {
            return match name {
                "RowLayout" | "ColumnLayout" | "Layout" => {
                    self.uis.set(true);
                    Ok(Ty::Exact(ty))
                }
                _ => Err(self.no(format!("`{name}` is not in {MODULE} yet"), at)),
            };
        }
        if let Some(controls) = [self.controls, self.dialogs]
            .into_iter()
            .flatten()
            .find(|c| c.qt.type_named(name).is_some())
            && let Some(ty) = controls.qt.type_named(name)
            && !shadowed_controls
        {
            return match controls.ljos.type_named(name) {
                Some(_) => Ok(Ty::Exact(ty)),
                None => Err(self.no(format!("`{name}` is not in {CONTROLS} yet"), at)),
            };
        }
        if let Some(ty) = self
            .shapes
            .filter(|_| !shadowed_shapes)
            .and_then(|m| m.type_named(name))
        {
            return match self.ui.type_named(name) {
                Some(_) => {
                    self.uis.set(true);
                    Ok(Ty::Exact(ty))
                }
                None => Err(self.no(format!("`{name}` is not in {MODULE} yet"), at)),
            };
        }
        if let Some(ty) = self.quick.type_named(name).filter(|ty| {
            !if ty.module.starts_with("QtQml") {
                shadowed_qml
            } else {
                shadowed_quick
            }
        }) {
            if ty.module.starts_with("QtQml") {
                if MODELS.contains(&name) {
                    self.makes_models.set(true);
                    return Ok(Ty::Exact(ty));
                }
                return self.core_type(name, ty, at);
            }
            let new = renamed_type(name).unwrap_or(name);
            return match self.ui.type_named(new) {
                Some(_) => {
                    self.uis.set(true);
                    Ok(Ty::Exact(ty))
                }
                None => Err(self.no(format!("`{name}` is not in {MODULE} yet"), at)),
            };
        }
        if depth > 16 {
            return Ok(Ty::Unknown);
        }
        let inline = self.inline.iter().any(|(n, _)| n == name);
        // Past the first step the name is another file's, found among its
        // own imports, which this file's do not say. An import of the
        // project after the line the Ljós module takes is found before it.
        if depth == 0
            && !inline
            && let Some((module, line)) = self.ljos_module_with(name)
            && !matches!((&imported, line), (Some((at, _)), Some(line)) if *at > line)
        {
            let what = if imported.is_some() || self.locals.root(name).is_some() {
                "is a file of the project"
            } else {
                "is not a type of the modules the file imports"
            };
            return Err(self.no(
                format!(
                    "`{name}` {what}, and {module}, which the lowered file imports, has a `{name}` that would be found first"
                ),
                at,
            ));
        }
        let root = self.local_root(name, imported).map(|r| r.type_name);
        match root {
            Some(root) if root != name => match self.named_type(&root, at, depth + 1)? {
                // What it declares is not in the table: its members are
                // known by the type it is built on, as far as that goes.
                Ty::Exact(ty) => Ok(Ty::Derived(ty)),
                other => Ok(other),
            },
            _ => Ok(Ty::Unknown),
        }
    }

    /// The root of the type `name` a file of the project is: an inline
    /// component, a type an import of the project brings, or a file beside
    /// this one, in the order the loader looks (`imported` is what
    /// [`imported_root`](Self::imported_root) found).
    fn local_root(&self, name: &str, imported: Option<(usize, Root)>) -> Option<Root> {
        self.inline
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, root)| root.clone())
            .or(imported.map(|(_, root)| root))
            .or_else(|| self.locals.root(name))
    }

    /// The last import of the project that has the type `name`, and what
    /// its root is.
    fn imported_root(&self, name: &str) -> Option<(usize, Root)> {
        if let Some(known) = self.imported.borrow().get(name) {
            return known.clone();
        }
        // Each is a file the project reads; a file names a type many times.
        let found = self
            .imports
            .iter()
            .enumerate()
            .rev()
            .find_map(|(at, import)| {
                let from = import.imported()?;
                self.locals.imported(from, name).map(|root| (at, root))
            });
        self.imported
            .borrow_mut()
            .insert(name.to_string(), found.clone());
        found
    }

    /// `name`, a type of `QtQml`'s, as `Ljos.Core` has it; the models are
    /// [`MODELS_MODULE`]'s, and found before this. `Qt` is a global of
    /// every file, as it is in Qt's QML.
    fn core_type(&self, name: &str, ty: &'static Type, at: usize) -> Result<Ty, Kept> {
        if name == "Qt" {
            return Ok(Ty::Exact(ty));
        }
        if self.core.type_named(name).is_some() {
            self.cores.set(true);
            return Ok(Ty::Exact(ty));
        }
        Err(self.no(format!("`{name}` is not in {CORE} yet"), at))
    }

    /// The Ljós module the lowered file imports that has `name`, for a name
    /// no Qt module the file imports has, and the place among the file's
    /// imports of the line it takes. A Qt file finds such a name among the
    /// files beside it, but an import is found before those, so the lowered
    /// file would make Ljós's type instead.
    fn ljos_module_with(&self, name: &str) -> Option<(&'static str, Option<usize>)> {
        if self.ui.type_named(name).is_some() {
            let line = self
                .imports
                .iter()
                .rposition(|i| matches!(i, Import::Qt { layouts: false }));
            return Some((MODULE, line));
        }
        let controls = self.controls.or(self.dialogs)?;
        let line = self
            .imports
            .iter()
            .rposition(|i| matches!(i, Import::Controls));
        controls.ljos.type_named(name).map(|_| (CONTROLS, line))
    }

    /// Lowers `object`, of type `ty`, whose unqualified names are looked up
    /// on `scope` (itself, or for a group the object it is a group of) and
    /// then on the file's root, `root`. `a_root` when what uses or holds it
    /// may size it, which the file does not show: the root of the file, of
    /// an inline component or of a `Component`, or an object given as a
    /// value (a `Loader` sizes its `sourceComponent`'s).
    fn object(&mut self, object: &QmlObject<'_>, ty: Ty, root: Ty, a_root: bool) -> Step {
        // A `Component` a view is given by id is its delegate as if it
        // were written in the view's `delegate:`.
        let given = self.given.take().or_else(|| match ty {
            Ty::Exact(t) if t.name == "Component" => {
                let id = object_id(object)?;
                let (_, rows) = self.by_id.iter().find(|(given, _)| *given == id)?;
                Some(rows.clone())
            }
            _ => None,
        });
        let saved = (
            self.delegate.clone(),
            self.scope_owns,
            self.view_rows.clone(),
            self.root_owns,
            self.loose,
        );
        let done = self.object_in(object, ty, root, a_root, given);
        (
            self.delegate,
            self.scope_owns,
            self.view_rows,
            self.root_owns,
            self.loose,
        ) = saved;
        done
    }

    /// [`Self::object`], given the rows of the view it is a delegate of.
    fn object_in(
        &mut self,
        object: &QmlObject<'_>,
        ty: Ty,
        root: Ty,
        a_root: bool,
        given: Option<Rows>,
    ) -> Step {
        let named = |names: &[&str]| matches!(ty, Ty::Exact(t) if names.contains(&t.name));
        // What hands a delegate on rather than being it.
        let carrier = given.is_some() && named(&["Component", "DelegateChooser", "DelegateChoice"]);
        if given.is_none() && named(&["Component"]) {
            self.loose = true;
        }
        let owns = self.owns(object, ty);
        match given {
            Some(rows) if carrier => self.view_rows = Some(rows),
            Some(rows) => self.delegate = Some(Delegate { rows, owns }),
            // A component made in the delegate: its root hides the row too.
            None if a_root => {
                if let Some(d) = &mut self.delegate {
                    d.owns = d.owns.or(owns);
                }
            }
            None => {}
        }
        if !matches!(ty, Ty::Plain) {
            self.scope_owns = owns;
        }
        let gives_rows =
            |t: &'static Type| t.property("model").is_some() && t.property("delegate").is_some();
        if let Ty::Exact(t) | Ty::Derived(t) = ty
            && gives_rows(t)
        {
            self.view_rows = Some(rows::of_view(object, &self.listed, &self.table_starts()));
        }
        let delegates_inside = match ty {
            Ty::Exact(t) | Ty::Derived(t) => {
                t.default_property() == Some("delegate") || (carrier && t.name == "Component")
            }
            _ => false,
        };
        if let Ty::Exact(t) = ty
            && let Some(shape) = arrange::Shape::of(t)
        {
            let changes = arrange::arrange(self.source, object, shape, a_root, self)?;
            self.changes.extend(changes);
        }
        let scope = match ty {
            Ty::Plain => root,
            other => other,
        };
        if let Some(on) = &object.on {
            self.member_name(on, scope, "a value source's target")?;
        }
        let is_changes = matches!(ty, Ty::Exact(t) if t.name == "PropertyChanges");
        let is_connections = matches!(ty, Ty::Exact(t) if t.name == "Connections");
        let target = if is_changes || is_connections {
            Some(self.target(object))
        } else {
            None
        };
        // Ljós's API names a change's object in the change: `target: box;
        // width: 30` is `box.width: 30`.
        let prefix = if is_changes {
            self.through_id(object)
        } else {
            None
        };
        for member in &object.members {
            match member {
                QmlMember::Object(child) if self.is_table(child) => {}
                QmlMember::Object(child) => {
                    if prefix.is_some() {
                        return Err(self.no(
                            format!(
                                "`{} {{ … }}` is set through the target of a `PropertyChanges`, \
                                 which Ljós's API names in each change",
                                child.type_name
                            ),
                            child.type_name.span.start as usize,
                        ));
                    }
                    let child_ty = self.object_type(child)?;
                    let child_root = if matches!(child_ty, Ty::Plain) {
                        scope
                    } else {
                        root
                    };
                    let made = matches!(ty, Ty::Exact(t) if t.name == "Component");
                    if delegates_inside && !matches!(child_ty, Ty::Plain) {
                        self.given = Some(self.view_rows.clone().unwrap_or(Rows::Unknown));
                    }
                    self.object(child, child_ty, child_root, made)?;
                }
                QmlMember::Binding(binding) => {
                    let made = self.changes.len();
                    self.binding(binding, ty, scope, root, target)?;
                    if let Some(id) = &prefix {
                        self.prefix(binding, id, made)?;
                    }
                }
                QmlMember::Property(property) => {
                    let name = property.name.name.as_str();
                    if renames_on(scope)
                        .is_some_and(|r| r.iter().any(|(o, n)| *o == name || *n == name))
                    {
                        return Err(self.no(
                            format!("it declares `{name}` on a type whose `{name}` Ljós renames"),
                            property.span.start as usize,
                        ));
                    }
                    if let Some(type_name) = &property.type_name {
                        if let Some(simple) = type_name.name.as_simple()
                            && matches!(simple, "var" | "variant")
                        {
                            let span = type_name.name.span;
                            self.change(span.start as usize..span.end as usize, "unknown");
                        }
                        self.written_type(type_name)?;
                    }
                    if let Some(value) = &property.value {
                        self.value(value, scope, root)?;
                    }
                }
                QmlMember::Function(function) => {
                    let name = function.id.as_ref().map(|id| id.name.as_str());
                    if is_connections
                        && let (Some(name), Some(span)) =
                            (name, function.id.as_ref().map(|id| id.span))
                    {
                        self.handler_name(
                            name,
                            span.start as usize..span.end as usize,
                            target.unwrap_or(Ty::Unknown),
                        )?;
                    }
                    self.script(scope, root, |v| {
                        v.visit_function(function, ScopeFlags::Function)
                    })?;
                }
                QmlMember::InlineComponent(component) => {
                    let ty = self.object_type(&component.object)?;
                    // Its own file, as far as names go.
                    let (delegate, root_owns) = (self.delegate.take(), self.root_owns);
                    let loose = std::mem::take(&mut self.loose);
                    self.root_owns = self.owns(&component.object, ty);
                    self.object(&component.object, ty, ty, true)?;
                    (self.delegate, self.root_owns, self.loose) = (delegate, root_owns, loose);
                }
                QmlMember::Signal(signal) => {
                    for param in &signal.params {
                        if let Some(type_name) = &param.type_name {
                            self.written_type(type_name)?;
                        }
                    }
                }
                QmlMember::Enum(_) => {}
            }
        }
        Ok(())
    }

    /// A type the file writes, a property's or a signal argument's, is one
    /// it names as much as one it makes: renamed as one is, and had from
    /// the module that has it (`property QtObject o` imports `Ljos.Core`).
    /// A value type (`int`, `font`) and a dotted name, an enum's or a
    /// module's, are not types of either module.
    fn written_type(&mut self, type_name: &QmlTypeName<'_>) -> Result<(), Kept> {
        let Some(name) = type_name.name.as_simple() else {
            return Ok(());
        };
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Ok(());
        }
        let at = type_name.name.span;
        if let Some(new) = renamed_type(name) {
            self.change(at.start as usize..at.end as usize, new);
        }
        self.named_type(name, at.start as usize, 0).map(|_| ())
    }

    /// What a `PropertyChanges` or `Connections` acts on.
    fn target(&self, object: &QmlObject<'_>) -> Ty {
        for member in &object.members {
            if let QmlMember::Binding(binding) = member
                && binding.name.as_simple() == Some("target")
            {
                return match &binding.value {
                    QmlBindingValue::Expression(Expression::Identifier(id)) => {
                        self.id(&id.name).unwrap_or(Ty::Unknown)
                    }
                    _ => Ty::Unknown,
                };
            }
        }
        // Without a target, a `Connections` is on its parent.
        Ty::Unknown
    }

    /// The id a `PropertyChanges`'s `target` names, with the target taken
    /// out; none when it has no target, or one that is not an id.
    ///
    /// DECISION: a target that is not an id (a property, `parent`, a path)
    /// stays a target. The id form has no spelling for it, and Ljos.Ui's
    /// `PropertyChanges` still has `target`, so the file is lowered as
    /// before rather than kept.
    fn through_id(&mut self, object: &QmlObject<'_>) -> Option<String> {
        let binding = object.members.iter().find_map(|member| match member {
            QmlMember::Binding(binding) if binding.name.as_simple() == Some("target") => {
                Some(binding)
            }
            _ => None,
        })?;
        let QmlBindingValue::Expression(Expression::Identifier(id)) = &binding.value else {
            return None;
        };
        self.id(&id.name)?;
        let id = id.name.to_string();
        let range = member_extent(
            self.source,
            binding.span.start as usize,
            binding.span.end as usize,
        );
        self.change(range, "");
        Some(id)
    }

    /// Writes the id of a `PropertyChanges`'s target into a change of it:
    /// `width: 30` is `box.width: 30`, `onClicked` `box.onClicked`. What
    /// the pass renamed of it since `made` is renamed under the id too.
    fn prefix(&mut self, binding: &QmlBinding<'_>, id: &str, made: usize) -> Step {
        let name = &binding.name;
        let first = name.parts[0];
        if matches!(first, "target" | "explicit" | "restoreEntryValues" | "id") {
            return Ok(());
        }
        if first.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Err(self.no(
                format!(
                    "`{name}` is set through the target of a `PropertyChanges`, which Ljós's \
                     API names in each change"
                ),
                name.span.start as usize,
            ));
        }
        if name.parts.len() > 1 && self.id(first).is_some() {
            // Already through an id: `target: a; b.width: 3`.
            return Ok(());
        }
        let span = name.span.start as usize..name.span.end as usize;
        let start = span.start;
        match self.changes[made..]
            .iter_mut()
            .find(|c| c.range.start == start)
        {
            Some(renamed) => renamed.replacement = format!("{id}.{}", renamed.replacement),
            None => self.change(start..start, format!("{id}.")),
        }
        Ok(())
    }

    fn binding(
        &mut self,
        binding: &QmlBinding<'_>,
        ty: Ty,
        scope: Ty,
        root: Ty,
        target: Option<Ty>,
    ) -> Step {
        let name = &binding.name;
        let at = name.span.start as usize;
        let first = name.parts[0];
        if name.parts.len() == 1 {
            if first == "id" {
                return Ok(());
            }
            match (
                target,
                matches!(ty, Ty::Exact(t) if t.name == "PropertyChanges"),
            ) {
                // `PropertyChanges { target: rect; color: ... }` sets the
                // target's members.
                (Some(target), true)
                    if first != "target"
                        && first != "explicit"
                        && first != "restoreEntryValues" =>
                {
                    self.member_name(name, target, "the target of a `PropertyChanges`")?;
                }
                (Some(target), false) if first.starts_with("on") => {
                    let span = name.span;
                    self.handler_name(first, span.start as usize..span.end as usize, target)?;
                }
                _ if matches!(ty, Ty::Plain) => {}
                _ => self.member_name(name, ty, "the object")?,
            }
        } else if first.starts_with(|c: char| c.is_ascii_uppercase()) {
            // An attached object: `Keys.onPressed`, `Component.onCompleted`.
            self.named_type(first, at, 0)?;
        } else if let Some(id) = self.id(first)
            && matches!(ty, Ty::Exact(t) if t.name == "PropertyChanges")
        {
            // `PropertyChanges { rect.color: ... }`.
            self.part(name, 1, id)?;
        }
        if name.parts.len() == 1
            && first == "delegate"
            && matches!(binding.value, QmlBindingValue::Object(_))
            && matches!(ty, Ty::Exact(t) | Ty::Derived(t) if t.property("delegate").is_some())
        {
            self.given = Some(self.view_rows.clone().unwrap_or(Rows::Unknown));
        }
        if let QmlBindingValue::Object(object) = &binding.value
            && self.is_table(object)
        {
            return Ok(());
        }
        self.value(&binding.value, scope, root)
    }

    /// Renames the simple name `name` when it is a member the type `on`
    /// has under another name in Ljós.
    fn member_name(&mut self, name: &QmlQualifiedName<'_>, on: Ty, what: &str) -> Step {
        let Some(simple) = name.as_simple() else {
            return Ok(());
        };
        let span = name.span.start as usize..name.span.end as usize;
        if simple.starts_with("on") && simple.len() > 2 {
            return self.handler_name(simple, span, on);
        }
        match self.member(simple, on) {
            Rename::To(new) => self.change(span, new),
            Rename::Keep => {}
            Rename::Unknown => {
                return Err(self.no(
                    format!("`{simple}` is set on {what}, whose type is not known"),
                    span.start,
                ));
            }
        }
        Ok(())
    }

    /// `onColorChanged` on a type whose `color` is `fill`: `onFillChanged`.
    fn handler_name(&mut self, handler: &str, span: Range<usize>, on: Ty) -> Step {
        let Some(signal) = handler.strip_prefix("on") else {
            return Ok(());
        };
        let mut chars = signal.chars();
        let Some(first) = chars.next() else {
            return Ok(());
        };
        let signal = format!("{}{}", first.to_ascii_lowercase(), chars.as_str());
        match self.member(&signal, on) {
            Rename::To(new) => {
                let mut chars = new.chars();
                let first: String = chars
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .into_iter()
                    .collect();
                self.change(span, format!("on{first}{}", chars.as_str()));
            }
            Rename::Keep => {}
            Rename::Unknown => {
                return Err(self.no(
                    format!("`{handler}` handles a signal of an object whose type is not known"),
                    span.start,
                ));
            }
        }
        Ok(())
    }

    /// Renames the `index`th part of a dotted name, a member of `on`.
    fn part(&mut self, name: &QmlQualifiedName<'_>, index: usize, on: Ty) -> Step {
        let Some(range) = part_range(self.source, name, index) else {
            return Ok(());
        };
        match self.member(name.parts[index], on) {
            Rename::To(new) => self.change(range, new),
            Rename::Keep => {}
            Rename::Unknown => {
                return Err(self.no(
                    format!("`{name}` is set on an object whose type is not known"),
                    range.start,
                ));
            }
        }
        Ok(())
    }

    /// What `name`, read off something of type `on`, is called in Ljós.
    fn member(&self, name: &str, on: Ty) -> Rename {
        if !is_renamed_member(name) {
            return Rename::Keep;
        }
        match on {
            Ty::Plain => Rename::Keep,
            Ty::Unknown => Rename::Unknown,
            Ty::Exact(ty) | Ty::Derived(ty) => match renames_of(ty) {
                Some(renames) => match lookup_rename(renames, name) {
                    Some(new) => Rename::To(new),
                    None => Rename::Keep,
                },
                // Derived from a type that is not one that renames it, but
                // perhaps from one derived from that: an Item may be a
                // Rectangle.
                None if matches!(on, Ty::Derived(_)) && could_be_renaming(ty) => Rename::Unknown,
                None => Rename::Keep,
            },
        }
    }

    fn value(&mut self, value: &QmlBindingValue<'_>, scope: Ty, root: Ty) -> Step {
        match value {
            QmlBindingValue::Expression(expression) => {
                self.script(scope, root, |v| v.visit_expression(expression))
            }
            QmlBindingValue::Statement(statement) => {
                self.script(scope, root, |v| v.visit_statement(statement))
            }
            QmlBindingValue::Object(object) => {
                let ty = self.object_type(object)?;
                self.object(object, ty, root, true)
            }
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    let ty = self.object_type(object)?;
                    self.object(object, ty, root, true)?;
                }
                Ok(())
            }
        }
    }

    fn script(&mut self, scope: Ty, root: Ty, walk: impl FnOnce(&mut Script<'_, 's>)) -> Step {
        let mut script = Script {
            pass: self,
            scope,
            root,
            kept: None,
            declared: Vec::new(),
            row: RowEdits::default(),
            called: Vec::new(),
        };
        walk(&mut script);
        script.row_done();
        match script.kept {
            Some(kept) => Err(kept),
            None => Ok(()),
        }
    }

    /// Whether `object`, of type `ty`, has `model`, `modelData` and
    /// `index`: declared, or as members of its type.
    fn owns(&self, object: &QmlObject<'_>, ty: Ty) -> Owns {
        let one = |name: &str| -> Option<bool> {
            let declared = object
                .members
                .iter()
                .any(|m| matches!(m, QmlMember::Property(p) if p.name.name.as_str() == name));
            if declared {
                return Some(true);
            }
            match ty {
                Ty::Exact(t) => Some(t.property(name).is_some()),
                Ty::Plain => Some(false),
                Ty::Derived(_) | Ty::Unknown => object
                    .type_name
                    .as_simple()
                    .and_then(|n| self.type_declares(n, name, 0)),
            }
        };
        Owns {
            model: one("model"),
            model_data: one("modelData"),
            index: one("index"),
        }
    }

    /// Whether the type `name` has the property `member`: Qt's, or one a
    /// file of the project or an inline component declares.
    fn type_declares(&self, name: &str, member: &str, depth: usize) -> Option<bool> {
        use arrange::Types;
        if let Some(t) = self.qt_type(name) {
            return Some(t.property(member).is_some());
        }
        if depth > 16 {
            return None;
        }
        let root = self
            .inline
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, root)| root.clone())
            .or_else(|| self.locals.root(name))?;
        if root.properties.iter().any(|p| p == member) {
            return Some(true);
        }
        if root.type_name == name {
            return None;
        }
        self.type_declares(&root.type_name, member, depth + 1)
    }
}

/// The id an object binds.
fn object_id(object: &QmlObject<'_>) -> Option<String> {
    object.members.iter().find_map(|member| match member {
        QmlMember::Binding(binding) if binding.name.as_simple() == Some("id") => {
            match &binding.value {
                QmlBindingValue::Expression(Expression::Identifier(id)) => {
                    Some(id.name.to_string())
                }
                _ => None,
            }
        }
        _ => None,
    })
}

/// What a script writes of a delegate's row, held until the script is
/// read whole: a local it declares later may be what a name is.
#[derive(Default)]
struct RowEdits {
    changes: Vec<Change>,
    kept: Option<Kept>,
    /// Where it read `model` as the row.
    read: Option<u32>,
    /// The names the edits write: `index`, `modelData`.
    wrote: Vec<&'static str>,
}

/// What `model` names where a script is.
enum Model {
    /// A member of an object, or an id: the file's own.
    Own,
    /// The row of a delegate of a view of these rows; `None` for the row
    /// of a view the file is not shown in.
    Row(Option<Rows>),
    /// An object here may have it.
    Unsure,
}

impl arrange::Types for Pass<'_> {
    fn qt_type(&self, name: &str) -> Option<&'static Type> {
        self.quick
            .type_named(name)
            .or_else(|| self.layouts.and_then(|m| m.type_named(name)))
            .or_else(|| self.controls.and_then(|c| c.qt.type_named(name)))
            .or_else(|| self.shapes.and_then(|m| m.type_named(name)))
            .or_else(|| self.dialogs.and_then(|c| c.qt.type_named(name)))
    }

    fn root_bindings(&self, name: &str) -> Option<Vec<String>> {
        let mut found: Option<Vec<String>> = None;
        let mut name = name.to_string();
        // A root built on another file of the project binds what that
        // file's root binds too; as far as `named_type` follows it.
        for _ in 0..16 {
            if self.qt_type(&name).is_some() {
                break;
            }
            let Some(root) = self.local_root(&name, self.imported_root(&name)) else {
                break;
            };
            found.get_or_insert_default().extend(root.bindings);
            if root.type_name == name {
                break;
            }
            name = root.type_name;
        }
        found
    }
}

enum Rename {
    To(String),
    Keep,
    Unknown,
}

fn renamed_type(name: &str) -> Option<&'static str> {
    type_renames()
        .find(|(old, _)| *old == name)
        .map(|(_, new)| *new)
}

/// A member name, with its change signal, that some type renames.
fn is_renamed_member(name: &str) -> bool {
    member_renames().any(|(_, renames)| lookup_rename(renames, name).is_some())
}

fn lookup_rename(renames: &[(&str, &str)], name: &str) -> Option<String> {
    renames.iter().find_map(|(old, new)| {
        if *old == name {
            return Some((*new).to_string());
        }
        // The change signal follows the property.
        let signal = name.strip_suffix("Changed")?;
        (signal == *old).then(|| format!("{new}Changed"))
    })
}

/// The renames of `ty` or the type it derives from that has some.
fn renames_of(ty: &'static Type) -> Option<&'static [(&'static str, &'static str)]> {
    let mut at = Some(ty);
    while let Some(ty) = at {
        if let Some((_, renames)) = member_renames().find(|(name, _)| *name == ty.name) {
            return Some(renames);
        }
        at = ty.prototype();
    }
    None
}

fn renames_on(on: Ty) -> Option<&'static [(&'static str, &'static str)]> {
    match on {
        Ty::Exact(ty) | Ty::Derived(ty) => renames_of(ty),
        Ty::Plain | Ty::Unknown => None,
    }
}

/// Whether something of type `ty` may be of a type that renames members:
/// an `Item` may be a `Rectangle`, a `Text` or a `Timer` may not.
fn could_be_renaming(ty: &'static Type) -> bool {
    let is_item = |mut at: Option<&'static Type>| {
        while let Some(t) = at {
            if t.name == "Item" {
                return true;
            }
            at = t.prototype();
        }
        false
    };
    // Every type that renames is an Item; one that is an Item and is no
    // further along than a renaming type's ancestors may be one.
    is_item(Some(ty))
        && member_renames().any(|(name, _)| {
            qt_types::module("QtQuick")
                .and_then(|m| m.type_named(name))
                .is_some_and(|renaming| {
                    let mut at = Some(renaming);
                    while let Some(t) = at {
                        if std::ptr::eq(t, ty) {
                            return true;
                        }
                        at = t.prototype();
                    }
                    false
                })
        })
}

/// What of the text a member from `start` to `end` takes, with the `;`
/// after it and the blanks after that; on a line of its own, the line but
/// its line break, so that every other line stays where it was.
fn member_extent(source: &str, start: usize, end: usize) -> Range<usize> {
    let bytes = source.as_bytes();
    let blanks = |mut at: usize| {
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
        at
    };
    let mut after = blanks(end);
    if bytes.get(after) == Some(&b';') {
        after = blanks(after + 1);
    }
    let line = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let alone = source[line..start].trim().is_empty()
        && matches!(bytes.get(after), None | Some(b'\n' | b'\r'));
    if alone { line..after } else { start..after }
}

/// Where the `index`th part of a dotted name is in the text.
fn part_range(source: &str, name: &QmlQualifiedName<'_>, index: usize) -> Option<Range<usize>> {
    let start = name.span.start as usize;
    let text = source.get(start..name.span.end as usize)?;
    let mut from = 0;
    for (i, part) in name.parts.iter().enumerate() {
        let found = text[from..].find(part)? + from;
        if i == index {
            return Some(start + found..start + found + part.len());
        }
        from = found + part.len();
    }
    None
}

/// The pass over a script: a binding's expression, a handler, a function.
struct Script<'p, 's> {
    pass: &'p mut Pass<'s>,
    /// The object whose members an unqualified name is first.
    scope: Ty,
    /// The file's root object, whose members it is next.
    root: Ty,
    kept: Option<Kept>,
    /// The locals it declares that a delegate's row has names of.
    declared: Vec<String>,
    row: RowEdits,
    /// Where the names that are called start: a called name is a method,
    /// and every member Ljós renames is a property (`model.remove(0)`).
    called: Vec<u32>,
}

impl Script<'_, '_> {
    fn keep(&mut self, reason: String, at: u32) {
        if self.kept.is_none() {
            self.kept = Some(Kept {
                reason,
                at: at as usize,
            });
        }
    }

    /// What the pass knows of what `expression` is.
    fn ty(&self, expression: &Expression<'_>) -> Ty {
        match expression {
            Expression::Identifier(id) => self.name(&id.name),
            Expression::StaticMemberExpression(member) => {
                let on = self.ty(&member.object);
                self.member_ty(on, &member.property.name)
            }
            Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::TemplateLiteral(_) => Ty::Plain,
            _ => Ty::Unknown,
        }
    }

    fn name(&self, name: &str) -> Ty {
        if let Some(ty) = self.pass.id(name) {
            return ty;
        }
        if name.starts_with(|c: char| c.is_ascii_uppercase())
            && self
                .pass
                .local_root(name, self.pass.imported_root(name))
                .is_some()
        {
            // A type of the project read as a value is a singleton: the
            // one object its file makes, so of the type its root is and of
            // no type built on that. What the file declares is its own.
            return match self.pass.named_type(name, 0, 0) {
                Ok(Ty::Exact(ty) | Ty::Derived(ty)) => Ty::Exact(ty),
                Ok(other) => other,
                Err(_) => Ty::Unknown,
            };
        }
        if name == "parent" {
            // An Item; which one is known when the file runs.
            return self
                .pass
                .quick
                .type_named("Item")
                .map_or(Ty::Unknown, Ty::Derived);
        }
        match self.member_ty(self.scope, name) {
            Ty::Unknown => self.member_ty(self.root, name),
            known => known,
        }
    }

    /// The type of the member `name` of something of type `on`.
    fn member_ty(&self, on: Ty, name: &str) -> Ty {
        let (Ty::Exact(ty) | Ty::Derived(ty)) = on else {
            return Ty::Unknown;
        };
        let Some(property) = ty.property(name) else {
            return Ty::Unknown;
        };
        match property.value_type() {
            Some(value) if value.is_value => Ty::Plain,
            Some(value) => Ty::Derived(value),
            None => Ty::Plain,
        }
    }
}

impl Script<'_, '_> {
    /// What `model` is here.
    fn model(&self) -> Model {
        if self.pass.id("model").is_some() {
            return Model::Own;
        }
        let behind = self
            .pass
            .delegate
            .as_ref()
            .map_or(self.pass.root_owns, |d| d.owns);
        match (self.pass.scope_owns.model, behind.model) {
            (Some(true), _) | (Some(false), Some(true)) => Model::Own,
            (None, _) | (_, None) => Model::Unsure,
            (Some(false), Some(false)) => {
                Model::Row(self.pass.delegate.as_ref().map(|d| d.rows.clone()))
            }
        }
    }

    /// Whether `name`, read unqualified here, is the row's.
    fn free(&self, name: &str) -> bool {
        let behind = self
            .pass
            .delegate
            .as_ref()
            .map_or(self.pass.root_owns, |d| d.owns);
        self.pass.id(name).is_none()
            && self.pass.scope_owns.get(name) == Some(false)
            && behind.get(name) == Some(false)
    }

    fn row_keep(&mut self, reason: String, at: u32) {
        if self.row.kept.is_none() {
            self.row.kept = Some(Kept {
                reason,
                at: at as usize,
            });
        }
    }

    /// Writes `span` as `name`, which a delegate's row gives, unless
    /// something here has a member of that name.
    fn row_write(&mut self, span: oxc_span::Span, name: &'static str, what: &str) {
        if self.free(name) {
            self.row.changes.push(Change {
                range: span.start as usize..span.end as usize,
                replacement: name.to_string(),
            });
            self.row.wrote.push(name);
        } else {
            self.row_keep(
                format!("`{what}` would be `{name}`, which an object here has a member of"),
                span.start,
            );
        }
    }

    /// `model.<member>`, where `model` is the row of a delegate of `rows`.
    fn row_member(&mut self, it: &StaticMemberExpression<'_>, rows: Option<Rows>) {
        let member = it.property.name.as_str();
        let at = it.span.start;
        self.row.read.get_or_insert(at);
        let what = format!("model.{member}");
        match (member, rows) {
            ("index", Some(_)) => self.row_write(it.span, "index", &what),
            ("modelData", Some(_)) => self.row_write(it.span, "modelData", &what),
            // No view is shown, so what the row is is not known either.
            ("index", None) => self.row_keep(self.unshown(&what, "the row", "int index"), at),
            ("modelData", None) => {
                self.row_keep(self.unshown(&what, "the row", "var modelData"), at);
            }
            (_, Some(Rows::Roles)) => self.row_write(it.object.span(), "modelData", &what),
            (_, Some(Rows::OneRole(role))) if role == member => {
                self.row_write(it.span, "modelData", &what);
            }
            (_, Some(Rows::OneRole(_))) => self.row_keep(
                format!("`{what}` reads a role the model does not have, which is `undefined`; take it out"),
                at,
            ),
            (_, Some(Rows::Objects)) => self.row_keep(
                format!("`{what}` reads a role of an array, which gives none; read `modelData.{member}`, the element's"),
                at,
            ),
            (_, Some(Rows::Values)) => self.row_keep(
                format!("`{what}` reads a role of a model of values, which gives none; take it out"),
                at,
            ),
            (_, Some(Rows::Unknown)) => self.row_keep(
                format!("`{what}` reads a role of a model the file does not show, which `modelData.{member}` reads only of a model of several roles; give the delegate `required property var {member}`"),
                at,
            ),
            (_, None) => self.row_keep(
                self.unshown(&what, "a role of the row", &format!("var {member}")),
                at,
            ),
        }
    }

    /// Why `what`, which reads `part` of a row where the file shows no
    /// view, keeps the file, and what to declare in its place.
    fn unshown(&self, what: &str, part: &str, declare: &str) -> String {
        if self.pass.loose {
            format!(
                "`{what}` reads {part} of a view the `Component` it is in is not written in the `delegate:` of, or given to by id alone; give the Component's root `required property {declare}`"
            )
        } else {
            format!(
                "`{what}` reads {part} of a view the file is not shown in; give the root `required property {declare}`"
            )
        }
    }

    /// The script is read: what it wrote of the row is the file's, unless
    /// a local it declares has a name of the row.
    fn row_done(&mut self) {
        let row = std::mem::take(&mut self.row);
        if let Some(at) = row.read
            && self.declared.iter().any(|d| d == "model")
        {
            self.keep(
                "a local is called `model`, as a delegate's row is".into(),
                at,
            );
            return;
        }
        for name in &row.wrote {
            if self.declared.iter().any(|d| d == name) {
                let at = row.changes.first().map_or(0, |c| c.range.start as u32);
                self.keep(
                    format!("a local is called `{name}`, as the row's `{name}` is"),
                    at,
                );
                return;
            }
        }
        if let Some(kept) = row.kept {
            self.keep(kept.reason, kept.at as u32);
        }
        self.pass.changes.extend(row.changes);
    }
}

impl<'a> Visit<'a> for Script<'_, '_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        match it.callee.without_parentheses() {
            Expression::StaticMemberExpression(m) => self.called.push(m.property.span.start),
            Expression::Identifier(id) => self.called.push(id.span.start),
            _ => {}
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object
            && object.name == "model"
        {
            match self.model() {
                Model::Own => {}
                Model::Unsure => {
                    let reason = format!(
                        "`model.{}` may read a member of an object whose type is not known",
                        it.property.name
                    );
                    self.row_keep(reason, it.span.start);
                    return;
                }
                Model::Row(rows) => {
                    self.row_member(it, rows);
                    return;
                }
            }
        }
        let name = it.property.name.as_str();
        if name == "createQmlObject" {
            // The text is compiled when the script runs, as written.
            self.keep(
                "it makes objects from QML text, which is not lowered yet".into(),
                it.property.span.start,
            );
        }
        if is_renamed_member(name) && !self.called.contains(&it.property.span.start) {
            let on = self.ty(&it.object);
            match self.pass.member(name, on) {
                Rename::To(new) => {
                    let span = it.property.span;
                    self.pass
                        .change(span.start as usize..span.end as usize, new);
                }
                Rename::Keep => {}
                Rename::Unknown => self.keep(
                    format!("`.{name}` is read off something whose type is not known"),
                    it.property.span.start,
                ),
            }
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if let Expression::StringLiteral(key) = &it.expression
            && is_renamed_member(&key.value)
        {
            self.keep(
                format!("`[\"{}\"]` names a member Ljós renames", key.value),
                key.span.start,
            );
        }
        walk::walk_computed_member_expression(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        let name = it.name.as_str();
        if name == "model" {
            match self.model() {
                Model::Own => {}
                Model::Unsure => {
                    self.row_keep(
                        "`model` may be a member of an object whose type is not known".into(),
                        it.span.start,
                    );
                }
                Model::Row(Some(Rows::Roles)) => {
                    self.row.read.get_or_insert(it.span.start);
                    self.row_write(it.span, "modelData", "model");
                }
                Model::Row(_) => {
                    self.row.read.get_or_insert(it.span.start);
                    self.row_keep(
                        "`model` is the row of a model whose roles the file does not show; read the roles it needs by name".into(),
                        it.span.start,
                    );
                }
            }
            return;
        }
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            if let Some(new) = renamed_type(name) {
                self.pass
                    .change(it.span.start as usize..it.span.end as usize, new);
            } else if (self.pass.quick.type_named(name).is_some()
                || [self.pass.controls, self.pass.dialogs]
                    .into_iter()
                    .flatten()
                    .any(|c| c.qt.type_named(name).is_some())
                || self
                    .pass
                    .shapes
                    .is_some_and(|m| m.type_named(name).is_some()))
                && let Err(kept) = self.pass.named_type(name, it.span.start as usize, 0)
            {
                self.keep(kept.reason, it.span.start);
            } else if self.pass.ljos_module_with(name).is_some()
                && let Err(kept) = self.pass.named_type(name, it.span.start as usize, 0)
            {
                // A name the Qt imports lack, read in a script, is the
                // project's or the context's; lowered, it would be Ljós's.
                self.keep(kept.reason, it.span.start);
            }
            return;
        }
        if !is_renamed_member(name)
            || self.pass.id(name).is_some()
            || self.called.contains(&it.span.start)
        {
            return;
        }
        // An unqualified member: the scope object's, else the root's.
        let on = match self.pass.member(name, self.scope) {
            Rename::Keep if self.member_ty(self.scope, name).is_known() => return,
            Rename::Keep => self.root,
            _ => self.scope,
        };
        match self.pass.member(name, on) {
            Rename::To(new) => self
                .pass
                .change(it.span.start as usize..it.span.end as usize, new),
            Rename::Keep => {}
            Rename::Unknown => self.keep(
                format!("`{name}` may be a member of an object whose type is not known"),
                it.span.start,
            ),
        }
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        if matches!(it.name.as_str(), "model" | "modelData" | "index") {
            self.declared.push(it.name.to_string());
        }
        // A local of a renamed name hides the member; renaming its uses
        // would rename the local's. Only where the member is what the name
        // could read: on an object that is no ListView, `remove` is free.
        let hides = |on: Ty| !matches!(self.pass.member(&it.name, on), Rename::Keep);
        if is_renamed_member(&it.name) && (hides(self.scope) || hides(self.root)) {
            self.keep(
                format!(
                    "a local is called `{}`, as a member Ljós renames is",
                    it.name
                ),
                it.span.start,
            );
        }
    }
}

impl Ty {
    fn is_known(self) -> bool {
        !matches!(self, Ty::Unknown)
    }
}

#[cfg(test)]
mod tests;
