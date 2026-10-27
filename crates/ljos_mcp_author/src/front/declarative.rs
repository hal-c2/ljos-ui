//! The declarative lints (`decl-*`, `docs/declarative.md`): script that
//! says what a binding, a set of states or a transition would. What is
//! declared, the engine compiles and the motion and focus rules can read;
//! what a handler does by hand, neither can.
//!
//! Every rule here is a hint, and holds back wherever the file does not
//! show the whole story: a target it cannot name, a property something else
//! writes as well, a handler that does more than the pattern. Changing state
//! on an event (`onClicked: open = !open`) is what handlers are for, so a
//! value that is not written out never counts as a mode.

use std::collections::{HashMap, HashSet};

use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use qml_front::{
    QtType,
    types::{Kind, Member},
};
use qml_syntax::{
    GetSpan,
    ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject},
    js::{
        ArrowFunctionBody, AssignmentExpression, AssignmentOperator, AssignmentTarget,
        BindingIdentifier, CallExpression, Expression, Function, IdentifierReference,
        SimpleAssignmentTarget, Statement, StaticMemberExpression, UnaryOperator, UpdateExpression,
    },
};

use super::{
    design::{children, class},
    diagnostic,
    lint::Cx,
};
use crate::{Diagnostic, lints};

/// Properties a control's user sets: a binding on one is undone by the
/// first click or keystroke, so a handler that copies into one is the way to
/// keep it in step, not a binding written by hand.
const USER_SET: &[&str] = &[
    "text",
    "checked",
    "checkState",
    "value",
    "position",
    "currentIndex",
    "currentValue",
    "editText",
    "cursorPosition",
    "contentX",
    "contentY",
];

/// Handlers that run once as an object is made or goes: they give a
/// property its first value, which is not a mode.
fn is_lifecycle(name: &str) -> bool {
    name.ends_with("onCompleted") || name.ends_with("onDestruction")
}

const WINDOW: &[&str] = &["QQuickWindow", "QQuickWindowQmlImpl", "QtQuick.Window"];
const CONNECTIONS: &[&str] = &["QQmlConnections", "QtQml.Connections"];

pub(super) fn run<'a>(cx: &Cx<'_, 'a>, root: &QmlObject<'a>, out: &mut Vec<Diagnostic>) {
    let mut file = File {
        cx,
        objects: HashMap::new(),
        parents: HashMap::new(),
        sites: Vec::new(),
    };
    file.walk(root, None);
    for member in &root.members {
        if let QmlMember::InlineComponent(inline) = member {
            file.walk(&inline.object, None);
        }
    }
    let scripts: Vec<Script> = file.sites.iter().map(|s| file.script(s)).collect();
    let mut found = Findings { file: &file, out };
    found.mirror_handlers(&scripts);
    let moded = found.modes(&scripts);
    found.visibility(&scripts, &moded);
    found.animations(&scripts);
}

/// A handler or a function, and the object it is written on.
struct Site<'n, 'a> {
    /// As written: `onClicked`, `Keys.onPressed`, `show`.
    name: String,
    owner: usize,
    at: u32,
    body: Body<'n, 'a>,
    handler: bool,
}

#[derive(Clone, Copy)]
enum Body<'n, 'a> {
    Expression(&'n Expression<'a>),
    Statement(&'n Statement<'a>),
    Function(&'n Function<'a>),
}

/// A statement at the top of a site's body.
enum Top<'n, 'a> {
    Expression(&'n Expression<'a>),
    Other,
}

impl<'n, 'a> Body<'n, 'a> {
    /// What the body does, in order, at its top level: a handler written as
    /// a function is its function's body.
    fn top(self) -> Vec<Top<'n, 'a>> {
        let mut out = Vec::new();
        match self {
            Body::Expression(e) => match e.without_parentheses() {
                Expression::ArrowFunctionExpression(arrow) => match &arrow.body {
                    ArrowFunctionBody::FunctionBody(body) => statements(&body.statements, &mut out),
                    concise => match concise.as_expression() {
                        Some(e) => expression(e, &mut out),
                        None => out.push(Top::Other),
                    },
                },
                Expression::FunctionExpression(function) => match &function.body {
                    Some(body) => statements(&body.statements, &mut out),
                    None => out.push(Top::Other),
                },
                e => expression(e, &mut out),
            },
            Body::Statement(s) => statements(std::slice::from_ref(s), &mut out),
            Body::Function(f) => {
                if let Some(body) = &f.body {
                    statements(&body.statements, &mut out);
                }
            }
        }
        out
    }
}

