//! `Binding` (QtQml): a value for a property of another object, for as
//! long as `when` is true.
//!
//! It is one layer on the property's override layer, as a change of a
//! state is: what the property had before is underneath, a binding there
//! goes on following what it reads, and `restoreMode` says what of it
//! shows again when the `Binding` lets go.

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, OverrideContent, OverrideId, PropIndex,
    QmlObject, QtObject, RestoreMode, Value, qml_impl,
};
use ljos_reactive::WriteKind;

use crate::animation::driver;
use crate::enums::EnumTable;
use crate::item::{init_item, is_item};

pub(crate) fn binding_enums() -> EnumTable {
    vec![(
        "RestoreMode",
        vec![
            ("RestoreNone", 0),
            ("RestoreBinding", 1),
            ("RestoreValue", 2),
            ("RestoreBindingOrValue", 3),
        ],
    )]
}

/// `Binding` (QtQml).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQml",
    name = "Binding",
    extends = QtObject,
    enums = binding_enums
)]
pub struct Binding {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    /// The property's name, dotted for one of a group (`border.width`).
    #[qml(notify)]
    pub property: Atom,
    #[qml(notify)]
    pub value: Value,
    #[qml(notify)]
    pub when: bool,
    /// `Binding.RestoreMode`: what the property has when `when` turns
    /// false.
    #[qml(notify, enumeration = "RestoreMode")]
    pub restore_mode: i32,
    /// A new `value` is passed on with the next frame, and only the last
    /// of several that came before it.
    #[qml(notify)]
    pub delayed: bool,
    /// Its layer, while it holds the property.
    #[qml(skip)]
    held: Option<(ObjectId, PropIndex, OverrideId)>,
    /// A delayed value is on its way.
    #[qml(skip)]
    waiting: bool,
    #[qml(skip)]
    warned: bool,
}

impl Default for Binding {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            target: None,
            property: Atom::EMPTY,
            value: Value::Undefined,
            when: true,
            restore_mode: 3,
            delayed: false,
            held: None,
            waiting: false,
            warned: false,
        }
    }
}

#[qml_impl]
impl Binding {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (engine, me) = (cx.engine, cx.this);
        for p in [
            Binding::TARGET.index,
            Binding::PROPERTY.index,
            Binding::WHEN.index,
        ] {
            engine.watch(me, p, move |e, _| sync(e, me))?;
        }
        engine.watch(me, Binding::VALUE.index, move |e, _| pass_on(e, me))?;
        sync(engine, me);
        Ok(())
    }
}

/// `Binding on width { }`: the property it is declared on is its target.
pub(crate) fn aim(engine: &Engine, binding: ObjectId, target: ObjectId, prop: PropIndex) -> bool {
    let name = engine
        .meta_of(target)
        .and_then(|m| m.property(prop).map(|p| p.name.to_string()));
    let Some(name) = name else {
        return false;
    };
    for (p, v) in [
        (Binding::TARGET.index, Value::Object(target)),
        (Binding::PROPERTY.index, Value::string(name)),
    ] {
        let _ = engine.write_internal(binding, p, v, WriteKind::Initial);
    }
    true
}

fn held(engine: &Engine, me: ObjectId) -> Option<(ObjectId, PropIndex, OverrideId)> {
    engine.with::<Binding, _>(me, |b| b.held).ok().flatten()
}

/// Takes or lets go of the property, as `target`, `property` and `when`
/// say now.
fn sync(engine: &Engine, me: ObjectId) {
    let read = |p: PropIndex| engine.untrack(|| engine.read(me, p));
    let target = read(Binding::TARGET.index)
        .as_object()
        .filter(|t| engine.is_alive(*t));
    let name = read(Binding::PROPERTY.index).to_js_string();
    let aim = match target {
        Some(t) if !name.is_empty() => match engine.property_index(t, &name) {
            Ok(p) => Some((t, p)),
            Err(_) => {
                warn_once(engine, me, t, &name);
                None
            }
        },
        _ => None,
    };
    let want = aim.filter(|_| read(Binding::WHEN.index).to_boolean());
    let has = held(engine, me);
    if has.map(|(t, p, _)| (t, p)) == want {
        return;
    }
    engine.batch(|| {
        if let Some((t, p, id)) = has {
            let mode = match read(Binding::RESTORE_MODE.index).to_number() as i32 {
                0 => RestoreMode::None,
                1 => RestoreMode::Binding,
                2 => RestoreMode::Value,
                _ => RestoreMode::BindingOrValue,
            };
            let _ = engine.remove_override(t, p, id, mode);
        }
        let mut now = None;
        if let Some((t, p)) = want {
            if is_item(engine, t) {
                let _ = init_item(engine, t);
            }
            // Taking the property is never delayed, only a later value.
            let v = read(Binding::VALUE.index);
            now = engine
                .push_override(t, p, OverrideContent::Value(v))
                .ok()
                .map(|id| (t, p, id));
        }
        let _ = engine.with_mut::<Binding, _>(me, |b| {
            b.held = now;
            b.waiting = false;
        });
    });
}

fn warn_once(engine: &Engine, me: ObjectId, target: ObjectId, name: &str) {
    let first = engine
        .with_mut::<Binding, _>(me, |b| !std::mem::replace(&mut b.warned, true))
        .unwrap_or(false);
    if !first {
        return;
    }
    let ty = engine
        .meta_of(target)
        .map(|m| m.name.to_string())
        .unwrap_or_default();
    let at = engine.declared_at(me).unwrap_or_default();
    engine.log(
        LogLevel::Warning,
        "qml",
        format!("{at}QML Binding: Property '{name}' does not exist on {ty}."),
    );
}

/// `value` changed while the property is held.
fn pass_on(engine: &Engine, me: ObjectId) {
    if held(engine, me).is_none() {
        return;
    }
    let delayed = engine
        .untrack(|| engine.read(me, Binding::DELAYED.index))
        .to_boolean();
    if !delayed {
        write(engine, me);
        return;
    }
    let already = engine
        .with_mut::<Binding, _>(me, |b| std::mem::replace(&mut b.waiting, true))
        .unwrap_or(true);
    if already {
        return;
    }
    driver::later(
        engine,
        Box::new(move |e| {
            // Unless it let go of the property meanwhile.
            let still = e
                .with_mut::<Binding, _>(me, |b| std::mem::take(&mut b.waiting))
                .unwrap_or(false);
            if still {
                write(e, me);
            }
        }),
    );
}

fn write(engine: &Engine, me: ObjectId) {
    let Some((t, p, id)) = held(engine, me) else {
        return;
    };
    let v = engine.untrack(|| engine.read(me, Binding::VALUE.index));
    let _ = engine.write_internal(t, p, v, WriteKind::Override(id));
}
