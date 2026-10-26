//! `qml_check` and `qml_lint`: one pass per file over the QML tree, using
//! `qml_front` for what a name or a type is.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use oxc_ast_visit::Visit;
use oxc_syntax::scope::ScopeFlags;
use qml_front::{
    Code, QtType, QtTypeOracle, Special, TypeOracle,
    names::Names,
    project::{Project, summarize},
    scope::{Tree, handled, is_group},
    types::{Kind, Lookup, Member, Types},
};
use qml_syntax::{
    Allocator,
    ast::{QmlBinding, QmlBindingValue, QmlImportSource, QmlMember, QmlObject},
};

use super::{
    closest, diagnostic,
    facts::{Script, is_handler_name},
    lint,
    load::Loader,
};
use crate::{Diagnostic, Severity, lints};

type Oracle = QtTypeOracle;

/// What `Types` knows about an object, as far as a check may rely on it.
#[derive(Clone, Copy)]
pub(super) enum Shape {
    /// The type is not one the file knows; already reported.
    Unknown,
    /// The type chain ends in something whose members the Qt table does not
    /// carry (a Controls style written as a QML file with no native base), or
    /// in a component that could not be resolved.
    Incomplete,
    /// Qt reads the object's body itself (`ListElement`, `Connections`,
    /// `PropertyChanges`): any name may be in it.
    Opaque,
    /// Every member of the chain is known: the builtin type it ends in.
    Complete(QtType),
}

pub(super) fn shape(types: &Types<'_, Oracle>, kind: Option<&Kind<QtType>>) -> Shape {
    let Some(kind) = kind else {
        return Shape::Unknown;
    };
    // A component whose file did not parse whole is known by what did parse;
    // a name missing from it may be in the part that did not.
    if types.through_partial(kind) {
        return Shape::Incomplete;
    }
    let Some(base) = types.base(kind) else {
        return Shape::Incomplete;
    };
    let mut opaque = false;
    let mut ty = Some(base.0);
    while let Some(t) = ty {
        if t.qml_file.is_some() && t.prototype().is_none() {
            return Shape::Incomplete;
        }
        // `ListElement` is read by its `ListModel`'s parser, so the table
        // marks the model and not the element.
        opaque |= t.has_custom_parser || qt_types::class::LIST_ELEMENT.contains(&t.class);
        ty = t.prototype();
    }
    if opaque {
        Shape::Opaque
    } else {
        Shape::Complete(base)
    }
}

/// What the files of one call share.
struct Env<'p> {
    loader: &'p Loader<'p>,
    project: &'p Project,
    modules: &'p HashSet<String>,
    broken: &'p HashSet<String>,
}

/// A name that was unresolved in its file. Whether it really is waits until
/// every file of the project is loaded: another component may provide it.
struct Pending {
    slot: usize,
    name: String,
    diagnostic: Diagnostic,
}

/// The code of a type the front end holds back on (one some C++ registers, or
/// one an unknown module may provide). It never leaves [`run`]: it marks the
/// place for the compiler's findings to be held back at too.
const HELD: &str = "held";

