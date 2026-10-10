//! One script of a QML file, read as JavaScript: which names it leaves free,
//! what is TypeScript in it, and the edits that make it a plain function body.
//!
//! The script is parsed again on its own, wrapped so that it is a program
//! (an expression in `return (...)`, a statement in a function body), and
//! oxc's scope analysis says which identifiers nothing in it declares. The
//! signal's parameters are the wrapper's, so they are bound like any local.
//! Offsets are kept relative to the script's text, which is where the edits
//! apply.

use std::collections::{BTreeSet, HashMap};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, AssignmentTargetPropertyIdentifier, CallExpression, Expression, IdentifierReference,
    ObjectProperty, Program, Statement, StaticMemberExpression, TSAsExpression,
    TSNonNullExpression, TSSatisfiesExpression, TSTypeAnnotation, TSTypeAssertion,
    TSTypeParameterDeclaration, TSTypeParameterInstantiation,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::scope::ScopeId;

/// How a script is written in QML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    /// `width: parent.width / 2`.
    Expression,
    /// `onClicked: { ... }`, `width: { if (a) return 1; return 2 }`.
    Statement,
    /// `function name(a, b) { ... }`.
    Function,
}

/// A free name, as written.
#[derive(Debug, Clone)]
pub(crate) struct Free {
    pub name: String,
    /// Offsets into the script's text.
    pub span: Span,
    /// The members read off the name: `["AlignLeft"]` of `Text.AlignLeft`,
    /// up to two.
    pub chain: Vec<String>,
    /// `{ name }` in an object literal or a destructuring: a rewrite has to
    /// spell the key out.
    pub shorthand: bool,
}

/// A literal argument of a call, as far as a translation needs one.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ArgValue {
    Str(String),
    Num(f64),
    Other,
}

/// A call of a translation function the script makes.
#[derive(Debug, Clone)]
pub(crate) struct Call {
    pub callee: String,
    pub args: Vec<ArgValue>,
}

/// Replace `span` of the script's text with `text`.
#[derive(Debug, Clone)]
pub(crate) struct Edit {
    pub span: Span,
    pub text: String,
}

#[derive(Debug, Default)]
pub(crate) struct Analysis {
    /// In order of appearance, every occurrence.
    pub free: Vec<Free>,
    /// The parameters the script uses.
    pub locals: BTreeSet<String>,
    /// What is TypeScript and has to go.
    pub edits: Vec<Edit>,
    /// The script reads `arguments` of the function it is.
    pub uses_arguments: bool,
    /// The script is an expression that is a function: a handler written
    /// `onClicked: function (mouse) { }` or `(mouse) => ...`.
    pub is_function_value: bool,
    pub calls: Vec<Call>,
    /// The function a `Form::Function` script declares, and its parameters
    /// where they are plain names.
    pub function_params: Vec<String>,
}

/// The names of the functions a translatable string is marked with.
pub(crate) const TRANSLATIONS: &[&str] = &[
    "qsTr",
    "qsTranslate",
    "qsTrId",
    "qsTrNoOp",
    "qsTranslateNoOp",
    "qsTrIdNoOp",
    "QT_TR_NOOP",
    "QT_TRANSLATE_NOOP",
    "QT_TRID_NOOP",
];

/// Reads one script. `args` are the parameters its function will have (a
/// signal's), which it may use as locals. Errors are the parser's.
pub(crate) fn analyze(text: &str, form: Form, args: &[String]) -> Result<Analysis, String> {
    let (prefix, suffix) = match form {
        Form::Expression => (
            format!("function $h({}) {{ return (", args.join(", ")),
            "\n); }",
        ),
        Form::Statement => (format!("function $h({}) {{\n", args.join(", ")), "\n}"),
        Form::Function => (String::new(), ""),
    };
    let wrapped = format!("{prefix}{text}{suffix}");
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &wrapped, SourceType::ts()).parse();
    if let Some(error) = parsed.diagnostics.first() {
        return Err(error.message.to_string());
    }
    let program = &parsed.program;
    let semantic = SemanticBuilder::new().build(program).semantic;
    let scoping = semantic.scoping();
    let wrapper = wrapper_scope(program, form);
    let mut reader = Reader {
        scoping,
        offset: prefix.len() as u32,
        wrapper,
        args,
        chains: HashMap::new(),
        analysis: Analysis::default(),
    };
    reader.visit_program(program);
    let mut analysis = reader.analysis;
    analysis.is_function_value = form == Form::Expression && is_function_value(program);
    if form == Form::Function
        && let Some(Statement::FunctionDeclaration(function)) = program.body.first()
    {
        analysis.function_params = function
            .params
            .items
            .iter()
            .filter_map(|param| param.pattern.get_identifier_name().map(|n| n.to_string()))
            .collect();
    }
    analysis.edits.sort_by_key(|edit| edit.span.start);
    Ok(analysis)
}

