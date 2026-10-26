//! `color-translucent-surface`: a popup's background lets what is behind
//! it show through.
//!
//! The background's fill, at the opacity it is shown with, is held to
//! `SURFACE_MIN_ALPHA`, but only where something is behind it to show: a
//! translucent popup over one flat colour is that colour a little
//! changed, which its own text was held against by `color-text-contrast`
//! already. That rule also composites what shows through under the text,
//! so the half of this one about the text's contrast is its.

use ljos_design::color::{delta_e_ok, over};
use ljos_design::limits::SURFACE_MIN_ALPHA;
use ljos_design::limits::audit::VISIBLE_DELTA_E;

use crate::acting::Act;
use crate::cx::{place_of, scene_rect, within};
use crate::pixels::{Painter, backdrop};
use crate::surfaces::{dominant, open_surfaces, own_rect};

pub(crate) fn check(act: &mut Act) {
    act.rest();
    let Some(snapshot) = act.scene.snapshot().cloned() else {
        return;
    };
    let mut found = Vec::new();
    {
        let cx = act.cx();
        let e = cx.engine;
        let back = backdrop(e, cx.options);
        let mut painter = Painter::new(back);
        for s in open_surfaces(&cx) {
            let Some(bg) = s.background else { continue };
            let own = place_of(e, bg).is_some_and(|p| !ljos_controls::is_style_url(&p));
            let Some((fill, _)) = own_rect(&cx, &snapshot, bg) else {
                continue;
            };
            if !own || fill.a + 0.005 >= SURFACE_MIN_ALPHA || fill.a <= 0.0 {
                continue;
            }
            let Some(region) = scene_rect(e, bg) else {
                continue;
            };
            let Some(under) = painter.draw(&snapshot, region, |p| {
                !cx.scene
                    .item_of(p.node)
                    .is_some_and(|i| within(e, i, s.item))
            }) else {
                continue;
            };
            // What shows through is something only where what is behind
            // is not one colour.
            let Some((most, _)) = dominant(under.pixels()) else {
                continue;
            };
            let varied = under
                .pixels()
                .filter(|c| delta_e_ok(over(fill, *c), over(fill, most)) >= VISIBLE_DELTA_E)
                .count();
            if varied < ljos_design::limits::audit::VISIBLE_CHANGE_MIN_PX {
                continue;
            }
            found.push((s.popup, bg, fill.a));
        }
    }
    for (popup, bg, alpha) in found {
        let what = crate::cx::item_words(act.engine, popup);
        let message = format!(
            "the background of {what} is {:.0} % opaque, so what is behind it shows through \
             and competes with its own content; {:.0} % is the least",
            alpha * 100.0,
            SURFACE_MIN_ALPHA * 100.0,
        );
        let f = act.report("color-translucent-surface", Some(bg), message);
        f.measured = Some(format!("alpha {:.2} < {:.2}", alpha, SURFACE_MIN_ALPHA));
    }
}
