//! Dev reload on the compiler's side (ARCHITECTURE.md §12.1): what changed
//! between two compilations of a project, as a [`ReloadPatch`], and the
//! [`ReloadSession`] that keeps a project compiled while its files change.
//!
//! The diff matches objects by identity (id, then objectName, then type and
//! place among siblings) and never looks inside a binding: two expressions
//! are the same when their text hash and the names they use agree.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use ljos_ir::{
    CompIdx, CompiledFile, ComponentDef, ExprId, FilePatch, ImportSource, KeySeg, Literal, Members,
    ObjIdx, ObjRef, ObjectDef, ObjectKey, ObjectValue, ReloadOp, ReloadPatch, Resolved,
};
use qml_front::TypeOracle;

use crate::{Diagnostic, Error, Project, Severity, compile_file, reload_lint};

/// The compiled files of a project, by key.
pub type FileSet = BTreeMap<String, Arc<CompiledFile>>;

/// The patch that takes an application running `old` to `new`.
///
/// A file is in the patch whenever its compiled form differs at all (a
/// moved line changes where an error is reported), also when no op says why.
#[must_use]
pub fn diff(old: &FileSet, new: &FileSet, base: u64, generation: u64) -> ReloadPatch {
    let mut patch = ReloadPatch {
        generation,
        base,
        files: Vec::new(),
        removed: Vec::new(),
        ops: Vec::new(),
    };
    for (key, was) in old {
        match new.get(key) {
            None => {
                patch.removed.push(key.clone());
                patch.ops.push(ReloadOp::FileRemoved { file: key.clone() });
            }
            Some(now) if now != was => {
                patch.ops.extend(diff_file(was, now));
                patch.files.push(FilePatch {
                    file: CompiledFile::clone(now),
                    added: false,
                });
            }
            Some(_) => {}
        }
    }
    for (key, now) in new {
        if !old.contains_key(key) {
            patch.ops.push(ReloadOp::FileAdded { file: key.clone() });
            patch.files.push(FilePatch {
                file: CompiledFile::clone(now),
                added: true,
            });
        }
    }
    patch
}

/// What changed in one file.
#[must_use]
pub fn diff_file(old: &CompiledFile, new: &CompiledFile) -> Vec<ReloadOp> {
    let file = &new.file;
    let mut ops = Vec::new();

    let imports = |f: &CompiledFile| -> BTreeSet<(String, Option<String>, Option<String>)> {
        f.imports
            .iter()
            .map(|i| {
                let source = match &i.source {
                    ImportSource::Module(s) | ImportSource::Path(s) => s.clone(),
                };
                (source, i.version.clone(), i.alias.clone())
            })
            .collect()
    };
    let (was, now) = (imports(old), imports(new));
    for (source, ..) in was.difference(&now) {
        ops.push(ReloadOp::ImportRemoved {
            file: file.clone(),
            source: source.clone(),
        });
    }
    for (source, ..) in now.difference(&was) {
        ops.push(ReloadOp::ImportAdded {
            file: file.clone(),
            source: source.clone(),
        });
    }

    let pragmas = |f: &CompiledFile| -> BTreeMap<String, Vec<Option<String>>> {
        let mut out: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
        for p in &f.pragmas {
            out.entry(p.name.clone()).or_default().push(p.value.clone());
        }
        out
    };
    let (was, now) = (pragmas(old), pragmas(new));
    for name in was.keys().chain(now.keys()).collect::<BTreeSet<_>>() {
        if was.get(name) != now.get(name) {
            ops.push(ReloadOp::PragmaChanged {
                file: file.clone(),
                name: name.clone(),
            });
        }
    }

    let by_id = |f: &CompiledFile| -> HashMap<String, CompIdx> {
        f.components
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.clone(), CompIdx(i as u32)))
            .collect()
    };
    let (was, now) = (by_id(old), by_id(new));
    for (i, c) in old.components.iter().enumerate() {
        match now.get(&c.id) {
            None => ops.push(ReloadOp::ComponentRemoved {
                file: file.clone(),
                component: c.id.clone(),
            }),
            Some(&n) => {
                let side = Sides {
                    old,
                    new,
                    old_comp: CompIdx(i as u32),
                    new_comp: n,
                };
                side.diff_component(&mut ops);
            }
        }
    }
    for c in &new.components {
        if !was.contains_key(&c.id) {
            ops.push(ReloadOp::ComponentAdded {
                file: file.clone(),
                component: c.id.clone(),
            });
        }
    }
    ops
}

