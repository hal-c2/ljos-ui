//! The types and members a project's QML uses, as the table of types names
//! them: what `ljos check --port` weighs against what Ljós has.
//!
//! The walk is the checker's (`check.rs`): the same loader, the same
//! `qml_front` tree, the same reading of groups, handlers and attached
//! properties. What differs is what it keeps. A name that resolves to a type
//! of a module Qt or Ljós provides is a *use* of `Module.Type` (the module
//! being the one the file imported it from) or of `Module.Type.member`. A
//! type or member the project provides itself (a QML file, a module of its
//! own `qmldir`, a property a component declares) is not a use of anything
//! but what it is built on: `MyButton { checked: true }` uses
//! `Button.checked`.
//!
//! Members are attributed to the type the object is (or, for a component, the
//! builtin type its chain ends in), not to the base that declares them, so a
//! row says what a file wrote. The table lookups that decide whether Ljós has
//! one walk the base chain.
//!
//! Methods and property reads in scripts are found only as `id.name` and
//! `id.name(...)` where `id` is an object of the file: the front end does not
//! infer the type of any other expression, and a guess would only add noise.

use std::{collections::HashSet, path::Path};

use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use qml_front::{
    QtType, QtTypeOracle, TypeOracle,
    project::{Project, Source, summarize},
    scope::{Tree, handled, is_group},
    types::{Kind, Lookup, Types},
};
use qml_syntax::{
    Allocator, GetSpan,
    ast::{QmlBindingValue, QmlMember, QmlObject},
    js::{CallExpression, Expression, StaticMemberExpression},
};

use super::{
    check::{Shape, shape},
    last_part,
    load::Loader,
};
use crate::{MemberKind, api::qt_kind};

/// Whether the front end could place a use in a table of types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// The type or member is one the tables describe.
    Resolved,
    /// Nothing the file imports provides it; the text says what is known.
    Unresolved(String),
}

/// One place a file uses a type of a module, or a member of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiUse {
    /// The file, from the project's root, as written on disk (`ui/Main.qml`).
    pub file: String,
    /// The module the type was imported from. For an unresolved type: the one
    /// module the file imports that nothing describes, when there is exactly
    /// one (the likeliest to have it), else empty.
    pub module: String,
    /// The type as the file writes it (`Rectangle`, `C.Button`).
    pub type_name: String,
    /// None for a use of the type itself. A path for a member of a value
    /// (`font.pixelSize`), a handler by its name (`onClicked`).
    pub member: Option<String>,
    pub kind: MemberKind,
    pub resolution: Resolution,
}

/// Everything a project's QML uses.
#[derive(Clone, Debug, Default)]
pub struct Usage {
    /// Files read and analysed.
    pub files: usize,
    /// Files that did not parse whole: what they use is not in `uses`.
    pub unparsed: Vec<String>,
    pub uses: Vec<ApiUse>,
}

/// Directories are walked without following links, and without the places
/// tools keep their output.
fn qml_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.file_type() {
                Ok(t) if t.is_dir() && !crate::tools::skipped_dir(&name) => {
                    stack.push(entry.path())
                }
                Ok(t) if t.is_file() && name.ends_with(".qml") => found.push(entry.path()),
                _ => {}
            }
        }
    }
    found.sort();
    found
}

pub(super) fn collect(root: &Path) -> Usage {
    let mut loader = Loader::new(root);
    let mut usage = Usage::default();
    let mut parsed_ok: Vec<(String, String)> = Vec::new();
    for path in qml_files(root) {
        let Some(key) = loader.key(&path) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            loader.insert(&key, None);
            usage.unparsed.push(format!("{key}.qml"));
            continue;
        };
        let allocator = Allocator::default();
        let parsed = qml_syntax::parse(&allocator, &text);
        if parsed.is_ok() {
            loader.insert(&key, Some(summarize(&parsed.document)));
            parsed_ok.push((key, text));
        } else {
            // What did parse still tells others what this file's type has.
            loader.insert_partial(&key, &text);
            usage.unparsed.push(format!("{key}.qml"));
        }
    }
    loader.settle();
    loader.discover_modules();
    loader.settle();

    usage.files = parsed_ok.len();
    for (key, text) in &parsed_ok {
        file(&loader, key, text, &mut usage.uses);
    }
    usage
}

