//! One file, from its QML tree to [`CompiledFile`].
//!
//! The walk follows the front end's [`Tree`]: every object of the tree is an
//! [`ObjectDef`] of the component (context) the tree puts it in, in the same
//! order. A value that is the same every time it is read becomes a
//! [`Literal`]; anything else becomes a function of the file's module, whose
//! parameters are what its free names resolve to (tier 0, then tier 2 of
//! ARCHITECTURE.md §5.4).

use std::collections::{HashMap, HashSet};

use ljos_ir::*;
use qml_front::{
    Special, TypeOracle,
    names::{Global, Names, Target},
    project::Source,
    scope::{Tree, alias_target, handled, is_group},
    types::{Found, Kind, Origin, Types},
};
use qml_syntax::{
    Allocator, GetSpan, Span,
    ast::{
        QmlBindingValue, QmlDocument, QmlImportSource, QmlMember, QmlObject, QmlPropertyDeclaration,
    },
    js::{Expression, Function, ObjectPropertyKind, PropertyKind, UnaryOperator},
};

use crate::{
    Compiled, Diagnostic, Project,
    js::{self, ArgValue, Form},
    oracle::is_color,
};

/// Compiles the file `key` of the project.
///
/// A file with errors still compiles, as far as it can: what did not parse is
/// left out and what is unknown is marked so, and the errors come with the
/// result. Only a file the parser gave up on is an `Err`.
pub fn compile_file<O: TypeOracle>(
    project: &Project<O>,
    key: &str,
) -> Result<Compiled, Vec<Diagnostic>> {
    let Some(source) = project.source(key) else {
        return Err(vec![Diagnostic::error(
            format!("`{key}` is not a file of the project"),
            SourceSpan::default(),
        )]);
    };
    let lines = Lines::new(source);
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    let mut diagnostics: Vec<Diagnostic> = parsed
        .diagnostics
        .iter()
        .map(|d| Diagnostic::error(d.message.clone(), lines.span(source, d.span)))
        .collect();
    if parsed.panicked {
        if diagnostics.is_empty() {
            diagnostics.push(Diagnostic::error(
                "the file could not be parsed",
                SourceSpan::default(),
            ));
        }
        return Err(diagnostics);
    }
    let types = Types::new(&project.front, &project.oracle, key);
    let mut errors = Vec::new();
    let tree = Tree::analyze(&parsed.document, types, &mut errors);
    diagnostics.extend(
        errors
            .into_iter()
            .map(|e| Diagnostic::error(e.message, lines.span(source, e.span))),
    );
    let mut compiler = Compiler::new(project, key, source, lines, types, &tree);
    compiler.diagnostics = diagnostics;
    compiler.collect_enums(&parsed.document.root, None);
    compiler.object(&parsed.document.root, None);
    Ok(compiler.finish(&parsed.document))
}

/// Byte offsets to lines and columns.
struct Lines {
    starts: Vec<u32>,
}

impl Lines {
    fn new(source: &str) -> Self {
        let starts = std::iter::once(0)
            .chain(
                source
                    .bytes()
                    .enumerate()
                    .filter(|(_, b)| *b == b'\n')
                    .map(|(i, _)| i as u32 + 1),
            )
            .collect();
        Self { starts }
    }

    fn span(&self, source: &str, span: Span) -> SourceSpan {
        let start = (span.start as usize).min(source.len()) as u32;
        let line = self.starts.partition_point(|s| *s <= start);
        let line_start = self.starts[line - 1] as usize;
        let mut end = start as usize;
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        let column = source[line_start..end].chars().count() + 1;
        SourceSpan {
            start: span.start,
            end: span.end,
            line: line as u32,
            column: column as u32,
        }
    }
}

/// An enum a component of the file declares: its inline component (None
/// for the file's own), its name and its keys.
type FileEnum = (Option<String>, String, Vec<(String, i64)>);

/// Where a member's name puts it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Own,
    /// `anchors.left`: the group's path.
    Group(Vec<String>),
    /// `Keys.onPressed`: the type's parts, `["Keys"]` or `["C", "Layout"]`.
    Attached(Vec<String>),
}

/// What a free name becomes in a function.
#[derive(Debug, Clone)]
struct Plan {
    resolved: Resolved,
    param: Option<Param>,
    /// The name is read through this parameter: `width` is `$scope.width`.
    rewrite: Option<String>,
}

struct Compiler<'t, 'a, 'p, O: TypeOracle> {
    project: &'p Project<O>,
    key: &'p str,
    source: &'a str,
    lines: Lines,
    types: Types<'p, O>,
    tree: &'t Tree<'a, O::Type>,
    names: Names<'t, 'a, 'p, O>,
    /// Each object of the tree, as an object of a component.
    place: Vec<ObjRef>,
    components: Vec<ComponentDef>,
    /// For a template, the object it is written in and the property.
    templates: Vec<Option<(ObjRef, String)>>,
    /// How many objects of a type a parent has with neither id nor name.
    typed: HashMap<(CompIdx, Option<ObjIdx>, String), u32>,
    exprs: Vec<Binding>,
    functions: Vec<(JsFunction, String)>,
    translations: Vec<Translatable>,
    diagnostics: Vec<Diagnostic>,
    /// `qsTr`'s context: the file's stem, or `pragma Translator`.
    context: String,
    /// The type a property is declared with, by object and name.
    declared: HashMap<(usize, String), String>,
    /// The enums of the file's components: inline name, enum, keys.
    enums: Vec<FileEnum>,
    /// The `Qt` global, for its enums.
    qt: Option<O::Type>,
    warned: HashSet<String>,
}