fn statements<'n, 'a>(list: &'n [Statement<'a>], out: &mut Vec<Top<'n, 'a>>) {
    for s in list {
        match s {
            Statement::ExpressionStatement(e) => expression(&e.expression, out),
            Statement::BlockStatement(b) => statements(&b.body, out),
            Statement::EmptyStatement(_) => {}
            _ => out.push(Top::Other),
        }
    }
}

fn expression<'n, 'a>(e: &'n Expression<'a>, out: &mut Vec<Top<'n, 'a>>) {
    match e.without_parentheses() {
        Expression::SequenceExpression(seq) => {
            for e in &seq.expressions {
                expression(e, out);
            }
        }
        e => out.push(Top::Expression(e)),
    }
}

/// What an assignment writes to, as written.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Target {
    /// `shown = …`
    Name(String),
    /// `banner.visible = …`
    Member(String, String),
}

fn target(t: &AssignmentTarget<'_>) -> Option<Target> {
    match t.as_simple_assignment_target()? {
        SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
            Some(Target::Name(id.name.to_string()))
        }
        SimpleAssignmentTarget::StaticMemberExpression(m) => match &m.object {
            Expression::Identifier(id) => Some(Target::Member(
                id.name.to_string(),
                m.property.name.to_string(),
            )),
            _ => None,
        },
        _ => None,
    }
}

/// A value written out: a number, a string, `true`, `null`, an enumeration
/// key or the `qsTr` of a string. Its text, to compare by.
fn constant(source: &str, e: &Expression<'_>) -> Option<String> {
    let text = |e: &Expression<'_>| {
        let span = e.span();
        source[span.start as usize..span.end as usize].to_owned()
    };
    match e.without_parentheses() {
        e @ (Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)) => Some(text(e)),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => Some(text(e)),
        Expression::UnaryExpression(u)
            if u.operator == UnaryOperator::UnaryNegation
                && matches!(u.argument, Expression::NumericLiteral(_)) =>
        {
            Some(text(e))
        }
        Expression::StaticMemberExpression(m) => match &m.object {
            Expression::Identifier(id)
                if is_capitalised(&id.name) && is_capitalised(&m.property.name) =>
            {
                Some(text(e))
            }
            _ => None,
        },
        Expression::CallExpression(c)
            if matches!(&c.callee, Expression::Identifier(id) if id.name == "qsTr")
                && c.arguments.len() == 1
                && matches!(
                    c.arguments[0].as_expression(),
                    Some(Expression::StringLiteral(_))
                ) =>
        {
            Some(text(e))
        }
        _ => None,
    }
}

fn is_capitalised(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// One write a site makes, anywhere in its body.
struct Write {
    target: Target,
    /// What it writes, if it is a value written out with `=`.
    constant: Option<String>,
    at: u32,
}

/// One call of `id.method()`.
struct Call {
    object: String,
    method: String,
    at: u32,
}

/// What a site's body does, all through it.
struct Script {
    /// What the body declares (parameters, `var`, `let`, `const`): a name
    /// of these is not a property.
    locals: HashSet<String>,
    writes: Vec<Write>,
    calls: Vec<Call>,
}

struct Collect<'s> {
    source: &'s str,
    script: Script,
}

impl<'a> Visit<'a> for Collect<'_> {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.script.locals.insert(it.name.to_string());
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let Some(target) = target(&it.left) {
            let plain = it.operator == AssignmentOperator::Assign;
            self.script.writes.push(Write {
                target,
                constant: plain.then(|| constant(self.source, &it.right)).flatten(),
                at: it.span.start,
            });
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        let target = match &it.argument {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                Some(Target::Name(id.name.to_string()))
            }
            SimpleAssignmentTarget::StaticMemberExpression(m) => match &m.object {
                Expression::Identifier(id) => Some(Target::Member(
                    id.name.to_string(),
                    m.property.name.to_string(),
                )),
                _ => None,
            },
            _ => None,
        };
        if let Some(target) = target {
            self.script.writes.push(Write {
                target,
                constant: None,
                at: it.span.start,
            });
        }
        walk::walk_update_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::StaticMemberExpression(m) = &it.callee
            && let Expression::Identifier(id) = &m.object
        {
            self.script.calls.push(Call {
                object: id.name.to_string(),
                method: m.property.name.to_string(),
                at: it.span.start,
            });
        }
        walk::walk_call_expression(self, it);
    }
}

