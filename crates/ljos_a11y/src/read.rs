//! The tree read as one value, the way an assistive technology sees it:
//! what a test compares, and what a step finds a node in.

use accesskit::Toggled;

use crate::{NodeData, NodeKey, Politeness, Tree, role_word};

/// One node of the accessibility tree, as an assistive technology sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessibleNode {
    /// The node it was read from, which the scene maps to its item.
    /// `None` for the window, and where no [`Tree`] was read.
    pub key: Option<NodeKey>,
    pub role: String,
    pub name: String,
    pub description: String,
    /// The author's `objectName`, when set.
    pub object_name: String,
    /// Whether this is the one node the keyboard is at: what a screen
    /// reader reads out. The top node when nothing below it has it.
    pub focused: bool,
    /// What the node holds besides its name: the text of an editor.
    pub value: String,
    /// The part of `value` that is selected.
    pub selected: String,
    /// What the node says of itself: "checkable", "checked", "partially
    /// checked", "pressed", "expandable", "expanded", "selectable",
    /// "selected", "read-only", "disabled", for a live region "live" (and
    /// "assertive" when it interrupts), and "off screen" when it lies
    /// wholly outside the window.
    pub states: Vec<String>,
    /// Its place among the rows it is one of, counted from 1, and how
    /// many there are.
    pub set: Option<(usize, usize)>,
    /// The value of a slider-like node, then its least and its most, as
    /// numbers are written.
    pub range: Option<(String, String, String)>,
    /// The role and name of each node that labels this one.
    pub labelled_by: Vec<(String, String)>,
    /// What a screen reader calls the node when its role alone does not
    /// say ("dial" for a slider).
    pub called: String,
    pub children: Vec<AccessibleNode>,
}

impl AccessibleNode {
    /// `tree` as a window called `name` holding its top nodes, with the
    /// keyboard at `focus` (at the window when `None`).
    #[must_use]
    pub fn window(tree: &Tree, name: String, focus: Option<NodeKey>) -> Self {
        Self {
            role: "window".into(),
            name,
            focused: focus.is_none(),
            children: below(tree, NodeKey::ROOT, focus),
            ..Self::default()
        }
    }

    /// Depth-first search for `role` + `name`.
    #[must_use]
    pub fn find(&self, role: &str, name: &str) -> Option<&AccessibleNode> {
        if self.is(role, name) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(role, name))
    }

    #[must_use]
    pub fn is(&self, role: &str, name: &str) -> bool {
        self.role == role && self.name == name
    }

    /// The node the keyboard is at.
    #[must_use]
    pub fn focus(&self) -> Option<&AccessibleNode> {
        if self.focused {
            return Some(self);
        }
        self.children.iter().find_map(AccessibleNode::focus)
    }

    /// The node that has the first `role` + `name` as a child.
    #[must_use]
    pub fn parent_of(&self, role: &str, name: &str) -> Option<&AccessibleNode> {
        if self.children.iter().any(|c| c.is(role, name)) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.parent_of(role, name))
    }
}

fn below(tree: &Tree, key: NodeKey, focus: Option<NodeKey>) -> Vec<AccessibleNode> {
    let node = |k: &NodeKey| {
        let d = tree.get(*k)?;
        Some(AccessibleNode {
            key: Some(*k),
            role: role_word(d.role),
            name: d.name.clone(),
            description: d.description.clone(),
            object_name: d.author_id.clone(),
            focused: focus == Some(*k),
            value: d.text.as_ref().map(|t| t.value.clone()).unwrap_or_default(),
            selected: d
                .text
                .as_ref()
                .and_then(|t| {
                    let (a, b) = (t.anchor.min(t.cursor), t.anchor.max(t.cursor));
                    t.value.get(a..b).map(str::to_owned)
                })
                .unwrap_or_default(),
            states: states(d, tree.off_screen(*k)),
            set: d.position_in_set.zip(d.size_of_set),
            range: d
                .range
                .as_ref()
                .map(|r| (r.value.to_string(), r.min.to_string(), r.max.to_string())),
            labelled_by: d
                .labelled_by
                .iter()
                .filter_map(|l| tree.get(*l))
                .map(|l| (role_word(l.role), l.name.clone()))
                .collect(),
            called: d.role_description.clone(),
            children: below(tree, *k, focus),
        })
    };
    tree.children(key).iter().filter_map(node).collect()
}

fn states(d: &NodeData, off_screen: bool) -> Vec<String> {
    let says = [
        ("checkable", d.toggled.is_some()),
        ("checked", d.toggled == Some(Toggled::True)),
        ("partially checked", d.toggled == Some(Toggled::Mixed)),
        ("pressed", d.pressed),
        ("expandable", d.expanded.is_some()),
        ("expanded", d.expanded == Some(true)),
        ("selectable", d.selected.is_some()),
        ("selected", d.selected == Some(true)),
        ("read-only", d.read_only),
        ("disabled", d.disabled),
        // A live region: what it says is said again when it changes.
        ("live", d.politeness().is_some()),
        ("assertive", d.politeness() == Some(Politeness::Assertive)),
        // Wholly outside the window: not read as shown.
        ("off screen", off_screen),
    ];
    says.iter()
        .filter(|(_, is)| *is)
        .map(|(word, _)| (*word).to_owned())
        .collect()
}
