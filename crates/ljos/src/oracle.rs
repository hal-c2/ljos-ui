//! The [`EngineOracle`]: what the compiler is told of the native types an
//! Engine registers, beside the table of types.

use std::collections::HashSet;
use std::sync::Arc;

use ljos_engine::{
    Engine, MetaObject, MetaRef, PropertyAccess, PropertyFlags, TypeDump, TypeEntry, TypeTag,
};
use qml_front::{Property as FrontProperty, QtType, QtTypeOracle, Special, TypeOracle};

/// One type as the compiler sees it in a render with backends: of the
/// table of types, or native (an index into the oracle's own list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineType {
    Qt(QtType),
    Native(u32),
}

/// A [`TypeOracle`] over the table of types plus native types the table
/// does not describe. An application's native types do not reach the
/// table (ARCHITECTURE.md D1), so a host that compiles QML naming them
/// has to say what they are itself: from the Engine that registers them
/// ([`from_engine`](Self::from_engine)), or from the type dump that Engine
/// wrote ([`from_dump`](Self::from_dump)), which a build script reads
/// without running the program. A type the table knows stays the
/// table's — its description is the fuller one (enum numbers, flags), and
/// an example that uses no backend compiles exactly as before.
#[derive(Clone)]
pub struct EngineOracle {
    /// The native types the table lacks, and the group and attached types
    /// reachable from them (with no module: reachable, not named).
    /// Shared: a project reloaded for a text clones the oracle, and the
    /// types never change once read.
    natives: Arc<[NativeType]>,
}

/// One native type as the compiler asks about it, its bases' members
/// included.
#[derive(Clone, Debug, Default)]
struct NativeType {
    module: String,
    name: String,
    /// Registered as one object ([`Engine::register_singleton`]): a file
    /// reads its members, it does not make one.
    singleton: bool,
    /// Inherited first; a later one of a name shadows an earlier.
    properties: Vec<NativeProperty>,
    signals: Vec<(String, Vec<String>)>,
    methods: Vec<String>,
    default_property: Option<String>,
    attached: Option<String>,
    /// Inherited first.
    enums: Vec<(String, Vec<(String, i64)>)>,
    /// Group and attached types reached through the type's toolkit bases,
    /// which no dump lists.
    reached: Vec<MetaRef>,
    /// A name written on it that it lacks is an error
    /// ([`TypeOracle::members_checked`]).
    checked: bool,
    /// The toolkit type it extends, where Qt's table describes that type:
    /// a group it has from there (`anchors`) is the table's.
    toolkit: Option<QtType>,
}

#[derive(Clone, Debug)]
struct NativeProperty {
    name: String,
    ty: TypeTag,
    list: bool,
    readonly: bool,
    required: bool,
}

impl NativeType {
    fn from_meta(meta: &MetaObject, module: String, singleton: bool) -> NativeType {
        NativeType {
            checked: !module.is_empty(),
            module,
            name: meta.name.to_string(),
            singleton,
            properties: meta
                .properties
                .iter()
                .map(|p| NativeProperty {
                    name: p.name.to_string(),
                    ty: p.ty.clone(),
                    list: p.is_list(),
                    readonly: p.is_readonly(),
                    required: p.flags.contains(PropertyFlags::REQUIRED),
                })
                .collect(),
            signals: meta
                .signals
                .iter()
                .map(|s| {
                    let params = s.params.iter().map(|(n, _)| n.to_string()).collect();
                    (s.name.to_string(), params)
                })
                .collect(),
            methods: meta.methods.iter().map(|m| m.name.to_string()).collect(),
            default_property: meta.default_property().map(|a| a.to_string()),
            attached: meta.attached.map(|f| f().name.to_string()),
            enums: meta
                .enums
                .iter()
                .map(|e| {
                    let keys = e.keys.iter().map(|(k, v)| (k.to_string(), *v)).collect();
                    (e.name.to_string(), keys)
                })
                .collect(),
            reached: reaches(meta),
            toolkit: None,
        }
    }

