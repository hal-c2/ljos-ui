//! A Controller a TypeScript module exports (docs/DIALECT.md §4.20):
//! `export const Breakpoint = controller({ properties, bindings, attached,
//! detached, updated })`. Each is made a QML file whose root is a
//! `Controller`, keyed under the module (`breakpoint.ts/Breakpoint`), so
//! `import "breakpoint.ts" as Size` names it `Size.Breakpoint` and the
//! front end, the compiler and the runtime read it as any other file: the
//! object it makes is the one the QML form makes.
//!
//! What the file holds is the module's: each binding and each function of
//! the lifecycle is called from the file with the host and the Controller,
//! so the module's code runs as the module's, and a binding reads what it
//! reads through the call.

use std::borrow::Cow;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, BinaryOperator, Declaration, Expression, ObjectExpression,
    ObjectPropertyKind, Statement, TSType, UnaryOperator,
};
use oxc_parser::Parser;
use oxc_span::GetSpan;

use crate::script::module;

/// The name every module has for making a Controller.
pub(crate) const MAKER: &str = "controller";

/// The keys a controller's spec may have.
pub(crate) const KEYS: [&str; 5] = ["properties", "bindings", "attached", "detached", "updated"];

/// The QML files of the Controllers the module at `key` exports: each
/// file's key, and its text.
pub(crate) fn files(key: &str, source: &str) -> Vec<(String, String)> {
    if !module::is_module(key) || !source.contains(MAKER) {
        return Vec::new();
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, module::source_type()).parse();
    if !parsed.diagnostics.is_empty() || declares_maker(&parsed.program.body) {
        return Vec::new();
    }
    let script = key.rsplit('/').next().unwrap_or(key);
    let mut out = Vec::new();
    for statement in &parsed.program.body {
        let Statement::ExportDeclaration(export) = statement else {
            continue;
        };
        let Declaration::VariableDeclaration(declaration) = &export.declaration else {
            continue;
        };
        for declarator in &declaration.declarations {
            let (Some(name), Some(spec)) = (
                declarator.id.get_identifier_name(),
                declarator.init.as_ref().and_then(spec_of),
            ) else {
                continue;
            };
            out.push((
                format!("{key}/{name}"),
                file(script, name.as_str(), spec, source),
            ));
        }
    }
    out
}

/// Whether the module at `key` reads `controller` as every module has it:
/// it names it and declares no name of its own for it.
pub(crate) fn uses_maker(key: &str, source: &str) -> bool {
    if !module::is_module(key) || !source.contains(MAKER) {
        return false;
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, module::source_type()).parse();
    !declares_maker(&parsed.program.body)
}

/// Whether the key `key` is a file made of a module's Controller.
pub fn is_generated(key: &str) -> bool {
    key.rsplit_once('/')
        .is_some_and(|(dir, _)| module::is_module(dir))
}

/// The spec of `controller({ … })`.
pub(crate) fn spec_of<'b, 'a>(init: &'b Expression<'a>) -> Option<&'b ObjectExpression<'a>> {
    let Expression::CallExpression(call) = init.without_parentheses() else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name != MAKER || call.arguments.len() != 1 {
        return None;
    }
    match call.arguments[0].as_expression()?.without_parentheses() {
        Expression::ObjectExpression(spec) => Some(spec),
        _ => None,
    }
}

fn declares_maker(body: &[Statement<'_>]) -> bool {
    body.iter().any(|statement| {
        let declaration = match statement {
            Statement::ExportDeclaration(export) => Some(&export.declaration),
            other => other.as_declaration(),
        };
        match declaration {
            Some(Declaration::VariableDeclaration(v)) => v
                .declarations
                .iter()
                .any(|d| d.id.get_identifier_name().is_some_and(|n| n == MAKER)),
            Some(Declaration::FunctionDeclaration(f)) => {
                f.id.as_ref().is_some_and(|id| id.name == MAKER)
            }
            Some(Declaration::ClassDeclaration(c)) => {
                c.id.as_ref().is_some_and(|id| id.name == MAKER)
            }
            _ => false,
        }
    }) || body.iter().any(|statement| {
        let Statement::ImportDeclaration(import) = statement else {
            return false;
        };
        import
            .specifiers
            .iter()
            .flatten()
            .any(|s| s.local().name == MAKER)
    })
}

/// The members of the spec: each key, and what it is given.
pub(crate) fn members<'b, 'a>(
    spec: &'b ObjectExpression<'a>,
) -> impl Iterator<Item = (Cow<'a, str>, &'b Expression<'a>)> {
    spec.properties.iter().filter_map(|p| match p {
        ObjectPropertyKind::ObjectProperty(p) if !p.computed => {
            Some((p.key.static_name()?, &p.value))
        }
        _ => None,
    })
}

/// The text of the file of the Controller `name` the module `script` (the
/// module's file name) exports.
fn file(script: &str, name: &str, spec: &ObjectExpression<'_>, source: &str) -> String {
    let at = |e: &Expression<'_>| {
        let span = e.span();
        &source[span.start as usize..span.end as usize]
    };
    let mut lines = vec![
        "import Ljos.Core".to_string(),
        format!("import \"../{script}\" as Spec"),
        String::new(),
        "Controller {".to_string(),
        "    id: controller".to_string(),
    ];
    // Indexed: `Spec.Breakpoint` names the type this file is, and the
    // spec is the module's value of the name.
    let read = format!("Spec[\"{name}\"]");
    let call = |what: &str| format!("{read}.{what}(controller.host, controller)");
    for (key, value) in members(spec) {
        match &*key {
            "properties" => {
                let Expression::ObjectExpression(properties) = value.without_parentheses() else {
                    continue;
                };
                for (property, value) in members(properties) {
                    let line = match literal(value) {
                        Some(ty) => format!("    property {ty} {property}: {}", at(value)),
                        None => {
                            format!("    property var {property}: {read}.properties.{property}")
                        }
                    };
                    lines.push(line);
                }
            }
            "bindings" => {
                let Expression::ObjectExpression(bindings) = value.without_parentheses() else {
                    continue;
                };
                for (binding, value) in members(bindings) {
                    let ty = returned(value);
                    lines.push(format!(
                        "    readonly property {ty} {binding}: controller.host ? {} : {}",
                        call(&format!("bindings.{binding}")),
                        empty(ty),
                    ));
                }
            }
            "attached" => {
                lines.push("    onAttached: {".to_string());
                lines.push(format!("        const done = {}", call("attached")));
                lines.push(
                    "        if (typeof done === \"function\") controller.detached.connect(done)"
                        .to_string(),
                );
                lines.push("    }".to_string());
            }
            "detached" | "updated" => {
                let handler = if key == "detached" {
                    "onDetached"
                } else {
                    "onUpdated"
                };
                lines.push(format!("    {handler}: {}", call(&key)));
            }
            _ => {}
        }
    }
    lines.push("}".to_string());
    lines.push(String::new());
    lines.join("\n")
}

