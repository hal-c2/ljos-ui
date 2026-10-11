//! The checks that read the accessibility tree: names, pictures, names
//! that repeat, the room there is to press, and places in a set.

use std::collections::HashMap;

use ljos_a11y::{NodeData, NodeKey, Role};
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType};
use ljos_quick::{Accessible, AccessibleAttached, Image, Item};

use super::{Code, Cx, item_words, node_words, place_of};

/// The nodes that stand for an item, in tree order.
fn nodes(cx: &Cx) -> Vec<(NodeKey, NodeKey, NodeData)> {
    let mut out = Vec::new();
    cx.scene.accessibility_tree().walk(|key, parent, data| {
        if !key.is_announcement() {
            out.push((key, parent, data.clone()));
        }
    });
    out
}

/// Whether the node is something a user acts on. A text is focusable by
/// its role alone, as Qt has it, so that a screen reader can walk it: that
/// makes it nothing to press and nothing that wants a name.
fn acted_on(data: &NodeData) -> bool {
    data.actionable() && (data.qt_role != Role::StaticText || data.press)
}

fn has_name(data: &NodeData) -> bool {
    !data.name.trim().is_empty() || !data.labelled_by.is_empty()
}

/// `a11y-unnamed-interactive`: what a user acts on says what it is.
pub(super) fn names(cx: &mut Cx) {
    for (key, _, data) in nodes(cx) {
        // A scroll bar says what it is by its role and which way it goes;
        // nobody gives one a name.
        if data.qt_role == Role::ScrollBar {
            continue;
        }
        if acted_on(&data) && !has_name(&data) {
            let item = cx.item(key);
            cx.report(
                Code::UnnamedInteractive,
                item,
                format!(
                    "{} can be acted on and has no name, so an assistive technology cannot \
                     say what it does; set `Accessible.name`, or name a label with \
                     `Accessible.labelledBy`",
                    node_words(&data)
                ),
            );
        }
    }
}

fn is_a<T: QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(T::static_meta()))
}

fn ignored(engine: &Engine, item: ObjectId) -> bool {
    engine
        .existing_attached_object(item, &MetaRef::Static(Accessible::static_meta()))
        .is_some_and(|att| {
            engine
                .read(att, AccessibleAttached::IGNORED.index)
                .to_boolean()
        })
}

/// Every item the scene shows, popups and all, parents first.
pub(super) fn shown_items(cx: &Cx) -> Vec<ObjectId> {
    let tree = cx.scene.render_tree();
    let mut out = Vec::new();
    let mut stack = vec![tree.root()];
    while let Some(id) = stack.pop() {
        let Some(node) = tree.node(id) else { continue };
        if !node.visible() {
            continue;
        }
        if let Some(item) = cx.scene.item_of(id).filter(|i| cx.engine.is_alive(*i)) {
            out.push(item);
        }
        stack.extend(node.children().iter().rev().copied());
    }
    out
}

/// `a11y-unnamed-image`: a picture has a name, or says that it is only
/// decoration, or is part of something that is named for it.
pub(super) fn images(cx: &mut Cx) {
    let tree = cx.scene.accessibility_tree();
    let mut found = Vec::new();
    for item in shown_items(cx) {
        if !is_a::<Image>(cx.engine, item) || ignored(cx.engine, item) {
            continue;
        }
        if let Some(own) = cx.scene.accessible_node(item) {
            if !tree.get(own).is_some_and(has_name) {
                found.push(item);
            }
            continue;
        }
        // The icon of a button is the button's; so is a picture right
        // inside something that has a name.
        let parent = cx.engine.visual_parent(item);
        let mut cur = parent;
        let mut named_for = false;
        while let Some(above) = cur {
            if ignored(cx.engine, above) {
                named_for = true;
                break;
            }
            if let Some(data) = cx.scene.accessible_node(above).and_then(|k| tree.get(k)) {
                named_for = has_name(data) && (data.actionable() || Some(above) == parent);
                break;
            }
            cur = cx.engine.visual_parent(above);
        }
        if !named_for {
            found.push(item);
        }
    }
    for item in found {
        let words = item_words(cx.engine, item);
        cx.report(
            Code::UnnamedImage,
            Some(item),
            format!(
                "{words} is a picture with neither `Accessible.name` nor `Accessible.ignored`; \
                 describe it, or set `Accessible.ignored: true` if it is only decoration"
            ),
        );
    }
    // What says it is a picture without being an `Image`.
    for (key, _, data) in nodes(cx) {
        let Some(item) = cx.item(key) else { continue };
        if data.qt_role.image() && !has_name(&data) && !is_a::<Image>(cx.engine, item) {
            cx.report(
                Code::UnnamedImage,
                Some(item),
                format!(
                    "{} is a picture and has no name; set `Accessible.name`, or \
                     `Accessible.ignored: true` if it is only decoration",
                    node_words(&data)
                ),
            );
        }
    }
}

