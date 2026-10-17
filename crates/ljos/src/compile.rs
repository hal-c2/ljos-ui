//! Compiling QML against the table of types and an Engine's native
//! types: what [`crate::dev`] does at start-up and [`crate::build`]
//! does in a build script. Either feature links the parser.

use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use ljos_engine::{
    Engine, MapSource, MetaObject, MetaRef, PropertyAccess, PropertyFlags, TypeDump, TypeEntry,
    TypeTag,
};
use qml_front::{Property as FrontProperty, QtType, QtTypeOracle, Special, TypeOracle};

/// What a compile does with a file that has an error.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnError {
    /// The file is noted as one that did not compile
    /// ([`MapSource::insert_failure`]) and the rest of the directory
    /// is used: making that component fails with the compiler's
    /// message, as a QML engine fails a component only where it is
    /// made. A file nothing makes does not stop the application.
    #[default]
    Record,
    /// Any file with an error fails the whole compile, one error a
    /// line: for a build step or CI that wants every file clean.
    Fail,
}

/// What a compile of QML directories found ([`compile_quiet`]): the
/// files as a component source, and every diagnostic of every file.
#[derive(Debug)]
pub struct Compiled {
    /// The compiled files; each file with an error is recorded as one
    /// that did not compile ([`MapSource::failures`]).
    pub source: MapSource,
    /// In key order, and in the order the compiler found them in a
    /// file.
    pub diagnostics: Vec<Diagnostic>,
}

impl Compiled {
    /// Whether a diagnostic is an error: some file did not compile.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// How bad a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// The file did not compile: making it fails.
    Error,
    /// The file compiled, to something only the running program can
    /// settle (a name looked up when it runs, say).
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        })
    }
}

/// One thing a compile found in a file. Shown as a compiler at a
/// terminal says it, `path:line:column: severity: message`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// The file, as found under the directory it was compiled from.
    pub path: PathBuf,
    /// The key the file is known by (`Main`, `controls/Button`).
    pub key: String,
    /// 1-based, in characters.
    pub line: u32,
    pub column: u32,
    pub severity: Severity,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}: {}",
            self.path.display(),
            self.line,
            self.column,
            self.severity,
            self.message
        )
    }
}

/// Compiles every file under `dir` into a component source, against
/// the table of types alone: an application's native types are not
/// in the table (ARCHITECTURE.md D1), so a file that imports one is
/// recorded as not compiling (an unknown type). Prints the
/// diagnostics as a developer at a terminal expects; a file with an
/// error is recorded, not fatal ([`OnError::Record`]).
#[cfg(feature = "dev-qml")]
pub fn compile_dir(dir: &Path) -> Result<MapSource, String> {
    compile(dir, None, OnError::Record)
}

/// Compiles every file under `dir` into a component source, against
/// the table of types plus the native types registered on `engine`
/// that the table does not describe ([`EngineOracle::from_engine`]:
/// a type the table knows stays the table's, natives fill the
/// gaps). A file may `import` the application's own module and
/// create objects of its types. Same diagnostics and recording as
/// [`compile_dir`].
#[cfg(feature = "dev-qml")]
pub fn compile_dir_with(dir: &Path, engine: &Engine) -> Result<MapSource, String> {
    compile(dir, Some(engine), OnError::Record)
}

/// Compiles every file under `dir`, against the table of types and,
/// given an `engine`, its native types as [`compile_dir_with`] does;
/// `on_error` says whether a file with an error is recorded or fails
/// the compile. `Err` also when `dir` cannot be read.
#[cfg(feature = "dev-qml")]
pub fn compile(
    dir: &Path,
    engine: Option<&Engine>,
    on_error: OnError,
) -> Result<MapSource, String> {
    compile_dirs(&[dir], engine, on_error)
}

/// [`compile`] over several QML roots, an application's import path:
/// each root's files are keyed by their path from that root (the
/// entry of `app/Main.qml` under the root `app` is `Main`), and a file
/// of one root may import a module whose `qmldir` is in another.
/// Where two roots have a file of the same key the earlier root's is
/// used, as the first import path that has a module is.
///
/// Each diagnostic is printed to stderr as one line
/// (`path:line:column: severity: message`); [`compile_quiet`] hands
/// them back instead.
pub fn compile_dirs(
    dirs: &[&Path],
    engine: Option<&Engine>,
    on_error: OnError,
) -> Result<MapSource, String> {
    match engine {
        Some(engine) => compile_with_oracle(dirs, EngineOracle::from_engine(engine), on_error),
        None => compile_with_oracle(dirs, QtTypeOracle, on_error),
    }
}

/// [`compile_dirs`] printing nothing and failing no file: what the
/// compile found comes back as values, for an application to show
/// its own way, count, or hold to none. Each file has its own URL,
/// `file://` and its absolute path, so `Qt.resolvedUrl` and every
/// relative address in it resolve beside the file, as a QML file
/// loaded from disk does. `Err` only when a directory cannot be read.
#[cfg(feature = "dev-qml")]
pub fn compile_quiet(dirs: &[&Path], engine: Option<&Engine>) -> Result<Compiled, String> {
    match engine {
        Some(engine) => compile_found(dirs, EngineOracle::from_engine(engine)),
        None => compile_found(dirs, QtTypeOracle),
    }
}