/// One component of a file, before and after.
struct Sides<'a> {
    old: &'a CompiledFile,
    new: &'a CompiledFile,
    old_comp: CompIdx,
    new_comp: CompIdx,
}

/// What a property is given, without where it is written.
#[derive(PartialEq)]
enum Given<'a> {
    Value(&'a Literal),
    /// The index of an expression in its own file: compared through
    /// [`Sides::same_expr`], never by this number.
    Expr(ExprId),
    /// The identities of the objects, or the ids of the components.
    Objects(Vec<String>, bool),
}

impl Sides<'_> {
    fn file(&self) -> String {
        self.new.file.clone()
    }

    fn diff_component(&self, ops: &mut Vec<ReloadOp>) {
        let (was, now) = (
            self.old.component(self.old_comp),
            self.new.component(self.new_comp),
        );
        if was.signature != now.signature {
            ops.push(ReloadOp::SignatureChanged {
                file: self.file(),
                component: now.id.clone(),
            });
        }
        let (old_ids, new_ids) = (identities(was), identities(now));
        let mut new_by_id: HashMap<&str, ObjIdx> = HashMap::new();
        for (i, id) in new_ids.iter().enumerate() {
            new_by_id.insert(id, ObjIdx(i as u32));
        }
        let mut matched_new: HashSet<ObjIdx> = HashSet::new();
        // Old index to new index, for the order check below.
        let mut pairs: Vec<(ObjIdx, ObjIdx)> = Vec::new();
        for (i, id) in old_ids.iter().enumerate() {
            let o = ObjIdx(i as u32);
            let Some(&n) = new_by_id.get(id.as_str()) else {
                ops.push(ReloadOp::ObjectRemoved {
                    file: self.file(),
                    object: self.old_key(o),
                });
                continue;
            };
            matched_new.insert(n);
            pairs.push((o, n));
        }
        let moved = self.moved(was, now, &pairs);
        for &(o, n) in &pairs {
            let (od, nd) = (&was.objects[o.index()], &now.objects[n.index()]);
            if od.ty != nd.ty {
                ops.push(ReloadOp::TypeChanged {
                    file: self.file(),
                    object: self.new_key(n),
                });
                continue;
            }
            let (from, to) = (self.old_key(o), self.new_key(n));
            if from != to || moved.contains(&n) {
                ops.push(ReloadOp::ObjectMoved {
                    file: self.file(),
                    from,
                    to,
                });
            }
            self.diff_object(od, nd, n, &old_ids, &new_ids, ops);
        }
        for i in 0..now.objects.len() {
            let n = ObjIdx(i as u32);
            if !matched_new.contains(&n) {
                ops.push(ReloadOp::ObjectAdded {
                    file: self.file(),
                    object: self.new_key(n),
                });
            }
        }
    }

    /// The matched objects whose place among their matched siblings is not
    /// what it was.
    fn moved(
        &self,
        was: &ComponentDef,
        now: &ComponentDef,
        pairs: &[(ObjIdx, ObjIdx)],
    ) -> HashSet<ObjIdx> {
        let to_new: HashMap<ObjIdx, ObjIdx> = pairs.iter().copied().collect();
        let to_old: HashMap<ObjIdx, ObjIdx> = pairs.iter().map(|&(o, n)| (n, o)).collect();
        let mut out = HashSet::new();
        for &(op, np) in pairs {
            // What both parents have, in each one's order.
            let before: Vec<ObjIdx> = children_of(was, op)
                .filter_map(|c| to_new.get(&c).copied())
                .filter(|n| now.objects[n.index()].parent == Some(np))
                .collect();
            let after: Vec<ObjIdx> = children_of(now, np)
                .filter(|n| {
                    to_old
                        .get(n)
                        .is_some_and(|o| was.objects[o.index()].parent == Some(op))
                })
                .collect();
            for (b, a) in before.iter().zip(&after) {
                if b != a {
                    out.insert(*a);
                }
            }
        }
        out
    }

    fn old_key(&self, o: ObjIdx) -> ObjectKey {
        self.old.object_key(ObjRef {
            component: self.old_comp,
            object: o,
        })
    }

    fn new_key(&self, o: ObjIdx) -> ObjectKey {
        self.new.object_key(ObjRef {
            component: self.new_comp,
            object: o,
        })
    }

    fn diff_object<'a>(
        &self,
        was: &'a ObjectDef,
        now: &'a ObjectDef,
        at: ObjIdx,
        old_ids: &[String],
        new_ids: &[String],
        ops: &mut Vec<ReloadOp>,
    ) {
        let key = self.new_key(at);
        let mut properties: BTreeSet<String> = BTreeSet::new();
        let mut handlers: BTreeSet<String> = BTreeSet::new();
        let mut declarations: BTreeSet<String> = BTreeSet::new();

        // Declarations: `property int n`, `required n`.
        let decl = |d: &ObjectDef| -> BTreeMap<String, String> {
            d.properties
                .iter()
                .map(|p| {
                    let alias = p.alias.as_ref().map(|a| (a.id.clone(), a.path.clone()));
                    (
                        p.name.clone(),
                        format!(
                            "{} {} {} {} {} {alias:?}",
                            p.ty, p.is_list, p.is_default, p.is_readonly, p.is_required
                        ),
                    )
                })
                .chain(d.required.iter().map(|n| (format!("{n}\0"), String::new())))
                .collect()
        };
        let (a, b) = (decl(was), decl(now));
        for name in a.keys().chain(b.keys()) {
            if a.get(name) != b.get(name) {
                properties.insert(name.trim_end_matches('\0').to_owned());
            }
        }

        let named = |d: &ObjectDef| -> BTreeMap<String, String> {
            let signals = d
                .signals
                .iter()
                .map(|s| (s.name.clone(), format!("{:?}", s.params)));
            let enums = d
                .enums
                .iter()
                .map(|e| (e.name.clone(), format!("{:?}", e.keys)));
            signals.chain(enums).collect()
        };
        let (a, b) = (named(was), named(now));
        for name in a.keys().chain(b.keys()) {
            if a.get(name) != b.get(name) {
                declarations.insert(name.clone());
            }
        }
        let functions = |d: &ObjectDef| -> BTreeMap<String, ExprId> {
            d.functions
                .iter()
                .map(|f| (f.name.clone(), f.expr))
                .collect()
        };
        let (a, b) = (functions(was), functions(now));
        for name in a.keys().chain(b.keys()) {
            let same = match (a.get(name), b.get(name)) {
                (Some(&x), Some(&y)) => self.same_expr(x, y),
                _ => false,
            };
            if !same {
                declarations.insert(name.clone());
            }
        }
        if was.on != now.on {
            declarations.insert("on".to_owned());
        }
        let (old_groups, new_groups) = (groups(was), groups(now));
        let given = |list: &[(String, &'a Members)], f: &CompiledFile, ids: &[String]| {
            let mut out: BTreeMap<String, Vec<Given<'a>>> = BTreeMap::new();
            let mut signals: BTreeMap<String, Vec<ExprId>> = BTreeMap::new();
            for (prefix, m) in list {
                for a in &m.assignments {
                    out.entry(format!("{prefix}{}", a.name))
                        .or_default()
                        .push(Given::Value(&a.value));
                }
                for b in &m.bindings {
                    out.entry(format!("{prefix}{}", b.name))
                        .or_default()
                        .push(Given::Expr(b.expr));
                }
                for o in &m.objects {
                    let names = o.values.iter().map(|v| value_name(f, ids, *v)).collect();
                    out.entry(format!("{prefix}{}", o.name))
                        .or_default()
                        .push(Given::Objects(names, o.is_list));
                }
                for h in &m.handlers {
                    signals
                        .entry(format!("{prefix}{}", h.signal))
                        .or_default()
                        .push(h.expr);
                }
            }
            (out, signals)
        };
        let (a, a_signals) = given(&old_groups, self.old, old_ids);
        let (b, b_signals) = given(&new_groups, self.new, new_ids);
        for name in a.keys().chain(b.keys()) {
            let same = match (a.get(name), b.get(name)) {
                (Some(x), Some(y)) => {
                    x.len() == y.len() && x.iter().zip(y).all(|(x, y)| self.same_given(x, y))
                }
                _ => false,
            };
            if !same {
                properties.insert(name.clone());
            }
        }
        for name in a_signals.keys().chain(b_signals.keys()) {
            let same = match (a_signals.get(name), b_signals.get(name)) {
                (Some(x), Some(y)) => {
                    x.len() == y.len() && x.iter().zip(y).all(|(&x, &y)| self.same_expr(x, y))
                }
                _ => false,
            };
            if !same {
                handlers.insert(name.clone());
            }
        }

        let file = self.file();
        ops.extend(
            properties
                .into_iter()
                .map(|property| ReloadOp::PropertyChanged {
                    file: file.clone(),
                    object: key.clone(),
                    property,
                }),
        );
        ops.extend(handlers.into_iter().map(|signal| ReloadOp::HandlerChanged {
            file: file.clone(),
            object: key.clone(),
            signal,
        }));
        ops.extend(
            declarations
                .into_iter()
                .map(|name| ReloadOp::DeclarationChanged {
                    file: file.clone(),
                    object: key.clone(),
                    name,
                }),
        );
    }

    fn same_given(&self, a: &Given<'_>, b: &Given<'_>) -> bool {
        match (a, b) {
            (Given::Expr(x), Given::Expr(y)) => self.same_expr(*x, *y),
            (Given::Expr(_), _) | (_, Given::Expr(_)) => false,
            _ => a == b,
        }
    }

    /// Whether two expressions mean the same: the same text, taking the
    /// same arguments, with every free name resolved to the same kind of
    /// thing. Indices of objects are left out: they move when an object is
    /// added before another, and the text names an object by its id anyway.
    fn same_expr(&self, a: ExprId, b: ExprId) -> bool {
        let (a, b) = (self.old.expr(a), self.new.expr(b));
        a.hash == b.hash
            && a.kind == b.kind
            && a.args == b.args
            && a.refs.len() == b.refs.len()
            && a.refs
                .iter()
                .zip(&b.refs)
                .all(|(x, y)| x.name == y.name && same_resolved(&x.resolved, &y.resolved))
    }
}

