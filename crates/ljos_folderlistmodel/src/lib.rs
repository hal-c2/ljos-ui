//! The `Qt.labs.folderlistmodel` module for Ljós: `FolderListModel`, a
//! read-only list model of the files and folders in a folder.
//!
//! This is an opt-in module crate written against the public `ljos`
//! facade alone. A host registers it like any native module:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_folderlistmodel::register(&engine);
//! ```
//!
//! QML then says `import Qt.labs.folderlistmodel`:
//!
//! ```qml
//! ListView {
//!     model: FolderListModel { folder: "file:///home/user/Pictures"; nameFilters: ["*.png"] }
//!     delegate: Text { required property string fileName; text: fileName }
//! }
//! ```
//!
//! ## The surface
//!
//! Matching Qt 6.11.2 as probed on `qml6` (the scenarios in
//! `features/models/folder-list-model.feature`): `folder`,
//! `rootFolder`, `parentFolder`, `nameFilters`, `showDirs`,
//! `showDirsFirst`, `showDotAndDotDot`, `showFiles`, `showHidden`,
//! `showOnlyReadable`, `caseSensitive`, `sortCaseSensitive`,
//! `sortField` (`Unsorted`, `Name`, `Time`, `Size`, `Type`),
//! `sortReversed`, `count` and `status` (`Null`, `Ready`, `Loading`);
//! `get(index, role)`, `indexOf(url)` and `isFolder(index)`. A row has
//! `fileName`, `filePath`, `fileUrl`, `fileBaseName` (up to the first
//! dot), `fileSuffix` (after it), `fileSize`, `fileModified` and
//! `fileAccessed` (`Date`s) and `fileIsDir`.
//!
//! - The folder starts as the working directory. One that is not a
//!   folder when the model completes is replaced by it, as Qt does;
//!   set later, it empties the model and the status is `Null`.
//! - Name filters (`*`, `?` and `[...]`) pick files only; folders all
//!   show. A name starting with a dot is hidden.
//! - `Name` orders by code point (or ignoring case), `Time` newest
//!   first, `Size` largest first, `Type` by suffix; ties go by name.
//!   `sortReversed` turns the order round, and `showDirsFirst` then
//!   puts the folders first.
//! - `showOnlyReadable` asks the owner's read bit.
//! - `folder` reads as it was written: Ljós does not resolve a relative
//!   one in place, but lists the folder it resolves to.
//!
//! The folder is read on a thread of its own and the rows arrive as a
//! message: `status` is `Loading` until then. Nothing polls: the model
//! does not follow later changes on disk, as Qt's does.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ljos::prelude::*;

const CATEGORY: &str = "qml";

mod status {
    pub const NULL: i32 = 0;
    pub const READY: i32 = 1;
    pub const LOADING: i32 = 2;
}

mod field {
    pub const UNSORTED: i32 = 0;
    pub const NAME: i32 = 1;
    pub const TIME: i32 = 2;
    pub const SIZE: i32 = 3;
    pub const TYPE: i32 = 4;
}

fn enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "SortField",
            vec![
                ("Unsorted", i64::from(field::UNSORTED)),
                ("Name", i64::from(field::NAME)),
                ("Time", i64::from(field::TIME)),
                ("Size", i64::from(field::SIZE)),
                ("Type", i64::from(field::TYPE)),
            ],
        ),
        (
            "Status",
            vec![
                ("Null", i64::from(status::NULL)),
                ("Ready", i64::from(status::READY)),
                ("Loading", i64::from(status::LOADING)),
            ],
        ),
    ]
}

/// Registers `FolderListModel` (`Qt.labs.folderlistmodel`) on `engine`:
/// the one seam a host calls.
pub fn register(engine: &ljos::Engine) {
    engine.register_module(ljos::module!("Qt.labs.folderlistmodel" 1.0 => [FolderListModel]));
}

const ROLES: [&str; 9] = [
    "fileName",
    "filePath",
    "fileUrl",
    "fileBaseName",
    "fileSuffix",
    "fileSize",
    "fileModified",
    "fileAccessed",
    "fileIsDir",
];

/// One row as the model shows it.
#[derive(Debug)]
struct Row {
    name: String,
    path: String,
    url: String,
    size: f64,
    modified: Value,
    accessed: Value,
    is_dir: bool,
}

