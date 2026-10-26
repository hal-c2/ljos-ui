//! Focus, keyboard and pointer: what only a mouse can do or see
//! (`docs/design/interaction.md`), and the icon button a sighted user cannot
//! read (`docs/design/accessibility.md`).

use qml_syntax::{
    GetSpan,
    ast::{QmlBindingValue, QmlObject},
    js::Expression,
};

use super::{Found, Frame, bindings, bound, children, class, number};

/// The handlers that make a pointer area do something.
const ACTIONS: &[&str] = &[
    "onClicked",
    "onDoubleClicked",
    "onPressed",
    "onReleased",
    "onPressAndHold",
    "onTapped",
    "onDoubleTapped",
    "onSingleTapped",
    "onLongPressed",
];

/// Names that show something for a reason other than the pointer.
const NOT_HOVER: &[&str] = &[
    "activeFocus",
    "visualFocus",
    "focus",
    "pressed",
    "down",
    "checked",
    "current",
    "isCurrentItem",
    "selected",
    "highlighted",
    "expanded",
    "opened",
    "editing",
    "running",
];

pub(super) fn object<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let frame = chain.last().expect("the object itself");
    if found.is(frame, class::MOUSE_AREA) || found.is(frame, class::TAP_HANDLER) {
        pointer_only(found, chain);
    }
    hover_only(found, chain);
    if found.is(frame, class::TEXT_INPUT) {
        placeholder_label(found, chain);
    }
    if found.is(frame, class::BUTTON)
        || found.is(frame, class::MENU_ITEM)
        || found.is(frame, class::ACTION)
    {
        destructive_undo(found, chain);
    }
    if found.is(frame, class::ABSTRACT_BUTTON)
        && !found.is(frame, class::TEXT_ROW_BUTTON)
        && chain.len() > 1
    {
        icon_no_tooltip(found, frame);
    }
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
}

fn pointer_only<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let [.., parent, frame] = chain else { return };
    if frame.via.is_some() {
        return;
    }
    // A control takes focus and keys itself; a view moves its current item
    // with the keys; a popup closes with Escape.
    let handled = |f: &Frame<'_, 'a>| {
        found.is(f, class::CONTROL) || found.is(f, class::VIEW) || found.is(f, class::POPUP)
    };
    if handled(parent) || chain.iter().any(|f| f.via.as_deref() == Some("delegate")) {
        return;
    }
    let mine = bindings(frame.object);
    if mine.iter().any(|(n, _)| n == "drag.target") {
        return;
    }
    if mine
        .iter()
        .find(|(n, _)| n == "acceptedButtons")
        .and_then(|(_, b)| b.span())
        .is_some_and(|s| !words(found.text(s)).any(|w| w == "LeftButton" || w == "AllButtons"))
    {
        return;
    }
    let Some((name, handler)) = mine.iter().find(|(n, _)| ACTIONS.contains(&n.as_str())) else {
        return;
    };
    let body = match handler.value {
        qml_syntax::ast::QmlBindingValue::Expression(e) => found.text(e.span()),
        qml_syntax::ast::QmlBindingValue::Statement(s) => found.text(s.span()),
        _ => return,
    };
    // Taking focus or closing something are a pointer's way to what the
    // keyboard already does.
    let calls: Vec<&str> = words(body).collect();
    let only_focus_or_close = calls
        .iter()
        .any(|w| matches!(*w, "forceActiveFocus" | "close" | "hide"))
        && calls.len() <= 3;
    if only_focus_or_close {
        return;
    }
    let keyboard = bindings(parent.object).iter().any(|(n, b)| {
        n.starts_with("Keys.")
            || n == "activeFocusOnTab" && b.boolean() != Some(false)
            || n == "Accessible.onPressAction"
            || n == "focus" && b.boolean() == Some(true)
    }) || mine
        .iter()
        .any(|(n, _)| n.starts_with("Keys.") || n == "Accessible.onPressAction")
        || children(parent.object)
            .into_iter()
            .any(|(c, _)| c.type_name.parts.last().is_some_and(|p| *p == "Shortcut"));
    if keyboard {
        return;
    }
    found.report(
        handler.at,
        "focus-pointer-only",
        format!(
            "`{name}` on a `{}` in a plain `{}` is the only way to do this, and a keyboard \
             cannot reach it; use a control (`Button`, `ItemDelegate`), or give the item \
             `activeFocusOnTab`, a `Keys` handler and `Accessible.onPressAction`",
            found.name(frame),
            found.name(parent)
        ),
    );
}

