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
mod group;
mod rows;

use std::ops::Range;

use oxc_ast::ast::{
    BindingIdentifier, ComputedMemberExpression, Expression, IdentifierReference,
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
        QmlQualifiedName,
    },
};
use qt_types::Type;
use rows::{Delegate, Owns, Rows};

pub use group::{Alone, References, group, references};

/// The module a lowered file imports.
pub const MODULE: &str = "Ljos.Ui";

/// The module of Ljós's models, which a lowered file imports when it makes
/// one of [`MODELS`].
pub const MODELS_MODULE: &str = "Ljos.Models";

/// The types of `QtQml.Models` that are `Ljos.Models`' as they are.
pub const MODELS: &[&str] = &["ListModel", "ListElement", "Instantiator"];

/// What a lowered file says of its scripts: they are Qt's JavaScript, which
/// the checker of Ljós's API does not hold to its rules. A file is typed by
/// taking the pragma out, and `ljos check` then says what to change
/// (docs/DIALECT.md §8).
pub const JAVASCRIPT: &str = "pragma Scripts: JavaScript";

/// The Qt types Ljós calls by another name. Every other type of the slice is
/// called as Qt calls it.
pub const TYPE_RENAMES: &[(&str, &str)] = &[("MouseArea", "PointerArea")];

/// Per Qt type (and what derives from it), the members Ljós calls by another
/// name. A property's change signal and its handler follow its name.
pub const MEMBER_RENAMES: &[(&str, &[(&str, &str)])] = &[("Rectangle", &[("color", "fill")])];

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

/// What the pass needs to know of the files beside this one: for a type a
/// file of the project is (`SimpleButton` for `SimpleButton.qml`), its root
/// object.
pub trait Locals {
    fn root(&self, name: &str) -> Option<Root>;
}

impl<F: Fn(&str) -> Option<Root>> Locals for F {
    fn root(&self, name: &str) -> Option<Root> {
        self(name)
    }
}

/// No files beside this one.
pub fn no_locals(_: &str) -> Option<Root> {
    None
}

/// `source` in Ljós's API, or why it stays as it is.
pub fn lower(source: &str, locals: &dyn Locals) -> Outcome {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        // The compiler says what is wrong with the file as written.
        return kept("it does not parse", 0);
    }
    let Some(ui) = qt_types::module(MODULE) else {
        return kept("this build has no description of Ljos.Ui", 0);
    };
    let Some(quick) = qt_types::module("QtQuick") else {
        return kept("this build has no description of QtQuick", 0);
    };
    let mut pass = Pass {
        source,
        ui,
        quick,
        layouts: None,
        locals,
        ids: Vec::new(),
        inline: Vec::new(),
        changes: Vec::new(),
        listed: Vec::new(),
        imports_models: false,
        makes_models: std::cell::Cell::new(false),
        given: None,
        view_rows: None,
        delegate: None,
        scope_owns: Owns::NONE,
        root_owns: Owns::NONE,
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
            Outcome::Lowered(Lowered { text, changes })
        }
        Err(kept) => Outcome::Kept(kept),
    }
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
    quick: &'static qt_types::Module,
    /// QtQuick.Layouts, when the file imports it.
    layouts: Option<&'static qt_types::Module>,
    locals: &'s dyn Locals,
    ids: Vec<(String, Ty)>,
    inline: Vec<(String, Root)>,
    changes: Vec<Change>,
    /// The `ListModel`s with ids, and the roles their `ListElement`s give.
    listed: Vec<(String, Vec<String>)>,
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
    /// What the object whose members an unqualified name is first has.
    scope_owns: Owns,
    /// What the root of the file, or of the inline component, has.
    root_owns: Owns,
}

type Step = Result<(), Kept>;

