//! `a11y-unnamed-icon-button` and `a11y-icon-button-no-tooltip`: a button
//! that shows only a picture says what it does to an assistive technology
//! by its name, and to someone who sees it by a tooltip.
//!
//! A button's text is its name when it has no other, so a button with an
//! icon and no name is one with no text: what it does is in a picture no
//! reader can read. Its text hidden by `display: IconOnly` is still its
//! name, and the eye still has only the picture.

use ljos_engine::ObjectId;

use super::controls::{ButtonLook, button_look, tool_tip};
use crate::cx::{Cx, acted_on, has_name, node_words};

/// The icon button the rule of an unnamed one reports, so the general
/// rule of something unnamed does not say it again.
pub(crate) fn unnamed_icon_button(cx: &Cx, item: Option<ObjectId>) -> bool {
    cx.options.wants("a11y-unnamed-icon-button")
        && item
            .and_then(|i| button_look(cx.engine, i))
            .is_some_and(|l| l.icon && !l.text_shown)
}

pub(crate) fn check(cx: &mut Cx) {
    for (key, _, data) in cx.nodes().iter() {
        if !acted_on(data) {
            continue;
        }
        let Some(item) = cx.item(*key) else { continue };
        let Some(ButtonLook {
            icon: true,
            text_shown: false,
            ..
        }) = button_look(cx.engine, item)
        else {
            continue;
        };
        if !has_name(data) {
            cx.report(
                "a11y-unnamed-icon-button",
                Some(item),
                "the button shows only an icon and has no name, so an assistive technology \
                 reads it as a button and no more; set `Accessible.name` (or a `text` with \
                 `display: AbstractButton.IconOnly`)"
                    .to_string(),
            );
        }
        if tool_tip(cx.engine, item).is_none() {
            cx.report(
                "a11y-icon-button-no-tooltip",
                Some(item),
                format!(
                    "{} shows only an icon and nothing explains it on hover; set \
                     `ToolTip.text` and `ToolTip.visible: hovered`",
                    node_words(data)
                ),
            );
        }
    }
}