/// Something a user would miss: something to read or to use.
fn matters<'a>(found: &Found<'_, '_, '_, 'a>, f: &Frame<'_, 'a>) -> bool {
    found.is(f, class::ABSTRACT_BUTTON)
        || found.is(f, class::MOUSE_AREA)
        || found.is(f, class::TAP_HANDLER)
        || found.is(f, class::TEXT)
        || found.is(f, class::IMAGE)
        || found.is(f, class::TEXT_INPUT)
}

/// Whether `object` holds something that [`matters`].
fn contains<'a>(found: &Found<'_, '_, '_, 'a>, object: &QmlObject<'a>) -> bool {
    children(object).into_iter().any(|(c, via)| {
        via.is_none() && {
            let f = Frame {
                object: c,
                index: found.cx.tree.index_of(c),
                via: None,
            };
            matters(found, &f) || contains(found, c)
        }
    })
}

fn hover_only<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let frame = chain.last().expect("the object itself");
    if chain.iter().any(|f| found.is(f, class::TOOL_TIP)) {
        return;
    }
    for name in ["visible", "opacity"] {
        let Some(b) = bound(frame.object, &[name]) else {
            continue;
        };
        let Some(span) = b.span() else { continue };
        let text = found.text(span);
        let hover = words(text).any(|w| w == "hovered" || w == "containsMouse");
        if !hover || words(text).any(|w| NOT_HOVER.contains(&w)) || text.contains('!') {
            continue;
        }
        // An opacity that dims rather than hides still shows it.
        if name == "opacity" && !hides(b.expression()) {
            continue;
        }
        if name == "visible" && text.contains('?') {
            continue;
        }
        if !(matters(found, frame) || contains(found, frame.object)) {
            continue;
        }
        found.report(
            b.at,
            "focus-hover-only",
            format!(
                "`{name}` follows the pointer alone, so touch and keyboard users never see this \
                 `{}`; show it on `hovered || activeFocus` (or `visualFocus`) and give its \
                 action another way in",
                found.name(frame)
            ),
        );
    }
}

/// `hovered ? 1 : 0`: an opacity that hides, not one that dims.
fn hides(e: Option<&Expression<'_>>) -> bool {
    match e.map(|e| e.without_parentheses()) {
        Some(Expression::ConditionalExpression(c)) => [&c.consequent, &c.alternate]
            .iter()
            .any(|e| number(e) == Some(0.0)),
        _ => false,
    }
}

fn icon_no_tooltip<'a>(found: &mut Found<'_, '_, '_, 'a>, frame: &Frame<'_, 'a>) {
    let all = bindings(frame.object);
    let has = |name: &str| all.iter().any(|(n, _)| n == name);
    let icon = all
        .iter()
        .find(|(n, _)| n == "icon.source" || n == "icon.name")
        .map(|(_, b)| b.at);
    let Some(at) = icon else { return };
    let icon_only = all
        .iter()
        .find(|(n, _)| n == "display")
        .and_then(|(_, b)| b.enumeration())
        .is_some_and(|(_, v)| v == "IconOnly");
    let no_text = match all.iter().find(|(n, _)| n == "text") {
        None => !has("action"),
        Some((_, b)) => b.string() == Some(""),
    };
    if !(icon_only || no_text) {
        return;
    }
    let tooltip = has("ToolTip.text")
        || children(frame.object).into_iter().any(|(c, _)| {
            found.is(
                &Frame {
                    object: c,
                    index: found.cx.tree.index_of(c),
                    via: None,
                },
                class::TOOL_TIP,
            )
        });
    if tooltip {
        return;
    }
    found.report(
        at,
        "a11y-icon-button-no-tooltip",
        format!(
            "this `{}` shows only an icon and has no tooltip, so a sighted user cannot learn \
             what it does; set `ToolTip.text` (and `ToolTip.visible: hovered`) to the words of \
             its name",
            found.name(frame)
        ),
    );
}

