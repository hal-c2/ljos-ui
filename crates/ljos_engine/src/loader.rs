//! Where compiled files come from, and what the engine keeps per file
//! (contract item 5): the IR, its JS module (loaded once, from bytecode
//! when the source has it), the type made for each declaring object, and
//! the bindings simple enough to run without JS.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use ljos_ir::{BindingKind, CompIdx, CompiledFile, ComponentKind, FnIndex, ImportSource};
use ljos_js::{Bytecode, FnId, JsValue};

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::meta::MetaRef;
use crate::value::Value;

/// Gives the engine compiled files by key (`"Main"`, `"controls/Button"`).
/// A file's components refer to other files by the same keys.
pub trait ComponentSource {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>>;

    /// Whether the source has the file `key`, without reading it.
    fn contains(&self, key: &str) -> bool {
        self.load(key).is_some()
    }

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
    let dir = path.rfind('/').map_or("", |i| &path[..=i]);
    // Mostly a plain name beside a document whose address is plain: put
    // together as it is, without taking either apart (an image's source,
    // for each of hundreds of items made).
    let rooted = !root.is_empty() || dir.starts_with('/');
    if is_plain(rel) && !rel.starts_with('/') && is_plain(dir) && (dir.starts_with('/') || !rooted)
    {
        let mut out = String::with_capacity(root.len() + dir.len() + rel.len());
        out.push_str(root);
        out.push_str(dir);
        out.push_str(rel);
        return out;
    }
    normalised_join(root, path, rel)
}

/// Whether `path` has no segment that names where it is or its parent,
/// and no empty one but at either end.
fn is_plain(path: &str) -> bool {
    let b = path.as_bytes();
    let mut start = 0;
    for i in 0..=b.len() {
        if i == b.len() || b[i] == b'/' {
            let seg = &b[start..i];
            let inner = start > 0 && i < b.len();
            if seg == b"." || seg == b".." || (seg.is_empty() && inner) {
                return false;
            }
            start = i + 1;
        }
    }
    true
}