fn file(loader: &Loader<'_>, key: &str, source: &str, out: &mut Vec<ApiUse>) {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        return;
    }
    let oracle = QtTypeOracle;
    let types = Types::new(&loader.project, &oracle, key);
    let mut errors = Vec::new();
    let tree = Tree::analyze(&parsed.document, types, &mut errors);

    let name = format!("{key}.qml");
    let mut walk = Walk {
        loader,
        tree: &tree,
        types,
        file: &name,
        bases: Vec::new(),
        out,
    };
    for object in &tree.objects {
        let base = walk.base_of(object);
        walk.bases.push(base);
    }
    for (index, object) in tree.objects.iter().enumerate() {
        walk.type_use(index, &object.name, object.kind.as_ref());
    }
    walk.child(&parsed.document.root);
}

/// What an object is as far as its members go: the builtin type its chain of
/// components ends in, with the module and name a file writes it by.
struct Base {
    module: String,
    name: String,
    ty: QtType,
}

struct Walk<'w, 'a> {
    loader: &'w Loader<'w>,
    tree: &'w Tree<'a, QtType>,
    types: Types<'w, QtTypeOracle>,
    file: &'w str,
    /// One per object of the tree. None where the members cannot be told: the
    /// type is unknown, its body is read by the type itself (`Connections`),
    /// or its chain is not known in full.
    bases: Vec<Option<Base>>,
    out: &'w mut Vec<ApiUse>,
}

/// The builtin type `parts` ends in, however many components down, with the
/// module and name it is written by where it stops being a component.
fn resolve(
    project: &Project,
    oracle: &QtTypeOracle,
    file: &str,
    parts: &[&str],
    depth: usize,
) -> Option<(QtType, String, String)> {
    if depth > 64 {
        return None;
    }
    let types = Types::new(project, oracle, file);
    let found = types.find(parts)?;
    match found.kind {
        Kind::Builtin(ty) => Some((
            ty,
            module_of(&types, parts, ty),
            (*parts.last()?).to_owned(),
        )),
        Kind::Component(key) => {
            let shape = project.shape(&key)?;
            let root: Vec<&str> = shape.root.iter().map(String::as_str).collect();
            resolve(project, oracle, &key.file, &root, depth + 1)
        }
    }
}

/// The module `ty` is imported from where the file writes `parts`: the import
/// that gives the name, not the module the table happens to file the type in
/// (Controls' `Button` is described under `QtQuick.Controls.Basic`).
fn module_of(types: &Types<'_, QtTypeOracle>, parts: &[&str], ty: QtType) -> String {
    let (qualifier, name) = match parts {
        [name] => (None, *name),
        [qualifier, name] => (Some(*qualifier), *name),
        _ => return ty.0.module.to_owned(),
    };
    if let Some(summary) = types.project.summary(types.file) {
        for import in summary.imports.iter().rev() {
            if import.alias.as_deref() != qualifier {
                continue;
            }
            if let Source::Module(uri) = &import.source
                && types.oracle.module_type(uri, name) == Some(ty)
            {
                return uri.clone();
            }
        }
    }
    ty.0.module.to_owned()
}

/// What sits between `a.b` and a member: `id.name` accessed in a script, and
/// which of those are called.
#[derive(Default)]
struct Accesses<'a> {
    members: Vec<(&'a str, &'a str, u32)>,
    calls: HashSet<u32>,
}

impl<'a> Visit<'a> for Accesses<'a> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if matches!(it.callee, Expression::StaticMemberExpression(_)) {
            self.calls.insert(it.callee.span().start);
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            self.members.push((
                object.name.as_str(),
                it.property.name.as_str(),
                it.span.start,
            ));
        }
        walk::walk_static_member_expression(self, it);
    }
}

