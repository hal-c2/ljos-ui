//! A project's files lowered with the files they name, as they come, change
//! and go.
//!
//! A program that lowers its files as they load and `ljos migrate` lower
//! a file the same way (docs/adr/0004-compat-lowering-before-the-ir.md).
//! Each file is lowered alone and its references are read. A file is kept
//! when the project has a script where its rows module would go. Then
//! every file is lowered with its group ([`group`]). A [`Lowering`] is that
//! recipe, and it holds the rule for which files to lower again when one
//! changes. It reads the project only through [`Project`], so it does no
//! I/O of its own.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use qml_syntax::Allocator;

use crate::{Alone, Cause, Kept, Locals, Outcome, Root, group, group::references_in};

/// What a [`Lowering`] reads of the project its files are in, keyed as the
/// lowering's files are.
pub trait Project {
    /// What the pass is told of the project around the file `key`. A file
    /// it names that `held` holds is read from there, not from the project's
    /// copy or the disk.
    fn locals<'a>(&'a self, key: &'a str, held: &'a dyn Held) -> Box<dyn Locals + 'a>;

    /// The files of the project's module `uri`, which the file `key`
    /// imports, as the project's loader finds them: a module's files may be
    /// anywhere a build or a `qmldir` says.
    fn module_files(&self, key: &str, uri: &str) -> Vec<String>;

    /// The script the project has at `path`, where the file `key`'s rows
    /// module would go ([`rows_path`]).
    fn script(&self, key: &str, path: &str) -> Option<ProjectScript<'_>>;
}

/// A script of the project's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectScript<'p> {
    /// Its path as the project names it, for a keep reason: `ljos migrate`
    /// keys the files by their paths on the disk, a project by its own.
    pub path: String,
    pub text: &'p str,
}

/// The files a lowering holds, as the project around one of them reads
/// the others.
pub trait Held {
    /// The file `key` as it is written, when it is held.
    fn text(&self, key: &str) -> Option<&str>;

    /// The root object of the file `key`, when it is held: `Some(None)` for
    /// a held file that has none.
    fn root(&self, key: &str) -> Option<Option<Root>>;
}

impl<H: Held + ?Sized> Held for &H {
    fn text(&self, key: &str) -> Option<&str> {
        (**self).text(key)
    }

    fn root(&self, key: &str) -> Option<Option<Root>> {
        (**self).root(key)
    }
}