/// `a11y-duplicate-name`: two things to act on under one parent, with the
/// same role and the same name, cannot be told apart by ear or by voice.
pub(super) fn duplicates(cx: &mut Cx) {
    let mut first: HashMap<(NodeKey, String, String), NodeKey> = HashMap::new();
    for (key, parent, data) in nodes(cx) {
        let name = data.name.trim();
        if name.is_empty() || !acted_on(&data) {
            continue;
        }
        let id = (parent, format!("{:?}", data.role), name.to_string());
        match first.get(&id) {
            None => {
                first.insert(id, key);
            }
            Some(other) => {
                let other = cx
                    .item(*other)
                    .and_then(|i| place_of(cx.engine, i))
                    .map(|p| format!(" at {p}"))
                    .unwrap_or_default();
                let item = cx.item(key);
                cx.report(
                    Code::DuplicateName,
                    item,
                    format!(
                        "{} has the same name as the one beside it{other}; give each a name \
                         of its own",
                        node_words(&data)
                    ),
                );
            }
        }
    }
}

/// `a11y-target-size`: what is pressed is large enough to hit.
pub(super) fn target_size(cx: &mut Cx) {
    let least = cx.options.target_size;
    for (key, _, data) in nodes(cx) {
        // A link sits in a line of text, and what a scroll bar does the
        // wheel, the keys and a drag of the content do too: neither is
        // held to the size (WCAG 2.5.8, "inline" and "equivalent").
        if !acted_on(&data) || data.disabled || matches!(data.qt_role, Role::Link | Role::ScrollBar)
        {
            continue;
        }
        let (w, h) = (
            data.bounds.x1 - data.bounds.x0,
            data.bounds.y1 - data.bounds.y0,
        );
        // A hair under is a rounding, not a finding.
        if w + 0.01 >= least && h + 0.01 >= least {
            continue;
        }
        let item = cx.item(key);
        cx.report(
            Code::TargetSize,
            item,
            format!(
                "{} is {} by {}, smaller than the {least} by {least} there must be to press",
                node_words(&data),
                trim(w),
                trim(h),
            ),
        );
    }
}

/// A length without a tail of zeros.
fn trim(n: f64) -> String {
    let s = format!("{n:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// `a11y-set-size`: among rows of which one says its place, each says
/// its place and how many there are. Rows that are made as they scroll
/// into view cannot be counted, so the count has to be told; rows that
/// tell only half, or some of which do not tell, are read wrong.
pub(super) fn sets(cx: &mut Cx) {
    let all = nodes(cx);
    let mut telling: HashMap<NodeKey, bool> = HashMap::new();
    for (_, parent, data) in &all {
        if data.qt_role.in_set() {
            *telling.entry(*parent).or_default() |=
                data.position_in_set.is_some() || data.size_of_set.is_some();
        }
    }
    for (key, parent, data) in all {
        if !data.qt_role.in_set() || !telling.get(&parent).copied().unwrap_or(false) {
            continue;
        }
        let wrong = match (data.position_in_set, data.size_of_set) {
            (Some(place), Some(of)) if place >= 1 && place <= of => continue,
            (Some(place), Some(of)) => format!("says it is {place} of {of}"),
            (Some(_), None) => "says its place and not how many there are".to_string(),
            (None, Some(_)) => "says how many there are and not its place".to_string(),
            (None, None) => "does not say its place among rows that do".to_string(),
        };
        let item = cx.item(key);
        cx.report(
            Code::SetSize,
            item,
            format!(
                "{} {wrong}; rows made from a model tell both, so make this row from the \
                 model too",
                node_words(&data)
            ),
        );
    }
}

/// Whether the item is shown and enabled, as focus asks.
pub(super) fn takes_focus(engine: &Engine, item: ObjectId) -> bool {
    engine.read(item, Item::VISIBLE.index).to_boolean()
        && engine.read(item, Item::ENABLED.index).to_boolean()
}
