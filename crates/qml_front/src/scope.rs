//! The objects of a file: what each is, what it declares, and which
//! component (context) it belongs to.
//!
//! QML resolves a name at run time by walking contexts: the object the
//! expression belongs to, the root of its component, then the components
//! around it. Everything that walk needs is in the file and in the types it
//! names, so it is read off here once; [`crate::names`] walks it.

use std::collections::{HashMap, HashSet};

use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::{
    Span,
    ast::*,
    js::{
        AssignmentExpression, AssignmentTarget, Expression, SimpleAssignmentTarget,
        StaticMemberExpression, UpdateExpression,
    },
};

use crate::{
    Code, Error,
    oracle::{Property, Special, TypeOracle},
    types::{Kind, Lookup, Member as TypeMember, Origin, Types},
};

/// The objects and contexts of one file.
#[derive(Debug)]
pub struct Tree<'a, T> {
    /// In source order: an object comes before the objects inside it.
    pub objects: Vec<Object<'a, T>>,
    pub contexts: Vec<Context>,
    /// `pragma ComponentBehavior: Bound`.
    pub is_bound: bool,
    by_start: HashMap<u32, usize>,
    /// What a PropertyChanges or an AnchorChanges changes, and the object it
    /// is written in: it is evaluated as a binding of the target's.
    changes: Vec<(Span, usize)>,
    /// The id each of them, or a Connections, names as its `target`.
    targets: HashMap<usize, &'a str>,
    /// The PropertyChanges among them: what one changes is given its target.
    given: HashSet<usize>,
}

#[derive(Debug)]
pub struct Object<'a, T> {
    pub span: Span,
    /// The type as it is written: `Item`, `C.Button`.
    pub name: String,
    /// None when the type is not one the file knows, which was reported.
    pub kind: Option<Kind<T>>,
    pub origin: Option<Origin>,
    pub id: Option<&'a str>,
    /// What the object declares, by name.
    pub declared: HashMap<&'a str, Declared<T>>,
    pub context: usize,
    /// A `Component { }`: what is in it is a template.
    pub is_template: bool,
}

/// A member an object declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declared<T> {
    Property {
        property: Property<T>,
        member: Member,
    },
    Signal(Vec<String>),
    Function,
}

impl<T> Declared<T> {
    pub fn member(&self) -> &Member {
        match self {
            Declared::Property { member, .. } => member,
            Declared::Signal(_) | Declared::Function => &Member::Function,
        }
    }
}

/// How a declared member holds its value, read off the file: what a back end
/// needs to choose a representation. It says nothing of how a binding is
/// evaluated; that is the runtime's to decide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    /// Never changes once the object is made: a literal, or nothing, on a
    /// property nothing outside the object can set and no script assigns.
    Const,
    /// Its value is an expression, or may be set by whoever makes the object
    /// (a property of a component's root, a `required` one).
    Binding,
    /// A script assigns to it: it is state, not only a derivation.
    Signal,
    /// A function, or a signal: what calls it, or emits it.
    Function,
    /// Something the front end cannot say how to hold, and why.
    Unsupported(&'static str),
}

/// One component's worth of objects: what QML makes a context for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub root: usize,
    /// The context the component is written in. A file and an inline
    /// `component` have none: they see nothing around them.
    pub outer: Option<usize>,
    /// The component is a template, made later. Made by a view or a
    /// Repeater, its context is given what the model has (`index`,
    /// `modelData`, roles), which only the running program knows.
    pub is_delegate: bool,
}

/// The properties of `PropertyChanges` itself; any other name is a property
/// of its target.
pub const CHANGES: &[&str] = &["target", "explicit", "restoreEntryValues"];

impl<'a, T: Copy + Eq> Tree<'a, T> {
    pub fn analyze<O: TypeOracle<Type = T>>(
        document: &QmlDocument<'a>,
        types: Types<'_, O>,
        errors: &mut Vec<Error>,
    ) -> Self {
        let mut assigned = Assigned::default();
        walk_leaves(&document.root, &mut assigned);
        let is_bound = document
            .pragmas
            .iter()
            .any(|pragma| pragma.name == "ComponentBehavior" && pragma.value == Some("Bound"));
        let mut analysis = Analysis {
            tree: Tree {
                objects: Vec::new(),
                contexts: Vec::new(),
                is_bound,
                by_start: HashMap::new(),
                changes: Vec::new(),
                targets: HashMap::new(),
                given: HashSet::new(),
            },
            types,
            assigned,
            aliases: Vec::new(),
            errors,
        };
        analysis.component(&document.root, None, false);
        analysis.aliases();
        analysis.tree
    }

