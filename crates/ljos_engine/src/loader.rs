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

    /// Where on the disk a file the source lacks is, for `url` (its
    /// address, as a script or the host asked for it): Qt loads whatever
    /// file is at an address. The default is the address's own path. A
    /// source whose addresses are not the disk's says where its files
    /// are, and whether the engine may read one there; a source that
    /// wraps another passes the question on.
    fn on_disk(&self, _key: &str, url: &str) -> OnDisk {
        OnDisk::at(url)
    }

    /// `rel` as the document at `base` writes it, with a step up (`..`),
    /// before it is resolved: `Some` refuses it and is the address to
    /// load instead, one no file has. Resolving folds a step up at the
    /// root away (`../A.qml` beside `file:///Main.qml` is
    /// `file:///A.qml`), so a source that confines what its documents
    /// reach looks at the address as written. Asked only of an address
    /// with no scheme.
    ///
    /// Every source states its rule; there is no default. A source that
    /// wraps another passes the question on, or the wrapped source's
    /// confinement is lost without a word: the reload's patched source
    /// and the shell's fallback once answered `None` this way, and a
    /// `../` saved while an application ran loaded. A source whose files
    /// may reach anywhere `..` resolves to answers `None` itself.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use ljos_engine::ComponentSource;
    ///
    /// struct Wrapper(Box<dyn ComponentSource>);
    ///
    /// impl ComponentSource for Wrapper {
    ///     fn load(&self, key: &str) -> Option<Arc<ljos_ir::CompiledFile>> {
    ///         self.0.load(key)
    ///     }
    ///     fn refuse_step_up(&self, base: &str, rel: &str) -> Option<String> {
    ///         self.0.refuse_step_up(base, rel)
    ///     }
    /// }
    /// ```
    ///
    /// The same wrapper without it does not compile:
    ///
    /// ```compile_fail,E0046
    /// use std::sync::Arc;
    /// use ljos_engine::ComponentSource;
    ///
    /// struct Wrapper(Box<dyn ComponentSource>);
    ///
    /// impl ComponentSource for Wrapper {
    ///     fn load(&self, key: &str) -> Option<Arc<ljos_ir::CompiledFile>> {
    ///         self.0.load(key)
    ///     }
    /// }
    /// ```
    fn refuse_step_up(&self, base: &str, rel: &str) -> Option<String>;
}

/// Where a file a source lacks is on the disk: [`ComponentSource::on_disk`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDisk {
    /// The file at this path is read, when it is there.
    Read(std::path::PathBuf),
    /// Nothing is read: the source reads its files itself, and has none
    /// of the key. A message names the file at this path, where it would
    /// be.
    Missing(std::path::PathBuf),
    /// Nothing is read: the address is the source's own name for a file
    /// it has not got, not a place on the disk (a pack's). A message
    /// names the file by this name, as the application does.
    Absent(String),
    /// The address is not on the disk: the source's reason stands.
    Unknown,
}

impl OnDisk {
    /// The file at the address's own path, as Qt reads it.
    fn at(url: &str) -> Self {
        match url_path(url) {
            Some(path) => Self::Read(path),
            None => Self::Unknown,
        }
    }
}

