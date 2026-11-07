//! A `MouseArea` that fills its parent lowered to gestures on the parent
//! (docs/DIALECT.md §4.7, §4.14).
//!
//! An area that fills its parent and takes clicks is, on Qt as here, a
//! `TapHandler` on the parent that taps when the pointer is let go inside,
//! however it moved and however long it was held: `Tap { gesturePolicy:
//! Tap.ReleaseWithinBounds; longPressThreshold: 0 }`. It takes the press as
//! the area does, so what is under the parent gets none. The cursor it asks
//! for is a `Hover`'s, written after the `Tap` on its closing line.
//!
//! The area is over its siblings declared before it, and a gesture on the
//! parent under them; it stays a `PointerArea` where one of them could take
//! the pointer, where something beside it is stacked by `z`, where a
//! handler beside it would share the parent with the `Tap`, and where its
//! parent is not an item that only draws. It stays one, too, where it does
//! what a `Tap` does not, or not as a `Tap` does it: a press or a release
//! of its own, hovering, dragging (Qt's area moves its target the drag
//! threshold short of the pointer, a `Drag` all the way), the `mouse` a
//! handler is given, a name it or a `Tap` has read unqualified, or its id
//! read anywhere.

use std::collections::HashSet;
use std::ops::Range;

use oxc_ast::ast::{Expression, IdentifierReference};
use oxc_ast_visit::Visit;
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject};
use qt_types::{Module, Type};

/// What the area's `anchors.fill: parent` becomes: a tap is let go inside,
/// and no hold is too long for one, as a click.
const TAP: &str = "gesturePolicy: Tap.ReleaseWithinBounds; longPressThreshold: 0";

/// The parents a gesture is put on: items that draw and take no pointer.
const PARENTS: &[&str] = &["Item", "Rectangle", "Image"];

/// What an area that fills its parent may say and still be a `Tap`.
const MEMBERS: &[&str] = &[
    "id",
    "objectName",
    "anchors.fill",
    "onClicked",
    "enabled",
    "acceptedButtons",
    "cursorShape",
];

/// QtQuick's types that take the pointer themselves, beside those whose
/// name ends in `Handler`.
const INTERACTIVE: &[&str] = &[
    "MouseArea",
    "MultiPointTouchArea",
    "PinchArea",
    "Flickable",
    "ListView",
    "GridView",
    "PathView",
    "TableView",
    "TreeView",
    "TextInput",
    "TextEdit",
    "Loader",
];

/// The edits that make an area gestures.
pub(crate) struct Plan {
    /// Text replaced as given; what the pass makes inside these ranges is
    /// dropped.
    pub(crate) replace: Vec<(Range<usize>, String)>,
    /// The bindings the `Hover` copies, as the pass lowers them.
    pub(crate) hover: Vec<Range<usize>>,
    /// The end of the area: where the `Hover` goes.
    pub(crate) end: usize,
}

/// What the plan needs of the file.
pub(crate) struct File<'f> {
    pub(crate) source: &'f str,
    pub(crate) quick: &'static Module,
    pub(crate) ui: &'static Module,
    /// The type a name of an object's type is in the file, where it is
    /// Qt's.
    pub(crate) qt_type: &'f dyn Fn(&str) -> Option<&'static Type>,
    /// Whether a name is an id of the file.
    pub(crate) is_id: &'f dyn Fn(&str) -> bool,
    /// Every name a script of the file reads unqualified, or a binding's
    /// name starts with.
    pub(crate) read: &'f HashSet<String>,
}

