//! A retained accessibility tree that knows what changed.
//!
//! Whoever owns the scene keeps one of these, tells it about single nodes
//! ([`Tree::upsert`], [`Tree::set_bounds`], [`Tree::remove`]) and about
//! child lists ([`Tree::set_children`]), and a host turns it into its own
//! toolkit's nodes. Nothing here walks the whole tree unless a host asks
//! for all of it.

use std::collections::{BTreeSet, HashMap, HashSet};

use accesskit::{Action, Live, Node, NodeId, Role as AkRole, Toggled, TreeUpdate};

use crate::role::Role;
use crate::text::TextValue;

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

    /// Whether the node is one the tree made for an announcement
    /// ([`Tree::announce`]): no item is behind it.
    pub fn is_announcement(self) -> bool {
        self.0 & FIRST_RUN_ID == 0 && self.0 & FIRST_ANNOUNCEMENT_KEY != 0
    }
}

/// How much an announcement may interrupt, as `Accessible.announce` of Qt
/// names it (doc.qt.io/qt-6/qaccessible.html#AnnouncementPoliteness-enum).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Politeness {
    /// Said when the screen reader next has nothing to say.
    #[default]
    Polite,
    /// Said at once, over whatever is being read.
    Assertive,
}

impl Politeness {
    /// Qt's number: `Accessible.Polite` is 0, `Accessible.Assertive` 1.
    /// Any other number is polite, the kind that cannot do harm.
    pub fn from_qt(value: i32) -> Politeness {
        if value == 1 {
            Politeness::Assertive
        } else {
            Politeness::Polite
        }
    }

    pub fn qt_value(self) -> i32 {
        match self {
            Politeness::Polite => 0,
            Politeness::Assertive => 1,
        }
    }

    fn live(self) -> Live {
        match self {
            Politeness::Polite => Live::Polite,
            Politeness::Assertive => Live::Assertive,
        }
    }

    fn slot(self) -> usize {
        self as usize
    }
}

/// The actions an item's author handles (`Accessible.onIncreaseAction`
/// and the rest), beyond those the node's other data already offers. The
/// node offers each to the assistive technology.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Handled {
    pub increase: bool,
    pub decrease: bool,
    pub scroll_up: bool,
    pub scroll_down: bool,
    pub scroll_left: bool,
    pub scroll_right: bool,
    /// `Accessible.onShowOnScreenAction`: bring the item into view.
    pub show_on_screen: bool,
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

/// Where a node that stands for a number is between two ends: a scroll
/// bar, a slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    /// How far one increase or decrease goes. Nothing (`0`) for a node
    /// that only shows its value: it takes no such action.
    pub step: f64,
}

