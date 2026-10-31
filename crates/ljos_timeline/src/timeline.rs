//! `Timeline`, `KeyframeGroup` and `Keyframe`.

use std::rc::Rc;

use ljos::item::easing::Curve;
use ljos::item::{
    EasingGroup, OverrideContent, OverrideId, RestoreMode, TypeTag, WriteKind, between, curve_of,
};
use ljos::prelude::*;

use crate::keyframes;

/// `Keyframe`: the value at a frame. Its easing is the way to it.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Timeline", name = "Keyframe", extends = QtObject)]
pub struct Keyframe {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub frame: f64,
    #[qml(notify)]
    pub value: Value,
    #[qml(group)]
    pub easing: EasingGroup,
}

/// One keyframe, read.
#[derive(Clone, Debug)]
struct Key {
    frame: f64,
    value: Value,
    curve: Curve,
}

/// `KeyframeGroup`: the keyframes of one property of one object.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Timeline",
    name = "KeyframeGroup",
    extends = QtObject,
    default = "keyframes"
)]
pub struct KeyframeGroup {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    #[qml(notify)]
    pub property: Atom,
    #[qml(list)]
    pub keyframes: QmlList,
    /// A file of keyframes; while it holds some, `keyframes` is not used.
    #[qml(notify)]
    pub keyframe_source: Url,
    #[qml(skip)]
    written: Rc<[Key]>,
    #[qml(skip)]
    from_file: Option<Rc<[Key]>>,
    /// Which load is the last one asked for.
    #[qml(skip)]
    load: u64,
    #[qml(skip)]
    timeline: Option<ObjectId>,
    /// What cannot be set is said once.
    #[qml(skip)]
    warned: bool,
}

/// A property a timeline holds.
#[derive(Debug)]
struct Part {
    target: ObjectId,
    prop: PropIndex,
    id: OverrideId,
    ty: TypeTag,
    /// What it was when taken: the way to the first keyframe starts there.
    original: Value,
}

/// A group, resolved.
#[derive(Debug)]
struct Entry {
    group: ObjectId,
    part: usize,
    /// `eulerRotation.x`: one member of the value.
    member: Option<Box<str>>,
}

/// `Timeline`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Timeline",
    name = "Timeline",
    extends = QtObject,
    default = "keyframeGroups"
)]
pub struct Timeline {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub start_frame: f64,
    #[qml(notify)]
    pub end_frame: f64,
    #[qml(notify)]
    pub current_frame: f64,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(list)]
    pub keyframe_groups: QmlList,
    #[qml(list)]
    pub animations: QmlList,
    #[qml(skip)]
    parts: Vec<Part>,
    #[qml(skip)]
    entries: Vec<Entry>,
}