/// [`join_url`] of `rel` at the `path` below `root`, segment by segment.
fn normalised_join(root: &str, path: &str, rel: &str) -> String {
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

impl MapSource {
    /// `url` from the root of the tree a file with its own URL is in: a
    /// file no compile saw (the text of `Qt.createQmlObject` given a
    /// place) is keyed as a file of that tree, so it finds what is beside
    /// it. Where trees nest, the innermost one is taken.
    fn beside_own<'a>(&self, url: &'a str) -> Option<&'a str> {
        self.urls
            .iter()
            .filter_map(|(key, own)| {
                let root = own.strip_suffix(".qml")?.strip_suffix(key.as_str())?;
                root.ends_with('/').then_some(root)
            })
            .filter_map(|root| Some((root.len(), url.strip_prefix(root)?)))
            .max_by_key(|(len, _)| *len)
            .map(|(_, rest)| rest)
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
        match under_base.or_else(|| self.beside_own(strip_query(url))) {
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
    /// The modules the application's files import or make objects of
    /// ([`Engine::uses_module`]).
    pub(crate) used_modules: HashSet<String>,
    /// What [`Engine::uses_module`] tracks: moved when `used_modules`
    /// changes. Made at the first read; until then nobody follows it.
    pub(crate) modules_node: Option<ljos_reactive::NodeId>,
    /// The modules whose types stand in as empty Items that the log has
    /// said so of, by the first part of their name (`QtQuick3D`): once
    /// each, however many files import them.
    pub(crate) stood_in: HashSet<String>,
}

/// A binding that reads a path of properties from one parameter
/// (`root.width`, `$scope.parent.height`): run in Rust, no JS.
#[derive(Clone, Debug)]
pub(crate) struct Tier0 {
    pub(crate) param: usize,
    pub(crate) path: Vec<Atom>,
    /// Each step of `path`, linked on the type it is read from.
    pub(crate) steps: Box<[crate::plan::Slot]>,
    /// The step and the next as one group leaf (`anchors.top`).
    pub(crate) leaves: Box<[crate::plan::Slot]>,
}

pub(crate) struct LoadedFile {
    pub(crate) key: String,
    pub(crate) url: Atom,
    pub(crate) ir: Arc<CompiledFile>,
    bytecode: Option<Bytecode>,
    /// Function ids by `FnIndex`, once the module is loaded.
    fns: RefCell<Option<Rc<Vec<Option<FnId>>>>>,
    /// Types of declaring objects, by (component, object).
    pub(crate) metas: RefCell<rustc_hash::FxHashMap<(u32, u32), MetaRef>>,
    /// What its components and expressions are linked to: made again
    /// with [`LoadedFile::metas`] when those go.
    plans: RefCell<Rc<crate::plan::Plans>>,
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
    /// The handlers, by expression, already warned of using a signal's
    /// parameters they do not declare.
    pub(crate) injected_warned: RefCell<rustc_hash::FxHashSet<u32>>,
    /// The numbers of the enum literals the compiler left to the runtime
    /// (`Text.WrapMode` where Qt's type table has no numbers), by where the
    /// literal is in [`LoadedFile::ir`], with the type and key it was found
    /// for: a native type's keys are found by name once per file, not once
    /// per object a delegate makes.
    pub(crate) enum_numbers: RefCell<rustc_hash::FxHashMap<usize, (ljos_ir::TypeRef, String, i64)>>,
    /// What [`LoadedFile::bytes`] found: a file does not change once loaded.
    size: OnceCell<usize>,
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
        let plans = RefCell::new(Rc::new(crate::plan::Plans::new(&ir)));
        Self {
            key: key.to_owned(),
            url: Atom::new(&url),
            ir,
            bytecode,
            fns: RefCell::new(None),
            metas: RefCell::default(),
            plans,
            tier0,
            tier1,
            epoch: Cell::new(0),
            text: Cell::new(0),
            injected_warned: RefCell::default(),
            enum_numbers: RefCell::default(),
            size: OnceCell::new(),
        }
    }

    /// About the bytes its compiled form holds: the IR as it is written
    /// out (a floor: in memory its strings and lists have headers too),
    /// its script's bytecode and the paths of its tier-0 bindings. Found
    /// once, when first asked.
    pub(crate) fn bytes(&self) -> usize {
        *self.size.get_or_init(|| {
            self.ir.to_bytes().map_or(0, |b| b.len())
                + self.bytecode.as_ref().map_or(0, |b| b.as_bytes().len())
                + self.plans.borrow().bytes()
                + self.tier0.capacity() * std::mem::size_of::<Option<Tier0>>()
                + self
                    .tier0
                    .iter()
                    .flatten()
                    .map(|t| t.path.capacity() * std::mem::size_of::<Atom>())
                    .sum::<usize>()
        })
    }

    /// The file's plans (plan.rs).
    pub(crate) fn plans(&self) -> Rc<crate::plan::Plans> {
        self.plans.borrow().clone()
    }

    /// Drops the plans: the types they were linked against are made anew.
    pub(crate) fn unlink(&self) {
        *self.plans.borrow_mut() = Rc::new(crate::plan::Plans::new(&self.ir));
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
        // QuickJS keeps every module it loaded. A reload that leaves a
        // file's script as it was (a value edited on its own line) takes
        // the module it loaded before, which a file the reload did not
        // touch does anyway.
        let reusable = self.text.get() == 0 && self.bytecode.is_none();
        if reusable && let Some((name, fns)) = js.loaded_module(&self.key, &self.ir.js.source) {
            js.lines.borrow_mut().add(&name, &self.url, &self.ir);
            return Ok(fns);
        }
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
            EngineError::js(e)
        })?;
        js.lines.borrow_mut().add(&handle.name, &self.url, &self.ir);
        // A module that names `Qt` has its own, which reads a relative
        // address from this file (incubator/dynamic.rs).
        if let Some(at) = handle.function(crate::incubator::dynamic::AT_EXPORT) {
            js.host
                .call(at, &[JsValue::String(self.url.to_string())])
                .map_err(|e| {
                    engine.report_js_fault(&e);
                    EngineError::js(e)
                })?;
        }
        let fns: Vec<Option<FnId>> = self
            .ir
            .js
            .functions
            .iter()
            .map(|f| handle.function(&f.export))
            .collect();
        if reusable {
            js.keep_module(&self.key, &self.ir.js.source, &handle.name, &fns);
        }
        Ok(fns)
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
                    steps: path.iter().map(|_| Default::default()).collect(),
                    leaves: path.iter().map(|_| Default::default()).collect(),
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
        l.used_modules.clear();
        drop(l);
        self.singletons_sources_changed();
    }

    /// The source [`Engine::set_component_source`] set: for a layer that
    /// answers what it lacks and puts itself in its place.
    pub fn component_source(&self) -> Option<Rc<dyn ComponentSource>> {
        self.inner.loader.borrow().source.clone()
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
                .and_then(|h| h.location())
                .map(|l| l.url)
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
        Some(store.objects.get(obj)?.location()?.url)
    }

    /// The key of the file that declares `obj`, as the component source
    /// names it.
    pub(crate) fn document_key(&self, obj: ObjectId) -> Option<String> {
        let url = self.document_url(obj)?;
        let source = self.inner.loader.borrow().source.clone()?;
        Some(source.key(&url))
    }

    /// Whether the file `key` is loaded: an object of it was made since the
    /// files were last given. What a program reaches, for a development
    /// host that tells what the compiler says of those files only.
    pub fn has_loaded(&self, key: &str) -> bool {
        self.inner.loader.borrow().files.contains_key(key)
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
        self.add_loaded(key, source.url(key), ir, source.bytecode(key))
    }

    /// A file the source lacks, at `url`, which a script or the host asked
    /// for by its address: read from the disk and compiled with the text
    /// compiler when the program has one (a development build), as Qt
    /// loads any file it is given. `why` is what the source said.
    pub(crate) fn load_file_from_disk(
        &self,
        key: &str,
        url: &str,
        why: String,
    ) -> Result<Rc<LoadedFile>, EngineError> {
        let url = strip_query(url);
        let Some(path) = url_path(url) else {
            return Err(EngineError::Load(why));
        };
        // Only a file that is there is read: one stat, and only for an
        // address the source does not know.
        if !path.is_file() {
            return Err(EngineError::Load(format!(
                "{url}: No such file or directory"
            )));
        }
        let Some(compiled) = self.compile_from_disk(key, url, &path) else {
            return Err(EngineError::Load(format!(
                "{url}: this program runs compiled QML and has no compiler for a file \
                 it was not built with"
            )));
        };
        let ir = compiled.map_err(EngineError::Load)?;
        self.add_loaded(key, url.to_owned(), Arc::new(ir), None)
    }

    fn add_loaded(
        &self,
        key: &str,
        url: String,
        ir: Arc<CompiledFile>,
        bytecode: Option<Bytecode>,
    ) -> Result<Rc<LoadedFile>, EngineError> {
        if ir.version != ljos_ir::IR_VERSION {
            return Err(EngineError::Load(format!(
                "{url}: compiled for IR version {}, this engine reads {}",
                ir.version,
                ljos_ir::IR_VERSION
            )));
        }
        self.check_imports(&url, &ir)?;
        let used = modules_of(&ir);
        let file = Rc::new(LoadedFile::new(key, url, ir, bytecode));
        file.epoch.set(self.inner.reload.epoch.get());
        let mut loader = self.inner.loader.borrow_mut();
        let len = loader.used_modules.len();
        loader.used_modules.extend(used);
        let grew = loader.used_modules.len() != len;
        loader.files.insert(key.to_owned(), file.clone());
        drop(loader);
        if grew {
            self.modules_changed();
        }
        Ok(file)
    }

    /// Whether a file of the application imports the module `uri`, or makes
    /// an object of a type the compiler found in it. A style's palette is
    /// the application's when it uses the style: `import QtQuick.Controls`
    /// compiled for a style makes objects of that style's module.
    ///
    /// Tracked: what reads it runs again when a file loaded later, or a
    /// reload, changes the modules the application uses.
    pub fn uses_module(&self, uri: &str) -> bool {
        let node = self.inner.loader.borrow().modules_node;
        let node = node.or_else(|| {
            let g = self.graph();
            let n = g.property(g.root(), Value::Int(0)).ok()?;
            self.inner.loader.borrow_mut().modules_node = Some(n);
            Some(n)
        });
        if let Some(n) = node {
            self.graph().get(n);
        }
        self.inner.loader.borrow().used_modules.contains(uri)
    }

    /// The modules the application imports that Ljós does not have, whose
    /// types stand in as empty Items, by the first part of their name
    /// (`QtQuick3D`), sorted: what they would show is not in the scene.
    pub fn stood_in_modules(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .inner
            .loader
            .borrow()
            .stood_in
            .iter()
            .cloned()
            .collect();
        out.sort();
        out
    }

    fn modules_changed(&self) {
        let node = self.inner.loader.borrow().modules_node;
        if let Some(n) = node {
            let g = self.graph();
            let Value::Int(at) = g.get_untracked(n) else {
                return;
            };
            g.set(n, Value::Int(at.wrapping_add(1)));
        }
    }

    /// After a reload, the modules are those of the files the objects that
    /// stand were declared in, as a fresh load of the new files loads
    /// them: a file edited or no longer used takes back what it used.
    /// The root of a component file carries its user's location, so the
    /// files of the instances that stand are counted too.
    pub(crate) fn recount_modules(&self) {
        let urls: HashSet<Atom> = {
            let store = self.inner.store.borrow();
            store
                .objects
                .values()
                .filter_map(|h| Some(h.location()?.url))
                .collect()
        };
        let keys: HashSet<String> = {
            let store = self.inner.store.borrow();
            self.inner
                .reload
                .instances
                .borrow()
                .iter()
                .filter(|i| {
                    i.objects
                        .first()
                        .is_some_and(|o| store.objects.contains_key(*o))
                })
                .map(|i| i.file.key.clone())
                .collect()
        };
        let mut l = self.inner.loader.borrow_mut();
        let now: HashSet<String> = l
            .files
            .values()
            .filter(|f| urls.contains(&f.url) || keys.contains(&f.key))
            .flat_map(|f| modules_of(&f.ir))
            .collect();
        if now == l.used_modules {
            return;
        }
        l.used_modules = now;
        drop(l);
        self.modules_changed();
    }
}