/// A property of one object: the object's index in the tree and the name.
type Key = (usize, String);

struct File<'f, 'c, 'n, 'a> {
    cx: &'f Cx<'c, 'a>,
    objects: HashMap<usize, &'n QmlObject<'a>>,
    parents: HashMap<usize, usize>,
    sites: Vec<Site<'n, 'a>>,
}

impl<'n, 'a> File<'_, '_, 'n, 'a> {
    fn walk(&mut self, object: &'n QmlObject<'a>, parent: Option<usize>) {
        let Some(index) = self.cx.tree.index_of(object) else {
            return;
        };
        self.objects.insert(index, object);
        if let Some(parent) = parent {
            self.parents.insert(index, parent);
        }
        self.members(object, index, "");
        for (child, _) in children(object) {
            self.walk(child, Some(index));
        }
    }

    fn members(&mut self, object: &'n QmlObject<'a>, owner: usize, group: &str) {
        for member in &object.members {
            match member {
                QmlMember::Binding(QmlBinding { name, value, .. }) => {
                    let last = name.parts.last().copied().unwrap_or_default();
                    let is_handler = last.len() > 2
                        && last.starts_with("on")
                        && last[2..].starts_with(char::is_uppercase);
                    let body = match value {
                        QmlBindingValue::Expression(e) => Body::Expression(e),
                        QmlBindingValue::Statement(s) => Body::Statement(s),
                        _ => continue,
                    };
                    if is_handler {
                        self.sites.push(Site {
                            name: format!("{group}{}", name.parts.join(".")),
                            owner,
                            at: name.span.start,
                            body,
                            handler: true,
                        });
                    }
                }
                QmlMember::Function(function) => {
                    let name = function
                        .id
                        .as_ref()
                        .map(|id| id.name.to_string())
                        .unwrap_or_default();
                    // A function named for a signal in a `Connections` is its
                    // handler.
                    let handler = name.starts_with("on") && self.is(owner, CONNECTIONS);
                    self.sites.push(Site {
                        name,
                        owner,
                        at: function.span.start,
                        body: Body::Function(function),
                        handler,
                    });
                }
                QmlMember::Object(child) if qml_front::scope::is_group(child) => {
                    let group = format!("{group}{}.", child.type_name.parts.join("."));
                    self.members(child, owner, &group);
                }
                _ => {}
            }
        }
    }

    fn script(&self, site: &Site<'n, 'a>) -> Script {
        let mut collect = Collect {
            source: self.cx.source,
            script: Script {
                locals: HashSet::new(),
                writes: Vec::new(),
                calls: Vec::new(),
            },
        };
        match site.body {
            Body::Expression(e) => collect.visit_expression(e),
            Body::Statement(s) => collect.visit_statement(s),
            Body::Function(f) => collect.visit_function(f, ScopeFlags::Function),
        }
        collect.script
    }

    fn base(&self, index: usize) -> Option<QtType> {
        let kind = self.cx.tree.objects[index].kind.as_ref()?;
        self.cx.types.base(kind)
    }

    fn is(&self, index: usize, classes: &[&str]) -> bool {
        self.base(index).is_some_and(|ty| ty.0.is_a(classes))
    }

    /// Whether the object has a property of the name: one it declares, or
    /// one of its type.
    fn has(&self, index: usize, name: &str) -> bool {
        let object = &self.cx.tree.objects[index];
        if object.declared.contains_key(name) {
            return true;
        }
        object.kind.as_ref().is_some_and(|kind| {
            matches!(self.cx.types.member(kind, name), Some(Member::Property(_)))
        })
    }

    /// Whether the property is one the user sets on a control, unless the
    /// object declares it or is a plain item of the runtime's. A component
    /// may alias it to a control's (`property alias value: slider.value`).
    fn user_set(&self, key: &Key) -> bool {
        let object = &self.cx.tree.objects[key.0];
        USER_SET.contains(&key.1.as_str())
            && !object.declared.contains_key(key.1.as_str())
            && (!matches!(object.kind, Some(Kind::Builtin(_)))
                || [class::CONTROL, class::TEXT_INPUT, class::VIEW]
                    .iter()
                    .any(|c| self.is(key.0, c)))
    }

