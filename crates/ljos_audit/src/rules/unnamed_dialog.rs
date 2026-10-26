//! `a11y-unnamed-dialog`: a dialog that opens says what it is about. A
//! reader announces "dialog" when the focus moves into it, and with no
//! name that is all it says.
//!
//! A drawer is a pane slid in from an edge, named by what it holds; it is
//! left out.

use ljos_a11y::Role;
use ljos_controls::{Dialog, Drawer, item_popup};

use ljos_engine::{MetaRef, ObjectId, QmlType};
use ljos_quick::{Accessible, AccessibleAttached};

use crate::cx::{Cx, is_a, place_of};

fn popup_named(cx: &Cx, popup: ObjectId) -> bool {
    let e = cx.engine;
    e.existing_attached_object(popup, &MetaRef::Static(Accessible::static_meta()))
        .is_some_and(|att| {
            !e.untrack(|| e.read(att, AccessibleAttached::NAME.index))
                .to_js_string()
                .as_str()
                .trim()
                .is_empty()
        })
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    for (key, _, data) in cx.nodes().iter() {
        if data.qt_role != Role::Dialog || !data.name.trim().is_empty() {
            continue;
        }
        if !data.labelled_by.is_empty() {
            continue;
        }
        let Some(item) = cx.item(*key) else { continue };
        let popup = item_popup(e, item);
        if popup.is_some_and(|p| is_a::<Drawer>(e, p)) {
            continue;
        }
        // `Accessible.name` written on the popup itself, not its item.
        if popup.is_some_and(|p| popup_named(cx, p)) {
            continue;
        }
        let fix = if popup.is_some_and(|p| is_a::<Dialog>(e, p)) {
            "give it a `title`, or `Accessible.name`"
        } else {
            "set `Accessible.name` on it"
        };
        let what = if data.modal {
            "the modal dialog"
        } else {
            "the dialog"
        };
        let place = popup.and_then(|p| place_of(e, p));
        let f = cx.report(
            "a11y-unnamed-dialog",
            Some(item),
            format!(
                "{what} has no name, so a reader says only \"dialog\" as the focus moves into \
                 it; {fix}"
            ),
        );
        // The popup is what the QML declares; its item is made for it.
        if place.is_some() {
            f.place = place;
        }
    }
}
