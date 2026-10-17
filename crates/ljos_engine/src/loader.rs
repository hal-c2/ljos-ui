//! Where compiled files come from, and what the engine keeps per file
//! (contract item 5): the IR, its JS module (loaded once, from bytecode
//! when the source has it), the type made for each declaring object, and
//! the bindings simple enough to run without JS.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use ljos_ir::{BindingKind, CompIdx, CompiledFile, ComponentKind, FnIndex, ImportSource};
use ljos_js::{Bytecode, FnId, JsValue};

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::meta::MetaRef;

/// Gives the engine compiled files by key (`"Main"`, `"controls/Button"`).
/// A file's components refer to other files by the same keys.
pub trait ComponentSource {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>>;

    /// Why `key` has no compiled file although the source knows it: the
    /// file is there but did not compile. Qt names the file's own mistake
    /// when a type it needs does not load, not a missing file. A reason
    /// that starts with a place (`3:5: message`) follows the file's URL
    /// as a place in it does.
    fn failure(&self, _key: &str) -> Option<String> {
        None
    }

    /// The URL messages name the file by.
    fn url(&self, key: &str) -> String {
        format!("file:///{key}.qml")
    }

    /// The key of the file at `url`: the reverse of [`Self::url`]. A bare
    /// key or file name (`Main`, `Main.qml`) is accepted too.
    fn key(&self, url: &str) -> String {
        plain_key(url)
    }

    /// The file's JS module precompiled, if the build made it.
    fn bytecode(&self, _key: &str) -> Option<Bytecode> {
        None
    }
}

/// `url` without its query and fragment. A file is the same file whatever
/// is asked of it: `Panel.qml?generation=3` is how a tool makes a URL
/// that differs after a reload (ARCHITECTURE.md §12.1).
pub(crate) fn strip_query(url: &str) -> &str {
    match url.find(['?', '#']) {
        Some(i) => &url[..i],
        None => url,
    }
}

/// The `file://` URL of the absolute `path`, written as Qt writes a URL
/// for a script or a person to read (`QUrl::toString`): only what would
/// change what the address means is escaped — `%`, `#`, `?`, control
/// bytes and the characters a URL may never hold (quotes, angle and square
/// brackets, backslash, caret, backquote, braces and bar) — while a space and a character outside ASCII stay as they are. Every
/// reader percent-decodes it back to the path ([`url_path`]); what goes
/// out of the process takes [`encoded_url`] of it.
pub fn file_url(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        // A Windows path: file:///C:/...
        out.push('/');
    }
    for c in s.chars() {
        match c {
            '\\' if !s.starts_with('/') => out.push('/'),
            '%'
            | '#'
            | '?'
            | '"'
            | '<'
            | '>'
            | '['
            | '\\'
            | ']'
            | '^'
            | '`'
            | '{'
            | '|'
            | '}'
            | '\0'..='\x1f'
            | '\x7f' => out.push_str(&format!("%{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// `url` as it is written for another program (`QUrl::toEncoded`): a
/// space and every byte of a character outside ASCII escaped as well, so
/// that what [`file_url`] gives is a URL any reader takes.
pub fn encoded_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for b in url.bytes() {
        match b {
            b' ' | 0..0x20 | 0x7f.. => out.push_str(&format!("%{b:02X}")),
            _ => out.push(char::from(b)),
        }
    }
    out
}

/// The path of a `file:` URL, or of a plain absolute path; `None` for any
/// other URL.
pub fn url_path(url: &str) -> Option<std::path::PathBuf> {
    let rest = match url.strip_prefix("file://") {
        Some(rest) => rest.strip_prefix("localhost").unwrap_or(rest),
        None if url.starts_with('/') => url,
        None => url.strip_prefix("file:").filter(|r| r.starts_with('/'))?,
    };
    // `file:///C:/x` names `C:/x`.
    let rest = match rest.as_bytes() {
        [b'/', d, b':', ..] if d.is_ascii_alphabetic() => &rest[1..],
        _ => rest,
    };
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(&h), Some(&l)) if hex(h).is_some() && hex(l).is_some() => {
                out.push((hex(h)? * 16 + hex(l)?) as u8);
                i += 3;
            }
            (b, ..) => {
                out.push(b);
                i += 1;
            }
        }
    }
    Some(std::path::PathBuf::from(
        String::from_utf8_lossy(&out).into_owned(),
    ))
}

