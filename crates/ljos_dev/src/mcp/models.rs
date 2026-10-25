//! `app_objects` with `models` or `model`: the application's models, as
//! an observer's table viewer shows its tables.
//!
//! A model is an object that is one (a ListModel, an ObjectModel, a
//! DelegateModel, a model written in Rust behind an object), or the
//! number or array a view, repeater or instantiator was given, which has
//! no object but that view. Both are found when asked: the objects are
//! walked once, and the engine says which model each view was given.
//! A model is subscribed to only while a client follows it.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use ljos_engine::{Engine, ItemModel, ModelChange, ObjectId, Value as QmlValue};
use ljos_mcp_core::{SurfaceError, ToolReply};
use serde_json::{Value, json};

use super::act::to_json;
use super::objects::handle;
use super::observe::declared_at;
use super::tools::text_property;
use super::watch::ChangeFeed;

/// The most rows a page holds unless told otherwise, and at all.
const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 500;
/// The most models one answer lists unless told otherwise.
const DEFAULT_MAX: usize = 200;
/// The most clients that follow a model at once.
const MAX_FOLLOWERS: usize = 16;

fn data_ptr(m: &Rc<dyn ItemModel>) -> *const () {
    Rc::as_ptr(m).cast::<()>()
}

/// One model, and who has it.
struct Found {
    model: Rc<dyn ItemModel>,
    /// The object the model is, or the view a number or array was given.
    owner: ObjectId,
    /// Whether the model is an object of its own.
    is_object: bool,
    views: Vec<ObjectId>,
}

/// What the type of a model is called: its object's type, or what the
/// view was given.
fn type_of(engine: &Engine, f: &Found) -> String {
    if f.is_object {
        return engine
            .meta_of(f.owner)
            .map_or_else(|| "object".to_owned(), |m| m.name.to_string());
    }
    match engine.read_by_name(f.owner, "model") {
        QmlValue::Int(_) | QmlValue::Real(_) => "number",
        QmlValue::List(_) => "array",
        QmlValue::String(_) => "string",
        QmlValue::Object(_) => "objects",
        _ => "value",
    }
    .to_owned()
}

/// Every model there is: the objects that are one, in the order they
/// were made, then the numbers and arrays views were given.
fn all(engine: &Engine) -> Vec<Found> {
    engine.untrack(|| {
        let mut out: Vec<Found> = Vec::new();
        let mut at: HashMap<*const (), usize> = HashMap::new();
        for obj in engine.object_ids() {
            if let Some(model) = ljos_quick::model_of_object(engine, obj) {
                at.entry(data_ptr(&model)).or_insert_with(|| {
                    out.push(Found {
                        model,
                        owner: obj,
                        is_object: true,
                        views: Vec::new(),
                    });
                    out.len() - 1
                });
            }
        }
        let mut shown = engine.shown_models();
        shown.sort_by_key(|(view, _)| handle_order(*view));
        for (view, model) in shown {
            match at.get(&data_ptr(&model)) {
                Some(&i) => out[i].views.push(view),
                None => {
                    at.insert(data_ptr(&model), out.len());
                    out.push(Found {
                        model,
                        owner: view,
                        is_object: false,
                        views: vec![view],
                    });
                }
            }
        }
        out
    })
}

/// Objects in the order of their places, as the store keeps them.
fn handle_order(obj: ObjectId) -> u64 {
    use slotmap::Key;
    obj.data().as_ffi() & 0xffff_ffff
}

/// The model `obj` is, or the one it shows, with the object that has it:
/// a view given a model object stands for that object.
pub(crate) fn model_at(
    engine: &Engine,
    obj: ObjectId,
) -> Option<(Rc<dyn ItemModel>, ObjectId, bool)> {
    if let Some(m) = ljos_quick::model_of_object(engine, obj) {
        return Some((m, obj, true));
    }
    let shown = engine.shown_model(obj)?;
    if let QmlValue::Object(o) = engine.untrack(|| engine.read_by_name(obj, "model"))
        && engine.is_alive(o)
        && ljos_quick::model_of_object(engine, o).is_some_and(|m| data_ptr(&m) == data_ptr(&shown))
    {
        return Some((shown, o, true));
    }
    Some((shown, obj, false))
}

