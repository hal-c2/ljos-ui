//! What changed between two versions of one file, object by object.
//!
//! An object is named by its [`KeySeg`]s, the steps hot reload names it by
//! ([`ljos_ir::ObjectKey`]): its `id`, else its literal `objectName`, else
//! its type and how many unnamed siblings of that type come before it. The
//! last kind depends on order: put a `Rectangle` before two others and the
//! keys of both shift. So the two trees are not matched by key. Objects
//! with an `id` (or an `objectName` no other object has) are matched by it
//! wherever they are; unnamed siblings of one type are matched by what they
//! contain, those written exactly alike first, the rest in order between
//! them; and an object that left its parent is found by its content where
//! it went. What a match changes in the keys is reported, so a key from
//! before the change can be carried over to after it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

use ljos_ir::KeySeg;
use qml_syntax::{
    Allocator,
    ast::{QmlBindingValue, QmlMember, QmlObject},
};

use crate::{Error, NodePath, Step, first_problem, is_group, value_span};

/// One object of a file: where it is written and how it is keyed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Object {
    pub path: NodePath,
    /// The component the object belongs to when it is not the file's own:
    /// [`ljos_ir::ComponentDef::id`] after the file's key. `#Name` for an
    /// inline component (`component Name: T { }`); for a template, the key
    /// of the object it is written in and the property's own name,
    /// `/Id/Type[0].delegate` (`.left` for `swipe.left`), then `[n]` for the
    /// n-th other template written there.
    pub component: Option<String>,
    /// The steps of its key from its component's root, which has none:
    /// [`ljos_ir::ObjectKey::segments`].
    pub key: Vec<KeySeg>,
    /// Its type as written (`Rectangle`, `T.Button`).
    pub ty: String,
}

/// One difference between two versions of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeChange {
    /// An object of the new text, with everything written in it, that the
    /// old one has not.
    Added(Object),
    /// An object of the old text, with everything written in it, that the
    /// new one has not.
    Removed(Object),
    /// The object went to another parent or slot, or to another place among
    /// its parent's children. What is written in it moved with it.
    Moved { was: Object, now: Object },
    /// The object stayed but its key is not what it was: an unnamed sibling
    /// of its type came or went before it, or it gained or lost its `id`
    /// or `objectName`. The keys of the objects in it change likewise.
    Rekeyed { was: Object, now: Object },
    /// It is the same object (by `id` or `objectName`) of another type.
    Retyped { was: Object, now: Object },
    /// One binding, or the value of a property declaration, by its dotted
    /// name. `None` is not written; the text is as written, after the `:`.
    Changed {
        was: Object,
        now: Object,
        name: String,
        old: Option<String>,
        new: Option<String>,
    },
}

/// Whether an object written in `owner` as the value of `property`
/// (`default` for a child) is a template, a component of its own that is
/// made later (a delegate), by names alone: what Qt's types say for the
/// common cases, and what the compiler takes for a type it does not know.
/// The children of `Component`, and of `Repeater`, `Instantiator` and
/// `DelegateChoice`, whose default property is their delegate; and the value
/// of a property named `delegate` or ending in `Delegate` or `Component`.
#[must_use]
pub fn template_by_name(owner: &Object, property: &str) -> bool {
    let ty = owner.ty.rsplit('.').next().unwrap_or(&owner.ty);
    if property == "default" {
        return matches!(
            ty,
            "Component" | "Repeater" | "Instantiator" | "DelegateChoice"
        );
    }
    let last = property.rsplit('.').next().unwrap_or(property);
    last == "delegate" || last.ends_with("Delegate") || last.ends_with("Component")
}

/// Every object of `source` with its key, root first, in the order written;
/// templates by [`template_by_name`].
///
/// # Errors
/// [`Error::Syntax`] when `source` does not parse.
pub fn keys(source: &str) -> Result<Vec<Object>, Error> {
    keys_with(source, &template_by_name)
}

/// [`keys`], with `template` saying which values are templates (see
/// [`template_by_name`]): a caller that knows the types can say it
/// exactly. The value of a property declared `Component` always is one.
///
/// # Errors
/// [`Error::Syntax`] when `source` does not parse.
pub fn keys_with(source: &str, template: &Templates<'_>) -> Result<Vec<Object>, Error> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        return Err(Error::Syntax(first_problem(&parsed, source)));
    }
    let tree = Tree::of(source, &parsed.document.root, template);
    Ok((0..tree.nodes.len()).map(|i| tree.object(i)).collect())
}

/// Which values are templates: the object they are written in and the
/// property, as [`template_by_name`] takes them.
pub type Templates<'f> = dyn Fn(&Object, &str) -> bool + 'f;