/// `file:///Main.qml` and `Main.qml` are `Main`.
fn plain_key(url: &str) -> String {
    let url = strip_query(url);
    let s = url
        .strip_prefix("file:///")
        .or_else(|| url.strip_prefix("file://"))
        .or_else(|| url.strip_prefix("qrc:/"))
        .unwrap_or(url);
    s.strip_suffix(".qml").unwrap_or(s).to_owned()
}

/// `rel` as a document at `base` means it (RFC 3986 section 5.2, without
/// queries and fragments, which QML file URLs do not carry).
pub(crate) fn join_url(base: &str, rel: &str) -> String {
    if has_scheme(rel) {
        return rel.to_owned();
    }
    // `file:///a/b.qml` is (`file://`, `/a/b.qml`); `qrc:/a/b.qml` is
    // (`qrc:`, `/a/b.qml`); a bare key has no root at all.
    let (root, path) = match base.find("://") {
        Some(i) => {
            let rest = &base[i + 3..];
            let host = rest.find('/').unwrap_or(rest.len());
            base.split_at(i + 3 + host)
        }
        None => match base.find(':').filter(|_| has_scheme(base)) {
            Some(i) => base.split_at(i + 1),
            None => ("", base),
        },
    };
    let joined = if rel.starts_with('/') {
        rel.to_owned()
    } else {
        let dir = path.rfind('/').map_or("", |i| &path[..=i]);
        format!("{dir}{rel}")
    };
    // Below a scheme or a host there is nothing but absolute paths.
    let absolute = joined.starts_with('/') || !root.is_empty();
    // An address naming a directory keeps saying so (RFC 3986, 5.2.4):
    // `sub/`, `sub/.` and `sub/..` all end in a slash.
    let directory = matches!(joined.rsplit('/').next(), Some("" | "." | ".."));
    let mut out: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    let tail = if directory && !out.is_empty() {
        "/"
    } else {
        ""
    };
    format!(
        "{root}{}{}{tail}",
        if absolute { "/" } else { "" },
        out.join("/")
    )
}