impl Row {
    fn get(&self, role: &str) -> Value {
        match role {
            "fileName" => Value::from(self.name.as_str()),
            "filePath" => Value::from(self.path.as_str()),
            "fileUrl" => Value::Url(Atom::from(self.url.as_str())),
            "fileBaseName" => Value::from(base_name(&self.name)),
            "fileSuffix" => Value::from(suffix(&self.name)),
            "fileSize" => Value::Real(self.size),
            "fileModified" => self.modified.clone(),
            "fileAccessed" => self.accessed.clone(),
            "fileIsDir" => Value::Bool(self.is_dir),
            _ => Value::Undefined,
        }
    }
}

/// Up to the first dot, as Qt's `fileBaseName` has it.
fn base_name(name: &str) -> &str {
    name.split_once('.').map_or(name, |(b, _)| b)
}

/// After the first dot: `tar.gz` for `a.tar.gz`, as Qt's `fileSuffix`.
fn suffix(name: &str) -> &str {
    name.split_once('.').map_or("", |(_, s)| s)
}

#[derive(Debug, Default)]
struct Shared {
    rows: Vec<Row>,
    signals: Option<ModelSignals>,
}

struct Rows(Rc<RefCell<Shared>>);

impl QmlListModel for Rows {
    fn row_count(&self) -> usize {
        self.0.borrow().rows.len()
    }

    fn role_names(&self) -> Vec<Atom> {
        ROLES.iter().map(|r| Atom::from(*r)).collect()
    }

    fn data(&self, row: usize, role: &str) -> Value {
        self.0
            .borrow()
            .rows
            .get(row)
            .map_or(Value::Undefined, |r| r.get(role))
    }
}

/// `FolderListModel` (Qt.labs.folderlistmodel): the files and folders
/// in `folder`, one row each.
#[derive(Debug, QmlObject)]
#[qml(
    module = "Qt.labs.folderlistmodel",
    name = "FolderListModel",
    extends = QtObject,
    enums = enums
)]
pub struct FolderListModel {
    #[qml(base)]
    pub object: QtObject,
    /// The folder whose files are the rows.
    #[qml(notify)]
    pub folder: Url,
    /// A folder the application treats as the top.
    #[qml(notify)]
    pub root_folder: Url,
    /// The folder `folder` is in; empty at the top of the file system.
    #[qml(notify, readonly)]
    pub parent_folder: Url,
    /// Patterns a file's name must match one of (`*.png`).
    #[qml(notify)]
    pub name_filters: Vec<Value>,
    #[qml(notify)]
    pub show_dirs: bool,
    #[qml(notify)]
    pub show_dirs_first: bool,
    #[qml(notify)]
    pub show_dot_and_dot_dot: bool,
    #[qml(notify)]
    pub show_files: bool,
    #[qml(notify)]
    pub show_hidden: bool,
    #[qml(notify)]
    pub show_only_readable: bool,
    /// Whether the name filters tell case apart.
    #[qml(notify)]
    pub case_sensitive: bool,
    /// Whether sorting by name tells case apart.
    #[qml(notify)]
    pub sort_case_sensitive: bool,
    #[qml(notify, enumeration = "SortField")]
    pub sort_field: i32,
    #[qml(notify)]
    pub sort_reversed: bool,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    /// Bumped by every load, so an answer to an older one is dropped.
    #[qml(skip)]
    generation: u64,
    /// Whether the model has completed and loaded once.
    #[qml(skip)]
    started: bool,
    #[qml(skip)]
    shared: Rc<RefCell<Shared>>,
}

impl Default for FolderListModel {
    fn default() -> Self {
        FolderListModel {
            object: QtObject::default(),
            folder: Url(Atom::from(
                std::env::current_dir()
                    .map(|d| file_url(&d))
                    .unwrap_or_default()
                    .as_str(),
            )),
            root_folder: Url::default(),
            parent_folder: Url::default(),
            name_filters: vec![Value::from("*")],
            show_dirs: true,
            show_dirs_first: false,
            show_dot_and_dot_dot: false,
            show_files: true,
            show_hidden: false,
            show_only_readable: false,
            case_sensitive: true,
            sort_case_sensitive: true,
            sort_field: field::NAME,
            sort_reversed: false,
            count: 0,
            status: status::NULL,
            generation: 0,
            started: false,
            shared: Rc::default(),
        }
    }
}

/// Gives the object its model face, once, before a view declared after
/// it resolves `model:` at its own completion (see `ljos_xml`).
fn attach(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let shared = e.with(me, |m: &FolderListModel| Rc::clone(&m.shared))?;
    if shared.borrow().signals.is_some() {
        return Ok(());
    }
    let signals = e.set_list_model(me, Rc::new(Rows(Rc::clone(&shared))))?;
    shared.borrow_mut().signals = Some(signals);
    Ok(())
}

