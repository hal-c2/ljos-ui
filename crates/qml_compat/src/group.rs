//! A file kept keeps the files it uses.
//!
//! A file kept in Qt's QML that sets `color` on a `Button` of the project
//! whose root is a `Rectangle` needs that `Button` to have a `color`: lowered,
//! it has a `fill`. So a file that a kept file uses, by a name of it (a
//! type, a `"Button.qml"`, an imported directory or module), is kept too,
//! and so are the files that one uses. A lowered file may use a kept one:
//! the pass lowers it knowing the type is Qt's, keeping each name it binds
//! on it ([`Root::kept`](crate::Root::kept)), so a project is lowered a
//! file at a time.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{Expression, StaticMemberExpression, StringLiteral};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::{
    Allocator,
    ast::{QmlBindingValue, QmlImportSource, QmlMember, QmlObject},
};

use crate::rules::font::SCREEN;
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
    /// The type the root object is written as (`Rectangle`).
    pub root: Option<String>,
    /// The properties the root object declares.
    pub declares: Vec<String>,
    /// Per object, its type's last name and the first name of each binding
    /// on it (`Dial`, `color`): what a file binds on the types it uses.
    pub bound: Vec<(String, String)>,
}

/// What `source` names that may be a file of the project. A file that does
/// not parse names nothing: it is kept anyway.
pub fn references(source: &str) -> References {
    let allocator = Allocator::default();
    references_in(&qml_syntax::parse(&allocator, source))
}

/// [`references`] of a file parsed already.
pub(crate) fn references_in(parsed: &qml_syntax::Parsed<'_>) -> References {
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
    let root = &parsed.document.root;
    found.root = root.type_name.parts.last().map(|name| (*name).to_string());
    found.declares = root
        .members
        .iter()
        .filter_map(|member| match member {
            QmlMember::Property(property) => Some(property.name.name.to_string()),
            _ => None,
        })
        .collect();
    let mut named = BTreeMap::new();
    ids(root, &mut named);
    let mut inline = BTreeMap::new();
    inline_components(root, &mut inline);
    Walk {
        found: &mut found,
        ids: &named,
        inline: &inline,
    }
    .object(root, None);
    found.types.sort();
    found.types.dedup();
    found.files.sort();
    found.files.dedup();
    found.introduced.sort();
    found.introduced.dedup();
    found.bound.sort();
    found.bound.dedup();
    found
}

/// Each `id` of a component of the file (the file's own, or an inline
/// one's) and the last name of its object's type. An inline component is
/// a scope of its own: its ids are not the file's, nor the file's its.
fn ids<'s>(object: &QmlObject<'s>, named: &mut BTreeMap<&'s str, &'s str>) {
    let ty = object.type_name.parts.last().copied();
    for member in &object.members {
        match member {
            QmlMember::Binding(binding) if binding.name.as_simple() == Some("id") => {
                if let (Some(ty), QmlBindingValue::Expression(Expression::Identifier(id))) =
                    (ty, &binding.value)
                {
                    named.insert(id.name.as_str(), ty);
                }
            }
            QmlMember::Binding(binding) => each_object(&binding.value, |o| ids(o, named)),
            QmlMember::Property(property) => {
                if let Some(v) = &property.value {
                    each_object(v, |o| ids(o, named));
                }
            }
            QmlMember::Object(child) => ids(child, named),
            QmlMember::InlineComponent(_)
            | QmlMember::Function(_)
            | QmlMember::Signal(_)
            | QmlMember::Enum(_) => {}
        }
    }
}

/// An inline component of the file: the last name of the type it is built
/// on, and the properties and signals it declares itself.
struct Inline<'s> {
    base: &'s str,
    declares: BTreeSet<&'s str>,
}

impl Inline<'_> {
    /// Whether `name` (`color`, or its handler `onColorChanged`) is one the
    /// component declares, which its base's lowering renames nothing of.
    fn declares(&self, name: &str) -> bool {
        let handled = is_handler(name).then(|| {
            let rest = &name[2..];
            format!("{}{}", rest[..1].to_ascii_lowercase(), &rest[1..])
        });
        let changed = handled
            .as_deref()
            .and_then(|h| h.strip_suffix("Changed"))
            .map(str::to_owned);
        [Some(name.to_owned()), handled, changed]
            .iter()
            .flatten()
            .any(|n| self.declares.contains(n.as_str()))
    }
}

