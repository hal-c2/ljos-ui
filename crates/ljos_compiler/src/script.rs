//! A JavaScript file imported by path, as the engine runs it.
//!
//! The engine has no parser, so what it needs of the file is worked out
//! here: the names the file declares at its top level, which are the
//! members of the name it is imported as.

use ljos_ir::{Script, ScriptImport};
use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::i18n::without_directives;

pub(crate) mod module;

pub(crate) use module::is_module;

/// `path` is the import's own, `key` where the file is from the root.
pub(crate) fn lower(path: &str, key: &str, source: &str) -> Script {
    if is_module(key) {
        return lower_module(path, key, source);
    }
    let (text, _) = without_directives(source);
    let library = source.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with(".pragma") && line.split_whitespace().any(|word| word == "library")
    });
    let names = declared(&text);
    // Declared after the names are taken: what a script imports is its
    // own, not a member of the name it is imported as.
    let mut imports = Vec::new();
    let text = with_imports(source, &text, key, &mut imports);
    // Read and written through, so that a `var` the script changes is what
    // the importer reads next.
    let members: Vec<String> = names
        .iter()
        .map(|n| format!("get {n}() {{ return {n}; }}, set {n}(v) {{ {n} = v; }}"))
        .collect();
    // A relative address the script writes is the script's own file's, as
    // Qt resolves it, library or not, whoever calls it: the runtime calls
    // this function with that file's address, and the script's `Qt` is
    // the one that knows it (`__ljos_qt`, incubator/dynamic.rs). Written on
    // the first line, so the text's own lines do not move.
    let qt = if ["createComponent", "createQmlObject", "resolvedUrl"]
        .iter()
        .any(|maker| text.contains(maker))
    {
        "const Qt = globalThis.__ljos_qt(__ljos_url);"
    } else {
        ""
    };
    let source = format!(
        "(function (__ljos_url, __ljos_imports) {{{qt}{text}\n;return {{ {} }};\n}})",
        members.join(", ")
    );
    Script {
        path: path.to_string(),
        key: key.to_string(),
        library,
        source,
        imports,
    }
}

/// A TypeScript module, its types erased. It is made once, whoever
/// imports it, as an ECMAScript module is, and its importers read what it
/// exports and write none of it.
fn lower_module(path: &str, key: &str, source: &str) -> Script {
    let erased = module::erase(key, source);
    let members: Vec<String> = erased
        .exports
        .iter()
        .map(|(name, read)| format!("get {}() {{ return {read}; }}", module_key(name)))
        .collect();
    let qt = if ["createComponent", "createQmlObject", "resolvedUrl"]
        .iter()
        .any(|maker| erased.text.contains(maker))
    {
        "const Qt = globalThis.__ljos_qt(__ljos_url);"
    } else {
        ""
    };
    // A module's code is strict, as an ECMAScript module's is. On the
    // first line with the rest, so the module's lines do not move.
    let source = format!(
        "(function (__ljos_url, __ljos_imports) {{\"use strict\";{qt}{}\n;return {{ {} }};\n}})",
        erased.text,
        members.join(", ")
    );
    Script {
        path: path.to_string(),
        key: key.to_string(),
        library: true,
        source,
        imports: erased.imports,
    }
}

/// An exported name as a getter's name: quoted unless it is an identifier.
fn module_key(name: &str) -> String {
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !name.starts_with(|c: char| c.is_ascii_digit())
    {
        name.to_string()
    } else {
        format!("{name:?}")
    }
}

