//! `layout-window-controls`: a window that draws its own frame draws its
//! own close control too, where every other window has it: at a top
//! corner. A frameless window with none can be closed only by the keys
//! or the task bar, and controls set somewhere else are not found where
//! the hand goes to look.
//!
//! The close control is told by its name or its icon (close, quit,
//! exit), and must itself be the outermost control at the top right (as
//! Windows, GNOME and KDE put it) or the top left (as macOS does), within
//! `limits::WINDOW_CONTROLS_MAX_INSET_PX`. What a popup holds is never
//! the window's.

use ljos_controls::item_popup;
use ljos_design::limits::WINDOW_CONTROLS_MAX_INSET_PX;
use ljos_engine::ObjectId;
use ljos_quick::{Row, RowLayout};

use super::controls::button_look;
use crate::cx::{Cx, acted_on, ancestors, is_a, scene_rect};

/// `Qt.FramelessWindowHint`, as Qt documents `Qt::WindowType`.
const FRAMELESS: i64 = 0x0800;

const CLOSE_WORDS: &[&str] = &["close", "close window", "quit", "exit"];
const CLOSE_ICONS: &[&str] = &["window-close", "application-exit", "close"];

fn frameless(cx: &Cx, root: ObjectId) -> bool {
    let e = cx.engine;
    let flags = e.untrack(|| e.read_by_name(root, "flags"));
    let n = flags.to_number();
    n.is_finite() && (n as i64) & FRAMELESS != 0
}

fn closes(cx: &Cx, item: ObjectId, name: &str) -> bool {
    let name = name.trim().to_lowercase();
    if CLOSE_WORDS.contains(&name.as_str()) {
        return true;
    }
    let e = cx.engine;
    button_look(e, item).is_some()
        && e.untrack(|| {
            e.with::<ljos_controls::AbstractButton, _>(item, |b| {
                let icon = b.icon.name.as_str().to_lowercase();
                CLOSE_ICONS.contains(&icon.as_str())
            })
            .unwrap_or(false)
        })
}

/// The group the controls sit in, as a reader would move them: the row
/// that holds the close control, or the control alone.
fn group(cx: &Cx, item: ObjectId) -> ObjectId {
    let e = cx.engine;
    e.visual_parent(item)
        .filter(|p| is_a::<Row>(e, *p) || is_a::<RowLayout>(e, *p))
        .unwrap_or(item)
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let Some(root) = cx.scene.root() else { return };
    if !frameless(cx, root) {
        return;
    }
    let window = cx.scene.viewport();
    let inset = f64::from(WINDOW_CONTROLS_MAX_INSET_PX);
    let mut found: Vec<(ObjectId, bool)> = Vec::new();
    for (key, _, data) in cx.nodes().iter() {
        if !acted_on(data) {
            continue;
        }
        let Some(item) = cx.item(*key) else { continue };
        if std::iter::once(item)
            .chain(ancestors(e, item))
            .any(|a| item_popup(e, a).is_some())
        {
            continue;
        }
        if !closes(cx, item, &data.name) {
            continue;
        }
        let Some(r) = scene_rect(e, item) else {
            continue;
        };
        let top = r.y0 - window.y0 <= inset;
        let right = window.x1 - r.x1 <= inset;
        let left = r.x0 - window.x0 <= inset;
        found.push((item, top && (right || left)));
    }
    if found.iter().any(|(_, at_corner)| *at_corner) {
        return;
    }
    match found.first() {
        None => {
            cx.report(
                "layout-window-controls",
                Some(root),
                "the window draws no frame of its own (`Qt.FramelessWindowHint`) and shows no \
                 close control, so only the keys or the task bar close it; put close, minimise \
                 and maximise buttons in its title bar at the top corner"
                    .to_string(),
            );
        }
        Some(&(close, _)) => {
            let at = group(cx, close);
            cx.report(
                "layout-window-controls",
                Some(at),
                format!(
                    "the frameless window's close control is not at its top corner (it is \
                     more than {} px from the top right and the top left), where a user looks \
                     for it; put the window controls in the title bar at the corner",
                    crate::cx::trim(inset)
                ),
            );
        }
    }
}
