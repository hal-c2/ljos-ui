//! `InstanceModel` and `InstanceRepeater`: an instance table as rows a
//! view or a repeater reads, one row a copy.
//!
//! The rows are what QML wrote into the table (`instancing::rows`), not
//! what the GPU was given, so they are there on a host without one.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ljos::ljos_engine::{ModelSignals, QmlListModel, Vector4d};
use ljos::prelude::*;

use crate::instancing::{self, Entry};
use crate::node::host_write;
use crate::repeater::Repeater3D;
use crate::{Node, Object3D};

const ROLES: [&str; 5] = [
    "modelPosition",
    "modelRotation",
    "modelScale",
    "modelColor",
    "modelData",
];

/// The rows shown now and the signals that say they changed; shared
/// between the QML object and the model its views read.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    rows: Vec<Entry>,
    signals: Option<ModelSignals>,
}

struct Rows(Rc<RefCell<Shared>>);

impl QmlListModel for Rows {
    fn row_count(&self) -> usize {
        self.0.borrow().rows.len()
    }

    fn role_names(&self) -> Vec<Atom> {
        ROLES.iter().map(|r| Atom::intern(r)).collect()
    }

    fn data(&self, row: usize, role: &str) -> Value {
        let shared = self.0.borrow();
        let Some(e) = shared.rows.get(row) else {
            return Value::Undefined;
        };
        match role {
            "modelPosition" => vector(e.position),
            "modelRotation" => Value::Quaternion(Arc::new(Quaternion {
                scalar: e.rotation.w,
                x: e.rotation.x,
                y: e.rotation.y,
                z: e.rotation.z,
            })),
            "modelScale" => vector(e.scale),
            "modelColor" => Value::Color(e.color),
            "modelData" => {
                let [x, y, z, w] = e.custom;
                Value::Vector4d(Arc::new(Vector4d { x, y, z, w }))
            }
            _ => Value::Undefined,
        }
    }
}

fn vector(v: ljos::item::ljos_3d::math::Vec3) -> Value {
    Value::Vector3d(Vector3d {
        x: v.x,
        y: v.y,
        z: v.z,
    })
}

/// Gives `me` its model face, once, before anything shows its rows.
fn attach(e: &Engine, me: ObjectId, shared: &Rc<RefCell<Shared>>) -> Result<(), EngineError> {
    if shared.borrow().signals.is_some() {
        return Ok(());
    }
    let signals = e.set_list_model(me, Rc::new(Rows(Rc::clone(shared))))?;
    shared.borrow_mut().signals = Some(signals);
    Ok(())
}

/// Shows `rows`; false when they are the rows shown already.
fn show(e: &Engine, shared: &RefCell<Shared>, rows: Vec<Entry>) -> bool {
    let (signals, same_count, len) = {
        let mut s = shared.borrow_mut();
        if s.rows == rows {
            return false;
        }
        let same_count = s.rows.len() == rows.len();
        s.rows = rows;
        (s.signals.clone(), same_count, s.rows.len())
    };
    // Told after the borrow ends: a view reads the rows while it is told.
    if let Some(signals) = signals {
        if same_count {
            signals.data_changed(e, 0, len, &[]);
        } else {
            signals.model_reset(e);
        }
    }
    true
}

/// `InstanceModel` (QtQuick3D.Helpers): a row for each copy of
/// `instancingTable`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "InstanceModel", extends = Object3D)]
pub struct InstanceModel {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify, object = "Instancing")]
    pub instancing_table: Option<ObjectId>,
    #[qml(skip)]
    shared: Rc<RefCell<Shared>>,
}

#[qml_impl]
impl InstanceModel {
    /// A view that completed first resolved this object to no rows and
    /// tracked it; giving it its model face here makes that view look
    /// again.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let shared = e.with(me, |m: &InstanceModel| Rc::clone(&m.shared))?;
        attach(e, me, &shared)?;
        e.effect_in(me, move |e| {
            let rows = table_rows(e, me, InstanceModel::INSTANCING_TABLE);
            e.untrack(|| show(e, &shared, rows));
        })?;
        Ok(())
    }
}

fn table_rows(e: &Engine, me: ObjectId, table: Prop<Option<ObjectId>>) -> Vec<Entry> {
    match e.get(me, table).ok().flatten() {
        Some(t) => instancing::rows(e, t),
        None => Vec::new(),
    }
}

/// `InstanceRepeater` (QtQuick3D.Helpers): a `Repeater3D` whose model is
/// the copies of `instancingTable`; each delegate is moved, turned and
/// scaled as its copy is.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "InstanceRepeater",
    extends = Repeater3D,
    default = "delegate"
)]
pub struct InstanceRepeater {
    #[qml(base)]
    pub repeater: Repeater3D,
    #[qml(notify, object = "Instancing")]
    pub instancing_table: Option<ObjectId>,
    #[qml(skip)]
    shared: Rc<RefCell<Shared>>,
}

#[qml_impl]
impl InstanceRepeater {
    /// Runs after `Repeater3D`'s completion, so its delegates exist to be
    /// given this object as their model. The rows are read before that,
    /// so the first delegates are placed as they are made.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let shared = e.with(me, |m: &InstanceRepeater| Rc::clone(&m.shared))?;
        e.with_mut(me, |r: &mut Repeater3D| r.place = Some(place))?;
        attach(e, me, &shared)?;
        e.effect_in(me, move |e| {
            let rows = table_rows(e, me, InstanceRepeater::INSTANCING_TABLE);
            e.untrack(|| {
                // A new count remakes every delegate, and each is placed
                // as it is added; the same count moves the ones there.
                let count = rows.len();
                if show(e, &shared, rows) {
                    for row in 0..count {
                        let made = e.with(me, |r: &Repeater3D| r.object_of(row));
                        if let Ok(Some(object)) = made {
                            place(e, me, row, object);
                        }
                    }
                }
            });
        })?;
        e.set_internal(me, Repeater3D::MODEL, Value::Object(me))?;
        Ok(())
    }
}

/// Moves the delegate of `row` to where its copy is.
fn place(e: &Engine, me: ObjectId, row: usize, object: ObjectId) {
    let Some(entry) = e
        .with(me, |r: &InstanceRepeater| {
            r.shared.borrow().rows.get(row).cloned()
        })
        .ok()
        .flatten()
    else {
        return;
    };
    let (p, s, q) = (entry.position, entry.scale, entry.rotation);
    host_write(
        e,
        object,
        Node::POSITION,
        Vector3d {
            x: p.x,
            y: p.y,
            z: p.z,
        },
    );
    host_write(
        e,
        object,
        Node::SCALE,
        Vector3d {
            x: s.x,
            y: s.y,
            z: s.z,
        },
    );
    host_write(
        e,
        object,
        Node::ROTATION,
        Quaternion {
            scalar: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        },
    );
}
