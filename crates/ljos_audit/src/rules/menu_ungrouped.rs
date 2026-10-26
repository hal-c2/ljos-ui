//! `layout-menu-ungrouped`, as the menu is built: a long menu with no
//! separator and no submenu is read item by item to find anything. The
//! static lint counts the items written out; this counts what a
//! `Repeater` or `Instantiator` adds as well, open or not.

use ljos_controls::{Menu, MenuItem, MenuSeparator};
use ljos_design::limits::MENU_MAX_UNGROUPED;
use ljos_engine::Value;

use super::controls::objects;
use crate::cx::{Cx, is_a};

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let Some(root) = cx.scene.root() else { return };
    for menu in objects(e, root) {
        if !is_a::<Menu>(e, menu) {
            continue;
        }
        let count = e.untrack(|| e.with::<Menu, _>(menu, |m| m.count).unwrap_or(0));
        let (mut items, mut grouped) = (0usize, false);
        for i in 0..count.max(0) {
            let entry = e
                .untrack(|| e.invoke_by_name(menu, "itemAt", &[Value::Int(i)]))
                .ok()
                .and_then(|v| v.as_object());
            let Some(entry) = entry else { continue };
            if is_a::<MenuSeparator>(e, entry) {
                grouped = true;
            } else if is_a::<MenuItem>(e, entry) {
                let sub = e.untrack(|| {
                    e.with::<MenuItem, _>(entry, |m| m.sub_menu.is_some())
                        .unwrap_or(false)
                });
                grouped |= sub;
                items += 1;
            }
        }
        if grouped || items <= MENU_MAX_UNGROUPED {
            continue;
        }
        // The menu itself, open or not: what it is declared as is where
        // it is fixed.
        cx.report(
            "layout-menu-ungrouped",
            Some(menu),
            format!(
                "the menu holds {items} items and no separator or submenu, more than the \
                 {MENU_MAX_UNGROUPED} one reads through at a glance; group them with \
                 `MenuSeparator` and move rare ones into a submenu"
            ),
        );
    }
}
