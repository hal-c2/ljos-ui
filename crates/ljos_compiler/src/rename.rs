//! A module given a new name in its directory, and the files that import
//! it saying the new one: what `ts-lts-syntax`'s fix does to a `.ts`
//! module that uses `match` (docs/DIALECT.md §4.4), and `ljos migrate`
//! with it.
//!
//! A file names a module in one of four ways, each read as what it is
//! rather than searched for as text, so a comment or string that happens
//! to hold the name is left alone: a QML file's `import "size.ts"`, a
//! script's `.import "size.ts" as Size`, a module's `import … from
//! "./size.ts"` (or `export … from`, `export * from`), and a `qmldir`'s `Size 1.0 size.ts`.

use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::Parser;
use oxc_span::{SourceType, Span};
use qml_front::project::{directory, join};
use qml_syntax::ast::QmlImportSource;

use crate::script::module;

/// The name the module at `key`, a `.ts` one that uses Ljós syntax, is
/// renamed to: `size.ts` → `size.lts`.
pub fn lts_key(key: &str) -> String {
    module::lts_name(key)
}

/// The first `match` or `is` of `text`, a `.ts` module's: the syntax of an
/// `.lts` one, so the module is renamed. `None` where it has none, or does
/// not read even with it.
pub fn lts_syntax(text: &str) -> Option<Span> {
    let desugared = qml_syntax::desugar(text);
    if !desugared.diagnostics.is_empty() {
        return None;
    }
    desugared.uses.first().copied()
}

/// A `.ts` module's rename to `.lts`, with the files that import it, as
/// `ljos migrate`, the authoring tools and the language server make it.
#[derive(Debug)]
pub struct LtsRename {
    pub module: PathBuf,
    /// The module's new path, beside it.
    pub target: PathBuf,
    /// The module's key below the directory its importers were looked for
    /// in, and its new one.
    pub from: String,
    pub to: String,
    /// In the order of their keys.
    pub importers: Vec<Importer>,
}

/// A file that imports the module being renamed.
#[derive(Debug)]
pub struct Importer {
    /// Its key below the directory importers were looked for in, and its
    /// path, once the module is renamed: a module that imports itself is
    /// its own importer, at its new name.
    pub key: String,
    pub path: PathBuf,
    /// Its text now, and with the new name.
    pub text: String,
    pub renamed: String,
}

/// What renaming a `.ts` module to `.lts` would do.
#[derive(Debug)]
pub enum LtsPlan {
    /// The module has no `match`, `is` or `|>`: it stays `.ts`.
    NoSyntax,
    /// The module is not below the directory importers are looked for in.
    Outside,
    /// A file already has the new name, `to`.
    InTheWay {
        to: String,
    },
    Rename(LtsRename),
}

/// Something below the directory importers are looked for in that cannot
/// be read: a directory, or a file that may import the module. Whether it
/// does is not known, so there is no plan, rather than one that leaves it
/// naming the old path.
#[derive(Debug, thiserror::Error)]
#[error("{}: {error}", path.display())]
pub struct Unreadable {
    pub path: PathBuf,
    #[source]
    pub error: std::io::Error,
}

/// The plan for `module`, whose text is `text`, with its importers looked
/// for below `base`: every QML file, script, module and `qmldir` there
/// whose imports name it.
///
/// # Errors
/// A directory below `base`, or a file there that may import the module,
/// cannot be read (as text).
pub fn plan_lts(base: &Path, module: &Path, text: &str) -> Result<LtsPlan, Unreadable> {
    plan_lts_with(base, module, text, |path| std::fs::read_to_string(path))
}

/// [`plan_lts`], with each importer's text as `read` gives it: an editor
/// gives the text it has of a file it has open, which may not be saved.
///
/// # Errors
/// A directory below `base` cannot be read, or `read` fails for a file
/// there that may import the module.
pub fn plan_lts_with(
    base: &Path,
    module: &Path,
    text: &str,
    read: impl Fn(&Path) -> std::io::Result<String>,
) -> Result<LtsPlan, Unreadable> {
    if lts_syntax(text).is_none() {
        return Ok(LtsPlan::NoSyntax);
    }
    let Some(from) = key_below(base, module) else {
        return Ok(LtsPlan::Outside);
    };
    let to = lts_key(&from);
    let target = module.with_extension("lts");
    if target.exists() {
        return Ok(LtsPlan::InTheWay {
            to: to.rsplit('/').next().unwrap_or(&to).to_owned(),
        });
    }
    let mut below = Vec::new();
    crate::files_below_naming(base, imports_modules, &mut below)
        .map_err(|(path, error)| Unreadable { path, error })?;
    let mut importers = Vec::new();
    for path in below {
        // DECISION: a link is not an importer, as the rename would write
        // through it to wherever it points, perhaps outside the project.
        // The file it names is an importer where it is, when that is below
        // the directory looked in.
        if path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            continue;
        }
        let Some(key) = key_below(base, &path) else {
            continue;
        };
        let text = read(&path).map_err(|error| Unreadable {
            path: path.clone(),
            error,
        })?;
        if let Some(renamed) = imports(&key, &text, &from, &to) {
            let (key, path) = if key == from {
                (to.clone(), target.clone())
            } else {
                (key, path)
            };
            importers.push(Importer {
                key,
                path,
                text,
                renamed,
            });
        }
    }
    importers.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(LtsPlan::Rename(LtsRename {
        module: module.to_owned(),
        target,
        from,
        to,
        importers,
    }))
}