/// Each inline component of the file, wherever it is declared: one is
/// named in the whole file, in the other inline components too.
fn inline_components<'s>(object: &QmlObject<'s>, found: &mut BTreeMap<&'s str, Inline<'s>>) {
    for member in &object.members {
        match member {
            QmlMember::InlineComponent(component) => {
                let root = &component.object;
                if let Some(base) = root.type_name.parts.last().copied() {
                    let declares = root
                        .members
                        .iter()
                        .filter_map(|member| match member {
                            // `required color` declares nothing: it asks for
                            // the base's `color`.
                            QmlMember::Property(property) if property.type_name.is_some() => {
                                Some(property.name.name.as_str())
                            }
                            QmlMember::Signal(signal) => Some(signal.name.name.as_str()),
                            _ => None,
                        })
                        .collect();
                    found.insert(component.name.name.as_str(), Inline { base, declares });
                }
                inline_components(root, found);
            }
            QmlMember::Object(child) => inline_components(child, found),
            QmlMember::Binding(binding) => {
                each_object(&binding.value, |o| inline_components(o, found))
            }
            QmlMember::Property(property) => {
                if let Some(v) = &property.value {
                    each_object(v, |o| inline_components(o, found));
                }
            }
            QmlMember::Function(_) | QmlMember::Signal(_) | QmlMember::Enum(_) => {}
        }
    }
}

fn each_object<'s>(value: &QmlBindingValue<'s>, mut f: impl FnMut(&QmlObject<'s>)) {
    match value {
        QmlBindingValue::Object(o) => f(o),
        QmlBindingValue::Objects(objects) => objects.iter().for_each(f),
        QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
    }
}

/// The walk of a file that finds its [`References`], knowing its ids.
struct Walk<'f, 's> {
    found: &'f mut References,
    ids: &'f BTreeMap<&'s str, &'s str>,
    inline: &'f BTreeMap<&'s str, Inline<'s>>,
}

