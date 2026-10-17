//! A tree model that reads a folder's rows only when a view asks for
//! them, and has them a while later, as a file system or a remote listing
//! does. Folders are named uniquely; the top level is read from the start.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{Atom, Engine, ItemTree, TreeChange, TreeListener, TreeListeners, Value};

pub(super) struct LazyTree(Rc<Inner>);

struct Inner {
    /// The rows under each folder, by its name; "" is the top.
    listing: BTreeMap<String, Vec<String>>,
    /// The folders read so far, by path.
    read: RefCell<BTreeMap<Vec<usize>, Vec<String>>>,
    /// The folders a view asked for, read or not.
    asked: RefCell<BTreeSet<Vec<usize>>>,
    delay: Duration,
    listeners: TreeListeners,
}

impl LazyTree {
    /// `rows` are (folder, name) pairs; the folder "" is the top.
    pub(super) fn new(rows: &[(String, String)], delay: Duration) -> Self {
        let mut listing: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (folder, name) in rows {
            listing
                .entry(folder.clone())
                .or_default()
                .push(name.clone());
        }
        let top = listing.get("").cloned().unwrap_or_default();
        Self(Rc::new(Inner {
            listing,
            read: RefCell::new(BTreeMap::from([(Vec::new(), top)])),
            asked: RefCell::default(),
            delay,
            listeners: TreeListeners::default(),
        }))
    }
}

impl Inner {
    fn name_at(&self, path: &[usize]) -> Option<String> {
        let (last, parent) = path.split_last()?;
        self.read.borrow().get(parent)?.get(*last).cloned()
    }
}

impl ItemTree for LazyTree {
    fn row_count(&self, parent: &[usize]) -> usize {
        self.0.read.borrow().get(parent).map_or(0, Vec::len)
    }

    fn has_children(&self, path: &[usize]) -> bool {
        self.0
            .name_at(path)
            .is_some_and(|n| self.0.listing.contains_key(&n))
    }

    fn can_fetch_more(&self, path: &[usize]) -> bool {
        self.has_children(path) && !self.0.asked.borrow().contains(path)
    }

    fn fetch_more(&self, engine: &Engine, path: &[usize]) {
        let Some(name) = self.0.name_at(path) else {
            return;
        };
        self.0.asked.borrow_mut().insert(path.to_vec());
        let me = self.0.clone();
        let path = path.to_vec();
        engine.call_at(engine.now() + self.0.delay, move |engine| {
            let rows = me.listing.get(&name).cloned().unwrap_or_default();
            let count = rows.len();
            me.read.borrow_mut().insert(path.clone(), rows);
            me.listeners.emit(
                engine,
                &TreeChange::RowsInserted {
                    parent: path,
                    first: 0,
                    count,
                },
            );
        });
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::intern("display")]
    }

    fn data(&self, _: &Engine, path: &[usize], column: usize, role: &str) -> Value {
        match (column, role, self.0.name_at(path)) {
            (0, "display", Some(name)) => Value::from(name.as_str()),
            _ => Value::Undefined,
        }
    }

    fn subscribe(&self, listener: TreeListener) -> u64 {
        self.0.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.0.listeners.remove(id);
    }
}