/// The gestures the `index`th member of `parent`, of type `parent_ty`, is,
/// or `None` where it stays a `PointerArea`.
pub(crate) fn plan(
    file: &File<'_>,
    parent: &QmlObject<'_>,
    parent_ty: &'static Type,
    index: usize,
) -> Option<Plan> {
    let QmlMember::Object(area) = &parent.members[index] else {
        return None;
    };
    if area.type_name.as_simple() != Some("MouseArea") || area.on.is_some() {
        return None;
    }
    if !PARENTS.contains(&parent_ty.name) || binds(parent, "containmentMask").is_some() {
        return None;
    }
    for (at, member) in parent.members.iter().enumerate() {
        let QmlMember::Object(sibling) = member else {
            continue;
        };
        if at == index || !is_type(sibling) {
            continue;
        }
        let name = sibling.type_name.to_string();
        if binds(sibling, "z").is_some() || name.ends_with("Handler") {
            return None;
        }
        if at < index && takes_pointer(file, sibling) {
            return None;
        }
    }

    let mut bindings = Vec::new();
    for member in &area.members {
        let QmlMember::Binding(binding) = member else {
            return None;
        };
        let name = binding.name.to_string();
        if !MEMBERS.contains(&name.as_str()) {
            return None;
        }
        bindings.push((name, binding));
    }
    let find = |name: &str| bindings.iter().find(|(n, _)| n == name).map(|(_, b)| *b);
    let fill = find("anchors.fill")?;
    if !matches!(&fill.value, QmlBindingValue::Expression(Expression::Identifier(id)) if id.name == "parent")
    {
        return None;
    }
    if let Some(id) = find("id")
        && let QmlBindingValue::Expression(Expression::Identifier(id)) = &id.value
        && file.read.contains(id.name.as_str())
    {
        return None;
    }
    if let Some(clicked) = find("onClicked")
        && matches!(
            &clicked.value,
            QmlBindingValue::Expression(
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            )
        )
    {
        return None;
    }
    let cursor = find("cursorShape");

    // A name read unqualified is the area's in Qt, and the gesture's once
    // lowered; one neither has is found further out in both.
    let mouse_area = file.quick.type_named("MouseArea")?;
    let tap = file.ui.type_named("Tap")?;
    let hover = file.ui.type_named("Hover")?;
    let mut names = Names::default();
    for (name, binding) in &bindings {
        if name != "id" && name != "anchors.fill" {
            names.value(&binding.value);
        }
    }
    for name in &names.0 {
        let name = name.as_str();
        if name == "mouse" {
            return None;
        }
        if name == "parent" || (file.is_id)(name) {
            continue;
        }
        if has(mouse_area, name) || has(tap, name) || (cursor.is_some() && has(hover, name)) {
            return None;
        }
    }

    let source = file.source;
    let span = |b: &QmlBinding<'_>| b.span.start as usize..b.span.end as usize;
    let name_span = |b: &QmlBinding<'_>| b.name.span.start as usize..b.name.span.end as usize;
    let mut replace = vec![
        (
            area.type_name.span.start as usize..area.type_name.span.end as usize,
            "Tap".to_string(),
        ),
        (without_semicolon(source, span(fill)), TAP.to_string()),
    ];
    if let Some(clicked) = find("onClicked") {
        replace.push((name_span(clicked), "onTapped".into()));
    }
    if let Some(buttons) = find("acceptedButtons") {
        replace.push((name_span(buttons), "buttons".into()));
    }
    let mut copies = Vec::new();
    if let Some(cursor) = cursor {
        // The Hover is disabled as the area is.
        for binding in find("enabled").into_iter().chain([cursor]) {
            let range = without_semicolon(source, span(binding));
            if source[range.clone()].contains('\n') {
                return None;
            }
            copies.push(range);
        }
        replace.push((removal(source, span(cursor)), String::new()));
    }
    Some(Plan {
        replace,
        hover: copies,
        end: area.span.end as usize,
    })
}

/// An object made of a type, not a group of properties (`font { }`).
fn is_type(object: &QmlObject<'_>) -> bool {
    object
        .type_name
        .last()
        .starts_with(|c: char| c.is_ascii_uppercase())
}

fn binds<'o, 'a>(object: &'o QmlObject<'a>, name: &str) -> Option<&'o QmlBinding<'a>> {
    object.members.iter().find_map(|member| match member {
        QmlMember::Binding(binding) if binding.name.to_string() == name => Some(binding),
        _ => None,
    })
}