/// What an object sets, by the prefix its names get: none for its own
/// members, `anchors.` for a group, `Layout.` for what a type attaches.
fn groups(d: &ObjectDef) -> Vec<(String, &Members)> {
    let mut out = vec![(String::new(), &d.members)];
    for g in &d.grouped {
        out.push((format!("{}.", g.path.join(".")), &g.members));
    }
    for a in &d.attached {
        out.push((format!("{}.", a.written), &a.members));
    }
    out
}

fn same_resolved(a: &Resolved, b: &Resolved) -> bool {
    match (a, b) {
        (Resolved::Id(_), Resolved::Id(_))
        | (Resolved::OuterId(_), Resolved::OuterId(_))
        | (Resolved::OuterProp(_), Resolved::OuterProp(_)) => true,
        (Resolved::ContextProp { fallback: x, .. }, Resolved::ContextProp { fallback: y, .. }) => {
            match (x, y) {
                (Some(x), Some(y)) => same_resolved(x, y),
                (None, None) => true,
                _ => false,
            }
        }
        _ => a == b,
    }
}

fn children_of(c: &ComponentDef, parent: ObjIdx) -> impl Iterator<Item = ObjIdx> + '_ {
    c.objects
        .iter()
        .enumerate()
        .filter(move |(_, o)| o.parent == Some(parent))
        .map(|(i, _)| ObjIdx(i as u32))
}

