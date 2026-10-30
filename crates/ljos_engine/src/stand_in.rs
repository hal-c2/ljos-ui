//! Types that a type dump describes and no Rust code provides.
//!
//! An application's `ljostypes.json` says what its native types are, and a
//! program without the application's Rust (`ljos run`, a preview) can still
//! make objects of them: each stands in with the members it declares. Its
//! properties are stored as a QML file's declared ones are, its signals are
//! emitted and handled as a QML file's are, and its methods do nothing.

use rustc_hash::FxHashMap as HashMap;
use std::rc::Rc;

use crate::Cx;
use crate::SingletonScope;
use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::incubator::singletons::Make;
use crate::meta::{MetaBuilder, MetaRef, PropertyFlags};
use crate::registry::{ParamEntry, PropertyEntry, PropertyKind, TypeDump, TypeEntry, Version};
use crate::types::TypeTag;
use crate::value::Value;

/// How deep a dump's chain of bases is followed before it is taken to loop.
const BASE_DEPTH: usize = 64;

/// Why [`Engine::register_stand_ins`] made nothing of a dump.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StandInError {
    /// The type's chain of bases comes back to itself, or is deeper than
    /// any program registers.
    #[error("the bases of {name} loop")]
    BasesLoop { name: String },
    /// The type's base is neither in the dump nor registered.
    #[error("{name}: its base {base} is neither in the type dump nor one of the toolkit's types")]
    UnknownBase { name: String, base: String },
}

/// What [`Engine::register_stand_ins`] made of a dump.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct StandIns {
    /// The types that stand in, as `module.Name`.
    pub types: Vec<String>,
    /// Those that attach a type, as `module.Name` and the attached type's
    /// name. It is not made: an attached type is reached through a plain
    /// function (`MetaObject::attached`), which a description read at run
    /// time cannot be.
    pub attaching: Vec<(String, String)>,
}

/// What a method of a stand-in does, whatever it is called with.
fn does_nothing(_: &Cx<'_>, _: &[Value]) -> Result<Value, EngineError> {
    Ok(Value::Undefined)
}

fn flags_of(p: &PropertyEntry) -> PropertyFlags {
    let mut flags = PropertyFlags::empty();
    for (set, flag) in [
        (p.readonly, PropertyFlags::READONLY),
        (p.is_final, PropertyFlags::FINAL),
        (p.required, PropertyFlags::REQUIRED),
        (p.constant, PropertyFlags::CONSTANT),
        (p.list, PropertyFlags::LIST),
    ] {
        if set {
            flags |= flag;
        }
    }
    flags
}

/// A property's type: a native list says what it holds in `type` alone,
/// where a declared one writes `list<T>`.
fn tag_of(p: &PropertyEntry) -> TypeTag {
    if p.kind == PropertyKind::List && !p.ty.starts_with("list<") {
        return TypeTag::List(Some(Atom::intern(&p.ty)));
    }
    p.type_tag()
}

fn params_of(params: &[ParamEntry]) -> Vec<(Atom, TypeTag)> {
    params
        .iter()
        .map(|p| (Atom::intern(&p.name), TypeTag::parse(&p.ty)))
        .collect()
}

struct Builder<'a> {
    engine: &'a Engine,
    dump: &'a TypeDump,
    /// What was built, by the entry it was built from.
    built: HashMap<*const TypeEntry, MetaRef>,
}

