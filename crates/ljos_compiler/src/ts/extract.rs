//! A file in Ljós's API as a TypeScript module, so that TypeScript can say
//! what it makes of the file's scripts (docs/DIALECT.md M4: TypeScript
//! accepts a file iff the checker does).
//!
//! Each object of the file is a constant of an interface that extends its
//! type's declaration with what the object declares. Each script is a
//! function whose locals are what its free names resolve to, as the
//! compiler resolves them: an id is the object, a property of the scope or
//! of the root is read off it (`const` when the script may not write it).
//! A binding returns the type of its property, a handler takes the
//! arguments of its signal, and a QML function stays the function it is.
//! The scripts are written as they are in the file, each line on a line of
//! its own, so that what TypeScript says of a line is said of the file's.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use qml_front::names::Names;
use qml_front::{
    QtType,
    names::Target,
    scope::{Tree, handled, is_group},
    types::{Kind, Types},
};
use qml_syntax::{
    Allocator, GetSpan,
    ast::{
        QmlBindingValue, QmlDocument, QmlImportSource, QmlMember, QmlObject, QmlSignalDeclaration,
        QmlTypeName,
    },
    js::{Expression, Function},
};

use super::dts::Api;
use crate::{
    Project,
    js::{self, Form},
};

/// Where a line of the TypeScript module came from in the QML file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Origin {
    /// The file's line, from 1.
    pub line: u32,
    /// The file's column of the line's first character, from 1.
    pub column: u32,
    /// The line is the file's text, so a column on it is a column of the
    /// file; otherwise it was written for the file's sake and all of it is
    /// said of `column`.
    pub verbatim: bool,
}

/// A file as TypeScript, and where each line of it came from.
#[derive(Debug, Clone, Default)]
pub struct Extracted {
    pub text: String,
    /// One for each line of `text`.
    pub origins: Vec<Option<Origin>>,
}

impl Extracted {
    /// The file's line and column of TypeScript's `line` and `column` (both
    /// from 1).
    pub fn position(&self, line: u32, column: u32) -> Option<(u32, u32)> {
        let origin = (*self.origins.get(line.checked_sub(1)? as usize)?)?;
        Some(if origin.verbatim {
            (origin.line, origin.column + column.saturating_sub(1))
        } else {
            (origin.line, origin.column)
        })
    }

    fn line(&mut self, text: &str, origin: Option<Origin>) {
        self.text.push_str(text);
        self.text.push('\n');
        self.origins.push(origin);
    }

    /// The file's text from `start`, each of its lines on one of these.
    fn verbatim(&mut self, text: &str, at: (u32, u32)) {
        for (i, line) in text.split('\n').enumerate() {
            let origin = Origin {
                line: at.0 + i as u32,
                column: if i == 0 { at.1 } else { 1 },
                verbatim: true,
            };
            self.line(line.trim_end_matches('\r'), Some(origin));
        }
    }
}

/// Whether the file is TypeScript's to check: written in Ljós's API, and not
/// marked as still Qt's JavaScript (`pragma Scripts: JavaScript`), as the
/// checker reads it.
pub fn is_dialect(document: &QmlDocument<'_>) -> bool {
    crate::compile::is_dialect(document) && !crate::compile::scripts_are_javascript(document)
}