    pub fn index_of(&self, object: &QmlObject<'a>) -> Option<usize> {
        self.by_start.get(&object.span.start).copied()
    }

    /// The innermost object `offset` is written in.
    pub fn object_at(&self, offset: u32) -> usize {
        // Objects are in source order, so the last one that starts before the
        // offset and has not ended is the innermost.
        self.objects
            .iter()
            .rposition(|object| object.span.start <= offset && offset < object.span.end)
            .unwrap_or(0)
    }

    /// The object whose names an expression at `offset` may use as its own:
    /// the one it is written in, or for what a state changes, the target
    /// (`anchors.bottom: parent.bottom` is the bottom of the target's parent).
    /// A target that is not named by its id is known only to the running
    /// program: see [`Tree::aimed`].
    pub fn scope_at(&self, offset: u32) -> usize {
        let written = self.object_at(offset);
        if !self.changed(written, offset) {
            return written;
        }
        self.target(written).unwrap_or(written)
    }

    fn changed(&self, written: usize, offset: u32) -> bool {
        self.changes
            .iter()
            .any(|(span, owner)| *owner == written && span.start <= offset && offset < span.end)
    }

    /// The PropertyChanges `offset` is in what it changes of, when it does
    /// not name its target by its id (`target: view.currentItem`). The
    /// change is a binding of the target's all the same, so a name is the
    /// target's if the target has it, which only the running program can
    /// tell.
    pub fn aimed(&self, offset: u32) -> Option<usize> {
        let written = self.object_at(offset);
        (self.given.contains(&written)
            && self.changed(written, offset)
            && self.target(written).is_none())
        .then_some(written)
    }

    /// The object a PropertyChanges or a Connections names as its `target`,
    /// when it names it by its id.
    pub fn target(&self, owner: usize) -> Option<usize> {
        let id = self.targets.get(&owner)?;
        let mut context = Some(self.objects[owner].context);
        while let Some(at) = context {
            if let Some(target) = self.id_in(at, id) {
                return Some(target);
            }
            context = self.contexts[at].outer;
        }
        None
    }

    /// The object `id` names in the context `context` itself.
    pub fn id_in(&self, context: usize, id: &str) -> Option<usize> {
        self.objects
            .iter()
            .position(|object| object.context == context && object.id == Some(id))
    }