impl Builder<'_> {
    /// The base of `entry`: one the dump describes, or a registered one.
    /// A type that names none builds on `QtObject`, as a native type a
    /// module exports does.
    fn base(&mut self, entry: &TypeEntry, depth: usize) -> Result<MetaRef, StandInError> {
        if let Some((uri, base)) = self.dump.base_of(entry) {
            let registered = self.engine.lookup_type(uri, &base.name);
            return match registered {
                Some(meta) => Ok(meta),
                None => self.build(uri, base, depth + 1),
            };
        }
        let unknown = |base: &str| StandInError::UnknownBase {
            name: entry.name.clone(),
            base: base.to_owned(),
        };
        let Some(base) = &entry.base else {
            return self
                .engine
                .lookup_type("QtQml", "QtObject")
                .ok_or_else(|| unknown("QtObject"));
        };
        match &entry.base_module {
            Some(uri) => self.engine.lookup_type(uri, base),
            None => self.engine.lookup_type_any(base),
        }
        .ok_or_else(|| unknown(base))
    }

    fn build(
        &mut self,
        uri: &str,
        entry: &TypeEntry,
        depth: usize,
    ) -> Result<MetaRef, StandInError> {
        if depth > BASE_DEPTH {
            return Err(StandInError::BasesLoop {
                name: entry.name.clone(),
            });
        }
        if let Some(meta) = self.built.get(&std::ptr::from_ref(entry)) {
            return Ok(meta.clone());
        }
        let base = self.base(entry, depth)?;
        let meta = self.builder(uri, entry, base).build_shared();
        self.built.insert(std::ptr::from_ref(entry), meta.clone());
        Ok(meta)
    }

    fn builder(&self, uri: &str, entry: &TypeEntry, base: MetaRef) -> MetaBuilder {
        let mut b = MetaBuilder::new(&entry.name).base(base);
        if !uri.is_empty() {
            b = b.module(uri);
        }
        for e in &entry.enums {
            let keys: Vec<(&str, i64)> = e.keys.iter().map(|(k, v)| (k.as_str(), *v)).collect();
            b = b.enumeration(&e.name, &keys);
        }
        for p in &entry.properties {
            b = match p.kind {
                PropertyKind::Group => self.group(b, p),
                _ => b.property(&p.name, tag_of(p), flags_of(p), p.notify.is_some()),
            };
        }
        for s in &entry.signals {
            // A write says it through the property's own change signal.
            let implied = entry
                .properties
                .iter()
                .any(|p| p.notify.as_ref() == Some(&s.name));
            if !implied {
                b = b.signal(&s.name, params_of(&s.parameters));
            }
        }
        for m in &entry.methods {
            let ret = TypeTag::parse(&m.return_type);
            b = b.method(&m.name, params_of(&m.parameters), ret, Some(does_nothing));
        }
        if let Some(default) = &entry.default_property {
            b = b.default_property(default);
        }
        if let Some(block) = &entry.block {
            b = b.block(block.category.as_deref(), block.summary.as_deref());
        }
        if !entry.creatable {
            b = b.uncreatable();
        }
        b
    }

    /// A grouped property, with the members its type has in the dump.
    fn group(&self, b: MetaBuilder, p: &PropertyEntry) -> MetaBuilder {
        let leaves = self
            .dump
            .modules
            .iter()
            .flat_map(|m| &m.types)
            .chain(&self.dump.unnamed)
            .find(|t| t.name == p.ty)
            .map(|t| t.properties.as_slice())
            .unwrap_or_default();
        b.dynamic_group(
            &p.name,
            &p.ty,
            leaves
                .iter()
                .map(|l| (l.name.clone(), tag_of(l), flags_of(l))),
        )
    }
}

impl Engine {
    /// Registers every type of `dump`'s modules that nothing has registered
    /// under its name, as one that stands in with the members it declares.
    pub fn register_stand_ins(&self, dump: &TypeDump) -> Result<StandIns, StandInError> {
        let mut builder = Builder {
            engine: self,
            dump,
            built: HashMap::default(),
        };
        let mut made = StandIns::default();
        for module in &dump.modules {
            for entry in &module.types {
                if self.lookup_type(&module.uri, &entry.name).is_some() {
                    continue;
                }
                let version = Version::parse(&entry.since).unwrap_or_default();
                if entry.singleton {
                    self.register_stand_in_singleton(&mut builder, &module.uri, entry, version)?;
                } else {
                    let meta = builder.build(&module.uri, entry, 0)?;
                    self.register_type(&module.uri, version, meta);
                }
                let name = format!("{}.{}", module.uri, entry.name);
                if let Some(attached) = &entry.attached {
                    made.attaching.push((name.clone(), attached.clone()));
                }
                made.types.push(name);
            }
        }
        Ok(made)
    }

    /// A singleton's description has to outlive the engine's table of
    /// them, so it is kept for the life of the process: a dump is read once.
    fn register_stand_in_singleton(
        &self,
        builder: &mut Builder<'_>,
        uri: &str,
        entry: &TypeEntry,
        version: Version,
    ) -> Result<(), StandInError> {
        let base = builder.base(entry, 0)?;
        let name = || entry.name.clone();
        let create = base.create.ok_or_else(|| StandInError::UnknownBase {
            name: name(),
            base: base.name.to_string(),
        })?;
        let meta = Box::leak(Box::new(builder.builder(uri, entry, base).build()));
        let make: Make = Rc::new(move |_| create());
        self.register_singleton_meta(uri, version, meta, make, SingletonScope::Engine);
        Ok(())
    }
}
