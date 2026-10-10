//! Where compiled files come from, and what the engine keeps per file
//! (contract item 5): the IR, its JS module (loaded once, from bytecode
//! when the source has it), the type made for each declaring object, and
//! the bindings simple enough to run without JS.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use ljos_ir::{CompIdx, CompiledFile, ComponentKind, FnIndex, ImportSource};
use ljos_js::{Bytecode, FnId};

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::meta::MetaRef;

/// Gives the engine compiled files by key (`"Main"`, `"controls/Button"`).
/// A file's components refer to other files by the same keys.
pub trait ComponentSource {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>>;

    /// The URL messages name the file by.
    fn url(&self, key: &str) -> String {
        format!("file:///{key}.qml")
    }

    /// The file's JS module precompiled, if the build made it.
    fn bytecode(&self, _key: &str) -> Option<Bytecode> {
        None
    }
}

/// A [`ComponentSource`] over files given up front.
#[derive(Default)]
pub struct MapSource {
    files: HashMap<String, Arc<CompiledFile>>,
    bytecode: HashMap<String, Bytecode>,
    base: Option<String>,
}

impl MapSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// URLs become `{base}/{key}.qml`.
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base = Some(base.into());
        self
    }

    pub fn insert(&mut self, key: impl Into<String>, file: impl Into<Arc<CompiledFile>>) {
        self.files.insert(key.into(), file.into());
    }

    pub fn insert_bytecode(&mut self, key: impl Into<String>, code: Bytecode) {
        self.bytecode.insert(key.into(), code);
    }
}

impl std::fmt::Debug for MapSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapSource")
            .field("files", &self.files.keys().collect::<Vec<_>>())
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl ComponentSource for MapSource {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        self.files.get(key).cloned()
    }

    fn url(&self, key: &str) -> String {
        match &self.base {
            Some(b) => format!("{}/{key}.qml", b.trim_end_matches('/')),
            None => format!("file:///{key}.qml"),
        }
    }

    fn bytecode(&self, key: &str) -> Option<Bytecode> {
        self.bytecode.get(key).cloned()
    }
}

#[derive(Default)]
pub(crate) struct LoaderState {
    pub(crate) source: Option<Rc<dyn ComponentSource>>,
    pub(crate) files: HashMap<String, Rc<LoadedFile>>,
    /// Creation jobs waiting for the host's scheduler
    /// ([`Engine::incubate`]).
    pub(crate) incubating: Vec<crate::incubator::CreationJob>,
}

/// A binding that reads a path of properties from one parameter
/// (`root.width`, `$scope.parent.height`): run in Rust, no JS.
#[derive(Clone, Debug)]
pub(crate) struct Tier0 {
    pub(crate) param: usize,
    pub(crate) path: Vec<Atom>,
}

pub(crate) struct LoadedFile {
    pub(crate) key: String,
    pub(crate) url: Atom,
    pub(crate) ir: Arc<CompiledFile>,
    bytecode: Option<Bytecode>,
    /// Function ids by `FnIndex`, once the module is loaded.
    fns: RefCell<Option<Rc<Vec<Option<FnId>>>>>,
    /// Types of declaring objects, by (component, object).
    pub(crate) metas: RefCell<HashMap<(u32, u32), MetaRef>>,
    /// Tier-0 form of each function, by `FnIndex`.
    pub(crate) tier0: Vec<Option<Tier0>>,
}

impl LoadedFile {
    fn new(key: &str, url: String, ir: Arc<CompiledFile>, bytecode: Option<Bytecode>) -> Self {
        let tier0 = tier0_table(&ir);
        Self {
            key: key.to_owned(),
            url: Atom::new(&url),
            ir,
            bytecode,
            fns: RefCell::new(None),
            metas: RefCell::new(HashMap::new()),
            tier0,
        }
    }

    /// The file's inline component `name`, or its file component.
    pub(crate) fn find_component(&self, inline: Option<&str>) -> Option<CompIdx> {
        self.ir
            .components
            .iter()
            .position(|c| match (inline, &c.kind) {
                (None, ComponentKind::File) => true,
                (Some(n), ComponentKind::Inline) => c.name == n,
                _ => false,
            })
            .map(|i| CompIdx(i as u32))
    }

    /// The module's function for `f`, loading the module on first use.
    pub(crate) fn function(&self, engine: &Engine, f: FnIndex) -> Result<FnId, EngineError> {
        let loaded = self.fns.borrow().clone();
        let fns = match loaded {
            Some(fns) => fns,
            None => {
                let fns = Rc::new(self.load_module(engine)?);
                *self.fns.borrow_mut() = Some(fns.clone());
                fns
            }
        };
        fns.get(f.index())
            .copied()
            .flatten()
            .ok_or_else(|| EngineError::Js(format!("{}: no function #{}", self.url, f.index())))
    }

