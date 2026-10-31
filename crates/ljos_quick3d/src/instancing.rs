//! `Instancing` and the tables of `QtQuick3D`: `InstanceList` with its
//! `InstanceListEntry`s, and `FileInstancing`, read from an XML file.
//!
//! A table is read when a view walks the model it instances, so the walk
//! follows whatever the table is made of. What it read is kept with the
//! table's object: a walk that reads the same entries gets the same
//! [`InstanceTable`], and the view is not drawn again for it.

use std::sync::Arc;

use ljos::item::{self, ljos_3d};
use ljos::ljos_engine::Vector4d;
use ljos::prelude::*;
use ljos_3d::fetch::Fetch;
use ljos_3d::math::{Mat4, Quat, Vec3};
use ljos_3d::{Instance, InstanceTable};

use crate::Object3D;
use crate::fetch::EngineFetch;
use crate::node::{IDENTITY, quat, vec3};
use crate::space::to_value;

/// One copy as QML gives it, kept so the `instance…` methods answer what
/// was written rather than what a matrix decomposes to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub position: Vec3,
    pub scale: Vec3,
    pub rotation: Quat,
    pub color: Color,
    pub custom: [f32; 4],
}

impl Default for Entry {
    fn default() -> Self {
        Entry {
            position: Vec3::ZERO,
            scale: Vec3::ONE,
            rotation: Quat::IDENTITY,
            color: Color::from_rgba8(255, 255, 255, 255),
            custom: [0.0; 4],
        }
    }
}

impl Entry {
    fn instance(&self) -> Instance {
        let [r, g, b, a] = self.color.to_rgba_f();
        let color = ljos_3d::Color {
            r: r as f32,
            g: g as f32,
            b: b as f32,
            a: a as f32,
        };
        Instance {
            transform: Mat4::from_trs(self.position, self.rotation, self.scale),
            color: color.to_linear(),
            custom: self.custom,
        }
    }
}

/// The entries a table was last read as, and the table made of them.
#[derive(Debug, Default)]
pub(crate) struct Kept {
    entries: Vec<Entry>,
    table: Option<Arc<InstanceTable>>,
}

impl Kept {
    /// The table of `entries`: the one kept when they are the same.
    fn table(&mut self, entries: Vec<Entry>) -> Arc<InstanceTable> {
        if let Some(t) = &self.table
            && self.entries == entries
        {
            return t.clone();
        }
        let table = Arc::new(InstanceTable::new(
            entries.iter().map(Entry::instance).collect(),
        ));
        self.entries = entries;
        self.table = Some(table.clone());
        table
    }

    pub(crate) fn entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }
}

/// `Instancing`: what every instance table is. Its methods answer what
/// the table holds at an index; past its end, the defaults.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "Instancing", extends = Object3D, uncreatable)]
pub struct Instancing {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub depth_sorting_enabled: bool,
    #[qml(notify)]
    pub has_transparency: bool,
    /// Draws only this many copies; -1 for all of them.
    #[qml(notify)]
    pub instance_count_override: i32,
    #[qml(skip)]
    pub(crate) kept: Kept,
}

impl Default for Instancing {
    fn default() -> Self {
        Instancing {
            object3d: Object3D::default(),
            depth_sorting_enabled: false,
            has_transparency: false,
            instance_count_override: -1,
            kept: Kept::default(),
        }
    }
}

#[qml_impl]
impl Instancing {
    #[qml(invokable, name = "instancePosition")]
    fn instance_position(cx: &Cx, index: i32) -> Value {
        let e = entry(cx.engine, cx.this, index);
        to_value(e.position)
    }

    #[qml(invokable, name = "instanceScale")]
    fn instance_scale(cx: &Cx, index: i32) -> Value {
        to_value(entry(cx.engine, cx.this, index).scale)
    }

    #[qml(invokable, name = "instanceRotation")]
    fn instance_rotation(cx: &Cx, index: i32) -> Value {
        let q = entry(cx.engine, cx.this, index).rotation;
        Value::Quaternion(Arc::new(Quaternion {
            scalar: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        }))
    }

    #[qml(invokable, name = "instanceColor")]
    fn instance_color(cx: &Cx, index: i32) -> Value {
        Value::Color(entry(cx.engine, cx.this, index).color)
    }

    #[qml(invokable, name = "instanceCustomData")]
    fn instance_custom_data(cx: &Cx, index: i32) -> Value {
        let [x, y, z, w] = entry(cx.engine, cx.this, index).custom;
        Value::Vector4d(Arc::new(Vector4d { x, y, z, w }))
    }
}

/// The entry `index` of the table `obj`, read now.
fn entry(e: &Engine, obj: ObjectId, index: i32) -> Entry {
    if table(e, obj).is_none() {
        return Entry::default();
    }
    usize::try_from(index)
        .ok()
        .and_then(|i| {
            e.with(obj, |t: &Instancing| t.kept.entry(i).cloned())
                .ok()
                .flatten()
        })
        .unwrap_or_default()
}