/// How far what is in a node is scrolled, and how far it can be, in each
/// direction. The node takes the scroll actions that have somewhere to go.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scroll {
    pub x: f64,
    pub x_min: f64,
    pub x_max: f64,
    pub y: f64,
    pub y_min: f64,
    pub y_max: f64,
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
    /// Whether an assistive technology may give the node the keyboard.
    pub focusable: bool,
    /// Whether the keyboard is at or below the node. AccessKit keeps
    /// focus on the tree, not on a node, so this does not reach
    /// [`NodeData::apply`]; [`Tree::focus`] reads it.
    pub focused: bool,
    pub disabled: bool,
    /// Whether the node takes the press action.
    pub press: bool,
    /// Whether what is outside the node cannot be reached while it is
    /// shown (a modal dialog).
    pub modal: bool,
    /// The number the node stands for, when it stands for one.
    pub range: Option<Range>,
    /// Where the node's content is scrolled to, when it scrolls.
    pub scroll: Option<Scroll>,
    /// Whether what is in the node shows only inside its box. This is how
    /// AccessKit learns that a child scrolled out of it is off screen:
    /// the child stays in the tree, with bounds outside its parent's.
    pub clips_children: bool,
    /// The text the node holds, when it holds text that can be edited.
    pub text: Option<TextValue>,
    /// Whether it is checked, for a node that can be; `None` for one that
    /// cannot. `Mixed` is the third state of a check box that stands for
    /// several others.
    pub toggled: Option<Toggled>,
    /// Whether it is held down right now. AccessKit 0.24 has no property
    /// for this, so it does not reach [`NodeData::apply`]; tests, the
    /// audit and tools read it here.
    pub pressed: bool,
    /// Whether what it opens is open, for a node that opens something;
    /// `None` for one that does not.
    pub expanded: Option<bool>,
    /// Whether it is selected, for a node that can be; `None` for one
    /// that cannot. An assistive technology tells "not selected" from
    /// "selection means nothing here".
    pub selected: Option<bool>,
    /// Whether what it shows can be read but not changed. A node with
    /// [`NodeData::text`] says so there as well.
    pub read_only: bool,
    /// Which of its set the node is, counted from 1, and how many the set
    /// has: a row of a list knows both even when its siblings do not
    /// exist (a virtualised list makes only the rows on screen).
    pub position_in_set: Option<usize>,
    pub size_of_set: Option<usize>,
    /// Whether a change of the node's name is said without the user going
    /// to it, and how urgently.
    pub live: Option<Politeness>,
    /// The nodes that name this one. Keys the tree does not have are left
    /// out when the node is written.
    pub labelled_by: Vec<NodeKey>,
    /// How deep a heading is, 1 being the top.
    pub level: Option<usize>,
    /// What to call the role when AccessKit's word says less than the
    /// author's ("dial" for a slider that turns).
    pub role_description: String,
    pub handled: Handled,
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
            focused: false,
            disabled: false,
            press: false,
            modal: false,
            range: None,
            scroll: None,
            clips_children: false,
            text: None,
            toggled: None,
            pressed: false,
            expanded: None,
            selected: None,
            read_only: false,
            position_in_set: None,
            size_of_set: None,
            live: None,
            labelled_by: Vec::new(),
            level: None,
            role_description: String::new(),
            handled: Handled::default(),
        }
    }

    /// Whether an assistive technology may ask the node to do something,
    /// or give it the keyboard: such a node needs a name.
    pub fn actionable(&self) -> bool {
        self.press
            || self.focusable
            || self.toggled.is_some()
            || self.text.is_some()
            || self.range.is_some_and(|r| r.step > 0.0)
            || self.qt_role.actionable()
    }

    /// Whether a change of the node's name is said without the user going
    /// to it: as [`NodeData::live`] says, or else as its role has it
    /// ([`Role::live`]), so a status text an author gave a role is a live
    /// region without more.
    pub fn politeness(&self) -> Option<Politeness> {
        self.live.or_else(|| self.qt_role.live())
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
        if self.modal {
            node.set_modal();
        }
        if let Some(r) = self.range {
            node.set_numeric_value(r.value);
            node.set_min_numeric_value(r.min);
            node.set_max_numeric_value(r.max);
            if r.step > 0.0 {
                node.set_numeric_value_step(r.step);
                if !self.disabled {
                    node.add_action(Action::Increment);
                    node.add_action(Action::Decrement);
                    node.add_action(Action::SetValue);
                }
            }
        }
        if let Some(t) = self.toggled {
            node.set_toggled(t);
        }
        if let Some(open) = self.expanded {
            node.set_expanded(open);
            if !self.disabled {
                node.add_action(if open {
                    Action::Collapse
                } else {
                    Action::Expand
                });
            }
        }
        if let Some(on) = self.selected {
            node.set_selected(on);
        }
        if self.read_only {
            node.set_read_only();
        }
        if let Some(at) = self.position_in_set {
            node.set_position_in_set(at);
        }
        if let Some(of) = self.size_of_set {
            node.set_size_of_set(of);
        }
        if let Some(p) = self.politeness() {
            node.set_live(p.live());
            // The whole message is said, not the part that differs from
            // the one before.
            node.set_live_atomic();
        }
        if let Some(level) = self.level {
            node.set_level(level);
        }
        if !self.role_description.is_empty() {
            node.set_role_description(self.role_description.clone());
        }
        if !self.disabled {
            let h = self.handled;
            let offered = [
                (Action::Increment, h.increase),
                (Action::Decrement, h.decrease),
                (Action::ScrollUp, h.scroll_up),
                (Action::ScrollDown, h.scroll_down),
                (Action::ScrollLeft, h.scroll_left),
                (Action::ScrollRight, h.scroll_right),
                (Action::ScrollIntoView, h.show_on_screen),
            ];
            for (action, on) in offered {
                if on {
                    node.add_action(action);
                }
            }
        }
        if let Some(s) = self.scroll {
            node.set_scroll_x(s.x);
            node.set_scroll_x_min(s.x_min);
            node.set_scroll_x_max(s.x_max);
            node.set_scroll_y(s.y);
            node.set_scroll_y_min(s.y_min);
            node.set_scroll_y_max(s.y_max);
            let ways = [
                (Action::ScrollLeft, s.x > s.x_min),
                (Action::ScrollRight, s.x < s.x_max),
                (Action::ScrollUp, s.y > s.y_min),
                (Action::ScrollDown, s.y < s.y_max),
            ];
            for (action, room) in ways {
                if room && !self.disabled {
                    node.add_action(action);
                }
            }
        }
        if self.clips_children {
            node.set_clips_children();
        }
        if let Some(t) = &self.text {
            // The runs and the selection are the caller's, as the children
            // are: they name nodes.
            node.set_value(t.value.clone());
            if t.read_only {
                node.set_read_only();
            }
            if !self.disabled {
                node.add_action(Action::SetTextSelection);
                if !t.read_only {
                    node.add_action(Action::SetValue);
                    node.add_action(Action::ReplaceSelectedText);
                }
            }
        }
    }

    /// Writes what names other nodes. `id_of` gives the id a node has
    /// where this one is written, or `None` when it is not written there:
    /// like the children, the ids are the caller's.
    pub fn apply_relations(&self, node: &mut Node, id_of: impl Fn(NodeKey) -> Option<NodeId>) {
        let named_by: Vec<NodeId> = self.labelled_by.iter().filter_map(|k| id_of(*k)).collect();
        if !named_by.is_empty() {
            node.set_labelled_by(named_by);
        }
    }
}