impl<'s> Pass<'s> {
    /// Whether the file imports `QtQuick` and so has something to lower.
    fn document(&mut self, document: &QmlDocument<'_>) -> Result<bool, Kept> {
        let mut quick = None;
        let mut layouts = None;
        for import in &document.imports {
            let at = import.span.start as usize;
            let QmlImportSource::Module(name) = &import.source else {
                continue;
            };
            let uri = name.to_string();
            if import.alias.is_some()
                && matches!(uri.as_str(), "QtQuick" | "QtQml" | "QtQml.Models")
            {
                return Err(self.no(format!("it imports `{uri}` under a name"), at));
            }
            match uri.as_str() {
                "QtQuick" => quick = Some(import.span),
                "QtQml" => {}
                // Its models are `Ljos.Models`'; its other types are found
                // through `Ljos.Ui`'s import of `QtQml`, as they were.
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
                }
                // A module that builds on QtQuick names its types, and those
                // are Qt's until Ljós has its own (docs/DIALECT.md).
                other => {
                    return Err(self.no(
                        format!("it imports `{other}`, which is not lowered yet"),
                        at,
                    ));
                }
            }
        }
        let Some(span) = quick else {
            return Ok(false);
        };
        if let Some(span) = layouts {
            // `Ljos.Ui` has `Layout`; the line stays, empty.
            self.change(span.start as usize..span.end as usize, "");
        }
        self.collect(&document.root);
        let root = self.object_type(&document.root)?;
        self.root_owns = self.owns(&document.root, root);
        self.object(&document.root, root, root, true)?;
        // On the import's line, so every line stays where it was.
        let models = if self.makes_models.get() && !self.imports_models {
            format!("; import {MODELS_MODULE}")
        } else {
            String::new()
        };
        self.change(
            span.start as usize..span.end as usize,
            format!("{JAVASCRIPT}; import {MODULE}{models}"),
        );
        Ok(true)
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
                QmlMember::Object(child) => {
                    if child.type_name.as_simple() == Some("ListModel")
                        && let Some(id) = object_id(child)
                    {
                        self.listed.push((id, rows::listed_roles(child)));
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
            QmlBindingValue::Object(object) => self.collect(object),
            QmlBindingValue::Objects(objects) => objects.iter().for_each(|o| self.collect(o)),
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
        }
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
        if let Some(ty) = self.layouts.and_then(|m| m.type_named(name)) {
            return match name {
                "RowLayout" | "ColumnLayout" | "Layout" => Ok(Ty::Exact(ty)),
                _ => Err(self.no(format!("`{name}` is not in {MODULE} yet"), at)),
            };
        }
        if let Some(ty) = self.quick.type_named(name) {
            if ty.module.starts_with("QtQml") {
                if MODELS.contains(&name) {
                    self.makes_models.set(true);
                }
                return Ok(Ty::Exact(ty));
            }
            let new = renamed_type(name).unwrap_or(name);
            return match self.ui.type_named(new) {
                Some(_) => Ok(Ty::Exact(ty)),
                None => Err(self.no(format!("`{name}` is not in {MODULE} yet"), at)),
            };
        }
        if depth > 16 {
            return Ok(Ty::Unknown);
        }
        let root = self
            .inline
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, root)| root.type_name.clone())
            .or_else(|| self.locals.root(name).map(|r| r.type_name));
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

    /// Lowers `object`, of type `ty`, whose unqualified names are looked up
    /// on `scope` (itself, or for a group the object it is a group of) and
    /// then on the file's root, `root`. `a_root` when what uses or holds it
    /// may size it, which the file does not show: the root of the file, of
    /// an inline component or of a `Component`, or an object given as a
    /// value (a `Loader` sizes its `sourceComponent`'s).
    fn object(&mut self, object: &QmlObject<'_>, ty: Ty, root: Ty, a_root: bool) -> Step {
        let given = self.given.take();
        let saved = (
            self.delegate.clone(),
            self.scope_owns,
            self.view_rows.clone(),
            self.root_owns,
        );
        let done = self.object_in(object, ty, root, a_root, given);
        (
            self.delegate,
            self.scope_owns,
            self.view_rows,
            self.root_owns,
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
            self.view_rows = Some(rows::of_view(object, &self.listed));
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
        for member in &object.members {
            match member {
                QmlMember::Object(child) => {
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
                QmlMember::Binding(binding) => self.binding(binding, ty, scope, root, target)?,
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
                    if let Some(type_name) = &property.type_name
                        && let Some(simple) = type_name.name.as_simple()
                        && matches!(simple, "var" | "variant")
                    {
                        let span = type_name.name.span;
                        self.change(span.start as usize..span.end as usize, "unknown");
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
                    self.root_owns = self.owns(&component.object, ty);
                    self.object(&component.object, ty, ty, true)?;
                    (self.delegate, self.root_owns) = (delegate, root_owns);
                }
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
        Ok(())
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
            let root = self
                .inline
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, root)| root.clone())
                .or_else(|| self.locals.root(&name));
            let Some(root) = root else { break };
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
    TYPE_RENAMES
        .iter()
        .find(|(old, _)| *old == name)
        .map(|(_, new)| *new)
}

/// A member name, with its change signal, that some type renames.
fn is_renamed_member(name: &str) -> bool {
    MEMBER_RENAMES
        .iter()
        .any(|(_, renames)| lookup_rename(renames, name).is_some())
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
        if let Some((_, renames)) = MEMBER_RENAMES.iter().find(|(name, _)| *name == ty.name) {
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
        && MEMBER_RENAMES.iter().any(|(name, _)| {
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
        format!(
            "`{what}` reads {part} of a view the file is not shown in; give the root `required property {declare}`"
        )
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
        if is_renamed_member(name) {
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
            } else if self.pass.quick.type_named(name).is_some()
                && let Err(kept) = self.pass.named_type(name, it.span.start as usize, 0)
            {
                self.keep(kept.reason, it.span.start);
            }
            return;
        }
        if !is_renamed_member(name) || self.pass.id(name).is_some() {
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
        // would rename the local's.
        if is_renamed_member(&it.name) {
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