    /// `entry` over its base `base`, as the Engine that wrote the dump
    /// would have flattened it.
    fn from_entry(entry: &TypeEntry, module: String, base: Option<NativeType>) -> NativeType {
        let mut ty = base.unwrap_or_default();
        ty.checked = !module.is_empty();
        ty.module = module;
        ty.name.clone_from(&entry.name);
        ty.singleton = entry.singleton;
        for p in &entry.properties {
            ty.properties.push(NativeProperty {
                name: p.name.clone(),
                ty: p.type_tag(),
                list: p.list,
                readonly: p.readonly,
                required: p.required,
            });
        }
        // The dump leaves out the change signals the Engine makes for a
        // property; a handler of one is as good as any other. A property
        // may name a signal it inherits, whose arguments stay.
        for s in entry.own_signals() {
            let inherited = entry.signals.iter().all(|own| own.name != s.name)
                && ty.signals.iter().any(|(name, _)| *name == s.name);
            if !inherited {
                let params = s.parameters.iter().map(|p| p.name.clone()).collect();
                ty.signals.push((s.name.clone(), params));
            }
        }
        ty.methods
            .extend(entry.methods.iter().map(|m| m.name.clone()));
        if entry.default_property.is_some() {
            ty.default_property.clone_from(&entry.default_property);
        }
        ty.attached.clone_from(&entry.attached);
        ty.enums
            .extend(entry.enums.iter().map(|e| (e.name.clone(), e.keys.clone())));
        ty
    }

    fn property(&self, name: &str) -> Option<&NativeProperty> {
        self.properties.iter().rev().find(|p| p.name == name)
    }

    fn signal(&self, name: &str) -> Option<&[String]> {
        self.signals
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, params)| params.as_slice())
    }

    fn enumeration(&self, name: &str) -> Option<&[(String, i64)]> {
        self.enums
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, keys)| keys.as_slice())
    }

    fn enum_key(&self, key: &str) -> Option<i64> {
        self.enums
            .iter()
            .rev()
            .find_map(|(_, keys)| keys.iter().find(|(k, _)| k == key).map(|(_, v)| *v))
    }
}

impl std::fmt::Debug for EngineOracle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineOracle")
            .field("natives", &self.natives.len())
            .finish()
    }
}

impl EngineOracle {
    /// The table of types alone: with no example a render compiles
    /// exactly as it did before `--example` existed.
    pub fn table_only() -> Self {
        EngineOracle {
            natives: Arc::default(),
        }
    }

    pub fn from_engine(engine: &Engine) -> Self {
        let qt = QtTypeOracle;
        let mut natives: Vec<NativeType> = Vec::new();
        for module in engine.type_dump().modules {
            for ty in &module.types {
                if qt.module_type(&module.uri, &ty.name).is_some() {
                    continue;
                }
                if let Some(meta) = engine.lookup_type(&module.uri, &ty.name) {
                    natives.push(NativeType::from_meta(
                        &meta,
                        module.uri.clone(),
                        ty.singleton,
                    ));
                }
            }
        }
        Self::with_reached(natives, &table_reached(engine))
    }

    /// The native types `dump` describes ([`Engine::type_dump`], written
    /// by the program that registers them) that the table lacks. A base
    /// the dump does not have is one of the toolkit's, read from `engine`,
    /// an Engine with the standard modules. `Err` names a base neither has.
    pub fn from_dump(dump: &TypeDump, engine: &Engine) -> Result<Self, NativeTypeError> {
        let qt = QtTypeOracle;
        let mut natives: Vec<NativeType> = Vec::new();
        for module in &dump.modules {
            for ty in &module.types {
                if qt.module_type(&module.uri, &ty.name).is_some() {
                    continue;
                }
                natives.push(flatten(dump, engine, ty, module.uri.clone(), 0)?);
            }
        }
        for ty in &dump.unnamed {
            natives.push(flatten(dump, engine, ty, String::new(), 0)?);
        }
        Ok(Self::with_reached(natives, &table_reached(engine)))
    }