impl<'s> Walk<'_, 's> {
    /// `name` bound on an object of type `ty`. An inline component has its
    /// base's properties, so what it does not declare itself is bound on the
    /// base (`LocalDial { color: "red" }` binds `color` on the `Dial` that
    /// `component LocalDial: Dial {}` is built on).
    fn bind(&mut self, ty: &str, name: &str) {
        let mut ty = ty;
        // Bounded by the components' count: one cannot be built on itself,
        // but a file that tries is not followed for ever.
        for _ in 0..=self.inline.len() {
            let Some(component) = self.inline.get(ty) else {
                break;
            };
            if component.declares(name) {
                return;
            }
            ty = component.base;
        }
        self.found.bound.push((ty.to_string(), name.to_string()));
    }

    /// The type of the object `target` names by its id.
    fn target(&self, value: &QmlBindingValue<'_>) -> Option<&'s str> {
        match value {
            QmlBindingValue::Expression(Expression::Identifier(id)) => {
                self.ids.get(id.name.as_str()).copied()
            }
            _ => None,
        }
    }

    /// What `object` names, in the object of type `parent` when it is one's
    /// child.
    fn object(&mut self, object: &QmlObject<'_>, parent: Option<&str>) {
        let ty = object.type_name.parts.last().copied();
        if let Some(name) = ty {
            self.found.types.push(name.to_string());
        }
        self.sets_on_target(object, parent);
        for member in &object.members {
            match member {
                QmlMember::Object(child) => {
                    // `Behavior on color { }` and `font { bold: true }` bind
                    // that name on the object as a binding does.
                    let grouped = child
                        .type_name
                        .parts
                        .first()
                        .filter(|name| name.starts_with(|c: char| c.is_ascii_lowercase()));
                    let on = child.on.as_ref().and_then(|on| on.parts.first());
                    if let (Some(ty), Some(name)) = (ty, on.or(grouped)) {
                        self.bind(ty, name);
                    }
                    self.object(child, ty);
                }
                QmlMember::Binding(binding) => {
                    if let (Some(ty), Some(name)) = (ty, binding.name.parts.first()) {
                        self.bind(ty, name);
                    }
                    // `font.pointSize` is lowered to `Screen.points(…)`.
                    if binding.name.parts.last() == Some(&"pointSize") {
                        self.found.introduced.push(SCREEN.to_string());
                    }
                    self.value(&binding.value, ty);
                }
                QmlMember::Property(property) => {
                    if let Some(type_name) = &property.type_name {
                        // `property Button b`, `property list<Button> bs`.
                        let written = type_name.name.to_string();
                        self.found
                            .types
                            .push(written.rsplit('.').next().unwrap_or(&written).to_string());
                    }
                    if let Some(v) = &property.value {
                        self.value(v, ty);
                    }
                }
                QmlMember::Function(function) => {
                    Strings(self.found).visit_function(function, ScopeFlags::Function);
                }
                QmlMember::InlineComponent(component) => {
                    let mut named = BTreeMap::new();
                    ids(&component.object, &mut named);
                    Walk {
                        found: &mut *self.found,
                        ids: &named,
                        inline: self.inline,
                    }
                    .object(&component.object, None);
                }
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    /// What a `PropertyChanges` or a `Binding` sets on the object its
    /// `target` names, on the object an id of a binding's name does
    /// (`dial.color: "red"`) or on the one an id of a group names
    /// (`dial { color: "red" }`), and the handlers a `Connections` has for
    /// its `target`, else for its `parent`: bound on that object's type as
    /// a binding on it is.
    fn sets_on_target(&mut self, object: &QmlObject<'_>, parent: Option<&str>) {
        let kind = object.type_name.parts.last().copied();
        if !matches!(kind, Some("PropertyChanges" | "Binding" | "Connections")) {
            return;
        }
        let bindings = || {
            object.members.iter().filter_map(|member| match member {
                QmlMember::Binding(binding) => Some(binding),
                _ => None,
            })
        };
        let named = bindings().find(|b| b.name.as_simple() == Some("target"));
        let target = named.and_then(|b| self.target(&b.value));
        if kind == Some("Connections") {
            let Some(ty) = (if named.is_some() { target } else { parent }) else {
                return;
            };
            for member in &object.members {
                let name = match member {
                    QmlMember::Function(function) => {
                        function.id.as_ref().map(|id| id.name.as_str())
                    }
                    QmlMember::Binding(binding) => binding.name.as_simple(),
                    _ => None,
                };
                if let Some(name) = name.filter(|name| is_handler(name)) {
                    self.bind(ty, name);
                }
            }
            return;
        }
        if kind == Some("Binding") {
            let property = bindings()
                .find(|b| b.name.as_simple() == Some("property"))
                .and_then(|b| match &b.value {
                    QmlBindingValue::Expression(Expression::StringLiteral(s)) => {
                        Some(s.value.as_str())
                    }
                    _ => None,
                });
            if let (Some(ty), Some(name)) = (target, property) {
                self.bind(ty, name.split('.').next().unwrap_or(name));
            }
        }
        for binding in bindings() {
            match binding.name.parts.as_slice() {
                [name]
                    if kind == Some("PropertyChanges")
                        && !matches!(*name, "target" | "explicit" | "restoreEntryValues") =>
                {
                    if let Some(ty) = target {
                        self.bind(ty, name);
                    }
                }
                [id, name, ..] => {
                    if let Some(ty) = self.ids.get(*id).copied() {
                        self.bind(ty, name);
                    }
                }
                _ => {}
            }
        }
        for member in &object.members {
            if let QmlMember::Object(group) = member
                && let [id] = group.type_name.parts.as_slice()
                && let Some(ty) = self.ids.get(*id).copied()
            {
                for inner in &group.members {
                    if let QmlMember::Binding(binding) = inner
                        && let Some(name) = binding.name.parts.first()
                    {
                        self.bind(ty, name);
                    }
                }
            }
        }
    }

    /// What a value names. An object it holds is a child of the object of
    /// type `parent` (its QObject parent, which a `Connections` with no
    /// `target` connects to), as one declared among its members is.
    fn value(&mut self, value: &QmlBindingValue<'_>, parent: Option<&str>) {
        match value {
            QmlBindingValue::Expression(expression) => {
                Strings(self.found).visit_expression(expression);
            }
            QmlBindingValue::Statement(statement) => Strings(self.found).visit_statement(statement),
            QmlBindingValue::Object(o) => self.object(o, parent),
            QmlBindingValue::Objects(objects) => {
                objects.iter().for_each(|o| self.object(o, parent))
            }
        }
    }
}

/// Whether `name` is a signal handler's (`onColorChanged`).
fn is_handler(name: &str) -> bool {
    name.strip_prefix("on")
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
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
/// lowered unless a kept file uses it, or uses a file that one uses, and is
/// otherwise kept, saying which kept file uses it.
pub fn group(files: &BTreeMap<String, Alone>) -> BTreeMap<String, Outcome> {
    let uses = uses(files);
    let keys: Vec<&String> = files.keys().collect();
    // A file in Ljós's API that uses one in Qt's QML binds Qt's names on
    // it: lowered, the file would rename what that one binds.
    let alone: Vec<Outcome> = files
        .values()
        .enumerate()
        .map(|(at, file)| match &file.outcome {
            Outcome::Lowered(_) => match bound_as_qt(files, &keys, &uses, at) {
                Some(cause) => Outcome::Kept(Kept { cause, at: 0 }),
                None => file.outcome.clone(),
            },
            outcome => outcome.clone(),
        })
        .collect();
    // Per file, the kept file that keeps it, found from each kept file
    // along what it uses: the first to reach it. A file left as it is goes
    // on in Qt's names too, so what it uses is reached through it, and only
    // a lowered file changes.
    let mut kept_by: Vec<Option<(usize, &Kept)>> = vec![None; keys.len()];
    let mut reached = vec![false; keys.len()];
    let mut next: Vec<(usize, usize, &Kept)> = Vec::new();
    for (at, outcome) in alone.iter().enumerate() {
        if let Outcome::Kept(kept) = outcome {
            reached[at] = true;
            next.push((at, at, kept));
        }
    }
    while !next.is_empty() {
        let mut after = Vec::new();
        for (from, at, kept) in next {
            for &used in &uses[at] {
                if reached[used] {
                    continue;
                }
                match alone[used] {
                    Outcome::Lowered(_) => kept_by[used] = Some((from, kept)),
                    Outcome::Unchanged => {}
                    Outcome::Kept(_) => continue,
                }
                reached[used] = true;
                after.push((from, used, kept));
            }
        }
        next = after;
    }
    files
        .keys()
        .zip(&alone)
        .enumerate()
        .map(|(at, (key, outcome))| {
            let outcome = match (outcome, kept_by[at]) {
                (Outcome::Lowered(_), Some((by, why))) => Outcome::Kept(Kept {
                    cause: Cause::Group {
                        with: from_dir_of(key, keys[by]).to_owned(),
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

/// Why the file at `at`, which the pass lowered, is kept for a file in
/// Ljós's API that uses it: that one binds on it a name its lowering
/// renames (`color` on a `Dial` whose root is a `Rectangle`, which Ljós's
/// calls `fill`), in a binding, a `Behavior on color`, a grouped property,
/// or a `PropertyChanges` or `Binding` whose `target` is the `Dial`'s id.
/// What a script of that file sets is not read, nor a target that is no id
/// of the file: `ljos check` says it once the file is lowered.
fn bound_as_qt(
    files: &BTreeMap<String, Alone>,
    keys: &[&String],
    uses: &[BTreeSet<usize>],
    at: usize,
) -> Option<Cause> {
    let ty = keys[at].rsplit('/').next().unwrap_or(keys[at]);
    files.values().enumerate().find_map(|(by, user)| {
        let in_ljos = matches!(user.outcome, Outcome::Unchanged)
            && user
                .references
                .modules
                .iter()
                .any(|m| m == "Ljos" || m.starts_with("Ljos."));
        if !in_ljos || !uses[by].contains(&at) {
            return None;
        }
        user.references
            .bound
            .iter()
            .filter(|(on, _)| on == ty)
            .find_map(|(_, name)| {
                let (qt, now) = renamed(files, keys, uses, at, name, keys.len())?;
                Some(Cause::BoundAsQt {
                    by: from_dir_of(keys[at], keys[by]).to_owned(),
                    name: name.clone(),
                    ty: crate::rules::new_name(qt).unwrap_or(qt).to_owned(),
                    now,
                })
            })
    })
}

/// The Qt type the root of the file at `at` is built on, and what Ljós's
/// calls `name` on it, when the file lowered renames `name`: through the
/// files of the project its root is written as, while they are lowered
/// too. A handler (`onColorChanged`) follows its property.
fn renamed<'f>(
    files: &'f BTreeMap<String, Alone>,
    keys: &[&String],
    uses: &[BTreeSet<usize>],
    at: usize,
    name: &str,
    depth: usize,
) -> Option<(&'f str, String)> {
    let file = &files[keys[at]];
    if depth == 0 || file.references.declares.iter().any(|d| d == name) {
        return None;
    }
    let root = file.references.root.as_deref()?;
    let base = uses[at]
        .iter()
        .find(|&&other| keys[other].rsplit('/').next() == Some(root));
    if let Some(&base) = base {
        return match files[keys[base]].outcome {
            Outcome::Lowered(_) => renamed(files, keys, uses, base, name, depth - 1),
            _ => None,
        };
    }
    let (_, renames) = crate::rules::member_renames().find(|(ty, _)| *ty == root)?;
    renames.iter().find_map(|(old, new)| {
        if name == *old {
            Some((root, (*new).to_owned()))
        } else {
            (name == handler(old)).then(|| (root, handler(new)))
        }
    })
}

/// `onColorChanged` for `color`.
fn handler(property: &str) -> String {
    let mut chars = property.chars();
    let first = chars.next().map(|c| c.to_ascii_uppercase());
    format!(
        "on{}{}Changed",
        first.map(String::from).unwrap_or_default(),
        chars.as_str()
    )
}

/// Per file of `files`, in their order, the files it uses: by a type of
/// one beside it, a `.qml` address, a module the loader finds, or an
/// imported directory or module of the project.
pub(crate) fn uses(files: &BTreeMap<String, Alone>) -> Vec<BTreeSet<usize>> {
    let keys: Vec<&String> = files.keys().collect();
    let index = |key: &str| keys.binary_search_by(|k| k.as_str().cmp(key)).ok();
    let mut sets = Uses(vec![BTreeSet::new(); keys.len()]);
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
            let prefix = format!("{}/", imported_dir(dir, imported));
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
    sets.0
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

/// The key of the directory a file in `dir` imports by `path`.
pub(crate) fn imported_dir(dir: &str, path: &str) -> String {
    normal(&join(dir, path.trim_end_matches('/')))
}

/// Whether a file in `dir` that imports `path` may import the directory
/// `key`. A URL (`file:///…`) is the project's to resolve, against the
/// places it reads, so it may be any directory: lowering a file again
/// costs only time.
pub(crate) fn may_import(dir: &str, path: &str, key: &str) -> bool {
    path.contains(':') || imported_dir(dir, path) == key
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

/// Per file index, the indices of the files it uses.
struct Uses(Vec<BTreeSet<usize>>);

impl Uses {
    /// `a` uses `b`.
    fn join(&mut self, a: usize, b: usize) {
        if a != b {
            self.0[a].insert(b);
        }
    }
}

/// `import "lib.js"`, `import "filter.ts"`: a script, not a directory of
/// types (as `qml_front::project::is_script`, which this crate does not
/// link, says).
pub(crate) fn is_script(path: &str) -> bool {
    path.ends_with(".js")
        || path.ends_with(".mjs")
        || path.ends_with(".lts")
        || (path.ends_with(".ts") && !path.ends_with(".d.ts"))
}
