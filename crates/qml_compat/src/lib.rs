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

mod bindings;
mod cause;
mod given;
mod group;
mod modules;
mod qt_members;
mod rows;
mod rules;
mod scope;
mod tables;

use std::{collections::HashSet, ops::Range};

use bindings::{Bindings, Node};
use modules::{Ljos, Modules, Qt};
use oxc_ast::ast::{
    BindingIdentifier, CallExpression, ComputedMemberExpression, Expression, IdentifierReference,
    StaticMemberExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};
use qml_edit::Change;
use qml_syntax::{
    Allocator,
    ast::{
        QmlBinding, QmlBindingValue, QmlDocument, QmlMember, QmlObject, QmlQualifiedName,
        QmlTypeName,
    },
};
use qt_types::Type;
use rows::{Delegate, Owns, Rows};
use rules::{arrange, font, gesture};

pub use cause::{Cause, Differs, Font, Layout, Loader, Owner, Row};
pub use group::{Alone, References, group, references};
pub use rules::{FONT_RENAMES, LAYOUT_RENAMES, member_renames, type_renames};

/// The module a lowered file imports.
pub const MODULE: &str = "Ljos.Ui";

/// The module a lowered file imports its controls from: Ljós's controls
/// as Qt's Basic style draws them, so a lowered file looks as it did.
/// `Ljos.Controls`, Ljós's own style, is the one a file written for Ljós
/// imports (docs/DIALECT.md §4.16).
pub const CONTROLS: &str = "Ljos.Controls.Basic";

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

/// The module a lowered file imports too when it arranges children.
pub const LAYOUT: &str = "Ljos.Layout";

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
    /// What of the file the pass cannot lower; it is written as the reason
    /// `ljos migrate` prints: "`PathView` is not in Ljos.Ui yet".
    pub cause: Cause,
    /// Where, as a byte offset into the file.
    pub at: usize,
}

impl std::fmt::Display for Kept {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
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
    /// The properties, functions and signals it declares: names in sight
    /// of a file built on it.
    pub declares: Vec<String>,
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

    /// The program's own types, as the checker is told them
    /// (`ljostypes.json`); none when they are not described, and a file
    /// that imports a module of them is kept.
    fn natives(&self) -> Option<&dyn Natives> {
        None
    }
}

/// A native type, as the [`Natives`] that has it numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Native(pub u32);

/// What a property of a native type holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Holds {
    /// An object of another of the program's types.
    Native(Native),
    /// An object of a type of the toolkit's, in module `module`.
    Toolkit { module: String, name: String },
    /// A value, or an object the description does not type.
    Value,
}

/// The modules of types a program registers itself, which no table of
/// Qt's or Ljós's has: what the checker reads of them, for the pass.
pub trait Natives {
    /// Whether `uri` is a module of the program's own.
    fn has_module(&self, uri: &str) -> bool;

    fn module_type(&self, uri: &str, name: &str) -> Option<Native>;

    /// The type's name, for messages.
    fn name(&self, ty: Native) -> String;

    /// The type is one object a file reads, not one it makes.
    fn is_singleton(&self, ty: Native) -> bool;

    /// Whether the type has a member `name`: a property, its change
    /// signal, a signal, a method or an enum.
    fn has_member(&self, ty: Native, name: &str) -> bool;

    /// What its property `name` holds; None when it has no such property.
    fn property(&self, ty: Native, name: &str) -> Option<Holds>;

    /// The module and name of the toolkit type it is built on, whose
    /// members it has under Qt's names.
    fn toolkit_base(&self, ty: Native) -> Option<(String, String)>;
}

/// The project's files, in a program that starts with the controls style
/// `style`, with its own types `natives`.
pub struct Styled<'a> {
    pub locals: &'a dyn Locals,
    pub style: &'a str,
    pub natives: Option<&'a dyn Natives>,
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

    fn natives(&self) -> Option<&dyn Natives> {
        self.natives
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
        return kept(Cause::Unparsed, 0);
    }
    let modules = match Modules::new() {
        Ok(modules) => modules,
        Err(module) => return kept(Cause::NoDescription { module }, 0),
    };
    let mut pass = Pass {
        source,
        scope: scope::Scope::new(modules, locals),
        read: HashSet::new(),
        fonts: Vec::new(),
        changes: Vec::new(),
        listed: Vec::new(),
        given: None,
        view_rows: None,
        delegate: None,
        by_id: Vec::new(),
        loose: false,
        scope_owns: Owns::NONE,
        root_owns: Owns::NONE,
        stem,
        tables: Vec::new(),
        declared: Vec::new(),
        members: qt_members::Members::default(),
        root_at: parsed.document.root.span.start,
        inline_roots: Vec::new(),
        unqualified: Vec::new(),
        aimed: Vec::new(),
    };
    let mut unqualified = Unqualified::default();
    unqualified.object(&parsed.document.root);
    pass.unqualified = unqualified.found;
    match pass.document(&parsed.document) {
        Ok(false) => Outcome::Unchanged,
        Ok(true) => {
            let mut changes = pass.changes;
            changes.sort_by_key(|c| c.range.start);
            changes.dedup();
            let text = qml_edit::splice(source, &changes);
            let again = qml_syntax::parse(&allocator, allocator.alloc_str(&text));
            if !again.is_ok() {
                return kept(Cause::LoweredUnparsed, 0);
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
            declares: object
                .members
                .iter()
                .filter_map(|member| match member {
                    QmlMember::Property(property) => Some(property.name.name.to_string()),
                    QmlMember::Function(function) => {
                        function.id.as_ref().map(|id| id.name.to_string())
                    }
                    QmlMember::Signal(signal) => Some(signal.name.name.to_string()),
                    _ => None,
                })
                .collect(),
        }
    }
}

fn kept(cause: Cause, at: usize) -> Outcome {
    Outcome::Kept(Kept { cause, at })
}