fn has_scheme(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    let mut c = scheme.chars();
    // One letter is a Windows drive, not a scheme.
    scheme.len() > 1
        && c.next().is_some_and(|f| f.is_ascii_alphabetic())
        && c.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

/// A [`ComponentSource`] over files given up front.
///
/// A file's URL is the one given for its key
/// ([`insert_url`](Self::insert_url)), as a compile from a directory
/// records where each file is; else `{base}/{key}.qml` with a
/// [`base`](Self::with_base_url); else `file:///{key}.qml`.
#[derive(Default)]
pub struct MapSource {
    files: BTreeMap<String, Arc<CompiledFile>>,
    bytecode: HashMap<String, Bytecode>,
    failures: BTreeMap<String, String>,
    base: Option<String>,
    urls: HashMap<String, String>,
    keys_by_url: HashMap<String, String>,
}

impl MapSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// URLs become `{base}/{key}.qml`: for files that came from no
    /// one place (a pack, files given as text), where the application
    /// says where they and their assets live. A key with its own URL
    /// ([`insert_url`](Self::insert_url)) keeps it.
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base = Some(base.into());
        self
    }

    pub fn insert(&mut self, key: impl Into<String>, file: impl Into<Arc<CompiledFile>>) {
        self.files.insert(key.into(), file.into());
    }

    /// Says the file `key` (compiled or failed) is at `url`, a file
    /// read from disk at its own `file://` URL: `Qt.resolvedUrl` and
    /// every relative address its objects use (an `Image` source, a
    /// `Loader` source, an imported script) resolve against it.
    pub fn insert_url(&mut self, key: impl Into<String>, url: impl Into<String>) {
        let (key, url) = (key.into(), url.into());
        self.keys_by_url.insert(url.clone(), key.clone());
        if let Some(old) = self.urls.insert(key, url) {
            self.keys_by_url.remove(&old);
        }
    }

    /// The keys of the compiled files, in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// The files that did not compile, in key order, each with why
    /// ([`insert_failure`](Self::insert_failure)): where in the file
    /// the first error is and what it is (`4:1: Unexpected token`).
    pub fn failures(&self) -> impl Iterator<Item = (&str, &str)> {
        self.failures
            .iter()
            .map(|(key, why)| (key.as_str(), why.as_str()))
    }

    /// Notes that `key` is a file that did not compile, and why.
    pub fn insert_failure(&mut self, key: impl Into<String>, why: impl Into<String>) {
        self.failures.insert(key.into(), why.into());
    }

    pub fn insert_bytecode(&mut self, key: impl Into<String>, code: Bytecode) {
        self.bytecode.insert(key.into(), code);
    }

    /// The compiled files as one pack, for a build step to write and a
    /// shipped program to embed and read back with
    /// [`from_pack`](Self::from_pack): a count, then per file its key
    /// and its IR,
    /// each preceded by its length as a little-endian `u32`, in key
    /// order. Failures and bytecode are not packed.
    pub fn to_pack(&self) -> Result<Vec<u8>, EngineError> {
        let len = |n: usize| {
            u32::try_from(n)
                .map(u32::to_le_bytes)
                .map_err(|_| EngineError::Load("a pack entry over 4 GiB".to_owned()))
        };
        let keys: Vec<&String> = self.files.keys().collect();
        let mut out = Vec::new();
        out.extend_from_slice(&len(keys.len())?);
        for key in keys {
            let bytes = self.files[key]
                .to_bytes()
                .map_err(|e| EngineError::Load(format!("{key}: {e}")))?;
            out.extend_from_slice(&len(key.len())?);
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(&len(bytes.len())?);
            out.extend_from_slice(&bytes);
        }
        Ok(out)
    }

    /// The files a [`to_pack`](Self::to_pack) pack holds. A pack that
    /// is cut short, or an entry whose IR is of another version than
    /// this engine reads, is an error: a shipped program embeds the
    /// pack its own build wrote, so either means a broken build.
    pub fn from_pack(pack: &[u8]) -> Result<Self, EngineError> {
        fn take<'a>(rest: &mut &'a [u8], n: usize) -> Result<&'a [u8], EngineError> {
            if rest.len() < n {
                return Err(EngineError::Load("the pack is cut short".to_owned()));
            }
            let (head, tail) = rest.split_at(n);
            *rest = tail;
            Ok(head)
        }
        fn len(rest: &mut &[u8]) -> Result<usize, EngineError> {
            let b = take(rest, 4)?;
            Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        }
        let mut rest = pack;
        let mut source = Self::new();
        for _ in 0..len(&mut rest)? {
            let n = len(&mut rest)?;
            let key = std::str::from_utf8(take(&mut rest, n)?)
                .map_err(|_| EngineError::Load("a pack key is not UTF-8".to_owned()))?;
            let n = len(&mut rest)?;
            let file = CompiledFile::from_bytes(take(&mut rest, n)?)
                .map_err(|e| EngineError::Load(format!("{key}: {e}")))?;
            source.insert(key, file);
        }
        Ok(source)
    }
}