/// The properties a listing depends on.
const LISTED_BY: [PropIndex; 12] = [
    FolderListModel::FOLDER.index,
    FolderListModel::NAME_FILTERS.index,
    FolderListModel::SHOW_DIRS.index,
    FolderListModel::SHOW_DIRS_FIRST.index,
    FolderListModel::SHOW_DOT_AND_DOT_DOT.index,
    FolderListModel::SHOW_FILES.index,
    FolderListModel::SHOW_HIDDEN.index,
    FolderListModel::SHOW_ONLY_READABLE.index,
    FolderListModel::CASE_SENSITIVE.index,
    FolderListModel::SORT_CASE_SENSITIVE.index,
    FolderListModel::SORT_FIELD.index,
    FolderListModel::SORT_REVERSED.index,
];

#[qml_impl]
impl FolderListModel {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        attach(e, me)?;
        // Listed first: a folder that is not one when the model completes
        // is replaced, which must not list it again.
        load(e, me);
        for prop in LISTED_BY {
            e.watch(me, prop, move |e, _| load(e, me))?;
        }
        Ok(())
    }

    /// The value of `role` (`"fileName"`) in row `index`; `undefined`
    /// for a row or role there is not.
    #[qml(invokable)]
    fn get(&self, index: Value, role: Value) -> Value {
        let Some(i) = row_index(&index) else {
            return Value::Undefined;
        };
        self.shared
            .borrow()
            .rows
            .get(i)
            .map_or(Value::Undefined, |r| r.get(role.to_js_string().as_str()))
    }

    /// The row of the file at `url`, or -1.
    #[qml(invokable, name = "indexOf")]
    fn index_of(&self, url: Value) -> i32 {
        let url = url.to_js_string();
        self.shared
            .borrow()
            .rows
            .iter()
            .position(|r| r.url == url.as_str())
            .map_or(-1, |i| i as i32)
    }

    /// Whether row `index` is a folder.
    #[qml(invokable, name = "isFolder")]
    fn is_folder(&self, index: Value) -> bool {
        row_index(&index)
            .and_then(|i| self.shared.borrow().rows.get(i).map(|r| r.is_dir))
            .unwrap_or(false)
    }
}

fn row_index(index: &Value) -> Option<usize> {
    let n = index.to_number();
    (n >= 0.0 && n.fract() == 0.0).then_some(n as usize)
}

/// What a load lists by, read once as it starts.
#[derive(Clone, Debug)]
struct Options {
    filters: Vec<String>,
    show_dirs: bool,
    show_files: bool,
    dirs_first: bool,
    dots: bool,
    hidden: bool,
    only_readable: bool,
    case_sensitive: bool,
    sort_case_sensitive: bool,
    sort_field: i32,
    reversed: bool,
}

fn options(m: &FolderListModel) -> Options {
    Options {
        filters: m
            .name_filters
            .iter()
            .map(|f| f.to_js_string().to_string())
            .filter(|f| !f.is_empty())
            .collect(),
        show_dirs: m.show_dirs,
        show_files: m.show_files,
        dirs_first: m.show_dirs_first,
        dots: m.show_dot_and_dot_dot,
        hidden: m.show_hidden,
        only_readable: m.show_only_readable,
        case_sensitive: m.case_sensitive,
        sort_case_sensitive: m.sort_case_sensitive,
        sort_field: m.sort_field,
        reversed: m.sort_reversed,
    }
}

/// A file as the listing thread found it.
#[derive(Debug)]
struct Found {
    name: String,
    is_dir: bool,
    size: u64,
    /// Milliseconds since the UNIX epoch.
    modified: f64,
    accessed: f64,
    /// The owner's read bit.
    readable: bool,
}