/// What changed from `old` to `new`, two versions of one file: removals
/// first, then additions, then for each object both have, in the order of
/// the new text, a move or a key change, a type change, and its binding
/// changes by name. Templates are found by [`template_by_name`].
///
/// # Errors
/// [`Error::Syntax`] when either text does not parse.
pub fn diff(old: &str, new: &str) -> Result<Vec<TreeChange>, Error> {
    diff_with(old, new, &template_by_name)
}

/// [`diff`], with templates found as [`keys_with`] finds them.
///
/// # Errors
/// [`Error::Syntax`] when either text does not parse.
pub fn diff_with(old: &str, new: &str, template: &Templates<'_>) -> Result<Vec<TreeChange>, Error> {
    let allocator = Allocator::default();
    let was = qml_syntax::parse(&allocator, old);
    if !was.is_ok() {
        return Err(Error::Syntax(format!(
            "the old text: {}",
            first_problem(&was, old)
        )));
    }
    let now = qml_syntax::parse(&allocator, new);
    if !now.is_ok() {
        return Err(Error::Syntax(format!(
            "the new text: {}",
            first_problem(&now, new)
        )));
    }
    let a = Tree::of(old, &was.document.root, template);
    let b = Tree::of(new, &now.document.root, template);
    let mut m = Matching::new(&a, &b);
    m.run();
    Ok(m.changes())
}

/// Where an object is written in its parent.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Slot {
    Root,
    Child,
    /// The value of a binding or a property declaration, with whether that
    /// is declared `Component`.
    Value(String, bool),
    Component(String),
}

/// What is written in an object, in order, as the compiler counts it.
enum Entry {
    Object(usize),
    /// One of a list value's objects (`states: [ ]`), by type and whether
    /// it has an `id` or an `objectName`: no path reaches it, it is part of
    /// the binding, but it is counted among the unnamed siblings of its
    /// type.
    Listed(String, bool),
}

struct Node {
    path: NodePath,
    slot: Slot,
    parent: Option<usize>,
    component: Option<String>,
    /// Whether it is the root of a component, which has no step of its own.
    root: bool,
    ty: String,
    id: Option<String>,
    name: Option<String>,
    /// Bindings and property values by dotted name, as written.
    bindings: BTreeMap<String, String>,
    declarations: Vec<String>,
    entries: Vec<Entry>,
    /// Objects written in it, in order.
    children: Vec<usize>,
    key: Vec<KeySeg>,
    /// Hash of everything written in it, spacing aside.
    print: u64,
}

struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    fn of(source: &str, root: &QmlObject<'_>, template: &Templates<'_>) -> Self {
        let mut tree = Self { nodes: Vec::new() };
        tree.add(source, root, NodePath::root(), Slot::Root, None);
        tree.key(template);
        tree.print();
        tree
    }

    fn object(&self, i: usize) -> Object {
        let n = &self.nodes[i];
        Object {
            path: n.path.clone(),
            component: n.component.clone(),
            key: n.key.clone(),
            ty: n.ty.clone(),
        }
    }

    /// Adds `object` and what is written in it, each object before the
    /// objects in it.
    fn add(
        &mut self,
        source: &str,
        object: &QmlObject<'_>,
        path: NodePath,
        slot: Slot,
        parent: Option<usize>,
    ) -> usize {
        let at = self.nodes.len();
        let mut bindings = BTreeMap::new();
        let mut declarations = Vec::new();
        let mut inner = Vec::new();
        members(
            source,
            object,
            "",
            &mut 0,
            &mut bindings,
            &mut declarations,
            &mut inner,
        );
        let id = bindings.get("id").cloned();
        let name = bindings
            .get("objectName")
            .and_then(|v: &String| string_literal(v))
            .map(str::to_owned);
        self.nodes.push(Node {
            path,
            root: parent.is_none(),
            slot,
            parent,
            component: None,
            ty: object.type_name.to_string(),
            id,
            name,
            bindings,
            declarations,
            entries: Vec::new(),
            children: Vec::new(),
            key: Vec::new(),
            print: 0,
        });
        let mut entries = Vec::new();
        let mut children = Vec::new();
        for item in inner {
            match item {
                Inner::Listed(ty, named) => entries.push(Entry::Listed(ty, named)),
                Inner::Object(child, step, declared) => {
                    let slot = match &step {
                        Step::Child(_) => Slot::Child,
                        Step::Value(name) => Slot::Value(name.clone(), declared),
                        Step::Component(name) => Slot::Component(name.clone()),
                    };
                    let mut path = self.nodes[at].path.clone();
                    path.0.push(step);
                    let index = self.add(source, child, path, slot, Some(at));
                    entries.push(Entry::Object(index));
                    children.push(index);
                }
            }
        }
        self.nodes[at].entries = entries;
        self.nodes[at].children = children;
        at
    }

    /// Keys every object from the root down, as the compiler does: an
    /// object's step is counted among its siblings, and a template is named
    /// by the key of the object it is written in.
    fn key(&mut self, template: &Templates<'_>) {
        let mut count: HashMap<String, u32> = HashMap::new();
        for at in 0..self.nodes.len() {
            let owner = self.object(at);
            let mut typed: HashMap<String, u32> = HashMap::new();
            let mut steps = Vec::new();
            for entry in &self.nodes[at].entries {
                let (ty, named, child) = match entry {
                    Entry::Listed(ty, named) => (ty.clone(), *named, None),
                    Entry::Object(c) => {
                        let n = &self.nodes[*c];
                        let property = match &n.slot {
                            Slot::Component(name) => {
                                steps.push((*c, None, Some(format!("#{name}"))));
                                continue;
                            }
                            Slot::Value(name, true) => Some(name.as_str()),
                            Slot::Value(name, false) => {
                                template(&owner, name).then_some(name.as_str())
                            }
                            Slot::Child => template(&owner, "default").then_some("default"),
                            Slot::Root => None,
                        };
                        if let Some(property) = property {
                            let mut id = owner.component.clone().unwrap_or_default();
                            for seg in &owner.key {
                                id.push('/');
                                id.push_str(&seg_text(seg));
                            }
                            id.push('.');
                            id.push_str(own_name(property));
                            let nth = count.entry(id.clone()).or_insert(0);
                            if *nth > 0 {
                                id.push_str(&format!("[{nth}]"));
                            }
                            *nth += 1;
                            steps.push((*c, None, Some(id)));
                            continue;
                        }
                        (n.ty.clone(), n.id.is_some() || n.name.is_some(), Some(*c))
                    }
                };
                let nth = if named {
                    None
                } else {
                    let nth = typed.entry(ty.clone()).or_insert(0);
                    *nth += 1;
                    Some(*nth - 1)
                };
                if let Some(c) = child {
                    let n = &self.nodes[c];
                    let seg = match (&n.id, &n.name, nth) {
                        (Some(id), _, _) => KeySeg::Id(id.clone()),
                        (None, Some(name), _) => KeySeg::Named(name.clone()),
                        (None, None, nth) => KeySeg::Typed {
                            ty,
                            nth: nth.unwrap_or(0),
                        },
                    };
                    steps.push((c, Some(seg), None));
                }
            }
            for (c, seg, component) in steps {
                let n = &mut self.nodes[c];
                match (seg, component) {
                    (Some(seg), _) => {
                        n.key = owner.key.clone();
                        n.key.push(seg);
                        n.component = owner.component.clone();
                    }
                    (None, component) => {
                        n.root = true;
                        n.component = component;
                    }
                }
            }
        }
    }

    /// Hashes each object from the leaves up.
    fn print(&mut self) {
        for at in (0..self.nodes.len()).rev() {
            let n = &self.nodes[at];
            let mut h = DefaultHasher::new();
            n.ty.hash(&mut h);
            for (name, value) in &n.bindings {
                (name, squeeze(value)).hash(&mut h);
            }
            n.declarations.hash(&mut h);
            for &c in &n.children {
                (self.nodes[c].slot == Slot::Child).hash(&mut h);
                self.nodes[c].print.hash(&mut h);
            }
            self.nodes[at].print = h.finish();
        }
    }

    fn parent_slot_children(&self, parent: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].slot == Slot::Child)
    }
}

/// The name a template is known by in its owner: a grouped property's own
/// (`left` of `swipe.left`), an attached one's after its type (`modal` of
/// `T.Overlay.modal`).
fn own_name(property: &str) -> &str {
    let parts: Vec<&str> = property.split('.').collect();
    if parts[0].starts_with(|c: char| c.is_ascii_uppercase()) {
        let skip = parts
            .iter()
            .take_while(|p| p.starts_with(|c: char| c.is_ascii_uppercase()))
            .count()
            .min(parts.len() - 1);
        let at = parts[..skip].iter().map(|p| p.len() + 1).sum::<usize>();
        return &property[at..];
    }
    parts[parts.len() - 1]
}

/// A key step as the compiler writes it into a template's name.
fn seg_text(seg: &KeySeg) -> String {
    match seg {
        KeySeg::Id(id) => id.clone(),
        KeySeg::Named(name) => format!("'{name}'"),
        KeySeg::ModelRow(row) => format!("[{row}]"),
        KeySeg::Typed { ty, nth } => format!("{ty}[{nth}]"),
    }
}

/// An object written in an object's members.
enum Inner<'q, 'a> {
    /// With whether it is the value of a property declared `Component`.
    Object(&'q QmlObject<'a>, Step, bool),
    /// One of a list value's objects, by type and whether it has an `id` or
    /// an `objectName`: no path reaches it, it is part of the binding.
    Listed(String, bool),
}

