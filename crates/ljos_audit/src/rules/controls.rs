//! What the checks of names, layout and content read of the controls: what
//! a button shows, what explains it, and the words of a label.

use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value, WriteKind};
use ljos_quick::Text;

use ljos_controls::{AbstractButton, ToolTip, ToolTipAttached, button_display};

use crate::cx::is_a;

/// What a button shows of itself.
pub(crate) struct ButtonLook {
    /// Its text with the mnemonic marker taken out.
    pub text: String,
    /// It has an icon, by theme name or by source.
    pub icon: bool,
    /// Its text is shown (`display` is not `IconOnly`, or there is no icon).
    pub text_shown: bool,
}

pub(crate) fn button_look(engine: &Engine, item: ObjectId) -> Option<ButtonLook> {
    if !is_a::<AbstractButton>(engine, item) {
        return None;
    }
    engine
        .untrack(|| {
            engine.with::<AbstractButton, _>(item, |b| {
                let icon = !b.icon.name.as_str().is_empty() || !b.icon.source.0.as_str().is_empty();
                let text = without_mnemonic(b.text.as_str());
                let text_shown =
                    !text.trim().is_empty() && (b.display != button_display::ICON_ONLY || !icon);
                ButtonLook {
                    text,
                    icon,
                    text_shown,
                }
            })
        })
        .ok()
}

/// Whether a button is checked.
pub(crate) fn checked(engine: &Engine, b: ObjectId) -> bool {
    engine
        .untrack(|| engine.read_by_name(b, "checked"))
        .to_boolean()
}

/// Checks or unchecks a button as the toolkit does on its own account: a
/// binding on `checked` stays, and is what it says once the audit is done.
/// Runs `onCheckedChanged`, and an exclusive group unchecks the others.
pub(crate) fn set_checked(engine: &Engine, b: ObjectId, on: bool) {
    if let Ok(p) = engine.property_index(b, "checked") {
        let _ = engine.write(b, p, Value::Bool(on), WriteKind::Host);
    }
}

/// A button's check as [`hold_checked`] kept it.
pub(crate) struct Held {
    button: ObjectId,
    checked: bool,
    /// `checkState` where the button has one: a tristate box partly
    /// checked is unchecked as far as `checked` says, and putting
    /// `checked` back alone would leave it unchecked.
    state: Option<Value>,
}

/// Whether each of `buttons` is checked, for [`restore_checked`]: flipping
/// one of an exclusive group changes another.
pub(crate) fn hold_checked(engine: &Engine, buttons: &[ObjectId]) -> Vec<Held> {
    buttons
        .iter()
        .map(|b| Held {
            button: *b,
            checked: checked(engine, *b),
            state: engine
                .property_index(*b, "checkState")
                .ok()
                .map(|p| engine.untrack(|| engine.read(*b, p))),
        })
        .collect()
}

/// Puts back what [`hold_checked`] kept: what was unchecked first, so that
/// checking a group's one choice last leaves no other checked, then each
/// `checkState`, as the toolkit writes it, so a binding on it stays.
pub(crate) fn restore_checked(engine: &Engine, held: &[Held]) {
    for want in [false, true] {
        for h in held {
            if h.checked == want && engine.is_alive(h.button) && checked(engine, h.button) != want {
                set_checked(engine, h.button, want);
            }
        }
    }
    for h in held {
        let Some(was) = &h.state else { continue };
        if !engine.is_alive(h.button) {
            continue;
        }
        if let Ok(p) = engine.property_index(h.button, "checkState")
            && engine.untrack(|| engine.read(h.button, p)) != *was
        {
            let _ = engine.write(h.button, p, was.clone(), WriteKind::Host);
        }
    }
}

/// `&Save` as it reads: `Save`; `&&` is an ampersand.
pub(crate) fn without_mnemonic(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                chars.next();
                out.push('&');
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// What explains the item on hover: its `ToolTip.text`, or the text of a
/// `ToolTip` it holds.
pub(crate) fn tool_tip(engine: &Engine, item: ObjectId) -> Option<String> {
    let attached = engine
        .existing_attached_object(item, &MetaRef::Static(ToolTip::static_meta()))
        .and_then(|att| {
            engine
                .untrack(|| engine.read(att, ToolTipAttached::TEXT.index))
                .to_js_string()
                .as_str()
                .trim()
                .to_string()
                .into()
        })
        .filter(|t: &String| !t.is_empty());
    attached.or_else(|| {
        engine
            .children(item)
            .into_iter()
            .filter(|c| is_a::<ToolTip>(engine, *c))
            .map(|c| {
                engine
                    .untrack(|| engine.read_by_name(c, "text"))
                    .to_js_string()
                    .as_str()
                    .trim()
                    .to_string()
            })
            .find(|t| !t.is_empty())
    })
}

/// Every object of the scene, shown or not, parents first: a closed menu
/// and a page not on top are there too.
pub(crate) fn objects(engine: &Engine, root: ObjectId) -> Vec<ObjectId> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if out.len() >= crate::cx::DEPTH * 16 || !engine.is_alive(o) || !seen.insert(o) {
            continue;
        }
        out.push(o);
        let mut next = engine.children(o);
        next.extend(engine.visual_children(o));
        stack.extend(next.into_iter().rev());
    }
    out
}

/// The words of a label, lower case, without what is between them.
pub(crate) fn word_list(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The text a `Text` shows.
pub(crate) fn text_of(engine: &Engine, item: ObjectId) -> Option<String> {
    is_a::<Text>(engine, item).then(|| {
        engine
            .untrack(|| engine.read(item, Text::TEXT.index))
            .to_js_string()
            .as_str()
            .to_string()
    })
}

/// Every text the scene shows, with its words: shown, not see-through,
/// not empty.
pub(crate) fn shown_texts(cx: &crate::cx::Cx) -> Vec<(ObjectId, String)> {
    crate::cx::shown_items(cx)
        .into_iter()
        .filter(|i| crate::cx::shown_opacity(cx.engine, *i) > 0.0)
        .filter_map(|i| text_of(cx.engine, i).map(|t| (i, t)))
        .filter(|(_, t)| !t.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mnemonic_is_not_read() {
        assert_eq!(without_mnemonic("&Save"), "Save");
        assert_eq!(without_mnemonic("Fish && Chips"), "Fish & Chips");
    }

    #[test]
    fn words_are_lower_case() {
        assert_eq!(word_list("More file-options…"), ["more", "file", "options"]);
    }
}