/// Whether `rel`, an address with no scheme, takes a step up.
fn steps_up(rel: &str) -> bool {
    rel.contains("..")
        && !has_scheme(rel)
        && strip_query(rel).split(['/', '\\']).any(|part| part == "..")
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
/// other URL. `file:a/b` is relative: to the working directory, as in Qt.
pub fn url_path(url: &str) -> Option<std::path::PathBuf> {
    let rest = match url.strip_prefix("file://") {
        Some(rest) => rest.strip_prefix("localhost").unwrap_or(rest),
        None if url.starts_with('/') => url,
        None => url.strip_prefix("file:")?,
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
#[derive(Clone, Default)]
pub struct MapSource {
    files: BTreeMap<String, Arc<CompiledFile>>,
    bytecode: HashMap<String, Bytecode>,
    failures: BTreeMap<String, String>,
    base: Option<String>,
    urls: HashMap<String, String>,
    keys_by_url: HashMap<String, String>,
    trailer: Option<ResourceBytes>,
    /// The files beside the QML that a pack carries (fonts, pictures), by
    /// their paths from the root the keys are from.
    resources: BTreeMap<String, ResourceBytes>,
    /// Other addresses the program names a carried file by
    /// (`qrc:/qt/qml/Shapes/icons/dot.png`), with its path.
    resource_addresses: BTreeMap<String, String>,
    /// Read from a pack ([`from_pack`](Self::from_pack)): the files of a
    /// shipped program, named by their keys and on no disk.
    packed: bool,
}

/// The trailer of a pack: what follows its files, without the length
/// that ends it. A length that does not match means a pack cut short.
fn trailer_of(rest: &[u8]) -> Result<Option<&[u8]>, EngineError> {
    if rest.is_empty() {
        return Ok(None);
    }
    let cut = || EngineError::Load("the pack is cut short".to_owned());
    let (body, len) = rest.split_last_chunk::<4>().ok_or_else(cut)?;
    if usize::try_from(u32::from_le_bytes(*len)).ok() != Some(body.len()) {
        return Err(cut());
    }
    Ok(Some(body))
}

/// What [`MapSource::files_of_pack`] reads: the source, the files the
/// pack carries (still in it), and what follows them.
type PackParts<'a> = (MapSource, Vec<(String, &'a [u8])>, &'a [u8]);

/// Bytes a [`MapSource`] holds besides its compiled files: a file the
/// pack carries ([`MapSource::resources`]) or its trailer. Those of a pack
/// the program embeds are where the program has them, uncopied.
#[derive(Clone)]
pub enum ResourceBytes {
    /// In the program itself ([`MapSource::from_static_pack`]).
    Embedded(&'static [u8]),
    /// Read or given while the program runs.
    Shared(Arc<[u8]>),
}

impl std::ops::Deref for ResourceBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            ResourceBytes::Embedded(bytes) => bytes,
            ResourceBytes::Shared(bytes) => bytes,
        }
    }
}

impl From<&'static [u8]> for ResourceBytes {
    fn from(bytes: &'static [u8]) -> Self {
        ResourceBytes::Embedded(bytes)
    }
}

impl From<Arc<[u8]>> for ResourceBytes {
    fn from(bytes: Arc<[u8]>) -> Self {
        ResourceBytes::Shared(bytes)
    }
}

impl From<Vec<u8>> for ResourceBytes {
    fn from(bytes: Vec<u8>) -> Self {
        ResourceBytes::Shared(bytes.into())
    }
}

impl std::fmt::Debug for ResourceBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} bytes", self.len())
    }
}

