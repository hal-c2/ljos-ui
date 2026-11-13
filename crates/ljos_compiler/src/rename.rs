//! A module given a new name in its directory, and the files that import
//! it saying the new one: what `ts-lts-syntax`'s fix does to a `.ts`
//! module that uses `match` (docs/DIALECT.md §4.4), and `ljos migrate`
//! with it.
//!
//! A file names a module in one of four ways, each read as what it is
//! rather than searched for as text, so a comment or string that happens
//! to hold the name is left alone: a QML file's `import "size.ts"`, a
//! script's `.import "size.ts" as Size`, a module's `import … from
//! "./size.ts"` (or `export … from`), and a `qmldir`'s `Size 1.0 size.ts`.

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
    /// Its key below the directory importers were looked for in.
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

/// The plan for `module`, whose text is `text`, with its importers looked
/// for below `base`: every QML file, script, module and `qmldir` there
/// whose imports name it.
///
/// # Errors
/// A directory below `base` cannot be read.
pub fn plan_lts(base: &Path, module: &Path, text: &str) -> std::io::Result<LtsPlan> {
    plan_lts_with(base, module, text, |path| {
        std::fs::read_to_string(path).ok()
    })
}

/// [`plan_lts`], with each importer's text as `read` gives it: an editor
/// gives the text it has of a file it has open, which may not be saved.
///
/// # Errors
/// A directory below `base` cannot be read.
pub fn plan_lts_with(
    base: &Path,
    module: &Path,
    text: &str,
    read: impl Fn(&Path) -> Option<String>,
) -> std::io::Result<LtsPlan> {
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
    crate::files_below(base, imports_modules, &mut below)?;
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
        let Some(text) = read(&path) else {
            continue;
        };
        if let Some(renamed) = imports(&key, &text, &from, &to) {
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

    /// Renames the module, then writes its importers in the order of their
    /// keys. When one cannot be written, those written get their text back
    /// and the module its name, so a failure leaves the files as they were.
    ///
    /// # Errors
    /// Each file that could not be renamed or written, with why; the first
    /// is what failed, the rest what could not be put back.
    pub fn apply(&self) -> Result<(), Vec<(PathBuf, std::io::Error)>> {
        std::fs::rename(&self.module, &self.target).map_err(|e| vec![(self.module.clone(), e)])?;
        for (at, importer) in self.importers.iter().enumerate() {
            if let Err(e) = std::fs::write(&importer.path, &importer.renamed) {
                let mut failed = vec![(importer.path.clone(), e)];
                // The one that failed too: a write empties the file before
                // it writes, so one cut off part way holds neither text.
                for importer in &self.importers[..=at] {
                    if let Err(e) = std::fs::write(&importer.path, &importer.text) {
                        failed.push((importer.path.clone(), e));
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

/// The written path of each `import … from` and `export … from` of a
/// module that is the module, read with Ljós's syntax where it has some.
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
