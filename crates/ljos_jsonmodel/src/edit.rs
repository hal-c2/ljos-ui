//! The edits of a script: `append`, `insert`, `set`, `setProperty`, `remove`,
//! `move` and `clear`, and a delegate writing a role. Each says what
//! `ListModel` says for the same call, in the same order, so a view or a
//! handler cannot tell the two apart. The rows are the model's own copy of
//! the JSON: nothing here writes to disk, and the next read of the source
//! replaces what was edited (see [`crate::apply`]).

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::*;

use crate::parse::{Node, Row};
use crate::{CATEGORY, JsonListModel, Shared};

/// What a script's value is as JSON. A value JSON has no word for (an
/// object, a colour, `undefined`) is `null`, as `JSON.stringify` makes it in
/// an array; a URL is its text.
pub fn node_of(v: &Value) -> Node {
    match v {
        Value::Bool(b) => Node::Bool(*b),
        Value::Int(i) => Node::Number(f64::from(*i)),
        Value::Real(r) if r.is_finite() => Node::Number(*r),
        Value::String(s) | Value::Url(s) => Node::Text(s.as_str().into()),
        Value::List(items) => Node::List(items.iter().map(node_of).collect()),
        Value::Map(fields) => Node::Map(map_of(fields)),
        _ => Node::Null,
    }
}

pub(crate) fn map_of(fields: &[(Atom, Value)]) -> Vec<(Box<str>, Node)> {
    let mut out: Vec<(Box<str>, Node)> = Vec::with_capacity(fields.len());
    for (k, v) in fields {
        // A key said twice is the last one said, as the reader takes it.
        match out.iter_mut().find(|(o, _)| o.as_ref() == k.as_str()) {
            Some(slot) => slot.1 = node_of(v),
            None => out.push((k.as_str().into(), node_of(v))),
        }
    }
    out
}

/// An index argument: numbers are truncated, as `ListModel` does.
pub(crate) fn index_of(v: &Value) -> Option<i64> {
    let n = v.as_number()?;
    n.is_finite().then(|| n.trunc() as i64)
}

pub(crate) fn warn(e: &Engine, what: &str) {
    e.log(
        LogLevel::Warning,
        CATEGORY,
        format!("QML JsonListModel: {what}"),
    );
}