#[derive(Debug)]
struct Slot {
    data: NodeData,
    /// `None` while the node is in no child list.
    parent: Option<NodeKey>,
    children: Vec<NodeKey>,
    /// The AccessKit ids of the runs of the node's text.
    runs: Vec<NodeId>,
}

/// The ids of text runs are counted up from here. An item's key has this
/// bit only after its slot was reused two thousand million times.
const FIRST_RUN_ID: u64 = 1 << 63;

/// The keys of announcement nodes are counted up from here; an item's key
/// has this bit only after its slot was reused a thousand million times.
const FIRST_ANNOUNCEMENT_KEY: u64 = 1 << 62;

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
    /// The nodes that say they are focused: the focus item and the focus
    /// scopes around it, so a handful at most.
    focused: BTreeSet<NodeKey>,
    stats: TreeStats,
    /// Whether an AccessKit consumer has been given the whole tree.
    sent: bool,
    /// The focus the AccessKit consumer was last told.
    sent_focus: Option<NodeKey>,
    /// How many run ids were given out, and whose run each is.
    run_ids: u64,
    run_owners: HashMap<NodeId, (NodeKey, usize)>,
    /// The node of the latest announcement of each [`Politeness`]. They
    /// are the last children of the scene, whatever its other children.
    announced: [Option<NodeKey>; 2],
    announcements: u64,
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
        if data.focused {
            self.focused.insert(key);
        } else {
            self.focused.remove(&key);
        }
        let runs = data.text.as_ref().map_or(0, |t| t.runs().len());
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
                        runs: Vec::new(),
                    },
                );
            }
        }
        self.set_run_count(key, runs);
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
        for id in &slot.runs {
            self.run_owners.remove(id);
        }
        self.dirty.remove(&key);
        self.focused.remove(&key);
        for a in &mut self.announced {
            if *a == Some(key) {
                *a = None;
            }
        }
        self.removed.push(key);
        self.stats.removals += 1;
        true
    }

    /// Gives the node's text `count` run ids, keeping the ones it has.
    fn set_run_count(&mut self, key: NodeKey, count: usize) {
        let Some(slot) = self.slots.get_mut(&key) else {
            return;
        };
        for id in slot.runs.drain(count.min(slot.runs.len())..) {
            self.run_owners.remove(&id);
        }
        while slot.runs.len() < count {
            let id = NodeId(FIRST_RUN_ID | self.run_ids);
            self.run_ids += 1;
            self.run_owners.insert(id, (key, slot.runs.len()));
            slot.runs.push(id);
        }
    }

    /// The node whose text the run `id` is of, and which of its runs it
    /// is: what a selection an assistive technology asks for names.
    pub fn text_run(&self, id: NodeId) -> Option<(NodeKey, usize)> {
        self.run_owners.get(&id).copied()
    }

    /// The AccessKit nodes of the runs of the text of `key`.
    fn run_nodes(&self, key: NodeKey) -> Vec<(NodeId, Node)> {
        let Some(slot) = self.slots.get(&key) else {
            return Vec::new();
        };
        let Some(text) = &slot.data.text else {
            return Vec::new();
        };
        text.runs()
            .iter()
            .zip(&slot.runs)
            .map(|(run, id)| {
                let mut node = Node::new(AkRole::TextRun);
                run.apply(&text.value, &mut node);
                (*id, node)
            })
            .collect()
    }

    /// Sets the child list of `parent` ([`NodeKey::ROOT`] for the scene).
    /// A child that was in another list leaves it; a child dropped from
    /// this list is in no list until it is put in one or removed. Keys the
    /// tree does not have are skipped. Returns whether the list changed.
    pub fn set_children(&mut self, parent: NodeKey, children: &[NodeKey]) -> bool {
        if parent != NodeKey::ROOT && !self.slots.contains_key(&parent) {
            return false;
        }
        let mut new: Vec<NodeKey> = children
            .iter()
            .copied()
            .filter(|c| *c != parent && !c.is_announcement() && self.slots.contains_key(c))
            .collect();
        if parent == NodeKey::ROOT {
            // The scene's owner lists its items; what was announced stays.
            new.extend(self.announced.iter().flatten());
        }
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

    /// Says `message` to whoever listens without moving them: the tree
    /// gets a node that holds it, a status for a polite announcement and an
    /// alert for an assertive one, in place of the one before of that
    /// kind. The node has no size and nothing paints it.
    ///
    /// Each announcement is a new node: an assistive technology says a
    /// live node when it appears, so the same message twice is said twice.
    pub fn announce(&mut self, message: &str, politeness: Politeness) -> NodeKey {
        if let Some(old) = self.announced[politeness.slot()].take() {
            self.remove(old);
        }
        let key = NodeKey(FIRST_ANNOUNCEMENT_KEY | self.announcements);
        self.announcements += 1;
        let role = match politeness {
            Politeness::Polite => AkRole::Status,
            Politeness::Assertive => AkRole::Alert,
        };
        self.upsert(
            key,
            NodeData {
                name: message.to_string(),
                live: Some(politeness),
                ..NodeData::new(role)
            },
        );
        self.announced[politeness.slot()] = Some(key);
        if let Some(slot) = self.slots.get_mut(&key) {
            slot.parent = Some(NodeKey::ROOT);
        }
        // Polite before assertive, so the order does not depend on which
        // was said first.
        self.top.retain(|k| !k.is_announcement());
        self.top.extend(self.announced.iter().flatten());
        self.dirty.insert(NodeKey::ROOT);
        self.stats.child_updates += 1;
        key
    }

    /// What was last announced with `politeness`, if anything was.
    pub fn announcement(&self, politeness: Politeness) -> Option<&str> {
        let key = self.announced[politeness.slot()]?;
        self.get(key).map(|d| d.name.as_str())
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
        self.depth(key).is_some()
    }

    /// How many steps the node is below the root; `None` when it is not
    /// reachable from it.
    fn depth(&self, key: NodeKey) -> Option<usize> {
        let mut at = key;
        // A child list cannot loop back unless a caller builds a cycle;
        // the bound keeps that mistake from hanging the host.
        for depth in 0..=self.slots.len() {
            if at == NodeKey::ROOT {
                return Some(depth);
            }
            at = self.slots.get(&at)?.parent?;
        }
        None
    }

    /// The node that has the keyboard: the deepest reachable node that
    /// says it is focused, which is the focus item itself or, when that is
    /// not exposed, the nearest focus scope around it that is. `None`
    /// when no node is focused; a consumer then reports the root.
    ///
    /// Two focused nodes at one depth are an author's doing
    /// (`Accessible.focused` on an item without the keyboard); the lower
    /// key wins so that the answer does not change between calls.
    pub fn focus(&self) -> Option<NodeKey> {
        let mut best: Option<(usize, NodeKey)> = None;
        for key in &self.focused {
            if let Some(depth) = self.depth(*key)
                && best.is_none_or(|(d, _)| depth > d)
            {
                best = Some((depth, *key));
            }
        }
        best.map(|(_, key)| key)
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
            data.apply_relations(&mut node, |k| self.attached(k).then(|| k.node_id()));
            node.set_bounds(accesskit::Rect {
                x0: data.bounds.x0,
                y0: data.bounds.y0,
                x1: data.bounds.x1,
                y1: data.bounds.y1,
            });
            node
        };
        // The runs of its text come first: they are what the node says.
        let runs: &[NodeId] = self.slots.get(&key).map_or(&[], |s| &s.runs);
        if let Some(text) = self.get(key).and_then(|d| d.text.as_ref())
            && !runs.is_empty()
        {
            node.set_text_selection(
                text.selection(&text.runs(), |i| runs.get(i).copied().unwrap_or(runs[0])),
            );
        }
        node.set_children(
            runs.iter()
                .copied()
                .chain(self.children(key).iter().map(|c| c.node_id()))
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
                nodes.extend(self.run_nodes(key));
            }
        }
        let _ = self.take_changes();
        self.sent = true;
        self.sent_focus = self.focus();
        TreeUpdate {
            nodes,
            tree: Some(accesskit::Tree::new(NodeKey::ROOT.node_id())),
            tree_id: accesskit::TreeId::ROOT,
            focus: self.sent_focus.unwrap_or(NodeKey::ROOT).node_id(),
        }
    }

    /// The next AccessKit update: everything the first time, afterwards
    /// only the nodes that changed, with the focus as it is now. `None`
    /// when there is nothing to say.
    pub fn take_update(&mut self) -> Option<TreeUpdate> {
        if !self.sent {
            return Some(self.full_update());
        }
        let changes = self.take_changes();
        // Only reachable nodes are announced, and the focus is one of
        // them, so the consumer knows the node it is told about.
        let focus = self.focus();
        // AccessKit drops a node when no child list names it, so a
        // removal always comes with its parent's new list.
        if changes.changed.is_empty() && focus == self.sent_focus {
            return None;
        }
        self.sent_focus = focus;
        let mut nodes = Vec::with_capacity(changes.changed.len());
        for k in changes.changed {
            if let Some(node) = self.accesskit_node(k) {
                nodes.push((k.node_id(), node));
                nodes.extend(self.run_nodes(k));
            }
        }
        Some(TreeUpdate {
            nodes,
            tree: None,
            tree_id: accesskit::TreeId::ROOT,
            focus: focus.unwrap_or(NodeKey::ROOT).node_id(),
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
    fn a_node_with_a_step_takes_increase_and_decrease() {
        let mut bar = NodeData::new(AkRole::ScrollBar);
        bar.range = Some(Range {
            value: 0.25,
            min: 0.0,
            max: 0.75,
            step: 0.1,
        });
        let mut node = Node::new(AkRole::Unknown);
        bar.apply(&mut node);
        assert_eq!(node.numeric_value(), Some(0.25));
        assert_eq!(node.min_numeric_value(), Some(0.0));
        assert_eq!(node.max_numeric_value(), Some(0.75));
        assert_eq!(node.numeric_value_step(), Some(0.1));
        assert!(node.supports_action(Action::Increment));
        assert!(node.supports_action(Action::SetValue));

        // One that only shows a number takes nothing.
        bar.range = bar.range.map(|r| Range { step: 0.0, ..r });
        let mut node = Node::new(AkRole::Unknown);
        bar.apply(&mut node);
        assert_eq!(node.numeric_value(), Some(0.25));
        assert_eq!(node.numeric_value_step(), None);
        assert!(!node.supports_action(Action::Increment));
    }

    #[test]
    fn a_node_that_scrolls_takes_the_actions_that_have_somewhere_to_go() {
        let mut view = NodeData::new(AkRole::ScrollView);
        view.clips_children = true;
        view.scroll = Some(Scroll {
            x_max: 300.0,
            y: 200.0,
            y_max: 200.0,
            ..Scroll::default()
        });
        let mut node = Node::new(AkRole::Unknown);
        view.apply(&mut node);
        assert!(node.clips_children());
        assert_eq!(node.scroll_y(), Some(200.0));
        assert_eq!(node.scroll_y_max(), Some(200.0));
        assert!(node.supports_action(Action::ScrollUp));
        assert!(!node.supports_action(Action::ScrollDown));
        assert!(node.supports_action(Action::ScrollRight));
        assert!(!node.supports_action(Action::ScrollLeft));
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

    fn focused(mut data: NodeData) -> NodeData {
        data.focusable = true;
        data.focused = true;
        data
    }

    #[test]
    fn the_focus_is_the_deepest_focused_node() {
        let mut t = sample();
        assert_eq!(t.focus(), None);
        // A focus scope and the focus item inside it both say so.
        t.upsert(A, focused(group()));
        assert_eq!(t.focus(), Some(A));
        t.upsert(C, focused(button("Close")));
        assert_eq!(t.focus(), Some(C));
        // The item stops being exposed: the scope around it has the focus.
        t.remove(C);
        assert_eq!(t.focus(), Some(A));
        t.upsert(A, group());
        assert_eq!(t.focus(), None);
    }

    #[test]
    fn a_node_nobody_can_reach_is_not_the_focus() {
        let mut t = sample();
        t.upsert(D, focused(button("Loose")));
        assert_eq!(t.focus(), None);
        t.set_children(B, &[D]);
        assert_eq!(t.focus(), Some(D));
        t.set_children(B, &[]);
        assert_eq!(t.focus(), None);
    }

    #[test]
    fn updates_say_where_the_focus_is() {
        let mut t = sample();
        t.upsert(B, focused(button("Save")));
        let update = t.take_update().expect("first update");
        assert_eq!(update.focus, NodeId(2));

        // Focus moves: the two nodes that changed, and the new focus.
        t.upsert(B, button("Save"));
        t.upsert(C, focused(button("Close")));
        let update = t.take_update().expect("focus moved");
        let ids: Vec<u64> = update.nodes.iter().map(|(id, _)| id.0).collect();
        assert_eq!(ids, vec![2, 3]);
        assert_eq!(update.focus, NodeId(3));
        assert!(t.take_update().is_none(), "nothing changed since");

        // Nobody has it: the root does, as AccessKit asks.
        t.upsert(C, button("Close"));
        assert_eq!(t.take_update().expect("focus left").focus, NodeId(0));
    }

    #[test]
    fn a_focus_node_that_goes_away_takes_the_focus_with_it() {
        let mut t = sample();
        t.upsert(C, focused(button("Close")));
        let _ = t.take_update();
        t.remove(C);
        let update = t.take_update().expect("a removal");
        assert_eq!(update.focus, NodeId(0));
        assert!(update.nodes.iter().all(|(id, _)| *id != NodeId(3)));
    }

    #[test]
    fn a_new_tree_says_the_root_has_the_focus() {
        let mut t = sample();
        assert_eq!(t.full_update().focus, NodeId(0));
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

    fn applied(data: &NodeData) -> Node {
        let mut node = Node::new(data.role);
        data.apply(&mut node);
        node
    }

    #[test]
    fn a_plain_node_says_none_of_the_states() {
        let node = applied(&button("Save"));
        assert_eq!(node.toggled(), None);
        assert_eq!(node.is_expanded(), None);
        assert_eq!(node.is_selected(), None);
        assert!(!node.is_read_only());
        assert_eq!(node.position_in_set(), None);
        assert_eq!(node.size_of_set(), None);
        assert_eq!(node.live(), None);
        assert_eq!(node.level(), None);
        assert_eq!(node.role_description(), None);
    }

    #[test]
    fn check_states_reach_the_node() {
        for (state, want) in [
            (Toggled::False, Toggled::False),
            (Toggled::True, Toggled::True),
            (Toggled::Mixed, Toggled::Mixed),
        ] {
            let data = NodeData {
                toggled: Some(state),
                ..NodeData::new(AkRole::CheckBox)
            };
            assert_eq!(applied(&data).toggled(), Some(want));
        }
    }

    #[test]
    fn an_expandable_node_offers_the_other_state() {
        let closed = NodeData {
            expanded: Some(false),
            ..NodeData::new(AkRole::ComboBox)
        };
        let node = applied(&closed);
        assert_eq!(node.is_expanded(), Some(false));
        assert!(node.supports_action(Action::Expand));
        assert!(!node.supports_action(Action::Collapse));
        let open = NodeData {
            expanded: Some(true),
            ..closed.clone()
        };
        let node = applied(&open);
        assert_eq!(node.is_expanded(), Some(true));
        assert!(node.supports_action(Action::Collapse));
        let off = NodeData {
            disabled: true,
            ..closed
        };
        let node = applied(&off);
        assert!(!node.supports_action(Action::Expand));
    }

    #[test]
    fn selection_read_only_set_level_and_description_reach_the_node() {
        let data = NodeData {
            selected: Some(false),
            read_only: true,
            position_in_set: Some(2),
            size_of_set: Some(40),
            level: Some(3),
            role_description: "dial".into(),
            ..NodeData::new(AkRole::ListBoxOption)
        };
        let node = applied(&data);
        assert_eq!(node.is_selected(), Some(false));
        assert!(node.is_read_only());
        assert_eq!(node.position_in_set(), Some(2));
        assert_eq!(node.size_of_set(), Some(40));
        assert_eq!(node.level(), Some(3));
        assert_eq!(node.role_description(), Some("dial"));
    }

    #[test]
    fn handled_actions_are_offered_unless_disabled() {
        let data = NodeData {
            handled: Handled {
                increase: true,
                scroll_down: true,
                show_on_screen: true,
                ..Handled::default()
            },
            ..NodeData::new(AkRole::Slider)
        };
        let node = applied(&data);
        assert!(node.supports_action(Action::Increment));
        assert!(!node.supports_action(Action::Decrement));
        assert!(node.supports_action(Action::ScrollDown));
        assert!(!node.supports_action(Action::ScrollUp));
        assert!(node.supports_action(Action::ScrollIntoView));
        let off = NodeData {
            disabled: true,
            ..data
        };
        assert!(!applied(&off).supports_action(Action::Increment));
    }

    #[test]
    fn a_label_relation_names_only_nodes_that_are_there() {
        let mut t = sample();
        t.upsert(
            C,
            NodeData {
                labelled_by: vec![B, D],
                ..button("C")
            },
        );
        // D is not in the tree: an id for it would point at nothing.
        let node = t.accesskit_node(C).unwrap();
        assert_eq!(node.labelled_by(), &[B.node_id()]);
    }

    #[test]
    fn an_announcement_is_a_live_node_at_the_end_of_the_scene() {
        let mut t = sample();
        let _ = t.take_changes();
        let said = t.announce("Saved", Politeness::Polite);
        assert!(said.is_announcement());
        assert!(!A.is_announcement());
        assert_eq!(t.children(NodeKey::ROOT), &[A, said]);
        assert_eq!(t.parent(said), Some(NodeKey::ROOT));
        assert_eq!(t.announcement(Politeness::Polite), Some("Saved"));
        assert_eq!(t.announcement(Politeness::Assertive), None);
        let data = t.get(said).unwrap();
        assert_eq!(data.role, AkRole::Status);
        let node = t.accesskit_node(said).unwrap();
        assert_eq!(node.live(), Some(Live::Polite));
        assert!(node.is_live_atomic());
        assert_eq!(node.label(), Some("Saved"));
        let changes = t.take_changes();
        assert_eq!(changes.changed, vec![NodeKey::ROOT, said]);
    }

    #[test]
    fn the_same_message_again_is_a_new_node() {
        let mut t = sample();
        let first = t.announce("Saved", Politeness::Polite);
        let _ = t.take_changes();
        let second = t.announce("Saved", Politeness::Polite);
        assert_ne!(first, second);
        assert!(!t.contains(first));
        assert_eq!(t.children(NodeKey::ROOT), &[A, second]);
        let changes = t.take_changes();
        assert_eq!(changes.removed, vec![first]);
        assert!(changes.changed.contains(&second));
    }

    #[test]
    fn an_alert_is_kept_beside_a_status_and_after_it() {
        let mut t = sample();
        let alert = t.announce("Connection lost", Politeness::Assertive);
        let status = t.announce("Saved", Politeness::Polite);
        assert_eq!(t.get(alert).unwrap().role, AkRole::Alert);
        assert_eq!(t.children(NodeKey::ROOT), &[A, status, alert]);
        assert_eq!(
            t.accesskit_node(alert).unwrap().live(),
            Some(Live::Assertive)
        );
    }

    #[test]
    fn the_scenes_new_child_list_keeps_what_was_announced() {
        let mut t = sample();
        let said = t.announce("Saved", Politeness::Polite);
        t.upsert(D, button("D"));
        t.set_children(NodeKey::ROOT, &[D, A]);
        assert_eq!(t.children(NodeKey::ROOT), &[D, A, said]);
        // Saying the same list again changes nothing.
        assert!(!t.set_children(NodeKey::ROOT, &[D, A]));
    }

    #[test]
    fn a_status_text_is_a_live_region_by_its_role() {
        for (role, ak, want) in [
            (Role::StatusBar, AkRole::Status, Live::Polite),
            (Role::Notification, AkRole::Status, Live::Polite),
            (Role::AlertMessage, AkRole::Alert, Live::Assertive),
        ] {
            let data = NodeData {
                qt_role: role,
                name: "Saved".into(),
                ..NodeData::new(ak)
            };
            let node = applied(&data);
            assert_eq!(node.live(), Some(want), "{role:?}");
            assert!(node.is_live_atomic());
        }
        // Said as the author has it, over what the role would.
        let data = NodeData {
            qt_role: Role::StatusBar,
            live: Some(Politeness::Assertive),
            ..NodeData::new(AkRole::Status)
        };
        assert_eq!(applied(&data).live(), Some(Live::Assertive));
        assert_eq!(Role::StaticText.live(), None);
    }

    #[test]
    fn a_new_name_of_a_live_node_is_a_change_of_that_node() {
        let mut t = Tree::new();
        let status = |name: &str| NodeData {
            qt_role: Role::StatusBar,
            name: name.into(),
            ..NodeData::new(AkRole::Status)
        };
        t.upsert(A, status("Saving"));
        t.set_children(NodeKey::ROOT, &[A]);
        let _ = t.take_changes();
        assert!(t.upsert(A, status("Saved")));
        assert_eq!(t.take_changes().changed, vec![A]);
        let node = t.accesskit_node(A).unwrap();
        assert_eq!(node.label(), Some("Saved"));
        assert_eq!(node.live(), Some(Live::Polite));
    }

    #[test]
    fn politeness_has_qts_numbers() {
        assert_eq!(Politeness::from_qt(0), Politeness::Polite);
        assert_eq!(Politeness::from_qt(1), Politeness::Assertive);
        assert_eq!(Politeness::from_qt(7), Politeness::Polite);
        assert_eq!(Politeness::Assertive.qt_value(), 1);
    }

    #[test]
    fn what_can_be_acted_on_is_actionable() {
        assert!(button("Save").actionable());
        assert!(!group().actionable());
        let checkable = NodeData {
            toggled: Some(Toggled::False),
            ..group()
        };
        assert!(checkable.actionable());
        let slider = NodeData {
            qt_role: Role::Slider,
            ..NodeData::new(AkRole::Slider)
        };
        assert!(slider.actionable());
    }
}
