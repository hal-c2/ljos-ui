//! The outline an agent reads in place of a picture (ARCHITECTURE.md
//! §19.4): one line per exposed item, indented by depth, with a short
//! reference the agent can hand back.
//!
//! A server copies what it shows into a [`SnapTree`], which is plain data,
//! and everything here works on that copy: the application may go on
//! changing while a long outline is written over several frames.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use crate::refs::RefTable;

/// The longest name or value a line carries (§19.6).
pub const MAX_NAME_CHARS: usize = 200;

/// What a node holds: the text of a field, the number of a slider.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeValue {
    Text(String),
    Number(f64),
    /// There is a value and the agent may not see it: a password, or any
    /// editable value when the server was started to mask them.
    Redacted,
}

/// One exposed item.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapNode {
    /// The server's own stable number for the item. The same item has the
    /// same number in the next snapshot.
    pub id: u64,
    /// Set by [`SnapTree::push`].
    pub parent: Option<usize>,
    /// Set by [`SnapTree::push`].
    pub depth: u32,
    /// The role word: `button`, `textinput`, `window`.
    pub role: String,
    /// The accessible name; empty when the item has none.
    pub name: String,
    pub value: Option<NodeValue>,
    pub qml_id: Option<String>,
    pub object_name: Option<String>,
    pub type_name: Option<String>,
    pub description: String,
    /// `x, y, width, height` in the window.
    pub bounds: [f64; 4],
    /// For a window: its size, written after the reference.
    pub size: Option<(u32, u32)>,
    /// `focused`, `disabled` and the rest, in the order they are written.
    pub states: Vec<&'static str>,
    /// `items=240`, `pos=13/240`: written as `key=value`.
    pub attrs: Vec<(&'static str, String)>,
    /// Whether the item can be asked to do something. One without a name
    /// is flagged.
    pub actionable: bool,
    /// How many items below this one are not shown, and so not listed.
    pub hidden_below: usize,
    /// How many nodes the subtree has, this one counted. Kept by
    /// [`SnapTree::push`].
    pub subtree: usize,
}

impl SnapNode {
    pub fn new(id: u64, role: impl Into<String>) -> Self {
        SnapNode {
            id,
            parent: None,
            depth: 0,
            role: role.into(),
            name: String::new(),
            value: None,
            qml_id: None,
            object_name: None,
            type_name: None,
            description: String::new(),
            bounds: [0.0; 4],
            size: None,
            states: Vec::new(),
            attrs: Vec::new(),
            actionable: false,
            hidden_below: 0,
            subtree: 1,
        }
    }

    pub fn has_state(&self, state: &str) -> bool {
        match state {
            "unnamed" => self.actionable && self.name.is_empty(),
            "actionable" => self.actionable,
            "enabled" => !self.states.contains(&"disabled"),
            _ => self.states.contains(&state),
        }
    }
}

/// The exposed items of one moment, parents before children.
#[derive(Debug, Default)]
pub struct SnapTree {
    nodes: Vec<SnapNode>,
    by_id: HashMap<u64, usize>,
    /// How many nodes have each role and name: built when first asked for.
    named: OnceLock<HashMap<u64, u32>>,
}