    /// The ids of the context, in source order.
    pub fn ids_of(&self, context: usize) -> impl Iterator<Item = (usize, &'a str)> + '_ {
        self.objects
            .iter()
            .enumerate()
            .filter(move |(_, object)| object.context == context)
            .filter_map(|(index, object)| Some((index, object.id?)))
    }

    /// Whether the object is the root of a component written in the file.
    pub fn is_root(&self, index: usize) -> bool {
        self.contexts[self.objects[index].context].root == index
    }

    /// What `name` is on the object: something it declares, or a member of
    /// its type.
    pub fn member<O: TypeOracle<Type = T>>(
        &self,
        types: Types<'_, O>,
        index: usize,
        name: &str,
    ) -> Option<TypeMember<T>> {
        self.lookup(types, index, name).found()
    }

    /// As [`member`](Self::member), but says whether a name that is not found
    /// is not on the object ([`Lookup::Missing`]) or may be ([`Lookup::Undecided`]):
    /// the type is not known, is a component whose file did not parse whole, or
    /// is not listed in full.
    pub fn lookup<O: TypeOracle<Type = T>>(
        &self,
        types: Types<'_, O>,
        index: usize,
        name: &str,
    ) -> Lookup<TypeMember<T>> {
        let object = &self.objects[index];
        match object.declared.get(name) {
            Some(Declared::Property { property, .. }) => {
                return Lookup::Found(TypeMember::Property(*property));
            }
            Some(Declared::Signal(_)) => return Lookup::Found(TypeMember::Signal),
            Some(Declared::Function) => return Lookup::Found(TypeMember::Method),
            None => {}
        }
        if let Some(property) = name.strip_suffix("Changed")
            && matches!(
                object.declared.get(property),
                Some(Declared::Property { .. })
            )
        {
            return Lookup::Found(TypeMember::Signal);
        }
        match &object.kind {
            Some(kind) => types.lookup(kind, name),
            None => Lookup::Undecided,
        }
    }

    /// As [`signal`](Self::signal), with what [`lookup`](Self::lookup) says of
    /// a signal that is not found.
    pub fn lookup_signal<O: TypeOracle<Type = T>>(
        &self,
        types: Types<'_, O>,
        index: usize,
        name: &str,
    ) -> Lookup<Vec<String>> {
        let object = &self.objects[index];
        match object.declared.get(name) {
            Some(Declared::Signal(parameters)) => return Lookup::Found(parameters.clone()),
            Some(_) => return Lookup::Missing,
            None => {}
        }
        if let Some(property) = name.strip_suffix("Changed")
            && matches!(
                object.declared.get(property),
                Some(Declared::Property { .. })
            )
        {
            return Lookup::Found(Vec::new());
        }
        match &object.kind {
            Some(kind) => types.lookup_signal(kind, name),
            None => Lookup::Undecided,
        }
    }

    /// The names a handler of the object's signal `name` may call its
    /// arguments; None if it has no such signal.
    pub fn signal<O: TypeOracle<Type = T>>(
        &self,
        types: Types<'_, O>,
        index: usize,
        name: &str,
    ) -> Option<Vec<String>> {
        self.lookup_signal(types, index, name).found()
    }

    /// What the property a binding names is, through the groups on the way:
    /// `border.width` is the `width` of what `border` is.
    pub fn property<O: TypeOracle<Type = T>>(
        &self,
        types: Types<'_, O>,
        index: usize,
        path: &[&str],
    ) -> Option<Property<T>> {
        let (first, rest) = path.split_first()?;
        let TypeMember::Property(mut property) = self.member(types, index, first)? else {
            return None;
        };
        for name in rest {
            property = types.oracle.property(property.value?, name)?;
        }
        Some(property)
    }

    /// What a handler written on the object handles, if it is one of its own
    /// declarations.
    pub fn own(&self, index: usize, name: &str) -> Option<Own> {
        let declared = &self.objects[index].declared;
        classify(
            name,
            |signal| matches!(declared.get(signal), Some(Declared::Signal(_))),
            |property| matches!(declared.get(property), Some(Declared::Property { .. })),
        )
    }
}

/// An alias whose target is known once the ids of its component are.
struct Pending<'a> {
    owner: usize,
    name: &'a str,
    span: Span,
    id: &'a str,
    property: String,
    writable: bool,
}

struct Analysis<'a, 'p, 'e, O: TypeOracle> {
    tree: Tree<'a, O::Type>,
    types: Types<'p, O>,
    assigned: Assigned<'a>,
    aliases: Vec<Pending<'a>>,
    errors: &'e mut Vec<Error>,
}

impl<'a, O: TypeOracle> Analysis<'a, '_, '_, O> {
    /// The root of a component, in a context of its own.
    fn component(
        &mut self,
        root: &QmlObject<'a>,
        outer: Option<usize>,
        is_delegate: bool,
    ) -> usize {
        let context = self.tree.contexts.len();
        self.tree.contexts.push(Context {
            root: self.tree.objects.len(),
            outer,
            is_delegate,
        });
        self.object(root, context)
    }

    fn object(&mut self, object: &QmlObject<'a>, context: usize) -> usize {
        let parts: Vec<&str> = object.type_name.parts.iter().copied().collect();
        let found = self.types.find(&parts);
        if found.is_none() {
            self.errors.push(Error::with_code(
                Code::UnknownType,
                format!(
                    "`{}` is not a type of anything the file imports",
                    object.type_name
                ),
                object.type_name.span,
            ));
        }
        let (kind, origin) =
            found.map_or((None, None), |found| (Some(found.kind), Some(found.origin)));
        let is_template = kind
            .as_ref()
            .is_some_and(|kind| self.special(kind) == Some(Special::Component));
        let id = object_id(object);
        if let Some(id) = id
            && self.tree.id_in(context, id).is_some()
        {
            self.errors.push(Error::with_code(
                Code::DuplicateId,
                format!("id `{id}` is not unique"),
                object.span,
            ));
        }
        let index = self.tree.objects.len();
        self.tree.by_start.insert(object.span.start, index);
        self.tree.objects.push(Object {
            span: object.span,
            name: object.type_name.to_string(),
            kind,
            origin,
            id,
            declared: HashMap::new(),
            context,
            is_template,
        });
        self.declarations(object, index);
        self.members(object, index, context, &[]);
        index
    }

    fn special(&self, kind: &Kind<O::Type>) -> Option<Special> {
        self.types.oracle.special(self.types.base(kind)?)
    }

