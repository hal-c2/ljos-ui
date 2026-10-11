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
    Argument, AssignmentTargetPropertyIdentifier, CallExpression, ChainExpression, Expression,
    IdentifierReference, NewExpression, ObjectProperty, Program, SimpleAssignmentTarget, Statement,
    StaticMemberExpression, StringLiteral, TSAsExpression, TSNonNullExpression,
    TSSatisfiesExpression, TSTypeAnnotation, TSTypeAssertion, TSTypeParameterDeclaration,
    TSTypeParameterInstantiation, TaggedTemplateExpression,
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
    /// Where each member of `chain` ends in the script's text.
    pub chain_ends: Vec<u32>,
    /// The script assigns to the name (`n = 1`, `n++`).
    pub written: bool,
    /// `{ name }` in an object literal or a destructuring: a rewrite has to
    /// spell the key out.
    pub shorthand: bool,
}

/// `object.Name`: a member that is named as a type is, read off a value.
/// When the name is a type that attaches, this is what the type attached
/// to the value (`other.Layout` of `other.Layout.row`).
#[derive(Debug, Clone)]
pub(crate) struct TypeMember {
    pub name: String,
    /// What the member is read off, whole.
    pub object: Span,
    /// Where the member's name ends.
    pub end: u32,
    /// Where `object` starts when it is nothing but a name.
    pub base: Option<u32>,
}

/// What a member is written on: `a.b` of `a.b.x = 1`. Where that is a value
/// type (a `rect`, a `color`), the write has to go back to where the value
/// was read from, so the compiler needs to know where that is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ValueSource {
    /// `object.name`.
    Member {
        object: Span,
        name: String,
        /// Where `name` ends.
        end: u32,
    },
    /// `object[key]`.
    Element {
        object: Span,
        key: Span,
        /// Where the `]` ends.
        end: u32,
    },
    /// A name nothing in the script declares, by where it starts.
    Free(u32),
    /// A variable or a parameter of the script's that it can assign to.
    Local { span: Span, name: String },
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
    /// In order of appearance.
    pub type_members: Vec<TypeMember>,
    /// The function a `Form::Function` script declares, and its parameters
    /// where they are plain names.
    pub function_params: Vec<String>,
    /// What the script writes members of, in order of appearance.
    pub member_writes: Vec<ValueSource>,
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
    // The script is a piece of a QML document, where a string literal may
    // go on over the end of a line.
    let parsed = Parser::new(&allocator, &wrapped, SourceType::ts()).parse_qml_script();
    if let Some(error) = parsed.diagnostics.first() {
        return Err(error.message.to_string());
    }
    let program = &parsed.program;
    let semantic = SemanticBuilder::new().build(program).semantic;
    let scoping = semantic.scoping();
    let wrapper = wrapper_scope(program, form);
    let mut reader = Reader {
        scoping,
        source: &wrapped,
        offset: prefix.len() as u32,
        wrapper,
        args,
        chains: HashMap::new(),
        not_values: Vec::new(),
        optional: 0,
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
    /// The wrapped program's text.
    source: &'s str,
    /// Where the script's text starts in the wrapped program.
    offset: u32,
    wrapper: Option<ScopeId>,
    args: &'r [String],
    /// The members read off a free name and where each ends, by where the
    /// name starts.
    chains: HashMap<u32, (Vec<String>, Vec<u32>)>,
    /// Member expressions that are not read for their value: one that is
    /// assigned to, or called (the call needs its `this`).
    not_values: Vec<Span>,
    /// How many `a?.b` chains the visitor is inside.
    optional: u32,
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

    /// Whether the identifier is one nothing in the script declares, and
    /// then whether the script assigns to it there.
    fn free(&mut self, it: &IdentifierReference<'_>) -> Option<bool> {
        let id = it.reference_id.get()?;
        let reference = self.scoping.get_reference(id);
        if !reference.is_value() {
            return None;
        }
        match reference.symbol_id() {
            None => Some(reference.is_write()),
            Some(symbol) => {
                if self.wrapper == Some(self.scoping.symbol_scope_id(symbol))
                    && self.args.iter().any(|arg| arg == it.name.as_str())
                {
                    self.analysis.locals.insert(it.name.to_string());
                }
                None
            }
        }
    }

    fn record(&mut self, it: &IdentifierReference<'_>, shorthand: bool) {
        // The wrapper's own text has no names in it.
        if it.span.start < self.offset {
            return;
        }
        let Some(written) = self.free(it) else {
            return;
        };
        if it.name == "arguments" {
            self.analysis.uses_arguments = true;
            return;
        }
        let (chain, ends) = self.chains.remove(&it.span.start).unwrap_or_default();
        let span = self.local(it.span);
        self.analysis.free.push(Free {
            name: it.name.to_string(),
            span,
            chain,
            chain_ends: ends.iter().map(|e| e - self.offset).collect(),
            written,
            shorthand,
        });
    }

    /// `on.member = v`: notes what `on` is, where a write to a value it
    /// holds could be taken back to.
    fn written_on(&mut self, mut on: &Expression<'_>) {
        while let Expression::ParenthesizedExpression(inner) = on {
            on = &inner.expression;
        }
        if on.span().start < self.offset {
            return;
        }
        let source = match on {
            Expression::StaticMemberExpression(member)
                if !member.optional
                    && !matches!(member.object, Expression::Super(_))
                    // `other.Layout.row = 1`: what a type attached is an
                    // object, and is found another way.
                    && !member
                        .property
                        .name
                        .starts_with(|c: char| c.is_ascii_uppercase()) =>
            {
                ValueSource::Member {
                    object: self.local(member.object.span()),
                    name: member.property.name.to_string(),
                    end: member.property.span.end - self.offset,
                }
            }
            Expression::ComputedMemberExpression(member)
                if !member.optional && !matches!(member.object, Expression::Super(_)) =>
            {
                ValueSource::Element {
                    object: self.local(member.object.span()),
                    key: self.local(member.expression.span()),
                    end: member.span.end - self.offset,
                }
            }
            Expression::Identifier(id) => {
                let Some(reference) = id.reference_id.get() else {
                    return;
                };
                match self.scoping.get_reference(reference).symbol_id() {
                    None => ValueSource::Free(id.span.start - self.offset),
                    Some(symbol) => {
                        let flags = self.scoping.symbol_flags(symbol);
                        // A `const` cannot be given the changed value, nor
                        // can a function or a class be one.
                        if flags.is_const_variable()
                            || !(flags.is_variable() || flags.is_catch_variable())
                        {
                            return;
                        }
                        ValueSource::Local {
                            span: self.local(id.span),
                            name: id.name.to_string(),
                        }
                    }
                }
            }
            _ => return,
        };
        self.analysis.member_writes.push(source);
    }

    /// `object.Name`, where it is a value the script reads.
    fn type_member(&mut self, it: &StaticMemberExpression<'_>) {
        let name = it.property.name.as_str();
        if !name.starts_with(|c: char| c.is_ascii_uppercase())
            || it.optional
            || self.optional > 0
            || it.span.start < self.offset
            || self.not_values.contains(&it.span)
        {
            return;
        }
        let base = match &it.object {
            Expression::Identifier(id) => Some(id.span.start - self.offset),
            _ => None,
        };
        self.analysis.type_members.push(TypeMember {
            name: name.to_string(),
            object: self.local(it.object.span()),
            end: it.property.span.end - self.offset,
            base,
        });
    }
}