/// What the front end finds in `files`, and the places it held back on.
pub(super) fn run(
    root: &Path,
    files: &[PathBuf],
    lint: bool,
) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let mut per_file: Vec<Vec<Diagnostic>> = vec![Vec::new(); files.len()];
    let mut loader = Loader::new(root);
    // Files that parse, with their key and text.
    let mut parsed_ok: Vec<(usize, String, String)> = Vec::new();

    for (index, path) in files.iter().enumerate() {
        let key = loader.key(path).unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let text = match std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| String::from_utf8(b).map_err(|_| "not valid UTF-8".to_owned()))
        {
            Ok(text) => text,
            Err(why) => {
                per_file[index].push(diagnostic(
                    path,
                    "",
                    0,
                    Severity::Error,
                    "io",
                    format!("cannot read the file: {why}"),
                ));
                loader.insert(&key, None);
                continue;
            }
        };
        let allocator = Allocator::default();
        let parsed = qml_syntax::parse(&allocator, &text);
        if parsed.is_ok() {
            loader.insert(&key, Some(summarize(&parsed.document)));
            parsed_ok.push((index, key, text));
            continue;
        }
        for d in &parsed.diagnostics {
            per_file[index].push(diagnostic(
                path,
                &text,
                d.span.start,
                Severity::Error,
                Code::Syntax.as_str(),
                d.message.clone(),
            ));
        }
        if parsed.diagnostics.is_empty() {
            per_file[index].push(diagnostic(
                path,
                &text,
                0,
                Severity::Error,
                Code::Syntax.as_str(),
                "the file could not be parsed",
            ));
        }
        // Others that build on this file still know what of it parsed.
        loader.insert_partial(&key, &text);
    }

    for (index, _, _) in &parsed_ok {
        if let Some(dir) = files[*index].parent() {
            loader.add_dir(dir);
        }
    }
    loader.settle();
    loader.discover_modules();
    loader.settle();

    let mut pending: Vec<Pending> = Vec::new();
    {
        let env = Env {
            loader: &loader,
            project: &loader.project,
            modules: &loader.modules,
            broken: &loader.broken,
        };
        for (index, key, text) in &parsed_ok {
            file(
                &env,
                key,
                &files[*index],
                text,
                lint,
                &mut per_file[*index],
                &mut |name, diagnostic| {
                    pending.push(Pending {
                        slot: *index,
                        name,
                        diagnostic,
                    });
                },
            );
        }
    }

    if !pending.is_empty() {
        loader.add_everything();
        let dynamic = loader.project.dynamic_names(&QtTypeOracle);
        for p in pending {
            if !dynamic.contains(&p.name) {
                per_file[p.slot].push(p.diagnostic);
            }
        }
    }

    for diagnostics in &mut per_file {
        diagnostics.sort_by(|a, b| {
            (a.line, a.column, &a.code, &a.message).cmp(&(b.line, b.column, &b.code, &b.message))
        });
        diagnostics.dedup();
    }
    per_file.into_iter().flatten().partition(|d| d.code != HELD)
}

fn file(
    env: &Env<'_>,
    key: &str,
    path: &Path,
    source: &str,
    lint: bool,
    out: &mut Vec<Diagnostic>,
    pending: &mut dyn FnMut(String, Diagnostic),
) {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        return;
    }
    let doc = &parsed.document;
    let oracle = QtTypeOracle;
    let types = Types::new(env.project, &oracle, key);

    // Imports no one knows: the module may provide the types we cannot find.
    let mut unverifiable = false;
    let mut imports: Vec<String> = Vec::new();
    for import in &doc.imports {
        if let QmlImportSource::Module(uri) = &import.source {
            let uri = uri.to_string();
            if !oracle.has_module(&uri) && !env.modules.contains(&uri) {
                unverifiable = true;
                out.push(diagnostic(
                    path,
                    source,
                    import.span.start,
                    Severity::Warning,
                    Code::UnknownModule.as_str(),
                    format!(
                        "module `{uri}` is not in Qt's type table or in this project's qmldir/CMake \
                         modules; types from it cannot be checked"
                    ),
                ));
            }
            imports.push(uri);
        }
    }

    let mut errors = Vec::new();
    let tree = Tree::analyze(doc, types, &mut errors);
    for error in &errors {
        if let Some(d) = semantic_error(env, &imports, path, source, error, unverifiable) {
            out.push(d);
        }
    }

    let shapes: Vec<Shape> = tree
        .objects
        .iter()
        .map(|o| match (&o.kind, shape(&types, o.kind.as_ref())) {
            // A QML file that shares its name with a type some C++ registers
            // may not be the type that is meant.
            (Some(Kind::Component(_)), Shape::Complete(_))
                if env.loader.native(last_part(&o.name)) =>
            {
                Shape::Incomplete
            }
            (_, shape) => shape,
        })
        .collect();
    let check_names = !unverifiable
        && shapes
            .iter()
            .all(|s| !matches!(s, Shape::Unknown | Shape::Incomplete));
    let component_file = key
        .rsplit('/')
        .next()
        .is_some_and(|stem| stem.starts_with(|c: char| c.is_ascii_uppercase()));

    {
        let mut walker = Walker {
            names: Names { tree: &tree, types },
            tree: &tree,
            types,
            shapes: &shapes,
            path,
            source,
            out,
            pending,
            native: &|name| env.loader.native(name),
            check_types: !unverifiable,
            check_names,
            component_file,
            lint,
        };
        walker.child(&doc.root);
    }

    if lint {
        lint::run(
            &lint::Cx {
                tree: &tree,
                types,
                shapes: &shapes,
                path,
                source,
            },
            &doc.root,
            out,
        );
        super::allow::apply(path, source, &doc.root, out);
    }
}

