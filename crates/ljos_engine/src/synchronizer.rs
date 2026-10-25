//! `Synchronizer` (`Qt.labs.synchronizer`): two or more properties kept in
//! step, in whichever direction a change comes from, without breaking a
//! binding on any of them.
//!
//! The properties are the `sourceObject`/`sourceProperty` and
//! `targetObject`/`targetProperty` pairs, the aliases declared in it and
//! the property it is declared `on`. A value is written as an animation
//! step is, so a binding stays and takes over again when what it reads
//! changes. While one change is passed on the others are not listened to,
//! so a property that answers with another value has bounced it, and that
//! answer goes no further.

use crate::atom::Atom;
use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::log::LogLevel;
use crate::types::TypeTag;
use crate::value::Value;
use crate::{NodeId, QmlObject, qml_impl};

type Place = (ObjectId, PropIndex);

/// `Synchronizer`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Qt.labs.synchronizer", name = "Synchronizer", extends = QtObject)]
pub struct Synchronizer {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub source_object: Option<ObjectId>,
    #[qml(notify)]
    pub source_property: Atom,
    #[qml(notify)]
    pub target_object: Option<ObjectId>,
    #[qml(notify)]
    pub target_property: Atom,
    /// The property it is declared `on`.
    #[qml(skip)]
    on: Option<Place>,
    /// The properties it keeps in step, as of the last time they were
    /// named.
    #[qml(skip)]
    places: Vec<Place>,
    #[qml(skip)]
    watchers: Vec<NodeId>,
    /// A change is being passed on; what changes meanwhile is only heard.
    #[qml(skip)]
    syncing: bool,
    #[qml(skip)]
    heard: Vec<Place>,
}

#[qml_impl]
impl Synchronizer {
    #[qml(signal, name = "valueBounced")]
    fn value_bounced(object: Option<ObjectId>, property: Atom);

    #[qml(signal, name = "valueIgnored")]
    fn value_ignored(object: Option<ObjectId>, property: Atom);

    /// The source gives its value to the others once, at completion. A
    /// pair named again later changes what is kept in step from then on.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (engine, me) = (cx.engine, cx.this);
        for p in [
            Synchronizer::SOURCE_OBJECT.index,
            Synchronizer::SOURCE_PROPERTY.index,
            Synchronizer::TARGET_OBJECT.index,
            Synchronizer::TARGET_PROPERTY.index,
        ] {
            engine.watch(me, p, move |e, _| {
                gather(e, me);
            })?;
        }
        if let Some(source) = gather(engine, me) {
            spread(engine, me, source);
        }
        Ok(())
    }
}

/// `Synchronizer on value { }`: the property it is declared on is one of
/// those it keeps in step.
pub(crate) fn aim(engine: &Engine, sync: ObjectId, target: ObjectId, prop: PropIndex) -> bool {
    engine
        .with_mut::<Synchronizer, _>(sync, |s| s.on = Some((target, prop)))
        .is_ok()
}