    fn declarations(&mut self, object: &QmlObject<'a>, index: usize) {
        let id = self.tree.objects[index].id;
        let context = self.tree.objects[index].context;
        // What whoever makes the component may set.
        let is_interface = self.tree.contexts[context].root == index;
        let mut declared = HashMap::new();
        for member in &object.members {
            match member {
                QmlMember::Property(declaration) => {
                    let Some(type_name) = &declaration.type_name else {
                        continue;
                    };
                    let name = declaration.name.name.as_str();
                    let property = Property::declared(
                        &type_name.name.to_string(),
                        type_name.is_list,
                        declaration.is_readonly,
                    );
                    let writable = self.assigned.has(name, id) && !declaration.is_readonly;
                    let member = if type_name.name.as_simple() == Some("alias") {
                        self.alias(declaration, index, writable)
                    } else {
                        storage(declaration, is_interface, writable)
                    };
                    declared.insert(name, Declared::Property { property, member });
                }
                QmlMember::Signal(signal) => {
                    declared.insert(
                        signal.name.name.as_str(),
                        Declared::Signal(
                            signal
                                .params
                                .iter()
                                .map(|param| param.name.name.to_string())
                                .collect(),
                        ),
                    );
                }
                QmlMember::Function(function) => {
                    if let Some(id) = &function.id {
                        declared.insert(id.name.as_str(), Declared::Function);
                    }
                }
                _ => {}
            }
        }
        self.tree.objects[index].declared = declared;
    }

    /// `property alias name: id.path`: settled once every id is known.
    fn alias(
        &mut self,
        declaration: &QmlPropertyDeclaration<'a>,
        owner: usize,
        writable: bool,
    ) -> Member {
        let Some((id, property)) = alias_target(declaration) else {
            return Member::Unsupported("aliases of anything but `id` or `id.property`");
        };
        self.aliases.push(Pending {
            owner,
            name: declaration.name.name.as_str(),
            span: declaration.span,
            id,
            property,
            writable,
        });
        if writable {
            Member::Signal
        } else {
            Member::Binding
        }
    }

    /// Settles what each alias is, now that every id is known.
    fn aliases(&mut self) {
        for alias in std::mem::take(&mut self.aliases) {
            let mut context = Some(self.tree.objects[alias.owner].context);
            let mut target = None;
            while let Some(at) = context {
                target = self.tree.id_in(at, alias.id);
                if target.is_some() {
                    break;
                }
                context = self.tree.contexts[at].outer;
            }
            let Some(target) = target else {
                self.errors.push(Error::with_code(
                    Code::UnknownAliasTarget,
                    format!("`{}` is not an id in this component", alias.id),
                    alias.span,
                ));
                continue;
            };
            if alias.property.is_empty() {
                continue;
            }
            // An alias of a declared property is another name for it, which
            // whoever sets the alias now sets too.
            let first = alias.property.split('.').next().unwrap_or_default();
            let target_member = match self.tree.objects[target].declared.get_mut(first) {
                Some(Declared::Property { member, .. }) => {
                    if alias.writable || *member == Member::Const {
                        *member = if alias.writable {
                            Member::Signal
                        } else {
                            Member::Binding
                        };
                    }
                    Some(member.clone())
                }
                Some(_) => Some(Member::Unsupported("aliases of functions and signals")),
                None => None,
            };
            if let Some(member) = target_member
                && let Some(Declared::Property { member: own, .. }) =
                    self.tree.objects[alias.owner].declared.get_mut(alias.name)
            {
                *own = member;
            }
        }
    }

