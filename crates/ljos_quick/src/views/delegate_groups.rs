//! The groups of a `DelegateModel`
//! (doc.qt.io/qt-6/qml-qtqml-models-delegatemodel.html,
//! doc.qt.io/qt-6/qml-qtqml-models-delegatemodelgroup.html).
//!
//! A `DelegateModel` is a proxy over its model: it keeps one entry per row
//! of that model (and one per datum inserted into a group, which no row of
//! the model stands for yet), in the order a view shows them, each with the
//! set of groups it belongs to. This type is that list. It is the model a
//! DelegateModel's rows read through, so a view sees only the entries of
//! the group the model is filtered on, and a change here is told to the
//! rows as the smallest row change that says the same thing, as a reset of
//! the model only when nothing smaller does.
//!
//! What is not here: the `in<Group>` and `<group>Index` attached properties
//! Qt makes up from each group's name (an attached type in Ljós has the
//! same properties for every DelegateModel, so they cannot be its to give;
//! `DelegateModel.groups` says membership), `get`, `create` and `resolve`
//! of a group, which hand out a row object or a delegate made outside a
//! view (the engine makes row objects for itself), and `rootIndex`, which
//! needs a model whose rows have children.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use ljos_engine::{
    Atom, Engine, ItemModel, MetaRef, ModelChange, ModelListener, ModelListeners, ObjectId,
    QmlObject, QmlType, RowKey, Value,
};
use rustc_hash::{FxHashMap, FxHashSet};

use super::models::{DelegateModel, DelegateModelAttached, DelegateModelGroup};
use crate::item::write_computed;

/// One range of a `DelegateModelGroup.changed` signal: where it starts in
/// the group and how many entries it holds. Qt hands the handler a script
/// object with these two members; there is no type of the name.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "DelegateModelGroupChange", uncreatable)]
pub struct GroupChange {
    #[qml(notify, readonly)]
    pub index: i32,
    #[qml(notify, readonly)]
    pub count: i32,
}

#[derive(Clone, Debug)]
struct Def {
    object: Option<ObjectId>,
    name: String,
    include_by_default: bool,
}

#[derive(Clone, Debug)]
enum Src {
    /// A row of the model under the DelegateModel, by its key there.
    Model(RowKey),
    /// Data inserted into a group, not bound to a row of the model.
    Synthetic(Vec<(String, Value)>),
}

#[derive(Clone, Debug)]
struct Entry {
    key: u64,
    src: Src,
    groups: FxHashSet<String>,
}

#[derive(Default)]
struct Inner {
    dm: Option<ObjectId>,
    underlying: Option<Rc<dyn ItemModel>>,
    subscription: u64,
    defs: Vec<Def>,
    entries: Vec<Entry>,
    /// What `filterOnGroup` holds; the group views are filtered on is
    /// this one when there is such a group, and `items` when there is not
    /// (both read on qml6).
    filter: String,
    next_key: u64,
    /// The keys of the underlying model's rows at the last change that
    /// moved rows in it, which a move is read against.
    last_keys: Vec<RowKey>,
    /// The rows a removal is about, while the model still has them.
    pending_removed: Option<Vec<RowKey>>,
    model_map: Option<FxHashMap<RowKey, usize>>,
    cached_count: Option<usize>,
    extra_roles: Vec<Atom>,
}

impl Inner {
    fn effective_filter(&self) -> &str {
        if self.defs.iter().any(|d| d.name == self.filter) {
            &self.filter
        } else {
            "items"
        }
    }

    fn in_filter(&self, e: &Entry) -> bool {
        // The name is small and the groups are few: comparing the name the
        // view is filtered on is the whole test.
        let f = self.effective_filter();
        e.groups.contains(f)
    }

    fn filtered_keys(&self) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|e| self.in_filter(e))
            .map(|e| e.key)
            .collect()
    }

    fn group_keys(&self, name: &str) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|e| e.groups.contains(name))
            .map(|e| e.key)
            .collect()
    }

    fn filtered_count(&mut self) -> usize {
        if let Some(c) = self.cached_count {
            return c;
        }
        let c = self.entries.iter().filter(|e| self.in_filter(e)).count();
        self.cached_count = Some(c);
        c
    }

    fn entry_at_filtered(&self, row: usize) -> Option<&Entry> {
        self.entries.iter().filter(|e| self.in_filter(e)).nth(row)
    }

    fn default_groups(&self) -> FxHashSet<String> {
        self.defs
            .iter()
            .filter(|d| d.include_by_default)
            .map(|d| d.name.clone())
            .collect()
    }

    fn known(&self, names: &[String]) -> Vec<String> {
        names
            .iter()
            .filter(|n| self.defs.iter().any(|d| &d.name == *n))
            .cloned()
            .collect()
    }

    fn merge_pending_roles(&mut self, pending: &mut Vec<Atom>) {
        self.extra_roles.append(pending);
    }
}

/// The entries of one `DelegateModel` and the groups among them.
pub(crate) struct GroupState {
    inner: RefCell<Inner>,
    listeners: Rc<ModelListeners>,
    /// Set while the attached objects of its delegates are being told, so
    /// that a write of this telling is not read as the author changing the
    /// item's groups back.
    telling: Cell<bool>,
}

impl std::fmt::Debug for GroupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupState")
            .field("entries", &self.inner.borrow().entries.len())
            .finish()
    }
}

impl Default for GroupState {
    fn default() -> Self {
        Self {
            inner: RefCell::new(Inner {
                filter: "items".to_owned(),
                ..Inner::default()
            }),
            listeners: Rc::default(),
            telling: Cell::new(false),
        }
    }
}