/// The QML type of a property given a literal, written as it is.
fn literal(value: &Expression<'_>) -> Option<&'static str> {
    match value.without_parentheses() {
        Expression::NumericLiteral(_) => Some("real"),
        Expression::StringLiteral(_) => Some("string"),
        Expression::BooleanLiteral(_) => Some("bool"),
        Expression::UnaryExpression(u)
            if u.operator == UnaryOperator::UnaryNegation
                && matches!(u.argument, Expression::NumericLiteral(_)) =>
        {
            Some("real")
        }
        _ => None,
    }
}

/// The QML type of what a binding returns: its annotation's, else what
/// the expression it returns is written as, else `var`.
// DECISION: a binding's type is read from its written return type or the
// shape of the expression an arrow returns; the checker infers it fully,
// and the file only needs a property type the runtime converts to.
fn returned(value: &Expression<'_>) -> &'static str {
    let arrow: &ArrowFunctionExpression<'_> = match value.without_parentheses() {
        Expression::ArrowFunctionExpression(arrow) => arrow,
        Expression::FunctionExpression(f) => {
            return f
                .return_type
                .as_ref()
                .map_or("var", |t| annotated(&t.type_annotation));
        }
        _ => return "var",
    };
    if let Some(t) = &arrow.return_type {
        return annotated(&t.type_annotation);
    }
    match arrow.get_expression() {
        Some(e) => shaped(e),
        None => "var",
    }
}

fn annotated(ty: &TSType<'_>) -> &'static str {
    match ty {
        TSType::TSBooleanKeyword(_) => "bool",
        TSType::TSNumberKeyword(_) => "real",
        TSType::TSStringKeyword(_) => "string",
        _ => "var",
    }
}

fn shaped(e: &Expression<'_>) -> &'static str {
    match e.without_parentheses() {
        Expression::BooleanLiteral(_) => "bool",
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => "string",
        Expression::NumericLiteral(_) => "real",
        Expression::UnaryExpression(u) => match u.operator {
            UnaryOperator::LogicalNot => "bool",
            UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus => "real",
            _ => "var",
        },
        Expression::BinaryExpression(b) => match b.operator {
            op if op.is_equality() || op.is_compare() => "bool",
            BinaryOperator::In | BinaryOperator::Instanceof => "bool",
            BinaryOperator::Addition => "var",
            _ => "real",
        },
        Expression::ConditionalExpression(c) => {
            let (a, b) = (shaped(&c.consequent), shaped(&c.alternate));
            if a == b { a } else { "var" }
        }
        _ => "var",
    }
}

/// What a binding is while the Controller has no host.
fn empty(ty: &str) -> &'static str {
    match ty {
        "bool" => "false",
        "real" => "0",
        "string" => "\"\"",
        _ => "undefined",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BREAKPOINT: &str = r#"export const Breakpoint = controller({
  properties: { width: 600 },
  bindings: {
    compact: (host, self) => host.width < self.width,
  },
  attached(host, self) {
    return () => {}
  },
})
"#;

    #[test]
    fn a_module_controller_is_a_qml_file_beside_it() {
        let files = files("lib/breakpoint.ts", BREAKPOINT);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "lib/breakpoint.ts/Breakpoint");
        let text = &files[0].1;
        assert!(
            text.contains("import \"../breakpoint.ts\" as Spec"),
            "{text}"
        );
        assert!(text.contains("property real width: 600"), "{text}");
        assert!(
            text.contains("readonly property bool compact: controller.host ? Spec[\"Breakpoint\"].bindings.compact(controller.host, controller) : false"),
            "{text}"
        );
        assert!(text.contains("onAttached"), "{text}");
        assert!(!text.contains("onUpdated"), "{text}");
        assert!(is_generated("lib/breakpoint.ts/Breakpoint"));
        assert!(!is_generated("lib/Breakpoint"));
    }

    #[test]
    fn a_module_with_its_own_controller_makes_none() {
        let own = format!("function controller(spec: object) {{ return spec }}\n{BREAKPOINT}");
        assert!(files("a.ts", &own).is_empty());
        assert!(!uses_maker("a.ts", &own));
        assert!(uses_maker("a.ts", BREAKPOINT));
        assert!(files("a.js", BREAKPOINT).is_empty());
    }
}
