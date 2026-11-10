//! Files that name one another are lowered together or not at all.
//!
//! A file kept in Qt's QML that sets `color` on a `Button` of the project
//! whose root is a `Rectangle` needs that `Button` to have a `color`: lowered,
//! it has a `fill`. So does a lowered file that renames `color` on a kept
//! one. The pass decides per file what it can lower; a project decides per
//! *group*, the files connected by a name of one in another (a type, a
//! `"Button.qml"`, an imported directory or module), and lowers a group
//! only when it can lower each of its files.

use std::collections::BTreeMap;

use oxc_ast::ast::{Expression, StaticMemberExpression, StringLiteral};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::{
    Allocator,
    ast::{QmlBindingValue, QmlImportSource, QmlMember, QmlObject},
};

use crate::{Cause, Kept, Outcome};

/// What of the project a file may name: the words in it that could be a
/// file beside it, the `.qml` addresses its scripts hold, the directories it
/// imports. A name that is no file of the project ties nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct References {
    /// Type names, as `Button` names `Button.qml` beside the file.
    pub types: Vec<String>,
    /// `.qml` addresses, as a `Loader`'s `source` or `Qt.createComponent`
    /// has them, relative to the file.
    pub files: Vec<String>,
    /// Directories imported by path, relative to the file.
    pub dirs: Vec<String>,
    /// Modules imported by name (`MyControls`, `My.Controls`): a module of
    /// the project is a directory whose path ends in the name's parts.
    pub modules: Vec<String>,
    /// The type names lowering would bring in (`App` of `Qt.platform`),
    /// which a file beside it of that name takes: the file is lowered again
    /// when one comes or goes. They tie it to no group.
    pub introduced: Vec<String>,
}

/// What `source` names that may be a file of the project. A file that does
/// not parse names nothing: it is kept anyway.
pub fn references(source: &str) -> References {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    let mut found = References::default();
    if parsed.panicked {
        return found;
    }
    for import in &parsed.document.imports {
        match &import.source {
            QmlImportSource::Path(path) if !is_script(path) => {
                found.dirs.push((*path).to_string());
            }
            QmlImportSource::Module(name) => found.modules.push(name.to_string()),
            QmlImportSource::Path(_) => {}
        }
    }
    object(&parsed.document.root, &mut found);
    found.types.sort();
    found.types.dedup();
    found.files.sort();
    found.files.dedup();
    found.introduced.sort();
    found.introduced.dedup();
    found
}

fn object(object: &QmlObject<'_>, found: &mut References) {
    if let Some(name) = object.type_name.parts.last() {
        found.types.push((*name).to_string());
    }
    for member in &object.members {
        match member {
            QmlMember::Object(child) => self::object(child, found),
            QmlMember::Binding(binding) => value(&binding.value, found),
            QmlMember::Property(property) => {
                if let Some(type_name) = &property.type_name {
                    // `property Button b`, `property list<Button> bs`.
                    let written = type_name.name.to_string();
                    found
                        .types
                        .push(written.rsplit('.').next().unwrap_or(&written).to_string());
                }
                if let Some(v) = &property.value {
                    value(v, found);
                }
            }
            QmlMember::Function(function) => {
                Strings(found).visit_function(function, ScopeFlags::Function);
            }
            QmlMember::InlineComponent(component) => self::object(&component.object, found),
            QmlMember::Signal(_) | QmlMember::Enum(_) => {}
        }
    }
}

fn value(value: &QmlBindingValue<'_>, found: &mut References) {
    match value {
        QmlBindingValue::Expression(expression) => Strings(found).visit_expression(expression),
        QmlBindingValue::Statement(statement) => Strings(found).visit_statement(statement),
        QmlBindingValue::Object(o) => object(o, found),
        QmlBindingValue::Objects(objects) => objects.iter().for_each(|o| object(o, found)),
    }
}

/// The `.qml` addresses and the type names a script holds.
struct Strings<'f>(&'f mut References);

impl<'a> Visit<'a> for Strings<'_> {
    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        if it.value.ends_with(".qml") {
            self.0.files.push(it.value.to_string());
        }
    }

    fn visit_expression(&mut self, it: &Expression<'a>) {
        // `Button.Pressed`, an enum of a file of the project.
        if let Expression::Identifier(id) = it
            && id.name.starts_with(|c: char| c.is_ascii_uppercase())
        {
            self.0.types.push(id.name.to_string());
        }
        walk::walk_expression(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(qt) = &it.object
            && qt.name == "Qt"
            && let Some(name) = crate::qt_members::introduced(&it.property.name)
        {
            self.0.introduced.push(name.to_string());
        }
        walk::walk_static_member_expression(self, it);
    }
}

