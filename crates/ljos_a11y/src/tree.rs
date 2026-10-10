//! A retained accessibility tree that knows what changed.
//!
//! Whoever owns the scene keeps one of these, tells it about single nodes
//! ([`Tree::upsert`], [`Tree::set_bounds`], [`Tree::remove`]) and about
//! child lists ([`Tree::set_children`]), and a host turns it into its own
//! toolkit's nodes. Nothing here walks the whole tree unless a host asks
//! for all of it.

use std::collections::{HashMap, HashSet};

use accesskit::{Action, Node, NodeId, Role as AkRole, TreeUpdate};

use crate::role::Role;

/// The identity of a node: the owner's own stable number for the item.
/// `0` is the tree's root, which stands for the scene and has no item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeKey(pub u64);

impl NodeKey {
    /// The scene itself: the parent of the top-level nodes.
    pub const ROOT: NodeKey = NodeKey(0);

    /// The AccessKit id when the tree is its own AccessKit tree.
    pub fn node_id(self) -> NodeId {
        NodeId(self.0)
    }
}

/// An axis-aligned box in scene coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Bounds {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn center(&self) -> (f64, f64) {
        ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }
}

/// What one exposed item tells an assistive technology.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeData {
    /// The role the item asked for; `NoRole` when it only has a name.
    pub qt_role: Role,
    /// The role it is exposed with.
    pub role: AkRole,
    pub name: String,
    pub description: String,
    /// The item's `objectName`, so tests and tools can find the node.
    pub author_id: String,
    pub bounds: Bounds,
    pub focusable: bool,
    pub disabled: bool,
    /// Whether the node takes the press action.
    pub press: bool,
}

impl NodeData {
    /// A node with `role` and nothing else.
    pub fn new(role: AkRole) -> Self {
        Self {
            qt_role: Role::NoRole,
            role,
            name: String::new(),
            description: String::new(),
            author_id: String::new(),
            bounds: Bounds::default(),
            focusable: false,
            disabled: false,
            press: false,
        }
    }

    /// Writes the data onto an AccessKit node. The children and the bounds
    /// are the caller's: a host that embeds the tree in its own has its
    /// own coordinates and its own child lists.
    pub fn apply(&self, node: &mut Node) {
        node.set_role(self.role);
        if !self.name.is_empty() {
            // AccessKit keeps a label's text in its value; `label` is for
            // what names another node.
            if self.role == AkRole::Label {
                node.set_value(self.name.clone());
            } else {
                node.set_label(self.name.clone());
            }
        }
        if !self.description.is_empty() {
            node.set_description(self.description.clone());
        }
        if !self.author_id.is_empty() {
            node.set_author_id(self.author_id.clone());
        }
        if self.disabled {
            node.set_disabled();
        }
        if self.focusable && !self.disabled {
            node.add_action(Action::Focus);
        }
        if self.press && !self.disabled {
            node.add_action(Action::Click);
        }
    }
}

#[derive(Debug)]
struct Slot {
    data: NodeData,
    /// `None` while the node is in no child list.
    parent: Option<NodeKey>,
    children: Vec<NodeKey>,
}

/// How much work the tree has been asked to do, for tests that prove one
/// change touches one node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeStats {
    /// Nodes whose own data changed (new nodes included).
    pub node_updates: u64,
    /// Child lists that changed.
    pub child_updates: u64,
    /// Nodes removed.
    pub removals: u64,
}

/// What changed since the last [`Tree::take_changes`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Nodes in the tree whose data or child list changed, in key order.
    pub changed: Vec<NodeKey>,
    /// Nodes that are gone.
    pub removed: Vec<NodeKey>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

/// The accessibility tree of one scene.
#[derive(Debug, Default)]
pub struct Tree {
    slots: HashMap<NodeKey, Slot>,
    top: Vec<NodeKey>,
    dirty: HashSet<NodeKey>,
    removed: Vec<NodeKey>,
    stats: TreeStats,
    /// Whether an AccessKit consumer has been given the whole tree.
    sent: bool,
}