pub(crate) fn compile_found<O: TypeOracle>(dirs: &[&Path], oracle: O) -> Result<Compiled, String> {
    let mut source = MapSource::new();
    let mut found = Vec::new();
    let files = ljos_compiler::compile_roots_with(dirs, oracle).map_err(|e| e.to_string())?;
    for r in files {
        let path = r.path.clone().unwrap_or_else(|| PathBuf::from(&r.key));
        let diagnostics = match &r.result {
            Ok(c) => &c.diagnostics,
            Err(ds) => ds,
        };
        found.extend(diagnostics.iter().map(|d| Diagnostic {
            path: path.clone(),
            key: r.key.clone(),
            line: d.span.line,
            column: d.span.column,
            severity: match d.severity {
                ljos_compiler::Severity::Error => Severity::Error,
                ljos_compiler::Severity::Warning => Severity::Warning,
            },
            message: d.message.clone(),
        }));
        if let Some(path) = &r.path {
            source.insert_url(r.key.clone(), file_url(path));
        }
        match r.result {
            Ok(c) if !c.has_errors() => source.insert(r.key, c.file),
            // Compiled only as far as it could: not a file to run.
            Ok(c) => source.insert_failure(r.key, ljos_compiler::failure_reason(&c.diagnostics)),
            Err(ds) => source.insert_failure(r.key, ljos_compiler::failure_reason(&ds)),
        }
    }
    Ok(Compiled {
        source,
        diagnostics: found,
    })
}

/// [`compile_found`], printing each diagnostic, and with
/// [`OnError::Fail`] failing on any error.
pub(crate) fn compile_with_oracle<O: TypeOracle>(
    dirs: &[&Path],
    oracle: O,
    on_error: OnError,
) -> Result<MapSource, String> {
    let compiled = compile_found(dirs, oracle)?;
    for d in &compiled.diagnostics {
        eprintln!("{d}");
    }
    checked(dirs, compiled, on_error)
}

/// `compiled`'s source, or with [`OnError::Fail`] and an error, every
/// error a line.
pub(crate) fn checked(
    dirs: &[&Path],
    compiled: Compiled,
    on_error: OnError,
) -> Result<MapSource, String> {
    if on_error == OnError::Fail && compiled.has_errors() {
        let named = dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let errors = compiled
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("{named} did not compile:\n{errors}"));
    }
    Ok(compiled.source)
}

/// The `file://` URL of `path`, made absolute against the working
/// directory and without `.` and `..`, so that it is the same text
/// the engine reaches when it resolves a relative address beside
/// another file.
fn file_url(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let mut parts: Vec<String> = Vec::new();
    for part in absolute.components() {
        match part {
            Component::Prefix(p) => parts.push(p.as_os_str().to_string_lossy().into_owned()),
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(p) => parts.push(p.to_string_lossy().into_owned()),
        }
    }
    // `file:///home/a/Main.qml`; on Windows `file:///C:/a/Main.qml`.
    format!("file:///{}", parts.join("/"))
}

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
    natives: Vec<NativeType>,
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
        for s in &entry.signals {
            let params = s.parameters.iter().map(|p| p.name.clone()).collect();
            ty.signals.push((s.name.clone(), params));
        }
        // The dump leaves out the change signals the Engine makes for a
        // property; a handler of one is as good as any other.
        for p in &entry.properties {
            if let Some(notify) = &p.notify
                && !entry.signals.iter().any(|s| &s.name == notify)
            {
                ty.signals.push((notify.clone(), Vec::new()));
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
            natives: Vec::new(),
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
    pub fn from_dump(dump: &TypeDump, engine: &Engine) -> Result<Self, String> {
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
        EngineOracle { natives }
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
) -> Result<NativeType, String> {
    if depth > BASE_DEPTH {
        return Err(format!("the bases of {} loop", entry.name));
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
            .ok_or_else(|| {
                format!(
                    "{}: its base {base} is neither in the type dump nor one of the toolkit's types",
                    entry.name
                )
            })?;
            NativeType::from_meta(&meta, String::new(), false)
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
                let info = self.native(i).property(name)?;
                Some(FrontProperty {
                    is_component: matches!(&info.ty, TypeTag::Object(Some(n)) if n.as_str() == "Component"),
                    is_url: info.ty == TypeTag::Url,
                    is_text: info.ty == TypeTag::String,
                    is_script: false,
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
            EngineType::Qt(_) => false,
            EngineType::Native(i) => self.native(i).checked,
        }
    }

    fn unknown_type_note(&self, name: &str) -> Option<&'static str> {
        QtTypeOracle.unknown_type_note(name)
    }
}