/// What changed around the files that some of them read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Changed<'a> {
    /// The controls style, which is what `import QtQuick.Controls` means.
    Style,
    /// What the project's module `uri` has: the files that import it.
    Module(&'a str),
    /// Whether the directory `dir` (a key) is a module's, which says its
    /// types instead of its files: the files that import it by path.
    Dir(&'a str),
    /// The script at `path`: the file whose rows module goes there.
    Script(&'a str),
}

/// What the project is to do with a script after [`Lowering::settle`]: the
/// rows modules of the files that are lowered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptEdit {
    Put { path: String, text: String },
    Drop(String),
}

/// A file as it was put.
#[derive(Debug)]
struct File {
    text: String,
    /// Whether it may be lowered: `ljos migrate` reads the files around the
    /// ones it is named, and writes none of them.
    named: bool,
}

/// The files of a project, lowered alone and with their groups.
#[derive(Debug, Default)]
pub struct Lowering {
    files: BTreeMap<String, File>,
    /// Each file's root object, from the parse that lowered it, or from one
    /// of its own when another file read it first.
    roots: RefCell<BTreeMap<String, Option<Root>>>,
    alone: BTreeMap<String, Alone>,
    lowered: BTreeMap<String, Outcome>,
    /// Per lowered file, the scripts the pass wrote beside it: the row
    /// types of its constant `ListModel`s (docs/DIALECT.md §4.8).
    generated: BTreeMap<String, Vec<String>>,
    /// The files to lower again.
    stale: BTreeSet<String>,
}

impl Lowering {
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds the file `key` as `text`. It is lowered on the next
    /// [`settle`](Self::settle), with each file beside it that names it,
    /// as the root the pass read for them is now another or none.
    pub fn put(&mut self, key: &str, text: String) {
        self.hold(key, text, true);
    }

    /// [`put`](Self::put) for a file that is read, never written, as it is
    /// where the files that name it read it already: one the pass would
    /// lower is kept, so a file named is not lowered to match a file that
    /// stays Qt's. The files that name it are not lowered again, as what
    /// they read of it is the same.
    pub fn put_read_only(&mut self, key: &str, text: String) {
        self.hold(key, text, false);
    }

    fn hold(&mut self, key: &str, text: String, named: bool) {
        self.files.insert(key.to_string(), File { text, named });
        self.roots.get_mut().remove(key);
        self.stale.insert(key.to_string());
        if named {
            self.stale.extend(self.naming(key));
        }
    }

    /// Lets go of the file `key`; for a file it lowered, the scripts the
    /// pass wrote beside it, which go with it. The files beside it that
    /// named it are lowered on the next [`settle`](Self::settle).
    pub fn forget(&mut self, key: &str) -> Option<Vec<String>> {
        self.files.remove(key);
        self.roots.get_mut().remove(key);
        self.stale.remove(key);
        self.alone.remove(key)?;
        self.lowered.remove(key);
        self.stale.extend(self.naming(key));
        Some(self.generated.remove(key).unwrap_or_default())
    }

    /// The files beside `key` that name it, by its type or by a name
    /// lowering brings in, and the files that import its directory by
    /// path and bring its name in: it is a type of the directory to them.
    fn naming(&self, key: &str) -> Vec<String> {
        let (dir, name) = key.rsplit_once('/').unwrap_or(("", key));
        self.alone
            .iter()
            .filter(|(other, alone)| {
                let other_dir = other.rsplit_once('/').map_or("", |(d, _)| d);
                let brings = alone.references.introduced.iter().any(|t| t == name);
                let imports = || {
                    (alone.references.dirs.iter())
                        .any(|path| group::may_import(other_dir, path, dir))
                };
                other.as_str() != key
                    && ((other_dir == dir
                        && (brings || alone.references.types.iter().any(|t| t == name)))
                        || (brings && imports()))
            })
            .map(|(other, _)| other.clone())
            .collect()
    }

    /// Marks the files that read what changed, to lower on the next
    /// [`lower`](Self::lower) or [`settle`](Self::settle). Whether there was
    /// one.
    pub fn changed(&mut self, what: Changed<'_>) -> bool {
        let again: Vec<String> = match what {
            Changed::Style => self.files.keys().cloned().collect(),
            Changed::Module(uri) => self
                .alone
                .iter()
                .filter(|(_, alone)| alone.references.modules.iter().any(|m| m == uri))
                .map(|(key, _)| key.clone())
                .collect(),
            Changed::Dir(dir) => (self.alone.iter())
                .filter(|(key, alone)| {
                    let at = key.rsplit_once('/').map_or("", |(d, _)| d);
                    (alone.references.dirs.iter()).any(|path| group::may_import(at, path, dir))
                })
                .map(|(key, _)| key.clone())
                .collect(),
            Changed::Script(path) => self
                .alone
                .keys()
                .filter(|key| rows_path(key) == path)
                .cloned()
                .collect(),
        };
        let any = !again.is_empty();
        self.stale.extend(again);
        any
    }

    /// Forgets that the pass wrote the script at `path`: the project was
    /// given one of its own there, or lost it.
    pub fn disown(&mut self, path: &str) {
        for wrote in self.generated.values_mut() {
            wrote.retain(|p| p != path);
        }
        self.generated.retain(|_, wrote| !wrote.is_empty());
    }

    /// Lowers alone each file put or marked since, from one parse of it
    /// that also gives its references and its root.
    pub fn lower(&mut self, project: &dyn Project) {
        let stale: Vec<String> = std::mem::take(&mut self.stale)
            .into_iter()
            .filter(|key| self.files.contains_key(key))
            .collect();
        let fresh: Vec<(String, Alone)> = stale
            .into_iter()
            .map(|key| {
                let alone = self.alone_of(&key, project);
                (key, alone)
            })
            .collect();
        self.alone.extend(fresh);
    }

    fn alone_of(&self, key: &str, project: &dyn Project) -> Alone {
        let file = &self.files[key];
        let allocator = Allocator::default();
        let parsed = qml_syntax::parse(&allocator, &file.text);
        self.roots
            .borrow_mut()
            .entry(key.to_string())
            .or_insert_with(|| crate::root_in(&parsed));
        // The file's name names the module of its constant tables' rows.
        let stem = key.rsplit_once('/').map_or(key, |(_, stem)| stem);
        let locals = project.locals(key, self);
        let mut outcome = crate::lower_parsed(&file.text, &parsed, Some(stem), &*locals);
        drop(locals);
        // A script at the rows module's path that the pass did not write,
        // and that is not the module it would write, is the project's, and
        // does not declare the rows the lowered file imports: the file is
        // kept.
        if let Outcome::Lowered(lowered) = &outcome
            && !lowered.modules.is_empty()
            && let Some(there) = self.not_owned(project, key, &lowered.modules)
        {
            outcome = Outcome::Kept(Kept {
                cause: Cause::RowsScript { path: there },
                at: 0,
            });
        }
        if !file.named && matches!(outcome, Outcome::Lowered(_)) {
            outcome = Outcome::Kept(Kept {
                cause: Cause::NotNamed,
                at: 0,
            });
        }
        let references = references_in(&parsed);
        let uses = references
            .modules
            .iter()
            .flat_map(|uri| project.module_files(key, uri))
            .collect();
        Alone {
            outcome,
            references,
            uses,
        }
    }

    /// The path, as the project names it, of a script of the project's at
    /// the file `key`'s rows module, when the pass may not give the file
    /// that module, one of `modules` (their names beside the file, and their
    /// text). It may when no script is there, when the one there is the
    /// pass's, or when the one there is the module to the byte. That last
    /// is what `ljos migrate` wrote before: the file lowered again imports
    /// it as it is, so a file put back to its Qt text beside its module is
    /// lowered again.
    fn not_owned(
        &self,
        project: &dyn Project,
        key: &str,
        modules: &[(String, String)],
    ) -> Option<String> {
        let path = rows_path(key);
        let there = project.script(key, &path)?;
        let name = path
            .rsplit_once('/')
            .map_or(path.as_str(), |(_, name)| name);
        let owned = self
            .generated
            .get(key)
            .is_some_and(|wrote| wrote.contains(&path))
            || modules
                .iter()
                .any(|(module, text)| module == name && text == there.text);
        (!owned).then_some(there.path)
    }

    /// Lowers what is stale, then every file with its group: what each file
    /// is lowered to is [`lowering`](Self::lowering). What the project is
    /// to do with the rows modules the lowered files import is returned.
    /// The pass writes a module where the project has no script, or where
    /// the one there is the pass's. It drops the ones it wrote before and
    /// does not now. A script of that path the project has already is the
    /// project's, and stays. The edits are to be applied in the order
    /// given: each is decided against the scripts as the edits before it
    /// leave the project, the drops first, and a module of one path that
    /// two files write is the one settled first's.
    pub fn settle(&mut self, project: &dyn Project) -> Vec<ScriptEdit> {
        self.lower(project);
        let mut files = group(&self.alone);
        // A file that uses one is lowered knowing whether that one is kept
        // ([`Root::kept`]). Lowered with what the last settle said, it is
        // lowered again where that changed, until nothing does. A round
        // after the first goes on only when a file lowered again turned,
        // kept or lowered, and a file that turns twice is held kept as it
        // last was: lowered against a file it uses it was kept, which kept
        // that file, against which it was lowered, which let that file be
        // lowered again. So there are at most twice as many rounds as files.
        let mut turns: BTreeMap<String, Turns> = BTreeMap::new();
        let mut settled = false;
        for _ in 0..=2 * self.files.len() {
            let turned: Vec<&String> = files
                .iter()
                .filter(|(key, outcome)| is_kept(Some(outcome)) != is_kept(self.lowered.get(*key)))
                .map(|(key, _)| key)
                .collect();
            if turned.is_empty() {
                settled = true;
                break;
            }
            let users = self.users_of(&turned);
            for (key, outcome) in &files {
                self.lowered.insert(key.clone(), outcome.clone());
            }
            if users.is_empty() {
                settled = true;
                break;
            }
            let was: Vec<(String, Option<Outcome>)> = users
                .iter()
                .map(|key| {
                    let kept = self
                        .alone
                        .get(key)
                        .map(|alone| &alone.outcome)
                        .filter(|outcome| is_kept(Some(outcome)))
                        .cloned();
                    (key.clone(), kept)
                })
                .collect();
            self.stale.extend(users);
            self.lower(project);
            for (key, was) in was {
                if let Some(alone) = self.alone.get_mut(&key) {
                    turns.entry(key).or_default().see(was, &mut alone.outcome);
                }
            }
            files = group(&self.alone);
        }
        debug_assert!(settled, "a settle that did not settle");
        let files: Vec<(String, Outcome)> = files.into_iter().collect();
        let scripts = self.settle_modules(project, &files);
        self.lowered.extend(files);
        scripts
    }

    /// The files that use any of `keys`, as [`group`] reads it, or use a
    /// file that does: one built on a file built on a kept one reads the
    /// kept one's names through it, whether or not that file turned.
    fn users_of(&self, keys: &[&String]) -> BTreeSet<String> {
        let uses = group::uses(&self.alone);
        let all: Vec<&String> = self.alone.keys().collect();
        let mut found = BTreeSet::new();
        let mut next: Vec<usize> = (0..all.len())
            .filter(|&at| keys.contains(&all[at]))
            .collect();
        while let Some(at) = next.pop() {
            for (user, used) in uses.iter().enumerate() {
                if used.contains(&at) && found.insert(all[user].clone()) {
                    next.push(user);
                }
            }
        }
        found
    }

    /// The edits to the rows modules of `files`, lowered as given, in order.
    /// The drops come first, so a module one file stops writing is there
    /// for any file to write, whichever is settled first; one that a file
    /// that wrote it still writes is not dropped. Then each file's module
    /// is written where the edits before it leave no script, or one it
    /// wrote and no file settled before it has taken.
    fn settle_modules(
        &mut self,
        project: &dyn Project,
        files: &[(String, Outcome)],
    ) -> Vec<ScriptEdit> {
        let files: Vec<Settling<'_>> = files
            .iter()
            .map(|(key, outcome)| Settling {
                key,
                modules: modules_beside(key, outcome),
                before: self.generated.remove(key).unwrap_or_default(),
            })
            .collect();
        let writes = |key: &str, path: &str| {
            files.iter().any(|other| {
                other.key != key
                    && other.before.iter().any(|p| p == path)
                    && other.modules.iter().any(|(p, _)| p == path)
            })
        };
        let mut scripts = Vec::new();
        for file in &files {
            for path in &file.before {
                let drop = ScriptEdit::Drop(path.clone());
                if !file.modules.iter().any(|(p, _)| p == path)
                    && !writes(file.key, path)
                    && !scripts.contains(&drop)
                {
                    scripts.push(drop);
                }
            }
        }
        let mut taken = BTreeSet::new();
        for Settling {
            key,
            modules,
            before,
        } in files
        {
            let mut wrote = Vec::new();
            for (path, text) in modules {
                let there = script_after(project, &scripts, key, &path);
                let mine = before.contains(&path) && !taken.contains(&path);
                if mine || there.is_none() {
                    if there != Some(text.as_str()) {
                        scripts.push(ScriptEdit::Put {
                            path: path.clone(),
                            text,
                        });
                    }
                    taken.insert(path.clone());
                    wrote.push(path);
                }
            }
            if !wrote.is_empty() {
                self.generated.insert(key.to_string(), wrote);
            }
        }
        scripts
    }

    /// What the file `key` is lowered to with its group, as the last
    /// [`settle`](Self::settle) has it.
    pub fn lowering(&self, key: &str) -> Option<&Outcome> {
        self.lowered.get(key)
    }

    /// The file `key` as it was put.
    pub fn written(&self, key: &str) -> Option<&str> {
        self.files.get(key).map(|file| file.text.as_str())
    }

    /// The file `key` as a program compiles it: lowered, or as it was put.
    pub fn compiled(&self, key: &str) -> Option<&str> {
        match self.lowered.get(key) {
            Some(Outcome::Lowered(lowered)) => Some(&lowered.text),
            _ => self.written(key),
        }
    }

    /// The files held, in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// Each file lowered alone, with each file its imports bring.
    pub fn uses(&self) -> impl Iterator<Item = (&str, &str)> {
        self.alone.iter().flat_map(|(key, alone)| {
            alone
                .uses
                .iter()
                .map(move |used| (key.as_str(), used.as_str()))
        })
    }

    /// Lets go of every file: each file as it was put, and the scripts the
    /// pass wrote beside them.
    pub fn into_written(self) -> (Vec<(String, String)>, Vec<String>) {
        let written = self
            .files
            .into_iter()
            .map(|(key, file)| (key, file.text))
            .collect();
        let generated = self.generated.into_values().flatten().collect();
        (written, generated)
    }
}

impl Held for Lowering {
    fn text(&self, key: &str) -> Option<&str> {
        self.written(key)
    }

    fn root(&self, key: &str) -> Option<Option<Root>> {
        let file = self.files.get(key)?;
        // A file not lowered yet is parsed for its root alone, once.
        let mut roots = self.roots.borrow_mut();
        let root = roots
            .entry(key.to_string())
            .or_insert_with(|| crate::root(&file.text));
        // A file is kept as the last settle has it; one not settled yet is
        // taken to be lowered, and its users are lowered again if it is not.
        let kept = is_kept(self.lowered.get(key));
        Some(root.clone().map(|root| Root { kept, ..root }))
    }
}

/// The text of the script at `path` once `edits` are applied to the
/// project's scripts.
fn script_after<'a>(
    project: &'a dyn Project,
    edits: &'a [ScriptEdit],
    key: &str,
    path: &str,
) -> Option<&'a str> {
    let last = edits.iter().rev().find(|edit| match edit {
        ScriptEdit::Put { path: at, .. } | ScriptEdit::Drop(at) => at == path,
    });
    match last {
        Some(ScriptEdit::Put { text, .. }) => Some(text),
        Some(ScriptEdit::Drop(_)) => None,
        None => project.script(key, path).map(|script| script.text),
    }
}