/// The file `key` as TypeScript, which reads its types from the declarations
/// at `declarations` and the project's TypeScript modules under `modules`
/// (paths relative to the module). None when the file is not in Ljós's API
/// or does not parse: the checker says why.
pub fn extract(
    project: &Project<qml_front::QtTypeOracle>,
    api: &Api,
    key: &str,
    declarations: &str,
    modules: &str,
) -> Option<Extracted> {
    let source = project.source(key)?;
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if parsed.panicked || !is_dialect(&parsed.document) {
        return None;
    }
    let types = Types::new(&project.front, &project.oracle, key);
    let tree = Tree::analyze(&parsed.document, types, &mut Vec::new());
    let mut writer = Writer {
        api,
        source,
        starts: line_starts(source),
        tree: &tree,
        names: Names { tree: &tree, types },
        out: Extracted::default(),
        next: 0,
        objects: Vec::new(),
        signals: HashMap::new(),
        namespaces: Vec::new(),
    };
    // A module the file imports is imported as it is named, so that its
    // values and its types are TypeScript's to check. The names are known
    // before any declaration's type is written.
    let mut imports = Vec::new();
    for import in &parsed.document.imports {
        if let QmlImportSource::Path(path) = &import.source
            && crate::script::module::is_module(path)
            && let Some(name) = &import.alias
        {
            let module = project.front.import_dir(key, path);
            let line = format!("import * as {} from \"{modules}/{module}\";", name.name);
            imports.push((line, writer.origin(import.span.start)));
            writer.namespaces.push(name.name.to_string());
        }
    }
    writer.collect_signals(&parsed.document.root);
    writer
        .out
        .line(&format!("/// <reference path=\"{declarations}\" />"), None);
    writer.out.line(
        &format!("// {key}.qml, for TypeScript: written by `ljos check --tsc`."),
        None,
    );
    writer.out.line("export {};", None);
    for (line, origin) in imports {
        writer.out.line(&line, origin);
    }
    writer.object(&parsed.document.root);
    let objects = std::mem::take(&mut writer.objects);
    for object in objects {
        writer.declare(object);
    }
    Some(writer.out)
}

fn line_starts(source: &str) -> Vec<u32> {
    std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i as u32 + 1),
        )
        .collect()
}

/// What an object declares, for its interface.
#[derive(Default)]
struct Declared {
    index: usize,
    at: (u32, u32),
    /// Name, TypeScript type, readonly, where.
    properties: Vec<(String, String, bool, (u32, u32))>,
    /// Name, parameters, where.
    signals: Vec<(String, String, (u32, u32))>,
    /// Name, the constant that is the function.
    functions: Vec<(String, String)>,
}

struct Writer<'w, 'a, 'p> {
    api: &'w Api,
    source: &'a str,
    starts: Vec<u32>,
    tree: &'w Tree<'a, QtType>,
    names: Names<'w, 'a, 'p, qml_front::QtTypeOracle>,
    out: Extracted,
    next: usize,
    objects: Vec<Declared>,
    /// The parameters of each signal the file declares, by object and name.
    signals: HashMap<(usize, String), String>,
    /// The names the file imports TypeScript modules as.
    namespaces: Vec<String>,
}

impl<'a> Writer<'_, 'a, '_> {
    /// The line and column of a byte offset of the file.
    fn at(&self, offset: u32) -> (u32, u32) {
        let line = self.starts.partition_point(|s| *s <= offset);
        let start = self.starts[line - 1] as usize;
        let mut end = (offset as usize).min(self.source.len());
        while !self.source.is_char_boundary(end) {
            end -= 1;
        }
        let column = self.source[start..end].chars().count() + 1;
        (line as u32, column as u32)
    }

    fn origin(&self, offset: u32) -> Option<Origin> {
        let (line, column) = self.at(offset);
        Some(Origin {
            line,
            column,
            verbatim: false,
        })
    }

    fn fresh(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("__{prefix}{}", self.next)
    }

    /// The type of what the object is made of, as TypeScript names it; None
    /// when the declarations have no such type.
    fn native(&self, index: usize) -> Option<String> {
        match self.tree.objects[index].kind.as_ref()? {
            Kind::Builtin(ty) => self.api.name_of(ty.0).map(str::to_string),
            Kind::Component(_) => None,
        }
    }