/// `text` with each `.import` line of `source` (blank in `text`) declaring
/// its name, on the line it was, so lines stay put. `.import Module as Name`
/// is the module's types, each read as a script reads a type, by its enums;
/// `.import "file.js" as Name` the object of that script, which the runtime
/// hands the script in the order of `imports`.
fn with_imports(source: &str, text: &str, key: &str, imports: &mut Vec<ScriptImport>) -> String {
    let directory = qml_front::project::directory(key);
    source
        .split_inclusive('\n')
        .zip(text.split_inclusive('\n'))
        .map(|(line, blank)| {
            let Some((module, alias)) = import_line(line) else {
                return blank.to_string();
            };
            let end = if line.ends_with('\n') { "\n" } else { "" };
            if let Some(path) = script_path(module) {
                let index = imports.len();
                imports.push(ScriptImport {
                    path: path.to_string(),
                    key: qml_front::project::join(directory, path),
                });
                return format!("const {alias} = __ljos_imports[{index}];{end}");
            }
            if module.starts_with('"') || !named(module) {
                return blank.to_string();
            }
            format!("const {alias} = __ljos_module(\"{module}\");{end}")
        })
        .collect()
}

/// The paths of the scripts `source`'s `.import` lines import, as written:
/// a module a script imports is one the file that imports the script
/// reaches.
pub(crate) fn script_imports(source: &str) -> impl Iterator<Item = &str> {
    source
        .lines()
        .filter_map(import_line)
        .filter_map(|(module, _)| script_path(module))
}

/// `.import <module> [version] as <alias>`, its module and alias.
fn import_line(line: &str) -> Option<(&str, &str)> {
    let words: Vec<&str> = line
        .trim_start()
        .strip_prefix(".import")?
        .split_whitespace()
        .collect();
    let (module, alias) = match words.as_slice() {
        [module, "as", alias] | [module, _, "as", alias] => (*module, *alias),
        _ => return None,
    };
    (named(alias) && !alias.contains('.')).then_some((module, alias))
}

/// A dotted name: `QtQuick.LocalStorage`.
fn named(word: &str) -> bool {
    !word.is_empty()
        && word
            .split('.')
            .all(|p| p.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// The path of a quoted script: `"file.js"`.
fn script_path(module: &str) -> Option<&str> {
    module
        .strip_prefix('"')
        .and_then(|m| m.strip_suffix('"'))
        .filter(|p| qml_front::project::is_script(p))
}

/// Where and why a script does not parse: 1-based line and column (in
/// characters), and the parser's message. None when it parses. `key` says
/// how it is read: an ECMAScript module (`.mjs`), a TypeScript one (`.ts`),
/// or a script.
pub(crate) fn syntax_error(source: &str, key: &str) -> Option<(u32, u32, String)> {
    let (text, _) = without_directives(source);
    let allocator = Allocator::default();
    let kind = if is_module(key) {
        module::source_type()
    } else if key.ends_with(".mjs") {
        SourceType::mjs()
    } else {
        SourceType::script()
    };
    let parsed = Parser::new(&allocator, &text, kind).parse();
    let error = parsed.diagnostics.first()?;
    let offset = error.labels.first().map_or(0, |label| label.offset());
    let (line, column) = qml_syntax::position(&text, offset);
    Some((
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(column).unwrap_or(u32::MAX),
        error.message.to_string(),
    ))
}

/// The names a script declares at its top level, in order.
fn declared(text: &str) -> Vec<String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, text, SourceType::script()).parse();
    let mut names: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    };
    for statement in &parsed.program.body {
        match statement {
            Statement::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    add(id.name.as_str());
                }
            }
            Statement::ClassDeclaration(c) => {
                if let Some(id) = &c.id {
                    add(id.name.as_str());
                }
            }
            Statement::VariableDeclaration(d) => {
                for declarator in &d.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                        add(id.name.as_str());
                    }
                }
            }
            _ => {}
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::syntax_error;

    #[test]
    fn a_script_that_does_not_parse_says_where() {
        let (line, column, _) = syntax_error("let a = 1\nfunction f(n) { return n * }\n", "a.js")
            .expect("does not parse");
        assert_eq!((line, column), (2, 28));
        assert_eq!(
            syntax_error(".pragma library\nfunction f() {}\n", "a.js"),
            None
        );
        assert_eq!(syntax_error("export function f() {}\n", "a.mjs"), None);
        assert_eq!(
            syntax_error(
                "export function f(n: number): number { return n }\n",
                "a.ts"
            ),
            None
        );
    }
}
