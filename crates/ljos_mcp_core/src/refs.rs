//! The short references (`e9`) an outline gives its nodes.
//!
//! A reference is given the first time a node is shown to the agent and
//! stays with that node. When the node is gone the reference is stale; it
//! is handed on to another node only when that node is the only one with
//! the role and name the first had, and the first was the only one too
//! (ARCHITECTURE.md §19.4), which is what a hot reload leaves behind.

use std::collections::{HashMap, VecDeque};

use crate::snapshot::SnapTree;
use crate::{ErrorCode, SurfaceError};

#[derive(Clone, Debug)]
struct Entry {
    id: u64,
    role: String,
    name: String,
    /// Whether no other node had this role and name when it was last shown.
    unique: bool,
}

/// The references one server has given out.
#[derive(Debug)]
pub struct RefTable {
    next: u64,
    by_id: HashMap<u64, u64>,
    entries: HashMap<u64, Entry>,
    /// Oldest first, for eviction.
    order: VecDeque<u64>,
    capacity: usize,
}

/// `e12` is 12.
fn parse(label: &str) -> Option<u64> {
    let digits = label.strip_prefix('e')?;
    if digits.is_empty() || digits.len() > 18 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

impl RefTable {
    /// A table that remembers the last `capacity` references it gave. An
    /// older one is stale: an agent that kept a reference through
    /// thousands of others is told to look again.
    pub fn new(capacity: usize) -> Self {
        RefTable {
            next: 0,
            by_id: HashMap::new(),
            entries: HashMap::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    /// Whether `text` is written like a reference.
    pub fn is_ref(text: &str) -> bool {
        parse(text).is_some()
    }

    /// The reference of the node at `index`, given now if it has none.
    pub fn assign(&mut self, tree: &SnapTree, index: usize) -> String {
        let Some(node) = tree.get(index) else {
            return String::from("e0");
        };
        let unique = !node.name.is_empty() && tree.count_named(&node.role, &node.name) == 1;
        if let Some(n) = self.by_id.get(&node.id).copied()
            && let Some(e) = self.entries.get_mut(&n)
        {
            if e.role != node.role {
                e.role.clone_from(&node.role);
            }
            if e.name != node.name {
                e.name.clone_from(&node.name);
            }
            e.unique = unique;
            return format!("e{n}");
        }
        self.next += 1;
        let n = self.next;
        self.by_id.insert(node.id, n);
        self.entries.insert(
            n,
            Entry {
                id: node.id,
                role: node.role.clone(),
                name: node.name.clone(),
                unique,
            },
        );
        self.order.push_back(n);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front()
                && let Some(e) = self.entries.remove(&old)
                && self.by_id.get(&e.id) == Some(&old)
            {
                self.by_id.remove(&e.id);
            }
        }
        format!("e{n}")
    }

    /// The reference a node already has.
    pub fn peek(&self, id: u64) -> Option<String> {
        self.by_id.get(&id).map(|n| format!("e{n}"))
    }

    /// The node `label` names in `tree`, or `stale_ref`.
    pub fn resolve(&mut self, label: &str, tree: &SnapTree) -> Result<usize, SurfaceError> {
        let stale = |why: String| SurfaceError::new(ErrorCode::StaleRef, why);
        let Some(n) = parse(label) else {
            return Err(SurfaceError::invalid_argument(format!(
                "`{label}` is not a reference: references look like `e12`"
            )));
        };
        let Some(entry) = self.entries.get(&n).cloned() else {
            return Err(stale(format!(
                "reference `{label}` is stale: this inspector did not give it, or gave it too \
                 long ago. Take a new `app_snapshot` and use the references in it"
            )));
        };
        if let Some(index) = tree.index_of(entry.id) {
            return Ok(index);
        }
        if entry.unique
            && let Some(index) = tree.only_named(&entry.role, &entry.name)
        {
            // The item was made anew (a reload does that) and nothing else
            // could be meant.
            let id = tree.nodes()[index].id;
            // A node keeps the reference it was shown with, if it has one.
            if !self.by_id.contains_key(&id) {
                self.by_id.remove(&entry.id);
                self.by_id.insert(id, n);
                if let Some(e) = self.entries.get_mut(&n) {
                    e.id = id;
                }
            }
            return Ok(index);
        }
        let what = if entry.name.is_empty() {
            entry.role.clone()
        } else {
            format!("{} \"{}\"", entry.role, entry.name.replace('"', "\\\""))
        };
        Err(stale(format!(
            "reference `{label}` is stale: the {what} it named is gone or no longer shown. \
             Take a new `app_snapshot` and use the references in it"
        ))
        .with_details(serde_json::json!({ "ref": label, "role": entry.role, "name": entry.name })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::SnapNode;

    fn tree(nodes: &[(u64, &str, &str)]) -> SnapTree {
        let mut t = SnapTree::new();
        let w = t.push(None, SnapNode::new(0, "window"));
        for (id, role, name) in nodes {
            let mut n = SnapNode::new(*id, *role);
            n.name = (*name).to_string();
            t.push(Some(w), n);
        }
        t
    }

    #[test]
    fn a_node_keeps_its_reference() {
        let t = tree(&[(1, "button", "Save"), (2, "button", "Open")]);
        let mut refs = RefTable::new(16);
        assert_eq!(refs.assign(&t, 2), "e1");
        assert_eq!(refs.assign(&t, 1), "e2");
        assert_eq!(refs.assign(&t, 2), "e1");
        assert_eq!(refs.peek(2).as_deref(), Some("e1"));
        assert_eq!(refs.peek(9), None);
        assert_eq!(refs.resolve("e2", &t).unwrap(), 1);
    }

    #[test]
    fn a_reference_nobody_gave_is_stale() {
        let t = tree(&[(1, "button", "Save")]);
        let mut refs = RefTable::new(16);
        let e = refs.resolve("e99", &t).unwrap_err();
        assert_eq!(e.code, ErrorCode::StaleRef);
        assert!(e.message.contains("app_snapshot"));
        assert_eq!(
            refs.resolve("button", &t).unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn a_gone_node_is_stale() {
        let before = tree(&[(1, "button", "Save"), (2, "button", "Open")]);
        let mut refs = RefTable::new(16);
        let r = refs.assign(&before, 1);
        let after = tree(&[(2, "button", "Open")]);
        let e = refs.resolve(&r, &after).unwrap_err();
        assert_eq!(e.code, ErrorCode::StaleRef);
        assert!(e.message.contains("button \"Save\""), "{}", e.message);
    }

    #[test]
    fn a_node_made_anew_is_found_by_its_role_and_name() {
        let before = tree(&[(1, "button", "Save")]);
        let mut refs = RefTable::new(16);
        let r = refs.assign(&before, 1);
        let after = tree(&[(41, "button", "Save")]);
        assert_eq!(refs.resolve(&r, &after).unwrap(), 1);
        // And is shown with the same reference from now on.
        assert_eq!(refs.assign(&after, 1), r);
    }

    #[test]
    fn a_reference_is_never_handed_to_one_of_two_alike() {
        // Two buttons called Delete: when one goes, its reference may not
        // come to mean the other.
        let before = tree(&[(1, "button", "Delete"), (2, "button", "Delete")]);
        let mut refs = RefTable::new(16);
        let r = refs.assign(&before, 1);
        let after = tree(&[(2, "button", "Delete")]);
        assert_eq!(
            refs.resolve(&r, &after).unwrap_err().code,
            ErrorCode::StaleRef
        );
        // Nor when there are two to choose from now.
        let before = tree(&[(1, "button", "Save")]);
        let r = refs.assign(&before, 1);
        let after = tree(&[(5, "button", "Save"), (6, "button", "Save")]);
        assert_eq!(
            refs.resolve(&r, &after).unwrap_err().code,
            ErrorCode::StaleRef
        );
        // Nor for a node without a name.
        let before = tree(&[(7, "button", "")]);
        let r = refs.assign(&before, 1);
        let after = tree(&[(8, "button", "")]);
        assert_eq!(
            refs.resolve(&r, &after).unwrap_err().code,
            ErrorCode::StaleRef
        );
    }

    #[test]
    fn old_references_are_forgotten() {
        let t = tree(&[(1, "a", "A"), (2, "b", "B"), (3, "c", "C")]);
        let mut refs = RefTable::new(2);
        let first = refs.assign(&t, 1);
        refs.assign(&t, 2);
        refs.assign(&t, 3);
        assert_eq!(
            refs.resolve(&first, &t).unwrap_err().code,
            ErrorCode::StaleRef
        );
        // The node gets a new one when it is shown again.
        assert_eq!(refs.assign(&t, 1), "e4");
    }
}