impl MapSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// The bytes after the files of the pack this was read from
    /// ([`to_pack_with_trailer`](Self::to_pack_with_trailer)); None when
    /// there were none.
    pub fn trailer(&self) -> Option<&[u8]> {
        self.trailer.as_deref()
    }

    /// Carries `bytes` as the file at `path` (`fonts/Symbols.woff2`, from
    /// the root the keys are from), which a QML file names relative to
    /// itself: the program finds it at [`resource_url`](Self::resource_url).
    pub fn insert_resource(&mut self, path: impl Into<String>, bytes: impl Into<ResourceBytes>) {
        self.resources.insert(path.into(), bytes.into());
    }

    /// The files carried besides the QML, in path order.
    pub fn resources(&self) -> impl Iterator<Item = (&str, &ResourceBytes)> {
        self.resources
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes))
    }

    /// The carried file at `path`, if the source carries it.
    pub fn resource(&self, path: &str) -> Option<&ResourceBytes> {
        self.resources.get(path)
    }

    /// Says the carried file at `path` is also named `address`: the
    /// `qrc:` address of a file of a module, or of one its build lists.
    pub fn insert_resource_address(&mut self, address: impl Into<String>, path: impl Into<String>) {
        self.resource_addresses.insert(address.into(), path.into());
    }

    /// Each other address of a carried file, with its path, in address
    /// order.
    pub fn resource_addresses(&self) -> impl Iterator<Item = (&str, &str)> {
        self.resource_addresses
            .iter()
            .map(|(address, path)| (address.as_str(), path.as_str()))
    }

    /// Where the QML finds the carried file at `path`: beside the files
    /// as [`url`](ComponentSource::url) places them, `{base}/{path}` with
    /// a [`base`](Self::with_base_url), else `file:///{path}`.
    pub fn resource_url(&self, path: &str) -> String {
        match &self.base {
            Some(b) => format!("{}/{path}", b.trim_end_matches('/')),
            None => format!("file:///{path}"),
        }
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
    /// and its IR; then the files that did not compile, each key with
    /// why; then the files carried besides the QML, each path with its
    /// bytes; then their other addresses, each with its path. Every
    /// count, and every string or bytes, is preceded by its length as a
    /// little-endian `u32`, and each list is in key order. Bytecode and
    /// URLs are not packed: a shipped program finds its files by key.
    pub fn to_pack(&self) -> Result<Vec<u8>, EngineError> {
        self.to_pack_with_trailer(&[])
    }

    /// [`to_pack`](Self::to_pack), with `trailer` and its length after
    /// the files: an engine reads the files and keeps the trailer as it
    /// is ([`trailer`](Self::trailer)), so a pack with one reads where a
    /// pack without one does.
    pub fn to_pack_with_trailer(&self, trailer: &[u8]) -> Result<Vec<u8>, EngineError> {
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
        let field = |out: &mut Vec<u8>, bytes: &[u8]| -> Result<(), EngineError> {
            out.extend_from_slice(&len(bytes.len())?);
            out.extend_from_slice(bytes);
            Ok(())
        };
        out.extend_from_slice(&len(self.failures.len())?);
        for (key, why) in &self.failures {
            field(&mut out, key.as_bytes())?;
            field(&mut out, why.as_bytes())?;
        }
        out.extend_from_slice(&len(self.resources.len())?);
        for (path, bytes) in &self.resources {
            field(&mut out, path.as_bytes())?;
            field(&mut out, bytes)?;
        }
        out.extend_from_slice(&len(self.resource_addresses.len())?);
        for (address, path) in &self.resource_addresses {
            field(&mut out, address.as_bytes())?;
            field(&mut out, path.as_bytes())?;
        }
        // Its length after it, so a pack cut short in its trailer is
        // told from one with a shorter trailer.
        if !trailer.is_empty() {
            out.extend_from_slice(trailer);
            out.extend_from_slice(&len(trailer.len())?);
        }
        Ok(out)
    }

    /// The files a [`to_pack`](Self::to_pack) pack holds. A pack that
    /// is cut short, or an entry whose IR is of another version than
    /// this engine reads, is an error: a shipped program embeds the
    /// pack its own build wrote, so either means a broken build.
    pub fn from_pack(pack: &[u8]) -> Result<Self, EngineError> {
        let (mut source, carried, rest) = Self::files_of_pack(pack)?;
        for (path, bytes) in carried {
            source.insert_resource(path, ResourceBytes::Shared(bytes.into()));
        }
        source.trailer = trailer_of(rest)?.map(|t| ResourceBytes::Shared(t.into()));
        Ok(source)
    }

    /// [`from_pack`](Self::from_pack) of a pack the program embeds: the
    /// files it carries and its [`trailer`](Self::trailer) stay where
    /// they are, uncopied.
    pub fn from_static_pack(pack: &'static [u8]) -> Result<Self, EngineError> {
        let (mut source, carried, rest) = Self::files_of_pack(pack)?;
        for (path, bytes) in carried {
            source.insert_resource(path, ResourceBytes::Embedded(bytes));
        }
        source.trailer = trailer_of(rest)?.map(ResourceBytes::Embedded);
        Ok(source)
    }

    /// The files of `pack`, the files it carries besides them (left in
    /// the pack, for the caller to copy or not), and what follows them.
    fn files_of_pack(pack: &[u8]) -> Result<PackParts<'_>, EngineError> {
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
        fn field<'a>(rest: &mut &'a [u8]) -> Result<&'a [u8], EngineError> {
            let n = len(rest)?;
            take(rest, n)
        }
        fn text(rest: &mut &[u8], what: &str) -> Result<String, EngineError> {
            std::str::from_utf8(field(rest)?)
                .map(str::to_owned)
                .map_err(|_| EngineError::Load(format!("a pack {what} is not UTF-8")))
        }
        let mut rest = pack;
        let mut source = Self::new();
        for _ in 0..len(&mut rest)? {
            let key = text(&mut rest, "key")?;
            let file = CompiledFile::from_bytes(field(&mut rest)?)
                .map_err(|e| EngineError::Load(format!("{key}: {e}")))?;
            source.insert(key, file);
        }
        for _ in 0..len(&mut rest)? {
            let key = text(&mut rest, "key")?;
            let why = text(&mut rest, "failure")?;
            source.insert_failure(key, why);
        }
        let mut carried = Vec::new();
        for _ in 0..len(&mut rest)? {
            let path = text(&mut rest, "file path")?;
            carried.push((path, field(&mut rest)?));
        }
        for _ in 0..len(&mut rest)? {
            let address = text(&mut rest, "address")?;
            let path = text(&mut rest, "file path")?;
            source.insert_resource_address(address, path);
        }
        source.packed = true;
        Ok((source, carried, rest))
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

    /// A map's files reach wherever a step up resolves to.
    fn refuse_step_up(&self, _base: &str, _rel: &str) -> Option<String> {
        None
    }

    fn on_disk(&self, key: &str, url: &str) -> OnDisk {
        if self.names_by_key(key, url) {
            return OnDisk::Absent(format!("{key}.qml"));
        }
        OnDisk::at(url)
    }
}

