//! Read-only views of a running engine, for development tools (the
//! inspector of ARCHITECTURE.md §19.4): what an object is and where it was
//! written, and for each property its value, where the value came from and
//! which tier its binding was compiled for.
//!
//! Nothing here writes to the engine, and nothing here is on a hot path:
//! the answers are worked out from the loaded IR when asked for, so an
//! application that is not being inspected pays nothing for them.

use std::rc::Rc;

use ljos_ir::{CompiledFile, KeySeg, Members, ObjectDef, PropertyBinding};
use ljos_reactive::Provenance;

use crate::engine::Engine;
use crate::ids::{ObjectId, PropIndex};
use crate::log::LogEntry;
use crate::meta::{PropertyAccess, PropertyFlags};
use crate::object::Location;
use crate::value::Value;

/// A place in a QML file. `line` and `column` are 1-based; `column` is 0
/// when only the line is known.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourcePlace {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl SourcePlace {
    fn of(l: &Location) -> Self {
        SourcePlace {
            file: l.url.to_string(),
            line: l.line,
            column: l.column,
        }
    }
}

/// How a binding is run: as a property path in Rust (tier 0), as register
/// bytecode (tier 1) or as a JS function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingTier {
    Tier0,
    Tier1,
    Js,
}

impl BindingTier {
    /// `"0"`, `"1"` or `"js"`.
    pub fn as_str(self) -> &'static str {
        match self {
            BindingTier::Tier0 => "0",
            BindingTier::Tier1 => "1",
            BindingTier::Js => "js",
        }
    }
}

/// What an object is. See [`Engine::inspect_object`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectReport {
    /// The object's type, then the types it is made from, nearest first.
    pub types: Vec<String>,
    /// The `id` the object was given where it is declared.
    pub id: Option<String>,
    /// The object's stable key (ARCHITECTURE.md §12): the same for the
    /// object that takes its place after a reload.
    pub key: Option<String>,
    pub declared_at: Option<SourcePlace>,
}

/// One property of an object. See [`Engine::inspect_properties`].
#[derive(Clone, Debug)]
pub struct PropertyReport {
    /// `width`, or `border.width` for a member of a grouped property.
    pub name: String,
    /// The declared type, as QML writes it.
    pub ty: String,
    /// `None` when reading it failed.
    pub value: Option<Value>,
    /// For an enum, the value as QML writes it: `Text.AlignHCenter`. See
    /// [`MetaObject::key_text`](crate::meta::MetaObject::key_text).
    pub key: Option<String>,
    pub provenance: Provenance,
    /// For a bound property whose binding was written in a file.
    pub tier: Option<BindingTier>,
    /// Where the binding was written.
    pub written_at: Option<SourcePlace>,
    /// The bytes of the binding's expression in the file it was written
    /// in, for a tool that shows its text.
    pub expression: Option<std::ops::Range<u32>>,
    pub read_only: bool,
    pub is_list: bool,
}

impl Engine {
    /// What `obj` is and where it was written. `None` for a dead object.
    pub fn inspect_object(&self, obj: ObjectId) -> Option<ObjectReport> {
        let meta = self.meta_of(obj)?;
        let mut types = vec![meta.name.to_string()];
        let mut base = meta.base.clone();
        while let Some(b) = base {
            types.push(b.name.to_string());
            base = b.base.clone();
        }
        let at = self.place_of(obj);
        let (id, ir_key) = at
            .as_ref()
            .and_then(|at| {
                self.with_object_def(at, |file, key, def| (def.id.clone(), ir_key(file, key)))
            })
            .unwrap_or((None, None));
        Some(ObjectReport {
            types,
            id,
            // The reload path knows of delegates and of components used as
            // types; the key from the IR is the object's place in its own
            // file, which is all there is to say without tracking.
            key: self.reload_path(obj).or(ir_key),
            declared_at: at,
        })
    }

    /// A short name for `obj` that a person can find it by: its type and
    /// `objectName` (`Item "content"`), else its type and `id`
    /// (`Timer#tick`), else its type and where it is written
    /// (`Timer (Main.qml:12)`). `None` for a dead object.
    pub fn label_of(&self, obj: ObjectId) -> Option<String> {
        let report = self.inspect_object(obj)?;
        let ty = report.types.first().map_or("object", |t| t.as_str());
        let object_name = self
            .untrack(|| self.read_by_name(obj, "objectName"))
            .to_js_string()
            .to_string();
        Some(if !object_name.is_empty() && object_name != "undefined" {
            format!("{ty} \"{object_name}\"")
        } else if let Some(id) = &report.id {
            format!("{ty}#{id}")
        } else if let Some(at) = &report.declared_at {
            let file = at.file.rsplit('/').next().unwrap_or(&at.file);
            format!("{ty} ({file}:{})", at.line)
        } else {
            ty.to_owned()
        })
    }