    fn context(&self, index: usize) -> usize {
        self.cx.tree.objects[index].context
    }

    /// The object an id names, as seen from an object: its own component's
    /// ids, then those of the components it is written in.
    fn by_id(&self, from: usize, id: &str) -> Option<usize> {
        let mut context = Some(self.context(from));
        while let Some(c) = context {
            if let Some(found) = self
                .cx
                .tree
                .objects
                .iter()
                .position(|o| o.context == c && o.id == Some(id))
            {
                return Some(found);
            }
            context = self.cx.tree.contexts[c].outer;
        }
        None
    }

    /// The property a write in a site on `owner` reaches, where the file
    /// says for certain: the scope object's own, else its component root's,
    /// else that of an object named by its id.
    fn resolve(&self, owner: usize, target: &Target, locals: &HashSet<String>) -> Option<Key> {
        match target {
            Target::Name(name) => {
                if locals.contains(name) {
                    return None;
                }
                if self.has(owner, name) {
                    return Some((owner, name.clone()));
                }
                let root = self.cx.tree.contexts[self.context(owner)].root;
                self.has(root, name).then(|| (root, name.clone()))
            }
            Target::Member(id, name) => {
                if locals.contains(id) {
                    return None;
                }
                Some((self.by_id(owner, id)?, name.clone()))
            }
        }
    }

    /// How a reader names a property: `banner.visible`, or `visible` on an
    /// object with no id.
    fn show(&self, key: &Key) -> String {
        match self.cx.tree.objects[key.0].id {
            Some(id) => format!("{id}.{}", key.1),
            None => key.1.clone(),
        }
    }

    fn line(&self, at: u32) -> usize {
        qml_syntax::position(self.cx.source, at).0
    }

    fn text(&self, e: &Expression<'_>) -> &str {
        let span = e.span();
        &self.cx.source[span.start as usize..span.end as usize]
    }