fn roles(model: &Rc<dyn ItemModel>) -> Vec<String> {
    model.role_names().iter().map(|r| r.to_string()).collect()
}

/// The words a model is named by in a line of text.
fn label(engine: &Engine, owner: ObjectId) -> String {
    let ty = engine
        .meta_of(owner)
        .map_or_else(|| "object".to_owned(), |m| m.name.to_string());
    match text_property(engine, owner, "objectName") {
        Some(name) => format!("{ty} \"{name}\""),
        None => ty,
    }
}

/// The views that show a model, each with where it is declared, for a
/// client to open.
fn shown_by(engine: &Engine, views: &[ObjectId]) -> Vec<Value> {
    views
        .iter()
        .map(|v| {
            json!({
                "view": handle(*v),
                "name": label(engine, *v),
                "at": declared_at(engine, *v),
            })
        })
        .collect()
}

/// The views alive that show `model`, in the order of their places.
fn views_of(engine: &Engine, model: &Rc<dyn ItemModel>) -> Vec<ObjectId> {
    let mut views: Vec<ObjectId> = engine
        .shown_models()
        .into_iter()
        .filter(|(_, m)| data_ptr(m) == data_ptr(model))
        .map(|(v, _)| v)
        .collect();
    views.sort_by_key(|v| handle_order(*v));
    views
}

/// One model as a line of text.
fn line(engine: &Engine, f: &Found, ty: &str, count: usize, writable: bool) -> String {
    let mut line = if f.is_object {
        format!("{} {count} rows", label(engine, f.owner))
    } else {
        format!("{ty} {count} rows, of {}", label(engine, f.owner))
    };
    let names = roles(&f.model);
    if !names.is_empty() {
        let _ = write!(line, ", roles {}", names.join(", "));
    }
    if writable {
        line.push_str(", writable");
    }
    if f.is_object && !f.views.is_empty() {
        let names: Vec<String> = f.views.iter().map(|v| label(engine, *v)).collect();
        let _ = write!(line, ", shown by {}", names.join(", "));
    }
    let _ = write!(line, " {}", handle(f.owner));
    if let Some(at) = declared_at(engine, f.owner) {
        let _ = write!(line, " at {at}");
    }
    line
}

/// Answers `{"models": true}`.
pub(crate) fn list(engine: &Engine, max: Option<usize>, per_copy: usize) -> ToolReply {
    let found = all(engine);
    let total = found.len();
    let max = max.unwrap_or(DEFAULT_MAX);
    let rows: Vec<(Value, String)> = engine.untrack(|| {
        found
            .iter()
            .take(max)
            .map(|f| {
                let ty = type_of(engine, f);
                let count = f.model.row_count();
                let writable = f.is_object && engine.is_list_model(f.owner);
                let name = text_property(engine, f.owner, "objectName");
                let views: Vec<String> = f.views.iter().map(|v| handle(*v)).collect();
                let row = json!({
                    "model": handle(f.owner),
                    "type": ty,
                    "object_name": name,
                    "owned_by_view": !f.is_object,
                    "count": count,
                    "roles": roles(&f.model),
                    "writable": writable,
                    "views": views,
                    "shown_by": shown_by(engine, &f.views),
                    "at": declared_at(engine, f.owner),
                });
                let line = line(engine, f, &ty, count, writable);
                (row, line)
            })
            .collect()
    });
    let mut keep = rows.len();
    loop {
        let value = json!({
            "models": rows[..keep].iter().map(|(v, _)| v.clone()).collect::<Vec<_>>(),
            "left_out": total - keep,
        });
        let mut text: String = rows[..keep]
            .iter()
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if keep == 0 {
            text = "the application has no models".to_owned();
        }
        if total > keep {
            let _ = write!(text, "\n({} more models left out)", total - keep);
        }
        if keep <= 1 || (text.len() <= per_copy && value.to_string().len() <= per_copy) {
            return ToolReply::from(value).with_text(text);
        }
        keep -= keep.div_ceil(4);
    }
}