/// Whether `object`, or anything in it, may take the pointer: a type that
/// does, one of the project's (which may hold anything), or a `Text` with
/// links.
fn takes_pointer(file: &File<'_>, object: &QmlObject<'_>) -> bool {
    if !is_type(object) {
        return false;
    }
    let Some(name) = object.type_name.as_simple() else {
        return true;
    };
    if (file.qt_type)(name).is_none()
        || INTERACTIVE.contains(&name)
        || name.ends_with("Handler")
        || binds(object, "onLinkActivated").is_some()
    {
        return true;
    }
    object.members.iter().any(|member| match member {
        QmlMember::Object(child) => takes_pointer(file, child),
        QmlMember::Binding(binding) => match &binding.value {
            QmlBindingValue::Object(child) => takes_pointer(file, child),
            QmlBindingValue::Objects(children) => children.iter().any(|c| takes_pointer(file, c)),
            _ => false,
        },
        QmlMember::InlineComponent(_) => false,
        _ => false,
    })
}

fn has(ty: &'static Type, name: &str) -> bool {
    ty.property(name).is_some() || ty.signal(name).is_some() || ty.has_method(name)
}

/// A binding's range without the `;` that may end it.
fn without_semicolon(source: &str, range: Range<usize>) -> Range<usize> {
    let text = &source[range.clone()];
    let kept = text.trim_end_matches(';').trim_end().len();
    range.start..range.start + kept
}

/// What to take out of the text to take a binding out: the `;` after it,
/// and, where it is alone on its line, the line's room, leaving the line.
fn removal(source: &str, range: Range<usize>) -> Range<usize> {
    let bytes = source.as_bytes();
    let mut end = without_semicolon(source, range.clone()).end;
    let mut at = end;
    while at < bytes.len() && matches!(bytes[at], b' ' | b'\t') {
        at += 1;
    }
    if at < bytes.len() && bytes[at] == b';' {
        at += 1;
        while at < bytes.len() && matches!(bytes[at], b' ' | b'\t') {
            at += 1;
        }
        end = at;
    }
    let line = source[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let alone = source[line..range.start].trim().is_empty()
        && (end >= bytes.len() || matches!(bytes[end], b'\n' | b'\r'));
    (if alone { line } else { range.start })..end
}

/// The names the scripts read unqualified.
#[derive(Default)]
pub(crate) struct Names(pub(crate) HashSet<String>);

impl Names {
    pub(crate) fn value(&mut self, value: &QmlBindingValue<'_>) {
        match value {
            QmlBindingValue::Expression(expression) => self.visit_expression(expression),
            QmlBindingValue::Statement(statement) => self.visit_statement(statement),
            QmlBindingValue::Object(object) => self.object(object),
            QmlBindingValue::Objects(objects) => objects.iter().for_each(|o| self.object(o)),
        }
    }

    /// Every name the scripts of `object` read unqualified, and the first
    /// part of every dotted binding name (`area.enabled:` in a
    /// `PropertyChanges`).
    pub(crate) fn object(&mut self, object: &QmlObject<'_>) {
        for member in &object.members {
            match member {
                QmlMember::Binding(binding) => {
                    if binding.name.parts.len() > 1 {
                        self.0.insert(binding.name.parts[0].to_string());
                    }
                    if binding.name.as_simple() != Some("id") {
                        self.value(&binding.value);
                    }
                }
                QmlMember::Property(property) => {
                    if let Some(value) = &property.value {
                        self.value(value);
                    }
                }
                QmlMember::Function(function) => {
                    self.visit_function(function, ScopeFlags::Function);
                }
                QmlMember::Object(child) => self.object(child),
                QmlMember::InlineComponent(component) => self.object(&component.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }
}

impl<'a> Visit<'a> for Names {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.0.insert(it.name.to_string());
    }
}

/// Whether `inner` lies within `outer`.
pub(crate) fn within(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// `range` of `source` with the `changes` that lie in it made.
pub(crate) fn lowered_text(
    source: &str,
    range: &Range<usize>,
    changes: &[qml_edit::Change],
) -> String {
    let mut inside: Vec<qml_edit::Change> = changes
        .iter()
        .filter(|c| within(&c.range, range))
        .map(|c| qml_edit::Change {
            range: c.range.start - range.start..c.range.end - range.start,
            replacement: c.replacement.clone(),
        })
        .collect();
    inside.sort_by_key(|c| c.range.start);
    inside.dedup();
    qml_edit::splice(&source[range.clone()], &inside)
}