/// Starts (or restarts) listing the model's folder. The rows land in
/// [`apply`].
fn load(e: &Engine, me: ObjectId) {
    // A bound option is only pulled into the object when it is read, and
    // at completion nothing has read it yet.
    e.untrack(|| {
        for prop in LISTED_BY {
            e.read(me, prop);
        }
    });
    let Ok((generation, first, opts)) = e.untrack(|| {
        e.with_mut(me, |m: &mut FolderListModel| {
            m.generation += 1;
            let first = !m.started;
            m.started = true;
            (m.generation, first, options(m))
        })
    }) else {
        return;
    };
    let platform = Platform::of(e);
    let env = platform.env();
    let written = e.untrack(|| e.read(me, FolderListModel::FOLDER.index).to_js_string());
    let named = (!written.is_empty())
        .then(|| url_path(&e.resolve_url(me, &written)))
        .flatten();
    let named = match named {
        Some(path) if env.real(&path).is_dir() => Some(path),
        // Qt lists the working directory instead of a folder that is not
        // one when the model completes, and says that is its folder.
        _ if first => std::env::current_dir()
            .ok()
            .filter(|cwd| env.real(cwd).is_dir())
            .inspect(|cwd| {
                let _ = Cx::new(e, me).set(
                    FolderListModel::FOLDER,
                    Url(Atom::from(file_url(cwd).as_str())),
                );
            }),
        _ => None,
    };
    let cx = Cx::new(e, me);
    let parent = named
        .as_deref()
        .and_then(Path::parent)
        .map(file_url)
        .unwrap_or_default();
    // A folder that changes empties the model at once, then reads.
    clear(e, me);
    let _ = cx.set(
        FolderListModel::PARENT_FOLDER,
        Url(Atom::from(parent.as_str())),
    );
    let Some(named) = named else {
        return;
    };
    let _ = cx.set(FolderListModel::STATUS, status::LOADING);

    let real = env.real(&named);
    let reply = platform.expect();
    let spawned = std::thread::Builder::new()
        .name("ljos-folder".to_owned())
        .spawn(move || {
            let found = list(&real, &opts);
            reply.send(move |e| apply(e, me, generation, &named, found));
        });
    if let Err(err) = spawned {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("QML FolderListModel: no thread to read with: {err}"),
        );
        let _ = cx.set(FolderListModel::STATUS, status::NULL);
    }
}

/// No rows, a count of 0 and `Null`, saying so only where they change.
fn clear(e: &Engine, me: ObjectId) {
    let cx = Cx::new(e, me);
    let Ok(shared) = cx.with(|m: &FolderListModel| Rc::clone(&m.shared)) else {
        return;
    };
    let signals = {
        let mut s = shared.borrow_mut();
        if s.rows.is_empty() {
            None
        } else {
            s.rows.clear();
            s.signals.clone()
        }
    };
    // The count is there before the views are told, as on qml6; the
    // status follows, not batched: Qt says the count before it.
    e.batch(|| {
        let _ = cx.set(FolderListModel::COUNT, 0);
        if let Some(signals) = signals {
            signals.model_reset(e);
        }
    });
    let _ = cx.set(FolderListModel::STATUS, status::NULL);
}

/// The rows of `dir`, filtered and sorted as `opts` say.
fn list(dir: &Path, opts: &Options) -> Vec<Found> {
    let mut found = Vec::new();
    if opts.dots && opts.show_dirs {
        for dot in [".", ".."] {
            if let Some(f) = describe(dir.join(dot), dot.to_owned()) {
                found.push(f);
            }
        }
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !opts.hidden && name.starts_with('.') {
            continue;
        }
        // One stat a file, as Qt's listing makes.
        let Some(f) = describe(entry.path(), name) else {
            continue;
        };
        if opts.only_readable && !f.readable {
            continue;
        }
        let shown = if f.is_dir {
            opts.show_dirs
        } else {
            opts.show_files
                && (opts.filters.is_empty()
                    || opts
                        .filters
                        .iter()
                        .any(|p| matches(p, &f.name, opts.case_sensitive)))
        };
        if shown {
            found.push(f);
        }
    }
    sort(&mut found, opts);
    found
}

fn describe(path: PathBuf, name: String) -> Option<Found> {
    // What a link points at, as Qt shows it.
    let meta = std::fs::metadata(&path).ok()?;
    let ms = |t: std::io::Result<std::time::SystemTime>| {
        t.ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0.0, |d| d.as_millis() as f64)
    };
    Some(Found {
        name,
        is_dir: meta.is_dir(),
        size: meta.len(),
        modified: ms(meta.modified()),
        accessed: ms(meta.accessed()),
        readable: readable(&meta),
    })
}

#[cfg(unix)]
fn readable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o400 != 0
}

#[cfg(not(unix))]
fn readable(_meta: &std::fs::Metadata) -> bool {
    true
}

fn sort(found: &mut [Found], opts: &Options) {
    let by_name = |a: &Found, b: &Found| {
        if opts.sort_case_sensitive {
            a.name.cmp(&b.name)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    };
    let last_suffix = |f: &Found| f.name.rsplit_once('.').map_or("", |(_, s)| s).to_owned();
    // Stable, so names equal but for case keep the order they were read in.
    match opts.sort_field {
        field::NAME => found.sort_by(by_name),
        // Whole seconds, as Qt compares them: files written together tie
        // and go by name.
        field::TIME => found.sort_by(|a, b| {
            let (a_s, b_s) = ((a.modified / 1000.0).floor(), (b.modified / 1000.0).floor());
            b_s.partial_cmp(&a_s)
                .unwrap_or(Ordering::Equal)
                .then_with(|| by_name(a, b))
        }),
        field::SIZE => found.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| by_name(a, b))),
        field::TYPE => found.sort_by(|a, b| {
            last_suffix(a)
                .cmp(&last_suffix(b))
                .then_with(|| by_name(a, b))
        }),
        _ => {}
    }
    if opts.reversed {
        found.reverse();
    }
    if opts.dirs_first {
        found.sort_by_key(|f| !f.is_dir);
    }
}