fn objects(v: Value) -> Vec<ObjectId> {
    match v {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

/// `path` on `target`: the object holding the property, the property, and the
/// member of its value if the path ends in one.
fn resolve(
    e: &Engine,
    mut target: ObjectId,
    path: &str,
) -> Option<(ObjectId, PropIndex, Option<Box<str>>)> {
    let mut name = path;
    // `button.opacity` where `button` holds an object.
    while e.property_index(target, name).is_err()
        && let Some((head, rest)) = name.split_once('.')
        && let Ok(p) = e.property_index(target, head)
        && let Some(o) = e.read(target, p).as_object().filter(|o| e.is_alive(*o))
    {
        target = o;
        name = rest;
    }
    if let Ok(p) = e.property_index(target, name) {
        return Some((target, p, None));
    }
    let (head, member) = name.rsplit_once('.')?;
    let whole = e.property_index(target, head).ok()?;
    let ty = e.meta_of(target)?.property(whole)?.ty.clone();
    Value::Undefined
        .with_member(&ty, member, &Value::Undefined)
        .map(|_| (target, whole, Some(member.into())))
}

/// Lets go of everything: each property goes back to what it was, its binding
/// if it had one.
fn release(e: &Engine, me: ObjectId) {
    let parts = e
        .with_mut(me, |t: &mut Timeline| {
            t.entries.clear();
            std::mem::take(&mut t.parts)
        })
        .unwrap_or_default();
    for part in parts {
        if e.is_alive(part.target) {
            let _ = e.remove_override(part.target, part.prop, part.id, RestoreMode::BindingOrValue);
        }
    }
}

/// Works out what the groups hold now, takes and lets go of properties to
/// match, and shows the current frame.
fn sync(e: &Engine, me: ObjectId) {
    e.untrack(|| {
        let enabled = e.read(me, Timeline::ENABLED.index).to_boolean();
        if !enabled {
            release(e, me);
            return;
        }
        let groups = objects(e.read(me, Timeline::KEYFRAME_GROUPS.index));
        let mut parts = e
            .with_mut(me, |t: &mut Timeline| std::mem::take(&mut t.parts))
            .unwrap_or_default();
        let mut used = vec![false; parts.len()];
        let mut entries = Vec::with_capacity(groups.len());
        for group in groups {
            let Some(target) = e
                .read(group, KeyframeGroup::TARGET.index)
                .as_object()
                .filter(|t| e.is_alive(*t))
            else {
                continue;
            };
            let property = e.read(group, KeyframeGroup::PROPERTY.index).to_js_string();
            if property.is_empty() {
                continue;
            }
            let Some((target, prop, member)) = resolve(e, target, &property) else {
                cannot_set(e, group, &property);
                continue;
            };
            let found = parts
                .iter()
                .position(|p| p.target == target && p.prop == prop);
            let part = match found {
                Some(i) => i,
                None => {
                    let original = e.read(target, prop);
                    let ty = e
                        .meta_of(target)
                        .and_then(|m| m.property(prop).map(|p| p.ty.clone()));
                    let content = OverrideContent::Value(original.clone());
                    match (ty, e.push_override(target, prop, content)) {
                        (Some(ty), Ok(id)) => {
                            parts.push(Part {
                                target,
                                prop,
                                id,
                                ty,
                                original,
                            });
                            used.push(false);
                            parts.len() - 1
                        }
                        _ => {
                            cannot_set(e, group, &property);
                            continue;
                        }
                    }
                }
            };
            used[part] = true;
            entries.push(Entry {
                group,
                part,
                member,
            });
        }
        // Back to front, for the entries' numbers to stay right below.
        for i in (0..parts.len()).rev() {
            if !used[i] {
                let gone = parts.remove(i);
                if e.is_alive(gone.target) {
                    let _ = e.remove_override(
                        gone.target,
                        gone.prop,
                        gone.id,
                        RestoreMode::BindingOrValue,
                    );
                }
                for entry in &mut entries {
                    if entry.part > i {
                        entry.part -= 1;
                    }
                }
            }
        }
        let _ = e.with_mut(me, |t: &mut Timeline| {
            t.parts = parts;
            t.entries = entries;
        });
    });
    show(e, me);
}

fn cannot_set(e: &Engine, group: ObjectId, property: &str) {
    let first = e
        .with_mut(group, |g: &mut KeyframeGroup| {
            !std::mem::replace(&mut g.warned, true)
        })
        .unwrap_or(false);
    if first {
        e.warn(format!("Cannot set property \"{property}\""));
    }
}

/// What `keys` say at `frame`. Before the first the way starts at `original`,
/// standing at `start`; past the last it is the last's.
fn value_at(keys: &[Key], frame: f64, start: f64, original: &Value, ty: &TypeTag) -> Option<Value> {
    let after = keys.partition_point(|k| k.frame <= frame);
    let Some(to) = keys.get(after) else {
        return keys.last().map(|k| k.value.clone());
    };
    let (from, at) = match after.checked_sub(1) {
        Some(i) => (&keys[i].value, keys[i].frame),
        None => (original, start),
    };
    let span = to.frame - at;
    let p = if span > 0.0 {
        ((frame - at) / span).clamp(0.0, 1.0)
    } else {
        1.0
    };
    between(from, &to.value, ty, to.curve.value(p))
}

/// Writes what the current frame says, every member of a value in one write.
fn show(e: &Engine, me: ObjectId) {
    e.untrack(|| {
        let frame = e.read(me, Timeline::CURRENT_FRAME.index).to_number();
        let start = e.read(me, Timeline::START_FRAME.index).to_number();
        // Out of the object while it writes: a write may come back here
        // (a Behavior, a handler that moves the frame).
        let Ok((parts, entries)) = e.with_mut(me, |t: &mut Timeline| {
            (std::mem::take(&mut t.parts), std::mem::take(&mut t.entries))
        }) else {
            return;
        };
        let mut pending: Vec<Option<Value>> = vec![None; parts.len()];
        for entry in &entries {
            let part = &parts[entry.part];
            let keys = e
                .with(entry.group, keys_of)
                .unwrap_or_else(|_| Rc::from([]));
            let value = match &entry.member {
                None => value_at(&keys, frame, start, &part.original, &part.ty),
                Some(member) => {
                    let original = part.original.member(member).unwrap_or(Value::Undefined);
                    value_at(&keys, frame, start, &original, &TypeTag::Real)
                }
            };
            let Some(value) = value else { continue };
            let slot = &mut pending[entry.part];
            *slot = match &entry.member {
                None => Some(value),
                Some(member) => slot
                    .as_ref()
                    .unwrap_or(&part.original)
                    .with_member(&part.ty, member, &value)
                    .or(slot.take()),
            };
        }
        // Back before the writes run anything, for what they run to find
        // them; a nested change only writes newer values.
        let held: Vec<(ObjectId, PropIndex, OverrideId)> =
            parts.iter().map(|p| (p.target, p.prop, p.id)).collect();
        let _ = e.with_mut(me, |t: &mut Timeline| {
            t.parts = parts;
            t.entries = entries;
        });
        e.batch(|| {
            for ((target, prop, id), value) in held.into_iter().zip(pending) {
                if let Some(value) = value
                    && e.is_alive(target)
                {
                    let _ = e.write_internal(target, prop, value, WriteKind::Override(id));
                }
            }
        });
    });
}

fn keys_of(g: &KeyframeGroup) -> Rc<[Key]> {
    g.from_file.clone().unwrap_or_else(|| g.written.clone())
}

/// The group's keys or what it names changed: its timeline shows it.
fn tell_timeline(e: &Engine, group: ObjectId) {
    let timeline = e
        .with(group, |g: &KeyframeGroup| g.timeline)
        .ok()
        .flatten()
        .filter(|t| e.is_alive(*t));
    if let Some(timeline) = timeline {
        sync(e, timeline);
    }
}

fn file_keys(found: Vec<keyframes::FileKey>) -> Rc<[Key]> {
    let mut keys: Vec<Key> = found
        .into_iter()
        .map(|k| Key {
            frame: k.frame,
            value: k.value,
            curve: Curve {
                ty: k.easing,
                ..Curve::default()
            },
        })
        .collect();
    keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    keys.into()
}

/// Reads the bytes of a file that came, or says what is wrong with it.
fn arrived(e: &Engine, group: ObjectId, url: &str, load: u64, bytes: Option<Vec<u8>>) {
    let current = e
        .with(group, |g: &KeyframeGroup| g.load == load)
        .unwrap_or(false);
    if !current {
        return;
    }
    let keys = match bytes.as_deref().map(keyframes::parse) {
        Some(Ok(found)) => Some(file_keys(found)),
        Some(Err(corrupt)) => {
            e.warn(format!("Corrupt keyframeSource \"{url}\": {corrupt}"));
            None
        }
        None => {
            e.warn(format!("Unable to open keyframeSource: \"{url}\""));
            None
        }
    };
    let _ = e.with_mut(group, |g: &mut KeyframeGroup| g.from_file = keys);
    tell_timeline(e, group);
}

fn load(e: &Engine, group: ObjectId, source: &str) {
    let Ok(this) = e.with_mut(group, |g: &mut KeyframeGroup| {
        g.load += 1;
        g.from_file = None;
        g.load
    }) else {
        return;
    };
    if source.is_empty() {
        tell_timeline(e, group);
        return;
    }
    let url = e.resolve_url(group, source);
    let platform = Platform::of(e);
    if let Some(bytes) = platform.resource(&url) {
        let bytes = bytes.to_vec();
        arrived(e, group, &url, this, Some(bytes));
        return;
    }
    if is_remote(&url) {
        let shown = url.clone();
        platform.fetch_for(Some(group), &url, move |e, got| {
            arrived(e, group, &shown, this, got.ok());
        });
        return;
    }
    // A timeline starts showing as soon as it is made: its files are small,
    // and read here they are there for the first frame shown.
    let bytes = url_path(&url)
        .and_then(|p| platform.env().real(&p).ok())
        .and_then(|p| std::fs::read(p).ok());
    arrived(e, group, &url, this, bytes);
}

#[qml_impl]
impl KeyframeGroup {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.effect_in(me, move |e| {
            let source = e
                .read(me, KeyframeGroup::KEYFRAME_SOURCE.index)
                .to_js_string();
            e.untrack(|| load(e, me, &source));
        })?;
        // The keys are read here, with what each depends on, so that the
        // timeline's frames read none of it.
        e.effect_in(me, move |e| {
            let _ = e.read(me, KeyframeGroup::TARGET.index);
            let _ = e.read(me, KeyframeGroup::PROPERTY.index);
            let mut keys: Vec<Key> = objects(e.read(me, KeyframeGroup::KEYFRAMES.index))
                .into_iter()
                .filter(|k| e.is::<Keyframe>(*k))
                .map(|k| Key {
                    frame: e.read(k, Keyframe::FRAME.index).to_number(),
                    value: e.read(k, Keyframe::VALUE.index),
                    curve: curve_of(e, k),
                })
                .collect();
            keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
            let _ = e.with_mut(me, |g: &mut KeyframeGroup| g.written = keys.into());
            e.untrack(|| tell_timeline(e, me));
        })?;
        Ok(())
    }
}

#[qml_impl]
impl Timeline {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.effect_in(me, move |e| {
            let _ = e.read(me, Timeline::ENABLED.index);
            let _ = e.read(me, Timeline::START_FRAME.index);
            for group in objects(e.read(me, Timeline::KEYFRAME_GROUPS.index)) {
                let _ = e.with_mut(group, |g: &mut KeyframeGroup| g.timeline = Some(me));
            }
            sync(e, me);
        })?;
        e.watch(me, Timeline::CURRENT_FRAME.index, move |e, _| show(e, me))?;
        e.connect_by_name(me, "destroyed", move |e, _| release(e, me))?;
        Ok(())
    }
}
