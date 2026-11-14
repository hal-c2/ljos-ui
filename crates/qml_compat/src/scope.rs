//! What a name means where a file names it: its imports, in the order
//! written, the files of the project and the program's own types they
//! bring, and its ids and inline components.
//!
//! A type name a file writes is looked up here and nowhere else, so the
//! rules ([`crate::rules`]) ask the same question and get the same answer:
//! [`Scope::ty`] says which type a name is, and why the file is kept when
//! the lowered file would find another. A name a script reads is the
//! script's own before it is anything here: the script visitor asks that,
//! and then the qualifiers, types and ids here, in one place
//! (`Script::meaning`), for a rename and a member's type alike.

use std::{cell::RefCell, collections::HashMap};

use oxc_span::Span;
use qml_syntax::ast::{QmlImport, QmlImportSource};
use qt_types::Type;

use crate::modules::{self, Ljos, Modules, Qt};
use crate::{
    CONTROLS_STYLE, Cause, Hider, Imported, Kept, Locals, MODELS, Native, Natives, Owner,
    ProjectModule, Root, Ty, group, not_in, renames_of, renames_on, rules,
};

/// An import of the file, in the order written, that decides what a type
/// name is.
#[derive(Clone, Debug)]
enum Import {
    /// A module of Qt's the pass lowers ([`modules::ROWS`]).
    Qt(Qt),
    /// A module of the program's own types ([`Natives`]).
    Native(String),
    Module(String),
    Dir(String),
}

impl Import {
    fn imported(&self) -> Option<Imported<'_>> {
        match self {
            Import::Qt(_) | Import::Native(_) => None,
            Import::Module(uri) => Some(Imported::Module(uri)),
            Import::Dir(path) => Some(Imported::Dir(path)),
        }
    }
}

pub(crate) struct Scope<'s> {
    /// The modules the file imports, Qt's and the Ljós modules they lower
    /// to, and which line of [`imports`](Self::imports) each Ljós module
    /// takes.
    pub(crate) modules: Modules,
    pub(crate) locals: &'s dyn Locals,
    /// The imports that name types, in the order written: the last that
    /// has a name is the one it is.
    imports: Vec<Import>,
    /// What [`imported_root`](Self::imported_root) found, by name.
    imported: RefCell<HashMap<String, Option<(usize, Root)>>>,
    pub(crate) natives: Option<&'s dyn Natives>,
    /// The modules of the program's own types the file imports.
    native_modules: Vec<String>,
    /// The first import of a Ljós module the pass writes, and where.
    writes: Option<(String, usize)>,
    pub(crate) ids: Vec<(String, Ty)>,
    pub(crate) inline: Vec<(String, Root)>,
    /// The names the file imports under (`import "box.js" as Box`), which
    /// Qt and Ljós find before a type of an import.
    pub(crate) qualifiers: Vec<String>,
    /// What the roots of the files beside it that the file's types are
    /// built on declare, as far as [`ty`](Self::ty) has followed them.
    beside: RefCell<Vec<String>>,
}

fn no(cause: impl Into<Cause>, at: usize) -> Kept {
    Kept {
        cause: cause.into(),
        at,
    }
}

