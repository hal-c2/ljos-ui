//! `layout-stacked-scrims`: two popups dim the window at once. Each shade
//! darkens what is behind it again, so the first popup sinks into the
//! dark and what is under both is nearly black; and two modal layers at
//! once leave a user unsure which one answers.

use ljos_controls::{Popup, item_popup};
use ljos_engine::ObjectId;

use crate::cx::{Cx, is_a, place_of, shown_items};

/// Whether the popup is open and dims: `dim`, which follows `modal`
/// unless it is set.
fn dims(cx: &Cx, popup: ObjectId) -> bool {
    let e = cx.engine;
    is_a::<Popup>(e, popup)
        && e.untrack(|| {
            e.with::<Popup, _>(popup, |p| p.visible && p.dim)
                .unwrap_or(false)
        })
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let mut dimming: Vec<(ObjectId, ObjectId)> = Vec::new();
    for item in shown_items(cx) {
        let Some(popup) = item_popup(e, item) else {
            continue;
        };
        if dimming.iter().all(|(p, _)| *p != popup) && dims(cx, popup) {
            dimming.push((popup, item));
        }
    }
    let Some(&(first, _)) = dimming.first() else {
        return;
    };
    let under = place_of(e, first).unwrap_or_else(|| "another popup".to_string());
    for &(popup, item) in &dimming[1..] {
        let place = place_of(e, popup);
        let f = cx.report(
            "layout-stacked-scrims",
            Some(item),
            format!(
                "this popup dims the window while the popup at {under} dims it too, so what \
                 is behind darkens twice; close the first, or set `dim: false` on all but the \
                 topmost"
            ),
        );
        if place.is_some() {
            f.place = place;
        }
    }
}