    /// `natives` and the group and attached types reachable from them:
    /// a grouped property's type and an attached type are types the
    /// compiler is asked about too, and no module exports them by a name
    /// of their own. Those are checked as the application's types are,
    /// except the ones the table's types reach (`table`): a toolkit's
    /// group, as `anchors` of an application's `Item`, is left as the
    /// table's types are.
    fn with_reached(mut natives: Vec<NativeType>, table: &HashSet<String>) -> Self {
        let mut seen: Vec<MetaRef> = Vec::new();
        let mut i = 0;
        while i < natives.len() {
            for meta in std::mem::take(&mut natives[i].reached) {
                if seen.iter().any(|m| MetaRef::ptr_eq(m, &meta))
                    || natives.iter().any(|n| n.name == meta.name.as_str())
                {
                    continue;
                }
                seen.push(meta.clone());
                natives.push(NativeType::from_meta(&meta, String::new(), false));
            }
            i += 1;
        }
        for native in &mut natives {
            if native.module.is_empty() {
                native.checked = !table.contains(&native.name);
            }
        }
        EngineOracle {
            natives: natives.into(),
        }
    }

    fn native(&self, index: u32) -> &NativeType {
        &self.natives[index as usize]
    }

    fn named(&self, name: &str) -> Option<EngineType> {
        self.natives
            .iter()
            .position(|n| n.name == name)
            .map(|i| EngineType::Native(i as u32))
    }
}

/// The types of `meta`'s grouped properties, and the type it attaches.
fn reaches(meta: &MetaObject) -> Vec<MetaRef> {
    let mut reached: Vec<MetaRef> = meta
        .properties
        .iter()
        .filter_map(|p| match p.access {
            PropertyAccess::Group { meta, .. } => Some(MetaRef::Static(meta)),
            _ => None,
        })
        .collect();
    reached.extend(meta.attached.map(|f| f()));
    reached
}

/// The names of the group and attached types the table's types reach
/// in `engine`, however deep: the toolkit's, which the table describes
/// only in part.
fn table_reached(engine: &Engine) -> HashSet<String> {
    let qt = QtTypeOracle;
    let mut todo: Vec<MetaRef> = engine
        .type_dump()
        .modules
        .iter()
        .flat_map(|module| {
            module
                .types
                .iter()
                .filter(|ty| qt.module_type(&module.uri, &ty.name).is_some())
                .filter_map(|ty| engine.lookup_type(&module.uri, &ty.name))
        })
        .collect();
    let mut names = HashSet::new();
    while let Some(meta) = todo.pop() {
        for next in reaches(&meta) {
            if names.insert(next.name.to_string()) {
                todo.push(next);
            }
        }
    }
    names
}

/// Why [`EngineOracle::from_dump`] cannot describe a type of the dump.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NativeTypeError {
    /// The type's chain of bases comes back to itself, or is deeper than
    /// any program registers.
    #[error("the bases of {name} loop")]
    BasesLoop { name: String },
    /// The type's base is neither in the dump nor a toolkit type.
    #[error("{name}: its base {base} is neither in the type dump nor one of the toolkit's types")]
    UnknownBase { name: String, base: String },
}

/// How deep a dump's chain of bases is followed before it is taken to
/// loop.
const BASE_DEPTH: usize = 64;

/// `entry` with its bases' members: a base the dump has, or one the
/// toolkit registers on `engine`.
fn flatten(
    dump: &TypeDump,
    engine: &Engine,
    entry: &TypeEntry,
    module: String,
    depth: usize,
) -> Result<NativeType, NativeTypeError> {
    if depth > BASE_DEPTH {
        return Err(NativeTypeError::BasesLoop {
            name: entry.name.clone(),
        });
    }
    let Some(base) = &entry.base else {
        return Ok(NativeType::from_entry(entry, module, None));
    };
    let in_dump = dump
        .modules
        .iter()
        .filter(|m| entry.base_module.as_ref().is_none_or(|b| *b == m.uri))
        .find_map(|m| Some((m.uri.clone(), m.types.iter().find(|t| &t.name == base)?)))
        .or_else(|| {
            let t = dump.unnamed.iter().find(|t| &t.name == base)?;
            Some((String::new(), t))
        });
    let base = match in_dump {
        Some((uri, t)) => flatten(dump, engine, t, uri, depth + 1)?,
        None => {
            let meta = match &entry.base_module {
                Some(uri) => engine.lookup_type(uri, base),
                None => engine.lookup_type_any(base),
            }
            .ok_or_else(|| NativeTypeError::UnknownBase {
                name: entry.name.clone(),
                base: base.clone(),
            })?;
            let mut base = NativeType::from_meta(&meta, String::new(), false);
            base.toolkit = entry
                .base_module
                .as_deref()
                .and_then(|uri| QtTypeOracle.module_type(uri, &meta.name));
            base
        }
    };
    Ok(NativeType::from_entry(entry, module, Some(base)))
}