    /// The `id` each of `objs` was given where it is declared, in the same
    /// order. One pass over the loaded files however many objects are
    /// asked for, which is what a snapshot of the whole tree needs.
    pub fn inspect_ids(&self, objs: &[ObjectId]) -> Vec<Option<String>> {
        let mut ids: std::collections::HashMap<(&str, u32, u32), &str> =
            std::collections::HashMap::new();
        let loader = self.inner.loader.borrow();
        for file in loader.files.values() {
            for comp in &file.ir.components {
                for def in &comp.objects {
                    if let Some(id) = &def.id {
                        ids.insert((&*file.url, def.span.line, def.span.column), id);
                    }
                }
            }
        }
        let store = self.inner.store.borrow();
        objs.iter()
            .map(|obj| {
                let l = store.objects.get(*obj)?.location()?;
                ids.get(&(&*l.url, l.line, l.column))
                    .map(|id| id.to_string())
            })
            .collect()
    }

    /// Every property of `obj` (inherited first, as its type lists them),
    /// read without being tracked. A grouped property is given by its
    /// members. Empty for a dead object.
    pub fn inspect_properties(&self, obj: ObjectId) -> Vec<PropertyReport> {
        let Some(meta) = self.meta_of(obj) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(meta.properties.len());
        for (i, info) in meta.properties.iter().enumerate() {
            if matches!(info.access, PropertyAccess::GroupRoot)
                || info.flags.contains(PropertyFlags::GROUP)
            {
                continue;
            }
            let prop = PropIndex(i as u32);
            if self.is_script_property(obj, prop) {
                // A statement kept to be run, not a value.
                continue;
            }
            let value = self.untrack(|| self.try_read(obj, prop).ok());
            let key = value.as_ref().and_then(|v| meta.key_text(&info.ty, v));
            let provenance = self.provenance(obj, prop).unwrap_or(Provenance::Default);
            let loc = self.binding_location(obj, prop);
            let (tier, expression) = match (&loc, self.is_bound(obj, prop)) {
                (Some(l), true) => self.binding_written_at(l).unzip(),
                _ => (None, None),
            };
            out.push(PropertyReport {
                name: info.name.to_string(),
                ty: info.ty.name(),
                value,
                key,
                provenance,
                tier,
                written_at: loc.as_ref().map(SourcePlace::of),
                expression,
                read_only: info.is_readonly(),
                is_list: info.is_list(),
            });
        }
        out
    }

    /// `v`, a value of the property `prop` of `obj`, as QML writes it when
    /// the property holds an enum: `Text.AlignHCenter`. See
    /// [`MetaObject::key_text`](crate::meta::MetaObject::key_text).
    pub fn key_text(&self, obj: ObjectId, prop: PropIndex, v: &Value) -> Option<String> {
        let meta = self.meta_of(obj)?;
        meta.key_text(&meta.property(prop)?.ty, v)
    }

    /// Where the JS that is running right now was written: the innermost
    /// frame that is in a QML file. `None` when no JS is running. A log
    /// handler calls this to learn where a `console.log` was written.
    pub fn js_origin(&self) -> Option<SourcePlace> {
        let js = self.inner.js.borrow().clone()?;
        // Only inside a run: entering the host from outside one would start
        // a run of its own, with the pending jobs that brings.
        if !js.host.in_run() {
            return None;
        }
        let slice = js.host.config().slice;
        let stack: String = js
            .host
            .enter("origin", slice, |s| s.ctx().eval("new Error().stack"))
            .ok()?;
        let lines = js.lines.borrow();
        stack.lines().find_map(|frame| {
            let (module, line) = frame_place(frame)?;
            let (file, line) = lines.map(module, line)?;
            Some(SourcePlace {
                file,
                line,
                column: 0,
            })
        })
    }

    /// Like [`Engine::add_log_listener`], and the listener is also told where
    /// the message was written when JS wrote it (`console.log`). Finding
    /// the place costs a stack trace per message, so only a tool that
    /// shows it (the inspector's `app_logs`) asks for it.
    pub fn add_log_listener_with_origin(
        &self,
        mut f: impl FnMut(&LogEntry, Option<&SourcePlace>) + 'static,
    ) -> crate::log::LogListenerId {
        // Weak: the engine owns the listener, and a listener that owned the
        // engine would keep it alive for ever.
        let weak = Rc::downgrade(&self.inner);
        self.add_log_listener(move |entry| {
            let at = weak
                .upgrade()
                .and_then(|inner| Engine::from_inner(inner).js_origin());
            f(entry, at.as_ref());
        })
    }