impl SnapTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node below `parent` (`None` for the first node). Nodes come
    /// in outline order: a node's children follow it before its next
    /// sibling does.
    ///
    /// # Panics
    /// When `parent` is not the node last added or one it is inside of,
    /// which is a bug in the server that copies its tree.
    pub fn push(&mut self, parent: Option<usize>, mut node: SnapNode) -> usize {
        let index = self.nodes.len();
        match parent {
            None => {
                assert!(index == 0, "only the first node has no parent");
                node.depth = 0;
            }
            Some(p) => {
                let last = index - 1;
                assert!(
                    p <= last && last < p + self.nodes[p].subtree,
                    "nodes are added in outline order"
                );
                node.depth = self.nodes[p].depth + 1;
                let mut up = Some(p);
                while let Some(a) = up {
                    self.nodes[a].subtree += 1;
                    up = self.nodes[a].parent;
                }
            }
        }
        node.parent = parent;
        node.subtree = 1;
        self.by_id.insert(node.id, index);
        self.nodes.push(node);
        self.named = OnceLock::new();
        index
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn nodes(&self) -> &[SnapNode] {
        &self.nodes
    }

    pub fn get(&self, index: usize) -> Option<&SnapNode> {
        self.nodes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut SnapNode> {
        self.named = OnceLock::new();
        self.nodes.get_mut(index)
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.by_id.get(&id).copied()
    }

    /// The nodes below `index`, in outline order.
    pub fn descendants(&self, index: usize) -> std::ops::Range<usize> {
        match self.nodes.get(index) {
            Some(n) => index + 1..index + n.subtree,
            None => 0..0,
        }
    }

    /// Where a node is, for an agent that did not ask for the outline:
    /// `window > toolbar "Tools"`. The node itself is not part of it.
    pub fn path(&self, index: usize) -> String {
        let mut parts = Vec::new();
        let mut up = self.nodes.get(index).and_then(|n| n.parent);
        while let Some(a) = up {
            parts.push(short_line(&self.nodes[a], None));
            up = self.nodes[a].parent;
        }
        parts.reverse();
        parts.join(" > ")
    }

    /// How many nodes have this role and this name.
    pub fn count_named(&self, role: &str, name: &str) -> u32 {
        let named = self.named.get_or_init(|| {
            let mut map = HashMap::with_capacity(self.nodes.len());
            for n in &self.nodes {
                *map.entry(name_hash(&n.role, &n.name)).or_insert(0) += 1;
            }
            map
        });
        named.get(&name_hash(role, name)).copied().unwrap_or(0)
    }

    /// The one node with this role and name; `None` when there is none or
    /// more than one.
    pub fn only_named(&self, role: &str, name: &str) -> Option<usize> {
        if self.count_named(role, name) != 1 {
            return None;
        }
        self.nodes
            .iter()
            .position(|n| n.role == role && n.name == name)
    }
}

fn name_hash(role: &str, name: &str) -> u64 {
    // A table of strings would cost an allocation per node of every
    // snapshot. Two pairs with one hash would only make a reference that
    // could have found its item again report stale instead.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    role.hash(&mut h);
    name.hash(&mut h);
    h.finish()
}

/// What a line shows beyond role, name, value, reference and state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineOptions {
    pub bounds: bool,
    pub types: bool,
    pub description: bool,
}