/// The table `obj` holds now, as a scene draws it; `None` when `obj` is
/// not one. Tracked.
pub(crate) fn table(e: &Engine, obj: ObjectId) -> Option<Arc<InstanceTable>> {
    let entries = if e.is::<InstanceList>(obj) {
        list_entries(e, obj)
    } else if e.is::<FileInstancing>(obj) {
        // Notified when a file arrives; the entries are already kept.
        item::prop(e, obj, FileInstancing::INSTANCE_COUNT);
        return e
            .with(obj, |t: &Instancing| t.kept.table.clone())
            .ok()
            .flatten()
            .or_else(|| Some(Arc::new(InstanceTable::new(Vec::new()))));
    } else if e.is::<crate::random_instancing::RandomInstancing>(obj) {
        crate::random_instancing::entries(e, obj)?
    } else {
        return None;
    };
    e.untrack(|| e.with_mut(obj, |t: &mut Instancing| t.kept.table(entries)))
        .ok()
}

/// The copies the table `obj` holds now, as QML wrote them: what a
/// model of the table shows without asking the GPU. Empty when `obj` is
/// not a table. Tracked.
pub(crate) fn rows(e: &Engine, obj: ObjectId) -> Vec<Entry> {
    if table(e, obj).is_none() {
        return Vec::new();
    }
    e.with(obj, |t: &Instancing| t.kept.entries.clone())
        .unwrap_or_default()
}

/// What a scene's model takes of the table `obj`: everything but the
/// root, which is a scene node. Tracked.
pub(crate) fn instancing(e: &Engine, obj: ObjectId) -> Option<ljos_3d::Instancing> {
    let table = table(e, obj)?;
    let count = item::prop(e, obj, Instancing::INSTANCE_COUNT_OVERRIDE);
    Some(ljos_3d::Instancing {
        depth_sort: item::prop(e, obj, Instancing::DEPTH_SORTING_ENABLED),
        transparent: item::prop(e, obj, Instancing::HAS_TRANSPARENCY),
        count: usize::try_from(count).ok(),
        ..ljos_3d::Instancing::new(table)
    })
}

/// `InstanceListEntry`: one copy of an `InstanceList`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "InstanceListEntry", extends = Object3D)]
pub struct InstanceListEntry {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub position: Vector3d,
    #[qml(notify)]
    pub scale: Vector3d,
    /// Used when `rotation` is not set.
    #[qml(notify)]
    pub euler_rotation: Vector3d,
    #[qml(notify)]
    pub rotation: Quaternion,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub custom_data: Vector4d,
}

impl Default for InstanceListEntry {
    fn default() -> Self {
        InstanceListEntry {
            object3d: Object3D::default(),
            position: Vector3d::default(),
            scale: Vector3d {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            euler_rotation: Vector3d::default(),
            rotation: IDENTITY,
            color: Color::from_rgba8(255, 255, 255, 255),
            custom_data: Vector4d::default(),
        }
    }
}

/// `InstanceList`: a table written in QML, an entry a copy.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "InstanceList",
    extends = Instancing,
    default = "instances"
)]
pub struct InstanceList {
    #[qml(base)]
    pub instancing: Instancing,
    #[qml(notify, list)]
    pub instances: QmlList,
    #[qml(notify, readonly)]
    pub instance_count: i32,
}

#[qml_impl]
impl InstanceList {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine.effect_in(me, move |e| {
            let n = live(e, me).len() as i32;
            e.untrack(|| {
                let _ = e.set_internal(me, InstanceList::INSTANCE_COUNT, n);
            });
        })?;
        Ok(())
    }
}

fn live(e: &Engine, obj: ObjectId) -> Vec<ObjectId> {
    match e.read(obj, InstanceList::INSTANCES.index) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| e.is_alive(*o) && e.is::<InstanceListEntry>(*o))
            .collect(),
        _ => Vec::new(),
    }
}

fn list_entries(e: &Engine, obj: ObjectId) -> Vec<Entry> {
    live(e, obj)
        .into_iter()
        .map(|o| {
            let rotation = item::prop(e, o, InstanceListEntry::ROTATION);
            // An unset rotation is the identity: the Euler angles then say
            // how the copy is turned.
            let rotation = if rotation == IDENTITY {
                Quat::from_euler_degrees(vec3(item::prop(e, o, InstanceListEntry::EULER_ROTATION)))
            } else {
                quat(rotation)
            };
            let c = item::prop(e, o, InstanceListEntry::CUSTOM_DATA);
            Entry {
                position: vec3(item::prop(e, o, InstanceListEntry::POSITION)),
                scale: vec3(e.get(o, InstanceListEntry::SCALE).unwrap_or(Vector3d {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                })),
                rotation,
                color: item::prop(e, o, InstanceListEntry::COLOR),
                custom: [c.x, c.y, c.z, c.w],
            }
        })
        .collect()
}