    fn load_module(&self, engine: &Engine) -> Result<Vec<Option<FnId>>, EngineError> {
        let js = engine.js()?;
        // Module names must be unique per host; the key is.
        let name = format!("{}.qml.js", self.key);
        let handle = match &self.bytecode {
            Some(b) => js.host.load_module_bytecode(b),
            None => js.host.load_module(&name, &self.ir.js.source),
        }
        .map_err(|e| {
            engine.report_js_fault(&e);
            EngineError::Js(e.to_string())
        })?;
        js.lines
            .borrow_mut()
            .add(&handle.name, &self.url, &self.ir.js.source);
        Ok(self
            .ir
            .js
            .functions
            .iter()
            .map(|f| handle.function(&f.export))
            .collect())
    }
}

/// Finds the functions whose body is `return (p.a.b);` with `p` a
/// parameter.
fn tier0_table(ir: &CompiledFile) -> Vec<Option<Tier0>> {
    let mut by_name: HashMap<&str, Tier0> = HashMap::new();
    let src = &ir.js.source;
    let mut lines = src.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("export function ") else {
            continue;
        };
        let Some((name, rest)) = rest.split_once('(') else {
            continue;
        };
        let Some((params, tail)) = rest.split_once(')') else {
            continue;
        };
        if tail.trim() != "{" {
            continue;
        }
        let Some(body) = lines.peek().map(|l| l.trim()) else {
            continue;
        };
        let Some(expr) = body
            .strip_prefix("return (")
            .and_then(|b| b.strip_suffix(");"))
        else {
            continue;
        };
        let params: Vec<&str> = params.split(',').map(str::trim).collect();
        let mut segs = expr.split('.');
        let Some(first) = segs.next() else { continue };
        let Some(param) = params.iter().position(|p| *p == first) else {
            continue;
        };
        let path: Vec<&str> = segs.collect();
        if path.iter().all(|s| is_ident(s)) {
            by_name.insert(
                name,
                Tier0 {
                    param,
                    path: path.into_iter().map(Atom::new).collect(),
                },
            );
        }
    }
    ir.js
        .functions
        .iter()
        .map(|f| by_name.get(f.export.as_str()).cloned())
        .collect()
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|f| f.is_ascii_alphabetic() || f == '_' || f == '$')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

impl Engine {
    /// Where [`Engine::create_from_url`] and file-component types load from.
    pub fn set_component_source(&self, source: impl ComponentSource + 'static) {
        let mut l = self.inner.loader.borrow_mut();
        l.source = Some(Rc::new(source));
        l.files.clear();
    }

    /// `url` as a source key: `file:///Main.qml` and `Main.qml` are `Main`.
    pub(crate) fn url_key(url: &str) -> String {
        let s = url
            .strip_prefix("file:///")
            .or_else(|| url.strip_prefix("file://"))
            .or_else(|| url.strip_prefix("qrc:/"))
            .unwrap_or(url);
        s.strip_suffix(".qml").unwrap_or(s).to_owned()
    }

    /// The loaded file `key`, loading it on first use.
    pub(crate) fn load_file(&self, key: &str) -> Result<Rc<LoadedFile>, EngineError> {
        if let Some(f) = self.inner.loader.borrow().files.get(key) {
            return Ok(f.clone());
        }
        let source = self
            .inner
            .loader
            .borrow()
            .source
            .clone()
            .ok_or_else(|| EngineError::Load("no component source is set".into()))?;
        let ir = source
            .load(key)
            .ok_or_else(|| EngineError::Load(format!("{}: No such file", source.url(key))))?;
        if ir.version != ljos_ir::IR_VERSION {
            return Err(EngineError::Load(format!(
                "{}: compiled for IR version {}, this engine reads {}",
                source.url(key),
                ir.version,
                ljos_ir::IR_VERSION
            )));
        }
        let url = source.url(key);
        for import in &ir.imports {
            if let ImportSource::Module(uri) = &import.source
                && !self.has_module(uri)
            {
                return Err(EngineError::Load(format!(
                    "{url}: module \"{uri}\" is not installed"
                )));
            }
        }
        let file = Rc::new(LoadedFile::new(key, url, ir, source.bytecode(key)));
        self.inner
            .loader
            .borrow_mut()
            .files
            .insert(key.to_owned(), file.clone());
        Ok(file)
    }
}