    fn native_type(&self, index: usize) -> Option<&'static qt_types::Type> {
        match self.tree.objects[index].kind.as_ref()? {
            Kind::Builtin(ty) => Some(ty.0),
            Kind::Component(_) => None,
        }
    }

    /// A QML type name of a declaration, as TypeScript's.
    fn declared_type(&self, ty: Option<&QmlTypeName<'_>>) -> String {
        let Some(ty) = ty else {
            return "unknown".to_string();
        };
        let name = ty.name.to_string();
        let module_typed = matches!(&ty.name.parts[..], [namespace, _] if self.namespaces.iter().any(|n| n == namespace));
        let mapped = if module_typed {
            name
        } else if name == "alias" {
            "unknown".to_string()
        } else {
            match self.api.written_type(&name) {
                // Written as it is so that TypeScript cannot find it, as the
                // checker cannot. A type the file finds and the declarations
                // do not have (one of the project's own: a component
                // declared inline, a file beside it, a type of a module of
                // the project's) is the checker's to type.
                Some(bare) if bare == name && self.names.types.find(&[&name]).is_some() => {
                    "unknown".to_string()
                }
                Some(written) => written,
                None => "never".to_string(),
            }
        };
        if ty.is_list {
            format!("readonly {mapped}[]")
        } else {
            mapped
        }
    }

    /// The signals the file declares, which a handler written before its
    /// declaration handles too.
    fn collect_signals(&mut self, object: &QmlObject<'a>) {
        let index = self.tree.index_of(object);
        for member in &object.members {
            match member {
                QmlMember::Signal(signal) => {
                    if let Some(index) = index {
                        let params = self.signal_params(signal);
                        self.signals
                            .insert((index, signal.name.name.to_string()), params);
                    }
                }
                QmlMember::Object(child) => self.collect_signals(child),
                QmlMember::InlineComponent(inline) => self.collect_signals(&inline.object),
                QmlMember::Binding(binding) => match &binding.value {
                    QmlBindingValue::Object(child) => self.collect_signals(child),
                    QmlBindingValue::Objects(children) => {
                        for child in children {
                            self.collect_signals(child);
                        }
                    }
                    _ => {}
                },
                QmlMember::Property(property) => match &property.value {
                    Some(QmlBindingValue::Object(child)) => self.collect_signals(child),
                    Some(QmlBindingValue::Objects(children)) => {
                        for child in children {
                            self.collect_signals(child);
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    fn signal_params(&self, signal: &QmlSignalDeclaration<'_>) -> String {
        signal
            .params
            .iter()
            .map(|param| {
                format!(
                    "{}: {}",
                    param.name.name,
                    self.declared_type(param.type_name.as_ref())
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn object(&mut self, object: &QmlObject<'a>) {
        let Some(index) = self.tree.index_of(object) else {
            return;
        };
        let mut declared = Declared {
            index,
            at: self.at(object.span.start),
            ..Declared::default()
        };
        self.members(index, object, &[], &mut declared);
        self.objects.push(declared);
    }

    fn members(
        &mut self,
        owner: usize,
        object: &QmlObject<'a>,
        group: &[&'a str],
        declared: &mut Declared,
    ) {
        for member in &object.members {
            match member {
                QmlMember::Object(child) if is_group(child) => {
                    let mut path = group.to_vec();
                    path.extend(child.type_name.parts.iter().copied());
                    self.members(owner, child, &path, declared);
                }
                QmlMember::Object(child) => self.object(child),
                QmlMember::Binding(binding) => {
                    let mut path = group.to_vec();
                    path.extend(binding.name.parts.iter().copied());
                    if path == ["id"] {
                        continue;
                    }
                    self.value(owner, &path, &binding.value, declared);
                }
                QmlMember::Property(property) => {
                    let name = property.name.name.to_string();
                    let ty = self.declared_type(property.type_name.as_ref());
                    let at = self.at(property.span.start);
                    declared
                        .properties
                        .push((name.clone(), ty, property.is_readonly, at));
                    if let Some(value) = &property.value {
                        self.value(owner, &[property.name.name.as_str()], value, declared);
                    }
                }
                QmlMember::Signal(signal) => {
                    let params = self.signal_params(signal);
                    let at = self.at(signal.span.start);
                    declared
                        .signals
                        .push((signal.name.name.to_string(), params, at));
                }
                QmlMember::Function(function) => self.function(owner, function, declared),
                QmlMember::InlineComponent(inline) => self.object(&inline.object),
                QmlMember::Enum(_) => {}
            }
        }
    }

    /// The locals a script starting at byte `start` has for its free names.
    fn locals(&mut self, free: &[js::Free], start: u32, given: &[String]) {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let written: BTreeSet<&str> = free
            .iter()
            .filter(|f| f.written)
            .map(|f| f.name.as_str())
            .collect();
        for name in free {
            let name_str = name.name.as_str();
            if given.iter().any(|g| g == name_str) || !seen.insert(name_str) {
                continue;
            }
            let offset = start + name.span.start;
            let origin = self.origin(offset);
            let Some(resolution) = self.names.lookup(name_str, offset) else {
                continue;
            };
            let line = match &resolution.target {
                Some(Target::Id(object)) => format!("const {name_str} = __o{object};"),
                Some(Target::Property {
                    object,
                    name: member,
                }) => {
                    let assignable = self.assignable(*object, member);
                    let binding = if assignable && written.contains(name_str) {
                        "let"
                    } else {
                        "const"
                    };
                    format!("{binding} {name_str} = __o{object}.{member};")
                }
                Some(Target::Context { name: role, .. }) => {
                    let ty = if role == "index" { "number" } else { "unknown" };
                    format!("const {name_str} = undefined as unknown as {ty};")
                }
                Some(Target::Type(_) | Target::Namespace(_) | Target::Global(_)) | None => {
                    continue;
                }
            };
            self.out.line(&format!("    {line}"), origin);
        }
    }

    /// Whether a script may write the member `name` of the object: a
    /// property that is not readonly.
    fn assignable(&self, object: usize, name: &str) -> bool {
        // What the file declares is found in its text; a type's in the table.
        let declared = &self.tree.objects[object].declared;
        if let Some(found) = declared.get(name) {
            return matches!(found, qml_front::scope::Declared::Property { .. });
        }
        self.native_type(object)
            .and_then(|ty| ty.property(name))
            .is_some_and(|p| !p.is_readonly)
    }

    /// The TypeScript type of the property a binding names, as an indexed
    /// access on the object's interface.
    fn binding_type(&self, owner: usize, path: &[&str]) -> Option<String> {
        if path.first()?.starts_with(|c: char| c.is_ascii_uppercase()) {
            // An attached type's: `Layout.fillWidth`.
            return None;
        }
        let mut ty = format!("__O{owner}");
        for part in path {
            let _ = write!(ty, "[{part:?}]");
        }
        Some(ty)
    }

    /// The parameters of the signal a handler on `path` handles, as
    /// TypeScript writes them; None when the signal is not known.
    fn handler_params(&self, owner: usize, path: &[&str], signal: &str) -> Option<String> {
        let (object, path) = match path.split_last() {
            Some((_, [])) => (owner, None),
            Some((_, group)) => (owner, Some(group)),
            None => return None,
        };
        if let Some(group) = path {
            if group == ["Component"] && matches!(signal, "completed" | "destruction") {
                return Some(String::new());
            }
            let attached = self.api.named(group.first()?)?.attached()?;
            let found = attached.signal(signal)?;
            return Some(self.api.parameters(found, attached));
        }
        let object = if self
            .native_type(object)
            .is_some_and(|ty| ty.name == "Connections")
        {
            self.tree.target(object)?
        } else {
            object
        };
        // What the file declares: its signals, and its properties' changes.
        if let Some(found) = self.objects_signal(object, signal) {
            return Some(found);
        }
        if let Some(property) = signal.strip_suffix("Changed")
            && self.tree.objects[object].declared.contains_key(property)
        {
            return Some(String::new());
        }
        let ty = self.native_type(object)?;
        let found = ty.signal(signal)?;
        Some(self.api.parameters(found, ty))
    }

    fn objects_signal(&self, object: usize, signal: &str) -> Option<String> {
        self.signals.get(&(object, signal.to_string())).cloned()
    }

    fn value(
        &mut self,
        owner: usize,
        path: &[&str],
        value: &QmlBindingValue<'a>,
        declared: &mut Declared,
    ) {
        let _ = declared;
        let (form, span) = match value {
            QmlBindingValue::Expression(e) => (Form::Expression, e.span()),
            QmlBindingValue::Statement(s) => (Form::Statement, s.span()),
            QmlBindingValue::Object(object) => {
                self.object(object);
                return;
            }
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.object(object);
                }
                return;
            }
        };
        let last = path.last().copied().unwrap_or_default();
        let signal = handled(last);
        let text = qml_syntax::text(self.source, span);
        let at = self.at(span.start);
        let name_at = self.origin(span.start);
        match signal {
            Some(signal) => {
                let params = self.handler_params(owner, path, &signal);
                let names: Vec<String> = params
                    .as_deref()
                    .map(|p| {
                        p.split(", ")
                            .filter(|p| !p.is_empty())
                            .filter_map(|p| p.split(':').next())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let Ok(analysis) = js::analyze(text, form, &names) else {
                    return;
                };
                let function = self.fresh("h");
                let is_function = matches!(
                    value,
                    QmlBindingValue::Expression(
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                    )
                );
                let signature = params
                    .clone()
                    .unwrap_or_else(|| "...args: readonly unknown[]".into());
                if is_function {
                    self.out
                        .line(&format!("function {function}(): void {{"), name_at);
                    self.locals(&analysis.free, span.start, &[]);
                    self.out.line(
                        &format!("    const handler: ({signature}) => void = ("),
                        name_at,
                    );
                    self.out.verbatim(text, at);
                    self.out.line(");", name_at);
                    self.out.line("    void handler;", name_at);
                } else {
                    self.out.line(
                        &format!("function {function}(this: void, {signature}): void {{"),
                        name_at,
                    );
                    self.locals(&analysis.free, span.start, &names);
                    if form == Form::Expression {
                        self.out.line("    void (", name_at);
                        self.out.verbatim(text, at);
                        self.out.line(");", name_at);
                    } else {
                        self.out.verbatim(text, at);
                    }
                }
                self.out.line("}", name_at);
            }
            None => {
                let Ok(analysis) = js::analyze(text, form, &[]) else {
                    return;
                };
                let function = self.fresh("b");
                let ty = self
                    .binding_type(owner, path)
                    .map(|ty| format!(": {ty}"))
                    .unwrap_or_default();
                self.out
                    .line(&format!("function {function}(this: void){ty} {{"), name_at);
                self.locals(&analysis.free, span.start, &[]);
                if form == Form::Expression {
                    self.out.line("    return (", name_at);
                    self.out.verbatim(text, at);
                    self.out.line(");", name_at);
                } else {
                    self.out.verbatim(text, at);
                }
                self.out.line("}", name_at);
            }
        }
    }

    fn function(&mut self, owner: usize, function: &Function<'a>, declared: &mut Declared) {
        let Some(id) = &function.id else {
            return;
        };
        let text = qml_syntax::text(self.source, function.span);
        let Ok(analysis) = js::analyze(text, Form::Function, &[]) else {
            return;
        };
        let _ = owner;
        let constant = self.fresh("f");
        let at = self.at(function.span.start);
        let origin = self.origin(function.span.start);
        self.out
            .line(&format!("const {constant} = (() => {{"), origin);
        self.locals(&analysis.free, function.span.start, &[]);
        self.out.line("    return (", origin);
        self.out.verbatim(text, at);
        self.out.line(");", origin);
        self.out.line("})();", origin);
        declared.functions.push((id.name.to_string(), constant));
    }

    /// The object's interface and constant.
    fn declare(&mut self, declared: Declared) {
        let index = declared.index;
        let origin = Some(Origin {
            line: declared.at.0,
            column: declared.at.1,
            verbatim: false,
        });
        let (extends, open) = match self.native(index) {
            Some(native) => (native, false),
            None => ("QtObject".to_string(), true),
        };
        self.out.line(
            &format!("interface __O{index} extends {extends} {{"),
            origin,
        );
        if open {
            // Another file's type, or one the declarations lack: whatever
            // it has, nothing is known of it.
            self.out.line("    [member: string]: unknown;", origin);
        }
        for (name, ty, readonly, at) in &declared.properties {
            let readonly = if *readonly { "readonly " } else { "" };
            let origin = Some(Origin {
                line: at.0,
                column: at.1,
                verbatim: false,
            });
            self.out
                .line(&format!("    {readonly}{name}: {ty};"), origin);
        }
        for (name, params, at) in &declared.signals {
            let origin = Some(Origin {
                line: at.0,
                column: at.1,
                verbatim: false,
            });
            self.out
                .line(&format!("    {name}({params}): void;"), origin);
        }
        for (name, constant) in &declared.functions {
            self.out
                .line(&format!("    readonly {name}: typeof {constant};"), origin);
        }
        self.out.line("}", origin);
        self.out
            .line(&format!("declare const __o{index}: __O{index};"), origin);
    }
}