impl Tree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of nodes, the root not counted.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn contains(&self, key: NodeKey) -> bool {
        self.slots.contains_key(&key)
    }

    pub fn get(&self, key: NodeKey) -> Option<&NodeData> {
        self.slots.get(&key).map(|s| &s.data)
    }

    /// The children of `key`; of the scene for [`NodeKey::ROOT`].
    pub fn children(&self, key: NodeKey) -> &[NodeKey] {
        if key == NodeKey::ROOT {
            return &self.top;
        }
        self.slots.get(&key).map_or(&[], |s| &s.children)
    }

    /// The parent of `key`: [`NodeKey::ROOT`] for a top-level node, `None`
    /// for a node that is in no child list (or is not there at all).
    pub fn parent(&self, key: NodeKey) -> Option<NodeKey> {
        self.slots.get(&key)?.parent
    }

    pub fn stats(&self) -> TreeStats {
        self.stats
    }

    /// Whether anything changed since the last [`Tree::take_changes`].
    pub fn has_changes(&self) -> bool {
        !self.dirty.is_empty() || !self.removed.is_empty()
    }

    /// Adds the node or replaces its data. A new node is in no child list
    /// until [`Tree::set_children`] puts it in one. Returns whether
    /// anything changed.
    pub fn upsert(&mut self, key: NodeKey, data: NodeData) -> bool {
        debug_assert_ne!(key, NodeKey::ROOT, "the root has no data");
        match self.slots.get_mut(&key) {
            Some(slot) if slot.data == data => return false,
            Some(slot) => slot.data = data,
            None => {
                self.slots.insert(
                    key,
                    Slot {
                        data,
                        parent: None,
                        children: Vec::new(),
                    },
                );
            }
        }
        self.dirty.insert(key);
        self.stats.node_updates += 1;
        true
    }

    /// Moves the node without touching the rest of its data.
    pub fn set_bounds(&mut self, key: NodeKey, bounds: Bounds) -> bool {
        let Some(slot) = self.slots.get_mut(&key) else {
            return false;
        };
        if slot.data.bounds == bounds {
            return false;
        }
        slot.data.bounds = bounds;
        self.dirty.insert(key);
        self.stats.node_updates += 1;
        true
    }

    /// Takes the node out. Its children take its place in its parent, in
    /// order, because an item that stops being exposed still has its
    /// exposed descendants on screen.
    pub fn remove(&mut self, key: NodeKey) -> bool {
        let Some(slot) = self.slots.remove(&key) else {
            return false;
        };
        for c in &slot.children {
            if let Some(s) = self.slots.get_mut(c) {
                s.parent = slot.parent;
            }
        }
        if let Some(parent) = slot.parent {
            let list = self.list_mut(parent);
            if let Some(at) = list.iter().position(|c| *c == key) {
                list.splice(at..=at, slot.children);
            }
            self.dirty.insert(parent);
            self.stats.child_updates += 1;
        }
        self.dirty.remove(&key);
        self.removed.push(key);
        self.stats.removals += 1;
        true
    }

    /// Sets the child list of `parent` ([`NodeKey::ROOT`] for the scene).
    /// A child that was in another list leaves it; a child dropped from
    /// this list is in no list until it is put in one or removed. Keys the
    /// tree does not have are skipped. Returns whether the list changed.
    pub fn set_children(&mut self, parent: NodeKey, children: &[NodeKey]) -> bool {
        if parent != NodeKey::ROOT && !self.slots.contains_key(&parent) {
            return false;
        }
        let new: Vec<NodeKey> = children
            .iter()
            .copied()
            .filter(|c| *c != parent && self.slots.contains_key(c))
            .collect();
        if self.children(parent) == new.as_slice() {
            return false;
        }
        let old = std::mem::take(self.list_mut(parent));
        for c in &old {
            if !new.contains(c)
                && let Some(s) = self.slots.get_mut(c)
                && s.parent == Some(parent)
            {
                s.parent = None;
            }
        }
        for c in &new {
            let was = self.slots.get(c).and_then(|s| s.parent);
            if let Some(was) = was
                && was != parent
            {
                self.list_mut(was).retain(|k| k != c);
                self.dirty.insert(was);
                self.stats.child_updates += 1;
            }
            if let Some(s) = self.slots.get_mut(c) {
                s.parent = Some(parent);
            }
        }
        *self.list_mut(parent) = new;
        self.dirty.insert(parent);
        self.stats.child_updates += 1;
        true
    }

    fn list_mut(&mut self, key: NodeKey) -> &mut Vec<NodeKey> {
        if key == NodeKey::ROOT {
            return &mut self.top;
        }
        // Callers check the slot is there; a missing one gets a list that
        // nobody reads rather than a panic.
        match self.slots.get_mut(&key) {
            Some(s) => &mut s.children,
            None => &mut self.top,
        }
    }

    /// Whether the node is reachable from the root.
    pub fn attached(&self, key: NodeKey) -> bool {
        let mut at = key;
        // A child list cannot loop back unless a caller builds a cycle;
        // the bound keeps that mistake from hanging the host.
        for _ in 0..=self.slots.len() {
            if at == NodeKey::ROOT {
                return true;
            }
            match self.slots.get(&at).and_then(|s| s.parent) {
                Some(p) => at = p,
                None => return false,
            }
        }
        false
    }

    /// Visits every node reachable from the root, parents before children
    /// and siblings in order, with the node's parent.
    pub fn walk(&self, mut f: impl FnMut(NodeKey, NodeKey, &NodeData)) {
        let mut stack: Vec<(NodeKey, NodeKey)> =
            self.top.iter().rev().map(|k| (*k, NodeKey::ROOT)).collect();
        while let Some((key, parent)) = stack.pop() {
            let Some(slot) = self.slots.get(&key) else {
                continue;
            };
            f(key, parent, &slot.data);
            stack.extend(slot.children.iter().rev().map(|c| (*c, key)));
        }
    }

    /// The first node, in tree order, with this role and name.
    pub fn find(&self, role: AkRole, name: &str) -> Option<NodeKey> {
        let mut found = None;
        self.walk(|key, _, data| {
            if found.is_none() && data.role == role && data.name == name {
                found = Some(key);
            }
        });
        found
    }

    /// What changed since the last call. A changed node that is not yet
    /// in the tree stays pending: a consumer may only be told about nodes
    /// it can reach.
    pub fn take_changes(&mut self) -> Changes {
        let mut changed: Vec<NodeKey> = self
            .dirty
            .iter()
            .copied()
            .filter(|k| self.attached(*k))
            .collect();
        changed.sort_unstable();
        for k in &changed {
            self.dirty.remove(k);
        }
        Changes {
            changed,
            removed: std::mem::take(&mut self.removed),
        }
    }

    /// The AccessKit node for `key`, bounds and children included, for a
    /// consumer whose tree is this tree. `None` for a key that is not
    /// there.
    pub fn accesskit_node(&self, key: NodeKey) -> Option<Node> {
        let mut node = if key == NodeKey::ROOT {
            Node::new(AkRole::GenericContainer)
        } else {
            let data = self.get(key)?;
            let mut node = Node::new(data.role);
            data.apply(&mut node);
            node.set_bounds(accesskit::Rect {
                x0: data.bounds.x0,
                y0: data.bounds.y0,
                x1: data.bounds.x1,
                y1: data.bounds.y1,
            });
            node
        };
        node.set_children(
            self.children(key)
                .iter()
                .map(|c| c.node_id())
                .collect::<Vec<_>>(),
        );
        Some(node)
    }

    /// The whole tree as an AccessKit update: what a consumer needs first.
    /// Clears the pending changes, since the consumer now has everything.
    pub fn full_update(&mut self) -> TreeUpdate {
        let mut nodes = Vec::with_capacity(self.len() + 1);
        if let Some(root) = self.accesskit_node(NodeKey::ROOT) {
            nodes.push((NodeKey::ROOT.node_id(), root));
        }
        let mut keys = Vec::with_capacity(self.len());
        self.walk(|key, _, _| keys.push(key));
        for key in keys {
            if let Some(node) = self.accesskit_node(key) {
                nodes.push((key.node_id(), node));
            }
        }
        let _ = self.take_changes();
        self.sent = true;
        TreeUpdate {
            nodes,
            tree: Some(accesskit::Tree::new(NodeKey::ROOT.node_id())),
            tree_id: accesskit::TreeId::ROOT,
            focus: NodeKey::ROOT.node_id(),
        }
    }

    /// The next AccessKit update: everything the first time, afterwards
    /// only the nodes that changed. `None` when there is nothing to say.
    pub fn take_update(&mut self) -> Option<TreeUpdate> {
        if !self.sent {
            return Some(self.full_update());
        }
        let changes = self.take_changes();
        if changes.changed.is_empty() {
            // AccessKit drops a node when no child list names it, so a
            // removal always comes with its parent's new list.
            return None;
        }
        let nodes = changes
            .changed
            .into_iter()
            .filter_map(|k| Some((k.node_id(), self.accesskit_node(k)?)))
            .collect();
        Some(TreeUpdate {
            nodes,
            tree: None,
            tree_id: accesskit::TreeId::ROOT,
            focus: NodeKey::ROOT.node_id(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: NodeKey = NodeKey(1);
    const B: NodeKey = NodeKey(2);
    const C: NodeKey = NodeKey(3);
    const D: NodeKey = NodeKey(4);

    fn button(name: &str) -> NodeData {
        NodeData {
            qt_role: Role::Button,
            name: name.into(),
            press: true,
            ..NodeData::new(AkRole::Button)
        }
    }

    fn group() -> NodeData {
        NodeData::new(AkRole::Group)
    }

    /// root > A(group) > [B, C], settled.
    fn sample() -> Tree {
        let mut t = Tree::new();
        t.upsert(A, group());
        t.upsert(B, button("Save"));
        t.upsert(C, button("Close"));
        t.set_children(NodeKey::ROOT, &[A]);
        t.set_children(A, &[B, C]);
        let _ = t.take_changes();
        t
    }

    fn order(t: &Tree) -> Vec<(u64, u64)> {
        let mut v = Vec::new();
        t.walk(|k, p, _| v.push((k.0, p.0)));
        v
    }

    #[test]
    fn a_new_node_is_in_no_list_until_it_is_placed() {
        let mut t = Tree::new();
        assert!(t.upsert(A, button("Save")));
        assert_eq!(t.parent(A), None);
        assert!(!t.attached(A));
        assert!(order(&t).is_empty());
        // Not announced while a consumer could not reach it.
        assert!(t.take_changes().changed.is_empty());
        assert!(t.has_changes());

        assert!(t.set_children(NodeKey::ROOT, &[A]));
        assert_eq!(t.parent(A), Some(NodeKey::ROOT));
        assert_eq!(t.take_changes().changed, vec![NodeKey::ROOT, A]);
        assert!(!t.has_changes());
    }

    #[test]
    fn walk_is_depth_first_in_sibling_order() {
        let mut t = sample();
        t.upsert(D, button("Deep"));
        t.set_children(B, &[D]);
        assert_eq!(order(&t), vec![(1, 0), (2, 1), (4, 2), (3, 1)]);
    }

    #[test]
    fn the_same_data_is_no_change() {
        let mut t = sample();
        let before = t.stats();
        assert!(!t.upsert(B, button("Save")));
        assert!(!t.set_children(A, &[B, C]));
        assert!(!t.set_bounds(B, Bounds::default()));
        assert_eq!(t.stats(), before);
        assert!(!t.has_changes());
    }

    #[test]
    fn a_rename_touches_one_node() {
        let mut t = sample();
        let before = t.stats();
        assert!(t.upsert(B, button("Store")));
        assert_eq!(t.stats().node_updates, before.node_updates + 1);
        assert_eq!(t.stats().child_updates, before.child_updates);
        let changes = t.take_changes();
        assert_eq!(changes.changed, vec![B]);
        assert!(changes.removed.is_empty());
        assert_eq!(t.find(AkRole::Button, "Store"), Some(B));
        assert_eq!(t.find(AkRole::Button, "Save"), None);
    }

    #[test]
    fn moving_touches_one_node() {
        let mut t = sample();
        assert!(t.set_bounds(C, Bounds::new(1.0, 2.0, 3.0, 4.0)));
        assert_eq!(t.take_changes().changed, vec![C]);
        assert_eq!(t.get(C).map(|d| d.bounds.center()), Some((2.0, 3.0)));
        assert!(!t.set_bounds(D, Bounds::default()), "no such node");
    }

    #[test]
    fn removing_a_node_promotes_its_children_in_place() {
        let mut t = sample();
        t.upsert(D, button("Last"));
        t.set_children(NodeKey::ROOT, &[A, D]);
        let _ = t.take_changes();

        assert!(t.remove(A));
        assert_eq!(t.children(NodeKey::ROOT), &[B, C, D]);
        assert_eq!(t.parent(B), Some(NodeKey::ROOT));
        let changes = t.take_changes();
        assert_eq!(changes.changed, vec![NodeKey::ROOT]);
        assert_eq!(changes.removed, vec![A]);
        assert!(!t.remove(A));
    }

    #[test]
    fn a_child_moves_between_lists() {
        let mut t = sample();
        assert!(t.set_children(NodeKey::ROOT, &[A, C]));
        assert_eq!(t.children(A), &[B]);
        assert_eq!(t.parent(C), Some(NodeKey::ROOT));
        // Both lists changed, and nothing else.
        assert_eq!(t.take_changes().changed, vec![NodeKey::ROOT, A]);
    }

    #[test]
    fn a_dropped_child_is_in_no_list() {
        let mut t = sample();
        assert!(t.set_children(A, &[B]));
        assert_eq!(t.parent(C), None);
        assert!(t.contains(C));
        assert_eq!(order(&t), vec![(1, 0), (2, 1)]);
    }

    #[test]
    fn unknown_keys_are_skipped() {
        let mut t = sample();
        assert!(!t.set_children(A, &[B, C, D]));
        assert!(!t.set_children(D, &[B]));
        assert_eq!(t.children(D), &[]);
    }

    #[test]
    fn a_label_keeps_its_text_in_the_value() {
        let mut label = NodeData::new(AkRole::Label);
        label.name = "Hello".into();
        label.description = "A greeting".into();
        label.author_id = "greeting".into();
        let mut node = Node::new(AkRole::Unknown);
        label.apply(&mut node);
        assert_eq!(node.role(), AkRole::Label);
        assert_eq!(node.value(), Some("Hello"));
        assert_eq!(node.label(), None);
        assert_eq!(node.description(), Some("A greeting"));
        assert_eq!(node.author_id(), Some("greeting"));
        assert!(!node.supports_action(Action::Click));
    }

    #[test]
    fn a_button_is_named_by_its_label_and_can_be_pressed() {
        let mut node = Node::new(AkRole::Unknown);
        let mut data = button("Save");
        data.focusable = true;
        data.apply(&mut node);
        assert_eq!(node.label(), Some("Save"));
        assert_eq!(node.value(), None);
        assert!(node.supports_action(Action::Click));
        assert!(node.supports_action(Action::Focus));

        data.disabled = true;
        let mut node = Node::new(AkRole::Unknown);
        data.apply(&mut node);
        assert!(node.is_disabled());
        assert!(!node.supports_action(Action::Click));
        assert!(!node.supports_action(Action::Focus));
    }

    #[test]
    fn the_first_update_is_the_whole_tree() {
        let mut t = sample();
        let update = t.take_update().expect("first update");
        assert!(update.tree.is_some());
        let ids: Vec<u64> = update.nodes.iter().map(|(id, _)| id.0).collect();
        assert_eq!(ids, vec![0, 1, 2, 3]);
        assert_eq!(update.nodes[0].1.children(), &[NodeId(1)]);
        assert_eq!(update.nodes[1].1.children(), &[NodeId(2), NodeId(3)]);
        assert!(t.take_update().is_none(), "nothing changed since");
    }

    #[test]
    fn later_updates_carry_only_what_changed() {
        let mut t = sample();
        let _ = t.take_update();

        t.upsert(C, button("Quit"));
        let update = t.take_update().expect("a rename");
        assert!(update.tree.is_none());
        assert_eq!(update.nodes.len(), 1);
        assert_eq!(update.nodes[0].0, NodeId(3));
        assert_eq!(update.nodes[0].1.label(), Some("Quit"));

        t.remove(B);
        let update = t.take_update().expect("a removal");
        assert_eq!(update.nodes.len(), 1);
        assert_eq!(update.nodes[0].0, NodeId(1));
        assert_eq!(update.nodes[0].1.children(), &[NodeId(3)]);
    }

    #[test]
    fn update_bounds_are_scene_coordinates() {
        let mut t = sample();
        t.set_bounds(B, Bounds::new(10.0, 20.0, 90.0, 50.0));
        let node = t.accesskit_node(B).expect("B");
        assert_eq!(
            node.bounds(),
            Some(accesskit::Rect {
                x0: 10.0,
                y0: 20.0,
                x1: 90.0,
                y1: 50.0
            })
        );
        assert!(t.accesskit_node(D).is_none());
    }
}
