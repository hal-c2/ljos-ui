//! What the checks share: the scene, the report they add to, and the
//! ways of naming and placing an item.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::rc::Rc;

use ljos_a11y::{NodeData, NodeKey, Role, role_word};
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType};
use ljos_quick::{Accessible, AccessibleAttached, Item, QuickScene, Text};
use ljos_render::kurbo::{Point, Rect};

use crate::Options;
use crate::finding::{Bounds, Finding};

/// A tree deeper than this is a cycle someone built by hand.
pub(crate) const DEPTH: usize = 4096;

/// A node of the accessibility tree, with its parent.
pub(crate) type Node = (NodeKey, NodeKey, NodeData);

pub(crate) struct Cx<'a> {
    pub engine: &'a Engine,
    pub scene: &'a QuickScene,
    pub options: &'a Options,
    pub out: Vec<Finding>,
    nodes: OnceCell<Rc<Vec<Node>>>,
    /// How many nodes have each role and name, for locators.
    names: OnceCell<HashMap<(String, String), Vec<NodeKey>>>,
}

impl<'a> Cx<'a> {
    pub(crate) fn new(engine: &'a Engine, scene: &'a QuickScene, options: &'a Options) -> Self {
        Cx {
            engine,
            scene,
            options,
            out: Vec::new(),
            nodes: OnceCell::new(),
            names: OnceCell::new(),
        }
    }

    /// Adds a finding of `code` about `item`. The rule's code must be in
    /// `ljos_design::RULES`.
    pub(crate) fn report(
        &mut self,
        code: &'static str,
        item: Option<ObjectId>,
        message: String,
    ) -> &mut Finding {
        let rule = ljos_design::rule(code).unwrap_or_else(|| panic!("{code} is no rule"));
        let place = item.and_then(|i| place_of(self.engine, i));
        let data = item
            .and_then(|i| self.scene.accessible_node(i))
            .and_then(|k| self.scene.accessibility_tree().get(k).cloned());
        let (role, name) = match (&data, item) {
            (Some(d), _) => (
                Some(role_word(d.role).to_string()),
                d.name.trim().to_string(),
            ),
            // A Text that asks for no node is still read as a label of
            // what it says, and a report finds it by that.
            (None, Some(i)) if is_a::<Text>(self.engine, i) => {
                let text = self.engine.untrack(|| self.engine.read_by_name(i, "text"));
                (
                    Some("label".to_string()),
                    text.to_js_string().as_str().trim().to_string(),
                )
            }
            _ => (None, String::new()),
        };
        let name = Some(name).filter(|n| !n.is_empty());
        let locator = item.and_then(|i| self.locator(i, data.as_ref()));
        let bounds = item
            .and_then(|i| scene_rect(self.engine, i))
            .map(|r| Bounds {
                x: r.x0,
                y: r.y0,
                width: r.width(),
                height: r.height(),
            });
        self.out.push(Finding {
            code: rule.code,
            rule,
            item,
            place,
            role,
            name,
            locator,
            bounds,
            measured: None,
            message,
            points: Vec::new(),
        });
        self.out.last_mut().expect("just pushed")
    }

    /// The item behind a node.
    pub(crate) fn item(&self, key: NodeKey) -> Option<ObjectId> {
        self.scene.accessible_item(key)
    }

    /// The nodes that stand for an item, in tree order.
    pub(crate) fn nodes(&self) -> Rc<Vec<Node>> {
        Rc::clone(self.nodes.get_or_init(|| {
            let mut out = Vec::new();
            self.scene.accessibility_tree().walk(|key, parent, data| {
                if !key.is_announcement() {
                    out.push((key, parent, data.clone()));
                }
            });
            Rc::new(out)
        }))
    }