impl LtsRename {
    /// The module's new name, without its directory.
    pub fn to_name(&self) -> &str {
        self.to.rsplit('/').next().unwrap_or(&self.to)
    }

    /// Where a failure to write `importer` leaves it: the module that
    /// imports itself is put back at its old name.
    fn left_at<'a>(&'a self, importer: &'a Importer) -> &'a Path {
        if importer.path == self.target {
            &self.module
        } else {
            &importer.path
        }
    }

    /// Renames the module, then writes its importers in the order of their
    /// keys. When one cannot be written, those written get their text back
    /// and the module its name, so a failure leaves the files as they were.
    /// Each importer's new text is written beside it and put in its place
    /// only when whole, so one that cannot be written keeps its text.
    ///
    /// # Errors
    /// Each file that could not be renamed or written, with why; the first
    /// is what failed, the rest what could not be put back.
    pub fn apply(&self) -> Result<(), Vec<(PathBuf, std::io::Error)>> {
        self.apply_with(|path, text| std::fs::write(path, text))
    }

    /// [`apply`](Self::apply), with the bytes put on disk by `write`, so a
    /// test can have a write fail part way, as on a full disk.
    fn apply_with(
        &self,
        write: impl Fn(&Path, &str) -> std::io::Result<()>,
    ) -> Result<(), Vec<(PathBuf, std::io::Error)>> {
        std::fs::rename(&self.module, &self.target).map_err(|e| vec![(self.module.clone(), e)])?;
        for (at, importer) in self.importers.iter().enumerate() {
            if let Err(e) = replace(&importer.path, &importer.renamed, &write) {
                let mut failed = vec![(self.left_at(importer).to_owned(), e)];
                // The one that failed was not touched.
                for importer in &self.importers[..at] {
                    if let Err(e) = replace(&importer.path, &importer.text, &write) {
                        failed.push((self.left_at(importer).to_owned(), e));
                    }
                }
                if let Err(e) = std::fs::rename(&self.target, &self.module) {
                    failed.push((self.target.clone(), e));
                }
                return Err(failed);
            }
        }
        Ok(())
    }
}

/// Puts `text` in the file at `path` by writing it to a new file beside it,
/// with its permissions, and renaming that over it: the file holds its old
/// text or the new, never part of one, even when the disk fills part way.
///
/// DECISION: the file is replaced, not written in place, so a hard link
/// to it keeps the old text; a file that is not writable is refused, as a
/// write in place would refuse it, rather than replaced.
fn replace(
    path: &Path,
    text: &str,
    write: impl Fn(&Path, &str) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let permissions = std::fs::metadata(path)?.permissions();
    if permissions.readonly() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "the file is read-only",
        ));
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let staged = path.with_file_name(format!(".{name}.{}.ljos-rename", std::process::id()));
    let done = write(&staged, text)
        .and_then(|()| std::fs::File::open(&staged)?.sync_all())
        .and_then(|()| std::fs::set_permissions(&staged, permissions))
        .and_then(|()| std::fs::rename(&staged, path));
    if done.is_err() {
        // A staged file left behind would be found by the next walk.
        let _ = std::fs::remove_file(&staged);
    }
    done
}

/// A file that may import a module: a QML file, a script, a module or a
/// `qmldir`.
fn imports_modules(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "qmldir")
        || path
            .extension()
            .is_some_and(|e| ["qml", "js", "mjs", "ts", "lts"].iter().any(|x| e == *x))
}