impl<'a> Walk<'_, 'a> {
    fn base_of(&self, object: &qml_front::scope::Object<'a, QtType>) -> Option<Base> {
        let kind = object.kind.as_ref()?;
        // Complete: every member of the chain is known, and the body is the
        // type's own to read (`Connections` and `ListElement` are not).
        let Shape::Complete(ty) = shape(&self.types, Some(kind)) else {
            return None;
        };
        // A QML file that shares its name with a type some C++ registers may
        // not be the type that is meant.
        if matches!(kind, Kind::Component(_)) && self.loader.native(last_part(&object.name)) {
            return None;
        }
        let parts: Vec<&str> = object.name.split('.').collect();
        let (found, module, name) = resolve(
            &self.loader.project,
            &QtTypeOracle,
            self.types.file,
            &parts,
            0,
        )?;
        debug_assert!(found == ty || matches!(kind, Kind::Component(_)));
        Some(Base {
            module,
            name,
            ty: found,
        })
    }

    fn push(
        &mut self,
        module: &str,
        type_name: &str,
        member: Option<String>,
        kind: MemberKind,
        resolution: Resolution,
    ) {
        self.out.push(ApiUse {
            file: self.file.to_owned(),
            module: module.to_owned(),
            type_name: type_name.to_owned(),
            member,
            kind,
            resolution,
        });
    }

    /// What a name no table has is, by what the build can tell: with Qt's
    /// table it is nothing either has, without it a name Qt might have and
    /// Ljós has not.
    fn absent() -> Resolution {
        if qt_types::has_qt_table() {
            Resolution::Unresolved("not a member of the type in any table of types".to_owned())
        } else {
            Resolution::Resolved
        }
    }

    fn type_use(&mut self, index: usize, name: &str, kind: Option<&Kind<QtType>>) {
        let _ = index;
        let parts: Vec<&str> = name.split('.').collect();
        match kind {
            Some(Kind::Component(_)) => {}
            Some(Kind::Builtin(ty)) => {
                let module = module_of(&self.types, &parts, *ty);
                let type_name = last_part(name).to_owned();
                self.push(
                    &module,
                    &type_name,
                    None,
                    MemberKind::Type,
                    Resolution::Resolved,
                );
            }
            None => self.unresolved_type(name),
        }
    }

    fn unresolved_type(&mut self, name: &str) {
        let last = last_part(name);
        // The file exists and did not parse, or some C++ registers the type.
        if self.loader.broken.contains(last) || self.loader.native(last) {
            return;
        }
        let qualifier = name.split_once('.').map(|(q, _)| q);
        let mut hints = Vec::new();
        if let Some(summary) = self.loader.project.summary(self.types.file) {
            for import in &summary.imports {
                if let Source::Module(uri) = &import.source
                    && import.alias.as_deref() == qualifier
                    && !self.types.oracle.has_module(uri)
                    && !self.loader.project.has_module(uri)
                {
                    hints.push(uri.clone());
                }
            }
        }
        let module = if hints.len() == 1 {
            hints.remove(0)
        } else {
            String::new()
        };
        let why = if module.is_empty() {
            "not provided by any module the file imports".to_owned()
        } else {
            format!(
                "not provided by any module the file imports; `{module}` is described by no table"
            )
        };
        self.push(
            &module,
            name,
            None,
            MemberKind::Type,
            Resolution::Unresolved(why),
        );
    }

    fn child(&mut self, object: &QmlObject<'a>) {
        if let Some(index) = self.tree.index_of(object) {
            self.members(object, index, &[]);
        }
    }

    /// `group` is the names that lead from the owner to `object` when it is a
    /// group (`font { }`).
    fn members(&mut self, object: &QmlObject<'a>, owner: usize, group: &[&'a str]) {
        for member in &object.members {
            match member {
                QmlMember::Object(child) if is_group(child) => {
                    let mut path = group.to_vec();
                    path.extend(child.type_name.parts.iter().copied());
                    self.members(child, owner, &path);
                }
                QmlMember::Object(child) => self.child(child),
                QmlMember::Binding(binding) => {
                    let mut path = group.to_vec();
                    path.extend(binding.name.parts.iter().copied());
                    self.binding(owner, &path);
                    self.value(&binding.value, owner);
                }
                QmlMember::Property(declaration) => {
                    if let Some(value) = &declaration.value {
                        self.value(value, owner);
                    }
                }
                QmlMember::Function(function) => {
                    let mut accesses = Accesses::default();
                    accesses.visit_function(function, ScopeFlags::Function);
                    self.script(owner, &accesses);
                }
                QmlMember::InlineComponent(inline) => self.child(&inline.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn value(&mut self, value: &QmlBindingValue<'a>, owner: usize) {
        let mut accesses = Accesses::default();
        match value {
            QmlBindingValue::Expression(e) => accesses.visit_expression(e),
            QmlBindingValue::Statement(s) => accesses.visit_statement(s),
            QmlBindingValue::Object(o) => return self.child(o),
            QmlBindingValue::Objects(os) => {
                for o in os {
                    self.child(o);
                }
                return;
            }
        }
        self.script(owner, &accesses);
    }

    /// `id.name` and `id.name()` where `id` is an object of the file and the
    /// table knows the name on its type.
    fn script(&mut self, owner: usize, accesses: &Accesses<'a>) {
        for (id, name, offset) in &accesses.members {
            let mut context = Some(self.tree.objects[owner].context);
            let mut target = None;
            while let Some(at) = context {
                target = target.or_else(|| self.tree.id_in(at, id));
                context = self.tree.contexts[at].outer;
            }
            let Some(target) = target else { continue };
            let Some(base) = &self.bases[target] else {
                continue;
            };
            if self.tree.objects[target].declared.contains_key(name) {
                continue;
            }
            let Some(mut kind) = qt_kind(base.ty.0, name) else {
                continue;
            };
            if accesses.calls.contains(offset) && kind == MemberKind::Property {
                kind = MemberKind::Method;
            }
            let (module, type_name) = (base.module.clone(), base.name.clone());
            self.push(
                &module,
                &type_name,
                Some((*name).to_owned()),
                kind,
                Resolution::Resolved,
            );
        }
    }

    fn binding(&mut self, owner: usize, path: &[&'a str]) {
        if path == ["id"] {
            return;
        }
        let first = path[0];
        if first.starts_with(|c: char| c.is_ascii_uppercase()) {
            return self.attached(path);
        }
        // `rect.width: 3` in a state's changes names another object's member.
        if path.len() > 1 && self.tree.objects.iter().any(|o| o.id == Some(first)) {
            return;
        }
        let Some(base) = &self.bases[owner] else {
            return;
        };
        let (module, type_name, ty) = (base.module.clone(), base.name.clone(), base.ty.0);
        if self.tree.objects[owner].declared.contains_key(first) {
            return;
        }
        let member = path.join(".");
        match qt_kind(ty, first) {
            Some(kind) => {
                // `font.pixelSize`: the rest are members of the value.
                let resolution = if path.len() > 1 && !value_path_known(ty, path) {
                    Self::absent()
                } else {
                    Resolution::Resolved
                };
                self.push(&module, &type_name, Some(member), kind, resolution);
            }
            None => {
                // A component of the file may declare it; only a name the
                // chain definitely lacks is not the file's own.
                let missing = self.tree.lookup(self.types, owner, first) == Lookup::Missing
                    && handled(first).is_none_or(|s| {
                        self.tree.lookup_signal(self.types, owner, &s) == Lookup::Missing
                    });
                if missing {
                    let kind = if handled(first).is_some() {
                        MemberKind::Signal
                    } else {
                        MemberKind::Property
                    };
                    self.push(&module, &type_name, Some(member), kind, Self::absent());
                }
            }
        }
    }

    /// `Layout.fillWidth`, `Keys.onPressed`: a use of the type that attaches
    /// and of its attached member.
    fn attached(&mut self, path: &[&'a str]) {
        // `C.Layout.fillWidth` has a qualified type; `Layout.fillWidth` not.
        let (type_parts, rest) = if path.len() >= 3 {
            (&path[..2], &path[2..])
        } else {
            (&path[..1], &path[1..])
        };
        let written = type_parts.join(".");
        let Some(found) = self.types.find(type_parts) else {
            return self.unresolved_type(&written);
        };
        let Kind::Builtin(ty) = found.kind else {
            return;
        };
        let module = module_of(&self.types, type_parts, ty);
        let type_name = last_part(&written).to_owned();
        self.push(
            &module,
            &type_name,
            None,
            MemberKind::Type,
            Resolution::Resolved,
        );
        let Some(first) = rest.first() else { return };
        if ty.0.attached().is_none() {
            return;
        }
        let (kind, resolution) = match qt_kind(ty.0, first) {
            Some(kind) => (kind, Resolution::Resolved),
            None => (
                if handled(first).is_some() {
                    MemberKind::Signal
                } else {
                    MemberKind::Property
                },
                Self::absent(),
            ),
        };
        self.push(&module, &type_name, Some(rest.join(".")), kind, resolution);
    }
}

/// Whether the names after the first of `path` are members of the value the
/// first holds, as far as the table can say. A value it does not describe may
/// have any.
fn value_path_known(ty: &'static qt_types::Type, path: &[&str]) -> bool {
    let mut value = ty
        .property(path[0])
        .and_then(qt_types::Property::value_type);
    for name in &path[1..] {
        let Some(next) = value else { return true };
        if qt_kind(next, name).is_none() {
            return false;
        }
        value = next.property(name).and_then(qt_types::Property::value_type);
    }
    true
}