/// Text inside quotes: cut to [`MAX_NAME_CHARS`], and with nothing in it
/// that could end the quotes or the line. A name is whatever the
/// application shows, which may be text somebody wrote to look like an
/// outline.
fn quoted(out: &mut String, text: &str) {
    out.push('"');
    for (i, c) in text.chars().enumerate() {
        if i == MAX_NAME_CHARS {
            out.push('…');
            break;
        }
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// An `id` or `objectName`: bare when it is a plain word, quoted otherwise.
fn word(out: &mut String, text: &str) {
    let plain = !text.is_empty()
        && text.chars().count() <= MAX_NAME_CHARS
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/'));
    if plain {
        out.push_str(text);
    } else {
        quoted(out, text);
    }
}

/// A number as a person writes it: `3`, `0.25`.
pub fn number(n: f64) -> String {
    if n.is_finite() && n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else if n.is_finite() {
        let s = format!("{n:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        n.to_string()
    }
}

/// `role "name" [ref]`: how a node is named inside another line.
fn short_line(node: &SnapNode, reference: Option<&str>) -> String {
    let mut out = String::with_capacity(node.role.len() + node.name.len() + 12);
    out.push_str(&node.role);
    if !node.name.is_empty() {
        out.push(' ');
        quoted(&mut out, &node.name);
    }
    if let Some(r) = reference {
        let _ = write!(out, " [{r}]");
    }
    out
}

/// One node as the outline writes it, without indentation:
/// `button "Delete" [e9] obj=delete disabled`.
pub fn render_line(node: &SnapNode, reference: Option<&str>, opts: LineOptions) -> String {
    let mut out = String::with_capacity(48 + node.name.len());
    out.push_str(&node.role);
    if !node.name.is_empty() {
        out.push(' ');
        quoted(&mut out, &node.name);
    }
    match &node.value {
        Some(NodeValue::Text(t)) => {
            out.push_str(" value=");
            quoted(&mut out, t);
        }
        Some(NodeValue::Number(n)) => {
            let _ = write!(out, " value={}", number(*n));
        }
        Some(NodeValue::Redacted) => out.push_str(" value=<redacted>"),
        None => {}
    }
    if let Some(r) = reference {
        let _ = write!(out, " [{r}]");
    }
    if let Some((w, h)) = node.size {
        let _ = write!(out, " {w}x{h}");
    }
    if let Some(id) = &node.qml_id {
        out.push_str(" id=");
        word(&mut out, id);
    }
    if let Some(obj) = &node.object_name {
        out.push_str(" obj=");
        word(&mut out, obj);
    }
    for (key, value) in &node.attrs {
        let _ = write!(out, " {key}={value}");
    }
    if opts.types
        && let Some(ty) = &node.type_name
    {
        out.push_str(" type=");
        word(&mut out, ty);
    }
    if opts.bounds {
        let [x, y, w, h] = node.bounds;
        let _ = write!(
            out,
            " at={},{} size={}x{}",
            number(x),
            number(y),
            number(w),
            number(h)
        );
    }
    if opts.description && !node.description.is_empty() {
        out.push_str(" desc=");
        quoted(&mut out, &node.description);
    }
    for state in &node.states {
        out.push(' ');
        out.push_str(state);
    }
    if node.actionable && node.name.is_empty() {
        out.push_str(" ⚠unnamed");
    }
    out
}

fn count_line(out: &mut String, depth: u32, what: &str, n: usize) {
    indent(out, depth);
    let _ = writeln!(
        out,
        "- ({what}: {n} {})",
        if n == 1 { "node" } else { "nodes" }
    );
}

fn indent(out: &mut String, depth: u32) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// Which part of a tree an outline shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutlineOptions {
    /// The node the outline starts at.
    pub root: usize,
    /// How many levels below the root are listed. What is further down is
    /// counted.
    pub max_depth: Option<u32>,
    /// The outline is cut after this many nodes.
    pub max_nodes: usize,
    /// And before it is longer than this.
    pub max_bytes: usize,
    /// How many nodes earlier parts already listed.
    pub skip: usize,
    pub line: LineOptions,
}

impl OutlineOptions {
    pub fn whole(max_nodes: usize, max_bytes: usize) -> Self {
        OutlineOptions {
            root: 0,
            max_depth: None,
            max_nodes,
            max_bytes,
            skip: 0,
            line: LineOptions::default(),
        }
    }
}

/// A finished outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outline {
    pub text: String,
    /// How many nodes this part lists. Lines that count what is not listed
    /// and lines that repeat where a later part is are not nodes.
    pub nodes: usize,
    /// How many nodes all the parts list together.
    pub total: usize,
    pub truncated: bool,
    /// What to skip to get the next part.
    pub next_skip: Option<usize>,
}

/// Writes an outline a few nodes at a time, so that a server can spread a
/// long one over several frames.
#[derive(Debug)]
pub struct OutlineWriter {
    opts: OutlineOptions,
    text: String,
    nodes: usize,
    total: usize,
    /// The next node to write, and one past the last of the subtree.
    at: usize,
    end: usize,
    root_depth: u32,
    /// The nodes written whose subtrees are not finished.
    open: Vec<usize>,
    started: bool,
    cut: bool,
    done: bool,
}

impl OutlineWriter {
    pub fn new(tree: &SnapTree, opts: OutlineOptions) -> Self {
        let Some(root) = tree.get(opts.root) else {
            return OutlineWriter {
                opts,
                text: String::new(),
                nodes: 0,
                total: 0,
                at: 0,
                end: 0,
                root_depth: 0,
                open: Vec::new(),
                started: true,
                cut: false,
                done: true,
            };
        };
        let end = opts.root + root.subtree;
        let root_depth = root.depth;
        // Counting is index arithmetic, a few nanoseconds a node: it does
        // not need spreading over frames the way writing does.
        let (mut total, mut at, mut start) = (0, opts.root, end);
        while at < end {
            if total == opts.skip {
                start = at;
            }
            total += 1;
            let n = &tree.nodes[at];
            at += if opts.max_depth == Some(n.depth - root_depth) {
                n.subtree
            } else {
                1
            };
        }
        OutlineWriter {
            opts,
            text: String::new(),
            nodes: 0,
            total,
            at: start,
            end,
            root_depth,
            open: Vec::new(),
            started: false,
            cut: false,
            done: false,
        }
    }

    /// Writes up to `budget` more nodes. Returns whether the outline is
    /// finished.
    pub fn step(&mut self, tree: &SnapTree, refs: &mut RefTable, budget: usize) -> bool {
        let mut left = budget.max(1);
        while !self.done && left > 0 {
            if self.at >= self.end {
                while let Some(top) = self.open.pop() {
                    self.close(tree, top);
                }
                self.done = true;
                break;
            }
            if self.nodes >= self.opts.max_nodes.max(1) {
                self.cut = true;
                self.done = true;
                break;
            }
            let node = &tree.nodes[self.at];
            let rel = node.depth - self.root_depth;
            if !self.started {
                self.started = true;
                if self.opts.skip > 0 {
                    self.context(tree, refs);
                }
            }
            while let Some(&top) = self.open.last() {
                if tree.nodes[top].depth < node.depth {
                    break;
                }
                self.open.pop();
                self.close(tree, top);
            }
            let line = render_line(node, Some(&refs.assign(tree, self.at)), self.opts.line);
            let cost = rel as usize * 2 + 2 + line.len() + 1;
            if self.nodes > 0 && self.text.len() + cost > self.opts.max_bytes {
                self.cut = true;
                self.done = true;
                break;
            }
            indent(&mut self.text, rel);
            self.text.push_str("- ");
            self.text.push_str(&line);
            self.text.push('\n');
            self.nodes += 1;
            self.open.push(self.at);
            if self.opts.max_depth == Some(rel) {
                if node.subtree > 1 {
                    count_line(&mut self.text, rel + 1, "deeper", node.subtree - 1);
                }
                self.at += node.subtree;
            } else {
                self.at += 1;
            }
            left -= 1;
        }
        self.done
    }

    /// A later part starts with the lines its first node is inside of, so
    /// that the indentation means something.
    fn context(&mut self, tree: &SnapTree, refs: &mut RefTable) {
        let mut chain = Vec::new();
        let mut up = tree.nodes[self.at].parent;
        while let Some(a) = up {
            if a < self.opts.root {
                break;
            }
            chain.push(a);
            up = tree.nodes[a].parent;
        }
        chain.reverse();
        for a in chain {
            let node = &tree.nodes[a];
            indent(&mut self.text, node.depth - self.root_depth);
            let line = render_line(node, Some(&refs.assign(tree, a)), self.opts.line);
            let _ = writeln!(self.text, "- {line} (continued)");
            self.open.push(a);
        }
    }

    fn close(&mut self, tree: &SnapTree, index: usize) {
        let node = &tree.nodes[index];
        if node.hidden_below > 0 {
            count_line(
                &mut self.text,
                node.depth - self.root_depth + 1,
                "hidden",
                node.hidden_below,
            );
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn finish(mut self) -> Outline {
        if self.text.ends_with('\n') {
            self.text.pop();
        }
        let listed = self.opts.skip + self.nodes;
        let truncated = self.cut || (self.done && listed < self.total);
        Outline {
            text: self.text,
            nodes: self.nodes,
            total: self.total,
            truncated,
            next_skip: truncated.then_some(listed),
        }
    }
}

/// What changed between two snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delta {
    pub text: String,
    pub changed: usize,
    pub added: usize,
    pub removed: usize,
    pub truncated: bool,
}

/// The lines that differ between `old` (snapshot number `since`) and
/// `new`: `changed <line>`, `added <line> under <parent>` and
/// `removed <line>`. A node changed when its line did or when it has
/// another parent.
pub fn delta(
    old: &SnapTree,
    new: &SnapTree,
    since: u64,
    refs: &mut RefTable,
    max_lines: usize,
) -> Delta {
    let opts = LineOptions::default();
    let mut out = Delta {
        text: String::new(),
        changed: 0,
        added: 0,
        removed: 0,
        truncated: false,
    };
    let mut lines = 0usize;
    let mut more = 0usize;
    let parent_id = |t: &SnapTree, n: &SnapNode| n.parent.map(|p| t.nodes[p].id);
    for (i, node) in new.nodes.iter().enumerate() {
        let was = old.index_of(node.id).map(|o| &old.nodes[o]);
        let changed = match was {
            Some(was) => {
                render_line(was, None, opts) != render_line(node, None, opts)
                    || parent_id(old, was) != parent_id(new, node)
            }
            None => true,
        };
        if !changed {
            continue;
        }
        if was.is_some() {
            out.changed += 1;
        } else {
            out.added += 1;
        }
        if lines >= max_lines {
            more += 1;
            continue;
        }
        lines += 1;
        let line = render_line(node, Some(&refs.assign(new, i)), opts);
        match (was, node.parent) {
            (Some(_), _) => {
                let _ = writeln!(out.text, "changed {line}");
            }
            (None, Some(p)) => {
                let under = short_line(&new.nodes[p], Some(&refs.assign(new, p)));
                let _ = writeln!(out.text, "added {line} under {under}");
            }
            (None, None) => {
                let _ = writeln!(out.text, "added {line}");
            }
        }
    }
    for node in &old.nodes {
        if new.index_of(node.id).is_some() {
            continue;
        }
        out.removed += 1;
        if lines >= max_lines {
            more += 1;
            continue;
        }
        lines += 1;
        // A node the agent was never shown has no reference to tell it by.
        let reference = refs.peek(node.id);
        let _ = writeln!(
            out.text,
            "removed {}",
            render_line(node, reference.as_deref(), opts)
        );
    }
    if more > 0 {
        out.truncated = true;
        let _ = writeln!(
            out.text,
            "({more} more changes: take a whole snapshot to see them)"
        );
    }
    if lines == 0 {
        let _ = write!(out.text, "(no changes since snapshot {since})");
    }
    if out.text.ends_with('\n') {
        out.text.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, role: &str, name: &str) -> SnapNode {
        let mut n = SnapNode::new(id, role);
        n.name = name.to_string();
        n
    }

    /// window > (toolbar > (a, b), list > (r0..r4), button)
    fn tree() -> SnapTree {
        let mut t = SnapTree::new();
        let mut win = node(0, "window", "");
        win.size = Some((400, 300));
        let w = t.push(None, win);
        let mut bar = node(1, "toolbar", "Tools");
        bar.object_name = Some("tools".into());
        let bar = t.push(Some(w), bar);
        t.push(Some(bar), node(2, "button", "Undo"));
        let mut redo = node(3, "button", "Redo");
        redo.states = vec!["disabled"];
        t.push(Some(bar), redo);
        let mut list = node(4, "list", "Rows");
        list.hidden_below = 2;
        let list = t.push(Some(w), list);
        for i in 0..5 {
            t.push(Some(list), node(10 + i, "listitem", &format!("Row {i}")));
        }
        t.push(Some(w), node(5, "button", "Help"));
        t
    }

    fn outline(t: &SnapTree, refs: &mut RefTable, opts: OutlineOptions) -> Outline {
        let mut w = OutlineWriter::new(t, opts);
        // Three nodes at a time: the result may not depend on the slicing.
        while !w.step(t, refs, 3) {}
        w.finish()
    }

    #[test]
    fn the_whole_tree_is_written_with_references_in_order() {
        let t = tree();
        let mut refs = RefTable::new(64);
        let o = outline(&t, &mut refs, OutlineOptions::whole(400, 1 << 20));
        assert_eq!(
            o.text,
            "- window [e1] 400x300\n\
             \x20 - toolbar \"Tools\" [e2] obj=tools\n\
             \x20   - button \"Undo\" [e3]\n\
             \x20   - button \"Redo\" [e4] disabled\n\
             \x20 - list \"Rows\" [e5]\n\
             \x20   - listitem \"Row 0\" [e6]\n\
             \x20   - listitem \"Row 1\" [e7]\n\
             \x20   - listitem \"Row 2\" [e8]\n\
             \x20   - listitem \"Row 3\" [e9]\n\
             \x20   - listitem \"Row 4\" [e10]\n\
             \x20   - (hidden: 2 nodes)\n\
             \x20 - button \"Help\" [e11]"
        );
        assert_eq!((o.nodes, o.total, o.truncated), (11, 11, false));
        assert_eq!(o.next_skip, None);
    }

    #[test]
    fn slicing_does_not_change_the_outline() {
        let t = tree();
        let whole = outline(
            &t,
            &mut RefTable::new(64),
            OutlineOptions::whole(400, 1 << 20),
        );
        let mut refs = RefTable::new(64);
        let mut w = OutlineWriter::new(&t, OutlineOptions::whole(400, 1 << 20));
        let mut steps = 0;
        while !w.step(&t, &mut refs, 1) {
            steps += 1;
        }
        assert!(steps >= 11);
        assert_eq!(w.finish(), whole);
    }

    #[test]
    fn parts_join_up_and_say_where_they_are() {
        let t = tree();
        let mut refs = RefTable::new(64);
        let mut opts = OutlineOptions::whole(6, 1 << 20);
        let first = outline(&t, &mut refs, opts);
        assert_eq!((first.nodes, first.truncated), (6, true));
        assert_eq!(first.next_skip, Some(6));
        assert!(first.text.ends_with("- listitem \"Row 0\" [e6]"));
        assert!(!first.text.contains("hidden"));

        opts.skip = 6;
        let second = outline(&t, &mut refs, opts);
        assert_eq!(
            second.text,
            "- window [e1] 400x300 (continued)\n\
             \x20 - list \"Rows\" [e5] (continued)\n\
             \x20   - listitem \"Row 1\" [e7]\n\
             \x20   - listitem \"Row 2\" [e8]\n\
             \x20   - listitem \"Row 3\" [e9]\n\
             \x20   - listitem \"Row 4\" [e10]\n\
             \x20   - (hidden: 2 nodes)\n\
             \x20 - button \"Help\" [e11]"
        );
        assert_eq!(
            (second.nodes, second.total, second.truncated),
            (5, 11, false)
        );
    }

    #[test]
    fn depth_counts_what_is_further_down() {
        let t = tree();
        let mut refs = RefTable::new(64);
        let mut opts = OutlineOptions::whole(400, 1 << 20);
        opts.max_depth = Some(1);
        let o = outline(&t, &mut refs, opts);
        assert_eq!(
            o.text,
            "- window [e1] 400x300\n\
             \x20 - toolbar \"Tools\" [e2] obj=tools\n\
             \x20   - (deeper: 2 nodes)\n\
             \x20 - list \"Rows\" [e3]\n\
             \x20   - (deeper: 5 nodes)\n\
             \x20   - (hidden: 2 nodes)\n\
             \x20 - button \"Help\" [e4]"
        );
        assert_eq!((o.nodes, o.total), (4, 4));
    }

    #[test]
    fn a_subtree_starts_at_its_own_margin() {
        let t = tree();
        let mut refs = RefTable::new(64);
        let mut opts = OutlineOptions::whole(400, 1 << 20);
        opts.root = 1;
        let o = outline(&t, &mut refs, opts);
        assert_eq!(
            o.text,
            "- toolbar \"Tools\" [e1] obj=tools\n\
             \x20 - button \"Undo\" [e2]\n\
             \x20 - button \"Redo\" [e3] disabled"
        );
    }

    #[test]
    fn the_byte_budget_cuts_between_lines() {
        let t = tree();
        let mut refs = RefTable::new(64);
        let o = outline(&t, &mut refs, OutlineOptions::whole(400, 100));
        assert!(o.truncated);
        assert!(o.text.len() <= 100, "{}", o.text.len());
        assert_eq!(o.next_skip, Some(o.nodes));
        // Even a budget smaller than one line gives one node, or the agent
        // could never get past it.
        let o = outline(&t, &mut RefTable::new(64), OutlineOptions::whole(400, 1));
        assert_eq!(o.nodes, 1);
    }

    #[test]
    fn text_cannot_break_out_of_its_line() {
        let mut n = node(1, "button", "Save\n- button \"Delete\" [e9]\\");
        n.object_name = Some("a b\nc".into());
        let line = render_line(&n, Some("e2"), LineOptions::default());
        assert_eq!(
            line,
            "button \"Save\\n- button \\\"Delete\\\" [e9]\\\\\" [e2] obj=\"a b\\nc\""
        );
        assert!(!line.contains('\n'));
    }

    #[test]
    fn long_names_are_cut() {
        let n = node(1, "button", &"é".repeat(500));
        let line = render_line(&n, None, LineOptions::default());
        assert_eq!(line.chars().filter(|c| *c == 'é').count(), 200);
        assert!(line.ends_with("…\""));
    }

    #[test]
    fn values_states_and_flags_follow_the_name() {
        let mut n = node(1, "textinput", "Search");
        n.value = Some(NodeValue::Text("milk".into()));
        n.states = vec!["focused"];
        n.qml_id = Some("search".into());
        n.attrs = vec![("pos", "2/9".into())];
        n.bounds = [1.0, 2.5, 30.0, 4.0];
        n.type_name = Some("TextInput".into());
        n.description = "Finds notes".into();
        assert_eq!(
            render_line(&n, Some("e4"), LineOptions::default()),
            "textinput \"Search\" value=\"milk\" [e4] id=search pos=2/9 focused"
        );
        let all = LineOptions {
            bounds: true,
            types: true,
            description: true,
        };
        assert_eq!(
            render_line(&n, None, all),
            "textinput \"Search\" value=\"milk\" id=search pos=2/9 type=TextInput \
             at=1,2.5 size=30x4 desc=\"Finds notes\" focused"
        );
        n.value = Some(NodeValue::Redacted);
        assert!(render_line(&n, None, all).contains(" value=<redacted> "));
        n.value = Some(NodeValue::Number(0.5));
        assert!(render_line(&n, None, all).contains(" value=0.5 "));
        let mut b = node(2, "button", "");
        b.actionable = true;
        assert_eq!(
            render_line(&b, Some("e1"), LineOptions::default()),
            "button [e1] ⚠unnamed"
        );
    }

    #[test]
    fn a_delta_names_what_changed_came_and_went() {
        let old = tree();
        let mut refs = RefTable::new(64);
        outline(&old, &mut refs, OutlineOptions::whole(400, 1 << 20));

        let same = delta(&old, &tree(), 3, &mut refs, 100);
        assert_eq!(same.text, "(no changes since snapshot 3)");
        assert_eq!((same.changed, same.added, same.removed), (0, 0, 0));

        let mut new = SnapTree::new();
        let mut win = node(0, "window", "");
        win.size = Some((400, 300));
        let w = new.push(None, win);
        let mut bar = node(1, "toolbar", "Tools");
        bar.object_name = Some("tools".into());
        let bar = new.push(Some(w), bar);
        new.push(Some(bar), node(2, "button", "Undo all"));
        new.push(Some(bar), node(77, "button", "Cut"));
        new.push(Some(w), node(5, "button", "Help"));
        let d = delta(&old, &new, 1, &mut refs, 100);
        assert_eq!(
            d.text,
            "changed button \"Undo all\" [e3]\n\
             added button \"Cut\" [e12] under toolbar \"Tools\" [e2]\n\
             removed button \"Redo\" [e4] disabled\n\
             removed list \"Rows\" [e5]\n\
             removed listitem \"Row 0\" [e6]\n\
             removed listitem \"Row 1\" [e7]\n\
             removed listitem \"Row 2\" [e8]\n\
             removed listitem \"Row 3\" [e9]\n\
             removed listitem \"Row 4\" [e10]"
        );
        assert_eq!((d.changed, d.added, d.removed), (1, 1, 7));

        let cut = delta(&old, &new, 1, &mut refs, 2);
        assert!(cut.truncated);
        assert!(
            cut.text
                .ends_with("(7 more changes: take a whole snapshot to see them)")
        );
    }

    #[test]
    fn paths_and_counts() {
        let t = tree();
        assert_eq!(t.path(2), "window > toolbar \"Tools\"");
        assert_eq!(t.path(0), "");
        assert_eq!(t.descendants(1), 2..4);
        assert_eq!(t.count_named("button", "Help"), 1);
        assert_eq!(t.only_named("button", "Help"), Some(10));
        assert_eq!(t.only_named("button", "Nope"), None);
        assert_eq!(t.nodes()[0].subtree, 11);
    }

    #[test]
    fn numbers_are_written_plainly() {
        assert_eq!(number(3.0), "3");
        assert_eq!(number(0.25), "0.25");
        assert_eq!(number(-12.5), "-12.5");
        assert_eq!(number(1.0 / 3.0), "0.333");
    }
}
