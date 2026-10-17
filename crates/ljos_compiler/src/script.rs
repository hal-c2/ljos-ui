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

/// `path` is the import's own, `key` where the file is from the root.
pub(crate) fn lower(path: &str, key: &str, source: &str) -> Script {
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
            let words: Vec<&str> = line
                .trim_start()
                .strip_prefix(".import")
                .map(|rest| rest.split_whitespace().collect())
                .unwrap_or_default();
            let (module, alias) = match words.as_slice() {
                [module, "as", alias] | [module, _, "as", alias] => (*module, *alias),
                _ => return blank.to_string(),
            };
            let named = |w: &str| {
                !w.is_empty()
                    && w.split('.')
                        .all(|p| p.chars().all(|c| c.is_alphanumeric() || c == '_'))
            };
            if !named(alias) || alias.contains('.') {
                return blank.to_string();
            }
            let end = if line.ends_with('\n') { "\n" } else { "" };
            if let Some(path) = module
                .strip_prefix('"')
                .and_then(|m| m.strip_suffix('"'))
                .filter(|p| p.ends_with(".js"))
            {
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

/// Where and why a script does not parse: 1-based line and column (in
/// characters), and the parser's message. None when it parses. `module`:
/// an ECMAScript module (`.mjs`).
pub(crate) fn syntax_error(source: &str, module: bool) -> Option<(u32, u32, String)> {
    let (text, _) = without_directives(source);
    let allocator = Allocator::default();
    let kind = if module {
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
        let (line, column, _) = syntax_error("let a = 1\nfunction f(n) { return n * }\n", false)
            .expect("does not parse");
        assert_eq!((line, column), (2, 28));
        assert_eq!(
            syntax_error(".pragma library\nfunction f() {}\n", false),
            None
        );
        assert_eq!(syntax_error("export function f() {}\n", true), None);
    }
}