/// Turns one of `Tree::analyze`'s errors into a diagnostic that has the code
/// the front end gave it.
fn semantic_error(
    env: &Env<'_>,
    imports: &[String],
    path: &Path,
    source: &str,
    error: &qml_front::Error,
    unverifiable: bool,
) -> Option<Diagnostic> {
    let at = error.span.start;
    if error.code != Code::UnknownType {
        return Some(diagnostic(
            path,
            source,
            at,
            Severity::Error,
            error.code.as_str(),
            error.message.clone(),
        ));
    }
    // The span is the type's name as written.
    let name = source
        .get(error.span.start as usize..error.span.end as usize)
        .unwrap_or_default();
    let last = last_part(name);
    // The file exists and did not parse (its own errors say why), or the
    // type is one some C++ registers.
    if unverifiable || env.broken.contains(last) || env.loader.native(last) {
        return Some(diagnostic(
            path,
            source,
            at,
            Severity::Hint,
            HELD,
            String::new(),
        ));
    }
    let mut text = format!(
        "`{name}` is not a type provided by any module this file imports or by a file beside it"
    );
    let mut candidates: Vec<&str> = Vec::new();
    for uri in imports {
        for list in [qt_types::native_types(uri), qt_types::written_types(uri)]
            .into_iter()
            .flatten()
        {
            candidates.extend(list);
        }
    }
    if let Some(near) = closest(name, candidates) {
        text.push_str(&format!("; did you mean `{near}`?"));
    } else if !name.contains('.') {
        text.push_str("; is the module imported?");
    }
    // A name no module of Ljós's own table has may still be a type of Qt's
    // that Ljós lacks, which only Qt's table can tell from a slip.
    if !qt_types::has_qt_table() && !qt_types::is_type_name(last) {
        text.push_str(&format!(" ({})", qt_types::WITHOUT_QT_TABLE));
    }
    Some(diagnostic(
        path,
        source,
        at,
        Severity::Error,
        Code::UnknownType.as_str(),
        text,
    ))
}

struct Walker<'w, 'a> {
    tree: &'w Tree<'a, QtType>,
    types: Types<'w, Oracle>,
    names: Names<'w, 'a, 'w, Oracle>,
    shapes: &'w [Shape],
    path: &'w Path,
    source: &'w str,
    out: &'w mut Vec<Diagnostic>,
    pending: &'w mut dyn FnMut(String, Diagnostic),
    native: &'w dyn Fn(&str) -> bool,
    check_types: bool,
    check_names: bool,
    component_file: bool,
    lint: bool,
}

