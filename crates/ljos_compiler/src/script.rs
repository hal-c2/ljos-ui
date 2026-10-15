//! A JavaScript file imported by path, as the engine runs it.
//!
//! The engine has no parser, so what it needs of the file is worked out
//! here: the names the file declares at its top level, which are the
//! members of the name it is imported as.

use ljos_ir::Script;
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
    let text = with_modules(source, &text);
    // Read and written through, so that a `var` the script changes is what
    // the importer reads next.
    let members: Vec<String> = names
        .iter()
        .map(|n| format!("get {n}() {{ return {n}; }}, set {n}(v) {{ {n} = v; }}"))
        .collect();
    // The text is not moved, so a fault reads the script's own lines.
    let source = format!(
        "(function () {{{text}\n;return {{ {} }};\n}})()",
        members.join(", ")
    );
    Script {
        path: path.to_string(),
        key: key.to_string(),
        library,
        source,
    }
}

/// `text` with each `.import Module as Name` line of `source` (blank in
/// `text`) declaring `Name`: the module's types, each read as a script
/// reads a type, by its enums. On the line it was, so lines stay put.
fn with_modules(source: &str, text: &str) -> String {
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
            if module.starts_with('"') || !named(module) || !named(alias) || alias.contains('.') {
                return blank.to_string();
            }
            let end = if line.ends_with('\n') { "\n" } else { "" };
            format!("const {alias} = __ljos_module(\"{module}\");{end}")
        })
        .collect()
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
