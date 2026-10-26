//! `a11y-unnamed-interactive`: what a user acts on says what it is.

use ljos_a11y::{NodeKey, Role};

use crate::cx::{Cx, acted_on, has_name, node_words};

pub(crate) fn check(cx: &mut Cx) {
    let all = cx.nodes();
    // The number of a spin box is told inside the box, as Qt has it; the
    // box's name says what it counts.
    let boxes: Vec<NodeKey> = all
        .iter()
        .filter(|(_, _, d)| d.qt_role == Role::SpinBox)
        .map(|(k, _, _)| *k)
        .collect();
    for (key, parent, data) in all.iter() {
        // A scroll bar says what it is by its role and which way it goes;
        // nobody gives one a name.
        if data.qt_role == Role::ScrollBar || boxes.contains(parent) {
            continue;
        }
        if acted_on(data) && !has_name(data) {
            let item = cx.item(*key);
            cx.report(
                "a11y-unnamed-interactive",
                item,
                format!(
                    "{} can be acted on and has no name, so an assistive technology cannot \
                     say what it does; set `Accessible.name`, or name a label with \
                     `Accessible.labelledBy`",
                    node_words(data)
                ),
            );
        }
    }
}