    /// A locator `app_find` finds the item by: its `objectName`, its
    /// role and name (and which of those it is), or its `id`.
    fn locator(&self, item: ObjectId, data: Option<&NodeData>) -> Option<String> {
        let e = self.engine;
        let object_name = e.untrack(|| e.read_by_name(item, "objectName").to_js_string());
        if !object_name.is_empty() {
            return Some(format!("#{}", object_name.as_str()));
        }
        let key = self.scene.accessible_node(item);
        if let (Some(data), Some(key)) = (data, key)
            && !data.name.trim().is_empty()
        {
            let role = role_word(data.role).to_string();
            let names = self.names.get_or_init(|| {
                let mut m: HashMap<(String, String), Vec<NodeKey>> = HashMap::new();
                for (k, _, d) in self.nodes().iter() {
                    m.entry((role_word(d.role).to_string(), d.name.clone()))
                        .or_default()
                        .push(*k);
                }
                m
            });
            let quoted = data.name.replace('\\', "\\\\").replace('"', "\\\"");
            let base = format!("role={role}[name=\"{quoted}\"]");
            let same = names.get(&(role, data.name.clone()));
            return Some(match same {
                Some(all) if all.len() > 1 => {
                    let nth = all.iter().position(|k| *k == key).unwrap_or(0);
                    format!("{base} >> nth={nth}")
                }
                _ => base,
            });
        }
        e.inspect_ids(&[item])
            .into_iter()
            .next()
            .flatten()
            .map(|id| format!("id={id}"))
    }
}

/// `file:line:column` of the QML that made the item.
pub(crate) fn place_of(engine: &Engine, item: ObjectId) -> Option<String> {
    let at = engine.declared_at(item)?;
    let at = at.trim_end().trim_end_matches(':');
    (!at.is_empty()).then(|| at.to_string())
}

/// A node as a message names it: `button "Save"`.
pub(crate) fn node_words(data: &NodeData) -> String {
    let role = role_word(data.role);
    if data.name.is_empty() {
        format!("the {role} with no name")
    } else {
        format!("the {role} {:?}", data.name)
    }
}

/// An item with no node as a message names it: by its `objectName` or
/// `id`, else by its type.
pub(crate) fn item_words(engine: &Engine, item: ObjectId) -> String {
    let ty = engine
        .meta_of(item)
        .map(|m| m.name.to_string())
        .unwrap_or_else(|| "item".into());
    let name = engine.read_by_name(item, "objectName").to_js_string();
    if name.is_empty() {
        format!("the {ty}")
    } else {
        format!("the {ty} {:?}", name.as_str())
    }
}

/// An item as a message names it: by its node when it has one.
pub(crate) fn words(cx: &Cx, item: ObjectId) -> String {
    let tree = cx.scene.accessibility_tree();
    match cx.scene.accessible_node(item).and_then(|k| tree.get(k)) {
        Some(data) => node_words(data),
        None => item_words(cx.engine, item),
    }
}

/// ` (file:line:column)`, or nothing.
pub(crate) fn at(engine: &Engine, item: ObjectId) -> String {
    place_of(engine, item)
        .map(|p| format!(" ({p})"))
        .unwrap_or_default()
}

/// Whether the node is something a user acts on. A text is focusable by
/// its role alone, as Qt has it, so that a screen reader can walk it: that
/// makes it nothing to press and nothing that wants a name.
pub(crate) fn acted_on(data: &NodeData) -> bool {
    data.actionable() && (data.qt_role != Role::StaticText || data.press)
}

pub(crate) fn has_name(data: &NodeData) -> bool {
    !data.name.trim().is_empty() || !data.labelled_by.is_empty()
}

pub(crate) fn is_a<T: QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(T::static_meta()))
}

/// `Accessible.ignored`.
pub(crate) fn ignored(engine: &Engine, item: ObjectId) -> bool {
    engine
        .existing_attached_object(item, &MetaRef::Static(Accessible::static_meta()))
        .is_some_and(|att| {
            engine
                .read(att, AccessibleAttached::IGNORED.index)
                .to_boolean()
        })
}