/// One object, or an array of objects.
pub(crate) fn rows_of(item: &Value) -> Option<Vec<Row>> {
    match item {
        Value::Map(fields) => Some(vec![Row {
            fields: map_of(fields),
        }]),
        Value::List(items) => items
            .iter()
            .map(|i| match i {
                Value::Map(fields) => Some(Row {
                    fields: map_of(fields),
                }),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

pub(crate) type Handle = Rc<RefCell<Shared>>;

pub(crate) fn handle(e: &Engine, me: ObjectId) -> Option<Handle> {
    e.with(me, |m: &JsonListModel| Rc::clone(&m.shared)).ok()
}

/// Adds the keys of `fields` that are not roles yet, at the end, and says so
/// before anything about the rows.
fn grow(e: &Engine, me: ObjectId, shared: &Handle, keys: impl Iterator<Item = Box<str>>) {
    let mut fresh = Vec::new();
    {
        let s = shared.borrow();
        for key in keys {
            let known = s.role_names.iter().any(|n| n.as_str() == key.as_ref());
            if !known && !fresh.contains(&key) {
                fresh.push(key);
            }
        }
    }
    if fresh.is_empty() {
        return;
    }
    let names = {
        let mut s = shared.borrow_mut();
        s.role_names
            .extend(fresh.iter().map(|k| Atom::from(k.as_ref())));
        s.role_names.clone()
    };
    let _ = Cx::new(e, me).set(JsonListModel::ROLES, crate::roles_value(&names));
    let signals = shared.borrow().signals.clone();
    if let Some(signals) = signals {
        signals.roles_changed(e);
    }
}

fn set_count(e: &Engine, me: ObjectId, shared: &Handle) {
    let count = shared.borrow().rows.len() as i32;
    let _ = Cx::new(e, me).set(JsonListModel::COUNT, count);
}

/// What tells both who hears the rows: the views and the handlers of the
/// signals. Handlers are asked only when something is connected.
struct Voice<'a> {
    e: &'a Engine,
    me: ObjectId,
    told: bool,
    signals: Option<ModelSignals>,
}

impl<'a> Voice<'a> {
    fn new(e: &'a Engine, me: ObjectId, shared: &Handle) -> Self {
        Voice {
            e,
            me,
            told: e.any_connection(me),
            signals: shared.borrow().signals.clone(),
        }
    }

    fn cx(&self) -> Cx<'a> {
        Cx::new(self.e, self.me)
    }

    fn range(&self) -> (Value, Value) {
        (invalid_model_index(self.e), invalid_model_index(self.e))
    }
}

pub(crate) fn insert_rows(e: &Engine, me: ObjectId, shared: &Handle, at: usize, rows: Vec<Row>) {
    if rows.is_empty() {
        return;
    }
    let count = rows.len();
    let last = (at + count) as i32 - 1;
    grow(
        e,
        me,
        shared,
        rows.iter()
            .flat_map(|r| r.fields.iter().map(|(k, _)| k.clone()))
            .collect::<Vec<_>>()
            .into_iter(),
    );
    let v = Voice::new(e, me, shared);
    e.batch(|| {
        if let Some(s) = &v.signals {
            s.rows_about_to_be_inserted(e, at, count);
        }
        if v.told {
            let _ = JsonListModel::emit_rows_about_to_be_inserted(
                &v.cx(),
                v.range().0,
                at as i32,
                last,
            );
        }
        {
            let mut s = shared.borrow_mut();
            let fresh: Vec<_> = rows.into_iter().map(|r| s.kept(r)).collect();
            s.rows.splice(at..at, fresh);
        }
        set_count(e, me, shared);
        if let Some(s) = &v.signals {
            s.rows_inserted(e, at, count);
        }
        if v.told {
            let _ = JsonListModel::emit_rows_inserted(&v.cx(), v.range().0, at as i32, last);
        }
    });
}

pub(crate) fn remove_rows(e: &Engine, me: ObjectId, shared: &Handle, first: usize, count: usize) {
    let last = (first + count) as i32 - 1;
    let v = Voice::new(e, me, shared);
    if let Some(s) = &v.signals {
        s.rows_about_to_be_removed(e, first, count);
    }
    if v.told {
        let _ =
            JsonListModel::emit_rows_about_to_be_removed(&v.cx(), v.range().0, first as i32, last);
    }
    // A listener may have changed the model.
    let len = shared.borrow().rows.len();
    let first = first.min(len);
    let count = count.min(len - first);
    shared.borrow_mut().rows.drain(first..first + count);
    e.batch(|| {
        set_count(e, me, shared);
        if count > 0 {
            if let Some(s) = &v.signals {
                s.rows_removed(e, first, count);
            }
            if v.told {
                let _ = JsonListModel::emit_rows_removed(
                    &v.cx(),
                    v.range().0,
                    first as i32,
                    first as i32 + count as i32 - 1,
                );
            }
        }
    });
}

pub(crate) fn shift_rows(
    e: &Engine,
    me: ObjectId,
    shared: &Handle,
    from: usize,
    to: usize,
    count: usize,
) {
    // The row the moved ones go before, as it was.
    let before = if to > from { to + count } else { to } as i32;
    let v = Voice::new(e, me, shared);
    let (first, last) = (from as i32, (from + count) as i32 - 1);
    if v.told {
        let _ = JsonListModel::emit_rows_about_to_be_moved(
            &v.cx(),
            v.range().0,
            first,
            last,
            v.range().1,
            before,
        );
    }
    {
        let mut s = shared.borrow_mut();
        let moved: Vec<_> = s.rows.drain(from..from + count).collect();
        s.rows.splice(to..to, moved);
    }
    e.batch(|| {
        if let Some(s) = &v.signals {
            s.rows_moved(e, from, to, count);
        }
        if v.told {
            let _ = JsonListModel::emit_rows_moved(
                &v.cx(),
                v.range().0,
                first,
                last,
                v.range().1,
                before,
            );
        }
    });
}

/// Gives `row` the values in `entries`. A write of the value a role already
/// has is still said, as `ListModel` says it. True when the row is there.
pub fn set_fields(
    e: &Engine,
    me: ObjectId,
    shared: &Handle,
    row: usize,
    entries: Vec<(Box<str>, Node)>,
) -> bool {
    if row >= shared.borrow().rows.len() {
        return false;
    }
    if entries.is_empty() {
        return true;
    }
    grow(e, me, shared, entries.iter().map(|(k, _)| k.clone()));
    let ids: Vec<usize> = {
        let mut s = shared.borrow_mut();
        let names = s.role_names.clone();
        let target = &mut s.rows[row].row;
        let mut ids = Vec::new();
        for (key, node) in entries {
            match target.fields.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = node,
                None => target.fields.push((key.clone(), node)),
            }
            if let Some(i) = names.iter().position(|n| n.as_str() == key.as_ref()) {
                ids.push(i);
            }
        }
        ids
    };
    let v = Voice::new(e, me, shared);
    e.batch(|| {
        if let Some(s) = &v.signals {
            let names: Vec<Atom> = {
                let s = shared.borrow();
                ids.iter().map(|i| s.role_names[*i].clone()).collect()
            };
            let names: Vec<&str> = names.iter().map(Atom::as_str).collect();
            s.data_changed(e, row, 1, &names);
        }
        if v.told {
            let at = model_index(e, me, row as i32);
            let _ = JsonListModel::emit_data_changed(
                &v.cx(),
                at.clone(),
                at,
                Value::list(ids.iter().map(|i| Value::Int(*i as i32)).collect()),
            );
        }
    });
    true
}

/// A delegate wrote `role` of `row`.
pub fn write_role(e: &Engine, shared: &Handle, row: usize, role: &str, value: &Value) -> bool {
    let Some(me) = shared.borrow().me else {
        return false;
    };
    set_fields(e, me, shared, row, vec![(role.into(), node_of(value))])
}