impl std::fmt::Debug for MapSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapSource")
            .field("files", &self.files.keys().collect::<Vec<_>>())
            .field("failures", &self.failures.keys().collect::<Vec<_>>())
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl ComponentSource for MapSource {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        self.files.get(key).cloned()
    }

    fn failure(&self, key: &str) -> Option<String> {
        self.failures.get(key).cloned()
    }

    fn url(&self, key: &str) -> String {
        if let Some(url) = self.urls.get(key) {
            return url.clone();
        }
        match &self.base {
            Some(b) => format!("{}/{key}.qml", b.trim_end_matches('/')),
            None => format!("file:///{key}.qml"),
        }
    }

    fn key(&self, url: &str) -> String {
        if let Some(key) = self.keys_by_url.get(strip_query(url)) {
            return key.clone();
        }
        let under_base = self
            .base
            .as_deref()
            .and_then(|b| url.strip_prefix(b.trim_end_matches('/')))
            .and_then(|rest| rest.strip_prefix('/'));
        match under_base {
            Some(rest) => rest.strip_suffix(".qml").unwrap_or(rest).to_owned(),
            None => plain_key(url),
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
    /// Module types written in QML ([`Engine::register_qml_type`]).
    pub(crate) qml_types: crate::qml_types::QmlTypes,
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
    /// The tier-1 programs of the file's expressions.
    pub(crate) tier1: crate::incubator::tier1::Programs,
    /// The dev reload the file was loaded after; 0 without one. A JS
    /// host cannot unload a module or load one name twice, so the script
    /// of a reloaded file goes by a name with this in it.
    pub(crate) epoch: Cell<u64>,
    /// Which text a script handed over this is (`Qt.createQmlObject`); 0
    /// for a file. Two texts may be given one address, and a module name
    /// is used once.
    pub(crate) text: Cell<u64>,
}

impl LoadedFile {
    pub(crate) fn new(
        key: &str,
        url: String,
        ir: Arc<CompiledFile>,
        bytecode: Option<Bytecode>,
    ) -> Self {
        let tier0 = tier0_table(&ir);
        let tier1 = crate::incubator::tier1::Programs::new(&ir);
        Self {
            key: key.to_owned(),
            url: Atom::new(&url),
            ir,
            bytecode,
            fns: RefCell::new(None),
            metas: RefCell::new(HashMap::new()),
            tier0,
            tier1,
            epoch: Cell::new(0),
            text: Cell::new(0),
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
        let name = match (self.text.get(), self.epoch.get()) {
            (0, 0) => format!("{}.qml.js", self.key),
            (0, n) => format!("{}.qml.js?generation={n}", self.key),
            (t, _) => format!("{}.qml.js?text={t}", self.key),
        };
        let handle = match &self.bytecode {
            Some(b) => js.host.load_module_bytecode(b),
            None => js.host.load_module(&name, &self.ir.js.source),
        }
        .map_err(|e| {
            engine.report_js_fault(&e);
            EngineError::Js(e.to_string())
        })?;
        js.lines.borrow_mut().add(&handle.name, &self.url, &self.ir);
        // A module that names `Qt` has its own, which reads a relative
        // address from this file (incubator/dynamic.rs).
        if let Some(at) = handle.function(crate::incubator::dynamic::AT_EXPORT) {
            js.host
                .call(at, &[JsValue::String(self.url.to_string())])
                .map_err(|e| {
                    engine.report_js_fault(&e);
                    EngineError::Js(e.to_string())
                })?;
        }
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
        // The name of a singleton, as the compiler writes it: the path
        // starts at its object.
        let first = first
            .strip_prefix("__ljos_single(")
            .and_then(|f| f.strip_suffix(')'))
            .unwrap_or(first);
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
        drop(l);
        self.singletons_sources_changed();
    }

    /// Starts the JS host and loads the script of every file loaded so far
    /// that has a handler or a method, so that the first click or key
    /// does not pay for them (about 3 ms on a desktop, started from
    /// nothing). A host calls it when it has nothing else to do, after
    /// the first frame; a file of bindings alone starts nothing, and
    /// what is loaded already is not loaded again.
    pub fn prepare_scripts(&self) {
        let files: Vec<Rc<LoadedFile>> = self
            .inner
            .loader
            .borrow()
            .files
            .values()
            .filter(|f| f.fns.borrow().is_none())
            .cloned()
            .collect();
        for file in files {
            let run = file
                .ir
                .exprs
                .iter()
                .find(|e| matches!(e.kind, BindingKind::Handler | BindingKind::Function));
            if let Some(e) = run {
                // A module that does not load says so where it is used.
                let _ = file.function(self, e.js_fn);
            }
        }
    }

    /// `url` as a key of the component source.
    pub(crate) fn url_key(&self, url: &str) -> String {
        let url = strip_query(url);
        let source = self.inner.loader.borrow().source.clone();
        match source {
            Some(s) => s.key(url),
            None => plain_key(url),
        }
    }

    /// `url` as the file that declares `from` means it: a relative URL is
    /// taken from that file's directory, as Qt resolves `Loader.source`
    /// and `Image.source`. For the root of a component that is the file
    /// using the component, not the one defining it (Qt 6.11.2, by
    /// running qml6). An object not made from QML leaves `url` as it is.
    pub fn resolve_url(&self, from: ObjectId, url: &str) -> String {
        let base = {
            let store = self.inner.store.borrow();
            store
                .objects
                .get(from)
                .and_then(|h| h.ext.as_ref()?.location.as_ref())
                .map(|l| l.url.clone())
        };
        match base {
            Some(base) if !url.is_empty() => join_url(&base, url),
            _ => url.to_owned(),
        }
    }

    /// The address of the file that declares `obj`, which is what
    /// [`Engine::resolve_url`] resolves from. `None` for an object not made
    /// from QML.
    pub fn document_url(&self, obj: ObjectId) -> Option<Atom> {
        let store = self.inner.store.borrow();
        let location = store.objects.get(obj)?.ext.as_ref()?.location.as_ref()?;
        Some(location.url.clone())
    }

    /// The loaded file `key`, loading it on first use.
    pub(crate) fn load_file(&self, key: &str) -> Result<Rc<LoadedFile>, EngineError> {
        if let Some(f) = self.inner.loader.borrow().files.get(key) {
            return Ok(f.clone());
        }
        // A file of a component library (a style's `Button.qml`).
        if let Some(f) = self.library_file(key)? {
            return Ok(f);
        }
        let source = self
            .inner
            .loader
            .borrow()
            .source
            .clone()
            .ok_or_else(|| EngineError::Load("no component source is set".into()))?;
        let ir = source.load(key).ok_or_else(|| {
            EngineError::Load(match source.failure(key) {
                Some(why) if why.starts_with(|c: char| c.is_ascii_digit()) => {
                    format!("{}:{why}", source.url(key))
                }
                Some(why) => format!("{}: {why}", source.url(key)),
                None => format!("{}: No such file", source.url(key)),
            })
        })?;
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
                && !import.in_project
                && !self.has_module(uri)
            {
                return Err(EngineError::Load(format!(
                    "{url}: module \"{uri}\" is not installed"
                )));
            }
        }
        let file = Rc::new(LoadedFile::new(key, url, ir, source.bytecode(key)));
        file.epoch.set(self.inner.reload.epoch.get());
        self.inner
            .loader
            .borrow_mut()
            .files
            .insert(key.to_owned(), file.clone());
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_file_url_is_written_as_qt_writes_it() {
        // What qml6 gives for Qt.resolvedUrl beside a file in this directory.
        let p = std::path::Path::new("/d/a b#c%d[e]{f}|g^h`i\"j<k>l;m=n@o'p ä?q\\r\ts~!$&()*+,:/x");
        let url = file_url(p);
        assert_eq!(
            url,
            "file:///d/a b%23c%25d%5Be%5D%7Bf%7D%7Cg%5Eh%60i%22j%3Ck%3El;m=n@o'p ä%3Fq%5Cr%09s~!$&()*+,:/x"
        );
        assert_eq!(url_path(&url).as_deref(), Some(p));
        assert_eq!(
            encoded_url(&url),
            "file:///d/a%20b%23c%25d%5Be%5D%7Bf%7D%7Cg%5Eh%60i%22j%3Ck%3El;m=n@o'p%20%C3%A4%3Fq%5Cr%09s~!$&()*+,:/x"
        );
        assert_eq!(url_path(&encoded_url(&url)).as_deref(), Some(p));
        assert_eq!(url_path("http://x/y"), None);
        assert_eq!(
            url_path("/plain").as_deref(),
            Some(std::path::Path::new("/plain"))
        );
    }

    use super::*;

    #[test]
    fn a_relative_url_is_taken_from_the_directory_of_the_base() {
        for (base, rel, want) in [
            ("file:///Main.qml", "Leaf.qml", "file:///Leaf.qml"),
            ("file:///sub/Panel.qml", "Leaf.qml", "file:///sub/Leaf.qml"),
            ("file:///sub/Panel.qml", "../Leaf.qml", "file:///Leaf.qml"),
            (
                "file:///sub/Panel.qml",
                "./a/../b/Leaf.qml",
                "file:///sub/b/Leaf.qml",
            ),
            (
                "file:///sub/Panel.qml",
                "../../Leaf.qml",
                "file:///Leaf.qml",
            ),
            (
                "file:///sub/Panel.qml",
                "/top/Leaf.qml",
                "file:///top/Leaf.qml",
            ),
            (
                "file:///sub/Panel.qml",
                "qrc:/x/Leaf.qml",
                "qrc:/x/Leaf.qml",
            ),
            (
                "qrc:/app/sub/Panel.qml",
                "Leaf.qml",
                "qrc:/app/sub/Leaf.qml",
            ),
            ("qrc:/app/sub/Panel.qml", "/Leaf.qml", "qrc:/Leaf.qml"),
            (
                "https://host/a/Panel.qml",
                "../Leaf.qml",
                "https://host/Leaf.qml",
            ),
            ("https://host", "Leaf.qml", "https://host/Leaf.qml"),
            ("sub/Panel.qml", "Leaf.qml", "sub/Leaf.qml"),
        ] {
            assert_eq!(join_url(base, rel), want, "{rel} at {base}");
        }
    }

    #[test]
    fn an_address_naming_a_directory_keeps_its_slash() {
        for (base, rel, want) in [
            ("file:///a/Main.qml", "sub/", "file:///a/sub/"),
            ("file:///a/Main.qml", ".", "file:///a/"),
            ("file:///a/Main.qml", "sub/..", "file:///a/"),
            ("file:///a/Main.qml", "sub/.", "file:///a/sub/"),
            ("file:///a/Main.qml", "../..", "file:///"),
            ("https://host", ".", "https://host/"),
            ("qrc:/app/Main.qml", "sub/", "qrc:/app/sub/"),
            ("app/Main.qml", "sub/", "app/sub/"),
            ("Main.qml", ".", ""),
        ] {
            assert_eq!(join_url(base, rel), want, "{rel} at {base}");
        }
    }

    #[test]
    fn a_map_source_finds_the_key_of_its_own_urls() {
        let plain = MapSource::new();
        assert_eq!(plain.key(&plain.url("sub/Leaf")), "sub/Leaf");
        assert_eq!(plain.key("Main.qml"), "Main");
        assert_eq!(plain.key("Main"), "Main");
        let based = MapSource::new().with_base_url("qrc:/app/");
        assert_eq!(based.url("sub/Leaf"), "qrc:/app/sub/Leaf.qml");
        assert_eq!(based.key("qrc:/app/sub/Leaf.qml"), "sub/Leaf");
        assert_eq!(based.key("sub/Leaf.qml"), "sub/Leaf");
    }

    #[test]
    fn a_file_with_its_own_url_is_found_by_it() {
        let mut source = MapSource::new().with_base_url("qrc:/app");
        source.insert_url("Main", "file:///home/a/Main.qml");
        source.insert_url("Lib/Brick", "file:///opt/shared/Lib/Brick.qml");
        assert_eq!(source.url("Main"), "file:///home/a/Main.qml");
        assert_eq!(source.url("Other"), "qrc:/app/Other.qml");
        assert_eq!(source.key("file:///home/a/Main.qml?generation=2"), "Main");
        assert_eq!(source.key("file:///opt/shared/Lib/Brick.qml"), "Lib/Brick");
        assert_eq!(source.key("qrc:/app/Other.qml"), "Other");
        // A file moved: its old address names nothing of it.
        source.insert_url("Main", "file:///home/b/Main.qml");
        assert_eq!(source.key("file:///home/b/Main.qml"), "Main");
        assert_ne!(source.key("file:///home/a/Main.qml"), "Main");
    }

    #[test]
    fn a_map_source_says_what_it_holds() {
        // The compiled files are counted in `ljos`'s dev compile tests.
        let mut source = MapSource::new();
        source.insert_failure("d", "2:3: Expected token `}`");
        source.insert_failure("c", "4:1: Unexpected token");
        assert_eq!(source.keys().count(), 0);
        assert_eq!(
            source.failures().collect::<Vec<_>>(),
            [
                ("c", "4:1: Unexpected token"),
                ("d", "2:3: Expected token `}`")
            ]
        );
        let debug = format!("{source:?}");
        assert!(debug.contains("failures: [\"c\", \"d\"]"), "{debug}");
    }

    #[test]
    fn a_query_and_a_fragment_do_not_name_another_file() {
        for (url, want) in [
            ("Panel.qml?generation=3", "Panel"),
            ("file:///sub/Panel.qml?theme=dark&x=1#top", "sub/Panel"),
            ("Panel.qml#top", "Panel"),
            ("Panel.qml", "Panel"),
        ] {
            assert_eq!(plain_key(url), want, "{url}");
        }
        let engine = Engine::new();
        assert_eq!(engine.url_key("Panel.qml?generation=3"), "Panel");
        engine.set_component_source(MapSource::new().with_base_url("qrc:/app"));
        assert_eq!(
            engine.url_key("qrc:/app/sub/Leaf.qml?generation=9"),
            "sub/Leaf"
        );
    }
}
