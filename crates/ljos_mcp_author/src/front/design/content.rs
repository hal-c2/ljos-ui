//! Content: two controls for one setting (`docs/design/content.md`).

use oxc_ast_visit::{Visit, walk};
use qml_syntax::{
    GetSpan,
    ast::{QmlBindingValue, QmlObject},
    js::{AssignmentExpression, Expression, SimpleAssignmentTarget},
};

use super::{Found, Frame, bindings, bound, children, class};

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

/// Words that say nothing about what the button does.
const VAGUE: &[&str] = &[
    "ok",
    "yes",
    "no",
    "continue",
    "submit",
    "done",
    "go",
    "click here",
    "proceed",
];

pub(super) fn object<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let frame = chain.last().expect("the object itself");
    if found.is(frame, class::BUTTON) || found.is(frame, class::MENU_ITEM) {
        vague_action(found, frame);
    }
}

fn vague_action<'a>(found: &mut Found<'_, '_, '_, 'a>, frame: &Frame<'_, 'a>) {
    let Some(text) = bound(frame.object, &["text"]) else {
        return;
    };
    let Some(said) = text.literal_text() else {
        return;
    };
    let plain: String = said
        .chars()
        .filter(|c| *c != '&')
        .collect::<String>()
        .trim()
        .to_lowercase();
    if !VAGUE.contains(&plain.as_str()) {
        return;
    }
    found.report(
        text.at,
        "content-vague-action",
        format!(
            "`{said}` does not say what happens; name the action by its verb and what it acts on (`Send report`, `Save draft`), so the button reads right out of context"
        ),
    );
}

pub(super) fn file<'a>(found: &mut Found<'_, '_, '_, 'a>, root: &QmlObject<'a>) {
    let mut seen: Vec<(String, String)> = Vec::new();
    visit(found, root, false, &mut seen);
    let mut all = Vec::new();
    collect(root, &mut all);
    for member in &root.members {
        if let qml_syntax::ast::QmlMember::InlineComponent(inline) = member {
            collect(&inline.object, &mut all);
        }
    }
    for object in &all {
        menu_ellipsis(found, object, &all);
    }
}

fn collect<'o, 'a>(object: &'o QmlObject<'a>, out: &mut Vec<&'o QmlObject<'a>>) {
    out.push(object);
    for (child, _) in children(object) {
        collect(child, out);
    }
}

fn frame_of<'o, 'a>(found: &Found<'_, '_, '_, 'a>, object: &'o QmlObject<'a>) -> Frame<'o, 'a> {
    Frame {
        object,
        index: found.cx.tree.index_of(object),
        via: None,
    }
}

/// The ids written before `.open(` in a handler.
fn opened(text: &str) -> Vec<&str> {
    text.match_indices(".open(")
        .filter_map(|(at, _)| {
            let head = &text[..at];
            let start = head
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            let id = &head[start..];
            (!id.is_empty()).then_some(id)
        })
        .collect()
}

fn menu_ellipsis<'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    object: &QmlObject<'a>,
    all: &[&QmlObject<'a>],
) {
    let frame = frame_of(found, object);
    if !(found.is(&frame, class::MENU_ITEM) || found.is(&frame, class::ACTION)) {
        return;
    }
    let Some(text) = bound(object, &["text"]) else {
        return;
    };
    let Some(said) = text.literal_text() else {
        return;
    };
    let said = said.trim();
    if said.is_empty()
        || said.ends_with('…')
        || said.ends_with("...")
        || said.to_lowercase().replace('&', "").starts_with("about")
    {
        return;
    }
    let mut handlers = Vec::new();
    for (name, b) in bindings(object) {
        if name != "onTriggered" && name != "onClicked" {
            continue;
        }
        match b.value {
            QmlBindingValue::Expression(e) => handlers.push(found.text(e.span()).to_owned()),
            QmlBindingValue::Statement(s) => handlers.push(found.text(s.span()).to_owned()),
            _ => {}
        }
    }
    let ids: Vec<String> = handlers
        .iter()
        .flat_map(|h| opened(h))
        .map(str::to_owned)
        .collect();
    for id in ids {
        let target = all.iter().find(|o| {
            bound(o, &["id"])
                .and_then(|b| match b.expression() {
                    Some(Expression::Identifier(i)) => Some(i.name.to_string()),
                    _ => None,
                })
                .is_some_and(|n| n == id)
        });
        let Some(target) = target else { continue };
        let tf = frame_of(found, target);
        let dialog = (found.is(&tf, class::POPUP)
            && !found.is(&tf, class::MENU)
            && !found.is(&tf, class::TOOL_TIP))
            || found.name(&tf).ends_with("Dialog");
        if !dialog {
            continue;
        }
        // A dialog that only informs asks for nothing more.
        if let Some(buttons) = bound(target, &["standardButtons"]).and_then(|b| b.span()) {
            let words: Vec<&str> = found
                .text(buttons)
                .split(|c: char| !c.is_alphanumeric())
                .collect();
            let asks = words
                .iter()
                .any(|w| matches!(*w, "Cancel" | "Apply" | "Save" | "Yes" | "No" | "Discard"));
            if !asks && words.iter().any(|w| matches!(*w, "Ok" | "Close")) {
                continue;
            }
        }
        found.report(
            text.at,
            "content-menu-ellipsis",
            format!(
                "`{said}` opens a dialog that asks for more, so its text ends in `…` (`{said}…`): \
                 the ellipsis tells the user nothing happens until they answer"
            ),
        );
        return;
    }
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