/// A file in [`Lowering::settle_modules`]: its rows modules now, at their
/// paths, and the ones the pass wrote for it before.
struct Settling<'a> {
    key: &'a str,
    modules: Vec<(String, String)>,
    before: Vec<String>,
}

/// The rows modules the file `key` is lowered to, at their paths.
fn modules_beside(key: &str, outcome: &Outcome) -> Vec<(String, String)> {
    let dir = key.rsplit_once('/').map(|(dir, _)| dir);
    match outcome {
        Outcome::Lowered(lowered) => lowered
            .modules
            .iter()
            .map(|(name, text)| {
                let path = dir.map_or_else(|| name.clone(), |dir| format!("{dir}/{name}"));
                (path, text.clone())
            })
            .collect(),
        Outcome::Unchanged | Outcome::Kept(_) => Vec::new(),
    }
}

/// Whether a file lowered to `outcome` stays in Qt's QML.
/// How often a file lowered again in one [`Lowering::settle`] turned, kept
/// or lowered, and the last of its outcomes that was kept.
#[derive(Default)]
struct Turns {
    count: u8,
    kept: Option<Outcome>,
}

impl Turns {
    /// Takes the file's outcome lowered again, `now`, after it was `was`
    /// (its outcome when that was kept). A file that turned twice is held
    /// kept: `now` becomes the last outcome that was.
    fn see(&mut self, was: Option<Outcome>, now: &mut Outcome) {
        let turned = is_kept(Some(now)) != was.is_some();
        if was.is_some() {
            self.kept = was;
        }
        if turned && self.count < 2 {
            self.count += 1;
            if is_kept(Some(now)) {
                self.kept = Some(now.clone());
            }
        }
        if self.count >= 2
            && let Some(kept) = &self.kept
        {
            *now = kept.clone();
        }
    }
}

fn is_kept(outcome: Option<&Outcome>) -> bool {
    matches!(outcome, Some(Outcome::Kept(_)))
}

/// The path of the module of the file `key`'s rows, keyed as the files are.
pub fn rows_path(key: &str) -> String {
    match key.rsplit_once('/') {
        Some((dir, stem)) => format!("{dir}/{}", crate::rows_module(stem)),
        None => crate::rows_module(key),
    }
}

#[cfg(test)]
mod tests;