impl MapSource {
    /// Whether `url` is this pack's name for `key`, a file beside its own
    /// that it has not got. A pack with no base names its files by their
    /// keys (`file:///{key}.qml`): a key in a directory of its files is one
    /// of them, and not a place on the disk. An address elsewhere (a file
    /// written since the program was built, given by its path) is the
    /// disk's. A map given its files as text keeps Qt's words, the
    /// address, as a directory compiled into one does.
    fn names_by_key(&self, key: &str, url: &str) -> bool {
        fn dir(key: &str) -> &str {
            key.rsplit_once('/').map_or("", |(dir, _)| dir)
        }
        self.packed
            && self.base.is_none()
            && url == format!("file:///{key}.qml")
            && self
                .files
                .keys()
                .chain(self.failures.keys())
                .any(|own| !self.urls.contains_key(own) && dir(own) == dir(key))
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
    /// The modules whose types stand in as empty Items in the files the
    /// source gives, by the first part of their name (`QtGraphs`): what
    /// the design audit reads, so a new source starts it again.
    pub(crate) stood_in: HashSet<String>,
    /// What stands in in the files of a library the program adds types
    /// from ([`Engine::add_component_library`]), by the file's url: those
    /// are loaded once and kept past a new source, and count for the
    /// audit only while the scene has objects made of them.
    pub(crate) library_stood_in: HashMap<String, Vec<String>>,
    /// The roots made of those files, with the file's url. A component
    /// file's root carries its user's location, so it is not found by
    /// where the scene's objects are declared.
    pub(crate) library_roots: Vec<(ObjectId, Atom)>,
    /// How many of those were alive when the list was last swept.
    pub(crate) library_roots_kept: usize,
    /// What the log has said stands in: a module by the first part of its
    /// name (`QtGraphs`), a type of one the runtime builds part of by its
    /// module and name (`QtQuick3D.Helpers.Name`). Once each for the
    /// engine, however many files or sources name them.
    pub(crate) told_stood_in: HashSet<String>,
}

/// What stands in of an import: types of a module the runtime builds
/// part of, or the module itself by its family (`QtGraphs`).
enum StandIn<'i> {
    Types(&'i str, &'i [ljos_ir::StoodIn]),
    Module(&'i str, &'i str),
}