/// The values of row `row`, by role. A model without roles gives its
/// `modelData`, or the row's object for a model of objects.
fn row_values(engine: &Engine, model: &Rc<dyn ItemModel>, names: &[String], row: usize) -> Value {
    let mut values = serde_json::Map::new();
    for role in names {
        values.insert(
            role.clone(),
            to_json(engine, &model.data(engine, row, role), 0),
        );
    }
    if names.is_empty() {
        let data = model
            .model_data(engine, row)
            .unwrap_or(QmlValue::Int(i32::try_from(row).unwrap_or(i32::MAX)));
        values.insert("modelData".to_owned(), to_json(engine, &data, 0));
    }
    let mut out = json!({ "index": row, "values": values });
    if let Some(obj) = model.row_object(row) {
        out["object"] = json!(handle(obj));
    }
    out
}

/// Answers `{"model": …}`: a page of the rows of the model of `obj`.
pub(crate) fn rows(
    engine: &Engine,
    obj: ObjectId,
    what: &str,
    offset: Option<usize>,
    limit: Option<usize>,
    per_copy: usize,
) -> Result<(ToolReply, Rc<dyn ItemModel>), SurfaceError> {
    let Some((model, obj, is_object)) = model_at(engine, obj) else {
        return Err(SurfaceError::invalid_argument(format!(
            "`{what}` is no model and shows no model. `app_objects` with `models` lists them"
        )));
    };
    let f = Found {
        model: model.clone(),
        owner: obj,
        is_object,
        views: Vec::new(),
    };
    let reply = engine.untrack(|| {
        let ty = type_of(engine, &f);
        let writable = is_object && engine.is_list_model(obj);
        let count = model.row_count();
        let offset = offset.unwrap_or(0).min(count);
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        let names = roles(&model);
        let views = shown_by(engine, &views_of(engine, &model));
        let at = declared_at(engine, obj);
        let end = offset.saturating_add(limit).min(count);
        let all_rows: Vec<Value> = (offset..end)
            .map(|r| row_values(engine, &model, &names, r))
            .collect();
        let mut keep = all_rows.len();
        loop {
            let next = (offset + keep < count).then_some(offset + keep);
            let value = json!({
                "model": handle(obj),
                "type": ty,
                "writable": writable,
                "count": count,
                "roles": names,
                "offset": offset,
                "rows": all_rows[..keep],
                "next_offset": next,
                "at": at,
                "shown_by": views,
            });
            let mut text = format!(
                "{} {}: rows {offset} to {} of {count}",
                label(engine, obj),
                handle(obj),
                offset + keep
            );
            for r in &all_rows[..keep] {
                let _ = write!(text, "\n{}  {}", r["index"], r["values"]);
            }
            if keep <= 1 || (text.len() <= per_copy && value.to_string().len() <= per_copy) {
                return ToolReply::from(value).with_text(text);
            }
            keep -= keep.div_ceil(4);
        }
    });
    Ok((reply, model))
}