/// Names the properties to keep in step and listens to each. Returns
/// the source, if one is named.
fn gather(engine: &Engine, me: ObjectId) -> Option<Place> {
    let read = |p: PropIndex| engine.untrack(|| engine.read(me, p));
    let mut places: Vec<Place> = Vec::new();
    let mut source = None;
    let add = |place: Place, places: &mut Vec<Place>| {
        if !places.contains(&place) {
            places.push(place);
        }
    };
    // Its own aliases, an alias named `source` first among sources.
    if let Some(meta) = engine.meta_of(me) {
        for (i, p) in meta.properties.iter().enumerate() {
            if !p.is_alias() {
                continue;
            }
            let place = engine.redirect(me, PropIndex(i as u32));
            if place.0 == me || !engine.is_alive(place.0) {
                continue;
            }
            if p.name.as_str() == "source" {
                source = Some(place);
            }
            add(place, &mut places);
        }
    }
    let pair = |obj: PropIndex, name: PropIndex| -> Option<Place> {
        let obj = read(obj).as_object().filter(|o| engine.is_alive(*o))?;
        let name = read(name).to_js_string();
        if name.is_empty() {
            return None;
        }
        match engine.property_index(obj, &name) {
            Ok(p) => Some((obj, p)),
            Err(_) => {
                let at = engine.declared_at(me).unwrap_or_default();
                engine.log(
                    LogLevel::Warning,
                    "qml",
                    format!("{at}QML Synchronizer: Target object has no property called {name}"),
                );
                None
            }
        }
    };
    let from = pair(
        Synchronizer::SOURCE_OBJECT.index,
        Synchronizer::SOURCE_PROPERTY.index,
    );
    let to = pair(
        Synchronizer::TARGET_OBJECT.index,
        Synchronizer::TARGET_PROPERTY.index,
    );
    if let Some(p) = from {
        source = source.or(Some(p));
        add(p, &mut places);
    }
    if let Some(p) = to {
        add(p, &mut places);
    }
    let on = engine
        .with::<Synchronizer, _>(me, |s| s.on)
        .ok()
        .flatten()
        .filter(|(o, _)| engine.is_alive(*o));
    if let Some(p) = on {
        source = source.or(Some(p));
        add(p, &mut places);
    }

    let old = engine
        .with_mut::<Synchronizer, _>(me, |s| std::mem::take(&mut s.watchers))
        .unwrap_or_default();
    for w in old {
        let _ = engine.inner.graph.dispose(w);
    }
    let mut watchers = Vec::new();
    for &(obj, prop) in &places {
        let w = engine.watch(obj, prop, move |e, _| changed(e, me, (obj, prop)));
        if let Ok(w) = w {
            watchers.push(w);
        }
    }
    let _ = engine.with_mut::<Synchronizer, _>(me, |s| {
        s.places = places;
        s.watchers = watchers;
    });
    source
}

fn changed(engine: &Engine, me: ObjectId, place: Place) {
    if !engine.is_alive(me) {
        return;
    }
    let syncing = engine
        .with_mut::<Synchronizer, _>(me, |s| {
            if s.syncing {
                s.heard.push(place);
            }
            s.syncing
        })
        .unwrap_or(true);
    if !syncing {
        spread(engine, me, place);
    }
}

/// Writes the value of `from` to every other property kept in step. One
/// that gives no change signal and keeps its value has ignored it; one
/// that ends with another value than it was given has bounced it.
fn spread(engine: &Engine, me: ObjectId, from: Place) {
    let read = |(o, p): Place| engine.untrack(|| engine.read(o, p));
    let Ok(places) = engine.with_mut::<Synchronizer, _>(me, |s| {
        s.syncing = true;
        s.places.clone()
    }) else {
        return;
    };
    let v = read(from);
    let mut said: Vec<(&str, Place)> = Vec::new();
    for to in places.into_iter().filter(|p| *p != from) {
        if !engine.is_alive(to.0) {
            continue;
        }
        let _ = engine.with_mut::<Synchronizer, _>(me, |s| s.heard.clear());
        let old = read(to);
        let given = given_to(engine, to, &v);
        let _ = engine.write_derived(to.0, to.1, given.clone());
        let now = read(to);
        let heard = engine
            .with::<Synchronizer, _>(me, |s| s.heard.contains(&to))
            .unwrap_or(false);
        if !heard && now.change_equals(&old) {
            said.push(("valueIgnored", to));
        } else if !now.change_equals(&given) {
            said.push(("valueBounced", to));
        }
    }
    let _ = engine.with_mut::<Synchronizer, _>(me, |s| {
        s.syncing = false;
        s.heard.clear();
    });
    for (signal, (obj, prop)) in said {
        let name = engine
            .meta_of(obj)
            .and_then(|m| m.property(prop).map(|p| p.name.to_string()))
            .unwrap_or_default();
        let _ = engine.emit_by_name(me, signal, &[Value::Object(obj), Value::string(name)]);
    }
}

/// What `to` is given of `v`. The Synchronizer writes as a property
/// written from C++ is, not as a binding: a text that is no number makes
/// an int 0 there (checked on qml6), where a binding or a script refuses it.
fn given_to(engine: &Engine, to: Place, v: &Value) -> Value {
    let Some(ty) = engine
        .meta_of(to.0)
        .and_then(|m| m.property(to.1).map(|p| p.ty.clone()))
    else {
        return v.clone();
    };
    match ty.coerce(v) {
        Ok(c) => c,
        Err(_) if ty == TypeTag::Int && matches!(v, Value::String(_)) => Value::Int(0),
        Err(_) => v.clone(),
    }
}