/// The key of `file` below `root`, `/`-separated; both are made absolute.
fn key_below(root: &Path, file: &Path) -> Option<String> {
    let root = crate::roots::absolute(root);
    let file = crate::roots::absolute(file);
    let rest = file.strip_prefix(&root).ok()?;
    Some(
        rest.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// `text`, the file at `file` (a key from the project's root, as `from`
/// and `to` are), with each import of the module `from` naming `to`
/// instead. `None` when it imports no such module, or cannot be read.
pub fn imports(file: &str, text: &str, from: &str, to: &str) -> Option<String> {
    let name = |key: &str| key.rsplit('/').next().unwrap_or(key).to_string();
    let (old, new) = (name(from), name(to));
    let named = |path: &str| join(directory(file), path) == from && path.ends_with(&old);
    let mut spans: Vec<Span> = if file == "qmldir" || file.ends_with("/qmldir") {
        qmldir(text, &named)
    } else if file.ends_with(".qml") {
        qml(text, &named)
    } else if file.ends_with(".js") {
        script(text, &named)
    } else if file.ends_with(".mjs") || module::is_module(file) {
        es_module(file, text, &named)
    } else {
        Vec::new()
    };
    if spans.is_empty() {
        return None;
    }
    spans.sort_by_key(|span| span.start);
    let mut out = String::with_capacity(text.len() + spans.len());
    let mut at = 0;
    for span in spans {
        // Each span is the written path, whose last part is the name.
        let path = &text[span.start as usize..span.end as usize];
        out.push_str(&text[at..span.start as usize]);
        out.push_str(&path[..path.len() - old.len()]);
        out.push_str(&new);
        at = span.end as usize;
    }
    out.push_str(&text[at..]);
    Some(out)
}

/// The written path of each `qmldir` line whose file is the module: its
/// last word (`Size 1.0 size.ts`, `singleton Size 1.0 size.ts`).
fn qmldir(text: &str, named: &dyn Fn(&str) -> bool) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() >= 2
            && !line.trim_start().starts_with('#')
            && let Some(last) = words.last()
            && named(last)
            && let Some(at) = line.rfind(last)
        {
            spans.push(span_of(start + at, last));
        }
        start += line.len();
    }
    spans
}

/// The written path of each `import "…"` of a QML file that is the module.
fn qml(text: &str, named: &dyn Fn(&str) -> bool) -> Vec<Span> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, text);
    parsed
        .document
        .imports
        .iter()
        .filter_map(|import| {
            let QmlImportSource::Path(path) = &import.source else {
                return None;
            };
            if !named(path) {
                return None;
            }
            // The import's first quoted text is its path.
            let written = &text[import.span.start as usize..import.span.end as usize];
            let quote = written.find(['"', '\''])?;
            let at = import.span.start as usize + quote + 1;
            text[at..].starts_with(*path).then(|| span_of(at, path))
        })
        .collect()
}

/// The written path of each `.import "…" as Name` line of a script.
fn script(text: &str, named: &dyn Fn(&str) -> bool) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with(".import")
            && let Some(open) = line.find(['"', '\''])
            && let Some(len) = line[open + 1..].find(['"', '\''])
        {
            let path = &line[open + 1..open + 1 + len];
            if named(path) {
                spans.push(span_of(start + open + 1, path));
            }
        }
        start += line.len();
    }
    spans
}

/// The written path of each `import … from`, `export … from` and
/// `export * from` of a module that is the module, read with Ljós's syntax where it has some.
fn es_module(file: &str, text: &str, named: &dyn Fn(&str) -> bool) -> Vec<Span> {
    let allocator = Allocator::default();
    let source_type = if file.ends_with(".mjs") {
        SourceType::mjs()
    } else {
        module::source_type()
    };
    let (parsed, _) = Parser::new(&allocator, text, source_type).parse_with_sugar();
    if parsed.panicked {
        return Vec::new();
    }
    parsed
        .program
        .body
        .iter()
        .filter_map(|statement| match statement {
            Statement::ImportDeclaration(import) => Some(&import.source),
            Statement::ExportFromDeclaration(export) => Some(&export.source),
            Statement::ExportAllDeclaration(export) => Some(&export.source),
            _ => None,
        })
        .filter(|source| named(source.value.as_str()))
        // Inside the quotes.
        .map(|source| Span::new(source.span.start + 1, source.span.end - 1))
        .collect()
}