impl Drop for GroupState {
    fn drop(&mut self) {
        let inner = self.inner.get_mut();
        if let Some(m) = inner.underlying.take() {
            m.unsubscribe(inner.subscription);
        }
    }
}

fn scan_keys(model: &dyn ItemModel) -> Vec<RowKey> {
    (0..model.row_count()).map(|r| model.row_key(r)).collect()
}

/// A whole-number argument, truncated the way the model types truncate
/// theirs, or `None` for anything that is not a finite number.
pub(crate) fn index_of(v: &Value) -> Option<i64> {
    let n = v.as_number()?;
    n.is_finite().then(|| n.trunc() as i64)
}

pub(crate) fn count_of(v: &Value) -> Option<usize> {
    match v {
        Value::Undefined => Some(1),
        v => usize::try_from(index_of(v)?.max(0)).ok(),
    }
}

/// The strings of a list argument: a list of them, or one string on its
/// own, which the group methods also take.
pub(crate) fn strings_of(e: &Engine, v: &Value) -> Vec<String> {
    match v {
        Value::List(items) => items.iter().map(|i| i.to_js_string().to_string()).collect(),
        Value::String(s) => vec![s.to_string()],
        Value::Js(_) => match e.js_member(v, "length").and_then(|l| index_of(&l)) {
            Some(n) if n >= 0 => (0..n)
                .filter_map(|i| e.js_member(v, &i.to_string()))
                .map(|i| i.to_js_string().to_string())
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// What was in the filtered group and in every group before a change, for
/// saying afterwards what the change was.
struct Before {
    filtered: Vec<u64>,
    groups: Vec<(String, Option<ObjectId>, Vec<u64>)>,
}

/// One range of entries: where it starts in a group and how many it
/// holds.
type Ranges = Vec<(i32, i32)>;

/// The ranges a sequence lost and gained between `before` and `after`, as
/// a group's `changed` signal tells them: the entries taken out, at the
/// index each range had once the ranges before it are gone, and then the
/// entries put in, at the index each has when it is there.
fn ranges(before: &[u64], after: &[u64]) -> (Ranges, Ranges) {
    let after_set: FxHashSet<u64> = after.iter().copied().collect();
    let before_set: FxHashSet<u64> = before.iter().copied().collect();
    let runs = |seq: &[u64], keep: &FxHashSet<u64>| -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for (i, k) in seq.iter().enumerate() {
            if keep.contains(k) {
                continue;
            }
            match out.last_mut() {
                Some((start, n)) if *start + *n == i => *n += 1,
                _ => out.push((i, 1)),
            }
        }
        out
    };
    let mut removed = Vec::new();
    let mut gone = 0usize;
    for (start, n) in runs(before, &after_set) {
        removed.push(((start - gone) as i32, n as i32));
        gone += n;
    }
    let inserted = runs(after, &before_set)
        .into_iter()
        .map(|(start, n)| (start as i32, n as i32))
        .collect();
    (removed, inserted)
}

/// The row change of the filtered model that says what turned `before`
/// into `after`, or `None` when they are the same: one block in, one block
/// out, or one block moved; a reset for anything else.
fn filtered_change(before: &[u64], after: &[u64]) -> Option<ModelChange> {
    if before == after {
        return None;
    }
    let prefix = before
        .iter()
        .zip(after.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut suffix = 0;
    while suffix < before.len() - prefix.min(before.len())
        && suffix < after.len() - prefix
        && before[before.len() - 1 - suffix] == after[after.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let bmid = &before[prefix..before.len() - suffix];
    let amid = &after[prefix..after.len() - suffix];
    if bmid.is_empty() {
        return Some(ModelChange::RowsInserted {
            first: prefix,
            count: amid.len(),
        });
    }
    if amid.is_empty() {
        return Some(ModelChange::RowsRemoved {
            first: prefix,
            count: bmid.len(),
        });
    }
    if bmid.len() == amid.len() {
        // A block of the old order is somewhere else in the new one.
        for k in 1..bmid.len() {
            let turned: Vec<u64> = bmid[k..].iter().chain(bmid[..k].iter()).copied().collect();
            if turned == amid {
                return Some(ModelChange::RowsMoved {
                    from: prefix,
                    to: prefix + bmid.len() - k,
                    count: k,
                });
            }
            let turned: Vec<u64> = bmid[bmid.len() - k..]
                .iter()
                .chain(bmid[..bmid.len() - k].iter())
                .copied()
                .collect();
            if turned == amid {
                return Some(ModelChange::RowsMoved {
                    from: prefix + bmid.len() - k,
                    to: prefix,
                    count: k,
                });
            }
        }
    }
    Some(ModelChange::ModelReset)
}

impl GroupState {
    pub(crate) fn bind_dm(&self, dm: ObjectId) {
        self.inner.borrow_mut().dm = Some(dm);
    }

    fn snapshot(&self) -> Before {
        let inner = self.inner.borrow();
        Before {
            filtered: inner.filtered_keys(),
            groups: inner
                .defs
                .iter()
                .map(|d| (d.name.clone(), d.object, inner.group_keys(&d.name)))
                .collect(),
        }
    }

    /// Runs `f` on the entries, then tells everyone what it changed: the
    /// count of each group, the delegates, the views that follow the
    /// filtered model, and each group whose members are other than they
    /// were.
    fn mutate(self: &Rc<Self>, e: &Engine, f: impl FnOnce(&mut Inner)) {
        let before = self.snapshot();
        {
            let mut inner = self.inner.borrow_mut();
            f(&mut inner);
            inner.cached_count = None;
        }
        let after = self.snapshot();
        self.sync_counts(e);
        if before.filtered != after.filtered
            && let Some(change) = filtered_change(&before.filtered, &after.filtered)
        {
            self.listeners.emit(e, &change);
        }
        retell_all(e, self);
        let old: FxHashMap<&str, &Vec<u64>> = before
            .groups
            .iter()
            .map(|(n, _, k)| (n.as_str(), k))
            .collect();
        for (name, object, keys) in &after.groups {
            let Some(object) = object else { continue };
            let was: &[u64] = old.get(name.as_str()).map_or(&[], |k| k.as_slice());
            if was == keys.as_slice() || !e.is_alive(*object) {
                continue;
            }
            let (removed, inserted) = ranges(was, keys);
            if removed.is_empty() && inserted.is_empty() {
                continue;
            }
            let list = |rs: &[(i32, i32)]| {
                Value::list(
                    rs.iter()
                        .filter_map(|(index, count)| {
                            e.create_native_incomplete(
                                GroupChange {
                                    index: *index,
                                    count: *count,
                                },
                                Some(*object),
                            )
                            .ok()
                            .map(Value::Object)
                        })
                        .collect(),
                )
            };
            let _ = e.emit_by_name(*object, "changed", &[list(&removed), list(&inserted)]);
        }
        destroy_unpersisted(e, self);
    }

    fn sync_counts(&self, e: &Engine) {
        let counts: Vec<(ObjectId, i32)> = {
            let inner = self.inner.borrow();
            inner
                .defs
                .iter()
                .filter_map(|d| {
                    d.object
                        .map(|o| (o, inner.group_keys(&d.name).len() as i32))
                })
                .collect()
        };
        for (o, n) in counts {
            if e.is_alive(o) {
                let _ = e.set_internal(o, DelegateModelGroup::COUNT, n);
            }
        }
    }

    /// Takes the group definitions a `DelegateModel` holds, and what it is
    /// filtered on. A group that is new here takes the entries there are
    /// when it takes new ones; one that is gone takes its name off them.
    pub(crate) fn apply_defs_and_filter(
        self: &Rc<Self>,
        e: &Engine,
        defs: Vec<(Option<ObjectId>, String, bool)>,
        filter: String,
    ) {
        self.mutate(e, move |inner| {
            let was: FxHashMap<Option<ObjectId>, Def> =
                inner.defs.drain(..).map(|d| (d.object, d)).collect();
            let mut added: Vec<String> = Vec::new();
            let mut kept: FxHashSet<String> = FxHashSet::default();
            for (object, name, include) in defs {
                if let Some(old) = was.get(&object) {
                    if old.name != name {
                        for entry in &mut inner.entries {
                            if entry.groups.remove(&old.name) {
                                entry.groups.insert(name.clone());
                            }
                        }
                    }
                } else if include {
                    added.push(name.clone());
                }
                kept.insert(name.clone());
                inner.defs.push(Def {
                    object,
                    name,
                    include_by_default: include,
                });
            }
            for entry in &mut inner.entries {
                entry.groups.retain(|g| kept.contains(g));
                for name in &added {
                    entry.groups.insert(name.clone());
                }
            }
            inner.filter = filter;
        });
    }

    /// Follows a new model: its rows are the entries, in its order, in the
    /// groups an entry of the row was in before when the row is still
    /// there, and in the ones that take new items for the rows that are
    /// new. Data inserted into a group stays after them.
    pub(crate) fn set_underlying(self: &Rc<Self>, e: &Engine, model: Option<Rc<dyn ItemModel>>) {
        let weak: Weak<Self> = Rc::downgrade(self);
        let subscription = model.as_ref().map_or(0, |m| {
            let weak = weak.clone();
            m.subscribe(Rc::new(move |e: &Engine, c: &ModelChange| {
                if let Some(s) = weak.upgrade() {
                    s.on_underlying_change(e, c);
                }
            }))
        });
        let new = model.clone();
        let old = {
            let mut inner = self.inner.borrow_mut();
            let old = (inner.underlying.take(), inner.subscription);
            inner.underlying = model;
            inner.subscription = subscription;
            old
        };
        if let (Some(m), id) = old {
            m.unsubscribe(id);
        }
        self.mutate(e, move |inner| {
            let synthetic: Vec<Entry> = inner
                .entries
                .drain(..)
                .filter(|en| matches!(en.src, Src::Synthetic(_)))
                .collect();
            inner.entries.clear();
            inner.model_map = None;
            inner.pending_removed = None;
            inner.last_keys.clear();
            if let Some(m) = &new {
                let keys = scan_keys(m.as_ref());
                let defaults = inner.default_groups();
                for k in &keys {
                    inner.next_key += 1;
                    inner.entries.push(Entry {
                        key: inner.next_key,
                        src: Src::Model(*k),
                        groups: defaults.clone(),
                    });
                }
                inner.last_keys = keys;
            }
            inner.entries.extend(synthetic);
        });
        // What the views had is gone either way.
        self.listeners.emit(e, &ModelChange::ModelReset);
    }

    fn on_underlying_change(self: &Rc<Self>, e: &Engine, change: &ModelChange) {
        match change {
            ModelChange::RowsAboutToBeInserted { .. } => {}
            ModelChange::RowsAboutToBeRemoved { first, count } => {
                let mut inner = self.inner.borrow_mut();
                let keys: Vec<RowKey> = match inner.underlying.clone() {
                    Some(m) if inner.last_keys.len() == m.row_count() => inner
                        .last_keys
                        .iter()
                        .skip(*first)
                        .take(*count)
                        .copied()
                        .collect(),
                    Some(m) => (*first..first + count)
                        .filter(|r| *r < m.row_count())
                        .map(|r| m.row_key(r))
                        .collect(),
                    None => Vec::new(),
                };
                inner.pending_removed = Some(keys);
            }
            ModelChange::RowsInserted { first, count } => {
                let (first, count) = (*first, *count);
                self.mutate(e, move |inner| {
                    inner.model_map = None;
                    let Some(m) = inner.underlying.clone() else {
                        return;
                    };
                    let keys = scan_keys(m.as_ref());
                    let have: FxHashSet<RowKey> = inner
                        .entries
                        .iter()
                        .filter_map(|en| match en.src {
                            Src::Model(k) => Some(k),
                            Src::Synthetic(_) => None,
                        })
                        .collect();
                    let fresh: Vec<RowKey> = keys
                        .iter()
                        .skip(first)
                        .take(count)
                        .filter(|k| !have.contains(k))
                        .copied()
                        .collect();
                    if !fresh.is_empty() {
                        let at = insert_place(inner, &keys, first);
                        let defaults = inner.default_groups();
                        let mut made = Vec::new();
                        for k in fresh {
                            inner.next_key += 1;
                            made.push(Entry {
                                key: inner.next_key,
                                src: Src::Model(k),
                                groups: defaults.clone(),
                            });
                        }
                        inner.entries.splice(at..at, made);
                    }
                    inner.last_keys = keys;
                });
            }
            ModelChange::RowsRemoved { first, count } => {
                let (first, count) = (*first, *count);
                self.mutate(e, move |inner| {
                    inner.model_map = None;
                    let gone: FxHashSet<RowKey> = match inner.pending_removed.take() {
                        Some(keys) if !keys.is_empty() => keys.into_iter().collect(),
                        _ => inner
                            .last_keys
                            .iter()
                            .skip(first)
                            .take(count)
                            .copied()
                            .collect(),
                    };
                    inner
                        .entries
                        .retain(|en| !matches!(en.src, Src::Model(k) if gone.contains(&k)));
                    if let Some(m) = inner.underlying.clone() {
                        inner.last_keys = scan_keys(m.as_ref());
                    }
                });
            }
            ModelChange::RowsMoved { from, to, count } => {
                let (from, to, count) = (*from, *to, *count);
                self.mutate(e, move |inner| {
                    inner.model_map = None;
                    let moved: Vec<RowKey> = inner
                        .last_keys
                        .iter()
                        .skip(from)
                        .take(count)
                        .copied()
                        .collect();
                    if moved.is_empty() {
                        return;
                    }
                    let mut block: Vec<Entry> = Vec::new();
                    for k in &moved {
                        if let Some(i) = inner
                            .entries
                            .iter()
                            .position(|en| matches!(en.src, Src::Model(m) if m == *k))
                        {
                            block.push(inner.entries.remove(i));
                        }
                    }
                    let Some(m) = inner.underlying.clone() else {
                        inner.entries.extend(block);
                        return;
                    };
                    let keys = scan_keys(m.as_ref());
                    let start = moved
                        .first()
                        .and_then(|k| keys.iter().position(|x| x == k))
                        .unwrap_or(to);
                    let at = insert_place(inner, &keys, start);
                    inner.entries.splice(at..at, block);
                    inner.last_keys = keys;
                });
            }
            ModelChange::DataChanged {
                first,
                count,
                roles,
            } => {
                let range = {
                    let inner = self.inner.borrow();
                    let Some(m) = &inner.underlying else { return };
                    let changed: FxHashSet<RowKey> = (*first..first + count)
                        .filter(|r| *r < m.row_count())
                        .map(|r| m.row_key(r))
                        .collect();
                    let mut pos: Vec<usize> = Vec::new();
                    for (i, en) in inner
                        .entries
                        .iter()
                        .filter(|en| inner.in_filter(en))
                        .enumerate()
                    {
                        if matches!(en.src, Src::Model(k) if changed.contains(&k)) {
                            pos.push(i);
                        }
                    }
                    match (pos.first(), pos.last()) {
                        (Some(a), Some(b)) => Some((*a, b - a + 1)),
                        _ => None,
                    }
                };
                if let Some((first, count)) = range {
                    self.listeners.emit(
                        e,
                        &ModelChange::DataChanged {
                            first,
                            count,
                            roles: roles.clone(),
                        },
                    );
                    retell_all(e, self);
                }
            }
            ModelChange::RolesChanged => self.listeners.emit(e, &ModelChange::RolesChanged),
            ModelChange::ModelReset => {
                let model = self.inner.borrow().underlying.clone();
                self.mutate(e, move |inner| {
                    inner.model_map = None;
                    inner.pending_removed = None;
                    let was: FxHashMap<RowKey, FxHashSet<String>> = inner
                        .entries
                        .iter()
                        .filter_map(|en| match &en.src {
                            Src::Model(k) => Some((*k, en.groups.clone())),
                            Src::Synthetic(_) => None,
                        })
                        .collect();
                    let synthetic: Vec<Entry> = inner
                        .entries
                        .drain(..)
                        .filter(|en| matches!(en.src, Src::Synthetic(_)))
                        .collect();
                    if let Some(m) = &model {
                        let keys = scan_keys(m.as_ref());
                        let defaults = inner.default_groups();
                        for k in &keys {
                            inner.next_key += 1;
                            inner.entries.push(Entry {
                                key: inner.next_key,
                                src: Src::Model(*k),
                                groups: was.get(k).cloned().unwrap_or_else(|| defaults.clone()),
                            });
                        }
                        inner.last_keys = keys;
                    } else {
                        inner.last_keys.clear();
                    }
                    inner.entries.extend(synthetic);
                });
                self.listeners.emit(e, &ModelChange::ModelReset);
            }
        }
    }
}

/// Where in `entries` a block that starts at row `start` of the model (as
/// `keys` has it now) goes: after the entry of the row before it, or
/// before the entry of the first row after the block that has one.
impl GroupState {
    /// The group `object` is the definition of, by the object itself so
    /// that a group renamed since it was defined is still the same group.
    pub(crate) fn name_of(&self, object: ObjectId) -> Option<String> {
        self.inner
            .borrow()
            .defs
            .iter()
            .find(|d| d.object == Some(object))
            .map(|d| d.name.clone())
    }

    /// The entry keys of `count` members of group `name` from member
    /// `index`, or nothing when that is not a range of the group.
    fn targets(&self, name: &str, index: i64, count: usize) -> Option<Vec<u64>> {
        let inner = self.inner.borrow();
        let keys = inner.group_keys(name);
        let first = usize::try_from(index).ok()?;
        let end = first.checked_add(count)?;
        (end <= keys.len()).then(|| keys[first..end].to_vec())
    }

    /// `addGroups`, `removeGroups` and `setGroups` of a group, and writing
    /// an item's groups: `by` says what the named groups do to the
    /// membership of the entries of `keys`.
    pub(crate) fn membership(
        self: &Rc<Self>,
        e: &Engine,
        keys: Vec<u64>,
        names: Vec<String>,
        by: Membership,
    ) {
        if keys.is_empty() {
            return;
        }
        self.mutate(e, move |inner| {
            let names = inner.known(&names);
            for entry in inner.entries.iter_mut().filter(|en| keys.contains(&en.key)) {
                match by {
                    Membership::Add => entry.groups.extend(names.iter().cloned()),
                    Membership::Remove => entry.groups.retain(|g| !names.contains(g)),
                    Membership::Set => entry.groups = names.iter().cloned().collect(),
                }
            }
        });
    }

    pub(crate) fn add_groups(
        self: &Rc<Self>,
        e: &Engine,
        group: &str,
        index: i64,
        count: usize,
        names: Vec<String>,
    ) {
        if let Some(keys) = self.targets(group, index, count) {
            self.membership(e, keys, names, Membership::Add);
        }
    }

    pub(crate) fn remove_groups(
        self: &Rc<Self>,
        e: &Engine,
        group: &str,
        index: i64,
        count: usize,
        names: Vec<String>,
    ) {
        if let Some(keys) = self.targets(group, index, count) {
            self.membership(e, keys, names, Membership::Remove);
        }
    }

    pub(crate) fn set_groups(
        self: &Rc<Self>,
        e: &Engine,
        group: &str,
        index: i64,
        count: usize,
        names: Vec<String>,
    ) {
        if let Some(keys) = self.targets(group, index, count) {
            self.membership(e, keys, names, Membership::Set);
        }
    }

    /// `remove` of a group: the entries stay, in every other group they
    /// are in.
    pub(crate) fn remove_from_group(
        self: &Rc<Self>,
        e: &Engine,
        group: &str,
        index: i64,
        count: usize,
    ) {
        if let Some(keys) = self.targets(group, index, count) {
            self.membership(e, keys, vec![group.to_owned()], Membership::Remove);
        }
    }

    /// `move` of a group: the entries keep their place among themselves
    /// and take a new one in the group's order, which is the order of the
    /// entries as a whole as far as the group's members go.
    pub(crate) fn move_in_group(
        self: &Rc<Self>,
        e: &Engine,
        group: &str,
        from: i64,
        to: i64,
        count: usize,
    ) {
        let Some(keys) = self.targets(group, from, count) else {
            return;
        };
        let Ok(to) = usize::try_from(to) else { return };
        let group = group.to_owned();
        self.mutate(e, move |inner| {
            let mut block: Vec<Entry> = Vec::new();
            for k in &keys {
                if let Some(i) = inner.entries.iter().position(|en| en.key == *k) {
                    block.push(inner.entries.remove(i));
                }
            }
            let rest = inner.group_keys(&group);
            let at = if to < rest.len() {
                inner
                    .entries
                    .iter()
                    .position(|en| en.key == rest[to])
                    .unwrap_or(inner.entries.len())
            } else if let Some(last) = rest.last() {
                inner
                    .entries
                    .iter()
                    .position(|en| en.key == *last)
                    .map_or(inner.entries.len(), |i| i + 1)
            } else {
                inner.entries.len()
            };
            inner.entries.splice(at..at, block);
        });
    }

    /// `insert` of a group: an entry that is only this data until it is
    /// resolved to a row of the model. Its groups are the ones it was
    /// given, or the group `insert` was called on.
    pub(crate) fn insert_entry(
        self: &Rc<Self>,
        e: &Engine,
        group: &str,
        index: Option<i64>,
        data: Vec<(String, Value)>,
        names: Option<Vec<String>>,
    ) {
        let fresh_roles: Vec<Atom> = {
            let inner = self.inner.borrow();
            let known: FxHashSet<String> = inner
                .underlying
                .as_ref()
                .map(|m| m.role_names().iter().map(ToString::to_string).collect())
                .unwrap_or_default();
            data.iter()
                .map(|(n, _)| Atom::from(n.as_str()))
                .filter(|n| {
                    !known.contains(n.as_str()) && !inner.extra_roles.iter().any(|r| r == n)
                })
                .collect()
        };
        if !fresh_roles.is_empty() {
            self.inner
                .borrow_mut()
                .merge_pending_roles(&mut fresh_roles.clone());
            self.listeners.emit(e, &ModelChange::RolesChanged);
        }
        let group = group.to_owned();
        self.mutate(e, move |inner| {
            let members = inner.group_keys(&group);
            let at_member = index.map_or(members.len(), |i| {
                usize::try_from(i).unwrap_or(usize::MAX).min(members.len())
            });
            let at = if at_member < members.len() {
                inner
                    .entries
                    .iter()
                    .position(|en| en.key == members[at_member])
                    .unwrap_or(inner.entries.len())
            } else if let Some(last) = members.last() {
                inner
                    .entries
                    .iter()
                    .position(|en| en.key == *last)
                    .map_or(inner.entries.len(), |i| i + 1)
            } else {
                inner.entries.len()
            };
            let groups: FxHashSet<String> = match &names {
                Some(n) => inner.known(n).into_iter().collect(),
                None => [group].into_iter().collect(),
            };
            inner.next_key += 1;
            let entry = Entry {
                key: inner.next_key,
                src: Src::Synthetic(data),
                groups,
            };
            inner.entries.insert(at, entry);
        });
    }

    /// What writing `DelegateModel.inItems` (and its like) or
    /// `DelegateModel.groups` of one delegate does to its entry.
    pub(crate) fn set_in_group(self: &Rc<Self>, e: &Engine, key: u64, group: &str, member: bool) {
        let now = self
            .inner
            .borrow()
            .entries
            .iter()
            .find(|en| en.key == key)
            .is_some_and(|en| en.groups.contains(group));
        if now == member {
            return;
        }
        self.membership(
            e,
            vec![key],
            vec![group.to_owned()],
            if member {
                Membership::Add
            } else {
                Membership::Remove
            },
        );
    }

    pub(crate) fn set_groups_of(self: &Rc<Self>, e: &Engine, key: u64, names: Vec<String>) {
        let same = {
            let inner = self.inner.borrow();
            let known: FxHashSet<String> = inner.known(&names).into_iter().collect();
            inner
                .entries
                .iter()
                .find(|en| en.key == key)
                .is_some_and(|en| en.groups == known)
        };
        if !same {
            self.membership(e, vec![key], names, Membership::Set);
        }
    }

    pub(crate) fn entry_key_at_filtered(&self, row: usize) -> Option<u64> {
        self.inner.borrow().entry_at_filtered(row).map(|en| en.key)
    }

    fn model_index_of(&self, key: RowKey) -> Option<usize> {
        let mut inner = self.inner.borrow_mut();
        if inner.model_map.is_none() {
            let map = match &inner.underlying {
                Some(m) => (0..m.row_count()).map(|r| (m.row_key(r), r)).collect(),
                None => FxHashMap::default(),
            };
            inner.model_map = Some(map);
        }
        inner.model_map.as_ref()?.get(&key).copied()
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Membership {
    Add,
    Remove,
    Set,
}

fn insert_place(inner: &Inner, keys: &[RowKey], start: usize) -> usize {
    let at = |k: RowKey| {
        inner
            .entries
            .iter()
            .position(|en| matches!(en.src, Src::Model(m) if m == k))
    };
    if start > 0
        && let Some(i) = keys.get(start - 1).and_then(|k| at(*k))
    {
        return i + 1;
    }
    for k in keys.iter().skip(start) {
        if let Some(i) = at(*k) {
            return i;
        }
    }
    inner.entries.len()
}

/// The key of the entry itself, for telling one item.
fn tell_key(of: &Entry) -> u64 {
    of.key
}

/// A delegate of a DelegateModel: which DelegateModel, its entries, and
/// the entry this delegate stands for.
type KnownDelegate = (ObjectId, Rc<GroupState>, u64);

/// Which delegates belong to which `DelegateModel`, and to which entry of
/// it: an attached object is made for a delegate without saying which row
/// it is, and a delegate is told when the answer changes.
#[derive(Default)]
struct Registry {
    delegates: RefCell<FxHashMap<ObjectId, KnownDelegate>>,
    /// Released by a view but kept by persistedItems, and so not shown.
    released: RefCell<FxHashSet<ObjectId>>,
}

pub(crate) fn register_delegate(
    e: &Engine,
    dm: ObjectId,
    state: &Rc<GroupState>,
    item: ObjectId,
    key: u64,
) {
    let registry = e.extension::<Registry>();
    registry.released.borrow_mut().remove(&item);
    registry
        .delegates
        .borrow_mut()
        .insert(item, (dm, state.clone(), key));
}

pub(crate) fn unregister_delegate(e: &Engine, item: ObjectId) {
    let registry = e.extension::<Registry>();
    registry.delegates.borrow_mut().remove(&item);
    registry.released.borrow_mut().remove(&item);
}

pub(crate) fn delegate_released(e: &Engine, item: ObjectId) {
    if is_persisted(e, item) {
        e.extension::<Registry>().released.borrow_mut().insert(item);
    } else {
        unregister_delegate(e, item);
    }
}

fn destroy_unpersisted(e: &Engine, state: &Rc<GroupState>) {
    let registry = e.extension::<Registry>();
    let doomed: Vec<ObjectId> = {
        let delegates = registry.delegates.borrow();
        registry
            .released
            .borrow()
            .iter()
            .filter(|item| {
                let Some((_, candidate, key)) = delegates.get(*item) else {
                    return false;
                };
                Rc::ptr_eq(candidate, state)
                    && !candidate.inner.borrow().entries.iter().any(|entry| {
                        tell_key(entry) == *key && entry.groups.contains("persistedItems")
                    })
            })
            .copied()
            .collect()
    };
    for item in doomed {
        unregister_delegate(e, item);
        e.destroy_at_turn_end(item);
    }
}

pub(crate) fn destroyed(e: &Engine, gone: &[ObjectId]) {
    let registry = e.extension::<Registry>();
    let mut doomed = Vec::new();
    {
        let mut delegates = registry.delegates.borrow_mut();
        if delegates.is_empty() {
            return;
        }
        let mut released = registry.released.borrow_mut();
        delegates.retain(|item, (dm, _, _)| {
            let item_gone = gone.contains(item);
            let model_gone = gone.contains(dm);
            if model_gone && !item_gone && released.contains(item) {
                doomed.push(*item);
            }
            !(item_gone || model_gone)
        });
        released.retain(|item| delegates.contains_key(item));
    }
    for item in doomed {
        e.destroy_at_turn_end(item);
    }
}

fn lookup(e: &Engine, item: ObjectId) -> Option<KnownDelegate> {
    e.extension::<Registry>()
        .delegates
        .borrow()
        .get(&item)
        .cloned()
}

pub(crate) fn is_persisted(e: &Engine, item: ObjectId) -> bool {
    let Some((_, state, key)) = lookup(e, item) else {
        return false;
    };
    state
        .inner
        .borrow()
        .entries
        .iter()
        .any(|entry| tell_key(entry) == key && entry.groups.contains("persistedItems"))
}

/// A delegate a view placed asked for its `DelegateModel` attached object:
/// the values are put there, and what the author writes to the ones that
/// can be written changes the groups.
pub(crate) fn attached_created(e: &Engine, item: ObjectId, att: ObjectId) {
    let Some((dm, state, _)) = lookup(e, item) else {
        return;
    };
    tell(e, dm, &state, item, att);
    for (prop, name) in [
        (DelegateModelAttached::IN_ITEMS.index, "items"),
        (
            DelegateModelAttached::IN_PERSISTED_ITEMS.index,
            "persistedItems",
        ),
    ] {
        let by = state.clone();
        let _ = e.watch(att, prop, move |e, v| {
            if by.telling.get() {
                return;
            }
            if let Some((_, _, key)) = lookup(e, item) {
                by.set_in_group(e, key, name, v.to_boolean());
            }
        });
    }
    let by = state.clone();
    let _ = e.watch(att, DelegateModelAttached::GROUPS.index, move |e, v| {
        if by.telling.get() {
            return;
        }
        if let Some((_, _, key)) = lookup(e, item) {
            by.set_groups_of(e, key, strings_of(e, v));
        }
    });
}

/// Tells `att`, the attached object of `item`, what its entry is in now.
pub(crate) fn tell(
    e: &Engine,
    dm: ObjectId,
    state: &Rc<GroupState>,
    item: ObjectId,
    att: ObjectId,
) {
    let Some((_, _, key)) = lookup(e, item) else {
        return;
    };
    let values = {
        let inner = state.inner.borrow();
        let at = inner.entries.iter().position(|en| tell_key(en) == key);
        let (names, unresolved) = match at {
            Some(at) => {
                let entry = &inner.entries[at];
                (
                    inner
                        .defs
                        .iter()
                        .filter(|d| entry.groups.contains(&d.name))
                        .map(|d| d.name.clone())
                        .collect::<Vec<String>>(),
                    matches!(entry.src, Src::Synthetic(_)),
                )
            }
            None => (Vec::new(), false),
        };
        // The place of an entry in a group: its own index there when it
        // is in, and the number of the group's members before it when it
        // is not. Both numbers are the same count (read on qml6).
        let place = |group: &str| -> i32 {
            match at {
                Some(at) => inner.entries[..at]
                    .iter()
                    .filter(|en| en.groups.contains(group))
                    .count() as i32,
                None => 0,
            }
        };
        (names, unresolved, place("items"), place("persistedItems"))
    };
    let (names, unresolved, items_at, persisted_at) = values;
    let was = state.telling.replace(true);
    let write = |p, v: Value| {
        let _ = write_computed(e, att, p, v);
    };
    e.batch(|| {
        write(DelegateModelAttached::MODEL.index, Value::Object(dm));
        write(
            DelegateModelAttached::IN_ITEMS.index,
            Value::Bool(names.iter().any(|n| n == "items")),
        );
        write(
            DelegateModelAttached::IN_PERSISTED_ITEMS.index,
            Value::Bool(names.iter().any(|n| n == "persistedItems")),
        );
        write(
            DelegateModelAttached::ITEMS_INDEX.index,
            Value::Int(items_at),
        );
        write(
            DelegateModelAttached::PERSISTED_ITEMS_INDEX.index,
            Value::Int(persisted_at),
        );
        write(
            DelegateModelAttached::IS_UNRESOLVED.index,
            Value::Bool(unresolved),
        );
        write(
            DelegateModelAttached::GROUPS.index,
            Value::list(names.into_iter().map(Value::string).collect()),
        );
    });
    state.telling.set(was);
}

fn retell_all(e: &Engine, state: &Rc<GroupState>) {
    let mine: Vec<ObjectId> = e
        .extension::<Registry>()
        .delegates
        .borrow()
        .iter()
        .filter(|(_, (_, s, _))| Rc::ptr_eq(s, state))
        .map(|(item, _)| *item)
        .collect();
    for item in mine {
        if let Some(att) =
            e.existing_attached_object(item, &MetaRef::Static(DelegateModel::static_meta()))
            && let Some((dm, _, _)) = lookup(e, item)
        {
            tell(e, dm, state, item, att);
        }
    }
}

/// The delegate `item` shows row `row` of the filtered model now: the
/// entry it stands for is that row's, and its attached object, when it
/// has asked for one, is told again. For a delegate a view uses again for
/// another row, whose row changed behind its back.
pub(crate) fn retell_for_row(e: &Engine, item: ObjectId, row: usize) {
    let Some((dm, state, _)) = lookup(e, item) else {
        return;
    };
    let Some(key) = state.entry_key_at_filtered(row) else {
        return;
    };
    register_delegate(e, dm, &state, item, key);
    if let Some(att) =
        e.existing_attached_object(item, &MetaRef::Static(DelegateModel::static_meta()))
    {
        tell(e, dm, &state, item, att);
    }
}

impl ItemModel for GroupState {
    fn row_count(&self) -> usize {
        self.inner.borrow_mut().filtered_count()
    }

    fn role_names(&self) -> Vec<Atom> {
        let (underlying, extra) = {
            let inner = self.inner.borrow();
            (inner.underlying.clone(), inner.extra_roles.clone())
        };
        let mut names = underlying.map_or_else(Vec::new, |m| m.role_names());
        for r in extra {
            if !names.contains(&r) {
                names.push(r);
            }
        }
        names
    }

    fn data(&self, engine: &Engine, row: usize, role: &str) -> Value {
        let (entry, underlying) = {
            let inner = self.inner.borrow();
            let Some(entry) = inner.entry_at_filtered(row).cloned() else {
                return Value::Undefined;
            };
            (entry, inner.underlying.clone())
        };
        match &entry.src {
            Src::Synthetic(data) => data
                .iter()
                .find(|(n, _)| n == role)
                .map_or(Value::Undefined, |(_, v)| v.clone()),
            Src::Model(k) => match (underlying, self.model_index_of(*k)) {
                (Some(m), Some(i)) => m.data(engine, i, role),
                _ => Value::Undefined,
            },
        }
    }

    fn set_data(&self, engine: &Engine, row: usize, role: &str, value: &Value) -> bool {
        let (entry, underlying) = {
            let inner = self.inner.borrow();
            let Some(entry) = inner.entry_at_filtered(row).cloned() else {
                return false;
            };
            (entry, inner.underlying.clone())
        };
        match &entry.src {
            Src::Model(k) => match (underlying, self.model_index_of(*k)) {
                (Some(m), Some(i)) => m.set_data(engine, i, role, value),
                _ => false,
            },
            Src::Synthetic(_) => {
                let changed = {
                    let mut inner = self.inner.borrow_mut();
                    match inner
                        .entries
                        .iter_mut()
                        .find(|en| tell_key(en) == tell_key(&entry))
                    {
                        Some(Entry {
                            src: Src::Synthetic(data),
                            ..
                        }) => {
                            if let Some((_, v)) = data.iter_mut().find(|(n, _)| n == role) {
                                *v = value.clone();
                            } else {
                                data.push((role.to_owned(), value.clone()));
                            }
                            true
                        }
                        _ => false,
                    }
                };
                if changed {
                    self.listeners.emit(
                        engine,
                        &ModelChange::DataChanged {
                            first: row,
                            count: 1,
                            roles: vec![Atom::intern(role)],
                        },
                    );
                }
                changed
            }
        }
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.inner
            .borrow()
            .entry_at_filtered(row)
            .map_or(row as RowKey + (1 << 50), tell_key)
    }

    fn model_data(&self, engine: &Engine, row: usize) -> Option<Value> {
        let entry = self.inner.borrow().entry_at_filtered(row).cloned()?;
        match &entry.src {
            Src::Synthetic(_) => None,
            Src::Model(k) => {
                let m = self.inner.borrow().underlying.clone()?;
                m.model_data(engine, self.model_index_of(*k)?)
            }
        }
    }

    fn row_object(&self, row: usize) -> Option<ObjectId> {
        let entry = self.inner.borrow().entry_at_filtered(row).cloned()?;
        match &entry.src {
            Src::Synthetic(_) => None,
            Src::Model(k) => {
                let m = self.inner.borrow().underlying.clone()?;
                m.row_object(self.model_index_of(*k)?)
            }
        }
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }

    fn delegate_retained(&self, engine: &Engine, object: ObjectId) -> bool {
        is_persisted(engine, object)
    }
}

#[cfg(test)]
mod tests {
    use super::{filtered_change, ranges};
    use ljos_engine::ModelChange;

    #[test]
    fn one_block_in_and_out() {
        assert_eq!(filtered_change(&[1, 2, 3], &[1, 2, 3]), None);
        assert_eq!(
            filtered_change(&[1, 3], &[1, 2, 3]),
            Some(ModelChange::RowsInserted { first: 1, count: 1 })
        );
        assert_eq!(
            filtered_change(&[1, 2, 3], &[1, 3]),
            Some(ModelChange::RowsRemoved { first: 1, count: 1 })
        );
    }

    #[test]
    fn a_moved_block() {
        assert_eq!(
            filtered_change(&[1, 2, 3, 4], &[2, 3, 1, 4]),
            Some(ModelChange::RowsMoved {
                from: 0,
                to: 2,
                count: 1
            })
        );
        assert_eq!(
            filtered_change(&[1, 2, 3, 4], &[1, 4, 2, 3]),
            Some(ModelChange::RowsMoved {
                from: 3,
                to: 1,
                count: 1
            })
        );
        assert_eq!(
            filtered_change(&[1, 2], &[3, 4]),
            Some(ModelChange::ModelReset)
        );
    }

    #[test]
    fn ranges_are_where_they_are() {
        assert_eq!(ranges(&[1], &[1, 2]), (vec![], vec![(1, 1)]));
        assert_eq!(ranges(&[1, 2, 3], &[2]), (vec![(0, 1), (1, 1)], vec![]));
    }
}