    /// The objects written in `object`, which is `owner` or a group of it
    /// (`font { }`), `group` being the names that lead there.
    fn members(&mut self, object: &QmlObject<'a>, owner: usize, context: usize, group: &[&'a str]) {
        let is_template = self.tree.objects[owner].is_template;
        let default_component = self.tree.objects[owner]
            .kind
            .as_ref()
            .and_then(|kind| self.types.default_component(kind))
            .is_some();
        for member in &object.members {
            match member {
                QmlMember::Object(child) if is_group(child) => {
                    let mut path = group.to_vec();
                    path.extend(child.type_name.parts.iter().copied());
                    self.members(child, owner, context, &path);
                }
                QmlMember::Object(child) => {
                    let template = child.on.is_none() && (is_template || default_component);
                    self.value(child, context, template);
                }
                QmlMember::Binding(binding) => {
                    let mut path = group.to_vec();
                    path.extend(binding.name.parts.iter().copied());
                    self.change(binding, owner, &path);
                    let template = self
                        .tree
                        .property(self.types, owner, &path)
                        .or_else(|| self.attached(&path))
                        .is_some_and(|property| property.is_component);
                    self.values(&binding.value, context, template);
                }
                QmlMember::Property(QmlPropertyDeclaration {
                    value: Some(value),
                    type_name,
                    ..
                }) => {
                    let template = type_name.as_ref().is_some_and(|type_name| {
                        type_name.name.as_simple() == Some("Component") && !type_name.is_list
                    });
                    self.values(value, context, template);
                }
                QmlMember::InlineComponent(inline) => {
                    self.component(&inline.object, None, false);
                }
                _ => {}
            }
        }
    }

    /// A binding written in a PropertyChanges or an AnchorChanges: its
    /// target, or something of the target's to change.
    fn change(&mut self, binding: &QmlBinding<'a>, owner: usize, path: &[&'a str]) {
        let special = self.tree.objects[owner]
            .kind
            .as_ref()
            .and_then(|kind| self.special(kind));
        let context = self.tree.objects[owner].context;
        let changes = match special {
            Some(Special::PropertyChanges) => {
                // `rect.width: 10` is a binding like any other.
                !CHANGES.contains(&path[0])
                    && !(path.len() > 1 && self.tree.ids_of(context).any(|(_, id)| id == path[0]))
            }
            Some(Special::AnchorChanges) => path[0] == "anchors",
            Some(Special::Connections) => false,
            _ => return,
        };
        if changes {
            self.tree.changes.push((binding.span, owner));
            if special == Some(Special::PropertyChanges) {
                self.tree.given.insert(owner);
            }
        } else if let (["target"], QmlBindingValue::Expression(Expression::Identifier(id))) =
            (path, &binding.value)
        {
            self.tree.targets.insert(owner, id.name.as_str());
        }
    }

    /// What `TableView.editDelegate` is: a property of what a type attaches.
    fn attached(&self, path: &[&str]) -> Option<Property<O::Type>> {
        let (name, of) = path.split_last()?;
        let attaching = self
            .types
            .oracle
            .attached(self.types.base(&self.types.find(of)?.kind)?)?;
        self.types.oracle.property(attaching, name)
    }

    fn values(&mut self, value: &QmlBindingValue<'a>, context: usize, template: bool) {
        match value {
            QmlBindingValue::Object(object) => self.value(object, context, template),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.value(object, context, template);
                }
            }
            _ => {}
        }
    }

    /// An object where a value goes. Where a template goes it is the root of
    /// a component, unless it is a `Component` itself: then what is in it is.
    fn value(&mut self, object: &QmlObject<'a>, context: usize, template: bool) {
        let parts: Vec<&str> = object.type_name.parts.iter().copied().collect();
        let is_component = self
            .types
            .find(&parts)
            .is_some_and(|found| self.special(&found.kind) == Some(Special::Component));
        if template && !is_component {
            self.component(object, Some(context), true);
        } else {
            self.object(object, context);
        }
    }
}

/// How a declared property that is not an alias holds its value.
fn storage(declaration: &QmlPropertyDeclaration<'_>, is_interface: bool, writable: bool) -> Member {
    if writable {
        return Member::Signal;
    }
    if declaration.is_required || (is_interface && !declaration.is_readonly) {
        return Member::Binding;
    }
    match &declaration.value {
        None => Member::Const,
        Some(QmlBindingValue::Expression(expression)) if is_literal(expression) => Member::Const,
        // An object is made once, with the object that has it.
        Some(QmlBindingValue::Object(_) | QmlBindingValue::Objects(_)) => Member::Const,
        Some(_) => Member::Binding,
    }
}

/// A handler that belongs to a declaration of the object it is written on.
#[derive(Debug, PartialEq, Eq)]
pub enum Own {
    /// `onPicked` for `signal picked`.
    Signal(String),
    /// `onTitleChanged` for `property string title`.
    Changed(String),
    /// `Component.onCompleted`.
    Completed,
    /// `Component.onDestruction`.
    Destruction,
}

/// What the handler `name` handles, given what the object declares.
pub fn classify(
    name: &str,
    is_signal: impl Fn(&str) -> bool,
    is_property: impl Fn(&str) -> bool,
) -> Option<Own> {
    match name {
        "Component.onCompleted" => return Some(Own::Completed),
        "Component.onDestruction" => return Some(Own::Destruction),
        _ => {}
    }
    let handled = handled(name)?;
    if is_signal(&handled) {
        return Some(Own::Signal(handled));
    }
    let property = handled.strip_suffix("Changed")?;
    is_property(property).then(|| Own::Changed(property.to_string()))
}