/// The modules `ir` imports or makes objects of, the project's own left out.
fn modules_of(ir: &CompiledFile) -> Vec<String> {
    let imported = ir.imports.iter().filter_map(|i| match &i.source {
        ImportSource::Module(uri) if !i.in_project => Some(uri),
        _ => None,
    });
    let made = ir
        .components
        .iter()
        .flat_map(|c| &c.objects)
        .filter_map(|o| match &o.ty {
            ljos_ir::TypeRef::Native { module, .. } => Some(module),
            _ => None,
        });
    let mut out: Vec<String> = imported.chain(made).cloned().collect();
    out.sort_unstable();
    out.dedup();
    out
}

impl Engine {
    /// Refuses a file that imports a module the engine does not have,
    /// unless the compiler made its types stand in as empty Items: then
    /// the log says so, once for the module.
    pub(crate) fn check_imports(&self, url: &str, ir: &CompiledFile) -> Result<(), EngineError> {
        for import in &ir.imports {
            let ImportSource::Module(uri) = &import.source else {
                continue;
            };
            if import.in_project || self.has_module(uri) {
                continue;
            }
            if !import.stands_in {
                return Err(EngineError::Load(format!(
                    "{url}: module \"{uri}\" is not installed"
                )));
            }
            // `QtQuick3D.Helpers` comes with `QtQuick3D`: one word for both.
            // `QtQuick.Timeline` is a module of its own all the same.
            let family = match uri.split_once('.') {
                Some(("QtQuick", _)) | None => uri.as_str(),
                Some((first, _)) => first,
            };
            if self
                .inner
                .loader
                .borrow_mut()
                .stood_in
                .insert(family.to_owned())
            {
                self.warn(format!(
                    "{url}: module \"{uri}\" is not part of Ljós: the types of {family} and the \
                     modules below it are empty Items, and what only they have is left out"
                ));
            }
        }
        Ok(())
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
    fn a_plain_name_is_joined_as_segment_by_segment() {
        let bases = [
            "file:///a/Main.qml",
            "file:///Main.qml",
            "file:///a/./b/Main.qml",
            "file:///a//Main.qml",
            "file://host",
            "qrc:/app/Main.qml",
            "qrc:app/Main.qml",
            "https://host/a/../Main.qml",
            "sub/Panel.qml",
            "/abs/Panel.qml",
            "Main.qml",
            "",
        ];
        let rels = [
            "gfx/red.png",
            "red.png",
            "sub/",
            "./red.png",
            "../red.png",
            "a//b.png",
            "/red.png",
            ".",
            "..",
            "",
        ];
        for base in bases {
            for rel in rels {
                let (root, path) = match base.find("://") {
                    Some(i) => {
                        let rest = &base[i + 3..];
                        base.split_at(i + 3 + rest.find('/').unwrap_or(rest.len()))
                    }
                    None => match base.find(':') {
                        Some(i) => base.split_at(i + 1),
                        None => ("", base),
                    },
                };
                assert_eq!(
                    join_url(base, rel),
                    normalised_join(root, path, rel),
                    "{rel} at {base}"
                );
            }
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
    fn a_url_no_file_has_is_keyed_in_the_tree_beside_it() {
        let mut source = MapSource::new();
        source.insert_url("Main", "file:///home/a/Main.qml");
        source.insert_url("Lib/Brick", "file:///opt/shared/Lib/Brick.qml");
        source.insert_url("Inner", "file:///opt/shared/Lib/inner/Inner.qml");
        assert_eq!(
            source.key("file:///home/a/parts/inline.qml"),
            "parts/inline"
        );
        assert_eq!(source.key("file:///opt/shared/Lib/made.qml"), "Lib/made");
        assert_eq!(source.key("file:///opt/shared/Lib/inner/x.qml"), "x");
        assert_eq!(source.key("file:///elsewhere/x.qml"), "elsewhere/x");
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