impl<'s> Scope<'s> {
    pub(crate) fn new(modules: Modules, locals: &'s dyn Locals) -> Scope<'s> {
        Scope {
            modules,
            locals,
            imports: Vec::new(),
            imported: RefCell::default(),
            // Where the program's types are not described, Ljós's own
            // modules still are: they are in the table the pass reads.
            natives: Some(locals.natives().unwrap_or(&crate::own::Own)),
            native_modules: Vec::new(),
            writes: None,
            ids: Vec::new(),
            inline: Vec::new(),
            qualifiers: Vec::new(),
            beside: RefCell::default(),
        }
    }

    /// Reads the file's imports, and gives those of Qt's modules the pass
    /// lowers, in the order written. What a name means is known from then
    /// on, but for the ids and inline components the pass collects.
    pub(crate) fn read(
        &mut self,
        imports: &[QmlImport<'_>],
    ) -> Result<Vec<(&'static modules::Row, Span)>, Kept> {
        // The imports of Qt's modules the pass lowers, in the order written.
        let mut qt_imports: Vec<(&'static modules::Row, Span)> = Vec::new();
        self.qualifiers = imports
            .iter()
            .filter_map(|import| import.alias.as_ref().map(|alias| alias.name.to_string()))
            .collect();
        for import in imports {
            let at = import.span.start as usize;
            let name = match &import.source {
                QmlImportSource::Module(name) => name,
                // A directory's types are files of the project; one
                // imported under a name is only ever named through it.
                QmlImportSource::Path(path) => {
                    if import.alias.is_none() && !group::is_script(path) {
                        self.imports.push(Import::Dir((*path).to_string()));
                    }
                    continue;
                }
            };
            let uri = name.to_string();
            let row = modules::row(&uri).filter(|row| {
                import.alias.is_none() || row.under_name == modules::UnderName::Keeps
            });
            if let Some(row) = row {
                if import.alias.is_some() {
                    return Err(no(Cause::ImportedUnderName { uri }, at));
                }
                if row.styled && self.locals.controls_style() != Some(CONTROLS_STYLE) {
                    let style = self.locals.controls_style().map(str::to_owned);
                    return Err(no(Cause::ControlsStyle { style }, at));
                }
                if let Err(cause) = self.modules.read(row) {
                    return Err(no(cause, at));
                }
                qt_imports.push((row, import.span));
                if row.names_types {
                    if row.takes_the_line {
                        self.modules.imported(row.lowers_to, self.imports.len());
                    }
                    self.imports.push(Import::Qt(row.qt));
                }
                continue;
            }
            match uri.as_str() {
                // The program's own types are what it registers, in Qt's
                // API and in Ljós's: the import stays as it is written.
                other if self.natives.is_some_and(|n| n.has_module(other)) => {
                    if self.writes.is_none() && modules::written(other) {
                        self.writes = Some((other.to_owned(), at));
                    }
                    if import.alias.is_none() {
                        self.native_modules.push(other.to_owned());
                        self.imports.push(Import::Native(other.to_owned()));
                    }
                }
                other => match self.locals.module(other) {
                    // Files of the project, lowered with this one
                    // ([`group`]); the line stays as it is.
                    Some(ProjectModule::Files) if import.alias.is_none() => {
                        // Its types are in sight as a directory's are, and
                        // which it has may change after the file is lowered.
                        self.imports.push(Import::Module(uri));
                    }
                    Some(ProjectModule::Files) => {
                        return Err(no(
                            Cause::ImportedUnderName {
                                uri: other.to_owned(),
                            },
                            at,
                        ));
                    }
                    Some(ProjectModule::Native) => {
                        return Err(no(
                            Cause::NativeModule {
                                uri: other.to_owned(),
                            },
                            at,
                        ));
                    }
                    // A module that builds on QtQuick names its types, and
                    // those are Qt's until Ljós has its own
                    // (docs/DIALECT.md).
                    None => {
                        return Err(no(
                            Cause::ModuleNotLowered {
                                uri: other.to_owned(),
                            },
                            at,
                        ));
                    }
                },
            }
        }
        self.modules.share_lines();
        Ok(qt_imports)
    }

    /// Keeps a file that imports, beside the Qt modules the pass lowers,
    /// a Ljós module the pass writes for them: Ljós's types of Qt's names
    /// would stand for Qt's, whose members are then not renamed, and the
    /// lowered file would import the module twice.
    pub(crate) fn beside_what_it_lowers_to(&self) -> Result<(), Kept> {
        match &self.writes {
            Some((uri, at)) => Err(no(Cause::ImportsWhatItLowersTo { uri: uri.clone() }, *at)),
            None => Ok(()),
        }
    }

    /// What may take `name`, which lowering a member of `Qt` would bring
    /// into the file, beside it: a type of a directory or a module of the
    /// project it imports not under a name, a file beside it, or what the
    /// root of one its types are built on declares.
    pub(crate) fn may_be_beside(&self, name: &str) -> Option<Hider> {
        // Only a capitalised name is a type. A directory or module whose
        // types the project knows takes the name only if it has it; one
        // whose types it does not know may.
        let imported = name.starts_with(|c: char| c.is_ascii_uppercase())
            && self.imports.iter().any(|import| {
                import
                    .imported()
                    .is_some_and(|from| self.locals.imports_type(from, name) != Some(false))
            });
        if imported {
            Some(Hider::Directory)
        } else if self.locals.root(name).is_some() || self.beside.borrow().iter().any(|d| d == name)
        {
            Some(Hider::Beside)
        } else {
            None
        }
    }

    /// Whether `name` is QtQuick.Layouts' type here, not the type of an
    /// import after it that has the name too (see [`Self::ty`]).
    /// Such an import's type, built on one the pass cannot lower yet,
    /// keeps the file.
    pub(crate) fn of_layouts(&self, name: &str, at: usize) -> Result<bool, Kept> {
        if self.modules.qt_type(&[Qt::Layouts], name).is_none() {
            return Ok(false);
        }
        match self.ty(name, at, 0) {
            Ok(Ty::Exact(t)) => Ok(t.module == "QtQuick.Layouts"),
            Ok(_) => Ok(false),
            Err(kept)
                if self.shadows(self.imported_root(name).map(|(at, _)| at), &[Qt::Layouts]) =>
            {
                Err(kept)
            }
            // Qt's, of a type Ljos.Layout has not yet.
            Err(_) => Ok(true),
        }
    }

    /// Which of [`imports`](Self::imports) is the last of one of `qts`.
    fn last(&self, qts: &[Qt]) -> Option<usize> {
        self.imports
            .iter()
            .rposition(|import| matches!(import, Import::Qt(qt) if qts.contains(qt)))
    }

    /// Whether an import at `own`, of the project's files or the program's
    /// own types, comes after the last of `qts`, so that a name both have
    /// is its: the loader takes the last.
    fn shadows(&self, own: Option<usize>, qts: &[Qt]) -> bool {
        matches!((own, self.last(qts)), (Some(own), Some(qt)) if own > qt)
    }

    /// Keeps a file that names a type of QtQuick.Layouts whose attached
    /// object Ljos.Layout has not: only `Layout`'s is there.
    pub(crate) fn attached_layout(&self, name: &str, at: usize) -> Result<(), Kept> {
        if name != "Layout" && self.of_layouts(name, at)? {
            return Err(no(
                Cause::AttachedNotInLayout {
                    name: name.to_owned(),
                },
                at,
            ));
        }
        Ok(())
    }

    pub(crate) fn id(&self, name: &str) -> Option<Ty> {
        self.ids
            .iter()
            .find(|(id, _)| id == name)
            .map(|(_, ty)| *ty)
    }

    /// The type `name` is in this file: Qt's (and then one Ljos.Ui has),
    /// an inline component, or a file of the project.
    pub(crate) fn ty(&self, name: &str, at: usize, depth: usize) -> Result<Ty, Kept> {
        // Of an import after Qt's that has the name too, the project's
        // files' or the program's own types', the type is that import's, as
        // the loader takes the last.
        let imported = self.imported_root(name);
        let native = self.native_type(name);
        let project = imported
            .as_ref()
            .map(|(at, _)| *at)
            .max(native.map(|(at, _)| at));
        let shadowed_native =
            matches!((&imported, native), (Some((at, _)), Some((n, _))) if *at > n);
        for group in modules::LOOKED_UP {
            let Some((qt, ty, qt_line)) = self.qt_found(group, name) else {
                continue;
            };
            let (ljos, has) = self.lowered(qt, name);
            if self.project_first(
                name,
                project,
                qt_line,
                (ljos.uri(), self.modules.line(ljos)),
                at,
            )? {
                continue;
            }
            if !has {
                return Err(no(not_in(name, ljos.uri()), at));
            }
            // `Qt` is a global of every file, as it is in Qt's QML: naming
            // it imports nothing.
            if name != "Qt" {
                self.modules.names(ljos);
            }
            return Ok(Ty::Exact(ty));
        }
        if let Some((native_at, ty)) = native.filter(|_| !shadowed_native) {
            // A Ljós module whose line comes after the program's import
            // is found first in the lowered file.
            if let Some((module, line)) = self.ljos_module_with(name)
                && !matches!(line, Some(line) if native_at > line)
            {
                return Err(no(
                    Cause::FoundFirst {
                        name: name.to_owned(),
                        is: Some(Owner::Program),
                        module,
                    },
                    at,
                ));
            }
            return Ok(Ty::Native(ty, true));
        }
        if depth > 16 {
            return Ok(Ty::Unknown);
        }
        let inline = self.inline.iter().any(|(n, _)| n == name);
        // Past the first step the name is another file's, found among its
        // own imports, which this file's do not say. An import of the
        // project after the line the Ljós module takes is found before it.
        if depth == 0
            && !inline
            && let Some((module, line)) = self.ljos_module_with(name)
            && !matches!((&imported, line), (Some((at, _)), Some(line)) if *at > line)
        {
            let is =
                (imported.is_some() || self.locals.root(name).is_some()).then_some(Owner::Project);
            return Err(no(
                Cause::FoundFirst {
                    name: name.to_owned(),
                    is,
                    module,
                },
                at,
            ));
        }
        let root = self.local_root(name, imported);
        if let Some(root) = &root {
            self.beside
                .borrow_mut()
                .extend(root.declares.iter().cloned());
            if root.kept {
                return Ok(Ty::Kept);
            }
        }
        match root.map(|root| root.type_name) {
            Some(root) if root != name => match self.ty(&root, at, depth + 1)? {
                // What it declares is not in the table: its members are
                // known by the type it is built on, as far as that goes.
                Ty::Exact(ty) => Ok(Ty::Derived(ty)),
                Ty::Native(ty, _) => Ok(Ty::Native(ty, false)),
                other => Ok(other),
            },
            Some(_) => Ok(Ty::Unknown),
            // The file names the program's types, and this is not one the
            // program describes: what it is in the file is not known.
            None if !self.native_modules.is_empty() => Err(no(
                Cause::NotBesideNorNative {
                    name: name.to_owned(),
                    modules: self.native_modules.clone(),
                },
                at,
            )),
            None => Ok(Ty::Unknown),
        }
    }

    /// The program's own type `name`, of the last module the file imports
    /// that has it, with where that import is among [`imports`](Self::imports).
    pub(crate) fn native_type(&self, name: &str) -> Option<(usize, Native)> {
        let natives = self.natives?;
        self.imports
            .iter()
            .enumerate()
            .rev()
            .find_map(|(at, import)| match import {
                Import::Native(uri) => natives.module_type(uri, name).map(|ty| (at, ty)),
                _ => None,
            })
    }

    /// What Ljós calls the type `name`, where that is Qt's and Ljós calls
    /// it otherwise: a type of the same name imported after QtQuick is
    /// not Qt's, and keeps its name.
    pub(crate) fn renamed_type(&self, name: &str) -> Option<&'static str> {
        let new = rules::new_name(name)?;
        let own_at = self
            .imported_root(name)
            .map(|(at, _)| at)
            .max(self.native_type(name).map(|(at, _)| at));
        (!self.shadows(own_at, &[Qt::Quick])).then_some(new)
    }

    /// The toolkit type of Qt's table the native type `ty` is built on.
    pub(crate) fn toolkit_base(&self, ty: Native) -> Option<&'static Type> {
        let (module, name) = self.natives?.toolkit_base(ty)?;
        qt_types::module(&module)?.type_named(&name)
    }

    /// The renames of the type of `on`, or of the toolkit type it is
    /// built on.
    pub(crate) fn renames_on(&self, on: Ty) -> Option<&'static [(&'static str, &'static str)]> {
        match on {
            Ty::Native(ty, _) => renames_of(self.toolkit_base(ty)?),
            other => renames_on(other),
        }
    }