/// Whether `name` matches the wildcard `pattern`: `*`, `?` and `[...]`
/// (with `!` or `^` to negate), as a file dialog's name filter.
fn matches(pattern: &str, name: &str, case_sensitive: bool) -> bool {
    let fold = |s: &str| -> Vec<char> {
        if case_sensitive {
            s.chars().collect()
        } else {
            s.to_lowercase().chars().collect()
        }
    };
    wildcard(&fold(pattern), &fold(name))
}

fn wildcard(p: &[char], s: &[char]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some('*') => (0..=s.len()).any(|i| wildcard(&p[1..], &s[i..])),
        Some('?') => !s.is_empty() && wildcard(&p[1..], &s[1..]),
        Some('[') => {
            let Some(close) = p.iter().skip(2).position(|c| *c == ']').map(|i| i + 2) else {
                return s.first() == Some(&'[') && wildcard(&p[1..], &s[1..]);
            };
            let Some(c) = s.first() else {
                return false;
            };
            let mut set = &p[1..close];
            let negated = matches!(set.first(), Some('!' | '^'));
            if negated {
                set = &set[1..];
            }
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == '-' {
                    hit |= (set[i]..=set[i + 2]).contains(c);
                    i += 3;
                } else {
                    hit |= set[i] == *c;
                    i += 1;
                }
            }
            hit != negated && wildcard(&p[close + 1..], &s[1..])
        }
        Some(c) => s.first() == Some(c) && wildcard(&p[1..], &s[1..]),
    }
}

/// The listing has arrived: the rows replace the old ones.
fn apply(e: &Engine, me: ObjectId, generation: u64, dir: &Path, found: Vec<Found>) {
    let current = e.is_alive(me)
        && e.with(me, |m: &FolderListModel| m.generation == generation)
            .unwrap_or(false);
    if !current {
        return;
    }
    let cx = Cx::new(e, me);
    let Ok(shared) = cx.with(|m: &FolderListModel| Rc::clone(&m.shared)) else {
        return;
    };
    // A script's own Date, as Qt gives `fileModified`.
    let date = e.eval_js("(function (ms) { return new Date(ms) })").ok();
    let at = |ms: f64| {
        date.as_ref()
            .and_then(|f| e.call_js(f, &[Value::Real(ms)]))
            .unwrap_or(Value::Real(ms))
    };
    let rows: Vec<Row> = found
        .into_iter()
        .map(|f| {
            let path = dir.join(&f.name);
            Row {
                url: file_url(&path),
                path: path.to_string_lossy().into_owned(),
                size: f.size as f64,
                modified: at(f.modified),
                accessed: at(f.accessed),
                is_dir: f.is_dir,
                name: f.name,
            }
        })
        .collect();
    let (signals, count) = {
        let mut s = shared.borrow_mut();
        s.rows = rows;
        (s.signals.clone(), s.rows.len() as i32)
    };
    // A view told of the rows finds them counted (measured on qml6).
    e.batch(|| {
        let _ = cx.set(FolderListModel::COUNT, count);
        if let Some(signals) = signals {
            signals.model_reset(e);
        }
    });
    let _ = cx.set(FolderListModel::STATUS, status::READY);
    e.request_wake();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_as_name_filters_do() {
        assert!(matches("*.png", "a.png", true));
        assert!(!matches("*.PNG", "a.png", true));
        assert!(matches("*.PNG", "a.png", false));
        assert!(matches("?.png", "a.png", true));
        assert!(!matches("?.png", "ab.png", true));
        assert!(matches("[ab]*", "b.txt", true));
        assert!(!matches("[!ab]*", "b.txt", true));
        assert!(matches("[a-c].txt", "b.txt", true));
        assert!(matches("*", "", true));
    }

    #[test]
    fn the_base_name_ends_at_the_first_dot() {
        assert_eq!(base_name("a.tar.gz"), "a");
        assert_eq!(suffix("a.tar.gz"), "tar.gz");
        assert_eq!(suffix("README"), "");
    }
}