/// The scope of the function the wrapper is, whose parameters are the
/// signal's.
fn wrapper_scope(program: &Program<'_>, form: Form) -> Option<ScopeId> {
    if form == Form::Function {
        return None;
    }
    match program.body.first()? {
        Statement::FunctionDeclaration(function) => function.scope_id.get(),
        _ => None,
    }
}

/// Whether the wrapped expression is a function itself.
fn is_function_value(program: &Program<'_>) -> bool {
    let Some(Statement::FunctionDeclaration(function)) = program.body.first() else {
        return false;
    };
    let Some(body) = &function.body else {
        return false;
    };
    let Some(Statement::ReturnStatement(ret)) = body.statements.first() else {
        return false;
    };
    let mut expression = ret.argument.as_ref();
    while let Some(Expression::ParenthesizedExpression(inner)) = expression {
        expression = Some(&inner.expression);
    }
    matches!(
        expression,
        Some(Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_))
    )
}

/// The literal values of a call's arguments.
pub(crate) fn call_args(call: &CallExpression<'_>) -> Option<(String, Vec<ArgValue>)> {
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if !TRANSLATIONS.contains(&callee.name.as_str()) {
        return None;
    }
    let args = call
        .arguments
        .iter()
        .map(|argument| match argument {
            Argument::SpreadElement(_) => ArgValue::Other,
            argument => arg_value(argument.to_expression()),
        })
        .collect();
    Some((callee.name.to_string(), args))
}

fn arg_value(expression: &Expression<'_>) -> ArgValue {
    match expression {
        Expression::StringLiteral(string) => ArgValue::Str(string.value.to_string()),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map_or(ArgValue::Other, |cooked| ArgValue::Str(cooked.to_string())),
        Expression::NumericLiteral(number) => ArgValue::Num(number.value),
        Expression::ParenthesizedExpression(inner) => arg_value(&inner.expression),
        _ => ArgValue::Other,
    }
}

struct Reader<'s, 'r> {
    scoping: &'s Scoping,
    /// Where the script's text starts in the wrapped program.
    offset: u32,
    wrapper: Option<ScopeId>,
    args: &'r [String],
    /// The members read off a free name, by where the name starts.
    chains: HashMap<u32, Vec<String>>,
    analysis: Analysis,
}

impl Reader<'_, '_> {
    /// A span of the wrapped program as one of the script's text.
    fn local(&self, span: Span) -> Span {
        Span::new(
            span.start.saturating_sub(self.offset),
            span.end.saturating_sub(self.offset),
        )
    }

    fn delete(&mut self, start: u32, end: u32) {
        if start < end && start >= self.offset {
            let span = self.local(Span::new(start, end));
            self.analysis.edits.push(Edit {
                span,
                text: String::new(),
            });
        }
    }

    /// Whether the identifier is one nothing in the script declares.
    fn free(&mut self, it: &IdentifierReference<'_>) -> bool {
        let Some(id) = it.reference_id.get() else {
            return false;
        };
        let reference = self.scoping.get_reference(id);
        if !reference.is_value() {
            return false;
        }
        match reference.symbol_id() {
            None => true,
            Some(symbol) => {
                if self.wrapper == Some(self.scoping.symbol_scope_id(symbol))
                    && self.args.iter().any(|arg| arg == it.name.as_str())
                {
                    self.analysis.locals.insert(it.name.to_string());
                }
                false
            }
        }
    }

    fn record(&mut self, it: &IdentifierReference<'_>, shorthand: bool) {
        // The wrapper's own text has no names in it.
        if it.span.start < self.offset || !self.free(it) {
            return;
        }
        if it.name == "arguments" {
            self.analysis.uses_arguments = true;
            return;
        }
        let chain = self.chains.remove(&it.span.start).unwrap_or_default();
        let span = self.local(it.span);
        self.analysis.free.push(Free {
            name: it.name.to_string(),
            span,
            chain,
            shorthand,
        });
    }
}