impl<'a> Walker<'_, 'a> {
    fn report(&mut self, at: u32, severity: Severity, code: &str, message: String) {
        self.out.push(diagnostic(
            self.path,
            self.source,
            at,
            severity,
            code,
            message,
        ));
    }

    fn child(&mut self, object: &QmlObject<'a>) {
        if let Some(index) = self.tree.index_of(object) {
            self.members(object, index, &[]);
        }
    }

    /// `group` is the names that lead from the owner to `object` when it is
    /// a group (`font { }`).
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
                    self.binding(owner, &path, binding);
                    self.value(&binding.value, owner, &path);
                }
                QmlMember::Property(declaration) => {
                    if let Some(value) = &declaration.value {
                        self.value(value, owner, &[]);
                    }
                }
                QmlMember::Function(function) => {
                    let mut script = Script::default();
                    script.visit_function(function, ScopeFlags::Function);
                    self.finish(owner, script, Some(Vec::new()));
                }
                QmlMember::InlineComponent(inline) => self.child(&inline.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn value(&mut self, value: &QmlBindingValue<'a>, owner: usize, path: &[&'a str]) {
        let mut script = Script::default();
        match value {
            QmlBindingValue::Expression(e) => script.visit_expression(e),
            QmlBindingValue::Statement(s) => script.visit_statement(s),
            QmlBindingValue::Object(o) => return self.child(o),
            QmlBindingValue::Objects(os) => {
                for o in os {
                    self.child(o);
                }
                return;
            }
        }
        let params = self.script_params(owner, path);
        self.finish(owner, script, params);
    }

    /// What a handler may call its arguments without declaring them: `Some`
    /// and empty for anything that is not a handler, `None` when the signal
    /// is not known and so neither are the names.
    fn script_params(&self, owner: usize, path: &[&str]) -> Option<Vec<String>> {
        let Some(last) = path.last().filter(|l| is_handler_name(l)) else {
            return Some(Vec::new());
        };
        let signal = handled(last)?;
        match path {
            [_] => self.tree.signal(self.types, owner, &signal),
            [qualifier, _] if qualifier.starts_with(|c: char| c.is_ascii_uppercase()) => {
                let found = self.types.find(&[qualifier])?;
                let Kind::Builtin(ty) = found.kind else {
                    return None;
                };
                let signal = ty.0.attached()?.signal(&signal)?;
                Some(signal.parameters.iter().map(|p| (*p).to_owned()).collect())
            }
            _ => None,
        }
    }

    fn finish(&mut self, owner: usize, script: Script<'a>, params: Option<Vec<String>>) {
        if self.lint {
            for at in &script.dynamic_tr {
                self.report(
                    *at,
                    lints::QSTR_NON_LITERAL.severity,
                    lints::QSTR_NON_LITERAL.code,
                    "`qsTr` needs a string literal: the translation tools extract the text from the \
                     source, and a variable or a concatenation cannot be extracted"
                        .to_owned(),
                );
            }
        }
        let Some(params) = params else { return };
        if !self.check_names || !matches!(self.shapes[owner], Shape::Complete(_)) {
            return;
        }
        for (name, offset) in &script.refs {
            // Capitalised names are types, namespaces and JavaScript's own
            // globals, whose list this does not try to complete.
            if !name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                || script.locals.contains(name)
                || params.iter().any(|p| p == name)
                // What a model gives every delegate, and which a delegate in
                // a file of its own still has.
                || matches!(*name, "index" | "model" | "modelData")
                || self.names.lookup(name, *offset).is_some()
            {
                continue;
            }
            let mut message = format!(
                "`{name}` is not defined: no id, property or function in this component or around it has that name"
            );
            if self.component_file {
                message.push_str(
                    "; if the component that creates this one (or a delegate's model) provides it, ignore this",
                );
            }
            let d = diagnostic(
                self.path,
                self.source,
                *offset,
                Severity::Warning,
                Code::UnresolvedName.as_str(),
                message,
            );
            (self.pending)((*name).to_owned(), d);
        }
    }

    fn binding(&mut self, owner: usize, path: &[&'a str], binding: &QmlBinding<'a>) {
        let owner = self.changed(owner, path[0]).unwrap_or(owner);
        if !matches!(self.shapes[owner], Shape::Complete(_)) || path == ["id"] {
            return;
        }
        let first = path[0];
        let at = binding.name.span.start;
        if first.starts_with(|c: char| c.is_ascii_uppercase()) {
            self.attached(path, at);
            return;
        }
        // `rect.width: 3` in a state's changes names another object's member.
        if path.len() > 1 && self.tree.objects.iter().any(|o| o.id == Some(first)) {
            return;
        }
        if path.len() == 1 && is_handler_name(first) {
            self.handler(owner, first, at);
        } else {
            self.property(owner, path, at);
        }
    }

    /// The object a PropertyChanges `owner` changes `name` of, when its
    /// `target` is an id and `name` is none of the PropertyChanges' own:
    /// what it is given is held to that object's members.
    fn changed(&self, owner: usize, name: &str) -> Option<usize> {
        let kind = self.tree.objects[owner].kind.as_ref()?;
        let ty = self.types.base(kind)?;
        (self.types.oracle.special(ty) == Some(Special::PropertyChanges)
            && !matches!(self.tree.lookup(self.types, owner, name), Lookup::Found(_)))
        .then(|| self.tree.target(owner))
        .flatten()
    }

    /// Whether Ljós gives the type of `owner` a member `name` that Qt's
    /// table does not list (`font.scaleWithSystem`): the runtime has it,
    /// so it is no finding, as it is none of the compiler's.
    fn adds(&self, owner: usize, name: &str) -> bool {
        matches!(self.shapes[owner], Shape::Complete(base) if base.0.adds(name))
    }

    /// How sure a name the type of `owner` lacks is wrong: see
    /// [`missing_severity`].
    fn missing_severity(&self, owner: usize) -> Severity {
        match self.shapes[owner] {
            Shape::Complete(base) => missing_severity(base),
            _ => Severity::Error,
        }
    }

    fn type_label(&self, owner: usize) -> String {
        self.tree.objects[owner].name.clone()
    }

    fn handler(&mut self, owner: usize, name: &str, at: u32) {
        // A property may really be called `onFoo`. Only a name the type
        // definitely lacks is reported; one it may have is not.
        if !matches!(self.tree.lookup(self.types, owner, name), Lookup::Missing) {
            return;
        }
        let Some(signal) = handled(name) else { return };
        if !matches!(
            self.tree.lookup_signal(self.types, owner, &signal),
            Lookup::Missing
        ) || self.adds(owner, &signal)
        {
            return;
        }
        // `on<Property>Changed` handles the property's change, whatever
        // its notifying signal is called, as the compiler takes it.
        let changed = signal.strip_suffix("Changed").is_some_and(|property| {
            matches!(
                self.tree.lookup(self.types, owner, property),
                Lookup::Found(Member::Property(_))
            )
        });
        if changed {
            return;
        }
        let mut message = format!(
            "`{name}` handles the signal `{signal}`, which `{}` does not have",
            self.type_label(owner)
        );
        if let Shape::Complete(base) = self.shapes[owner] {
            let mut names: Vec<&str> = Vec::new();
            let mut ty = Some(base.0);
            while let Some(t) = ty {
                names.extend(t.signals.iter().map(|s| s.name));
                ty = t.prototype();
            }
            if let Some(near) = closest(&signal, names) {
                message.push_str(&format!("; did you mean `{}`?", handler_name(near)));
            }
        }
        let severity = self.missing_severity(owner);
        self.report(at, severity, Code::UnknownSignal.as_str(), message);
    }

    fn property(&mut self, owner: usize, path: &[&'a str], at: u32) {
        let first = path[0];
        match self.tree.lookup(self.types, owner, first) {
            Lookup::Undecided => {}
            Lookup::Missing if self.adds(owner, first) => {}
            Lookup::Missing => {
                let mut message = format!(
                    "`{first}` is not a property of `{}`",
                    self.type_label(owner)
                );
                if let Shape::Complete(base) = self.shapes[owner] {
                    let mut names: Vec<&str> = Vec::new();
                    let mut ty = Some(base.0);
                    while let Some(t) = ty {
                        names.extend(t.properties.iter().map(|p| p.name));
                        ty = t.prototype();
                    }
                    if let Some(near) = closest(first, names) {
                        message.push_str(&format!("; did you mean `{near}`?"));
                    }
                }
                let severity = self.missing_severity(owner);
                self.report(at, severity, Code::UnknownProperty.as_str(), message);
            }
            Lookup::Found(Member::Property(mut property)) => {
                // `anchors.fill`, `font.pixelSize`: the rest are properties
                // of the value, where its type is known.
                for (i, name) in path.iter().enumerate().skip(1) {
                    if is_handler_name(name) {
                        return;
                    }
                    let Some(value) = property.value else { return };
                    // A value type whose members the table does not list in
                    // full may have the name.
                    if !self.types.oracle.members_known(value) {
                        return;
                    }
                    match self.types.oracle.property(value, name) {
                        Some(next) => property = next,
                        // Not assignable perhaps, but not unknown either.
                        None if self.types.oracle.signal(value, name).is_some()
                            || self.types.oracle.has_method(value, name)
                            || value.0.adds(name) =>
                        {
                            return;
                        }
                        None => {
                            let mut names: Vec<&str> = Vec::new();
                            let mut ty = Some(value.0);
                            while let Some(t) = ty {
                                names.extend(t.properties.iter().map(|p| p.name));
                                ty = t.prototype();
                            }
                            let mut message =
                                format!("`{name}` is not a property of `{}`", path[..i].join("."));
                            if let Some(near) = closest(name, names) {
                                message.push_str(&format!("; did you mean `{near}`?"));
                            }
                            self.report(
                                at,
                                missing_severity(value),
                                Code::UnknownProperty.as_str(),
                                message,
                            );
                            return;
                        }
                    }
                }
            }
            Lookup::Found(_) => {}
        }
    }

    /// `Layout.fillWidth`, `Keys.onPressed`: the property of what the type
    /// attaches.
    fn attached(&mut self, path: &[&str], at: u32) {
        if !self.check_types {
            return;
        }
        // `C.Layout.fillWidth` has a qualified type; `Layout.fillWidth` not.
        let (type_parts, rest) = if path.len() >= 3 {
            (&path[..2], &path[2..])
        } else {
            (&path[..1], &path[1..])
        };
        let Some(found) = self.types.find(type_parts) else {
            // A qualified name that does not resolve is more likely a
            // namespace the file did not import than a typo: stay quiet.
            if type_parts.len() == 1 && !(self.native)(type_parts[0]) {
                self.report(
                    at,
                    Severity::Error,
                    Code::UnknownType.as_str(),
                    format!(
                        "`{}` is not a type provided by any module this file imports, so `{}` \
                         is not an attached property; is the module imported?",
                        type_parts[0],
                        path.join(".")
                    ),
                );
            }
            return;
        };
        let (Kind::Builtin(ty), Some(member)) = (found.kind, rest.first()) else {
            return;
        };
        if ty.0.attached().is_none() {
            return;
        }
        let attaches = |name: &str| {
            ty.0.attaches(name) || ty.0.attached().is_some_and(|attached| attached.adds(name))
        };
        let known = attaches(member) || handled(member).is_some_and(|signal| attaches(&signal));
        if !known {
            self.report(
                at,
                missing_severity(ty),
                Code::UnknownProperty.as_str(),
                format!(
                    "`{member}` is not a member of what `{}` attaches",
                    type_parts.join(".")
                ),
            );
        }
    }
}

/// A name `ty` lacks is an error where Qt's table describes it, as Qt
/// fails to load the file and the compiler stops it. Where only Ljós's own
/// table does, which may lack what Qt has, it is a warning, as the
/// compiler's is.
fn missing_severity(ty: QtType) -> Severity {
    if ty.0.is_own {
        Severity::Warning
    } else {
        Severity::Error
    }
}

fn last_part(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// `clicked` is handled by `onClicked`.
fn handler_name(signal: &str) -> String {
    let mut chars = signal.chars();
    let first = chars.next().map(|c| c.to_ascii_uppercase());
    format!("on{}{}", first.unwrap_or_default(), chars.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_name_capitalises() {
        assert_eq!(handler_name("clicked"), "onClicked");
        assert_eq!(handler_name("widthChanged"), "onWidthChanged");
    }

    /// The codes a check emits are the front end's own, so they cannot drift
    /// from its errors; a reworded message changes nothing here.
    #[test]
    fn front_end_errors_carry_their_codes() {
        let source = "import QtQuick\nItem { id: a; Rectangle { id: a } Nope {} property alias x: missing }\n";
        let mut project = Project::new();
        project.add("T", source).unwrap();
        let oracle = QtTypeOracle;
        let types = Types::new(&project, &oracle, "T");
        let allocator = Allocator::default();
        let parsed = qml_syntax::parse(&allocator, source);
        let mut errors = Vec::new();
        Tree::analyze(&parsed.document, types, &mut errors);
        let codes: Vec<Code> = errors.iter().map(|e| e.code).collect();
        for want in [
            Code::UnknownType,
            Code::DuplicateId,
            Code::UnknownAliasTarget,
        ] {
            assert!(codes.contains(&want), "{codes:?}");
        }
        // What the type error says it is about is read off the source.
        let unknown = errors
            .iter()
            .find(|e| e.code == Code::UnknownType)
            .expect("a type error");
        assert_eq!(
            &source[unknown.span.start as usize..unknown.span.end as usize],
            "Nope"
        );
    }
}
