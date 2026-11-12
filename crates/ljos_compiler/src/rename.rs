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
