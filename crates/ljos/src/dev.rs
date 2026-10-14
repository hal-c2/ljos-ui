//! Development-only QML compilation (the non-default `dev-qml`
//! feature).
//!
//! A shipped application never links the QML parser: it embeds
//! components compiled ahead of time and hands them to
//! [`App::source`](crate::App::source). What is here compiles a
//! directory of `.qml` files at start-up instead, for examples,
//! harnesses and the first run of a project — the feature is named
//! for that, and turning it on links `ljos_compiler` and with it
//! the parser.

use std::path::Path;

use ljos_engine::{Engine, MapSource, MetaObject, MetaRef, PropertyAccess, PropertyFlags, TypeTag};
use qml_front::{Property as FrontProperty, QtType, QtTypeOracle, Special, TypeOracle};

/// Compiles every file under `dir` into a component source, against
/// the table of types alone: an application's native types are not
/// in the table (ARCHITECTURE.md D1), so a file that imports one
/// fails here as an unknown type. Printing the diagnostics as a
/// developer at a terminal expects and failing with them, one a
/// line, if any file has an error.
pub fn compile_dir(dir: &Path) -> Result<MapSource, String> {
    compile_dir_with_oracle(dir, QtTypeOracle)
}

/// Compiles every file under `dir` into a component source, against
/// the table of types plus the native types registered on `engine`
/// that the table does not describe ([`EngineOracle::from_engine`]:
/// a type the table knows stays the table's, natives fill the
/// gaps). A file may `import` the application's own module and
/// create objects of its types. Same diagnostics and failure as
/// [`compile_dir`].
pub fn compile_dir_with(dir: &Path, engine: &Engine) -> Result<MapSource, String> {
    compile_dir_with_oracle(dir, EngineOracle::from_engine(engine))
}

fn compile_dir_with_oracle<O: TypeOracle>(dir: &Path, oracle: O) -> Result<MapSource, String> {
    let mut source = MapSource::new();
    let mut errors = Vec::new();
    let files = ljos_compiler::compile_project_with(dir, oracle)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    for r in files {
        match r.result {
            Ok(c) => {
                for d in &c.diagnostics {
                    eprintln!("{}: {:?}: {}", r.key, d.severity, d.message);
                }
                if c.has_errors() {
                    errors.extend(
                        c.diagnostics
                            .iter()
                            .filter(|d| d.severity == ljos_compiler::Severity::Error)
                            .map(|d| format!("{}: {}", r.key, d.message)),
                    );
                }
                source.insert(r.key, c.file);
            }
            Err(ds) => {
                for d in &ds {
                    eprintln!("{}: {}", r.key, d.message);
                }
                errors.extend(ds.iter().map(|d| format!("{}: {}", r.key, d.message)));
            }
        }
    }
    if errors.is_empty() {
        Ok(source)
    } else {
        Err(format!(
            "{} did not compile:\n{}",
            dir.display(),
            errors.join("\n")
        ))
    }
}

/// One type as the compiler sees it in a render with backends: of the
/// table of types, or native (an index into the oracle's own list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineType {
    Qt(QtType),
    Native(u32),
}

/// A [`TypeOracle`] over the table of types plus the native types of an
/// Engine that the table does not describe. An application's native types
/// do not reach the table (ARCHITECTURE.md D1), so a host that registers
/// them and then compiles QML naming them has to say what they are itself:
/// their `MetaObject`s, read here. A type the table knows stays the
/// table's — its description is the fuller one (enum numbers, flags), and
/// an example that uses no backend compiles exactly as before.
#[derive(Clone)]
pub struct EngineOracle {
    /// The registered types the table lacks, and the group and attached
    /// types reachable from them (with no module: reachable, not named).
    natives: Vec<NativeType>,
}

#[derive(Clone)]
struct NativeType {
    module: String,
    meta: MetaRef,
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
            natives: Vec::new(),
        }
    }

    pub fn from_engine(engine: &Engine) -> Self {
        let qt = QtTypeOracle;
        let mut natives: Vec<NativeType> = Vec::new();
        let add = |natives: &mut Vec<NativeType>, module: String, meta: MetaRef| {
            if !natives.iter().any(|n| MetaRef::ptr_eq(&n.meta, &meta)) {
                natives.push(NativeType { module, meta });
            }
        };
        for module in engine.type_dump().modules {
            for ty in &module.types {
                if qt.module_type(&module.uri, &ty.name).is_some() {
                    continue;
                }
                if let Some(meta) = engine.lookup_type(&module.uri, &ty.name) {
                    add(&mut natives, module.uri.clone(), meta);
                }
            }
        }
        // What those types reach: a grouped property's type and an
        // attached type are types the compiler is asked about too, and
        // no module exports them by a name of their own.
        let mut i = 0;
        while i < natives.len() {
            let meta = natives[i].meta.clone();
            let mut reached: Vec<MetaRef> = meta
                .properties
                .iter()
                .filter_map(|p| match p.access {
                    PropertyAccess::Group { meta, .. } => Some(MetaRef::Static(meta)),
                    _ => None,
                })
                .collect();
            if let Some(attached) = meta.attached {
                reached.push(attached());
            }
            for meta in reached {
                add(&mut natives, String::new(), meta);
            }
            i += 1;
        }
        EngineOracle { natives }
    }

    fn meta(&self, index: u32) -> &MetaObject {
        &self.natives[index as usize].meta
    }

    fn named(&self, name: &str) -> Option<EngineType> {
        self.natives
            .iter()
            .position(|n| n.meta.name.as_str() == name)
            .map(|i| EngineType::Native(i as u32))
    }
}