#[allow(clippy::too_many_arguments)]
fn members<'q, 'a>(
    source: &str,
    object: &'q QmlObject<'a>,
    prefix: &str,
    child: &mut usize,
    bindings: &mut BTreeMap<String, String>,
    declarations: &mut Vec<String>,
    inner: &mut Vec<Inner<'q, 'a>>,
) {
    for member in &object.members {
        match member {
            QmlMember::Object(group) if is_group(group) => {
                let prefix = format!("{prefix}{}.", group.type_name);
                members(source, group, &prefix, child, bindings, declarations, inner);
            }
            QmlMember::Object(o) => {
                if prefix.is_empty() {
                    *child += 1;
                    inner.push(Inner::Object(o, Step::Child(*child - 1), false));
                }
            }
            QmlMember::Binding(b) => {
                let name = format!("{prefix}{}", b.name);
                match &b.value {
                    QmlBindingValue::Object(o) => {
                        inner.push(Inner::Object(o, Step::Value(name), false))
                    }
                    QmlBindingValue::Objects(list) => {
                        listed(source, list, inner);
                        let text = qml_syntax::text(source, value_span(source, b));
                        bindings.insert(name, text.to_owned());
                    }
                    _ => {
                        let text = qml_syntax::text(source, value_span(source, b));
                        bindings.insert(name, text.to_owned());
                    }
                }
            }
            QmlMember::Property(p) if prefix.is_empty() => {
                let name = p.name.name.to_string();
                declarations.push(squeeze(&source[p.span.start as usize..p.span.end as usize]));
                match &p.value {
                    Some(QmlBindingValue::Object(o)) => {
                        let component = p
                            .type_name
                            .as_ref()
                            .is_some_and(|t| t.name.as_simple() == Some("Component") && !t.is_list);
                        inner.push(Inner::Object(o, Step::Value(name), component));
                    }
                    Some(value) => {
                        if let QmlBindingValue::Objects(list) = value {
                            listed(source, list, inner);
                        }
                        let text =
                            after_colon(source, p.name.span.end as usize, p.span.end as usize);
                        bindings.insert(name, text.to_owned());
                    }
                    None => {}
                }
            }
            QmlMember::InlineComponent(c) if prefix.is_empty() => {
                inner.push(Inner::Object(
                    &c.object,
                    Step::Component(c.name.name.to_string()),
                    false,
                ));
            }
            QmlMember::Signal(s) => declarations.push(squeeze(qml_syntax::text(source, s.span))),
            QmlMember::Function(f) => declarations.push(squeeze(qml_syntax::text(source, f.span))),
            QmlMember::Enum(e) => declarations.push(squeeze(qml_syntax::text(source, e.span))),
            _ => {}
        }
    }
}

fn listed<'q, 'a>(source: &str, list: &'q [QmlObject<'a>], inner: &mut Vec<Inner<'q, 'a>>) {
    for o in list {
        let named = o.members.iter().any(|m| match m {
            QmlMember::Binding(b) => {
                let name = b.name.to_string();
                name == "id"
                    || (name == "objectName"
                        && string_literal(qml_syntax::text(source, value_span(source, b)))
                            .is_some())
            }
            _ => false,
        });
        inner.push(Inner::Listed(o.type_name.to_string(), named));
    }
}

/// The value of a property declaration: what follows the `:` after its
/// name, without a closing `;`.
fn after_colon(source: &str, name_end: usize, end: usize) -> &str {
    let rest = &source[name_end..end];
    let rest = rest.find(':').map_or("", |i| &rest[i + 1..]);
    rest.trim().trim_end_matches(';').trim_end()
}

/// The string a plain literal (`"x"`, `'x'`) holds; `None` for anything
/// else, an escape included, which the compiler would read otherwise.
fn string_literal(text: &str) -> Option<&str> {
    let text = text.trim();
    let quote = text.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let inside = text.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inside.contains(quote) && !inside.contains('\\')).then_some(inside)
}

/// Text with runs of white space made one space, so spacing alone does not
/// make two objects differ.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Matching<'t> {
    a: &'t Tree,
    b: &'t Tree,
    /// For each object of `a`, its object in `b`; and back.
    ab: Vec<Option<usize>>,
    ba: Vec<Option<usize>>,
}

impl<'t> Matching<'t> {
    fn new(a: &'t Tree, b: &'t Tree) -> Self {
        Self {
            a,
            b,
            ab: vec![None; a.nodes.len()],
            ba: vec![None; b.nodes.len()],
        }
    }

    fn pair(&mut self, i: usize, j: usize) {
        self.ab[i] = Some(j);
        self.ba[j] = Some(i);
    }