impl<'t, 'a, 'p, O: TypeOracle> Compiler<'t, 'a, 'p, O> {
    fn new(
        project: &'p Project<O>,
        key: &'p str,
        source: &'a str,
        lines: Lines,
        types: Types<'p, O>,
        tree: &'t Tree<'a, O::Type>,
    ) -> Self {
        let mut counts = vec![0u32; tree.contexts.len()];
        let place = tree
            .objects
            .iter()
            .map(|object| {
                let context = object.context;
                let at = ObjRef {
                    component: CompIdx(context as u32),
                    object: ObjIdx(counts[context]),
                };
                counts[context] += 1;
                at
            })
            .collect();
        let components = counts
            .iter()
            .map(|count| ComponentDef {
                name: String::new(),
                id: String::new(),
                kind: ComponentKind::File,
                objects: vec![ObjectDef::default(); *count as usize],
                signature: ComponentSignature([0; 32]),
            })
            .collect();
        let oracle = &project.oracle;
        let qt = oracle
            .module_type("QtQml", "Qt")
            .or_else(|| oracle.module_type("QML", "Qt"));
        Self {
            project,
            key,
            source,
            lines,
            types,
            tree,
            names: Names { tree, types },
            place,
            templates: vec![None; tree.contexts.len()],
            components,
            typed: HashMap::new(),
            exprs: Vec::new(),
            functions: Vec::new(),
            translations: Vec::new(),
            diagnostics: Vec::new(),
            context: stem(key).to_string(),
            declared: HashMap::new(),
            enums: Vec::new(),
            qt,
            warned: HashSet::new(),
        }
    }

    fn span(&self, span: Span) -> SourceSpan {
        self.lines.span(self.source, span)
    }

    fn def_mut(&mut self, at: ObjRef) -> &mut ObjectDef {
        &mut self.components[at.component.index()].objects[at.object.index()]
    }

    /// The enums the file declares, with their numbers: the front end's
    /// summary has their keys only.
    fn collect_enums(&mut self, object: &QmlObject<'a>, inline: Option<&str>) {
        for member in &object.members {
            match member {
                QmlMember::Enum(declaration) => self.enums.push((
                    inline.map(str::to_string),
                    declaration.name.name.to_string(),
                    declaration
                        .members
                        .iter()
                        .map(|m| (m.name.name.to_string(), m.value))
                        .collect(),
                )),
                QmlMember::InlineComponent(component) => {
                    self.collect_enums(&component.object, Some(component.name.name.as_str()));
                }
                QmlMember::Object(child) => self.collect_enums(child, inline),
                _ => {}
            }
        }
    }

    // ---- objects ----------------------------------------------------------

    /// Lowers an object and everything in it; its index in the tree.
    fn object(&mut self, object: &QmlObject<'a>, parent: Option<usize>) -> Option<usize> {
        let index = self.tree.index_of(object)?;
        let at = self.place[index];
        let ty = self.object_type(index);
        let parent = parent
            .map(|p| self.place[p])
            .filter(|p| p.component == at.component)
            .map(|p| p.object);
        let span = self.span(object.span);
        let id = self.tree.objects[index].id.map(str::to_string);
        let def = self.def_mut(at);
        def.ty = ty;
        def.id = id;
        def.on = object
            .on
            .as_ref()
            .map(|on| on.parts.iter().map(|p| p.to_string()).collect());
        def.span = span;
        def.parent = parent;
        for member in &object.members {
            if let QmlMember::Property(declaration) = member
                && let Some(type_name) = &declaration.type_name
            {
                self.declared.insert(
                    (index, declaration.name.name.to_string()),
                    type_name.name.to_string(),
                );
            }
        }
        self.members(index, object, &[]);
        self.key_of(index, at, parent);
        Some(index)
    }

    /// The object's step of its key, now that its `objectName` is known.
    fn key_of(&mut self, index: usize, at: ObjRef, parent: Option<ObjIdx>) {
        let def = &self.components[at.component.index()].objects[at.object.index()];
        let named = def.members.assignments.iter().find_map(|a| match &a.value {
            Literal::String(name) if a.name == "objectName" => Some(name.clone()),
            _ => None,
        });
        let key = match (&def.id, named) {
            (Some(id), _) => KeySeg::Id(id.clone()),
            (None, Some(name)) => KeySeg::Named(name),
            (None, None) => {
                let ty = self.tree.objects[index].name.clone();
                let nth = self
                    .typed
                    .entry((at.component, parent, ty.clone()))
                    .or_insert(0);
                let key = KeySeg::Typed { ty, nth: *nth };
                *nth += 1;
                key
            }
        };
        self.def_mut(at).key = key;
    }

