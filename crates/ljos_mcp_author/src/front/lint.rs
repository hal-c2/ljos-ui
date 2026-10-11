//! The lint rules of `qml_lint`. A small set, each rule conservative: it names
//! a pattern that is wrong in practice and cannot be mistaken for something
//! else in one file. `i18n-qstr-non-literal` is found while the scripts are
//! read, in `check.rs`, and is documented with the others in the parent
//! module. The codes and how serious each is are in `crate::lints`, where
//! `ljos check` reads them too.

use std::path::Path;

use qml_front::{
    QtType, QtTypeOracle,
    scope::Tree,
    types::{Kind, Member, Types},
};
use qml_syntax::ast::{QmlBinding, QmlMember, QmlObject};

use super::{
    check::Shape,
    diagnostic,
    facts::{child_objects, has_binding, is_accessibly_described, string_literal},
};
use crate::{Diagnostic, lints};

pub(super) struct Cx<'c, 'a> {
    pub tree: &'c Tree<'a, QtType>,
    pub types: Types<'c, QtTypeOracle>,
    pub shapes: &'c [Shape],
    pub path: &'c Path,
    pub source: &'c str,
}

pub(super) fn run<'a>(cx: &Cx<'_, 'a>, root: &QmlObject<'a>, out: &mut Vec<Diagnostic>) {
    visit(cx, root, None, out);
}

fn visit<'a>(
    cx: &Cx<'_, 'a>,
    object: &QmlObject<'a>,
    parent: Option<&QmlObject<'a>>,
    out: &mut Vec<Diagnostic>,
) {
    if let Some(index) = cx.tree.index_of(object) {
        accessibility(cx, object, parent, index, out);
        strings(cx, object, index, out);
    }
    for child in child_objects(object) {
        visit(cx, child, Some(object), out);
    }
    for member in &object.members {
        if let QmlMember::InlineComponent(inline) = member {
            visit(cx, &inline.object, None, out);
        }
    }
}

/// The builtin type of an object written as one, not as a component built
/// on one: a component may well set what a rule looks for itself.
fn builtin(cx: &Cx<'_, '_>, index: usize) -> Option<QtType> {
    match cx.tree.objects[index].kind {
        Some(Kind::Builtin(ty)) => Some(ty),
        _ => None,
    }
}

/// The types a rule is about, each by its class in Qt's table and in
/// Ljós's own (`qt_types::Type::is_a`).
const ABSTRACT_BUTTON: &[&str] = &["QQuickAbstractButton", "QtQuick.Templates.AbstractButton"];
const MOUSE_AREA: &[&str] = &["QQuickMouseArea", "QtQuick.MouseArea"];
const TAP_HANDLER: &[&str] = &["QQuickTapHandler", "QtQuick.TapHandler"];
const IMAGE: &[&str] = &["QQuickImage", "QtQuick.Image"];

fn inherits(ty: QtType, classes: &[&str]) -> bool {
    ty.0.is_a(classes)
}

fn parent_is_button(cx: &Cx<'_, '_>, parent: Option<&QmlObject<'_>>) -> bool {
    parent
        .and_then(|p| cx.tree.index_of(p))
        .and_then(|i| match &cx.tree.objects[i].kind {
            Some(kind) => cx.types.base(kind),
            None => None,
        })
        .is_some_and(|ty| inherits(ty, ABSTRACT_BUTTON))
}

const CLICK_HANDLERS: &[&str] = &[
    "onClicked",
    "onDoubleClicked",
    "onPressed",
    "onReleased",
    "onPressAndHold",
];
const TAP_HANDLERS: &[&str] = &[
    "onTapped",
    "onSingleTapped",
    "onDoubleTapped",
    "onLongPressed",
];