    fn run(&mut self) {
        self.pair(0, 0);
        // Names first, wherever the objects are: an `id` is one object in
        // its component, and so is an `objectName` no other object has.
        for (i, j) in unique_pairs(self.a, self.b, |n| n.id.clone()) {
            self.pair(i, j);
        }
        for (i, j) in unique_pairs(self.a, self.b, |n| n.name.clone()) {
            if self.ab[i].is_none()
                && self.ba[j].is_none()
                && self.a.nodes[i].ty == self.b.nodes[j].ty
            {
                self.pair(i, j);
            }
        }
        let mut work: Vec<(usize, usize)> = vec![(0, 0)];
        work.extend((1..self.a.nodes.len()).filter_map(|i| self.ab[i].map(|j| (i, j))));
        self.descend(work);
        // What left its parent unnamed is found by its content, when that
        // is written once among what is left on each side.
        let gone: Vec<usize> = (1..self.a.nodes.len())
            .filter(|&i| {
                self.ab[i].is_none() && self.a.nodes[i].parent.is_some_and(|p| self.ab[p].is_some())
            })
            .collect();
        let came: Vec<usize> = (1..self.b.nodes.len())
            .filter(|&j| self.ba[j].is_none())
            .collect();
        // Per (print, type): the objects that left, and those that came.
        type Sides = (Vec<usize>, Vec<usize>);
        let mut once: HashMap<(u64, &str), Sides> = HashMap::new();
        for &i in &gone {
            let n = &self.a.nodes[i];
            once.entry((n.print, &n.ty)).or_default().0.push(i);
        }
        for &j in &came {
            let n = &self.b.nodes[j];
            if let Some(e) = once.get_mut(&(n.print, n.ty.as_str())) {
                e.1.push(j);
            }
        }
        let mut found: Vec<(usize, usize)> = once
            .into_values()
            .filter(|(x, y)| x.len() == 1 && y.len() == 1)
            .map(|(x, y)| (x[0], y[0]))
            .collect();
        found.sort_unstable();
        for &(i, j) in &found {
            self.pair(i, j);
        }
        self.descend(found);
    }

    /// Matches what is written in each pair of matched objects, and in
    /// those, down the trees.
    fn descend(&mut self, mut work: Vec<(usize, usize)>) {
        while let Some((i, j)) = work.pop() {
            let (a, b) = (self.a, self.b);
            // `contentItem:` and `component Name:` hold one object each.
            for &x in &a.nodes[i].children {
                if self.ab[x].is_some() || a.nodes[x].slot == Slot::Child {
                    continue;
                }
                let y = b.nodes[j].children.iter().copied().find(|&y| {
                    self.ba[y].is_none()
                        && b.nodes[y].slot == a.nodes[x].slot
                        && b.nodes[y].ty == a.nodes[x].ty
                });
                if let Some(y) = y {
                    self.pair(x, y);
                    work.push((x, y));
                }
            }
            // Unnamed children: those already paired, and those written
            // alike, whatever their type, anchor the rest in order, which
            // pair up type by type between. Anchoring over all siblings
            // keeps two identical ones apart by what stands between them.
            let xs: Vec<usize> = a.parent_slot_children(i).collect();
            let ys: Vec<usize> = b.parent_slot_children(j).collect();
            let open = |x: usize, y: usize| self.ab[x].is_none() && self.ba[y].is_none();
            let pairs = align(
                &xs,
                &ys,
                |x, y| {
                    self.ab[x] == Some(y)
                        || open(x, y)
                            && a.nodes[x].ty == b.nodes[y].ty
                            && a.nodes[x].print == b.nodes[y].print
                },
                |x, y| open(x, y) && a.nodes[x].ty == b.nodes[y].ty,
            );
            let pairs: Vec<(usize, usize)> =
                pairs.into_iter().filter(|&(x, y)| open(x, y)).collect();
            for (x, y) in pairs {
                self.pair(x, y);
                work.push((x, y));
            }
        }
    }

    fn changes(&self) -> Vec<TreeChange> {
        let (a, b) = (self.a, self.b);
        let mut out = Vec::new();
        let outermost = |t: &Tree, taken: &[Option<usize>], i: usize| {
            taken[i].is_none() && t.nodes[i].parent.is_some_and(|p| taken[p].is_some())
        };
        for i in 0..a.nodes.len() {
            if outermost(a, &self.ab, i) {
                out.push(TreeChange::Removed(a.object(i)));
            }
        }
        for j in 0..b.nodes.len() {
            if outermost(b, &self.ba, j) {
                out.push(TreeChange::Added(b.object(j)));
            }
        }
        let reordered = self.reordered();
        for j in 0..b.nodes.len() {
            let Some(i) = self.ba[j] else { continue };
            let (x, y) = (&a.nodes[i], &b.nodes[j]);
            let (was, now) = (a.object(i), b.object(j));
            let moved = j != 0
                && (x.parent.and_then(|p| self.ab[p]) != y.parent
                    || x.slot != y.slot
                    || reordered.contains(&j));
            if moved {
                out.push(TreeChange::Moved {
                    was: was.clone(),
                    now: now.clone(),
                });
            } else if x.key.last() != y.key.last() {
                out.push(TreeChange::Rekeyed {
                    was: was.clone(),
                    now: now.clone(),
                });
            }
            if x.ty != y.ty {
                out.push(TreeChange::Retyped {
                    was: was.clone(),
                    now: now.clone(),
                });
            }
            let names: BTreeSet<&String> = x.bindings.keys().chain(y.bindings.keys()).collect();
            for name in names {
                let (old, new) = (x.bindings.get(name), y.bindings.get(name));
                if old != new {
                    out.push(TreeChange::Changed {
                        was: was.clone(),
                        now: now.clone(),
                        name: name.clone(),
                        old: old.cloned(),
                        new: new.cloned(),
                    });
                }
            }
        }
        out
    }