/// What the pass knows of what a name or an expression is.
#[derive(Clone, Copy, Debug)]
enum Ty {
    /// This object, of this type: an id, or the object a binding is on.
    Exact(&'static Type),
    /// Of this type or one derived from it.
    Derived(&'static Type),
    /// A value with no members the pass renames: a number, a string, a
    /// `color`.
    Plain,
    /// A `font`, whose members [`FONT_RENAMES`] says.
    Font,
    /// The pass does not know.
    Unknown,
    /// Of the program's own type, exactly when the flag says so, else of
    /// it or one derived from it.
    Native(Native, bool),
}

struct Pass<'s> {
    source: &'s str,
    /// What a name means in the file.
    scope: scope::Scope<'s>,
    /// Every name the file's scripts read unqualified, or a binding's name
    /// starts with.
    read: HashSet<String>,
    /// The properties the file declares as a `font`, a `var`, a `variant`
    /// or an `alias` (`property alias face: label.font`): what a script may
    /// hold a font in.
    fonts: Vec<String>,
    changes: Vec<Change>,
    /// The `ListModel`s with ids, and the rows they give.
    listed: Vec<(String, Rows)>,
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
    /// What the scope object declares: an unqualified name it declares is
    /// its own, before the root's.
    declared: Vec<String>,
    /// The members of `Qt` that `Ljos.Core` has under its own names.
    members: qt_members::Members,
    /// Where the file's root object starts.
    root_at: u32,
    /// The root object of each inline component, which is the scope of
    /// the scripts in it as the file's root is of the file's.
    inline_roots: Vec<Span>,
    /// Where a script names unqualified a member a rule asks about
    /// ([`rules::unqualified`]): a Loader's `source`, say, when one is in
    /// reach.
    unqualified: Vec<(u32, &'static str)>,
    /// What objects aimed at an id (`target: loader`) set or handle there.
    aimed: Vec<(String, String)>,
}

type Step = Result<(), Kept>;

impl<'s> Pass<'s> {
    /// Whether the file imports `QtQuick` and so has something to lower.
    fn document(&mut self, document: &QmlDocument<'_>) -> Result<bool, Kept> {
        let qt_imports = self.scope.read(&document.imports)?;
        if !qt_imports
            .iter()
            .any(|(row, _)| matches!(row.qt, Qt::Quick | Qt::Qml))
        {
            return Ok(false);
        }
        let lines = modules::lines(&qt_imports.iter().map(|(row, _)| *row).collect::<Vec<_>>());
        if self.stem.is_some() {
            let quick = self.scope.modules.quick();
            self.tables = tables::tables(self.source, &document.root, &|name| {
                quick.type_named(name).is_some()
            });
        }
        self.collect(&document.root);
        let mut names = gesture::Names::default();
        names.object(&document.root);
        self.read = names.0;
        let by_id = given::by_id(
            &document.root,
            &|o| self.is_view(o),
            &self.listed,
            &self.table_starts(),
        )
        .into_iter()
        .filter(|(id, _)| matches!(self.scope.id(id), Some(Ty::Exact(t)) if t.name == "Component"))
        .collect();
        self.by_id = by_id;
        let root = self.object_type(&document.root)?;
        self.root_owns = self.owns(&document.root, root);
        self.object(&document.root, root, root, true)?;
        let members = std::mem::take(&mut self.members);
        for (range, name) in members.settle(self.source, |name| self.scope.may_be_beside(name)) {
            self.change(range, name);
            self.scope.modules.names(Ljos::Core);
        }
        // On the import's line, so every line stays where it was. Each Qt
        // import takes the line of the Ljós module it lowers to, or is
        // emptied ([`modules::lines`]). `QtQuick`'s, else `QtQml`'s, has the
        // pragma and the Ljós modules the file names a type of that no
        // import of its lowers to: a file that makes a model imports
        // `Ljos.Models` there, and one a child of a type of another file
        // may be given a `Layout` in, `Ljos.Layout`.
        let own = |ljos: Ljos| lines.contains(&modules::Line::Takes(ljos));
        let shared = if own(Ljos::Ui) { Ljos::Ui } else { Ljos::Core };
        // A file whose constant tables are typed lists imports their rows
        // module on that line too.
        let rows = match (self.stem, self.tables.is_empty()) {
            (Some(stem), false) => {
                format!("; import \"{}\" as {}", rows_module(stem), tables::ALIAS)
            }
            _ => String::new(),
        };
        for ((_, span), line) in qt_imports.iter().zip(&lines) {
            let import = match line {
                modules::Line::Takes(ljos) if *ljos == shared => {
                    let line =
                        modules::shared_line(shared, own, |ljos| self.scope.modules.is_named(ljos));
                    format!("{line}{rows}")
                }
                modules::Line::Takes(ljos) => format!("import {}", ljos.uri()),
                modules::Line::Emptied => String::new(),
            };
            self.change(span.start as usize..span.end as usize, import);
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

    fn no(&self, cause: impl Into<Cause>, at: usize) -> Kept {
        Kept {
            cause: cause.into(),
            at,
        }
    }

    fn change(&mut self, range: Range<usize>, replacement: impl Into<String>) {
        self.changes.push(Change {
            range,
            replacement: replacement.into(),
        });
    }

    /// The ids and inline components, before anything refers to them.
    fn collect(&mut self, object: &QmlObject<'_>) {
        let aimed_at = object.members.iter().find_map(|member| match member {
            QmlMember::Binding(QmlBinding {
                name,
                value: QmlBindingValue::Expression(Expression::Identifier(id)),
                ..
            }) if name.as_simple() == Some("target") => Some(id.name.to_string()),
            _ => None,
        });
        if let Some(id) = aimed_at {
            for member in &object.members {
                let name = match member {
                    QmlMember::Binding(binding) => binding.name.parts[0].to_string(),
                    QmlMember::Function(function) => match &function.id {
                        Some(name) => name.name.to_string(),
                        None => continue,
                    },
                    _ => continue,
                };
                self.aimed.push((id.clone(), name));
            }
        }
        for member in &object.members {
            match member {
                QmlMember::Binding(binding) => {
                    if binding.name.as_simple() == Some("id")
                        && let QmlBindingValue::Expression(Expression::Identifier(id)) =
                            &binding.value
                    {
                        let ty = self.object_type(object).unwrap_or(Ty::Unknown);
                        self.scope.ids.push((id.name.to_string(), ty));
                    }
                    self.collect_value(&binding.value);
                }
                QmlMember::Property(property) => {
                    if let Some(type_name) = &property.type_name
                        && matches!(
                            type_name.name.as_simple(),
                            Some("font" | "var" | "variant" | "alias")
                        )
                    {
                        self.fonts.push(property.name.name.to_string());
                    }
                    if let Some(value) = &property.value {
                        self.collect_value(value);
                    }
                }
                QmlMember::Object(child) if self.is_table(child) => {
                    // A list of objects, which is not looked into.
                    if let Some(id) = object_id(child) {
                        self.scope.ids.push((id.to_owned(), Ty::Plain));
                        self.listed.push((id.to_owned(), Rows::Roles));
                    }
                }
                QmlMember::Object(child) => {
                    if child.type_name.as_simple() == Some("ListModel")
                        && let Some(id) = object_id(child)
                    {
                        let rows = rows::of_roles(rows::listed_roles(child), true);
                        self.listed.push((id.to_owned(), rows));
                    }
                    self.collect(child);
                }
                QmlMember::InlineComponent(component) => {
                    self.scope
                        .inline
                        .push((component.name.name.to_string(), Root::of(&component.object)));
                    self.inline_roots.push(component.object.span);
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

    /// The type an object is made as, renaming it in the text when Ljós
    /// calls it otherwise. A grouped property (`font { }`) is no type.
    fn object_type(&mut self, object: &QmlObject<'_>) -> Result<Ty, Kept> {
        let Some(name) = object.type_name.as_simple() else {
            return Err(self.no(
                Cause::QualifiedType {
                    name: object.type_name.to_string(),
                },
                object.type_name.span.start as usize,
            ));
        };
        if name.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Ok(Ty::Plain);
        }
        let at = object.type_name.span;
        let ty = self.named_type(name, at.start as usize, 0)?;
        if let Some(new) = self.scope.renamed_type(name) {
            self.change(at.start as usize..at.end as usize, new);
        } else if name != "GridLayout"
            && matches!(ty, Ty::Exact(t) if t.module == "QtQuick.Layouts")
            && let Some(new) = rules::layout::new_name(name)
        {
            self.change(at.start as usize..at.end as usize, new);
        }
        Ok(ty)
    }
    /// The type `name` is in this file ([`Scope::ty`]).
    fn named_type(&self, name: &str, at: usize, depth: usize) -> Result<Ty, Kept> {
        self.scope.ty(name, at, depth)
    }

    /// Lowers `object`, of type `ty`, whose unqualified names are looked up
    /// on `scope` (itself, or for a group the object it is a group of) and
    /// then on the file's root, `root`. `a_root` when what uses or holds it
    /// may size it, which the file does not show: the root of the file, of
    /// an inline component or of a `Component`, or an object given as a
    /// value (a `Loader` sizes its `sourceComponent`'s).
    fn object(&mut self, object: &QmlObject<'_>, ty: Ty, root: Ty, a_root: bool) -> Step {
        match ty {
            Ty::Exact(t) | Ty::Derived(t) => {
                self.members.object_type(Some(t));
                // The lowered file's object is Ljós's, whose members are
                // in sight there.
                let ljos = rules::new_name(t.name).unwrap_or(t.name);
                if let Some(t) = [Ljos::Ui, Ljos::Core, Ljos::Layout]
                    .into_iter()
                    .find_map(|of| self.scope.modules.ljos(of)?.type_named(ljos))
                {
                    self.members.object_type(Some(t));
                }
                // A delegate's model gives it roles, which come before
                // anything else in sight.
                if t.default_property() == Some("delegate")
                    && object
                        .members
                        .iter()
                        .any(|m| matches!(m, QmlMember::Object(_)))
                {
                    self.members.delegate();
                }
            }
            // What the program's type has is in its description, not in
            // Qt's table.
            Ty::Unknown | Ty::Native(..) => self.members.object_type(None),
            Ty::Plain | Ty::Font => {}
        }
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
        for rule in rules::rules_of(ty) {
            let changes = rule.object(object, ty, a_root, self)?;
            self.changes.extend(changes);
        }
        let scope = match ty {
            Ty::Plain => root,
            other => other,
        };
        if let Some(on) = &object.on {
            self.member_name(on, scope, "a value source's target")?;
        }
        // A group's names are the object's it is a group of.
        let outer = (!matches!(ty, Ty::Plain))
            .then(|| std::mem::replace(&mut self.declared, declared(object)));
        let aim = rules::rules_of(ty).find_map(|rule| rule.aim(object, ty, self));
        let prefix = aim
            .as_ref()
            .and_then(|aim| aim.through.as_ref())
            .map(|through| {
                self.change(through.taken_out.clone(), "");
                through.id.as_str()
            });
        let own = match aim.as_ref().map(|aim| aim.aimed) {
            Some(rules::Aimed::All { own, .. }) => own,
            _ => &[],
        };
        let names = binding_names(object);
        for (index, member) in object.members.iter().enumerate() {
            match member {
                QmlMember::Object(child) if self.is_table(child) => {}
                QmlMember::Object(child)
                    if let Some(written) = rules::rules_of(ty)
                        .find_map(|rule| rule.child(object, ty, index, self)) =>
                {
                    self.child(child, written, root)?;
                }
                QmlMember::Object(child) => {
                    if prefix.is_some() {
                        return Err(self.no(
                            Cause::ThroughTarget {
                                what: format!("`{} {{ … }}`", child.type_name),
                            },
                            child.type_name.span.start as usize,
                        ));
                    }
                    let on = match &aim {
                        Some(aim) if matches!(aim.aimed, rules::Aimed::All { .. }) => aim.at,
                        _ => scope,
                    };
                    if let Some(group) = child.type_name.as_simple()
                        && group.starts_with(|c: char| c.is_ascii_lowercase())
                    {
                        let group_ty = member_ty(on, group);
                        if matches!(group_ty, Ty::Font) || group == "font" {
                            self.font_group(child, group, group_ty, scope, root, &names)?;
                            continue;
                        }
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
                    if binding.name.as_simple() == Some("delegate") {
                        self.members.delegate();
                    }
                    let made = self.changes.len();
                    self.binding(binding, ty, scope, root, aim.as_ref(), &names)?;
                    if let Some(id) = prefix {
                        self.prefix(binding, id, own, made)?;
                    }
                }
                QmlMember::Property(property) => {
                    let name = property.name.name.as_str();
                    self.members
                        .typed(property.span.start as usize..property.name.span.start as usize);
                    if self
                        .scope
                        .renames_on(scope)
                        .is_some_and(|r| r.iter().any(|(o, n)| *o == name || *n == name))
                    {
                        return Err(self.no(
                            Cause::DeclaresRenamed {
                                name: name.to_owned(),
                            },
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
                    if let Some(aim) = &aim
                        && matches!(aim.aimed, rules::Aimed::Handlers)
                        && let (Some(name), Some(span)) =
                            (name, function.id.as_ref().map(|id| id.span))
                    {
                        self.handler_name(name, span.start as usize..span.end as usize, aim.at)?;
                    }
                    self.script(scope, root, Node::Function(function))?;
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
        if let Some(outer) = outer {
            self.declared = outer;
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
        if let Some(new) = self.scope.renamed_type(name) {
            self.change(at.start as usize..at.end as usize, new);
        }
        self.named_type(name, at.start as usize, 0).map(|_| ())
    }

    /// Lowers `child` as the pass would, then makes it what a rule
    /// `written` it as.
    fn child(&mut self, child: &QmlObject<'_>, written: rules::Child, root: Ty) -> Step {
        let before = self.changes.len();
        let ty = self.object_type(child)?;
        self.object(child, ty, root, false)?;
        let made = self.changes.split_off(before);
        let copies: Vec<String> = written
            .copies
            .iter()
            .map(|range| rules::lowered_text(self.source, range, &made))
            .collect();
        self.changes.extend(made);
        // Collecting the ids renamed the child's type already.
        self.changes.retain(|c| {
            !written
                .replace
                .iter()
                .any(|(r, _)| rules::within(&c.range, r))
        });
        for (range, text) in written.replace {
            self.change(range, text);
        }
        if !copies.is_empty() {
            self.change(
                written.end..written.end,
                format!(" {} {{ {} }}", written.copied_into, copies.join("; ")),
            );
        }
        Ok(())
    }

    /// Writes `id`, the target a rule aims an object at, into a binding of
    /// it: `width: 30` is `box.width: 30`, `onClicked` `box.onClicked`.
    /// The object's `own` names stay its own. What the pass renamed of it
    /// since `made` is renamed under the id too.
    fn prefix(&mut self, binding: &QmlBinding<'_>, id: &str, own: &[&str], made: usize) -> Step {
        let name = &binding.name;
        let first = name.parts[0];
        if first == "id" || own.contains(&first) {
            return Ok(());
        }
        if first.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Err(self.no(
                Cause::ThroughTarget {
                    what: format!("`{name}`"),
                },
                name.span.start as usize,
            ));
        }
        if name.parts.len() > 1 && self.scope.id(first).is_some() {
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
        aim: Option<&rules::Aim>,
        names: &[String],
    ) -> Step {
        let name = &binding.name;
        let at = name.span.start as usize;
        let first = name.parts[0];
        // `rect.color` in a `PropertyChanges` is through an id of the
        // target's.
        let aims_all = aim.is_some_and(|aim| matches!(aim.aimed, rules::Aimed::All { .. }));
        if name.parts.len() == 1 {
            if first == "id" {
                return Ok(());
            }
            if is_grab_permissions(first) {
                return Err(self.no(Cause::GrabPermissions, at));
            }
            match aim.map(|aim| (aim.at, aim.aimed)) {
                // `PropertyChanges { target: rect; color: ... }` sets the
                // target's members.
                Some((at, rules::Aimed::All { own, what })) if !own.contains(&first) => {
                    self.member_name(name, at, what)?;
                }
                Some((at, rules::Aimed::Handlers)) if first.starts_with("on") => {
                    let span = name.span;
                    self.handler_name(first, span.start as usize..span.end as usize, at)?;
                }
                _ if matches!(ty, Ty::Plain) => {}
                _ => self.member_name(name, ty, "the object")?,
            }
        } else if first.starts_with(|c: char| c.is_ascii_uppercase()) {
            // An attached object: `Keys.onPressed`, `Component.onCompleted`,
            // and `Drag.active`, which is `DragSource.active`.
            self.named_type(first, at, 0)?;
            self.scope.attached_layout(first, at)?;
            if let Some(new) = rules::new_name(first)
                && let Some(range) = part_range(self.source, name, 0)
            {
                self.change(range, new);
            }
        } else if let Some(id) = self.scope.id(first)
            && aims_all
        {
            // `PropertyChanges { rect.color: ... }`, `{ label.font.bold: ... }`.
            self.part(name, 1, id)?;
            if name.parts.len() > 2 {
                self.font_part(binding, 2, member_ty(id, name.parts[1]), names)?;
            }
        } else {
            // A group's member: `font.pixelSize`, on the object or on the
            // target of a `PropertyChanges`.
            let on = match aim {
                Some(aim) if aims_all => aim.at,
                _ => ty,
            };
            self.font_part(binding, 1, member_ty(on, first), names)?;
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

    /// `font { pixelSize: 14; bold: true }`, a group of `font` called
    /// `name` on an object (`font` is what the pass knows of it). `outer`
    /// are the names set on that object: its `font.weight` is the group's
    /// `weight`.
    fn font_group(
        &mut self,
        group: &QmlObject<'_>,
        name: &str,
        font: Ty,
        scope: Ty,
        root: Ty,
        outer: &[String],
    ) -> Step {
        let prefix = format!("{name}.");
        let mut names = binding_names(group);
        names.extend(
            outer
                .iter()
                .filter_map(|n| n.strip_prefix(&prefix).map(str::to_owned)),
        );
        for member in &group.members {
            if let QmlMember::Binding(binding) = member {
                self.font_part(binding, 0, font, &names)?;
                self.value(&binding.value, scope, root)?;
            }
        }
        Ok(())
    }

    /// Lowers the `index`th part of `binding`'s name when it is the last
    /// and a member of a font, `group` being what the pass knows of the
    /// part before it. `names` are the bindings beside it.
    fn font_part(
        &mut self,
        binding: &QmlBinding<'_>,
        index: usize,
        group: Ty,
        names: &[String],
    ) -> Step {
        let name = &binding.name;
        if index + 1 != name.parts.len() || !font::is_member(name.parts[index]) {
            return Ok(());
        }
        let Some(range) = part_range(self.source, name, index) else {
            return Ok(());
        };
        match group {
            Ty::Font => {}
            // A group called `font` on an object whose type is not known
            // may be one; another group of a member called so is not.
            Ty::Unknown if index == 0 || name.parts[index - 1] == "font" => {
                return Err(self.no(
                    Cause::SetOnUnknownObject {
                        name: name.to_string(),
                    },
                    range.start,
                ));
            }
            _ => return Ok(()),
        }
        let beside = |member: &str| {
            let beside: Vec<&str> = name.parts[..index]
                .iter()
                .copied()
                .chain([member])
                .collect();
            names.contains(&beside.join("."))
        };
        let written = font::written(name.parts[index], &binding.value, beside)
            .map_err(|cause| self.no(cause, range.start))?;
        if let Some(new) = written.name {
            self.change(range, new);
        }
        match written.value {
            font::Value::AsWritten => {}
            font::Value::Replaced { range, with } => self.change(range, with),
            font::Value::Around {
                range,
                before,
                after,
            } => {
                // Around what the expression's own renames change, so first.
                self.change(range.start..range.start, before);
                self.change(range.end..range.end, after);
            }
        }
        Ok(())
    }

    /// Renames the simple name `name` when it is a member the type `on`
    /// has under another name in Ljós.
    fn member_name(&mut self, name: &QmlQualifiedName<'_>, on: Ty, what: &'static str) -> Step {
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
            Rename::No(cause) => return Err(self.no(cause, span.start)),
            Rename::Unknown => {
                return Err(self.no(
                    Cause::SetOnUnknown {
                        name: simple.to_owned(),
                        on: what,
                    },
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
            Rename::No(cause) => return Err(self.no(cause, span.start)),
            Rename::Unknown => {
                return Err(self.no(
                    Cause::HandlerOnUnknown {
                        handler: handler.to_owned(),
                    },
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
            Rename::No(cause) => return Err(self.no(cause, range.start)),
            Rename::Unknown => {
                return Err(self.no(
                    Cause::SetOnUnknownObject {
                        name: name.to_string(),
                    },
                    range.start,
                ));
            }
        }
        Ok(())
    }

    /// What `name`, read off something of type `on`, is called in Ljós.
    fn member(&self, name: &str, on: Ty) -> Rename {
        if let Some(rename) = rules::rules_of(on).find_map(|rule| rule.member(name)) {
            return rename;
        }
        if !is_renamed_member(name) {
            return Rename::Keep;
        }
        match on {
            Ty::Plain | Ty::Font => Rename::Keep,
            Ty::Unknown => Rename::Unknown,
            Ty::Exact(ty) | Ty::Derived(ty) => match renames_of(ty) {
                Some(renames) => match lookup_rename(renames, name) {
                    Some(new) => Rename::To(new),
                    None => Rename::Keep,
                },
                // Derived from a type that is not one that renames it, but
                // perhaps from one derived from that: an Item may be a
                // Rectangle.
                None if matches!(on, Ty::Derived(_)) && could_be_renaming(ty, name) => {
                    Rename::Unknown
                }
                None => Rename::Keep,
            },
            Ty::Native(ty, exact) => self.native_member(name, ty, exact),
        }
    }

    /// What `name`, read off something of the program's type `ty`, is
    /// called in Ljós.
    fn native_member(&self, name: &str, ty: Native, exact: bool) -> Rename {
        let Some(natives) = self.scope.natives else {
            return Rename::Unknown;
        };
        let Some(base) = self.scope.toolkit_base(ty) else {
            // Built on nothing of the toolkit's: no name of it is one Ljós
            // renames.
            return Rename::Keep;
        };
        if let Some(new) = renames_of(base).and_then(|renames| lookup_rename(renames, name)) {
            // The program's type is built on the native type, not on the
            // face Ljós's API has of it: it answers to Qt's name only.
            return Rename::No(Cause::NativeRenamed {
                name: name.to_owned(),
                native: natives.name(ty).to_string(),
                base: base.name.to_owned(),
                new: new.to_string(),
            });
        }
        if natives.has_member(ty, name) {
            return Rename::Keep;
        }
        self.member(
            name,
            if exact {
                Ty::Exact(base)
            } else {
                Ty::Derived(base)
            },
        )
    }

    fn value(&mut self, value: &QmlBindingValue<'_>, scope: Ty, root: Ty) -> Step {
        match value {
            QmlBindingValue::Expression(expression) => {
                self.script(scope, root, Node::Expression(expression))
            }
            QmlBindingValue::Statement(statement) => {
                self.script(scope, root, Node::Statement(statement))
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

    fn script(&mut self, scope: Ty, root: Ty, node: Node<'_, '_>) -> Step {
        let mut script = Script {
            pass: self,
            scope,
            root,
            kept: None,
            declared: Vec::new(),
            own: Bindings::of(node),
            row: RowEdits::default(),
            called: Vec::new(),
        };
        node.visit(&mut script);
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
                Ty::Plain | Ty::Font => Some(false),
                // The program's type has it, or the toolkit type it is
                // built on; of a file built on it, the file may too.
                Ty::Native(native, exact) => {
                    let has = self
                        .scope
                        .natives
                        .is_some_and(|n| n.has_member(native, name))
                        || self
                            .scope
                            .toolkit_base(native)
                            .is_some_and(|base| base.property(name).is_some());
                    if has || exact {
                        Some(has)
                    } else {
                        object
                            .type_name
                            .as_simple()
                            .and_then(|n| self.type_declares(n, name, 0))
                    }
                }
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
        use rules::File;
        if let Some(t) = self.qt_type(name) {
            return Some(t.property(member).is_some());
        }
        if depth > 16 {
            return None;
        }
        let root = self
            .scope
            .inline
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, root)| root.clone())
            .or_else(|| self.scope.locals.root(name))?;
        if root.properties.iter().any(|p| p == member) {
            return Some(true);
        }
        if root.type_name == name {
            return None;
        }
        self.type_declares(&root.type_name, member, depth + 1)
    }
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

impl rules::File for Pass<'_> {
    fn source(&self) -> &str {
        self.source
    }

    fn quick(&self) -> &'static qt_types::Module {
        self.scope.modules.quick()
    }

    fn ui(&self) -> &'static qt_types::Module {
        self.scope.modules.ui()
    }

    fn id(&self, name: &str) -> Option<Ty> {
        self.scope.id(name)
    }

    fn is_read(&self, name: &str) -> bool {
        self.read.contains(name)
    }

    fn is_beside(&self, name: &str) -> bool {
        self.scope.locals.root(name).is_some()
    }

    fn aimed_at(&self, id: &str, member: &str) -> bool {
        self.aimed
            .iter()
            .any(|(at, name)| at == id && name == member)
    }

    fn names_unqualified(&self, object: &QmlObject<'_>, names: &[&str]) -> bool {
        let bare = || {
            self.unqualified
                .iter()
                .filter(|(_, name)| names.contains(name))
                .map(|(at, _)| *at)
        };
        if object.span.start == self.root_at {
            return bare().next().is_some();
        }
        if self.inline_roots.contains(&object.span) {
            return bare().any(|at| (object.span.start..object.span.end).contains(&at));
        }
        // One in a child is the child's own, unless the child has none and
        // the object is a root; the pass keeps the file then too.
        object.members.iter().any(|member| {
            let span = match member {
                QmlMember::Binding(binding) => match binding.value {
                    QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => binding.span,
                    _ => return false,
                },
                QmlMember::Function(function) => function.span,
                QmlMember::Property(property) if property.value.is_some() => property.span,
                _ => return false,
            };
            bare().any(|at| (span.start..span.end).contains(&at))
        })
    }

    fn qt_type(&self, name: &str) -> Option<&'static Type> {
        self.scope.modules.qt_type(
            &[
                Qt::Quick,
                Qt::Layouts,
                Qt::Controls,
                Qt::Shapes,
                Qt::Dialogs,
            ],
            name,
        )
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
            let Some(root) = self
                .scope
                .local_root(&name, self.scope.imported_root(&name))
            else {
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
    /// The name cannot be lowered, for the reason given.
    No(Cause),
    Unknown,
}

/// A Qt type its Ljós module does not have yet.
fn not_in(name: &str, module: &'static str) -> Cause {
    Cause::NotInModule {
        name: name.to_owned(),
        module,
    }
}

/// `grabPermissions`, its change signal or its handler.
fn is_grab_permissions(name: &str) -> bool {
    matches!(
        name,
        "grabPermissions" | "grabPermissionsChanged" | "onGrabPermissionsChanged"
    )
}

/// Of the renames `on` has, the one whose new name is `name` (or its change
/// signal's), where `on`'s Qt type has no `name` of its own: once lowered,
/// `name` unqualified is that member, where in Qt it is found further out.
fn captured(on: Ty, name: &str) -> Option<(&'static Type, &'static str)> {
    let (Ty::Exact(ty) | Ty::Derived(ty)) = on else {
        return None;
    };
    let (old, _) = renames_of(ty)?
        .iter()
        .find(|(_, new)| name == *new || name.strip_suffix("Changed") == Some(*new))?;
    let own = ty.property(name).is_some() || ty.signal(name).is_some() || ty.has_method(name);
    (!own).then_some((ty, *old))
}

/// The properties, signals and functions `object` declares.
fn declared(object: &QmlObject<'_>) -> Vec<String> {
    object
        .members
        .iter()
        .filter_map(|member| match member {
            QmlMember::Property(property) => Some(property.name.name.to_string()),
            QmlMember::Signal(signal) => Some(signal.name.name.to_string()),
            QmlMember::Function(function) => function.id.as_ref().map(|id| id.name.to_string()),
            _ => None,
        })
        .collect()
}

/// Whether `on`'s Qt type has a member `name`. A type the pass does not
/// know, or one derived from a known one, may have it or not: the root is
/// asked about it then too.
fn has_own(on: Ty, name: &str) -> bool {
    let Ty::Exact(ty) = on else {
        return false;
    };
    ty.property(name).is_some() || ty.signal(name).is_some() || ty.has_method(name)
}

/// An `Item`, or a type derived from one, in Qt's table or Ljós's own.
fn is_item(ty: &'static qt_types::Type) -> bool {
    ty.is_a(&["QQuickItem", "QtQuick.Item"])
}

/// The names of `object`'s bindings, as written: `font.bold`, and a
/// group's as if written so: `font { weight: 300 }` sets `font.weight`.
fn binding_names(object: &QmlObject<'_>) -> Vec<String> {
    let mut names = Vec::new();
    for member in &object.members {
        match member {
            QmlMember::Binding(binding) => names.push(binding.name.to_string()),
            QmlMember::Object(child) => {
                if let Some(group) = child.type_name.as_simple()
                    && group.starts_with(|c: char| c.is_ascii_lowercase())
                {
                    names.extend(child.members.iter().filter_map(|member| match member {
                        QmlMember::Binding(binding) => Some(format!("{group}.{}", binding.name)),
                        _ => None,
                    }));
                }
            }
            _ => {}
        }
    }
    names
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

/// Whether `text` has `id.member` as whole names: `l.source` is not in
/// `l.sourceComponent` or `pool.source`.
fn names_member(text: &str, id: &str, member: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    let needle = format!("{id}.{member}");
    text.match_indices(&needle)
        .any(|(at, _)| !text[..at].ends_with(word) && !text[at + needle.len()..].starts_with(word))
}

/// A QML type's name: an ASCII capital, then letters, digits and `_`.
fn is_type_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Where the scripts of a file name unqualified a member a rule asks about
/// ([`rules::unqualified`]). A name a script binds itself (a parameter, a
/// local) is the script's, as [`Script`] reads it, and no object's member.
#[derive(Default)]
struct Unqualified {
    found: Vec<(u32, &'static str)>,
    /// What the script being read binds.
    own: Bindings,
}

impl Unqualified {
    fn object(&mut self, object: &QmlObject<'_>) {
        for member in &object.members {
            match member {
                QmlMember::Binding(binding) => self.value(&binding.value),
                QmlMember::Property(property) => {
                    if let Some(value) = &property.value {
                        self.value(value);
                    }
                }
                QmlMember::Function(function) => self.script(Node::Function(function)),
                QmlMember::Object(child) => self.object(child),
                QmlMember::InlineComponent(component) => self.object(&component.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn value(&mut self, value: &QmlBindingValue<'_>) {
        match value {
            QmlBindingValue::Expression(expression) => self.script(Node::Expression(expression)),
            QmlBindingValue::Statement(statement) => self.script(Node::Statement(statement)),
            QmlBindingValue::Object(object) => self.object(object),
            QmlBindingValue::Objects(objects) => objects.iter().for_each(|o| self.object(o)),
        }
    }

    fn script(&mut self, node: Node<'_, '_>) {
        self.own = Bindings::of(node);
        node.visit(self);
    }
}

impl<'a> Visit<'a> for Unqualified {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        let Some(name) = rules::unqualified().find(|name| *name == it.name.as_str()) else {
            return;
        };
        if !self.own.binds(&it.name, it.span.start) {
            self.found.push((it.span.start, name));
        }
    }
}

/// The `id` an object gives itself.
fn object_id<'a>(object: &QmlObject<'a>) -> Option<&'a str> {
    object.members.iter().find_map(|member| match member {
        QmlMember::Binding(QmlBinding {
            name,
            value: QmlBindingValue::Expression(Expression::Identifier(id)),
            ..
        }) if name.as_simple() == Some("id") => Some(id.name.as_str()),
        _ => None,
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
        Ty::Plain | Ty::Font | Ty::Unknown | Ty::Native(..) => None,
    }
}

/// Whether something of type `ty` may be of a type that renames the member
/// `name`: an `Item` may be a `Rectangle`, whose `color` is `fill`, and a
/// `QtObject` a `TapHandler`; a `Text` or a `Timer` may be neither.
fn could_be_renaming(ty: &'static Type, name: &str) -> bool {
    // One that is no further along than a renaming type's ancestors may be
    // one.
    member_renames()
        .filter(|(_, renames)| lookup_rename(renames, name).is_some())
        .any(|(renaming, _)| {
            qt_types::module("QtQuick")
                .and_then(|m| m.type_named(renaming))
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
pub(crate) fn member_extent(source: &str, start: usize, end: usize) -> Range<usize> {
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
    /// Every name it binds itself, and where each is seen.
    own: Bindings,
    row: RowEdits,
    /// Where the names that are called start: a called name is a method,
    /// and every member Ljós renames is a property (`model.remove(0)`).
    called: Vec<u32>,
}

impl Script<'_, '_> {
    fn keep(&mut self, cause: impl Into<Cause>, at: u32) {
        if self.kept.is_none() {
            self.kept = Some(Kept {
                cause: cause.into(),
                at: at as usize,
            });
        }
    }

    /// What the pass knows of what `expression` is.
    fn ty(&self, expression: &Expression<'_>) -> Ty {
        match expression {
            Expression::Identifier(id) => self.name(&id.name, id.span.start),
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

    fn name(&self, name: &str, at: u32) -> Ty {
        // A parameter or a local of the script is found before an id and
        // before any member, and what it holds is not known here.
        if self.own.declares(name, at) {
            return Ty::Unknown;
        }
        if let Some(ty) = self.pass.scope.id(name) {
            return ty;
        }
        if name.starts_with(|c: char| c.is_ascii_uppercase())
            && self
                .pass
                .scope
                .local_root(name, self.pass.scope.imported_root(name))
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
                .scope
                .modules
                .quick()
                .type_named("Item")
                .map_or(Ty::Unknown, Ty::Derived);
        }
        // Read only where no later import has a type of the name.
        if self.pass.scope.native_type(name).is_some()
            && let Ok(Ty::Native(ty, _)) = self.pass.named_type(name, 0, 0)
            && self.pass.scope.natives.is_some_and(|n| n.is_singleton(ty))
        {
            return Ty::Native(ty, true);
        }
        match self.member_ty(self.scope, name) {
            Ty::Unknown => self.member_ty(self.root, name),
            known => known,
        }
    }

    fn member_ty(&self, on: Ty, name: &str) -> Ty {
        if let Ty::Native(ty, _) = on {
            return self.native_member_ty(ty, name);
        }
        member_ty(on, name)
    }

    /// Whether `expression`, whose type the pass does not know, may be a
    /// font: a name or a member called `font`, or one the file declares as a
    /// font, a `var` or an `alias`. What a function is given or a local holds is not
    /// followed (docs/DIALECT.md §4.13).
    fn could_be_font(&self, expression: &Expression<'_>) -> bool {
        match expression {
            // A parameter or a local of the name is the script's, which
            // is not followed.
            Expression::Identifier(id) if self.own.binds(&id.name, id.span.start) => false,
            Expression::Identifier(id) => {
                id.name == "font" || self.pass.fonts.iter().any(|f| f == id.name.as_str())
            }
            Expression::StaticMemberExpression(member) => {
                let name = member.property.name.as_str();
                name == "font" || self.pass.fonts.iter().any(|f| f == name)
            }
            _ => false,
        }
    }
}

/// The type of the member `name` of something of type `on`.
fn member_ty(on: Ty, name: &str) -> Ty {
    let (Ty::Exact(ty) | Ty::Derived(ty)) = on else {
        return Ty::Unknown;
    };
    let Some(property) = ty.property(name) else {
        return Ty::Unknown;
    };
    match property.value_type() {
        Some(value) if value.is_value && value.name == "font" => Ty::Font,
        Some(value) if value.is_value => Ty::Plain,
        Some(value) => Ty::Derived(value),
        None => Ty::Plain,
    }
}

impl Script<'_, '_> {
    /// What `model` is here.
    fn model(&self) -> Model {
        if self.pass.scope.id("model").is_some() {
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
        self.pass.scope.id(name).is_none()
            && self.pass.scope_owns.get(name) == Some(false)
            && behind.get(name) == Some(false)
    }

    fn row_keep(&mut self, cause: Row, at: u32) {
        if self.row.kept.is_none() {
            self.row.kept = Some(Kept {
                cause: cause.into(),
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
                Row::Taken {
                    what: what.to_owned(),
                    name,
                },
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
            (_, Some(Rows::OneRole(_))) => self.row_keep(Row::MissingRole { what }, at),
            (_, Some(Rows::Objects)) => self.row_keep(
                Row::RoleOfArray {
                    what,
                    member: member.to_owned(),
                },
                at,
            ),
            (_, Some(Rows::Values)) => self.row_keep(Row::RoleOfValues { what }, at),
            (_, Some(Rows::Unknown)) => self.row_keep(
                Row::RoleOfUnshown {
                    what,
                    member: member.to_owned(),
                },
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
    fn unshown(&self, what: &str, part: &'static str, declare: &str) -> Row {
        Row::Unshown {
            what: what.to_owned(),
            part,
            declare: declare.to_owned(),
            in_component: self.pass.loose,
        }
    }

    /// The script is read: what it wrote of the row is the file's, unless
    /// a local it declares has a name of the row.
    fn row_done(&mut self) {
        let row = std::mem::take(&mut self.row);
        if let Some(at) = row.read
            && self.declared.iter().any(|d| d == "model")
        {
            self.keep(Row::LocalModel, at);
            return;
        }
        for name in &row.wrote {
            if self.declared.iter().any(|d| d == name) {
                let at = row.changes.first().map_or(0, |c| c.range.start as u32);
                self.keep(
                    Row::LocalRow {
                        name: (*name).to_owned(),
                    },
                    at,
                );
                return;
            }
        }
        if let Some(kept) = row.kept {
            self.keep(kept.cause, kept.at as u32);
        }
        self.pass.changes.extend(row.changes);
    }

    /// The type of the member `name` of something of the program's type
    /// `ty`: its toolkit base's, as Qt types it, else what the description
    /// says it holds.
    fn native_member_ty(&self, ty: Native, name: &str) -> Ty {
        let Some(natives) = self.pass.scope.natives else {
            return Ty::Unknown;
        };
        if let Some(base) = self.pass.scope.toolkit_base(ty)
            && base.property(name).is_some()
        {
            return self.member_ty(Ty::Derived(base), name);
        }
        match natives.property(ty, name) {
            Some(Holds::Native(held)) => Ty::Native(held, false),
            Some(Holds::Toolkit { module, name }) => qt_types::module(&module)
                .and_then(|m| m.type_named(&name))
                .map_or(Ty::Unknown, |held| {
                    if held.is_value {
                        Ty::Plain
                    } else {
                        Ty::Derived(held)
                    }
                }),
            Some(Holds::Value) => Ty::Plain,
            None => Ty::Unknown,
        }
    }
}

impl<'a> Visit<'a> for Script<'_, '_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        // Qt 6 resolves `Qt.resolvedUrl(u, ctx)` against the file of ctx's
        // component, and `url` takes one argument. So `Qt.resolvedUrl` is
        // `url` only as the callee of a call given one plain argument; a
        // spread, `.call`, `.apply` or the bare function may carry a context,
        // so every other use stays Qt's.
        if let Expression::StaticMemberExpression(callee) = &it.callee
            && let Expression::Identifier(qt) = &callee.object
            && qt.name == "Qt"
            && !self.own.binds("Qt", qt.span.start)
            && callee.property.name == "resolvedUrl"
            && let [argument] = it.arguments.as_slice()
            && !argument.is_spread()
        {
            let span = |s: qml_syntax::Span| s.start as usize..s.end as usize;
            self.pass
                .members
                .qt(span(qt.span), "resolvedUrl", span(callee.property.span));
        }
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
                    let cause = Row::ModelMemberUnknown {
                        member: it.property.name.to_string(),
                    };
                    self.row_keep(cause, it.span.start);
                    return;
                }
                Model::Row(rows) => {
                    self.row_member(it, rows);
                    return;
                }
            }
        }
        let name = it.property.name.as_str();
        // A parameter or a local called `Qt` is no global: its members stay.
        if let Expression::Identifier(qt) = &it.object
            && qt.name == "Qt"
            && !self.own.binds("Qt", qt.span.start)
            // Lowered only where it is called; see visit_call_expression.
            && name != "resolvedUrl"
        {
            let span = |s: qml_syntax::Span| s.start as usize..s.end as usize;
            self.pass
                .members
                .qt(span(qt.span), name, span(it.property.span));
        }
        if name == "createQmlObject" {
            // The text is compiled when the script runs, as written.
            self.keep(Cause::QmlText, it.property.span.start);
        }
        if is_grab_permissions(name) {
            self.keep(Cause::GrabPermissions, it.property.span.start);
        }
        // A key of the grab permissions read through a handler's name.
        if let Expression::Identifier(ty) = &it.object
            && let Some(ty) = self.pass.scope.modules.quick().type_named(&ty.name)
            && let Some(key) = ty.enum_value(name)
            && (key.enumeration.name == "GrabPermissions"
                || key.enumeration.alias == Some("GrabPermissions"))
        {
            self.keep(Cause::GrabPermissions, it.property.span.start);
        }
        // An attached object read off an item: `item.Drag.active`. Of
        // anything else the name is a key of its own (`config.Drag`), and a
        // type that attaches nothing is never read off an object.
        if name.starts_with(|c: char| c.is_ascii_uppercase())
            && let Some(new) = rules::new_name(name)
            && self
                .pass
                .scope
                .modules
                .quick()
                .type_named(name)
                .is_some_and(|ty| ty.attached().is_some())
        {
            let on = match self.ty(&it.object) {
                // The program's type attaches what the toolkit type it is
                // built on does.
                Ty::Native(native, _) => match self.pass.scope.toolkit_base(native) {
                    Some(base) => Ty::Derived(base),
                    None => Ty::Native(native, false),
                },
                on => on,
            };
            match on {
                Ty::Exact(ty) | Ty::Derived(ty) if is_item(ty) => {
                    let span = it.property.span;
                    self.pass
                        .change(span.start as usize..span.end as usize, new);
                }
                Ty::Exact(ty) | Ty::Derived(ty) => self.keep(
                    Cause::AttachedOffNoItem {
                        name: name.to_owned(),
                        on: ty.name.to_owned(),
                    },
                    it.property.span.start,
                ),
                Ty::Plain | Ty::Font => {}
                Ty::Native(..) => self.keep(
                    Cause::AttachedOffNative {
                        name: name.to_owned(),
                    },
                    it.property.span.start,
                ),
                Ty::Unknown => self.keep(
                    Cause::AttachedOffUnknown {
                        name: name.to_owned(),
                    },
                    it.property.span.start,
                ),
            }
        }
        if (is_renamed_member(name) || font::is_member(name))
            && !self.called.contains(&it.property.span.start)
        {
            let on = self.ty(&it.object);
            let read = font::read(name, on, || self.could_be_font(&it.object))
                .unwrap_or_else(|| self.pass.member(name, on));
            match read {
                Rename::To(new) => {
                    let span = it.property.span;
                    self.pass
                        .change(span.start as usize..span.end as usize, new);
                }
                Rename::Keep => {}
                Rename::No(cause) => self.keep(cause, it.property.span.start),
                Rename::Unknown => self.keep(
                    Cause::ReadOffUnknown {
                        name: name.to_owned(),
                    },
                    it.property.span.start,
                ),
            }
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if let Expression::StringLiteral(key) = &it.expression
            && (is_renamed_member(&key.value) || font::is_member(&key.value))
        {
            self.keep(
                Cause::ComputedRenamed {
                    key: key.value.to_string(),
                },
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
                    self.row_keep(Row::ModelMaybeMember, it.span.start);
                }
                Model::Row(Some(Rows::Roles)) => {
                    self.row.read.get_or_insert(it.span.start);
                    self.row_write(it.span, "modelData", "model");
                }
                Model::Row(_) => {
                    self.row.read.get_or_insert(it.span.start);
                    self.row_keep(Row::ModelRolesUnshown, it.span.start);
                }
            }
            return;
        }
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            if self.pass.scope.qualifiers.iter().any(|q| q == name)
                || self.own.binds(name, it.span.start)
            {
                // The file's own, or the script's: it means in the lowered
                // file what it means in Qt's, whatever type has its name.
                return;
            }
            if let Some(new) = self.pass.scope.renamed_type(name) {
                self.pass
                    .change(it.span.start as usize..it.span.end as usize, new);
            // QtQuick.Layouts' names are looked at below: a `GridLayout`'s
            // enumerations, and the objects its types attach.
            } else if self
                .pass
                .scope
                .modules
                .qt_type(&[Qt::Quick, Qt::Controls, Qt::Dialogs, Qt::Shapes], name)
                .is_some()
                && let Err(kept) = self.pass.named_type(name, it.span.start as usize, 0)
            {
                self.keep(kept.cause, it.span.start);
            } else if name == "GridLayout" {
                match self.pass.scope.of_layouts(name, it.span.start as usize) {
                    // Its enumerations: `GridLayout.TopToBottom` is `Grid`'s.
                    Ok(true) => {
                        if let Some(new) = rules::layout::new_name(name) {
                            self.pass
                                .change(it.span.start as usize..it.span.end as usize, new);
                        }
                    }
                    Ok(false) => {}
                    Err(kept) => self.keep(kept.cause, it.span.start),
                }
            } else if let Err(kept) = self
                .pass
                .scope
                .attached_layout(name, it.span.start as usize)
            {
                self.keep(kept.cause, it.span.start);
            } else if self.pass.scope.ljos_module_with(name).is_some()
                && let Err(kept) = self.pass.named_type(name, it.span.start as usize, 0)
            {
                // A name the Qt imports lack, read in a script, is the
                // project's or the context's; lowered, it would be Ljós's.
                self.keep(kept.cause, it.span.start);
            }
            return;
        }
        if is_grab_permissions(name) {
            self.keep(Cause::GrabPermissions, it.span.start);
            return;
        }
        // A parameter or a local is found before an id and the scope's
        // members.
        if self.own.declares(name, it.span.start) {
            return;
        }
        // An id is found before the scope's members, in Qt as once lowered.
        if self.pass.scope.id(name).is_some() {
            return;
        }
        // Not the scope's own, the name is looked up on the root next, in
        // Qt as once lowered.
        let captor = captured(self.scope, name).or_else(|| {
            let own = has_own(self.scope, name) || self.pass.declared.iter().any(|d| d == name);
            (!own).then(|| captured(self.root, name))?
        });
        if let Some((ty, old)) = captor {
            let new = rules::new_name(ty.name).unwrap_or(ty.name);
            self.keep(
                Cause::OwnOnceLowered {
                    name: name.to_owned(),
                    new: new.to_owned(),
                    old: old.to_string(),
                },
                it.span.start,
            );
            return;
        }
        // A called name is a method, which no type renames as a property.
        if !is_renamed_member(name) || self.called.contains(&it.span.start) {
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
            Rename::No(cause) => self.keep(cause, it.span.start),
            Rename::Unknown => self.keep(
                Cause::MaybeMember {
                    name: name.to_owned(),
                },
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
                Cause::LocalCalledRenamed {
                    name: it.name.to_string(),
                },
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
