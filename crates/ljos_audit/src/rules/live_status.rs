//! `a11y-live-status` (WCAG 4.1.3, status messages): a line that tells
//! what just happened ("Saved", "3 errors", "Connecting…") is read out
//! only when it says it is a status; otherwise a reader user never hears
//! it change.
//!
//! The scene cannot tell a status from a label by what it shows, so this
//! is a hint, given where the application calls the text a status: an
//! `id` or `objectName` of a status, a toast, a notice or an error, on a
//! text whose words are bound or set from script. A row of a list is left
//! out: a list of messages is no status line.

use ljos_a11y::accesskit::Role as AkRole;
use ljos_a11y::{NodeData, Role};
use ljos_engine::{ObjectId, Provenance};
use ljos_quick::{ItemView, Text};

use crate::cx::{Cx, ancestors, is_a, item_words, shown_items};

/// The words an `id` or `objectName` of a status line is made of.
const STATUS_WORDS: [&str; 7] = [
    "status",
    "toast",
    "notice",
    "notification",
    "snackbar",
    "banner",
    "error",
];

fn says_status(name: &str) -> bool {
    let name = name.to_lowercase();
    STATUS_WORDS.iter().any(|w| name.contains(w))
}

fn announced(data: &NodeData) -> bool {
    data.live.is_some()
        || matches!(
            data.qt_role,
            Role::StatusBar | Role::AlertMessage | Role::Notification
        )
        || matches!(data.role, AkRole::Status | AkRole::Alert)
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let tree = cx.scene.accessibility_tree();
    for item in shown_items(cx) {
        if !is_a::<Text>(e, item) || ancestors(e, item).any(|a| is_a::<ItemView>(e, a)) {
            continue;
        }
        let changes = matches!(
            e.provenance(item, Text::TEXT.index),
            Some(Provenance::Bound | Provenance::Assigned)
        );
        if !changes {
            continue;
        }
        let object_name = e
            .untrack(|| e.read_by_name(item, "objectName"))
            .to_js_string();
        let id = e.inspect_ids(&[item]).into_iter().next().flatten();
        let named = std::iter::once(object_name.as_str().to_string())
            .chain(id.clone())
            .find(|n| says_status(n));
        let Some(named) = named else { continue };
        // The text, or the item above it that speaks for it, may be
        // marked already.
        let marked = std::iter::once(item)
            .chain(ancestors(e, item))
            .filter_map(|i: ObjectId| cx.scene.accessible_node(i))
            .filter_map(|k| tree.get(k))
            .any(announced);
        if marked {
            continue;
        }
        let words = item_words(e, item);
        cx.report(
            "a11y-live-status",
            Some(item),
            format!(
                "{words} ({named:?}) reads like a status line and does not say it is one, \
                 so a reader does not announce it when it changes; set `Accessible.role: \
                 Accessible.StatusBar` (or `AlertMessage` for what must be heard at once)"
            ),
        );
    }
}