fn qt_property(property: FrontProperty<QtType>) -> FrontProperty<EngineType> {
    FrontProperty {
        is_component: property.is_component,
        is_url: property.is_url,
        is_text: property.is_text,
        is_script: property.is_script,
        takes_key: property.takes_key,
        is_list: property.is_list,
        is_readonly: property.is_readonly,
        value: property.value.map(EngineType::Qt),
    }
}

impl TypeOracle for EngineOracle {
    type Type = EngineType;

    fn has_module(&self, uri: &str) -> bool {
        QtTypeOracle.has_module(uri) || self.natives.iter().any(|n| n.module == uri)
    }

    fn module_type(&self, uri: &str, name: &str) -> Option<EngineType> {
        if let Some(ty) = QtTypeOracle.module_type(uri, name) {
            return Some(EngineType::Qt(ty));
        }
        self.natives
            .iter()
            .position(|n| n.module == uri && n.name == name)
            .map(|i| EngineType::Native(i as u32))
    }

    fn name(&self, ty: EngineType) -> &str {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.name(ty),
            EngineType::Native(i) => &self.native(i).name,
        }
    }

    fn special(&self, ty: EngineType) -> Option<Special> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.special(ty),
            EngineType::Native(_) => None,
        }
    }

    fn is_singleton(&self, ty: EngineType) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.is_singleton(ty),
            EngineType::Native(i) => self.native(i).singleton,
        }
    }

    fn property(&self, ty: EngineType, name: &str) -> Option<FrontProperty<EngineType>> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.property(ty, name).map(qt_property),
            EngineType::Native(i) => {
                let native = self.native(i);
                let info = native.property(name)?;
                // A group the toolkit type has, as the table has it.
                if let (TypeTag::Group(n), Some(qt)) = (&info.ty, native.toolkit)
                    && self
                        .named(n.as_str())
                        .is_none_or(|g| !self.members_checked(g))
                    && let Some(p) = QtTypeOracle.property(qt, name)
                {
                    return Some(qt_property(p));
                }
                Some(FrontProperty {
                    is_component: matches!(&info.ty, TypeTag::Object(Some(n)) if n.as_str() == "Component"),
                    is_url: info.ty == TypeTag::Url,
                    is_text: info.ty == TypeTag::String,
                    is_script: false,
                    takes_key: info.ty == TypeTag::Int && !info.list,
                    is_list: info.list || matches!(info.ty, TypeTag::List(_)),
                    is_readonly: info.readonly,
                    value: match &info.ty {
                        TypeTag::Group(n) | TypeTag::Object(Some(n)) | TypeTag::List(Some(n)) => {
                            self.named(n.as_str())
                        }
                        _ => None,
                    },
                })
            }
        }
    }

    fn signal(&self, ty: EngineType, name: &str) -> Option<Vec<String>> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.signal(ty, name),
            EngineType::Native(i) => self.native(i).signal(name).map(<[String]>::to_vec),
        }
    }

    fn has_method(&self, ty: EngineType, name: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.has_method(ty, name),
            EngineType::Native(i) => self.native(i).methods.iter().any(|m| m == name),
        }
    }

    fn default_property(&self, ty: EngineType) -> Option<&str> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.default_property(ty),
            EngineType::Native(i) => self.native(i).default_property.as_deref(),
        }
    }

    fn attached(&self, ty: EngineType) -> Option<EngineType> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.attached(ty).map(EngineType::Qt),
            EngineType::Native(i) => self.named(self.native(i).attached.as_deref()?),
        }
    }

    fn has_enum_key(&self, ty: EngineType, scope: Option<&str>, key: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.has_enum_key(ty, scope, key),
            EngineType::Native(i) => match scope {
                Some(scope) => self
                    .native(i)
                    .enumeration(scope)
                    .is_some_and(|keys| keys.iter().any(|(k, _)| k == key)),
                None => self.native(i).enum_key(key).is_some(),
            },
        }
    }

    fn has_enum(&self, ty: EngineType, name: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.has_enum(ty, name),
            EngineType::Native(i) => self.native(i).enumeration(name).is_some(),
        }
    }

    fn enum_is_flags(&self, ty: EngineType, name: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.enum_is_flags(ty, name),
            // What a property of the type says it holds; a type that only
            // has the enum may hold several keys, as the trait allows.
            EngineType::Native(i) => !self
                .native(i)
                .properties
                .iter()
                .any(|p| matches!(&p.ty, TypeTag::Enum(n) if n.as_str() == name)),
        }
    }

    fn required(&self, ty: EngineType) -> Vec<String> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.required(ty),
            EngineType::Native(i) => {
                let mut required: Vec<String> = Vec::new();
                for p in &self.native(i).properties {
                    if p.required && !required.contains(&p.name) {
                        required.push(p.name.clone());
                    }
                }
                required
            }
        }
    }

    fn is_member_name(&self, name: &str) -> bool {
        QtTypeOracle.is_member_name(name) || self.natives.iter().any(|n| n.property(name).is_some())
    }

    fn property_type(&self, ty: EngineType, name: &str) -> Option<String> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.property_type(ty, name),
            EngineType::Native(i) => self.native(i).property(name).map(|p| p.ty.name()),
        }
    }

    fn enum_value(&self, ty: EngineType, scope: Option<&str>, key: &str) -> Option<i64> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.enum_value(ty, scope, key),
            EngineType::Native(i) => match scope {
                Some(scope) => self
                    .native(i)
                    .enumeration(scope)?
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| *v),
                None => self.native(i).enum_key(key),
            },
        }
    }

    fn module(&self, ty: EngineType) -> Option<String> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.module(ty),
            EngineType::Native(i) => {
                let module = &self.native(i).module;
                (!module.is_empty()).then(|| module.clone())
            }
        }
    }

    fn members_known(&self, ty: EngineType) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.members_known(ty),
            EngineType::Native(_) => true,
        }
    }

    fn members_checked(&self, ty: EngineType) -> bool {
        // A native type's members are what it registered, and the Engine
        // makes none at run time: a name it lacks is a mistake in the file.
        // So are those of the group and attached types it reaches; one the
        // table's types reach too is the toolkit's, and is left as the
        // table's types are.
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.members_checked(ty),
            EngineType::Native(i) => self.native(i).checked,
        }
    }

    fn members_warned(&self, ty: EngineType) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.members_warned(ty),
            EngineType::Native(_) => false,
        }
    }

    fn is_unlisted(&self, ty: EngineType) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.is_unlisted(ty),
            EngineType::Native(_) => false,
        }
    }

    fn reads_checked(&self, ty: EngineType) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.reads_checked(ty),
            EngineType::Native(_) => self.members_checked(ty),
        }
    }

    fn adds_member(&self, ty: EngineType, name: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.adds_member(ty, name),
            EngineType::Native(_) => false,
        }
    }

    fn unknown_type_note(&self, name: &str) -> Option<&'static str> {
        QtTypeOracle.unknown_type_note(name)
    }

    fn stands_in(&self, uri: &str) -> bool {
        // An application that registers types of the module has it.
        QtTypeOracle.stands_in(uri) && !self.natives.iter().any(|n| n.module == uri)
    }

    fn stands_in_as_item(&self, name: &str) -> bool {
        QtTypeOracle.stands_in_as_item(name)
    }

    fn holds_object(&self, ty: EngineType, name: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.holds_object(ty, name),
            EngineType::Native(i) => self
                .native(i)
                .property(name)
                .is_some_and(|p| matches!(p.ty, TypeTag::Object(_))),
        }
    }

    fn class_name(&self, ty: EngineType) -> Option<&str> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.class_name(ty),
            EngineType::Native(_) => None,
        }
    }
}