/// What a file has stand in, found as its imports are checked: the
/// module put on record, what the log says it of once, and the saying.
pub(crate) struct Stood {
    module: String,
    told: String,
    message: String,
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
    /// The compiled code the file was matched to, when it first ran an
    /// expression ([`crate::aot`]).
    pub(crate) aot: crate::incubator::aot::LinkedCell,
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
            aot: OnceCell::new(),
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
            engine.report_js_fault(&e, None);
            EngineError::js(e)
        })?;
        js.lines.borrow_mut().add(&handle.name, &self.url, &self.ir);
        // A module that names `Qt` has its own, which reads a relative
        // address from this file (incubator/dynamic.rs).
        if let Some(at) = handle.function(crate::incubator::dynamic::AT_EXPORT) {
            js.host
                .call(at, &[JsValue::String(self.url.to_string())])
                .map_err(|e| {
                    engine.report_js_fault(&e, None);
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
        l.stood_in.clear();
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
            Some(base) if !url.is_empty() => self.join_from(&base, url),
            _ => url.to_owned(),
        }
    }

    /// `rel` as the file at `base` means it ([`join_url`]), unless the
    /// component source refuses the step up it is written with
    /// ([`ComponentSource::refuse_step_up`]).
    pub(crate) fn join_from(&self, base: &str, rel: &str) -> String {
        if steps_up(rel) {
            let source = self.inner.loader.borrow().source.clone();
            if let Some(refused) = source.and_then(|s| s.refuse_step_up(base, rel)) {
                return refused;
            }
        }
        join_url(base, rel)
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

    /// Forgets the loaded files `keys`: an object made of one from here on
    /// has it read from the component source again, for a host whose
    /// source gives another version of a file than it gave. Objects made
    /// already keep the version they were made of.
    pub fn forget_loaded<'k>(&self, keys: impl IntoIterator<Item = &'k str>) {
        let mut l = self.inner.loader.borrow_mut();
        let mut forgot = false;
        for key in keys {
            forgot |= l.files.remove(key).is_some();
        }
        if !forgot {
            return;
        }
        // The type made for an object of a file kept may derive from one
        // of a file forgotten.
        for f in l.files.values() {
            f.metas.borrow_mut().clear();
            f.unlink();
        }
        drop(l);
        self.singletons_sources_changed();
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
        let source = self.inner.loader.borrow().source.clone();
        let found = match &source {
            Some(source) => source.on_disk(key, url),
            None => OnDisk::at(url),
        };
        // Named where it is looked for: a source's addresses need not be
        // the disk's (a user's directory over a pack).
        let missing = |path: &std::path::Path| {
            let named = if url_path(url).as_deref() == Some(path) {
                url.to_owned()
            } else {
                file_url(path)
            };
            EngineError::Load(format!("{named}: No such file or directory"))
        };
        let path = match found {
            OnDisk::Read(path) => path,
            OnDisk::Missing(path) => return Err(missing(&path)),
            OnDisk::Absent(name) => {
                return Err(EngineError::Load(format!(
                    "{name}: No such file or directory"
                )));
            }
            OnDisk::Unknown => return Err(EngineError::Load(why)),
        };
        // Only a file that is there is read: one stat, and only for an
        // address the source does not know.
        if !path.is_file() {
            return Err(missing(&path));
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
        let stood = self.check_imports(&url, &ir)?;
        let used = modules_of(&ir);
        let tell = self.note_stood_in(&url, stood, false);
        let file = Rc::new(LoadedFile::new(key, url, ir, bytecode));
        file.epoch.set(self.inner.reload.epoch.get());
        let mut loader = self.inner.loader.borrow_mut();
        let len = loader.used_modules.len();
        loader.used_modules.extend(used);
        let grew = loader.used_modules.len() != len;
        loader.files.insert(key.to_owned(), file.clone());
        drop(loader);
        // The file and what stands in for it are on record before anyone
        // hears of it: a listener may put another source in.
        for message in tell {
            self.warn(message);
        }
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
    /// (`QtGraphs`), sorted: what they would show is not in the scene.
    pub fn stood_in_modules(&self) -> Vec<String> {
        let mut live = self.live_urls();
        {
            let store = self.inner.store.borrow();
            let mut loader = self.inner.loader.borrow_mut();
            loader
                .library_roots
                .retain(|(root, _)| store.objects.contains_key(*root));
            live.extend(loader.library_roots.iter().map(|(_, url)| url.clone()));
        }
        let loader = self.inner.loader.borrow();
        let library = loader
            .library_stood_in
            .iter()
            .filter(|(url, _)| live.contains(url.as_str()))
            .flat_map(|(_, modules)| modules);
        let mut out: Vec<String> = loader.stood_in.iter().chain(library).cloned().collect();
        out.sort();
        out.dedup();
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

    /// The files the scene has objects of, by url.
    fn live_urls(&self) -> HashSet<Atom> {
        let store = self.inner.store.borrow();
        store
            .objects
            .values()
            .filter_map(|h| Some(h.location()?.url))
            .collect()
    }

    /// After a reload, the modules are those of the files the objects that
    /// stand were declared in, as a fresh load of the new files loads
    /// them: a file edited or no longer used takes back what it used.
    /// The root of a component file carries its user's location, so the
    /// files of the instances that stand are counted too.
    pub(crate) fn recount_modules(&self) {
        let urls = self.live_urls();
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
        let in_use: Vec<Rc<LoadedFile>> = self
            .inner
            .loader
            .borrow()
            .files
            .values()
            .filter(|f| urls.contains(&f.url) || keys.contains(&f.key))
            .cloned()
            .collect();
        let now: HashSet<String> = in_use.iter().flat_map(|f| modules_of(&f.ir)).collect();
        // A reload may have replaced the last type of a module that stood
        // in: the audit reads what stands in for the files in use now.
        let stood_in: HashSet<String> = in_use
            .iter()
            .flat_map(|f| self.stood_in_by(&f.ir))
            .collect();
        let mut l = self.inner.loader.borrow_mut();
        l.stood_in = stood_in;
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
    /// what stands in is given back, the module, or of one the runtime
    /// builds part of, each type of it that stands in, where the file
    /// first names it. It changes nothing: [`Engine::note_stood_in`] puts
    /// that on record once the file is, so a file refused leaves nothing
    /// behind, and the log is told only after both.
    pub(crate) fn check_imports(
        &self,
        url: &str,
        ir: &CompiledFile,
    ) -> Result<Vec<Stood>, EngineError> {
        for import in &ir.imports {
            if let ImportSource::Module(uri) = &import.source
                && !import.in_project
                && import.stood_in.is_none()
                && !import.stands_in
                && !self.has_module(uri)
            {
                return Err(EngineError::Load(format!(
                    "{url}: module \"{uri}\" is not installed"
                )));
            }
        }
        let mut stood = Vec::new();
        for import in &ir.imports {
            match self.stand_in_of(import) {
                Some(StandIn::Types(uri, types)) => stood.extend(types.iter().map(|ty| {
                    let of = if ty.known {
                        format!(" of module \"{uri}\"")
                    } else {
                        String::new()
                    };
                    Stood {
                        module: uri.to_owned(),
                        told: format!("{uri}.{}", ty.name),
                        message: format!(
                            "{url}:{}:{}: type {}{of} is not part of Ljós: it is an empty \
                             Item, and what only it has is left out",
                            ty.span.line, ty.span.column, ty.name
                        ),
                    }
                })),
                Some(StandIn::Module(uri, family)) => stood.push(Stood {
                    module: family.to_owned(),
                    told: family.to_owned(),
                    message: format!(
                        "{url}: module \"{uri}\" is not part of Ljós: the types of {family} \
                         and the modules below it are empty Items, and what only they have is \
                         left out"
                    ),
                }),
                None => {}
            }
        }
        Ok(stood)
    }

    /// Puts on record what stands in for the file at `url`, once it is
    /// loaded: for the source, or for the library file it is. What the
    /// log has not said yet is given back, to be said after.
    pub(crate) fn note_stood_in(&self, url: &str, stood: Vec<Stood>, library: bool) -> Vec<String> {
        let mut loader = self.inner.loader.borrow_mut();
        let mut tell = Vec::new();
        for Stood {
            module,
            told,
            message,
        } in stood
        {
            if library {
                let modules = loader.library_stood_in.entry(url.to_owned()).or_default();
                if !modules.contains(&module) {
                    modules.push(module);
                }
            } else {
                loader.stood_in.insert(module);
            }
            if loader.told_stood_in.insert(told) {
                tell.push(message);
            }
        }
        tell
    }

    /// Notes `root`, made of the file at `url`, when that file is a
    /// library file something of which stands in: the audit counts it
    /// while the root lives.
    pub(crate) fn note_library_root(&self, root: ObjectId, url: &Atom) {
        let mut loader = self.inner.loader.borrow_mut();
        if loader.library_stood_in.is_empty() || !loader.library_stood_in.contains_key(url.as_str())
        {
            return;
        }
        // Delegates come and go: the roots gone are let go of as the list
        // doubles, not only when the audit reads it.
        if loader.library_roots.len() >= loader.library_roots_kept.max(32) * 2
            && let Ok(store) = self.inner.store.try_borrow()
        {
            loader
                .library_roots
                .retain(|(root, _)| store.objects.contains_key(*root));
            loader.library_roots_kept = loader.library_roots.len();
        }
        loader.library_roots.push((root, url.clone()));
    }

    /// What of `import` stands in as empty Items, if anything does.
    fn stand_in_of<'i>(&self, import: &'i ljos_ir::Import) -> Option<StandIn<'i>> {
        let ImportSource::Module(uri) = &import.source else {
            return None;
        };
        if import.in_project {
            return None;
        }
        // Of a module the runtime builds part of, what the file names may
        // all be built: only a type that stands in is told, even where the
        // program registers types of the module.
        if let Some(types) = &import.stood_in {
            return (!types.is_empty()).then_some(StandIn::Types(uri, types));
        }
        if !import.stands_in || self.has_module(uri) {
            return None;
        }
        // `QtWebEngine.Controls` comes with `QtWebEngine`: one word for
        // both. `QtQuick.Timeline` and `QtQuick3D.Helpers` are modules of
        // their own all the same: Ljós has `QtQuick` and `QtQuick3D`.
        let family = match uri.split_once('.') {
            Some(("QtQuick" | "QtQuick3D", _)) | None => uri.as_str(),
            Some((first, _)) => first,
        };
        Some(StandIn::Module(uri, family))
    }

    /// The modules that stand in for the files `ir` of the source: what
    /// [`Engine::stood_in_modules`] lists once a reload has replaced some.
    fn stood_in_by(&self, ir: &CompiledFile) -> Vec<String> {
        ir.imports
            .iter()
            .filter_map(|import| match self.stand_in_of(import)? {
                StandIn::Types(uri, _) => Some(uri.to_owned()),
                StandIn::Module(_, family) => Some(family.to_owned()),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_step_up_is_seen_as_written() {
        assert!(steps_up("../A.qml"));
        assert!(steps_up("parts/../../A.qml"));
        assert!(steps_up("..\\A.qml"));
        assert!(!steps_up("A..B.qml"));
        assert!(!steps_up("https://example.com/../A.qml"));
        assert!(!steps_up("A.qml?from=.."));
    }

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
    fn a_trailer_reads_back_and_a_pack_cut_in_it_is_refused() {
        let source = MapSource::new();
        let plain = source.to_pack().unwrap();
        assert!(MapSource::from_pack(&plain).unwrap().trailer().is_none());
        let pack = source.to_pack_with_trailer(b"sources").unwrap();
        let read = MapSource::from_pack(&pack).unwrap();
        assert_eq!(read.trailer(), Some(&b"sources"[..]));
        // All of it cut is a pack without one.
        for cut in 1..b"sources".len() + 4 {
            assert!(
                MapSource::from_pack(&pack[..pack.len() - cut]).is_err(),
                "cut by {cut}"
            );
        }
    }

    #[test]
    fn a_pack_keeps_its_failures_and_the_files_it_carries() {
        let mut given = MapSource::new();
        given.insert("Main", standing_in("QtQuick", None));
        given.insert_failure("Broken", "3:1: Expected `}`");
        given.insert_resource("fonts/Symbols.woff2", b"wOF2".to_vec());
        given.insert_resource("pics/dot.png", b"\x89PNG".to_vec());
        given.insert_resource_address("qrc:/qt/qml/App/pics/dot.png", "pics/dot.png");
        let pack = given.to_pack_with_trailer(b"sources").unwrap();
        static PACK: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        let embedded: &'static [u8] = PACK.get_or_init(|| pack.clone());
        for read in [
            MapSource::from_pack(&pack).unwrap(),
            MapSource::from_static_pack(embedded).unwrap(),
        ] {
            assert_eq!(read.keys().collect::<Vec<_>>(), ["Main"]);
            assert_eq!(
                read.failures().collect::<Vec<_>>(),
                [("Broken", "3:1: Expected `}`")]
            );
            let carried: Vec<(&str, &[u8])> = read.resources().map(|(p, b)| (p, &**b)).collect();
            assert_eq!(
                carried,
                [
                    ("fonts/Symbols.woff2", &b"wOF2"[..]),
                    ("pics/dot.png", &b"\x89PNG"[..])
                ]
            );
            assert_eq!(
                read.resource_addresses().collect::<Vec<_>>(),
                [("qrc:/qt/qml/App/pics/dot.png", "pics/dot.png")]
            );
            assert_eq!(read.trailer(), Some(&b"sources"[..]));
        }
        // An embedded pack's files are read where the program has them.
        let read = MapSource::from_static_pack(embedded).unwrap();
        let (_, bytes) = read.resources().next().unwrap();
        assert!(
            matches!(bytes, ResourceBytes::Embedded(b) if embedded.as_ptr_range().contains(&b.as_ptr()))
        );
        // Cut anywhere, it is refused, not read as a smaller pack.
        let plain = given.to_pack().unwrap();
        for cut in 1..plain.len() {
            assert!(MapSource::from_pack(&plain[..cut]).is_err(), "cut to {cut}");
        }
    }

    #[test]
    fn a_file_a_pack_lacks_beside_its_own_is_on_no_disk() {
        let mut given = MapSource::new();
        given.insert("Main", standing_in("QtQuick", None));
        given.insert("parts/Card", standing_in("QtQuick", None));
        let pack = MapSource::from_pack(&given.to_pack().unwrap()).unwrap();
        let shell = pack.url("Shell");
        assert_eq!(
            pack.on_disk("Shell", &shell),
            OnDisk::Absent("Shell.qml".into())
        );
        assert_eq!(
            pack.on_disk("parts/Edge", "file:///parts/Edge.qml"),
            OnDisk::Absent("parts/Edge.qml".into())
        );
        // A file given by its path, written since the program was built.
        assert_eq!(
            pack.on_disk("home/u/Late", "file:///home/u/Late.qml"),
            OnDisk::Read("/home/u/Late.qml".into())
        );
        // Files given as text keep Qt's words: the address is read.
        assert_eq!(
            given.on_disk("Shell", &shell),
            OnDisk::Read("/Shell.qml".into())
        );
    }

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

    /// A file that only `module` stands in for: the types it names are
    /// told under the import when `named` is some.
    fn standing_in(module: &str, named: Option<&str>) -> CompiledFile {
        CompiledFile {
            version: ljos_ir::IR_VERSION,
            file: "Main".to_owned(),
            source_hash: [0; 32],
            imports: vec![ljos_ir::Import {
                source: ImportSource::Module(module.to_owned()),
                version: None,
                alias: None,
                in_project: false,
                stands_in: true,
                stood_in: named.map(|name| {
                    vec![ljos_ir::StoodIn {
                        name: name.to_owned(),
                        span: ljos_ir::SourceSpan::default(),
                        known: true,
                    }]
                }),
            }],
            pragmas: Vec::new(),
            components: Vec::new(),
            exprs: Vec::new(),
            translations: Vec::new(),
            js: ljos_ir::JsModule::default(),
            invocable: Vec::new(),
            scripts: Vec::new(),
        }
    }

    /// Checks the file `ir` at `url` and puts what stands in on record,
    /// as a load does: what the log would be told.
    fn noted(engine: &Engine, url: &str, ir: &CompiledFile, library: bool) -> Vec<String> {
        let stood = engine.check_imports(url, ir).unwrap();
        engine.note_stood_in(url, stood, library)
    }

    #[test]
    fn what_stands_in_in_a_library_outlasts_a_new_source() {
        // A library file is loaded once and kept, so its imports are not
        // checked again under the next source: what it stood in for stays
        // on record, and counts while the scene has objects of the file.
        let engine = Engine::new();
        let library = standing_in("QtQuick3D.Helpers", Some("SceneGizmo"));
        let app = standing_in("QtGraphs", None);
        assert_eq!(noted(&engine, "lib:Gizmo", &library, true).len(), 1);
        assert_eq!(noted(&engine, "app:Main", &app, false).len(), 1);
        // No object of the library file is in the scene.
        assert_eq!(engine.stood_in_modules(), ["QtGraphs"]);
        engine.set_component_source(MapSource::new());
        assert!(engine.stood_in_modules().is_empty());
        assert_eq!(
            engine.inner.loader.borrow().library_stood_in["lib:Gizmo"],
            ["QtQuick3D.Helpers"]
        );
    }

    #[test]
    fn a_file_refused_leaves_nothing_on_record() {
        // The stand-in import comes before the one that refuses the file:
        // nothing of it is listed, and the next file is told all the same.
        let engine = Engine::new();
        let mut refused = standing_in("QtGraphs", None);
        refused.imports.push(ljos_ir::Import {
            source: ImportSource::Module("NotInstalled".to_owned()),
            version: None,
            alias: None,
            in_project: false,
            stands_in: false,
            stood_in: None,
        });
        assert!(engine.check_imports("app:Refused", &refused).is_err());
        assert!(engine.stood_in_modules().is_empty());
        let told = noted(&engine, "app:Main", &standing_in("QtGraphs", None), false);
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].contains("module \"QtGraphs\""), "{told:?}");
        assert_eq!(engine.stood_in_modules(), ["QtGraphs"]);
        // Once for the engine, however many files name it.
        assert!(noted(&engine, "app:Other", &standing_in("QtGraphs", None), false).is_empty());
        assert_eq!(engine.stood_in_modules(), ["QtGraphs"]);
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