/// `onClicked` → `clicked`: the signal a handler's name stands for.
pub fn handled(name: &str) -> Option<String> {
    let rest = name.strip_prefix("on")?;
    let first = rest.chars().next()?;
    (first.is_ascii_uppercase() || first == '_').then(|| {
        let mut signal = first.to_ascii_lowercase().to_string();
        signal.push_str(&rest[first.len_utf8()..]);
        signal
    })
}

/// The `id` and property path of `property alias name: id.path`; the path is
/// empty for `property alias name: id`.
pub fn alias_target<'a>(property: &QmlPropertyDeclaration<'a>) -> Option<(&'a str, String)> {
    let Some(QmlBindingValue::Expression(expression)) = property.value.as_ref() else {
        return None;
    };
    let mut expression = expression;
    let mut path = Vec::new();
    while let Expression::StaticMemberExpression(member) = expression {
        path.push(member.property.name.as_str());
        expression = &member.object;
    }
    let Expression::Identifier(id) = expression else {
        return None;
    };
    path.reverse();
    Some((id.name.as_str(), path.join(".")))
}

/// The `id` an object gives itself.
pub fn object_id<'a>(object: &QmlObject<'a>) -> Option<&'a str> {
    object.members.iter().find_map(|member| match member {
        QmlMember::Binding(QmlBinding {
            name,
            value: QmlBindingValue::Expression(Expression::Identifier(id)),
            ..
        }) if name.as_simple() == Some("id") => Some(id.name.as_str()),
        _ => None,
    })
}

/// `font { ... }`: an object whose "type" is a lowercase property name.
pub fn is_group(object: &QmlObject<'_>) -> bool {
    object.on.is_none() && object.type_name.parts[0].starts_with(|c: char| c.is_ascii_lowercase())
}

/// A value that is the same every time it is read.
pub fn is_literal(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            matches!(unary.argument, Expression::NumericLiteral(_))
        }
        Expression::ParenthesizedExpression(inner) => is_literal(&inner.expression),
        _ => false,
    }
}

/// Every name the file's scripts assign to. A property nothing assigns to is
/// a derivation, whatever its declaration allows.
#[derive(Default)]
struct Assigned<'a> {
    bare: HashSet<&'a str>,
    members: HashSet<(&'a str, &'a str)>,
}

impl<'a> Assigned<'a> {
    fn has(&self, name: &str, id: Option<&str>) -> bool {
        self.bare.contains(name) || id.is_some_and(|id| self.members.contains(&(id, name)))
    }

    fn member(&mut self, member: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &member.object {
            self.members
                .insert((object.name.as_str(), member.property.name.as_str()));
        }
    }
}

impl<'a> Visit<'a> for Assigned<'a> {
    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        match &it.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.bare.insert(id.name.as_str());
            }
            AssignmentTarget::StaticMemberExpression(member) => self.member(member),
            _ => {}
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        match &it.argument {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.bare.insert(id.name.as_str());
            }
            SimpleAssignmentTarget::StaticMemberExpression(member) => self.member(member),
            _ => {}
        }
        walk::walk_update_expression(self, it);
    }
}

fn walk_value<'a>(value: &QmlBindingValue<'a>, visitor: &mut impl Visit<'a>) {
    match value {
        QmlBindingValue::Expression(expression) => visitor.visit_expression(expression),
        QmlBindingValue::Statement(statement) => visitor.visit_statement(statement),
        QmlBindingValue::Object(object) => walk_leaves(object, visitor),
        QmlBindingValue::Objects(objects) => {
            for object in objects {
                walk_leaves(object, visitor);
            }
        }
    }
}

/// Visits every script of the object and of the objects in it.
pub fn walk_leaves<'a>(object: &QmlObject<'a>, visitor: &mut impl Visit<'a>) {
    for member in &object.members {
        match member {
            QmlMember::Object(child) => walk_leaves(child, visitor),
            QmlMember::Binding(binding) => walk_value(&binding.value, visitor),
            QmlMember::Property(property) => {
                if let Some(value) = &property.value {
                    walk_value(value, visitor);
                }
            }
            QmlMember::Function(function) => visitor.visit_function(function, ScopeFlags::Function),
            QmlMember::InlineComponent(inline) => walk_leaves(&inline.object, visitor),
            QmlMember::Signal(_) | QmlMember::Enum(_) => {}
        }
    }
}