/// Writes `value` to role `role` of row `row` of the model of `obj`, as a
/// delegate's `model.role = value` does: through the model's own write.
/// Answers the value before and after.
pub(crate) fn write_cell(
    engine: &Engine,
    obj: ObjectId,
    what: &str,
    row: usize,
    role: &str,
    value: &QmlValue,
) -> Result<(Value, Value), SurfaceError> {
    let Some((model, _, _)) = model_at(engine, obj) else {
        return Err(SurfaceError::invalid_argument(format!(
            "`{what}` is no model and shows no model, so it has no rows to write"
        )));
    };
    let count = model.row_count();
    if row >= count {
        return Err(SurfaceError::invalid_argument(format!(
            "`row` {row} is past the last: `{what}` has {count} rows"
        )));
    }
    let names = roles(&model);
    if !names.iter().any(|n| n == role) {
        return Err(SurfaceError::new(
            ljos_mcp_core::ErrorCode::NotActionable,
            format!(
                "the model of `{what}` has no role `{role}`; it has: {}",
                if names.is_empty() {
                    "none".to_owned()
                } else {
                    names.join(", ")
                }
            ),
        ));
    }
    let before = to_json(engine, &model.data(engine, row, role), 0);
    if !model.set_data(engine, row, role, value) {
        return Err(SurfaceError::new(
            ljos_mcp_core::ErrorCode::NotActionable,
            format!(
                "the model of `{what}` took no `{role}` for row {row}: it takes no writes, or \
                 not a value of that type"
            ),
        ));
    }
    let after = to_json(engine, &model.data(engine, row, role), 0);
    Ok((before, after))
}

/// The model each client follows, by its holder name: a change of it
/// moves the watch's `models` count. Nothing is subscribed otherwise.
pub(crate) struct Follows {
    changes: Arc<ChangeFeed>,
    by_holder: HashMap<String, (Weak<dyn ItemModel>, u64)>,
}

impl Follows {
    pub(crate) fn new(changes: Arc<ChangeFeed>) -> Self {
        Follows {
            changes,
            by_holder: HashMap::new(),
        }
    }

    /// `holder` follows `model` from now on, and no other.
    pub(crate) fn follow(&mut self, holder: &str, model: &Rc<dyn ItemModel>) {
        if let Some((m, _)) = self.by_holder.get(holder)
            && m.upgrade().is_some_and(|m| data_ptr(&m) == data_ptr(model))
        {
            return;
        }
        self.end(holder);
        if self.by_holder.len() >= MAX_FOLLOWERS {
            return;
        }
        let changes = self.changes.clone();
        // What is about to happen is told again when it has.
        let id = model.subscribe(Rc::new(move |_: &Engine, c: &ModelChange| {
            if !matches!(
                c,
                ModelChange::RowsAboutToBeInserted { .. }
                    | ModelChange::RowsAboutToBeRemoved { .. }
            ) {
                changes.model();
            }
        }));
        self.by_holder
            .insert(holder.to_owned(), (Rc::downgrade(model), id));
    }

    /// `holder` follows nothing.
    pub(crate) fn end(&mut self, holder: &str) {
        if let Some((m, id)) = self.by_holder.remove(holder)
            && let Some(m) = m.upgrade()
        {
            m.unsubscribe(id);
        }
    }

    #[cfg(test)]
    fn following(&self) -> usize {
        self.by_holder.len()
    }
}

impl Drop for Follows {
    fn drop(&mut self) {
        for (m, id) in self.by_holder.values() {
            if let Some(m) = m.upgrade() {
                m.unsubscribe(*id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ljos_engine::{Atom, ModelListener, ModelListeners};

    struct Rows(ModelListeners);

    impl ItemModel for Rows {
        fn row_count(&self) -> usize {
            2
        }
        fn role_names(&self) -> Vec<Atom> {
            vec![Atom::intern("name")]
        }
        fn data(&self, _: &Engine, _: usize, _: &str) -> QmlValue {
            QmlValue::Undefined
        }
        fn subscribe(&self, listener: ModelListener) -> u64 {
            self.0.add(listener)
        }
        fn unsubscribe(&self, id: u64) {
            self.0.remove(id);
        }
    }

    #[test]
    fn a_holder_follows_one_model_and_lets_it_go() {
        let rows = Rc::new(Rows(ModelListeners::default()));
        let model: Rc<dyn ItemModel> = rows.clone();
        let mut follows = Follows::new(ChangeFeed::new());
        follows.follow("a", &model);
        follows.follow("a", &model);
        assert_eq!(follows.following(), 1);
        assert!(!rows.0.is_empty());
        follows.end("a");
        assert!(rows.0.is_empty());
        follows.follow("b", &model);
        drop(follows);
        assert!(rows.0.is_empty());
    }
}