/// `a11y-unnamed-interactive`, `a11y-unnamed-icon-button`, `a11y-unnamed-image`.
fn accessibility<'a>(
    cx: &Cx<'_, 'a>,
    object: &QmlObject<'a>,
    parent: Option<&QmlObject<'a>>,
    index: usize,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ty) = builtin(cx, index) else { return };
    let at = object.type_name.span.start;
    let name = &cx.tree.objects[index].name;
    let parent_described = parent.is_some_and(is_accessibly_described);

    if inherits(ty, MOUSE_AREA)
        && CLICK_HANDLERS.iter().any(|h| has_binding(object, &[h]))
        && !is_accessibly_described(object)
        && !parent_described
        && !parent_is_button(cx, parent)
    {
        out.push(diagnostic(
            cx.path,
            cx.source,
            at,
            lints::UNNAMED_INTERACTIVE.severity,
            lints::UNNAMED_INTERACTIVE.code,
            format!(
                "`{name}` reacts to clicks but neither it nor its parent has `Accessible.name`, \
                 so assistive technology cannot tell what it does; set `Accessible.role` and \
                 `Accessible.name`, or `Accessible.ignored: true` if it is only decoration"
            ),
        ));
    }
    // A handler is not an item and has no `Accessible`: the item it is in
    // is what a screen reader meets.
    if inherits(ty, TAP_HANDLER)
        && TAP_HANDLERS.iter().any(|h| has_binding(object, &[h]))
        && parent.is_some_and(|p| !is_accessibly_described(p))
        && !parent_is_button(cx, parent)
    {
        out.push(diagnostic(
            cx.path,
            cx.source,
            at,
            lints::UNNAMED_INTERACTIVE.severity,
            lints::UNNAMED_INTERACTIVE.code,
            format!(
                "`{name}` reacts to taps but the item it is in has no `Accessible.name`, so \
                 assistive technology cannot tell what it does; set `Accessible.role` and \
                 `Accessible.name` on that item"
            ),
        ));
    }

    if inherits(ty, ABSTRACT_BUTTON)
        && (has_binding(object, &["icon", "source"]) || has_binding(object, &["icon", "name"]))
        && !has_binding(object, &["text"])
        && !has_binding(object, &["action"])
        && !has_binding(object, &["contentItem"])
        && !is_accessibly_described(object)
    {
        out.push(diagnostic(
            cx.path,
            cx.source,
            at,
            lints::UNNAMED_ICON_BUTTON.severity,
            lints::UNNAMED_ICON_BUTTON.code,
            format!(
                "`{name}` shows only an icon and has no `text`, `action` or `Accessible.name`, \
                 so assistive technology has nothing to say for it; set `Accessible.name` \
                 (and a `ToolTip` if sighted users need one)"
            ),
        ));
    }

    if inherits(ty, IMAGE)
        && !is_accessibly_described(object)
        && !parent_described
        && !parent_is_button(cx, parent)
    {
        out.push(diagnostic(
            cx.path,
            cx.source,
            at,
            lints::UNNAMED_IMAGE.severity,
            lints::UNNAMED_IMAGE.code,
            format!(
                "`{name}` has neither `Accessible.name` nor `Accessible.ignored`; describe it, \
                 or set `Accessible.ignored: true` if it is only decoration"
            ),
        ));
    }
}

/// Types whose `text` is what the user types, not a label.
const EDITABLE: &[&str] = &[
    "QQuickTextInput",
    "QQuickTextEdit",
    "QQuickTextField",
    "QQuickTextArea",
    "QtQuick.TextInput",
    "QtQuick.TextEdit",
    "QtQuick.Templates.TextField",
    "QtQuick.Templates.TextArea",
];

/// `i18n-untranslated-string`.
fn strings<'a>(cx: &Cx<'_, 'a>, object: &QmlObject<'a>, index: usize, out: &mut Vec<Diagnostic>) {
    // Only where every member of the type is known: a `ListElement` has
    // data in its `text`, not a label.
    let Shape::Complete(base) = cx.shapes[index] else {
        return;
    };
    let editable = inherits(base, EDITABLE);
    for member in &object.members {
        let QmlMember::Binding(QmlBinding { name, value, .. }) = member else {
            continue;
        };
        let Some(text) = string_literal(value) else {
            continue;
        };
        if !text.chars().any(char::is_alphabetic) {
            continue;
        }
        let parts: Vec<&str> = name.parts.iter().copied().collect();
        let visible = match parts.as_slice() {
            [property @ ("text" | "title" | "placeholderText")] => {
                (*property == "placeholderText" || !editable)
                    && matches!(
                        cx.tree.member(cx.types, index, property),
                        Some(Member::Property(p)) if p.is_text
                    )
            }
            ["ToolTip", "text"] | ["Accessible", "name" | "description"] => true,
            _ => false,
        };
        if visible {
            out.push(diagnostic(
                cx.path,
                cx.source,
                name.span.start,
                lints::UNTRANSLATED_STRING.severity,
                lints::UNTRANSLATED_STRING.code,
                format!(
                    "the text \"{}\" is shown to the user but is not wrapped in `qsTr`, so it \
                     cannot be translated",
                    text.chars().take(40).collect::<String>()
                ),
            ));
        }
    }
}