    fn members(&mut self, owner: usize, object: &QmlObject<'a>, group: &[&'a str]) {
        let at = self.place[owner];
        for member in &object.members {
            match member {
                QmlMember::Object(child) if is_group(child) => {
                    let mut path = group.to_vec();
                    path.extend(child.type_name.parts.iter().copied());
                    self.members(owner, child, &path);
                }
                QmlMember::Object(child) => {
                    if let Some(value) = self.object_value(owner, child, "default") {
                        self.def_mut(at).children.push(value);
                    }
                }
                QmlMember::Binding(binding) => {
                    let mut path = group.to_vec();
                    path.extend(binding.name.parts.iter().copied());
                    if path == ["id"] {
                        continue;
                    }
                    let (slot, name) = self.slot(&path, binding.name.span);
                    self.value(owner, slot, name, binding.name.span, &binding.value, true);
                }
                QmlMember::Property(declaration) => self.property(owner, declaration),
                QmlMember::Signal(signal) => {
                    let declaration = SignalDecl {
                        name: signal.name.name.to_string(),
                        params: signal
                            .params
                            .iter()
                            .map(|param| SignalParam {
                                name: param.name.name.to_string(),
                                ty: param.type_name.as_ref().map(|t| t.name.to_string()),
                            })
                            .collect(),
                    };
                    self.def_mut(at).signals.push(declaration);
                }
                QmlMember::Function(function) => self.method(owner, function),
                QmlMember::InlineComponent(inline) => {
                    if let Some(index) = self.object(&inline.object, None) {
                        let component = &mut self.components[self.place[index].component.index()];
                        component.name = inline.name.name.to_string();
                        component.id = format!("{}#{}", self.key, component.name);
                    }
                }
                QmlMember::Enum(declaration) => {
                    let declaration = EnumDecl {
                        name: declaration.name.name.to_string(),
                        keys: declaration
                            .members
                            .iter()
                            .map(|m| (m.name.name.to_string(), m.value))
                            .collect(),
                    };
                    self.def_mut(at).enums.push(declaration);
                }
            }
        }
    }

    /// An object written where a value goes: one of the component, or the
    /// root of a template.
    fn object_value(
        &mut self,
        owner: usize,
        child: &QmlObject<'a>,
        property: &str,
    ) -> Option<ObjectValue> {
        let index = self.object(child, Some(owner))?;
        let at = self.place[index];
        if at.component == self.place[owner].component {
            return Some(ObjectValue::Object(at.object));
        }
        self.templates[at.component.index()] = Some((self.place[owner], property.to_string()));
        Some(ObjectValue::Component(at.component))
    }

    fn property(&mut self, owner: usize, declaration: &QmlPropertyDeclaration<'a>) {
        let at = self.place[owner];
        let name = declaration.name.name.to_string();
        let Some(type_name) = &declaration.type_name else {
            self.def_mut(at).required.push(name);
            return;
        };
        let is_alias = type_name.name.as_simple() == Some("alias");
        let alias = is_alias
            .then(|| alias_target(declaration))
            .flatten()
            .map(|(id, path)| AliasTarget {
                id: id.to_string(),
                object: self.id_in_sight(owner, id),
                path: path
                    .split('.')
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect(),
            });
        let span = self.span(declaration.span);
        self.def_mut(at).properties.push(PropertyDecl {
            name: name.clone(),
            ty: type_name.name.to_string(),
            is_list: type_name.is_list,
            is_default: declaration.is_default,
            is_readonly: declaration.is_readonly,
            is_required: declaration.is_required,
            alias,
            span,
        });
        if !is_alias && let Some(value) = &declaration.value {
            // Qt places such a binding at the declaration.
            self.value(owner, Slot::Own, name, declaration.span, value, false);
        }
    }

    /// The object `id` names from the object `owner`, nearest context first.
    fn id_in_sight(&self, owner: usize, id: &str) -> Option<ObjRef> {
        let mut context = Some(self.tree.objects[owner].context);
        while let Some(at) = context {
            if let Some(object) = self.tree.id_in(at, id) {
                return Some(self.place[object]);
            }
            context = self.tree.contexts[at].outer;
        }
        None
    }

    /// Where a member written `path` goes, and its name there.
    fn slot(&mut self, path: &[&str], span: Span) -> (Slot, String) {
        let owned = |parts: &[&str]| parts.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        match path {
            [name] => (Slot::Own, name.to_string()),
            [first, ..] if first.starts_with(|c: char| c.is_ascii_uppercase()) => {
                let qualified = path.len() >= 3
                    && path[1].starts_with(|c: char| c.is_ascii_uppercase())
                    && self.types.namespace(first).is_some();
                let n = if qualified { 2 } else { 1 };
                if self.types.find(&path[..n]).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        format!(
                            "`{}` is not a type of anything the file imports",
                            path[..n].join(".")
                        ),
                        self.lines.span(self.source, span),
                    ));
                }
                (Slot::Attached(owned(&path[..n])), path[n..].join("."))
            }
            _ => (
                Slot::Group(owned(&path[..path.len() - 1])),
                path[path.len() - 1].to_string(),
            ),
        }
    }

    fn members_mut(&mut self, at: ObjRef, slot: &Slot) -> &mut Members {
        let ty = match slot {
            Slot::Attached(parts) => Some(self.type_of_parts(parts)),
            _ => None,
        };
        let def = self.def_mut(at);
        match slot {
            Slot::Own => &mut def.members,
            Slot::Group(path) => {
                let index = match def.grouped.iter().position(|g| &g.path == path) {
                    Some(index) => index,
                    None => {
                        def.grouped.push(Group {
                            path: path.clone(),
                            members: Members::default(),
                        });
                        def.grouped.len() - 1
                    }
                };
                &mut def.grouped[index].members
            }
            Slot::Attached(parts) => {
                let written = parts.join(".");
                let index = match def.attached.iter().position(|a| a.written == written) {
                    Some(index) => index,
                    None => {
                        def.attached.push(Attached {
                            ty: ty.unwrap_or_default(),
                            written,
                            members: Members::default(),
                        });
                        def.attached.len() - 1
                    }
                };
                &mut def.attached[index].members
            }
        }
    }

    /// What a member is given: a literal, an expression, a handler or
    /// objects.
    fn value(
        &mut self,
        owner: usize,
        slot: Slot,
        name: String,
        name_span: Span,
        value: &QmlBindingValue<'a>,
        may_handle: bool,
    ) {
        let at = self.place[owner];
        let name_span = self.span(name_span);
        let signal = handled(&name).filter(|_| may_handle);
        let target = self.describe(owner, &slot, &name);
        match value {
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) if signal.is_some() => {
                let signal = signal.unwrap_or_default();
                let (form, span) = match value {
                    QmlBindingValue::Expression(e) => (Form::Expression, e.span()),
                    QmlBindingValue::Statement(s) => (Form::Statement, s.span()),
                    _ => unreachable!("matched above"),
                };
                let args = self.signal_args(owner, &slot, &signal);
                let expr = self.expr(BindingKind::Handler, form, span, args, &target);
                self.members_mut(at, &slot)
                    .handlers
                    .push(Handler { signal, expr });
            }
            QmlBindingValue::Expression(expression) => {
                let color = self.is_color(owner, &slot, &name);
                if let Some(literal) = self.literal(expression, color) {
                    let span = self.span(expression.span());
                    self.members_mut(at, &slot).assignments.push(Assignment {
                        name,
                        value: literal,
                        span,
                    });
                } else {
                    let expr = self.expr(
                        BindingKind::Property,
                        Form::Expression,
                        expression.span(),
                        Some(Vec::new()),
                        &target,
                    );
                    self.members_mut(at, &slot).bindings.push(PropertyBinding {
                        name,
                        expr,
                        name_span,
                    });
                }
            }
            QmlBindingValue::Statement(statement) => {
                let expr = self.expr(
                    BindingKind::Property,
                    Form::Statement,
                    statement.span(),
                    Some(Vec::new()),
                    &target,
                );
                self.members_mut(at, &slot).bindings.push(PropertyBinding {
                    name,
                    expr,
                    name_span,
                });
            }
            QmlBindingValue::Object(object) => {
                let values = self
                    .object_value(owner, object, &name)
                    .into_iter()
                    .collect();
                self.members_mut(at, &slot).objects.push(ObjectAssignment {
                    name,
                    values,
                    is_list: false,
                });
            }
            QmlBindingValue::Objects(objects) => {
                let values = objects
                    .iter()
                    .filter_map(|object| self.object_value(owner, object, &name))
                    .collect();
                self.members_mut(at, &slot).objects.push(ObjectAssignment {
                    name,
                    values,
                    is_list: true,
                });
            }
        }
    }

    fn method(&mut self, owner: usize, function: &Function<'a>) {
        let Some(id) = &function.id else {
            return;
        };
        let name = id.name.to_string();
        // `function onPicked(index) {}` in a Connections handles its
        // target's signal: it is a handler, not a method anyone calls.
        let signal = handled(&name).filter(|_| self.special(owner) == Some(Special::Connections));
        if let Some(signal) = signal {
            let target = format!("{}.{name}", self.tree.objects[owner].name);
            let expr = self.expr(
                BindingKind::Handler,
                Form::Function,
                function.span,
                Some(Vec::new()),
                &target,
            );
            self.def_mut(self.place[owner])
                .members
                .handlers
                .push(Handler { signal, expr });
            return;
        }
        let target = format!("{}.{name}()", self.tree.objects[owner].name);
        let expr = self.expr(
            BindingKind::Function,
            Form::Function,
            function.span,
            Some(Vec::new()),
            &target,
        );
        self.def_mut(self.place[owner])
            .functions
            .push(FunctionDecl { name, expr });
    }

    /// `Rectangle.anchors.left`, for the comment above a function.
    fn describe(&self, owner: usize, slot: &Slot, name: &str) -> String {
        let ty = &self.tree.objects[owner].name;
        match slot {
            Slot::Own => format!("{ty}.{name}"),
            Slot::Group(path) => format!("{ty}.{}.{name}", path.join(".")),
            Slot::Attached(parts) => format!("{ty}.{}.{name}", parts.join(".")),
        }
    }

    // ---- types ------------------------------------------------------------

    fn type_ref(&self, found: &Found<O::Type>, qualifier: Option<&str>) -> TypeRef {
        match &found.kind {
            Kind::Builtin(ty) => {
                let oracle = self.types.oracle;
                let imported = match (&found.origin, qualifier) {
                    (Origin::Module(uri), _) => Some(uri.clone()),
                    (_, Some(alias)) => match self.types.namespace(alias) {
                        Some(Source::Module(uri)) => Some(uri.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                TypeRef::Native {
                    module: oracle.module(*ty).or(imported).unwrap_or_default(),
                    name: oracle.name(*ty).to_string(),
                }
            }
            Kind::Component(key) => TypeRef::Component {
                file: key.file.clone(),
                inline: key.inline.clone(),
            },
        }
    }

    fn type_of_parts(&self, parts: &[String]) -> TypeRef {
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        match self.types.find(&refs) {
            Some(found) => {
                let qualifier = (refs.len() == 2).then(|| refs[0]);
                self.type_ref(&found, qualifier)
            }
            None => TypeRef::Unresolved(parts.join(".")),
        }
    }

    fn object_type(&self, index: usize) -> TypeRef {
        let object = &self.tree.objects[index];
        match (&object.kind, &object.origin) {
            (Some(kind), Some(origin)) => {
                let found = Found {
                    kind: kind.clone(),
                    origin: origin.clone(),
                };
                let qualifier = object.name.split_once('.').map(|(q, _)| q);
                self.type_ref(&found, qualifier)
            }
            _ => TypeRef::Unresolved(object.name.clone()),
        }
    }

    fn special(&self, index: usize) -> Option<Special> {
        let kind = self.tree.objects[index].kind.as_ref()?;
        self.types.oracle.special(self.types.base(kind)?)
    }

    /// The parameters of the signal a handler handles; None when the signal
    /// is not known.
    fn signal_args(&self, owner: usize, slot: &Slot, signal: &str) -> Option<Vec<String>> {
        let oracle = self.types.oracle;
        match slot {
            Slot::Own => {
                let object = if self.special(owner) == Some(Special::Connections) {
                    self.tree.target(owner)?
                } else {
                    owner
                };
                self.tree.signal(self.types, object, signal)
            }
            Slot::Attached(parts) => {
                if parts.as_slice() == ["Component"]
                    && matches!(signal, "completed" | "destruction")
                {
                    return Some(Vec::new());
                }
                let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
                let found = self.types.find(&refs)?;
                let attached = oracle.attached(self.types.base(&found.kind)?)?;
                oracle.signal(attached, signal)
            }
            Slot::Group(path) => {
                let refs: Vec<&str> = path.iter().map(String::as_str).collect();
                let property = self.tree.property(self.types, owner, &refs)?;
                oracle.signal(property.value?, signal)
            }
        }
    }

    /// Whether the property a member names holds a colour.
    fn is_color(&self, owner: usize, slot: &Slot, name: &str) -> bool {
        let oracle = self.types.oracle;
        let ty = match slot {
            Slot::Own => {
                // What a PropertyChanges changes is its target's.
                let object = self.tree.target(owner).unwrap_or(owner);
                self.declared_type(object, name)
            }
            Slot::Group(path) => {
                let refs: Vec<&str> = path.iter().map(String::as_str).collect();
                match refs.as_slice() {
                    [id] if self.id_in_sight(owner, id).is_some() => {
                        let context = self.tree.objects[owner].context;
                        let object = self.tree.id_in(context, id);
                        object.and_then(|object| self.declared_type(object, name))
                    }
                    _ => self
                        .tree
                        .property(self.types, owner, &refs)
                        .and_then(|property| oracle.property_type(property.value?, name)),
                }
            }
            Slot::Attached(parts) => {
                let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
                self.types
                    .find(&refs)
                    .and_then(|found| self.types.base(&found.kind))
                    .and_then(|base| oracle.attached(base))
                    .and_then(|attached| oracle.property_type(attached, name))
            }
        };
        ty.is_some_and(|ty| is_color(&ty))
    }

    /// The type of a property of an object: one it declares, or one of its
    /// type's.
    fn declared_type(&self, object: usize, name: &str) -> Option<String> {
        if let Some(ty) = self.declared.get(&(object, name.to_string())) {
            return Some(ty.clone());
        }
        let mut kind = self.tree.objects[object].kind.clone()?;
        for _ in 0..64 {
            match kind {
                Kind::Builtin(ty) => return self.types.oracle.property_type(ty, name),
                Kind::Component(key) => {
                    let shape = self.project.front.shape(&key)?;
                    if let Some(declaration) = shape.properties.get(name) {
                        return Some(declaration.type_name.clone());
                    }
                    let types = Types::new(&self.project.front, &self.project.oracle, &key.file);
                    let parts: Vec<&str> = shape.root.iter().map(String::as_str).collect();
                    kind = types.find(&parts)?.kind;
                }
            }
        }
        None
    }

    fn enum_value(&self, kind: &Kind<O::Type>, scope: Option<&str>, key: &str) -> Option<i64> {
        match kind {
            Kind::Builtin(ty) => self.types.oracle.enum_value(*ty, scope, key),
            // The enums of the file being compiled are read off its own tree;
            // those of the components it is built on, in other files or in
            // this one, come from what the front end summarised of them.
            Kind::Component(component) => (component.file == self.key)
                .then(|| {
                    self.enums
                        .iter()
                        .filter(|(inline, name, _)| {
                            *inline == component.inline && scope.is_none_or(|scope| scope == name)
                        })
                        .find_map(|(_, _, keys)| {
                            keys.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
                        })
                })
                .flatten()
                .or_else(|| self.types.enum_value(kind, scope, key)),
        }
    }

    fn qt_enum(&self, key: &str) -> Option<EnumRef> {
        let oracle = self.types.oracle;
        let ty = match self.qt {
            Some(qt) => {
                // With a table, only what it lists; without one, any
                // capitalised member of `Qt` is an enum key.
                if !oracle.has_enum_key(qt, None, key) {
                    return None;
                }
                TypeRef::Native {
                    module: oracle.module(qt).unwrap_or_else(|| "QtQml".into()),
                    name: oracle.name(qt).to_string(),
                }
            }
            None => TypeRef::Native {
                module: "QtQml".into(),
                name: "Qt".into(),
            },
        };
        Some(EnumRef {
            ty,
            scope: None,
            key: key.to_string(),
            value: self.qt.and_then(|qt| oracle.enum_value(qt, None, key)),
        })
    }

    // ---- literals ---------------------------------------------------------

    /// The value, when it is the same every time it is read.
    fn literal(&mut self, expression: &Expression<'a>, color: bool) -> Option<Literal> {
        Some(match expression {
            Expression::NumericLiteral(number) => Literal::Number(number.value),
            Expression::StringLiteral(string) if color => Literal::Color(string.value.to_string()),
            Expression::StringLiteral(string) => Literal::String(string.value.to_string()),
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                let text = template.quasis.first()?.value.cooked.as_ref()?.to_string();
                if color {
                    Literal::Color(text)
                } else {
                    Literal::String(text)
                }
            }
            Expression::BooleanLiteral(boolean) => Literal::Bool(boolean.value),
            Expression::NullLiteral(_) => Literal::Null,
            Expression::Identifier(id)
                if id.name == "undefined" && self.is_global(id.name.as_str(), id.span.start) =>
            {
                Literal::Undefined
            }
            Expression::UnaryExpression(unary) => {
                let Expression::NumericLiteral(number) = &unary.argument else {
                    return None;
                };
                match unary.operator {
                    UnaryOperator::UnaryNegation => Literal::Number(-number.value),
                    UnaryOperator::UnaryPlus => Literal::Number(number.value),
                    _ => return None,
                }
            }
            Expression::ParenthesizedExpression(inner) => {
                return self.literal(&inner.expression, color);
            }
            Expression::ArrayExpression(array) => {
                let mut items = Vec::new();
                for element in &array.elements {
                    let item = element.as_expression()?;
                    items.push(self.literal(item, false)?);
                }
                Literal::List(items)
            }
            Expression::ObjectExpression(object) => {
                let mut fields = Vec::new();
                for property in &object.properties {
                    let ObjectPropertyKind::ObjectProperty(property) = property else {
                        return None;
                    };
                    if property.computed
                        || property.shorthand
                        || property.method
                        || property.kind != PropertyKind::Init
                    {
                        return None;
                    }
                    let key = property.key.static_name()?.to_string();
                    fields.push((key, self.literal(&property.value, false)?));
                }
                Literal::Object(fields)
            }
            Expression::StaticMemberExpression(_) => Literal::Enum(self.enum_literal(expression)?),
            Expression::CallExpression(call) => {
                let (callee, args) = js::call_args(call)?;
                let Expression::Identifier(id) = &call.callee else {
                    return None;
                };
                if !self.is_global(&callee, id.span.start) {
                    return None;
                }
                let (translatable, is_static) = self.translatable(&callee, &args)?;
                self.translations.push(translatable.clone());
                if !is_static {
                    return None;
                }
                match translatable.kind {
                    TranslationKind::NoOp => Literal::String(translatable.source),
                    _ => Literal::Translatable(translatable),
                }
            }
            _ => return None,
        })
    }

    fn is_global(&self, name: &str, offset: u32) -> bool {
        self.names
            .lookup(name, offset)
            .is_some_and(|r| r.roles.is_empty() && matches!(r.target, Some(Target::Global(_))))
    }

    /// `Text.AlignLeft`, `Image.Fill.Stretch`, `C.Text.AlignLeft`,
    /// `Qt.AlignLeft`.
    fn enum_literal(&self, expression: &Expression<'a>) -> Option<EnumRef> {
        let mut members = Vec::new();
        let mut object = expression;
        while let Expression::StaticMemberExpression(member) = object {
            members.push(member.property.name.as_str());
            object = &member.object;
        }
        let Expression::Identifier(base) = object else {
            return None;
        };
        members.reverse();
        let name = base.name.as_str();
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) || members.len() > 3 {
            return None;
        }
        let resolution = self.names.lookup(name, base.span.start)?;
        let (found, qualifier, rest) = match resolution.target? {
            Target::Type(found) => (found, None, &members[..]),
            Target::Namespace(_) => {
                let (ty, rest) = members.split_first()?;
                (self.types.find(&[name, ty])?, Some(name), rest)
            }
            Target::Global(_) if name == "Qt" => {
                return match members.as_slice() {
                    [key] if is_key_name(key) => self.qt_enum(key),
                    _ => None,
                };
            }
            _ => return None,
        };
        let (scope, key) = match rest {
            [key] => (None, *key),
            [scope, key] => (Some(*scope), *key),
            _ => return None,
        };
        if !is_key_name(key) || !self.types.is_key(&found.kind, scope, key) {
            return None;
        }
        Some(EnumRef {
            ty: self.type_ref(&found, qualifier),
            scope: scope.map(str::to_string),
            key: key.to_string(),
            value: self.enum_value(&found.kind, scope, key),
        })
    }

    /// What a call of a translation function marks, and whether every
    /// argument is a literal.
    fn translatable(&self, callee: &str, args: &[ArgValue]) -> Option<(Translatable, bool)> {
        let mut is_static = args.iter().all(|a| !matches!(a, ArgValue::Other));
        let text = |index: usize| match args.get(index) {
            Some(ArgValue::Str(text)) => Some(text.clone()),
            _ => None,
        };
        let (kind, context, source, comment, n) = match callee {
            "qsTr" => (
                TranslationKind::Text,
                Some(self.context.clone()),
                0,
                Some(1),
                Some(2),
            ),
            "qsTranslate" => (TranslationKind::Text, None, 1, Some(2), Some(3)),
            "qsTrId" => (TranslationKind::Id, Some(String::new()), 0, None, Some(1)),
            "qsTrNoOp" | "QT_TR_NOOP" => (
                TranslationKind::NoOp,
                Some(self.context.clone()),
                0,
                Some(1),
                None,
            ),
            "qsTranslateNoOp" | "QT_TRANSLATE_NOOP" => {
                (TranslationKind::NoOp, None, 1, Some(2), None)
            }
            "qsTrIdNoOp" | "QT_TRID_NOOP" => {
                (TranslationKind::NoOp, Some(String::new()), 0, None, None)
            }
            _ => return None,
        };
        let context = match context {
            Some(context) => context,
            None => text(0)?,
        };
        let source = text(source)?;
        let comment = comment.and_then(text);
        let (plural, is_plural) = match n.and_then(|n| args.get(n)) {
            Some(ArgValue::Num(n)) => (Some(*n as i64), true),
            Some(_) => {
                is_static = false;
                (None, true)
            }
            None => (None, false),
        };
        Some((
            Translatable {
                kind,
                context,
                source,
                comment,
                plural,
                is_plural,
            },
            is_static,
        ))
    }

    // ---- expressions ------------------------------------------------------

    /// Compiles a script to a function of the module. `args` are the
    /// signal's parameters; None when the signal is not known.
    fn expr(
        &mut self,
        kind: BindingKind,
        form: Form,
        span: Span,
        args: Option<Vec<String>>,
        target: &str,
    ) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        let js_fn = FnIndex(self.functions.len() as u32);
        let export = format!("f{}", js_fn.0);
        let text = qml_syntax::text(self.source, span);
        let scope_index = self.tree.scope_at(span.start);
        let scope = self.place[scope_index];
        let source_span = self.span(span);
        let hash = u64::from_le_bytes(
            blake3::hash(text.as_bytes()).as_bytes()[..8]
                .try_into()
                .unwrap_or_default(),
        );
        let known = args.clone().unwrap_or_default();
        let header = format!(
            "// {}.qml:{}:{} {target}\n",
            self.key, source_span.line, source_span.column
        );
        let (params, refs, body_args, code) = match js::analyze(text, form, &known) {
            Ok(analysis) => self.function(
                &export,
                text,
                span,
                form,
                kind,
                scope_index,
                &args,
                analysis,
            ),
            Err(message) => {
                self.diagnostics.push(Diagnostic::error(
                    format!("the script does not parse as JavaScript: {message}"),
                    source_span,
                ));
                let quoted = format!("{message:?}");
                let code = format!(
                    "export function {export}() {{\n  throw new SyntaxError({quoted});\n}}\n"
                );
                (Vec::new(), Vec::new(), Vec::new(), code)
            }
        };
        self.functions
            .push((JsFunction { export, expr: id }, format!("{header}{code}")));
        self.exprs.push(Binding {
            kind,
            scope,
            js_fn,
            params,
            args: body_args,
            refs,
            span: source_span,
            hash,
            tier1: None,
        });
        id
    }

    /// Resolves the script's free names and writes its function.
    #[allow(clippy::too_many_arguments)]
    fn function(
        &mut self,
        export: &str,
        text: &str,
        span: Span,
        form: Form,
        kind: BindingKind,
        scope: usize,
        args: &Option<Vec<String>>,
        analysis: js::Analysis,
    ) -> (Vec<Param>, Vec<Ref>, Vec<String>, String) {
        let mut params: Vec<Param> = Vec::new();
        let mut refs: Vec<Ref> = Vec::new();
        let mut edits = analysis.edits.clone();
        let mut plans: HashMap<(String, Vec<String>), Plan> = HashMap::new();
        for free in &analysis.free {
            let offset = span.start + free.span.start;
            let plan = plans
                .entry((free.name.clone(), free.chain.clone()))
                .or_insert_with(|| self.resolve(&free.name, offset, &free.chain, scope))
                .clone();
            if let Some(param) = &plan.param
                && !params.iter().any(|p| p.name == param.name)
            {
                params.push(param.clone());
            }
            let r = Ref {
                name: free.name.clone(),
                resolved: plan.resolved.clone(),
            };
            if !refs.contains(&r) {
                refs.push(r);
            }
            if let Some(through) = &plan.rewrite {
                let text = if free.shorthand {
                    format!("{0}: {through}.{0}", free.name)
                } else {
                    format!("{through}.{}", free.name)
                };
                edits.push(js::Edit {
                    span: free.span,
                    text,
                });
            }
        }
        for local in &analysis.locals {
            refs.push(Ref {
                name: local.clone(),
                resolved: Resolved::Local,
            });
        }
        for call in &analysis.calls {
            if let Some((translatable, _)) = self.translatable(&call.callee, &call.args) {
                self.translations.push(translatable);
            }
        }
        let body = js::apply(text, &edits);
        let close = if body.contains("//") { "\n  " } else { "" };
        let mut signature: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        let wrapped = args.is_none() || analysis.uses_arguments;
        let known = args.clone().unwrap_or_default();
        let (body_args, code) = match form {
            Form::Function => {
                signature.push("...$args".into());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (analysis.function_params.clone(), code)
            }
            Form::Expression if kind == BindingKind::Handler && analysis.is_function_value => {
                signature.push("...$args".into());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            _ if wrapped => {
                signature.push("...$args".into());
                let inner = match form {
                    Form::Expression => format!(
                        "function ({}) {{ return ({body}{close}); }}",
                        known.join(", ")
                    ),
                    _ => format!("function ({}) {}", known.join(", "), block(&body)),
                };
                let code = format!(
                    "export function {export}({}) {{\n  return ({inner}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            Form::Expression => {
                signature.extend(known.iter().cloned());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close});\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            Form::Statement => {
                signature.extend(known.iter().cloned());
                let code = format!(
                    "export function {export}({}) {}\n",
                    signature.join(", "),
                    block(&body)
                );
                (known, code)
            }
        };
        (params, refs, body_args, code)
    }

    /// What a free name at `offset` is, and how the function gets it.
    fn resolve(&mut self, name: &str, offset: u32, chain: &[String], scope: usize) -> Plan {
        let context = self.tree.objects[self.tree.object_at(offset)].context;
        let Some(resolution) = self.names.lookup(name, offset) else {
            if self.project.dynamic_names().contains(name) {
                return Plan {
                    resolved: Resolved::ContextProp {
                        component: None,
                        fallback: None,
                    },
                    param: Some(Param {
                        name: name.to_string(),
                        arg: Arg::Context {
                            component: None,
                            name: name.to_string(),
                            fallback: None,
                        },
                    }),
                    rewrite: None,
                };
            }
            if self.warned.insert(name.to_string()) {
                let span = self
                    .lines
                    .span(self.source, Span::new(offset, offset + name.len() as u32));
                self.diagnostics.push(Diagnostic::warning(
                    format!(
                        "`{name}` is not a name in sight: it is looked up when the program runs"
                    ),
                    span,
                ));
            }
            return Plan {
                resolved: Resolved::Unknown,
                param: Some(Param {
                    name: name.to_string(),
                    arg: Arg::Dynamic(name.to_string()),
                }),
                rewrite: None,
            };
        };
        let base = resolution
            .target
            .as_ref()
            .map(|target| self.target(name, target, chain, scope, context));
        let role = resolution.roles.first().copied();
        if role.is_none()
            && resolution.aimed.is_none()
            && let Some(base) = base
        {
            return base;
        }
        // The running program may have the name first: a role of the model,
        // or what the target of a PropertyChanges has.
        let component = role.map(|c| CompIdx(c as u32));
        let fallback_arg = base.as_ref().and_then(|base| {
            let param = base.param.as_ref()?;
            Some(Box::new(match (&base.rewrite, &param.arg) {
                (Some(_), Arg::Object(object)) => Arg::Property {
                    object: *object,
                    name: name.to_string(),
                },
                (_, arg) => arg.clone(),
            }))
        });
        Plan {
            resolved: Resolved::ContextProp {
                component,
                fallback: base.map(|base| Box::new(base.resolved)),
            },
            param: Some(Param {
                name: name.to_string(),
                arg: Arg::Context {
                    component,
                    name: name.to_string(),
                    fallback: fallback_arg,
                },
            }),
            rewrite: None,
        }
    }

    fn target(
        &self,
        name: &str,
        target: &Target<O::Type>,
        chain: &[String],
        scope: usize,
        context: usize,
    ) -> Plan {
        let as_is = |arg: Arg| {
            Some(Param {
                name: name.to_string(),
                arg,
            })
        };
        match target {
            Target::Id(object) => {
                let at = self.place[*object];
                let resolved = if self.tree.objects[*object].context == context {
                    Resolved::Id(at)
                } else {
                    Resolved::OuterId(at)
                };
                Plan {
                    resolved,
                    param: as_is(Arg::Object(at)),
                    rewrite: None,
                }
            }
            Target::Property { object, .. } => {
                let at = self.place[*object];
                let root = self.tree.contexts[context].root;
                let (resolved, through) = if *object == scope {
                    (Resolved::ScopeProp, "$scope".to_string())
                } else if *object == root {
                    (Resolved::RootProp, "$root".to_string())
                } else {
                    (
                        Resolved::OuterProp(at),
                        format!("$c{}o{}", at.component.0, at.object.0),
                    )
                };
                Plan {
                    resolved,
                    param: Some(Param {
                        name: through.clone(),
                        arg: Arg::Object(at),
                    }),
                    rewrite: Some(through),
                }
            }
            Target::Context { context, .. } => {
                let component = Some(CompIdx(*context as u32));
                Plan {
                    resolved: Resolved::ContextProp {
                        component,
                        fallback: None,
                    },
                    param: as_is(Arg::Context {
                        component,
                        name: name.to_string(),
                        fallback: None,
                    }),
                    rewrite: None,
                }
            }
            Target::Type(found) => self.type_plan(name, found, None, chain),
            Target::Namespace(source) => {
                let source = import_source(source);
                // `S.Theme.accent`: the script reads a type of the module,
                // which is what the reference is, through the namespace.
                let resolved = chain
                    .first()
                    .filter(|ty| is_key_name(ty))
                    .and_then(|ty| Some((self.types.find(&[name, ty.as_str()])?, &chain[1..])))
                    .map(|(found, rest)| self.type_plan(name, &found, Some(name), rest).resolved)
                    .unwrap_or_else(|| Resolved::ImportNs(source.clone()));
                Plan {
                    resolved,
                    param: as_is(Arg::Namespace {
                        alias: name.to_string(),
                        source,
                    }),
                    rewrite: None,
                }
            }
            Target::Global(global) => {
                if name == "Qt"
                    && let Some(key) = chain.first().filter(|key| is_key_name(key))
                    && let Some(value) = self.qt_enum(key)
                {
                    return Plan {
                        resolved: Resolved::Enum(value),
                        param: None,
                        rewrite: None,
                    };
                }
                Plan {
                    resolved: Resolved::Global {
                        is_qml: *global == Global::Qml,
                    },
                    param: None,
                    rewrite: None,
                }
            }
        }
    }

    /// A type named in a script: what of it the script reads.
    fn type_plan(
        &self,
        name: &str,
        found: &Found<O::Type>,
        qualifier: Option<&str>,
        chain: &[String],
    ) -> Plan {
        let ty = self.type_ref(found, qualifier);
        let kind = &found.kind;
        let param = |arg: Arg| {
            Some(Param {
                name: name.to_string(),
                arg,
            })
        };
        let enumeration = |scope: Option<&str>, key: &str| Plan {
            resolved: Resolved::Enum(EnumRef {
                ty: ty.clone(),
                scope: scope.map(str::to_string),
                key: key.to_string(),
                value: self.enum_value(kind, scope, key),
            }),
            param: param(Arg::Type(ty.clone())),
            rewrite: None,
        };
        if let Some(first) = chain.first() {
            match self.types.declared_enum(kind, first) {
                Some(false) => return enumeration(None, first),
                Some(true) => {
                    if let Some(key) = chain.get(1)
                        && self.types.is_key(kind, Some(first), key)
                    {
                        return enumeration(Some(first), key);
                    }
                }
                None => {}
            }
        }
        if self.types.is_singleton(kind) {
            return Plan {
                resolved: Resolved::Singleton(ty.clone()),
                param: param(Arg::Singleton(ty)),
                rewrite: None,
            };
        }
        let attaches = self
            .types
            .base(kind)
            .and_then(|base| self.types.oracle.attached(base))
            .is_some();
        let resolved = match chain.first() {
            Some(member) if attaches && member.starts_with(|c: char| c.is_ascii_lowercase()) => {
                Resolved::Attached {
                    ty: ty.clone(),
                    member: member.clone(),
                }
            }
            _ => Resolved::Type(ty.clone()),
        };
        Plan {
            resolved,
            param: param(Arg::Type(ty)),
            rewrite: None,
        }
    }

    // ---- the file ---------------------------------------------------------

    fn finish(mut self, document: &QmlDocument<'a>) -> Compiled {
        let tree = self.tree;
        let mut inline = Vec::new();
        for (index, context) in tree.contexts.iter().enumerate() {
            let component = &mut self.components[index];
            component.kind = match context.outer {
                None if index == 0 => {
                    component.name = stem(self.key).to_string();
                    component.id = self.key.to_string();
                    ComponentKind::File
                }
                None => {
                    inline.push(component.name.clone());
                    ComponentKind::Inline
                }
                Some(outer) => {
                    component.name = tree.objects[context.root].name.clone();
                    ComponentKind::Template {
                        outer: CompIdx(outer as u32),
                        is_delegate: context.is_delegate,
                    }
                }
            };
        }
        for (index, component) in self.components.iter_mut().enumerate() {
            let names: &[String] = if index == 0 { &inline } else { &[] };
            component.signature = ComponentSignature::of(component.root(), names);
        }
        let mut translations = Vec::new();
        for translatable in std::mem::take(&mut self.translations) {
            if !translations.contains(&translatable) {
                translations.push(translatable);
            }
        }
        let mut source = format!(
            "// {}.qml, compiled by ljos_compiler: a function per binding, handler and method.\n",
            self.key
        );
        let mut functions = Vec::new();
        for (function, code) in self.functions {
            source.push('\n');
            source.push_str(&code);
            functions.push(function);
        }
        let mut file = CompiledFile {
            version: IR_VERSION,
            file: self.key.to_string(),
            source_hash: *blake3::hash(self.source.as_bytes()).as_bytes(),
            imports: document
                .imports
                .iter()
                .map(|import| Import {
                    source: match &import.source {
                        QmlImportSource::Module(name) => ImportSource::Module(name.to_string()),
                        QmlImportSource::Path(path) => ImportSource::Path(path.to_string()),
                    },
                    version: import.version.map(str::to_string),
                    alias: import.alias.as_ref().map(|alias| alias.name.to_string()),
                })
                .collect(),
            pragmas: document
                .pragmas
                .iter()
                .map(|pragma| Pragma {
                    name: pragma.name.to_string(),
                    value: pragma.value.map(str::to_string),
                })
                .collect(),
            components: self.components,
            exprs: self.exprs,
            translations,
            js: JsModule { source, functions },
        };
        // A template is known by where it is written, which is only known
        // once every key is: templates come after what they are written in.
        let mut nth: HashMap<String, u32> = HashMap::new();
        for (index, template) in self.templates.iter().enumerate() {
            let Some((owner, property)) = template else {
                continue;
            };
            let key = file.object_key(*owner);
            let mut id = key.component.clone();
            for segment in &key.segments {
                id.push('/');
                id.push_str(&segment_text(segment));
            }
            id.push('.');
            id.push_str(property);
            let count = nth.entry(id.clone()).or_insert(0);
            if *count > 0 {
                id.push_str(&format!("[{count}]"));
            }
            *count += 1;
            file.components[index].id = id;
        }
        Compiled {
            file,
            diagnostics: self.diagnostics,
        }
    }
}

/// A statement as a function body.
fn block(body: &str) -> String {
    if body.trim_start().starts_with('{') {
        body.to_string()
    } else {
        format!("{{\n  {body}\n}}")
    }
}

fn is_key_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// `Main` of `app/Main`: `qsTr`'s context and the file component's name.
fn stem(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

fn segment_text(segment: &KeySeg) -> String {
    match segment {
        KeySeg::Id(id) => id.clone(),
        KeySeg::Named(name) => format!("'{name}'"),
        KeySeg::ModelRow(row) => format!("[{row}]"),
        KeySeg::Typed { ty, nth } => format!("{ty}[{nth}]"),
    }
}

fn import_source(source: &Source) -> ImportSource {
    match source {
        Source::Module(uri) => ImportSource::Module(uri.clone()),
        Source::Path(path) => ImportSource::Path(path.clone()),
    }
}