    /// What `PropertyChanges` and `Binding` objects set: a property that
    /// states or a `Binding` already manage.
    fn managed(&self) -> HashSet<Key> {
        let mut out = HashSet::new();
        for (&index, object) in &self.objects {
            let kind = object.type_name.parts.last().copied().unwrap_or_default();
            if kind != "PropertyChanges" && kind != "Binding" {
                continue;
            }
            let target =
                super::design::bound(object, &["target"]).and_then(|b| match b.expression()? {
                    Expression::Identifier(id) => self.by_id(index, &id.name),
                    _ => None,
                });
            if kind == "Binding" {
                if let (Some(target), Some(property)) = (
                    target,
                    super::design::bound(object, &["property"]).and_then(|b| b.string()),
                ) {
                    out.insert((target, property.to_owned()));
                }
                continue;
            }
            for member in &object.members {
                let QmlMember::Binding(QmlBinding { name, .. }) = member else {
                    continue;
                };
                match name.parts.as_slice() {
                    [id, property] => {
                        if let Some(target) = self.by_id(index, id) {
                            out.insert((target, (*property).to_owned()));
                        }
                    }
                    [property] if !qml_front::scope::CHANGES.contains(property) => {
                        if let Some(target) = target {
                            out.insert((target, (*property).to_owned()));
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// The value an object's own declaration or binding gives the property,
    /// if it has one: `Some(None)` when what it gives is not written out.
    fn initial(&self, key: &Key) -> Option<Option<String>> {
        let object = self.objects.get(&key.0)?;
        for member in &object.members {
            let value = match member {
                QmlMember::Binding(QmlBinding { name, value, .. })
                    if name.parts.len() == 1 && name.parts[0] == key.1 =>
                {
                    value
                }
                QmlMember::Property(p) if p.name.name.as_str() == key.1 => p.value.as_ref()?,
                _ => continue,
            };
            return Some(match value {
                QmlBindingValue::Expression(e) => constant(self.cx.source, e),
                _ => None,
            });
        }
        None
    }

    /// What an animation animates, as `(object, property)`: `Type on
    /// property`, or `target` and `property`/`properties`, or what the
    /// animations of a group animate.
    fn animates(&self, index: usize) -> Vec<Key> {
        let Some(object) = self.objects.get(&index) else {
            return Vec::new();
        };
        if let Some(on) = &object.on {
            return self
                .parents
                .get(&index)
                .map(|&p| vec![(p, on.parts.join("."))])
                .unwrap_or_default();
        }
        let target =
            super::design::bound(object, &["target"]).and_then(|b| match b.expression()? {
                Expression::Identifier(id) => self.by_id(index, &id.name),
                _ => None,
            });
        let names: Vec<String> = ["property", "properties"]
            .iter()
            .filter_map(|p| super::design::bound(object, &[p])?.string())
            .flat_map(|s| s.split(',').map(|n| n.trim().to_owned()))
            .filter(|n| !n.is_empty())
            .collect();
        let mut out: Vec<Key> = match target {
            Some(t) => names.into_iter().map(|n| (t, n)).collect(),
            None => Vec::new(),
        };
        for (child, _) in children(object) {
            if let Some(i) = self.cx.tree.index_of(child)
                && self.is(i, class::ANIMATION)
            {
                out.extend(self.animates(i));
            }
        }
        out
    }
}

struct Findings<'f, 'c, 'n, 'a, 'o> {
    file: &'f File<'f, 'c, 'n, 'a>,
    out: &'o mut Vec<Diagnostic>,
}

impl Findings<'_, '_, '_, '_, '_> {
    fn report(&mut self, at: u32, lint: &lints::Lint, message: String) {
        let cx = self.file.cx;
        self.out.push(diagnostic(
            cx.path,
            cx.source,
            at,
            lint.severity,
            lint.code,
            message,
        ));
    }

    /// Every resolved write of the file, by property.
    fn writes<'s>(&self, scripts: &'s [Script]) -> HashMap<Key, Vec<(usize, &'s Write)>> {
        let mut out: HashMap<Key, Vec<(usize, &'s Write)>> = HashMap::new();
        for (i, (site, script)) in self.file.sites.iter().zip(scripts).enumerate() {
            for write in &script.writes {
                if let Some(key) = self.file.resolve(site.owner, &write.target, &script.locals) {
                    out.entry(key).or_default().push((i, write));
                }
            }
        }
        out
    }

    /// `decl-mirror-handler`.
    fn mirror_handlers(&mut self, scripts: &[Script]) {
        let file = self.file;
        let writes = self.writes(scripts);
        let managed = file.managed();
        for (i, (site, script)) in file.sites.iter().zip(scripts).enumerate() {
            if !site.handler || site.name.contains('.') || file.is(site.owner, CONNECTIONS) {
                continue;
            }
            let Some(changed) = site
                .name
                .strip_prefix("on")
                .and_then(|n| n.strip_suffix("Changed"))
                .filter(|n| !n.is_empty())
            else {
                continue;
            };
            let mut chars = changed.chars();
            let changed: String = chars
                .next()
                .map(|c| c.to_lowercase().chain(chars).collect())
                .unwrap_or_default();
            let [Top::Expression(Expression::AssignmentExpression(assign))] =
                site.body.top().as_slice()
            else {
                continue;
            };
            if assign.operator != AssignmentOperator::Assign {
                continue;
            }
            let Some(key) =
                target(&assign.left).and_then(|t| file.resolve(site.owner, &t, &script.locals))
            else {
                continue;
            };
            let owner_id = file.cx.tree.objects[site.owner].id;
            if reads(&assign.right, &changed, owner_id, &script.locals) != Some(true)
                || key == (site.owner, changed.clone())
                || writes
                    .get(&key)
                    .is_some_and(|ws| ws.iter().any(|(s, _)| *s != i))
                || managed.contains(&key)
                || file.user_set(&key)
                || file.context(key.0) != file.context(site.owner)
                || (key.0 != site.owner && owner_id.is_none())
                || matches!(file.initial(&key), Some(None))
            {
                continue;
            }
            // On another object the value is read through this one's id.
            let value = match owner_id.filter(|_| key.0 != site.owner) {
                Some(id) => qualified(file.cx.source, &assign.right, &changed, id),
                None => file.text(&assign.right).to_owned(),
            };
            let place = if key.0 == site.owner {
                "on this object".to_owned()
            } else {
                match file.cx.tree.objects[key.0].id {
                    Some(id) => format!("on `{id}`"),
                    None => "on the object it sets".to_owned(),
                }
            };
            let message = format!(
                "`{}` only copies `{changed}` into `{}`, which nothing else writes; the binding \
                 `{}: {value}` {place} holds the same value, and the handler can go",
                site.name,
                file.show(&key),
                key.1,
            );
            self.report(site.at, &lints::MIRROR_HANDLER, message);
        }
    }

    /// `decl-mode-by-handlers`. Returns the properties it named.
    fn modes(&mut self, scripts: &[Script]) -> HashSet<Key> {
        let file = self.file;
        // Per site: what its top level sets to values written out.
        let sets: Vec<Vec<(Key, String)>> = file
            .sites
            .iter()
            .zip(scripts)
            .map(|(site, script)| {
                if is_lifecycle(&site.name) {
                    return Vec::new();
                }
                let mut out: Vec<(Key, String)> = Vec::new();
                for top in site.body.top() {
                    let Top::Expression(Expression::AssignmentExpression(a)) = top else {
                        continue;
                    };
                    if a.operator != AssignmentOperator::Assign {
                        continue;
                    }
                    let (Some(key), Some(value)) = (
                        target(&a.left).and_then(|t| file.resolve(site.owner, &t, &script.locals)),
                        constant(file.cx.source, &a.right),
                    ) else {
                        continue;
                    };
                    // Setting `state` is already the declarative way; what
                    // a user sets, and what a store of values holds, are
                    // not what a mode looks like.
                    if key.1 == "state" || file.user_set(&key) || !file.is(key.0, class::ITEM) {
                        continue;
                    }
                    out.retain(|(k, _)| *k != key);
                    out.push((key, value));
                }
                out
            })
            .collect();
        let n = sets.len();
        let mut group: Vec<usize> = (0..n).collect();
        fn find(group: &mut [usize], i: usize) -> usize {
            let mut i = i;
            while group[i] != i {
                group[i] = group[group[i]];
                i = group[i];
            }
            i
        }
        let mut keys: HashMap<usize, Vec<Key>> = HashMap::new();
        let mut paired = vec![false; n];
        for i in 0..n {
            if sets[i].len() < 2 {
                continue;
            }
            for j in i + 1..n {
                let differ: Vec<&Key> = sets[i]
                    .iter()
                    .filter_map(|(k, v)| {
                        let (_, w) = sets[j].iter().find(|(l, _)| l == k)?;
                        (v != w).then_some(k)
                    })
                    .collect();
                let common = sets[i]
                    .iter()
                    .filter(|(k, _)| sets[j].iter().any(|(l, _)| l == k))
                    .count();
                if common < 2 || differ.len() < 2 {
                    continue;
                }
                paired[i] = true;
                paired[j] = true;
                let (a, b) = (find(&mut group, i), find(&mut group, j));
                let root = a.min(b);
                group[a.max(b)] = root;
                let mut moved = keys.remove(&a.max(b)).unwrap_or_default();
                let list = keys.entry(root).or_default();
                list.append(&mut moved);
                for k in differ {
                    if !list.contains(k) {
                        list.push(k.clone());
                    }
                }
            }
        }
        let mut named = HashSet::new();
        let mut clusters: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in (0..n).filter(|&i| paired[i]) {
            let root = find(&mut group, i);
            clusters.entry(root).or_default().push(i);
        }
        let mut roots: Vec<usize> = clusters.keys().copied().collect();
        roots.sort_unstable();
        for root in roots {
            let members = &clusters[&root];
            let Some(props) = keys.get(&root) else {
                continue;
            };
            let handlers: Vec<String> = members
                .iter()
                .map(|&i| {
                    let site = &file.sites[i];
                    format!("`{}` (line {})", site.name, file.line(site.at))
                })
                .collect();
            let shown: Vec<String> = props
                .iter()
                .map(|k| format!("`{}`", file.show(k)))
                .collect();
            let first = members
                .iter()
                .map(|&i| file.sites[i].at)
                .min()
                .expect("a cluster has members");
            let message = format!(
                "{} each set {} to values of their own: that is a mode per handler; make each \
                 a `State` with `PropertyChanges`, and let the handlers set `state` (or give \
                 each state a `when:`)",
                and_list(&handlers),
                and_list(&shown),
            );
            self.report(first, &lints::MODE_BY_HANDLERS, message);
            named.extend(props.iter().cloned());
        }
        named
    }

    /// `decl-visibility-juggling`.
    fn visibility(&mut self, scripts: &[Script], moded: &HashSet<Key>) {
        let file = self.file;
        let managed = file.managed();
        let writes = self.writes(scripts);
        let mut keys: Vec<&Key> = writes
            .keys()
            .filter(|k| k.1 == "visible" || k.1 == "opacity")
            .collect();
        keys.sort();
        for key in keys {
            if moded.contains(key)
                || managed.contains(key)
                || file.is(key.0, class::POPUP)
                || file.is(key.0, WINDOW)
            {
                continue;
            }
            let set: Vec<&(usize, &Write)> = writes[key]
                .iter()
                .filter(|(s, w)| w.constant.is_some() && !is_lifecycle(&file.sites[*s].name))
                .collect();
            let mut sites: Vec<usize> = set.iter().map(|(s, _)| *s).collect();
            sites.dedup();
            let values: HashSet<&str> = set
                .iter()
                .filter_map(|(_, w)| w.constant.as_deref())
                .collect();
            if sites.len() < 2 || values.len() < 2 {
                continue;
            }
            let handlers: Vec<String> = sites
                .iter()
                .map(|&i| {
                    let site = &file.sites[i];
                    format!("`{}` (line {})", site.name, file.line(site.at))
                })
                .collect();
            let first = set.iter().map(|(_, w)| w.at).min().expect("two writes");
            let message = format!(
                "`{}` is set by hand from {} handlers, {}: bind it to what decides it, or make \
                 the shown and hidden states and give them a transition, which the motion rules \
                 can check",
                file.show(key),
                sites.len(),
                and_list(&handlers),
            );
            self.report(first, &lints::VISIBILITY_JUGGLING, message);
        }
    }

    /// `decl-imperative-animation`.
    fn animations(&mut self, scripts: &[Script]) {
        let file = self.file;
        // Every start of an animation from script: (site, animation, where).
        let mut starts: Vec<(usize, usize, u32)> = Vec::new();
        for (i, (site, script)) in file.sites.iter().zip(scripts).enumerate() {
            for call in &script.calls {
                if (call.method == "start" || call.method == "restart")
                    && !script.locals.contains(&call.object)
                    && let Some(anim) = file.by_id(site.owner, &call.object)
                    && file.is(anim, class::ANIMATION)
                {
                    starts.push((i, anim, call.at));
                }
            }
            for write in &script.writes {
                if let Target::Member(id, name) = &write.target
                    && name == "running"
                    && write.constant.as_deref() == Some("true")
                    && !script.locals.contains(id)
                    && let Some(anim) = file.by_id(site.owner, id)
                    && file.is(anim, class::ANIMATION)
                {
                    starts.push((i, anim, write.at));
                }
            }
        }
        starts.sort_by_key(|s| s.2);
        let name = |i: usize| file.cx.tree.objects[i].id.unwrap_or("this animation");
        let mut chained = HashSet::new();
        for &(s, anim, at) in &starts {
            let site = &file.sites[s];
            if site.handler
                && (site.name == "onFinished" || site.name == "onStopped")
                && site.owner != anim
                && file.is(site.owner, class::ANIMATION)
            {
                chained.insert(site.owner);
                chained.insert(anim);
                let message = format!(
                    "`{}` starts `{}` from `{}`: put both in one `SequentialAnimation`, which \
                     runs them in turn and is started, stopped and read as one",
                    name(site.owner),
                    name(anim),
                    site.name,
                );
                self.report(at, &lints::IMPERATIVE_ANIMATION, message);
            }
        }
        // Opposite animations of one property, each started by hand: the
        // states it moves between, with a transition.
        let mut by_key: HashMap<Key, Vec<(usize, u32)>> = HashMap::new();
        for &(_, anim, at) in &starts {
            if chained.contains(&anim) {
                continue;
            }
            for key in file.animates(anim) {
                let list = by_key.entry(key).or_default();
                if !list.iter().any(|(a, _)| *a == anim) {
                    list.push((anim, at));
                }
            }
        }
        let mut told: HashSet<Vec<usize>> = HashSet::new();
        let mut keys: Vec<&Key> = by_key.keys().collect();
        keys.sort();
        for key in keys {
            let list = &by_key[key];
            if list.len() < 2 {
                continue;
            }
            let mut anims: Vec<usize> = list.iter().map(|(a, _)| *a).collect();
            anims.sort_unstable();
            if !told.insert(anims) {
                continue;
            }
            let names: Vec<String> = list
                .iter()
                .map(|(a, _)| format!("`{}`", name(*a)))
                .collect();
            let at = list.iter().map(|(_, at)| *at).min().expect("two starts");
            let message = format!(
                "{} all animate `{}` and are started from script: make the states they move \
                 between and give them a `Transition`, or put a `Behavior on {}` on it",
                and_list(&names),
                file.show(key),
                key.1,
            );
            self.report(at, &lints::IMPERATIVE_ANIMATION, message);
        }
    }
}

/// Whether an expression reads only `changed` (unqualified, or through the
/// id of the object it is on) and values written out, with no effect of its
/// own: what a binding of it would follow is then exactly what the handler
/// follows. `Some(true)` if it reads `changed`.
fn reads(
    e: &Expression<'_>,
    changed: &str,
    owner: Option<&str>,
    locals: &HashSet<String>,
) -> Option<bool> {
    let both = |a: &Expression<'_>, b: &Expression<'_>| {
        Some(reads(a, changed, owner, locals)? | reads(b, changed, owner, locals)?)
    };
    match e.without_parentheses() {
        Expression::Identifier(id) => {
            (id.name == changed && !locals.contains(changed)).then_some(true)
        }
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => Some(false),
        Expression::TemplateLiteral(t) => {
            let mut any = false;
            for e in &t.expressions {
                any |= reads(e, changed, owner, locals)?;
            }
            Some(any)
        }
        Expression::StaticMemberExpression(m) => match &m.object {
            Expression::Identifier(id)
                if is_capitalised(&id.name) && is_capitalised(&m.property.name) =>
            {
                Some(false)
            }
            Expression::Identifier(id) if Some(id.name.as_str()) == owner => {
                (m.property.name == changed).then_some(true)
            }
            object => reads(object, changed, owner, locals),
        },
        Expression::UnaryExpression(u)
            if !matches!(u.operator, UnaryOperator::Delete | UnaryOperator::Void) =>
        {
            reads(&u.argument, changed, owner, locals)
        }
        Expression::BinaryExpression(b) => both(&b.left, &b.right),
        Expression::LogicalExpression(l) => both(&l.left, &l.right),
        Expression::ConditionalExpression(c) => Some(
            reads(&c.test, changed, owner, locals)?
                | reads(&c.consequent, changed, owner, locals)?
                | reads(&c.alternate, changed, owner, locals)?,
        ),
        Expression::CallExpression(c) => {
            let Expression::StaticMemberExpression(m) = &c.callee else {
                return None;
            };
            let mut any = match &m.object {
                Expression::Identifier(id) if id.name == "Math" => false,
                object if PURE_METHODS.contains(&m.property.name.as_str()) => {
                    reads(object, changed, owner, locals)?
                }
                _ => return None,
            };
            for argument in &c.arguments {
                any |= reads(argument.as_expression()?, changed, owner, locals)?;
            }
            Some(any)
        }
        _ => None,
    }
}

/// The source of `e` with each unqualified `name` read through `id`.
fn qualified(source: &str, e: &Expression<'_>, name: &str, id: &str) -> String {
    struct Names<'n> {
        name: &'n str,
        at: Vec<u32>,
    }
    impl<'a> Visit<'a> for Names<'_> {
        fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
            if it.name == self.name {
                self.at.push(it.span.start);
            }
        }
        fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
            // `a.name` is not `name`.
            self.visit_expression(&it.object);
        }
    }
    let mut names = Names {
        name,
        at: Vec::new(),
    };
    names.visit_expression(e);
    let span = e.span();
    let mut out = String::new();
    let mut from = span.start as usize;
    for at in names.at {
        out.push_str(&source[from..at as usize]);
        out.push_str(id);
        out.push('.');
        from = at as usize;
    }
    out.push_str(&source[from..span.end as usize]);
    out
}

/// Methods of a value that give a new value and change nothing.
const PURE_METHODS: &[&str] = &["toFixed", "toString", "trim", "toLowerCase", "toUpperCase"];

/// `a`, `a and b`, `a, b and c`.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