fn span_of(at: usize, path: &str) -> Span {
    Span::new(at as u32, (at + path.len()) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_qml_file_a_script_a_module_and_a_qmldir_name_the_new_file() {
        let qml = "import Ljos.Ui\nimport \"size.ts\" as Size\nItem {}\n";
        assert_eq!(
            imports("Pad.qml", qml, "size.ts", "size.lts").as_deref(),
            Some("import Ljos.Ui\nimport \"size.lts\" as Size\nItem {}\n")
        );
        let script = ".pragma library\n.import \"../size.ts\" as Size\nfunction f() {}\n";
        assert_eq!(
            imports("js/a.js", script, "size.ts", "size.lts").as_deref(),
            Some(".pragma library\n.import \"../size.lts\" as Size\nfunction f() {}\n")
        );
        let module = "// \"./size.ts\" stays in a comment\nimport { half } from \"./size.ts\"\nexport { half } from './size.ts'\n";
        assert_eq!(
            imports("line.ts", module, "size.ts", "size.lts").as_deref(),
            Some(
                "// \"./size.ts\" stays in a comment\nimport { half } from \"./size.lts\"\nexport { half } from './size.lts'\n"
            )
        );
        assert_eq!(
            imports(
                "qmldir",
                "module Pads\nSize 1.0 size.ts\n",
                "size.ts",
                "size.lts"
            )
            .as_deref(),
            Some("module Pads\nSize 1.0 size.lts\n")
        );
    }

    /// A barrel's `export * from` and `export * as … from` name the
    /// module as an `export { … } from` does.
    #[test]
    fn an_export_of_everything_names_the_new_file() {
        let barrel = "export * from \"./size.ts\"\nexport * as Size from './size.ts'\n";
        assert_eq!(
            imports("index.ts", barrel, "size.ts", "size.lts").as_deref(),
            Some("export * from \"./size.lts\"\nexport * as Size from './size.lts'\n")
        );
    }

    /// A module that imports itself is its own importer: its import is
    /// rewritten in the renamed file, and nothing is left at the old name.
    #[test]
    fn a_module_that_imports_itself_says_its_new_name_in_its_new_file() {
        let project = tempfile::tempdir().unwrap();
        let module = project.path().join("size.ts");
        let text = "import * as Me from \"./size.ts\"\nexport const k: number = match (1) { default: 2 }\n";
        std::fs::write(&module, text).unwrap();
        let LtsPlan::Rename(rename) = plan_lts(project.path(), &module, text).unwrap() else {
            panic!("the module is renamed");
        };
        // What a report of the rename lists: the module at its new name.
        assert_eq!(
            rename
                .importers
                .iter()
                .map(|i| (i.key.as_str(), i.path.as_path()))
                .collect::<Vec<_>>(),
            [("size.lts", rename.target.as_path())]
        );
        rename.apply().unwrap();
        assert!(!module.exists(), "nothing is left at the old name");
        assert_eq!(
            std::fs::read_to_string(project.path().join("size.lts")).unwrap(),
            "import * as Me from \"./size.lts\"\nexport const k: number = match (1) { default: 2 }\n"
        );
    }

    /// When an importer after it cannot be written, a module that imports
    /// itself is back at its old name with its old text.
    #[test]
    fn a_module_that_imports_itself_is_put_back_when_the_rename_fails() {
        let project = tempfile::tempdir().unwrap();
        let module = project.path().join("size.ts");
        let text = "import * as Me from \"./size.ts\"\nexport const k: number = match (1) { default: 2 }\n";
        std::fs::write(&module, text).unwrap();
        let LtsPlan::Rename(mut rename) = plan_lts(project.path(), &module, text).unwrap() else {
            panic!("the module is renamed");
        };
        // An importer whose directory is gone cannot be written.
        rename.importers.push(Importer {
            key: "z/gone.ts".to_owned(),
            path: project.path().join("z/gone.ts"),
            text: String::new(),
            renamed: String::new(),
        });
        let failed = rename.apply().unwrap_err();
        // Only what failed: the module itself was put back.
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].0, project.path().join("z/gone.ts"));
        assert_eq!(std::fs::read_to_string(&module).unwrap(), text);
        assert!(!project.path().join("size.lts").exists());
    }

    /// A file that cannot be read may import the module, so the rename is
    /// refused rather than leave it naming the old path.
    #[test]
    fn a_file_that_cannot_be_read_refuses_the_rename() {
        let project = tempfile::tempdir().unwrap();
        let mut pad = b"import \"size.ts\" as Size\nItem { property string s: \"".to_vec();
        pad.extend([0xff, 0xfe]);
        pad.extend(b"\" }\n");
        std::fs::write(project.path().join("Pad.qml"), pad).unwrap();
        let module = project.path().join("size.ts");
        let text = "export const k: number = match (1) { default: 2 }\n";
        std::fs::write(&module, text).unwrap();
        let plan = plan_lts(project.path(), &module, text);
        let Err(Unreadable { path, error }) = plan else {
            panic!("the rename is refused: {plan:?}");
        };
        assert_eq!(path, project.path().join("Pad.qml"));
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    /// A write that fails part way, as on a full disk, leaves the file it
    /// was replacing as it was, even when putting it back fails too: the
    /// new text is written beside it and put in its place only when whole.
    #[test]
    fn a_write_that_fails_part_way_leaves_the_importer_as_it_was() {
        let project = tempfile::tempdir().unwrap();
        let module = project.path().join("size.ts");
        let text = "export const k: number = match (1) { default: 2 }\n";
        std::fs::write(&module, text).unwrap();
        let qml = "import \"size.ts\" as Size\nItem {}\n";
        for name in ["A.qml", "B.qml"] {
            std::fs::write(project.path().join(name), qml).unwrap();
        }
        let Ok(LtsPlan::Rename(rename)) = plan_lts(project.path(), &module, text) else {
            panic!("a rename");
        };
        // Every write of B's text empties what it writes to, then fails.
        let full = |path: &Path, text: &str| {
            if path.to_string_lossy().contains("B.qml") {
                std::fs::File::create(path)?;
                return Err(std::io::Error::from(std::io::ErrorKind::StorageFull));
            }
            std::fs::write(path, text)
        };
        let failed = rename.apply_with(full).unwrap_err();
        assert_eq!(failed[0].0, project.path().join("B.qml"));
        for name in ["A.qml", "B.qml"] {
            let now = std::fs::read_to_string(project.path().join(name)).unwrap();
            assert_eq!(now, qml, "{name}");
        }
        assert_eq!(std::fs::read_to_string(&module).unwrap(), text);
        let mut left: Vec<_> = std::fs::read_dir(project.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["A.qml", "B.qml", "size.ts"]);
    }

    /// A directory that cannot be listed is named, not the directory the
    /// walk began at: that is the one to fix.
    #[cfg(unix)]
    #[test]
    fn a_directory_that_cannot_be_read_is_named_and_refuses_the_rename() {
        use std::os::unix::fs::PermissionsExt;
        let project = tempfile::tempdir().unwrap();
        let shut = project.path().join("lib/shut");
        std::fs::create_dir_all(&shut).unwrap();
        let module = project.path().join("size.ts");
        let text = "export const k: number = match (1) { default: 2 }\n";
        std::fs::write(&module, text).unwrap();
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
        // One who may read anything (root) reads it anyway.
        let listable = std::fs::read_dir(&shut).is_ok();
        let plan = plan_lts(project.path(), &module, text);
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
        if listable {
            return;
        }
        let Err(Unreadable { path, error }) = plan else {
            panic!("the rename is refused: {plan:?}");
        };
        assert_eq!(path, shut);
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn a_file_that_names_another_module_of_the_name_is_left_alone() {
        let qml = "import \"other/size.ts\" as Size\nItem {}\n";
        assert_eq!(imports("Pad.qml", qml, "size.ts", "size.lts"), None);
        let module = "import { half } from \"./size.ts\"\n";
        assert_eq!(imports("lib/line.ts", module, "size.ts", "size.lts"), None);
    }

    /// A link below the project to a file outside it is not an importer:
    /// the rename writes each importer, and would write through the link.
    #[cfg(unix)]
    #[test]
    fn a_link_to_a_file_outside_the_project_is_not_an_importer() {
        let outside = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let pad = "import Ljos.Ui\nimport \"size.ts\" as Size\nItem {}\n";
        std::fs::write(outside.path().join("Pad.qml"), pad).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("Pad.qml"),
            project.path().join("Pad.qml"),
        )
        .unwrap();
        let module = project.path().join("size.ts");
        let text = "export const k: number = match (1) { default: 2 }\n";
        std::fs::write(&module, text).unwrap();
        let LtsPlan::Rename(rename) = plan_lts(project.path(), &module, text).unwrap() else {
            panic!("the module is renamed");
        };
        assert!(
            rename.importers.is_empty(),
            "{:?}",
            rename.importers.iter().map(|i| &i.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_module_with_ljos_syntax_is_still_read() {
        let module =
            "import { half } from \"./size.ts\"\nexport const k = match (1) { default: 2 }\n";
        assert_eq!(
            imports("line.lts", module, "size.ts", "size.lts").as_deref(),
            Some(
                "import { half } from \"./size.lts\"\nexport const k = match (1) { default: 2 }\n"
            )
        );
    }
}
