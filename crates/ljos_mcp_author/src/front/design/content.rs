//! Content: two controls for one setting (`docs/design/content.md`).

use oxc_ast_visit::{Visit, walk};
use qml_syntax::{
    ast::{QmlBindingValue, QmlObject},
    js::{AssignmentExpression, Expression, SimpleAssignmentTarget},
};

use super::{Found, Frame, bindings, children, class};

/// The handlers in which a control hands its value on.
const VALUE_HANDLERS: &[&str] = &[
    "onToggled",
    "onCheckedChanged",
    "onCheckStateChanged",
    "onMoved",
    "onValueChanged",
    "onValueModified",
    "onActivated",
    "onCurrentIndexChanged",
    "onTextEdited",
    "onTextChanged",
    "onEditingFinished",
    "onAccepted",
];

/// What a control's value is called.
const VALUES: &[&str] = &[
    "checked",
    "checkState",
    "value",
    "position",
    "currentIndex",
    "currentText",
    "currentValue",
    "text",
];

/// Radio buttons each set the one property to their own choice: that is
/// one control in parts.
const EXCLUSIVE: &[&str] = &[
    "QQuickRadioButton",
    "QQuickRadioDelegate",
    "QtQuick.Templates.RadioButton",
    "QtQuick.Templates.RadioDelegate",
];

/// `a.b = <the control's value>`: the member path and where it is.
#[derive(Default)]
struct Edits {
    found: Vec<(String, u32)>,
}

fn path(e: &Expression<'_>) -> Option<String> {
    match e.without_parentheses() {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::ThisExpression(_) => Some("this".into()),
        Expression::StaticMemberExpression(m) => {
            Some(format!("{}.{}", path(&m.object)?, m.property.name))
        }
        _ => None,
    }
}

fn reads_value(e: &Expression<'_>) -> bool {
    match e.without_parentheses() {
        Expression::Identifier(id) => VALUES.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(m) => VALUES.contains(&m.property.name.as_str()),
        _ => false,
    }
}

impl<'a> Visit<'a> for Edits {
    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if reads_value(&it.right)
            && let Some(SimpleAssignmentTarget::StaticMemberExpression(m)) =
                it.left.as_simple_assignment_target()
            && let Some(object) = path(&m.object)
        {
            self.found.push((
                format!("{object}.{}", m.property.name),
                m.property.span.start,
            ));
        }
        walk::walk_assignment_expression(self, it);
    }
}

pub(super) fn file<'a>(found: &mut Found<'_, '_, '_, 'a>, root: &QmlObject<'a>) {
    let mut seen: Vec<(String, String)> = Vec::new();
    visit(found, root, false, &mut seen);
}

fn visit<'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    object: &QmlObject<'a>,
    delegate: bool,
    seen: &mut Vec<(String, String)>,
) {
    let frame = Frame {
        object,
        index: found.cx.tree.index_of(object),
        via: None,
    };
    // One control written in a delegate is many on screen, each for its row.
    if !delegate
        && (found.is(&frame, class::CONTROL) || found.is(&frame, class::TEXT_INPUT))
        && !found.is(&frame, EXCLUSIVE)
    {
        let mut edits = Edits::default();
        for (name, b) in bindings(object) {
            if !VALUE_HANDLERS.contains(&name.as_str()) {
                continue;
            }
            match b.value {
                QmlBindingValue::Expression(e) => edits.visit_expression(e),
                QmlBindingValue::Statement(s) => edits.visit_statement(s),
                _ => {}
            }
        }
        let me = found.name(&frame);
        let mut mine: Vec<String> = Vec::new();
        for (target, at) in edits.found {
            if mine.contains(&target) {
                continue;
            }
            if let Some((_, other)) = seen.iter().find(|(t, _)| *t == target) {
                found.report(
                    at,
                    "content-duplicate-control",
                    format!(
                        "this `{me}` sets `{target}`, which a `{other}` in the same file sets \
                         too: two controls for one setting disagree as soon as one is used; keep \
                         one, and show the value where the other was"
                    ),
                );
            }
            mine.push(target);
        }
        seen.extend(mine.into_iter().map(|t| (t, me.clone())));
    }
    for (child, via) in children(object) {
        let delegate =
            delegate || via.as_deref() == Some("delegate") || found.is(&frame, class::REPEATER);
        visit(found, child, delegate, seen);
    }
}