impl<'a> Visit<'a> for Reader<'_, '_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.record(it, false);
    }

    // A string that goes on over the end of a line is QML's, not
    // JavaScript's: the module the engine runs needs the break written as an
    // escape. It is one line feed whatever the file ends its lines with. The
    // escape is followed by a line continuation, so that what comes after
    // the string stays on the line it has in the document.
    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        if it.span.start < self.offset {
            return;
        }
        let raw = &self.source[it.span.start as usize..it.span.end as usize];
        if !raw.contains(['\n', '\r']) {
            return;
        }
        let bytes = raw.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            let len = match bytes[at] {
                // An escape keeps what follows it: `\` and a break join
                // the two lines.
                b'\\' => {
                    at += 2;
                    if bytes.get(at - 1) == Some(&b'\r') && bytes.get(at) == Some(&b'\n') {
                        at += 1;
                    }
                    continue;
                }
                b'\r' if bytes.get(at + 1) == Some(&b'\n') => 2,
                b'\r' | b'\n' => 1,
                _ => {
                    at += 1;
                    continue;
                }
            };
            let start = it.span.start + at as u32;
            let span = self.local(Span::new(start, start + len));
            self.analysis.edits.push(Edit {
                span,
                text: "\\n\\\n".to_owned(),
            });
            at += len as usize;
        }
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
        let mut ends = vec![it.property.span.end];
        let mut object = &it.object;
        while let Expression::StaticMemberExpression(inner) = object {
            chain.push(inner.property.name.to_string());
            ends.push(inner.property.span.end);
            object = &inner.object;
        }
        if let Expression::Identifier(base) = object {
            chain.reverse();
            chain.truncate(2);
            ends.reverse();
            ends.truncate(2);
            self.chains.entry(base.span.start).or_insert((chain, ends));
        }
        self.type_member(it);
        walk::walk_static_member_expression(self, it);
    }

    fn visit_simple_assignment_target(&mut self, it: &SimpleAssignmentTarget<'a>) {
        self.not_values.push(it.span());
        match it {
            SimpleAssignmentTarget::StaticMemberExpression(member) if !member.optional => {
                self.written_on(&member.object);
            }
            SimpleAssignmentTarget::ComputedMemberExpression(member) if !member.optional => {
                self.written_on(&member.object);
            }
            _ => {}
        }
        walk::walk_simple_assignment_target(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        self.not_values.push(it.callee.span());
        walk::walk_new_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        self.not_values.push(it.tag.span());
        walk::walk_tagged_template_expression(self, it);
    }

    fn visit_chain_expression(&mut self, it: &ChainExpression<'a>) {
        self.optional += 1;
        walk::walk_chain_expression(self, it);
        self.optional -= 1;
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.not_values.push(it.callee.span());
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
    fn a_line_break_in_a_string_is_written_as_an_escape() {
        let edited = |text: &str| {
            let analysis = analyze(text, Form::Expression, &[]).unwrap();
            apply(text, &analysis.edits)
        };
        assert_eq!(edited("\"a\nb\" + 'c\r\nd'"), "\"a\\n\\\nb\" + 'c\\n\\\nd'");
        assert_eq!(edited("\"a\rb\""), "\"a\\n\\\nb\"");
        // A line continuation and a template are JavaScript already.
        assert_eq!(edited("\"a\\\nb\""), "\"a\\\nb\"");
        assert_eq!(edited("\"a\\\r\nb\""), "\"a\\\r\nb\"");
        assert_eq!(edited("`a\nb`"), "`a\nb`");
        // An escaped backslash does not escape the break after it.
        assert_eq!(edited("\"a\\\\\nb\""), "\"a\\\\\\n\\\nb\"");
        // The lines after the string keep their numbers.
        let text = "\"a\nb\" +\nwidth";
        assert_eq!(edited(text).lines().count(), text.lines().count());
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
    fn writes_and_chain_ends_are_noticed() {
        let text = "{ count++; total = count + NS.Layout.row }";
        let analysis = analyze(text, Form::Statement, &[]).unwrap();
        let written: Vec<(&str, bool)> = analysis
            .free
            .iter()
            .map(|f| (f.name.as_str(), f.written))
            .collect();
        assert_eq!(
            written,
            [
                ("count", true),
                ("total", true),
                ("count", false),
                ("NS", false)
            ]
        );
        let ns = &analysis.free[3];
        let ends: Vec<&str> = ns
            .chain_ends
            .iter()
            .map(|e| &text[ns.span.start as usize..*e as usize])
            .collect();
        assert_eq!(ends, ["NS.Layout", "NS.Layout.row"]);
    }

    #[test]
    fn members_named_as_types_are_noticed_where_they_are_read() {
        let text = "a.Layout.row + (b.c).Keys.enabled + d?.Layout.row + e.Run() + (f.Layout = 1)";
        let analysis = analyze(text, Form::Expression, &[]).unwrap();
        let found: Vec<(&str, &str, bool)> = analysis
            .type_members
            .iter()
            .map(|m| {
                (
                    &text[m.object.start as usize..m.object.end as usize],
                    &text[m.object.start as usize..m.end as usize],
                    m.base.is_some(),
                )
            })
            .collect();
        assert_eq!(
            found,
            [("a", "a.Layout", true), ("(b.c)", "(b.c).Keys", false)]
        );
    }

    #[test]
    fn what_a_member_is_written_on_is_noticed() {
        let text = "{ var v, k = 0; const c = 1; \
                    a.b.x = 1; (a.c).y += 2; a[k].x++; d.x = 3; v.x = 4; m.x = 5; \
                    c.x = 6; f().x = 7; this.x = 8; a.Layout.row = 9; [v.y] = [1] }";
        let analysis = analyze(text, Form::Statement, &["m".to_string()]).unwrap();
        let at = |span: &Span| &text[span.start as usize..span.end as usize];
        let found: Vec<String> = analysis
            .member_writes
            .iter()
            .map(|source| match source {
                ValueSource::Member { object, name, end } => {
                    format!(
                        "{}|{}",
                        at(object),
                        &text[object.end as usize..*end as usize]
                    ) + &format!("|{name}")
                }
                ValueSource::Element { object, key, end } => {
                    format!("{}[{}]{}", at(object), at(key), end - key.end)
                }
                ValueSource::Free(start) => format!("free {}", &text[*start as usize..][..1]),
                ValueSource::Local { span, name } => format!("local {} {name}", at(span)),
            })
            .collect();
        assert_eq!(
            found,
            [
                "a|.b|b",
                "a|.c|c",
                "a[k]1",
                "free d",
                "local v v",
                "local m m",
                "local v v"
            ]
        );
    }

    #[test]
    fn function_values_are_noticed() {
        let analysis = analyze("function (m) { m.accepted = true }", Form::Expression, &[]);
        assert!(analysis.unwrap().is_function_value);
    }
}
