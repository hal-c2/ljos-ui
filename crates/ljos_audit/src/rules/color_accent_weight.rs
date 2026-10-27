//! `color-accent-weight`: the accent is spent on too much.
//!
//! An accent marks the one thing to look at; on a tenth of the window or
//! more, or on two calls to action side by side, it marks nothing. Two
//! things are counted: the pixels of the window drawn in the palette's
//! accent (`highlight`), within one just noticeable step of it, against
//! `ACCENT_MAX_SHARE`; and the highlighted buttons of one surface (the
//! window, or one popup) against `PRIMARY_BUTTONS_MAX`. A hint: a design
//! may mean it.

use ljos_controls::Button;
use ljos_design::color::{Srgb, delta_e_ok};
use ljos_design::limits::audit::VISIBLE_DELTA_E;
use ljos_design::limits::hue::{ACCENT_MAX_SHARE, MIN_CHROMA, PRIMARY_BUTTONS_MAX};
use ljos_engine::{ObjectId, Value};

use crate::cx::{Cx, ancestors, is_a, item_words, shown_items, takes_focus, words};
use crate::pixels::{Painter, backdrop};
use crate::surfaces::qcolor;

/// The palette's accent, from the first item that has a palette.
fn accent(cx: &Cx, items: &[ObjectId]) -> Option<Srgb> {
    let e = cx.engine;
    items.iter().find_map(
        |item| match e.untrack(|| e.read_by_name(*item, "palette.highlight")) {
            Value::Color(c) => Some(qcolor(c).rgb()),
            _ => None,
        },
    )
}

/// The popup an item is shown in, `None` for the window itself.
fn surface_of(cx: &Cx, item: ObjectId) -> Option<ObjectId> {
    let e = cx.engine;
    std::iter::once(item)
        .chain(ancestors(e, item))
        .find_map(|a| ljos_controls::item_popup(e, a))
}

pub(crate) fn check(cx: &mut Cx) {
    let scene = cx.scene;
    let Some(snapshot) = scene.snapshot() else {
        return;
    };
    let e = cx.engine;
    let items = shown_items(cx);

    // Highlighted buttons, one surface at a time.
    let mut surfaces: Vec<(Option<ObjectId>, Vec<ObjectId>)> = Vec::new();
    for b in items.iter().copied() {
        let highlighted = is_a::<Button>(e, b)
            && takes_focus(e, b)
            && e.untrack(|| e.read_by_name(b, "highlighted")).to_boolean();
        if !highlighted {
            continue;
        }
        let s = surface_of(cx, b);
        match surfaces.iter_mut().find(|(k, _)| *k == s) {
            Some((_, v)) => v.push(b),
            None => surfaces.push((s, vec![b])),
        }
    }
    for (surface, buttons) in surfaces {
        if buttons.len() <= PRIMARY_BUTTONS_MAX {
            continue;
        }
        let on = surface.map_or_else(|| "the window".to_string(), |p| item_words(e, p));
        let first = words(cx, buttons[0]);
        let message = format!(
            "{} buttons on {on} are highlighted, {first} first; at most \
             {PRIMARY_BUTTONS_MAX} should be, so that one action stands out",
            buttons.len(),
        );
        let f = cx.report(
            "color-accent-weight",
            Some(buttons[PRIMARY_BUTTONS_MAX]),
            message,
        );
        f.measured = Some(format!("{} > {PRIMARY_BUTTONS_MAX}", buttons.len()));
    }

    // The share of the window in the accent.
    let Some(root) = cx.scene.root() else { return };
    let Some(accent) =
        accent(cx, &items).filter(|a| ljos_design::color::Oklch::from_srgb(*a).c >= MIN_CHROMA)
    else {
        return;
    };
    let mut painter = Painter::new(backdrop(e, cx.options));
    let Some(picture) = painter.draw(snapshot, snapshot.viewport, |_| true) else {
        return;
    };
    let total = (picture.width * picture.height) as f32;
    let accented = picture
        .pixels()
        .filter(|c| delta_e_ok(*c, accent) < VISIBLE_DELTA_E)
        .count() as f32;
    let share = accented / total.max(1.0);
    if share > ACCENT_MAX_SHARE {
        let message = format!(
            "{:.0} % of the window is drawn in the accent colour {}; at most {:.0} % keeps it \
             for what should stand out",
            share * 100.0,
            accent.to_hex(),
            ACCENT_MAX_SHARE * 100.0,
        );
        let f = cx.report("color-accent-weight", Some(root), message);
        f.measured = Some(format!("{:.2} > {ACCENT_MAX_SHARE:.2}", share));
    }
}