/// Every item the scene shows, popups and all, parents first.
pub(crate) fn shown_items(cx: &Cx) -> Vec<ObjectId> {
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

/// Whether the item is shown and enabled, as focus asks.
pub(crate) fn takes_focus(engine: &Engine, item: ObjectId) -> bool {
    engine.read(item, Item::VISIBLE.index).to_boolean()
        && engine.read(item, Item::ENABLED.index).to_boolean()
}

/// The items above `item`, nearest first.
pub(crate) fn ancestors(engine: &Engine, item: ObjectId) -> impl Iterator<Item = ObjectId> + '_ {
    let mut cur = engine.visual_parent(item);
    let mut depth = 0;
    std::iter::from_fn(move || {
        let here = cur.filter(|_| depth < DEPTH)?;
        depth += 1;
        cur = engine.visual_parent(here);
        Some(here)
    })
}

/// Whether `inner` is `outer` or lies inside it.
pub(crate) fn within(engine: &Engine, inner: ObjectId, outer: ObjectId) -> bool {
    inner == outer || ancestors(engine, inner).any(|a| a == outer)
}

/// The item's box in the scene's coordinates: the bounds of its four
/// corners, so a turned item has the box around it.
pub(crate) fn scene_rect(engine: &Engine, item: ObjectId) -> Option<Rect> {
    if !engine.is_alive(item) || !ljos_quick::is_item(engine, item) {
        return None;
    }
    let (w, h) = engine.untrack(|| {
        (
            engine.read(item, Item::WIDTH.index).to_number(),
            engine.read(item, Item::HEIGHT.index).to_number(),
        )
    });
    if !(w.is_finite() && h.is_finite()) {
        return None;
    }
    let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
        .map(|(x, y)| ljos_quick::map_to_scene(engine, item, Point::new(x, y)));
    let mut r = Rect::from_points(corners[0], corners[3]);
    for c in corners {
        r = r.union_pt(c);
    }
    Some(r)
}

/// The opacity the item is shown with: its own times its ancestors'.
pub(crate) fn shown_opacity(engine: &Engine, item: ObjectId) -> f64 {
    let own = |i: ObjectId| {
        engine
            .untrack(|| engine.read(i, Item::OPACITY.index))
            .to_number()
    };
    std::iter::once(item)
        .chain(ancestors(engine, item))
        .filter(|i| ljos_quick::is_item(engine, *i))
        .map(own)
        .product()
}

/// A length without a tail of zeros.
pub(crate) fn trim(n: f64) -> String {
    let s = format!("{n:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// Whether something between `item` and the window scrolls, so that what
/// lies past its edge can be brought into view.
pub(crate) fn scrolls(engine: &Engine, item: ObjectId) -> bool {
    ancestors(engine, item).any(|a| {
        is_a::<ljos_quick::Flickable>(engine, a) || is_a::<ljos_controls::ScrollView>(engine, a)
    })
}

/// The slack of `ljos_design::limits::audit::SLACK_PX`.
pub(crate) fn slack() -> f64 {
    f64::from(ljos_design::limits::audit::SLACK_PX)
}

/// How far `inner` reaches past `outer` on each side (left, top, right,
/// bottom); 0 where it does not.
pub(crate) fn overhang(inner: Rect, outer: Rect) -> [f64; 4] {
    [
        (outer.x0 - inner.x0).max(0.0),
        (outer.y0 - inner.y0).max(0.0),
        (inner.x1 - outer.x1).max(0.0),
        (inner.y1 - outer.y1).max(0.0),
    ]
}

/// The sides of [`overhang`] past the slack, in words: `"the right edge
/// by 40 px"`.
pub(crate) fn overhang_words(over: [f64; 4]) -> Option<String> {
    let names = ["left", "top", "right", "bottom"];
    let parts: Vec<String> = over
        .iter()
        .zip(names)
        .filter(|(n, _)| **n > slack())
        .map(|(n, side)| format!("the {side} edge by {} px", trim(*n)))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" and "))
}