    /// The matched children that did not keep their order among the
    /// children both versions of their parent share: those outside the
    /// longest run that did.
    fn reordered(&self) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        for j in 0..self.b.nodes.len() {
            let Some(i) = self.ba[j] else { continue };
            let kept: Vec<usize> = self
                .a
                .parent_slot_children(i)
                .filter_map(|x| self.ab[x])
                .filter(|&y| {
                    self.b.nodes[y].parent == Some(j) && self.b.nodes[y].slot == Slot::Child
                })
                .collect();
            let order: Vec<usize> = kept
                .iter()
                .map(|y| {
                    self.b.nodes[j]
                        .children
                        .iter()
                        .position(|c| c == y)
                        .unwrap_or(0)
                })
                .collect();
            let stay = longest_rising(&order);
            for (k, y) in kept.iter().enumerate() {
                if !stay.contains(&k) {
                    out.insert(*y);
                }
            }
        }
        out
    }
}

/// Pairs of objects that are the only ones with their name (in their
/// component) on each side.
fn unique_pairs(a: &Tree, b: &Tree, name: impl Fn(&Node) -> Option<String>) -> Vec<(usize, usize)> {
    let index = |t: &Tree| {
        let mut seen: HashMap<(Option<String>, String), Option<usize>> = HashMap::new();
        for (i, n) in t.nodes.iter().enumerate().skip(1) {
            if let Some(name) = name(n) {
                seen.entry((n.component.clone(), name))
                    .and_modify(|e| *e = None)
                    .or_insert(Some(i));
            }
        }
        seen
    };
    let (x, y) = (index(a), index(b));
    let mut out: Vec<(usize, usize)> = x
        .iter()
        .filter_map(|(k, i)| Some(((*i)?, (*y.get(k)?)?)))
        .collect();
    out.sort_unstable();
    out
}

/// Pairs `xs` with `ys` in order: those `same` holds for along a longest
/// common run, then, between each two of those, the rest `kind` holds for,
/// in order, as far as both last.
fn align(
    xs: &[usize],
    ys: &[usize],
    same: impl Fn(usize, usize) -> bool,
    kind: impl Fn(usize, usize) -> bool,
) -> Vec<(usize, usize)> {
    let (n, m) = (xs.len(), ys.len());
    let mut len = vec![vec![0u32; m + 1]; n + 1];
    for p in (0..n).rev() {
        for q in (0..m).rev() {
            len[p][q] = if same(xs[p], ys[q]) {
                len[p + 1][q + 1] + 1
            } else {
                len[p + 1][q].max(len[p][q + 1])
            };
        }
    }
    let mut anchors = Vec::new();
    let (mut p, mut q) = (0, 0);
    while p < n && q < m {
        if same(xs[p], ys[q]) && len[p][q] == len[p + 1][q + 1] + 1 {
            anchors.push((p, q));
            p += 1;
            q += 1;
        } else if len[p + 1][q] >= len[p][q + 1] {
            p += 1;
        } else {
            q += 1;
        }
    }
    let mut out = Vec::new();
    let (mut p0, mut q0) = (0, 0);
    for &(p, q) in anchors.iter().chain([(n, m)].iter()) {
        let mut free: Vec<usize> = ys[q0..q].to_vec();
        for &x in &xs[p0..p] {
            if let Some(k) = free.iter().position(|&y| kind(x, y)) {
                out.push((x, free.remove(k)));
            }
        }
        if p < n && q < m {
            out.push((xs[p], ys[q]));
        }
        (p0, q0) = (p + 1, q + 1);
    }
    out
}