/// How an object value is named in a comparison: an object by its identity,
/// a template by its component's id.
fn value_name(file: &CompiledFile, ids: &[String], v: ObjectValue) -> String {
    match v {
        ObjectValue::Object(o) => ids[o.index()].clone(),
        ObjectValue::Component(c) => format!("component {}", file.component(c).id),
    }
}

/// The identity of every object of a component: what makes it the same
/// object in another version of the file. An id is one whatever the object
/// is written in; a name or a place counts below the parent's identity.
/// Two objects that would share one (siblings with one objectName) are
/// numbered in order.
fn identities(component: &ComponentDef) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(component.objects.len());
    let mut seen: HashMap<String, u32> = HashMap::new();
    for def in &component.objects {
        let id = match (&def.key, def.parent) {
            (_, None) => String::new(),
            (KeySeg::Id(id), _) => format!("#{id}"),
            (seg, Some(parent)) => format!("{}/{}", out[parent.index()], seg_name(seg)),
        };
        let n = seen.entry(id.clone()).or_insert(0);
        *n += 1;
        out.push(if *n > 1 { format!("{id}~{n}") } else { id });
    }
    out
}

fn seg_name(seg: &KeySeg) -> String {
    match seg {
        KeySeg::Id(id) => format!("#{id}"),
        KeySeg::Named(name) => format!("'{name}'"),
        KeySeg::ModelRow(row) => format!("[{row}]"),
        KeySeg::Typed { ty, nth } => format!("{ty}[{nth}]"),
    }
}