/// A file of a project, as [`group`] is given it: what the pass made of it
/// alone, and what it names.
#[derive(Clone, Debug)]
pub struct Alone {
    pub outcome: Outcome,
    pub references: References,
    /// The files of the project its imports bring, keyed as [`group`]'s
    /// files are, as the project's loader finds them: a module's files may
    /// be anywhere a build or a `qmldir` says.
    pub uses: Vec<String>,
}

/// What each file of a project is lowered to, keyed as `files` is by the
/// file's path without `.qml` (`qml/Button`): a file the pass lowered stays
/// lowered only when every file of its group was lowered or had nothing to
/// lower, and is otherwise kept, saying which file of the group was.
pub fn group(files: &BTreeMap<String, Alone>) -> BTreeMap<String, Outcome> {
    let keys: Vec<&String> = files.keys().collect();
    let index = |key: &str| keys.binary_search_by(|k| k.as_str().cmp(key)).ok();
    let mut sets = Sets::new(keys.len());
    for (at, (key, file)) in files.iter().enumerate() {
        let dir = key.rsplit_once('/').map_or("", |(dir, _)| dir);
        let beside = |name: &str| join(dir, name);
        for name in &file.references.types {
            if let Some(other) = index(&beside(name)) {
                sets.join(at, other);
            }
        }
        for address in &file.references.files {
            let path = address.strip_suffix(".qml").unwrap_or(address);
            if let Some(other) = index(&normal(&beside(path))) {
                sets.join(at, other);
            }
        }
        for used in &file.uses {
            if let Some(other) = index(used) {
                sets.join(at, other);
            }
        }
        for imported in &file.references.dirs {
            let prefix = format!("{}/", normal(&beside(imported.trim_end_matches('/'))));
            for (other, k) in keys.iter().enumerate() {
                if k.strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.contains('/'))
                {
                    sets.join(at, other);
                }
            }
        }
    }
    // An import path makes a module of a directory whose path ends in the
    // module's name, wherever the path starts: every file below such a
    // directory may be the module's (a qmldir may name files in its
    // subdirectories), and joining one that is not only keeps it with the
    // importer.
    let dirs: Vec<Vec<&str>> = keys
        .iter()
        .map(|key| {
            let mut parts: Vec<&str> = key.split('/').collect();
            parts.pop();
            parts
        })
        .collect();
    for (at, (_, file)) in files.iter().enumerate() {
        for module in &file.references.modules {
            let name: Vec<&str> = module.split('.').collect();
            for (other, dir) in dirs.iter().enumerate() {
                let below = (0..dir.len()).any(|end| dir[..=end].ends_with(&name));
                if below {
                    sets.join(at, other);
                }
            }
        }
    }
    // Per group, the first file of it the pass kept.
    let mut kept_in: BTreeMap<usize, (&String, &Kept)> = BTreeMap::new();
    for (at, (key, file)) in files.iter().enumerate() {
        if let Outcome::Kept(kept) = &file.outcome {
            kept_in.entry(sets.root(at)).or_insert((key, kept));
        }
    }
    files
        .iter()
        .enumerate()
        .map(|(at, (key, file))| {
            let outcome = match (&file.outcome, kept_in.get(&sets.root(at))) {
                (Outcome::Lowered(_), Some((other, why))) => Outcome::Kept(Kept {
                    cause: Cause::Group {
                        with: from_dir_of(key, other).to_owned(),
                        cause: Box::new(why.cause.clone()),
                    },
                    at: 0,
                }),
                (outcome, _) => outcome.clone(),
            };
            (key.clone(), outcome)
        })
        .collect()
}

/// `other` as a path from the directory of `key`: `Gauges/Dial` below it,
/// and its name otherwise, as beside it.
fn from_dir_of<'k>(key: &str, other: &'k str) -> &'k str {
    let below = match key.rsplit_once('/') {
        Some((dir, _)) => other
            .strip_prefix(dir)
            .and_then(|rest| rest.strip_prefix('/')),
        None => Some(other),
    };
    below.unwrap_or_else(|| other.rsplit('/').next().unwrap_or(other))
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// `a/./b/../c` as `a/c`.
fn normal(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if path.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

/// Disjoint sets of file indices.
struct Sets(Vec<usize>);

impl Sets {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }

    fn root(&mut self, mut at: usize) -> usize {
        while self.0[at] != at {
            self.0[at] = self.0[self.0[at]];
            at = self.0[at];
        }
        at
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        self.0[a.max(b)] = a.min(b);
    }
}

/// `import "lib.js"`, `import "filter.ts"`: a script, not a directory of
/// types (as `qml_front::project::is_script`, which this crate does not
/// link, says).
pub(crate) fn is_script(path: &str) -> bool {
    path.ends_with(".js")
        || path.ends_with(".mjs")
        || (path.ends_with(".ts") && !path.ends_with(".d.ts"))
}