fn qt_property(property: FrontProperty<QtType>) -> FrontProperty<EngineType> {
    FrontProperty {
        is_component: property.is_component,
        is_url: property.is_url,
        is_text: property.is_text,
        is_script: property.is_script,
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
            .position(|n| n.module == uri && n.meta.name.as_str() == name)
            .map(|i| EngineType::Native(i as u32))
    }

    fn name(&self, ty: EngineType) -> &str {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.name(ty),
            EngineType::Native(i) => self.meta(i).name.as_str(),
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
            // A type description does not say it is one object; a backend
            // singleton is read as a type the file may not name bare,
            // which the compile of its example will say.
            EngineType::Native(_) => false,
        }
    }

    fn property(&self, ty: EngineType, name: &str) -> Option<FrontProperty<EngineType>> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.property(ty, name).map(qt_property),
            EngineType::Native(i) => {
                let meta = self.meta(i);
                let info = meta.property(meta.property_index(name)?)?;
                Some(FrontProperty {
                    is_component: matches!(&info.ty, TypeTag::Object(Some(n)) if n.as_str() == "Component"),
                    is_url: info.ty == TypeTag::Url,
                    is_text: info.ty == TypeTag::String,
                    is_script: false,
                    is_list: info.is_list() || matches!(info.ty, TypeTag::List(_)),
                    is_readonly: info.is_readonly(),
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
            EngineType::Native(i) => {
                let meta = self.meta(i);
                let signal = meta.signal(meta.signal_index(name)?)?;
                Some(signal.params.iter().map(|(n, _)| n.to_string()).collect())
            }
        }
    }

    fn has_method(&self, ty: EngineType, name: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.has_method(ty, name),
            EngineType::Native(i) => self.meta(i).method_index(name).is_some(),
        }
    }

    fn default_property(&self, ty: EngineType) -> Option<&str> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.default_property(ty),
            EngineType::Native(i) => self.meta(i).default_property().map(|a| a.as_str()),
        }
    }

    fn attached(&self, ty: EngineType) -> Option<EngineType> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.attached(ty).map(EngineType::Qt),
            EngineType::Native(i) => {
                let attached = self.meta(i).attached?();
                self.natives
                    .iter()
                    .position(|n| MetaRef::ptr_eq(&n.meta, &attached))
                    .map(|i| EngineType::Native(i as u32))
            }
        }
    }

    fn has_enum_key(&self, ty: EngineType, scope: Option<&str>, key: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.has_enum_key(ty, scope, key),
            EngineType::Native(i) => match scope {
                Some(scope) => self
                    .meta(i)
                    .enumeration(scope)
                    .is_some_and(|e| e.keys.iter().any(|(k, _)| k.as_str() == key)),
                None => self.meta(i).enum_key(key).is_some(),
            },
        }
    }

    fn has_enum(&self, ty: EngineType, name: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.has_enum(ty, name),
            EngineType::Native(i) => self.meta(i).enumeration(name).is_some(),
        }
    }

    fn enum_is_flags(&self, ty: EngineType, name: &str) -> bool {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.enum_is_flags(ty, name),
            // What a property of the type says it holds; a type that only
            // has the enum may hold several keys, as the trait allows.
            EngineType::Native(i) => !self
                .meta(i)
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
                for p in &self.meta(i).properties {
                    if p.flags.contains(PropertyFlags::REQUIRED)
                        && !required.iter().any(|n| n == p.name.as_str())
                    {
                        required.push(p.name.to_string());
                    }
                }
                required
            }
        }
    }

    fn is_member_name(&self, name: &str) -> bool {
        QtTypeOracle.is_member_name(name)
            || self
                .natives
                .iter()
                .any(|n| n.meta.property_index(name).is_some())
    }

    fn property_type(&self, ty: EngineType, name: &str) -> Option<String> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.property_type(ty, name),
            EngineType::Native(i) => {
                let meta = self.meta(i);
                meta.property(meta.property_index(name)?)
                    .map(|p| p.ty.name())
            }
        }
    }

    fn enum_value(&self, ty: EngineType, scope: Option<&str>, key: &str) -> Option<i64> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.enum_value(ty, scope, key),
            EngineType::Native(i) => match scope {
                Some(scope) => self
                    .meta(i)
                    .enumeration(scope)?
                    .keys
                    .iter()
                    .find(|(k, _)| k.as_str() == key)
                    .map(|(_, v)| *v),
                None => self.meta(i).enum_key(key),
            },
        }
    }

    fn module(&self, ty: EngineType) -> Option<String> {
        match ty {
            EngineType::Qt(ty) => QtTypeOracle.module(ty),
            EngineType::Native(i) => {
                let module = &self.natives[i as usize].module;
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

    fn unknown_type_note(&self, name: &str) -> Option<&'static str> {
        QtTypeOracle.unknown_type_note(name)
    }
}