/// Something wrong with a file, where a developer finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileFault {
    /// The file's path from the project's root, with `.qml` (or the
    /// script's own suffix).
    pub file: String,
    /// 1-based; 0 when the place is not known.
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl std::fmt::Display for CompileFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.file, self.line, self.column, self.message
        )
    }
}

impl CompileFault {
    /// Why the file cannot be made, as a loader says it after the file's
    /// URL ([`crate::failure_reason`]).
    #[must_use]
    pub fn reason(&self) -> String {
        if self.line == 0 {
            self.message.clone()
        } else {
            format!("{}:{}: {}", self.line, self.column, self.message)
        }
    }

    fn of(key: &str, d: &Diagnostic) -> Self {
        Self {
            file: format!("{key}.qml"),
            line: d.span.line,
            column: d.span.column,
            message: d.message.clone(),
        }
    }
}

/// A project kept compiled while its files change.
///
/// It knows what the application runs (the set it was last told was
/// applied) and what the files now say, and gives the patch from the one to
/// the other. Only what a change can affect is compiled again: the changed
/// files and the files that name them.
///
/// A file that does not compile when the session starts does not stop it:
/// the application runs without it, as a program that never makes an object
/// of the file runs, and [`failures`](Self::failures) says why it is
/// missing. Such a file stops no later build until it is saved; a save that
/// does not compile is refused, as any other is.
#[derive(Debug)]
pub struct ReloadSession<O> {
    oracle: O,
    root: Option<PathBuf>,
    sources: BTreeMap<String, String>,
    project: Project<O>,
    /// What the application runs.
    live: FileSet,
    /// The files the application runs without, because they did not
    /// compile.
    live_failed: BTreeSet<String>,
    /// The files that do not compile now and are let be: they did not
    /// compile in what the application runs either, and were not saved
    /// since.
    failed: BTreeMap<String, Vec<CompileFault>>,
    /// What the sources say, for every file that is not dirty.
    compiled: FileSet,
    dirty: BTreeSet<String>,
    all_dirty: bool,
    /// The names only a running program resolves, at the last build: when
    /// the set changes, every file may compile differently.
    dynamic: Option<HashSet<String>>,
    generation: u64,
    last: u64,
    pending: Option<(u64, FileSet, BTreeSet<String>)>,
    warnings: BTreeMap<String, Vec<CompileFault>>,
    /// The scripts saved since the start that do not parse: no build is
    /// offered while one is.
    broken_scripts: BTreeMap<String, CompileFault>,
}

