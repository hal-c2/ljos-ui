//! What the checks of names, layout and content read of the controls: what
//! a button shows, what explains it, and the words of a label.

use ljos_engine::{Engine, MetaRef, ObjectId, QmlType};

use ljos_controls::{AbstractButton, ToolTip, ToolTipAttached};

use crate::cx::is_a;

/// `AbstractButton.IconOnly`, as Qt documents the enumeration.
const ICON_ONLY: i32 = 0;

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
                let text_shown = !text.trim().is_empty() && (b.display != ICON_ONLY || !icon);
                ButtonLook {
                    text,
                    icon,
                    text_shown,
                }
            })
        })
        .ok()
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

/// The words of a label, lower case, without what is between them.
pub(crate) fn word_list(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
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