/// `FileInstancing`: a table read from an XML file. An `InstanceTable`
/// element holds an `Instance` element a copy; other elements and
/// attributes are left out, as the documentation says.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "FileInstancing", extends = Instancing)]
pub struct FileInstancing {
    #[qml(base)]
    pub instancing: Instancing,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify, readonly)]
    pub instance_count: i32,
    #[qml(skip)]
    generation: u64,
}

const CATEGORY: &str = "qt.quick3d.instancing";

#[qml_impl]
impl FileInstancing {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        load(e, me);
        e.watch(me, FileInstancing::SOURCE.index, move |e, _| load(e, me))?;
        Ok(())
    }
}

fn load(e: &Engine, me: ObjectId) {
    let source = e.untrack(|| e.read(me, FileInstancing::SOURCE.index).to_js_string());
    let Ok(generation) = e.with_mut(me, |f: &mut FileInstancing| {
        f.generation += 1;
        f.generation
    }) else {
        return;
    };
    arrived(e, me, Vec::new());
    if source.is_empty() {
        return;
    }
    let url = e.resolve_url(me, &source);
    // The binary table is laid out as Qt uploads it to its GPU, which is
    // not this renderer's layout.
    if url.ends_with(".bin") {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("{source}: binary instance tables are not read; give the XML file"),
        );
        return;
    }
    let got = move |e: &Engine, bytes: Result<Vec<u8>, String>| {
        if !e.is_alive(me) || e.with(me, |f: &FileInstancing| f.generation) != Ok(generation) {
            return;
        }
        match bytes.and_then(|b| parse(&b)) {
            Ok(entries) => arrived(e, me, entries),
            Err(why) => e.log(LogLevel::Warning, CATEGORY, format!("{source}: {why}")),
        }
    };
    EngineFetch::at_once(me).fetch(e, &url, Box::new(got));
}

fn arrived(e: &Engine, me: ObjectId, entries: Vec<Entry>) {
    let n = entries.len() as i32;
    let _ = e.with_mut(me, |t: &mut Instancing| t.kept.table(entries));
    let _ = e.set_internal(me, FileInstancing::INSTANCE_COUNT, n);
    // The same count is a new table all the same.
    let _ = e.notify(me, FileInstancing::INSTANCE_COUNT.index);
}

/// The `Instance` elements of an XML instance table.
pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    use quick_xml::events::{BytesStart, Event};

    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut in_table = false;
    let mut buf = Vec::new();
    let instance = |el: &BytesStart, entries: &mut Vec<Entry>| -> Result<(), String> {
        let mut entry = Entry::default();
        for a in el.attributes() {
            let a = a.map_err(|err| err.to_string())?;
            let value = a
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|err| err.to_string())?;
            match a.key.as_ref() {
                "position" => entry.position = vector3(&value),
                "scale" => entry.scale = vector3(&value),
                "eulerRotation" => entry.rotation = Quat::from_euler_degrees(vector3(&value)),
                // A vector4d, as QQuaternion's: x, y, z, then the scalar.
                "quaternion" => {
                    let [x, y, z, w] = numbers::<4>(&value);
                    entry.rotation = Quat { x, y, z, w };
                }
                "custom" => entry.custom = numbers::<4>(&value),
                "color" => {
                    entry.color = Color::parse(value.trim())
                        .ok_or_else(|| format!("{value:?} is not a colour"))?;
                }
                _ => {}
            }
        }
        entries.push(entry);
        Ok(())
    };
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|err| err.to_string())?
        {
            Event::Start(el) => {
                if depth == 0 {
                    in_table = el.name().as_ref() == "InstanceTable";
                } else if depth == 1 && in_table && el.name().as_ref() == "Instance" {
                    instance(&el, &mut entries)?;
                }
                depth += 1;
            }
            Event::Empty(el) => {
                if depth == 1 && in_table && el.name().as_ref() == "Instance" {
                    instance(&el, &mut entries)?;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if !in_table {
        return Err("the file is not an InstanceTable".to_owned());
    }
    Ok(entries)
}

/// Space-separated numbers; missing trailing ones are zeros.
fn numbers<const N: usize>(s: &str) -> [f32; N] {
    let mut out = [0.0; N];
    for (o, n) in out.iter_mut().zip(s.split_whitespace()) {
        *o = n.parse().unwrap_or(0.0);
    }
    out
}

fn vector3(s: &str) -> Vec3 {
    let [x, y, z] = numbers::<3>(s);
    Vec3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_xml_table_reads_its_instances_and_leaves_out_the_rest() {
        let entries = parse(
            br#"<InstanceTable>
                 <Instance position="1 2" color="red" unknown="x"/>
                 <Instance scale="2 2 2" custom="1" quaternion="0 0 0 1"></Instance>
                 <Other position="9 9 9"/>
               </InstanceTable>"#,
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].position, Vec3::new(1.0, 2.0, 0.0));
        assert_eq!(entries[0].scale, Vec3::ONE);
        assert_eq!(entries[0].color, Color::parse("red").unwrap());
        assert_eq!(entries[1].scale, Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(entries[1].custom, [1.0, 0.0, 0.0, 0.0]);
        assert!(parse(b"<Table/>").is_err());
    }
}