impl<O: TypeOracle + Clone> ReloadSession<O> {
    /// A session over files given by [`set_file`](Self::set_file).
    pub fn in_memory(oracle: O) -> Self {
        Self {
            project: Project::new(oracle.clone()),
            oracle,
            root: None,
            sources: BTreeMap::new(),
            live: FileSet::new(),
            live_failed: BTreeSet::new(),
            failed: BTreeMap::new(),
            compiled: FileSet::new(),
            dirty: BTreeSet::new(),
            all_dirty: true,
            dynamic: None,
            generation: 0,
            last: 0,
            pending: None,
            warnings: BTreeMap::new(),
            broken_scripts: BTreeMap::new(),
        }
    }

    /// A session over every QML file under `root`, as [`Project::load`]
    /// finds them.
    pub fn open(root: &Path, oracle: O) -> Result<Self, Error> {
        let mut session = Self::in_memory(oracle);
        session.root = Some(root.to_owned());
        session.rescan()?;
        Ok(session)
    }

    /// The directory of a session made by [`open`](Self::open).
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Reads the directory again: for a file that was added or removed, and
    /// for a `qmldir` or a `CMakeLists.txt` that changed what a module has.
    pub fn rescan(&mut self) -> Result<(), Error> {
        let Some(root) = self.root.clone() else {
            return Ok(());
        };
        self.project = Project::load(&root, self.oracle.clone())?;
        // A broken script stays refused until it is mended or gone.
        let project = &self.project;
        self.broken_scripts.retain(|key, _| {
            project.script(key).is_some_and(|text| {
                crate::script::syntax_error(text, key.ends_with(".mjs")).is_some()
            })
        });
        self.sources = self
            .project
            .files()
            .map(|key| {
                let text = self.project.source(key).unwrap_or_default().to_owned();
                (key.to_owned(), text)
            })
            .collect();
        self.all_dirty = true;
        Ok(())
    }

    /// The key of a path under the session's root: its path from there
    /// without `.qml`, or with its suffix for a JavaScript file. None for
    /// a path that is neither of the project.
    pub fn key_of(&self, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(self.root.as_deref()?).ok()?;
        let text = relative.to_string_lossy().replace('\\', "/");
        if is_script(&text) {
            return Some(text);
        }
        text.strip_suffix(".qml").map(str::to_owned)
    }

    /// The file `key` now has this text. Whether it is new or not. A key
    /// ending in `.js` or `.mjs` is a JavaScript file.
    pub fn set_file(&mut self, key: &str, source: impl Into<String>) {
        let source = source.into();
        if is_script(key) {
            if self.project.script(key) != Some(source.as_str()) {
                match crate::script::syntax_error(&source, key.ends_with(".mjs")) {
                    Some((line, column, message)) => {
                        let file = key.to_owned();
                        let fault = CompileFault {
                            file,
                            line,
                            column,
                            message,
                        };
                        self.broken_scripts.insert(key.to_owned(), fault);
                    }
                    None => {
                        self.broken_scripts.remove(key);
                    }
                }
                self.project.add_script(key, source);
                self.script_changed(key);
            }
            return;
        }
        if self.sources.get(key) == Some(&source) {
            return;
        }
        if !self.sources.contains_key(key) {
            // Any file may have been waiting for a type of this name.
            self.all_dirty = true;
        }
        self.project.add(key, source.as_str());
        self.sources.insert(key.to_owned(), source);
        self.dirty.insert(key.to_owned());
    }

    /// The file `key` is gone.
    pub fn remove_file(&mut self, key: &str) {
        if is_script(key) {
            self.broken_scripts.remove(key);
            if self.project.script(key).is_some() {
                self.project.remove_script(key);
                self.script_changed(key);
            }
            return;
        }
        if self.sources.remove(key).is_none() {
            return;
        }
        // A project cannot forget a file, so it is made again without it.
        // A module's types come from the directory, which a session over
        // files in memory does not have.
        let mut project = Project::new(self.oracle.clone());
        for (key, source) in &self.sources {
            project.add(key, source.as_str());
        }
        for (path, text) in self.project.scripts() {
            project.add_script(path, text);
        }
        self.project = project;
        self.all_dirty = true;
    }