impl<'a> Visit<'a> for Reader<'_, '_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.record(it, false);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if it.shorthand
            && let Expression::Identifier(id) = &it.value
        {
            self.record(id, true);
            return;
        }
        walk::walk_object_property(self, it);
    }

    fn visit_assignment_target_property_identifier(
        &mut self,
        it: &AssignmentTargetPropertyIdentifier<'a>,
    ) {
        self.record(&it.binding, true);
        if let Some(init) = &it.init {
            self.visit_expression(init);
        }
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        // `A.b.c` is visited from the outside in: the outermost sees the
        // longest chain, and the first to record it wins.
        let mut chain = vec![it.property.name.to_string()];
        let mut object = &it.object;
        while let Expression::StaticMemberExpression(inner) = object {
            chain.push(inner.property.name.to_string());
            object = &inner.object;
        }
        if let Expression::Identifier(base) = object {
            chain.reverse();
            chain.truncate(2);
            self.chains.entry(base.span.start).or_insert(chain);
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some((callee, args)) = call_args(it)
            && let Expression::Identifier(id) = &it.callee
            && it.span.start >= self.offset
            && id
                .reference_id
                .get()
                .is_some_and(|id| self.scoping.get_reference(id).symbol_id().is_none())
        {
            self.analysis.calls.push(Call { callee, args });
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_ts_type_annotation(&mut self, it: &TSTypeAnnotation<'a>) {
        self.delete(it.span.start, it.span.end);
    }

    fn visit_ts_type_parameter_declaration(&mut self, it: &TSTypeParameterDeclaration<'a>) {
        self.delete(it.span.start, it.span.end);
    }

    fn visit_ts_type_parameter_instantiation(&mut self, it: &TSTypeParameterInstantiation<'a>) {
        self.delete(it.span.start, it.span.end);
    }

    fn visit_ts_as_expression(&mut self, it: &TSAsExpression<'a>) {
        self.delete(it.expression.span().end, it.span.end);
        self.visit_expression(&it.expression);
    }

    fn visit_ts_satisfies_expression(&mut self, it: &TSSatisfiesExpression<'a>) {
        self.delete(it.expression.span().end, it.span.end);
        self.visit_expression(&it.expression);
    }

    fn visit_ts_non_null_expression(&mut self, it: &TSNonNullExpression<'a>) {
        self.delete(it.expression.span().end, it.span.end);
        self.visit_expression(&it.expression);
    }

    fn visit_ts_type_assertion(&mut self, it: &TSTypeAssertion<'a>) {
        self.delete(it.span.start, it.expression.span().start);
        self.visit_expression(&it.expression);
    }
}

/// The script's text with the edits made. Edits must not overlap.
pub(crate) fn apply(text: &str, edits: &[Edit]) -> String {
    let mut edits: Vec<&Edit> = edits.iter().collect();
    edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
    let mut out = String::with_capacity(text.len() + 16 * edits.len());
    let mut at = 0usize;
    for edit in edits {
        let (start, end) = (edit.span.start as usize, edit.span.end as usize);
        if start < at || end > text.len() {
            continue;
        }
        out.push_str(&text[at..start]);
        out.push_str(&edit.text);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free(text: &str, form: Form, args: &[&str]) -> Vec<String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        analyze(text, form, &args)
            .unwrap()
            .free
            .into_iter()
            .map(|free| format!("{}{:?}", free.name, free.chain))
            .collect()
    }

    #[test]
    fn locals_and_parameters_are_not_free() {
        assert_eq!(
            free(
                "{ var a = 1; mouse.x + a + width }",
                Form::Statement,
                &["mouse"]
            ),
            ["width[]"]
        );
        assert_eq!(free("(x => x + y)(1)", Form::Expression, &[]), ["y[]"]);
    }

    #[test]
    fn member_chains_are_kept_for_the_base() {
        assert_eq!(
            free(
                "Text.AlignLeft | Image.Fill.Stretch.x",
                Form::Expression,
                &[]
            ),
            ["Text[\"AlignLeft\"]", "Image[\"Fill\", \"Stretch\"]"]
        );
    }

    #[test]
    fn typescript_is_stripped() {
        let text = "function f(a: int, b: string): bool { return (a as any)! + <int>b }";
        let analysis = analyze(text, Form::Function, &[]).unwrap();
        assert_eq!(
            apply(text, &analysis.edits),
            "function f(a, b) { return (a) + b }"
        );
        assert_eq!(analysis.function_params, ["a", "b"]);
    }

    #[test]
    fn shorthand_and_arguments_are_noticed() {
        let analysis = analyze("({ width }, arguments[0])", Form::Expression, &[]).unwrap();
        assert!(analysis.uses_arguments);
        assert!(analysis.free[0].shorthand);
    }

    #[test]
    fn function_values_are_noticed() {
        let analysis = analyze("function (m) { m.accepted = true }", Form::Expression, &[]);
        assert!(analysis.unwrap().is_function_value);
    }
}