/// Whether a `Text` (a `Label`) sits beside the field, or beside what holds
/// it when it is the first or last of that: the field is named by it.
fn labelled<'a>(found: &Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) -> bool {
    for level in (1..chain.len()).rev() {
        let (parent, me) = (&chain[level - 1], &chain[level]);
        let kids: Vec<&QmlObject<'a>> = children(parent.object)
            .into_iter()
            .filter(|(_, via)| via.is_none())
            .map(|(c, _)| c)
            .collect();
        // A field some property holds is placed by that property.
        let Some(at) = kids.iter().position(|c| std::ptr::eq(*c, me.object)) else {
            return true;
        };
        let text = |i: usize| {
            kids.get(i).is_some_and(|c| {
                found.is(
                    &Frame {
                        object: c,
                        index: found.cx.tree.index_of(c),
                        via: None,
                    },
                    class::TEXT,
                )
            })
        };
        if at.checked_sub(1).is_some_and(text) || text(at + 1) {
            return true;
        }
        if at != 0 && at + 1 != kids.len() {
            break;
        }
    }
    false
}

fn placeholder_label<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let frame = chain.last().expect("the object itself");
    let all = bindings(frame.object);
    let Some((_, placeholder)) = all.iter().find(|(n, _)| n == "placeholderText") else {
        return;
    };
    if all
        .iter()
        .any(|(n, _)| n == "Accessible.name" || n == "Accessible.labelledBy")
    {
        return;
    }
    // A field alone in its file is named where it is used.
    if chain.len() < 2 || labelled(found, chain) {
        return;
    }
    found.report(
        placeholder.at,
        "a11y-placeholder-label",
        format!(
            "this `{}` has only its placeholder to say what it is for, which disappears as soon \
             as the user types and which a screen reader may not read; set `Accessible.name`, \
             or put a `Label` beside it",
            found.name(frame)
        ),
    );
}

/// What a destructive action's text starts with.
const DESTRUCTIVE: &[&str] = &["delete", "remove", "discard", "erase", "clear"];

/// Words after the verb that name something that comes back by itself.
const REVERSIBLE: &[&str] = &[
    "filter",
    "filters",
    "search",
    "selection",
    "query",
    "color",
    "colour",
    "field",
];

fn destructive_undo<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let frame = chain.last().expect("the object itself");
    let Some(text) = bound(frame.object, &["text"]) else {
        return;
    };
    let Some(said) = text.literal_text() else {
        return;
    };
    let lower = said.replace('&', "").to_lowercase();
    let mut said_words = words(&lower);
    if !said_words.next().is_some_and(|w| DESTRUCTIVE.contains(&w))
        || said_words.any(|w| REVERSIBLE.contains(&w))
    {
        return;
    }
    // The confirmation itself is where the deleting happens.
    if chain.iter().any(|f| {
        found.is(f, class::POPUP) && !found.is(f, class::MENU) && !found.is(f, class::TOOL_TIP)
    }) {
        return;
    }
    let mut handler = None;
    for (name, b) in bindings(frame.object) {
        if !(name.starts_with("on") && name.ends_with("ed") || name.ends_with("Triggered")) {
            continue;
        }
        let body = match b.value {
            QmlBindingValue::Expression(e) => found.text(e.span()),
            QmlBindingValue::Statement(s) => found.text(s.span()),
            _ => continue,
        };
        handler =
            Some(handler.map_or_else(|| body.to_lowercase(), |h: String| h + &body.to_lowercase()));
    }
    // With no handler the action is somebody else's, an `action:` or a
    // `Connections`.
    let Some(handler) = handler else { return };
    if [".open(", ".popup(", "confirm", "undo", "dialog"]
        .iter()
        .any(|k| handler.contains(k))
    {
        return;
    }
    found.report(
        text.at,
        "focus-destructive-undo",
        format!(
            "`{said}` destroys something and does it at once, with no way back; ask first in a \
             dialog, or do it and offer `Undo` (a toast or `Ctrl+Z`) so a slip costs nothing"
        ),
    );
}