    /// The text of a file as the session has it, a JavaScript file's too.
    pub fn source(&self, key: &str) -> Option<&str> {
        if is_script(key) {
            return self.project.script(key);
        }
        self.sources.get(key).map(String::as_str)
    }

    /// The JavaScript files: their keys and their text.
    pub fn scripts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.project.scripts()
    }

    /// The script `key` changed: a QML file that names it is compiled
    /// again, as the script is part of what it compiles to. A script may
    /// import another, so when one names it every file is.
    fn script_changed(&mut self, key: &str) {
        let name = key.rsplit('/').next().unwrap_or(key);
        if self
            .project
            .scripts()
            .any(|(other, text)| other != key && text.contains(name))
        {
            self.all_dirty = true;
            return;
        }
        for (file, source) in &self.sources {
            if source.contains(name) {
                self.dirty.insert(file.clone());
            }
        }
    }

    /// The keys of the files, in order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }

    /// What the application runs: the files of the last acknowledged build.
    pub fn live(&self) -> &FileSet {
        &self.live
    }

    /// The files that do not compile, with why, which the application runs
    /// without: whatever makes an object of one is told the first reason.
    pub fn failures(&self) -> impl Iterator<Item = (&str, &[CompileFault])> {
        self.failed.iter().map(|(k, f)| (k.as_str(), f.as_slice()))
    }

    /// The generation the application is at.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Warnings of the files as they now are: what the compiler could not
    /// settle, and the reload lint.
    pub fn warnings(&self) -> impl Iterator<Item = &CompileFault> {
        self.warnings.values().flatten()
    }

    /// Compiles what changed. `Ok(None)` when the files say what the
    /// application already runs; `Err` with every error when a file does
    /// not compile, and then nothing of the change is offered.
    pub fn build(&mut self) -> Result<Option<ReloadPatch>, Vec<CompileFault>> {
        // What imports a broken script would run without its functions.
        if !self.broken_scripts.is_empty() {
            return Err(self.broken_scripts.values().cloned().collect());
        }
        self.compile_dirty(false)?;
        let failed: BTreeSet<String> = self.failed.keys().cloned().collect();
        if failed == self.live_failed
            && self.compiled.len() == self.live.len()
            && self
                .compiled
                .iter()
                .zip(&self.live)
                .all(|((ka, a), (kb, b))| ka == kb && (Arc::ptr_eq(a, b) || a == b))
        {
            self.pending = None;
            return Ok(None);
        }
        self.last += 1;
        let mut patch = diff(&self.live, &self.compiled, self.generation, self.last);
        // A file that never compiled and is gone now: the application
        // forgets why it was missing.
        for key in self.live_failed.difference(&failed) {
            if !self.compiled.contains_key(key) && !patch.removed.contains(key) {
                patch.removed.push(key.clone());
            }
        }
        self.pending = Some((self.last, self.compiled.clone(), failed));
        Ok(Some(patch))
    }

    /// Compiles everything and takes it as what the application runs, at
    /// generation 0: how a session starts. A file that does not compile is
    /// left out and told by [`failures`](Self::failures).
    pub fn start(&mut self) -> &FileSet {
        self.all_dirty = true;
        if let Err(faults) = self.compile_dirty(true) {
            unreachable!("a start lets every file fail: {faults:?}");
        }
        self.live = self.compiled.clone();
        self.live_failed = self.failed.keys().cloned().collect();
        self.pending = None;
        &self.live
    }

    /// The application applied the patch of this generation.
    pub fn ack(&mut self, generation: u64) {
        if let Some((g, files, failed)) = self.pending.take() {
            if g == generation {
                self.live = files;
                self.live_failed = failed;
                self.generation = generation;
            } else {
                self.pending = Some((g, files, failed));
            }
        }
    }

    /// The application refused the patch of this generation: the next build
    /// starts again from what it runs.
    pub fn reject(&mut self, generation: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(g, ..)| *g == generation)
        {
            self.pending = None;
        }
    }

    /// `tolerate`: every file may fail, as at the start. Otherwise only a
    /// file the application already runs without, and which was not saved
    /// since, may.
    fn compile_dirty(&mut self, tolerate: bool) -> Result<(), Vec<CompileFault>> {
        let dynamic = self.project.front().dynamic_names(self.project.oracle());
        if self.dynamic.as_ref() != Some(&dynamic) {
            self.all_dirty = true;
        }
        let todo: BTreeSet<String> = if self.all_dirty {
            self.sources.keys().cloned().collect()
        } else {
            self.affected()
        };
        let mut faults = Vec::new();
        let mut done: Vec<(String, Arc<CompiledFile>, Vec<CompileFault>)> = Vec::new();
        let mut let_be: Vec<(String, Vec<CompileFault>)> = Vec::new();
        for key in &todo {
            let errors: Vec<CompileFault> = match compile_file(&self.project, key) {
                Err(diagnostics) => {
                    let mut errors: Vec<CompileFault> = diagnostics
                        .iter()
                        .map(|d| CompileFault::of(key, d))
                        .collect();
                    if errors.is_empty() {
                        errors.push(CompileFault {
                            file: format!("{key}.qml"),
                            line: 0,
                            column: 0,
                            message: "the file does not compile".to_owned(),
                        });
                    }
                    errors
                }
                Ok(c) => {
                    let (errors, warnings): (Vec<_>, Vec<_>) = c
                        .diagnostics
                        .iter()
                        .partition(|d| d.severity == Severity::Error);
                    if errors.is_empty() {
                        let warnings = warnings
                            .into_iter()
                            .map(|d| CompileFault::of(key, d))
                            .collect();
                        done.push((key.clone(), Arc::new(c.file), warnings));
                    }
                    errors
                        .into_iter()
                        .map(|d| CompileFault::of(key, d))
                        .collect()
                }
            };
            if errors.is_empty() {
                continue;
            }
            if tolerate || (self.live_failed.contains(key) && !self.dirty.contains(key)) {
                let_be.push((key.clone(), errors));
            } else {
                faults.extend(errors);
            }
        }
        if !faults.is_empty() {
            // What is dirty stays dirty: the next build tries all of it again.
            return Err(faults);
        }
        for (key, file, warnings) in done {
            self.failed.remove(&key);
            self.compiled.insert(key.clone(), file);
            self.warnings.insert(key, warnings);
        }
        for (key, errors) in let_be {
            self.compiled.remove(&key);
            self.warnings.remove(&key);
            self.failed.insert(key, errors);
        }
        self.compiled
            .retain(|key, _| self.sources.contains_key(key));
        self.failed.retain(|key, _| self.sources.contains_key(key));
        self.warnings
            .retain(|key, _| self.sources.contains_key(key));
        for key in &todo {
            let Some(file) = self.compiled.get(key) else {
                continue;
            };
            let lint = reload_lint::lint(file, &|other| self.compiled.get(other).map(Arc::as_ref));
            let lint = lint.iter().map(|d| CompileFault::of(key, d));
            self.warnings.entry(key.clone()).or_default().extend(lint);
        }
        self.dirty.clear();
        self.all_dirty = false;
        self.dynamic = Some(dynamic);
        Ok(())
    }

    /// The dirty files and every file that may compile differently because
    /// of them: a file names another by its type, so one that does not have
    /// the type's name in its text cannot depend on it. A file that is a
    /// type of a module may go by another name there, so then every file
    /// is taken.
    fn affected(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        let mut todo: Vec<String> = self.dirty.iter().cloned().collect();
        while let Some(key) = todo.pop() {
            if !self.sources.contains_key(&key) || !out.insert(key.clone()) {
                continue;
            }
            if !self.project.front().modules_of(&key).is_empty() {
                return self.sources.keys().cloned().collect();
            }
            let name = key.rsplit('/').next().unwrap_or(&key);
            for (other, source) in &self.sources {
                if !out.contains(other) && source.contains(name) {
                    todo.push(other.clone());
                }
            }
        }
        out
    }
}

/// Whether the key or path names a JavaScript file.
fn is_script(key: &str) -> bool {
    key.ends_with(".js") || key.ends_with(".mjs")
}
