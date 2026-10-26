//! `a11y-current-unmarked`: the current row of a list looks like the
//! others. The list knows which row is current (the setting in use, the
//! page shown, the choice of a combo box), and someone who looks at it
//! cannot tell.
//!
//! Only where the row is surely meant as current: a view whose
//! `currentIndex` the application sets, of rows a user presses (a chat
//! log scrolled to its end by `currentIndex` has no current row to
//! show), and the list a combo box opens. The rows' looks are compared,
//! not their words: the colours, weights and what is shown of each item
//! in them. A view that draws its own `highlight` marks its current row.
//! When the rows differ from each other anyway, which is the current one
//! cannot be told and nothing is said.

use ljos_engine::{Engine, ObjectId, Provenance};
use ljos_quick::{Item, ItemView, Rectangle, Text, view_item_at};

use ljos_controls::{AbstractButton, ComboBox, item_popup};

use crate::cx::{Cx, DEPTH, acted_on, ancestors, is_a, shown_items};

/// The combo box whose list `view` is, if it is one.
fn combo_of(cx: &Cx, view: ObjectId, combos: &[(ObjectId, ObjectId)]) -> Option<ObjectId> {
    let e = cx.engine;
    let popup = ancestors(e, view).find_map(|a| item_popup(e, a))?;
    combos.iter().find(|(_, p)| *p == popup).map(|(c, _)| *c)
}

/// How a row looks, item by item, without what it says.
fn look(e: &Engine, row: ObjectId) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![(row, 0usize)];
    while let Some((item, depth)) = stack.pop() {
        if depth > DEPTH {
            break;
        }
        let read = |name: &str| e.untrack(|| e.read_by_name(item, name)).to_js_string();
        let shown = e.untrack(|| e.read(item, Item::VISIBLE.index)).to_boolean();
        let mut parts = vec![
            e.meta_of(item)
                .map(|m| m.name.to_string())
                .unwrap_or_default(),
            shown.to_string(),
        ];
        if shown {
            parts.push(read("opacity").as_str().to_string());
            if is_a::<Rectangle>(e, item) {
                parts.push(read("color").as_str().to_string());
            }
            if is_a::<Text>(e, item) {
                parts.push(read("color").as_str().to_string());
                let font = e
                    .untrack(|| {
                        e.with::<Text, _>(item, |t| {
                            format!(
                                "{} {} {} {}",
                                t.font.weight,
                                t.font.italic,
                                t.font.underline,
                                t.font.capitalization
                            )
                        })
                    })
                    .unwrap_or_default();
                parts.push(font);
            }
            if is_a::<AbstractButton>(e, item) {
                for p in ["highlighted", "checked", "down"] {
                    parts.push(read(p).as_str().to_string());
                }
            }
            for child in e.visual_children(item).into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        out.push(parts.join(" "));
    }
    out
}

/// Whether the view's own highlight is shown.
fn draws_highlight(e: &Engine, view: ObjectId) -> bool {
    // The item is there with no `highlight` too, and draws nothing.
    let Ok(Some(h)) =
        e.untrack(|| e.with::<ItemView, _>(view, |v| v.highlight.and(v.highlight_item)))
    else {
        return false;
    };
    let read = |p| e.untrack(|| e.read(h, p)).to_number();
    e.untrack(|| e.read(h, Item::VISIBLE.index)).to_boolean()
        && read(Item::OPACITY.index) > 0.0
        && read(Item::WIDTH.index) > 0.0
        && read(Item::HEIGHT.index) > 0.0
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let shown = shown_items(cx);
    let combos: Vec<(ObjectId, ObjectId)> = shown
        .iter()
        .filter(|i| is_a::<ComboBox>(e, **i))
        .filter_map(|c| {
            let p = e.untrack(|| e.read_by_name(*c, "popup")).as_object()?;
            Some((*c, p))
        })
        .collect();
    for view in shown.iter().copied().filter(|i| is_a::<ItemView>(e, *i)) {
        let Ok((count, index)) =
            e.untrack(|| e.with::<ItemView, _>(view, |v| (v.count, v.current_index)))
        else {
            continue;
        };
        let combo = combo_of(cx, view, &combos);
        let current = match combo {
            Some(c) => e
                .untrack(|| e.with::<ComboBox, _>(c, |b| b.current_index))
                .unwrap_or(-1),
            None => {
                let set = matches!(
                    e.provenance(view, ItemView::CURRENT_INDEX.index),
                    Some(Provenance::Assigned | Provenance::Bound)
                );
                if !set {
                    continue;
                }
                index
            }
        };
        if count < 2 || current < 0 || current >= count || draws_highlight(e, view) {
            continue;
        }
        let Some(row) = view_item_at(e, view, current as usize) else {
            continue;
        };
        if !shown.contains(&row) {
            continue;
        }
        // A row a user presses: its node is something to act on.
        let pressed = cx
            .scene
            .accessible_node(row)
            .and_then(|k| cx.scene.accessibility_tree().get(k).cloned())
            .is_some_and(|d| acted_on(&d) && d.press);
        if combo.is_none() && !pressed {
            continue;
        }
        let mine = look(e, row);
        let others: Vec<Vec<String>> = (0..count as usize)
            .filter(|i| *i != current as usize)
            .filter_map(|i| view_item_at(e, view, i))
            .filter(|r| shown.contains(r))
            .map(|r| look(e, r))
            .collect();
        let alike_each_other = others.windows(2).all(|w| w[0] == w[1]);
        if others.is_empty() || !alike_each_other || !others.contains(&mine) {
            continue;
        }
        let whose = if combo.is_some() {
            "the combo box's choice"
        } else {
            "the current row"
        };
        cx.report(
            "a11y-current-unmarked",
            Some(row),
            format!(
                "{whose} (row {}) looks like every other row, so nobody who looks can tell \
                 which it is; mark it (`highlighted: ListView.isCurrentItem`, a bold font, a \
                 check mark) and keep the mark from leaning on colour alone",
                current + 1
            ),
        );
    }
}