    /// The root of the type `name` a file of the project is: an inline
    /// component, a type an import of the project brings, or a file beside
    /// this one, in the order the loader looks (`imported` is what
    /// [`imported_root`](Self::imported_root) found).
    pub(crate) fn local_root(&self, name: &str, imported: Option<(usize, Root)>) -> Option<Root> {
        self.inline
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, root)| root.clone())
            .or(imported.map(|(_, root)| root))
            .or_else(|| self.locals.root(name))
    }

    /// The last import of the project that has the type `name`, and what
    /// its root is.
    pub(crate) fn imported_root(&self, name: &str) -> Option<(usize, Root)> {
        if let Some(known) = self.imported.borrow().get(name) {
            return known.clone();
        }
        // Each is a file the project reads; a file names a type many times.
        let found = self
            .imports
            .iter()
            .enumerate()
            .rev()
            .find_map(|(at, import)| {
                let from = import.imported()?;
                self.locals.imported(from, name).map(|root| (at, root))
            });
        self.imported
            .borrow_mut()
            .insert(name.to_string(), found.clone());
        found
    }

    /// The Qt type `name` is of one of the modules of `group`
    /// ([`modules::LOOKED_UP`]), with which module's type it is and the
    /// last import that brings it: of the imports of the group, the last
    /// that has it.
    fn qt_found(&self, group: &[Qt], name: &str) -> Option<(Qt, &'static Type, Option<usize>)> {
        if group == [Qt::Quick] {
            // `QtQuick`'s types are known whether it is imported or not.
            // One of `QtQml`'s is shadowed by what is imported after the
            // last of `QtQuick` and `QtQml`.
            let ty = self.modules.quick().type_named(name)?;
            let qt = if self.modules.qml().has(ty) {
                Qt::Qml
            } else {
                Qt::Quick
            };
            return Some((qt, ty, self.last(qt.brought_by())));
        }
        self.imports
            .iter()
            .enumerate()
            .rev()
            .find_map(|(line, import)| match import {
                Import::Qt(qt) if group.contains(qt) => {
                    let ty = self.modules.qt(*qt)?.type_named(name)?;
                    Some((*qt, ty, Some(line)))
                }
                _ => None,
            })
    }

    /// The Ljós module the type `name` of `qt`'s lowers to, and whether
    /// that module has it.
    fn lowered(&self, qt: Qt, name: &str) -> (Ljos, bool) {
        let has = |ljos: Ljos, name: &str| {
            self.modules
                .ljos(ljos)
                .is_some_and(|m| m.type_named(name).is_some())
        };
        match qt {
            Qt::Layouts => (Ljos::Layout, rules::layout::LOWERED.contains(&name)),
            // The models are [`MODELS_MODULE`]'s.
            Qt::Qml if MODELS.contains(&name) => (Ljos::Models, true),
            Qt::Qml => (
                Ljos::Core,
                name == "Qt" || has(Ljos::Core, rules::new_name(name).unwrap_or(name)),
            ),
            // What arranges children is `Ljos.Layout`'s.
            Qt::Quick => {
                let new = rules::new_name(name).unwrap_or(name);
                if !has(Ljos::Ui, new) && has(Ljos::Layout, new) {
                    (Ljos::Layout, true)
                } else {
                    (Ljos::Ui, has(Ljos::Ui, new))
                }
            }
            other => {
                let ljos = other.lowers_to();
                (ljos, has(ljos, name))
            }
        }
    }

    /// The Ljós module the lowered file imports that has `name`, for a name
    /// no Qt module the file imports has, and the place among the file's
    /// imports of the line it takes. A Qt file finds such a name among the
    /// files beside it, but an import is found before those, so the lowered
    /// file would make Ljós's type instead.
    pub(crate) fn ljos_module_with(&self, name: &str) -> Option<(&'static str, Option<usize>)> {
        let ljos = self.modules.ljos_with(name)?;
        Some((ljos.uri(), self.modules.line(ljos)))
    }

    /// Whether the type `name` is the project's, imported as the
    /// `project`th import, rather than the Qt module's whose last import of
    /// it is the `qt`th; the lowered file finds `module`'s instead, on the
    /// `line`th. An import of the project between the two lines would be
    /// one type in Qt's file and another in the lowered one.
    fn project_first(
        &self,
        name: &str,
        project: Option<usize>,
        qt: Option<usize>,
        (module, line): (&'static str, Option<usize>),
        at: usize,
    ) -> Result<bool, Kept> {
        let after = |line: Option<usize>| matches!((project, line), (Some(p), Some(l)) if p > l);
        // The import is of the program's own types or of the project's
        // files.
        let native = matches!(
            project.and_then(|p| self.imports.get(p)),
            Some(Import::Native(_))
        );
        let owner = if native {
            Owner::Program
        } else {
            Owner::Project
        };
        match (after(qt), after(line)) {
            (true, false) => Err(no(
                Cause::FoundFirst {
                    name: name.to_owned(),
                    is: Some(owner),
                    module,
                },
                at,
            )),
            (false, true) => Err(no(
                Cause::ShadowedInLowered {
                    name: name.to_owned(),
                    by: owner,
                    module,
                },
                at,
            )),
            (in_qt, _) => Ok(in_qt),
        }
    }
}

#[cfg(test)]
mod tests;