/// Indexes into `order` of a longest strictly rising run.
fn longest_rising(order: &[usize]) -> BTreeSet<usize> {
    let n = order.len();
    let mut best = vec![1usize; n];
    let mut prev = vec![None; n];
    for k in 0..n {
        for l in 0..k {
            if order[l] < order[k] && best[l] + 1 > best[k] {
                best[k] = best[l] + 1;
                prev[k] = Some(l);
            }
        }
    }
    let mut out = BTreeSet::new();
    let mut at = (0..n).max_by_key(|&k| (best[k], std::cmp::Reverse(k)));
    while let Some(k) = at {
        out.insert(k);
        at = prev[k];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(ty: &str, nth: u32) -> KeySeg {
        KeySeg::Typed { ty: ty.into(), nth }
    }

    fn path(text: &str) -> NodePath {
        text.parse().unwrap()
    }

    #[test]
    fn keys_are_the_compilers() {
        let all = keys(
            "Item {\n  Rectangle { }\n  Text { }\n  Rectangle { id: r; Text { objectName: \"t\" } }\n  Rectangle { }\n  states: [State { }]\n  transitions: Transition { }\n  component Row: Item { Text { } }\n}\n",
        )
        .unwrap();
        let got: Vec<(String, Option<String>, Vec<KeySeg>)> = all
            .into_iter()
            .map(|o| (o.path.to_string(), o.component, o.key))
            .collect();
        assert_eq!(
            got,
            vec![
                ("root".into(), None, vec![]),
                ("root/0".into(), None, vec![typed("Rectangle", 0)]),
                ("root/1".into(), None, vec![typed("Text", 0)]),
                ("root/2".into(), None, vec![KeySeg::Id("r".into())]),
                (
                    "root/2/0".into(),
                    None,
                    vec![KeySeg::Id("r".into()), KeySeg::Named("t".into())]
                ),
                ("root/3".into(), None, vec![typed("Rectangle", 1)]),
                (
                    "root/transitions:".into(),
                    None,
                    vec![typed("Transition", 0)]
                ),
                ("root/component Row".into(), Some("#Row".into()), vec![]),
                (
                    "root/component Row/0".into(),
                    Some("#Row".into()),
                    vec![typed("Text", 0)]
                ),
            ]
        );
    }

    #[test]
    fn a_template_is_keyed_from_its_root() {
        let all = keys(
            "Item {\n  ListView { id: list; delegate: Rectangle { Text { } } }\n  Repeater { Text { } }\n  Repeater { Text { } }\n}\n",
        )
        .unwrap();
        let got: Vec<(String, Option<String>, Vec<KeySeg>)> = all
            .into_iter()
            .map(|o| (o.path.to_string(), o.component, o.key))
            .collect();
        assert_eq!(
            got,
            vec![
                ("root".into(), None, vec![]),
                ("root/0".into(), None, vec![KeySeg::Id("list".into())]),
                (
                    "root/0/delegate:".into(),
                    Some("/list.delegate".into()),
                    vec![]
                ),
                (
                    "root/0/delegate:/0".into(),
                    Some("/list.delegate".into()),
                    vec![typed("Text", 0)]
                ),
                ("root/1".into(), None, vec![typed("Repeater", 0)]),
                (
                    "root/1/0".into(),
                    Some("/Repeater[0].default".into()),
                    vec![]
                ),
                ("root/2".into(), None, vec![typed("Repeater", 1)]),
                (
                    "root/2/0".into(),
                    Some("/Repeater[1].default".into()),
                    vec![]
                ),
            ]
        );
    }

    #[test]
    fn the_same_text_has_no_changes() {
        let text = "Item {\n  Rectangle { color: \"red\" }\n  Rectangle { }\n}\n";
        assert_eq!(diff(text, text).unwrap(), vec![]);
    }

    #[test]
    fn a_sibling_put_first_shifts_the_others_keys_and_is_added() {
        let old = "Column {\n  Text { text: \"a\" }\n  Text { text: \"b\" }\n}\n";
        let new = "Column {\n  Text { text: \"new\" }\n  Text { text: \"a\" }\n  Text { text: \"b\" }\n}\n";
        let changes = diff(old, new).unwrap();
        let added: Vec<_> = changes
            .iter()
            .filter_map(|c| match c {
                TreeChange::Added(o) => Some(o.path.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(added, ["root/0"]);
        let rekeyed: Vec<_> = changes
            .iter()
            .filter_map(|c| match c {
                TreeChange::Rekeyed { was, now } => Some((was.key.clone(), now.key.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            rekeyed,
            vec![
                (vec![typed("Text", 0)], vec![typed("Text", 1)]),
                (vec![typed("Text", 1)], vec![typed("Text", 2)]),
            ]
        );
        assert_eq!(changes.len(), 3, "{changes:#?}");
    }

    #[test]
    fn an_edited_sibling_is_changed_not_swapped() {
        let old =
            "Column {\n  Text { text: \"a\" }\n  Text { text: \"b\" }\n  Text { text: \"c\" }\n}\n";
        let new = "Column {\n  Text { text: \"b\" }\n  Text { text: \"C\" }\n}\n";
        let changes = diff(old, new).unwrap();
        assert_eq!(
            changes,
            vec![
                TreeChange::Removed(Object {
                    path: path("root/0"),
                    component: None,
                    key: vec![typed("Text", 0)],
                    ty: "Text".into()
                }),
                TreeChange::Rekeyed {
                    was: Object {
                        path: path("root/1"),
                        component: None,
                        key: vec![typed("Text", 1)],
                        ty: "Text".into()
                    },
                    now: Object {
                        path: path("root/0"),
                        component: None,
                        key: vec![typed("Text", 0)],
                        ty: "Text".into()
                    },
                },
                TreeChange::Rekeyed {
                    was: Object {
                        path: path("root/2"),
                        component: None,
                        key: vec![typed("Text", 2)],
                        ty: "Text".into()
                    },
                    now: Object {
                        path: path("root/1"),
                        component: None,
                        key: vec![typed("Text", 1)],
                        ty: "Text".into()
                    },
                },
                TreeChange::Changed {
                    was: Object {
                        path: path("root/2"),
                        component: None,
                        key: vec![typed("Text", 2)],
                        ty: "Text".into()
                    },
                    now: Object {
                        path: path("root/1"),
                        component: None,
                        key: vec![typed("Text", 1)],
                        ty: "Text".into()
                    },
                    name: "text".into(),
                    old: Some("\"c\"".into()),
                    new: Some("\"C\"".into()),
                },
            ]
        );
    }

    #[test]
    fn an_object_with_an_id_is_followed_to_its_new_parent() {
        let old = "Item {\n  Row { Text { id: t; text: \"a\" } }\n  Column { }\n}\n";
        let new = "Item {\n  Row { }\n  Column { Text { id: t; text: \"b\" } }\n}\n";
        let changes = diff(old, new).unwrap();
        assert!(matches!(
            &changes[0],
            TreeChange::Moved { was, now } if was.path == path("root/0/0") && now.path == path("root/1/0")
        ));
        assert!(matches!(
            &changes[1],
            TreeChange::Changed { name, new: Some(v), .. } if name == "text" && v == "\"b\""
        ));
        assert_eq!(changes.len(), 2, "{changes:#?}");
    }

    #[test]
    fn an_unnamed_object_is_followed_by_its_content() {
        let old = "Item {\n  Row { Text { text: \"only\" } }\n  Column { }\n}\n";
        let new = "Item {\n  Row { }\n  Column { Text { text: \"only\" } }\n}\n";
        let changes = diff(old, new).unwrap();
        assert!(
            matches!(&changes[..], [TreeChange::Moved { was, now }] if was.path == path("root/0/0") && now.path == path("root/1/0")),
            "{changes:#?}"
        );
    }

    #[test]
    fn swapped_siblings_are_a_move() {
        let old = "Row {\n  Text { text: \"a\" }\n  Rectangle { }\n}\n";
        let new = "Row {\n  Rectangle { }\n  Text { text: \"a\" }\n}\n";
        let changes = diff(old, new).unwrap();
        assert_eq!(changes.len(), 1, "{changes:#?}");
        assert!(matches!(&changes[0], TreeChange::Moved { .. }));
    }

    #[test]
    fn identical_siblings_are_told_apart_by_what_stands_between() {
        let old = "Column {\n  Item { }\n  Text { id: t }\n  Item { }\n}\n";
        let new = "Column {\n  Text { id: t }\n  Item { }\n}\n";
        let changes = diff(old, new).unwrap();
        assert!(
            matches!(
                &changes[..],
                [TreeChange::Removed(gone), TreeChange::Rekeyed { .. }]
                    if gone.path.to_string() == "root/0"
            ),
            "{changes:#?}"
        );
    }

    #[test]
    fn a_new_id_rekeys_and_changes_a_binding() {
        let old = "Item {\n  Rectangle { }\n}\n";
        let new = "Item {\n  Rectangle { id: box }\n}\n";
        let changes = diff(old, new).unwrap();
        assert!(matches!(
            &changes[..],
            [TreeChange::Rekeyed { now, .. }, TreeChange::Changed { name, .. }]
                if now.key == [KeySeg::Id("box".into())] && name == "id"
        ));
    }

    #[test]
    fn the_same_id_on_another_type_is_retyped() {
        let changes = diff(
            "Item {\n  Text { id: a }\n}\n",
            "Item {\n  Label { id: a }\n}\n",
        )
        .unwrap();
        assert!(
            matches!(&changes[..], [TreeChange::Retyped { was, now }] if was.ty == "Text" && now.ty == "Label")
        );
    }

    #[test]
    fn grouped_and_declared_values_are_bindings() {
        let old = "Item {\n  property int n: 1\n  font { bold: true }\n}\n";
        let new = "Item {\n  property int n: 2\n  font.bold: false\n}\n";
        let names: Vec<String> = diff(old, new)
            .unwrap()
            .into_iter()
            .filter_map(|c| match c {
                TreeChange::Changed { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["font.bold", "n"]);
    }
}