    fn place_of(&self, obj: ObjectId) -> Option<SourcePlace> {
        let store = self.inner.store.borrow();
        let l = store.objects.get(obj)?.location()?;
        Some(SourcePlace::of(&l))
    }

    fn binding_location(&self, obj: ObjectId, prop: PropIndex) -> Option<Location> {
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        Some(store.objects.get(obj)?.binding_loc(prop)?.at)
    }

    /// Calls `f` with the declaration written at `at`.
    fn with_object_def<R>(
        &self,
        at: &SourcePlace,
        f: impl FnOnce(&CompiledFile, (usize, usize), &ObjectDef) -> R,
    ) -> Option<R> {
        let loader = self.inner.loader.borrow();
        for file in loader.files.values() {
            if *file.url != *at.file {
                continue;
            }
            for (ci, comp) in file.ir.components.iter().enumerate() {
                for (oi, def) in comp.objects.iter().enumerate() {
                    if def.span.line == at.line && def.span.column == at.column {
                        return Some(f(&file.ir, (ci, oi), def));
                    }
                }
            }
        }
        None
    }

    /// The tier the binding written at `at` was compiled for, and the bytes
    /// of its expression. A tier-1 program that gave up at run time still
    /// counts as tier 1 here.
    fn binding_written_at(&self, at: &Location) -> Option<(BindingTier, std::ops::Range<u32>)> {
        let loader = self.inner.loader.borrow();
        for file in loader.files.values() {
            if file.url != at.url {
                continue;
            }
            let found = file.ir.components.iter().find_map(|comp| {
                comp.objects
                    .iter()
                    .find_map(|def| binding_at(&file.ir, def, at))
            });
            let Some(pb) = found else {
                continue;
            };
            let b = file.ir.expr(pb.expr);
            let tier = if matches!(file.tier0.get(b.js_fn.index()), Some(Some(_))) {
                BindingTier::Tier0
            } else if b.tier1.is_some() && self.tier1() {
                BindingTier::Tier1
            } else {
                BindingTier::Js
            };
            return Some((tier, b.span.start..b.span.end));
        }
        None
    }
}

fn binding_at<'a>(
    ir: &'a CompiledFile,
    def: &'a ObjectDef,
    at: &Location,
) -> Option<&'a PropertyBinding> {
    let here = |m: &'a Members| {
        m.bindings.iter().find(|pb| {
            // As the incubator records it: the place of the name, or of the
            // expression when the name has none.
            let span = if pb.name_span.line == 0 {
                ir.expr(pb.expr).span
            } else {
                pb.name_span
            };
            span.line == at.line && span.column == at.column
        })
    };
    here(&def.members)
        .or_else(|| def.grouped.iter().find_map(|g| here(&g.members)))
        .or_else(|| def.attached.iter().find_map(|a| here(&a.members)))
}

/// The object's place in its own file, written as reload paths are.
fn ir_key(file: &CompiledFile, (ci, oi): (usize, usize)) -> Option<String> {
    let comp = file.components.get(ci)?;
    let mut segments = Vec::new();
    let mut cur = comp.objects.get(oi);
    while let Some(def) = cur {
        let Some(parent) = def.parent else {
            break;
        };
        segments.push(match &def.key {
            KeySeg::Id(id) => format!("#{id}"),
            KeySeg::Named(name) => format!("'{name}'"),
            KeySeg::ModelRow(row) => format!("[{row}]"),
            KeySeg::Typed { ty, nth } => format!("{ty}[{nth}]"),
        });
        cur = comp.objects.get(parent.index());
    }
    segments.reverse();
    let mut key = format!("{}|{}", file.file, comp.id);
    for s in segments {
        key.push('/');
        key.push_str(&s);
    }
    Some(key)
}

/// `at fn (module:line:col)` or `at module:line:col`: the module and line.
pub(crate) fn frame_place(frame: &str) -> Option<(&str, u32)> {
    let frame = frame.trim().strip_prefix("at ")?;
    let inner = match (frame.rfind('('), frame.ends_with(')')) {
        (Some(open), true) => &frame[open + 1..frame.len() - 1],
        _ => frame,
    };
    let mut parts = inner.rsplitn(3, ':');
    let (_col, line, module) = (parts.next()?, parts.next()?, parts.next()?);
    Some((module, line.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::frame_place;

    #[test]
    fn frames_are_read_with_and_without_a_function_name() {
        assert_eq!(frame_place("    at shout (Main:12:30)"), Some(("Main", 12)));
        assert_eq!(frame_place("at a/b.qml:7:1"), Some(("a/b.qml", 7)));
        assert_eq!(frame_place("    at log (native)"), None);
        assert_eq!(frame_place("Error"), None);
    }
}
