//! One script of a QML file, read as JavaScript: which names it leaves free,
//! what is TypeScript in it, and the edits that make it a plain function body.
//!
//! The script is parsed again on its own, wrapped so that it is a program
//! (an expression in `return (...)`, a statement in a function body), and
//! oxc's scope analysis says which identifiers nothing in it declares. The
//! signal's parameters are the wrapper's, so they are bound like any local.
//! Offsets are kept relative to the script's text, which is where the edits
//! apply.

use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, AssignmentTargetPropertyIdentifier, BinaryExpression, BindingPattern, CallExpression,
    ChainElement, ChainExpression, Expression, IdentifierReference, NewExpression, ObjectProperty,
    Program, SimpleAssignmentTarget, Statement, StaticMemberExpression, StringLiteral,
    TSAsExpression, TSNonNullExpression, TSSatisfiesExpression, TSTypeAnnotation, TSTypeAssertion,
    TSTypeParameterDeclaration, TSTypeParameterInstantiation, TaggedTemplateExpression,
    UnaryExpression, VariableDeclaration, VariableDeclarationKind,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};
use oxc_syntax::scope::{ScopeFlags, ScopeId};

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
    /// `typeof name`: asking what a name is does not throw where nothing
    /// has the name.
    pub asked: bool,
    /// The name starts what `new` makes one of (`new Name()`,
    /// `new Name.Kind()`): a call written in its place has to be put in
    /// parentheses, or `new` would take the call for the constructor.
    pub constructed: bool,
    /// The name alone is what `instanceof` asks of (`x instanceof Name`):
    /// a type that attaches is the type there, not what it attached.
    pub instance_type: bool,
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
    /// `object` as `value.Name`, where `Name` may be a namespace the type
    /// is named through (`item.L.Layout`): the name and `value`, whole.
    pub through: Option<(String, Span)>,
    /// The `a?.b` chain the member is on the spine of, in
    /// [`Analysis::optional_chains`].
    pub chain: Option<usize>,
}

/// An `a?.b` chain, outermost: where a `?.` meets nothing, the whole of it
/// is `undefined`, and where it meets something the rest is read as
/// written, so a member after it that is nothing throws.
#[derive(Debug, Clone)]
pub(crate) struct OptionalChain {
    pub span: Span,
    /// Its `?.`s, first to last.
    pub links: Vec<Link>,
    /// The parentheses that can go when the chain is written out:
    /// `(a?.b)?.c` is `a?.b?.c`, the `?.` after them stopping the same
    /// chain, and `(a.f)?.()` calls `f` with `this` as `a.f?.()` does.
    pub parens: Vec<Span>,
}

/// A `?.` of an [`OptionalChain`].
#[derive(Debug, Clone)]
pub(crate) struct Link {
    /// The `?.` itself.
    pub token: Span,
    pub kind: LinkKind,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LinkKind {
    /// `?.name`, `?.#name`.
    Member,
    /// `?.[key]`.
    Element,
    /// `?.(args)`.
    Call(Option<This>),
}

/// What an optional call is made on, where its callee is a member: the
/// function is called with that as `this`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum This {
    /// `a` of `a.f?.()`, whole, and where the callee's name ends when it
    /// is one (`f`).
    Value { object: Span, name_end: Option<u32> },
    /// `super.f?.()`: `super` cannot be read alone.
    Super,
}

/// An `await` or a `yield` of the script's, and where the function it
/// waits in starts (None: the wrapper the script is put in).
#[derive(Debug, Clone)]
pub(crate) struct Suspend {
    pub at: u32,
    pub function: Option<u32>,
    pub yields: bool,
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
    /// A variable or a parameter of the script's. A `const` whose members
    /// are written is declared `let`, so that it can be given the changed
    /// value.
    Local {
        span: Span,
        name: String,
        /// Which variable, for [`Place::symbol`].
        symbol: u32,
    },
}

/// `p = spot` of `const p = spot`: a variable whose members the script
/// writes, given a value read from somewhere. In Qt the variable names
/// that place, and a write to one of its members goes back there, so the
/// compiler keeps the place beside the variable. Only a variable declared
/// once and never assigned again has one.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Place {
    pub symbol: u32,
    pub name: String,
    /// From the variable's name to where its value starts (`p = `).
    pub binding: Span,
    /// Where the value is read from: [`ValueSource::Member`],
    /// [`ValueSource::Element`] or [`ValueSource::Free`].
    pub source: ValueSource,
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
    /// The parameters the script uses, in the order it first uses them.
    pub locals: Vec<String>,
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
    /// The chains a [`TypeMember`] can be on, outermost first.
    pub optional_chains: Vec<OptionalChain>,
    /// In order of appearance.
    pub suspends: Vec<Suspend>,
    /// The function a `Form::Function` script declares, and its parameters
    /// where they are plain names.
    pub function_params: Vec<String>,
    /// A `Form::Function` script that can be the function it is exported
    /// as, its parameters after the ones given for its names: one with
    /// plain parameters that reads neither `arguments` nor its own name.
    /// What is left of the declaration is dropped by
    /// [`Flat::head`]; [`Flat::names`] are what its body declares, which
    /// must not be the names given before.
    pub flat: Option<Flat>,
    /// What the script writes members of, in order of appearance.
    pub member_writes: Vec<ValueSource>,
    /// The variables in `member_writes` whose value was read from a place.
    pub places: Vec<Place>,
}

/// See [`Analysis::flat`].
#[derive(Debug)]
pub(crate) struct Flat {
    /// The script's text up to and with the `(` of its parameters.
    pub head: Span,
    /// The parameters and what the function's own scope declares.
    pub names: Vec<String>,
}

/// `p = <init>` of a declaration, for [`Place`].
struct Declarator {
    symbol: oxc_syntax::symbol::SymbolId,
    /// The `const` the declaration starts with.
    constant: Option<Span>,
    binding_start: u32,
    /// Where the value is read from, and where the value starts.
    init: Option<(ValueSource, u32)>,
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
        chain_of: HashMap::new(),
        absorbed: Vec::new(),
        functions: Vec::new(),
        asked: Vec::new(),
        constructed: Vec::new(),
        instance_types: Vec::new(),
        declarators: Vec::new(),
        analysis: Analysis::default(),
    };
    reader.visit_program(program);
    reader.hold_written_locals();
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
        // qml6 makes them so only of a function all of whose parameters are
        // typed: one left bare leaves every argument as it was given.
        let typed = function
            .params
            .items
            .iter()
            .all(|param| param.type_annotation.is_some());
        if let Some(body) = function.body.as_ref().filter(|_| typed) {
            let coerced: String = function
                .params
                .items
                .iter()
                .filter_map(|param| {
                    let name = param.pattern.get_identifier_name()?;
                    let ty = param.type_annotation.as_ref()?.type_annotation.span();
                    coercion(&wrapped[ty.start as usize..ty.end as usize], &name)
                })
                .collect();
            if !coerced.is_empty() {
                let at = body.span.start + 1;
                analysis.edits.push(Edit {
                    span: Span::new(at, at),
                    text: coerced,
                });
            }
        }
        let plain = !function.r#async
            && !function.generator
            && function.type_parameters.is_none()
            && function.this_param.is_none()
            && function.params.rest.is_none()
            && function.params.items.iter().all(|param| {
                param.initializer.is_none() && param.pattern.get_identifier_name().is_some()
            });
        let named = function
            .id
            .as_ref()
            .and_then(|id| id.symbol_id.get())
            .is_some_and(|symbol| !scoping.get_resolved_reference_ids(symbol).is_empty());
        if plain
            && !named
            && !analysis.uses_arguments
            && let Some(scope) = function.scope_id.get()
        {
            analysis.flat = Some(Flat {
                head: Span::new(0, function.params.span.start + 1),
                names: scoping
                    .get_bindings(scope)
                    .keys()
                    .map(|name| name.to_string())
                    .collect(),
            });
        }
    }
    analysis.edits.sort_by_key(|edit| edit.span.start);
    Ok(analysis)
}

/// What a method makes of what it is given for a parameter of a type a
/// script's value is one of: as qml6 does, a number as `Number()` does, an
/// int that cut to 32 bits, a string as `String()` does, and a bool as
/// `Boolean()` does. Each is made so before the body runs, a missing
/// argument being made of `undefined`.
fn coercion(ty: &str, name: &str) -> Option<String> {
    let made = match ty.trim() {
        "real" | "double" => format!("Number({name})"),
        "int" => format!("Number({name}) | 0"),
        "string" => format!("String({name})"),
        "bool" => format!("!!{name}"),
        _ => return None,
    };
    Some(format!(" {name} = {made};"))
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

/// One step of an `a?.b` chain's spine, outermost first.
enum Step<'b, 'a> {
    Member {
        object: &'b Expression<'a>,
        optional: bool,
        /// Where the member's name starts, when it has one.
        name: Option<u32>,
        /// `a[key]`.
        element: bool,
    },
    Call {
        callee: &'b Expression<'a>,
        optional: bool,
    },
    /// `a!`: the same value.
    Through(&'b Expression<'a>),
}

impl<'b, 'a> Step<'b, 'a> {
    fn of_chain(element: &'b ChainElement<'a>) -> Self {
        match element {
            ChainElement::StaticMemberExpression(m) => Step::Member {
                object: &m.object,
                optional: m.optional,
                name: Some(m.property.span.start),
                element: false,
            },
            ChainElement::ComputedMemberExpression(m) => Step::Member {
                object: &m.object,
                optional: m.optional,
                name: None,
                element: true,
            },
            ChainElement::PrivateFieldExpression(m) => Step::Member {
                object: &m.object,
                optional: m.optional,
                name: None,
                element: false,
            },
            ChainElement::CallExpression(c) => Step::Call {
                callee: &c.callee,
                optional: c.optional,
            },
            ChainElement::TSNonNullExpression(n) => Step::Through(&n.expression),
        }
    }

    fn of(expression: &'b Expression<'a>) -> Option<Self> {
        Some(match expression {
            Expression::StaticMemberExpression(m) => Step::Member {
                object: &m.object,
                optional: m.optional,
                name: Some(m.property.span.start),
                element: false,
            },
            Expression::ComputedMemberExpression(m) => Step::Member {
                object: &m.object,
                optional: m.optional,
                name: None,
                element: true,
            },
            Expression::PrivateFieldExpression(m) => Step::Member {
                object: &m.object,
                optional: m.optional,
                name: None,
                element: false,
            },
            Expression::CallExpression(c) => Step::Call {
                callee: &c.callee,
                optional: c.optional,
            },
            Expression::TSNonNullExpression(n) => Step::Through(&n.expression),
            _ => return None,
        })
    }
}

/// `expression` without the parentheses around it, which are noted, and
/// TypeScript's assertions, which go anyway.
fn unwrap<'b, 'a>(
    mut expression: &'b Expression<'a>,
    parens: &mut Vec<Span>,
) -> &'b Expression<'a> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(p) => {
                parens.push(p.span);
                &p.expression
            }
            Expression::TSNonNullExpression(n) => &n.expression,
            Expression::TSAsExpression(n) => &n.expression,
            Expression::TSSatisfiesExpression(n) => &n.expression,
            _ => return expression,
        };
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
    /// The named members on the spine of an `a?.b` chain, by where their
    /// names start: the chain, in [`Analysis::optional_chains`].
    chain_of: HashMap<u32, usize>,
    /// The `a?.b` chains in parentheses that a chain around them took in.
    absorbed: Vec<Span>,
    /// Where the functions start that the visit is in, innermost last.
    functions: Vec<u32>,
    /// Where the names start that a `typeof` is asked of.
    asked: Vec<u32>,
    /// Where the names start that a `new` makes one of.
    constructed: Vec<u32>,
    /// Where the names start that stand alone right of an `instanceof`.
    instance_types: Vec<u32>,
    /// Every declaration of a single name, in order of appearance.
    declarators: Vec<Declarator>,
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

    fn suspend(&mut self, at: u32, yields: bool) {
        if at < self.offset {
            return;
        }
        let function = self
            .functions
            .last()
            .filter(|start| **start >= self.offset)
            .map(|start| start - self.offset);
        self.analysis.suspends.push(Suspend {
            at: at - self.offset,
            function,
            yields,
        });
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
                    && !self.analysis.locals.iter().any(|l| l == it.name.as_str())
                {
                    self.analysis.locals.push(it.name.to_string());
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
            asked: self.asked.contains(&it.span.start),
            constructed: self.constructed.contains(&it.span.start),
            instance_type: self.instance_types.contains(&it.span.start),
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
                        // A function or a class cannot be given the changed
                        // value. A `const` can once it is declared `let`.
                        if !(flags.is_variable() || flags.is_catch_variable()) {
                            return;
                        }
                        ValueSource::Local {
                            span: self.local(id.span),
                            name: id.name.to_string(),
                            symbol: symbol.index() as u32,
                        }
                    }
                }
            }
            _ => return,
        };
        self.analysis.member_writes.push(source);
    }

    /// Where a variable's first value is read from, when that is somewhere a
    /// changed value can be written back to: `object.name`, `object[key]`
    /// or a name nothing in the script declares.
    fn place_of(&self, init: &Expression<'_>) -> Option<ValueSource> {
        if init.span().start < self.offset {
            return None;
        }
        let end = init.span().end - self.offset;
        let source = match init {
            Expression::StaticMemberExpression(member)
                if !member.optional
                    && !matches!(member.object, Expression::Super(_))
                    && !member
                        .property
                        .name
                        .starts_with(|c: char| c.is_ascii_uppercase()) =>
            {
                ValueSource::Member {
                    object: self.local(member.object.span()),
                    name: member.property.name.to_string(),
                    end,
                }
            }
            Expression::ComputedMemberExpression(member)
                if !member.optional && !matches!(member.object, Expression::Super(_)) =>
            {
                ValueSource::Element {
                    object: self.local(member.object.span()),
                    key: self.local(member.expression.span()),
                    end,
                }
            }
            Expression::Identifier(id) => {
                let reference = id.reference_id.get()?;
                if self.scoping.get_reference(reference).symbol_id().is_some() {
                    return None;
                }
                ValueSource::Free(id.span.start - self.offset)
            }
            _ => return None,
        };
        Some(source)
    }

    /// After the visit: a `const` whose members are written becomes `let`,
    /// and a variable declared once with a value read from a place, and
    /// never assigned again, keeps that place.
    fn hold_written_locals(&mut self) {
        let mut seen: Vec<u32> = Vec::new();
        let mut constants: Vec<Span> = Vec::new();
        for write in &self.analysis.member_writes {
            let ValueSource::Local { symbol, name, .. } = write else {
                continue;
            };
            if seen.contains(symbol) {
                continue;
            }
            seen.push(*symbol);
            let declared: Vec<&Declarator> = self
                .declarators
                .iter()
                .filter(|d| d.symbol.index() as u32 == *symbol)
                .collect();
            for d in &declared {
                if let Some(span) = d.constant
                    && !constants.contains(&span)
                {
                    constants.push(span);
                }
            }
            let [only] = declared[..] else {
                continue;
            };
            let assigned = self
                .scoping
                .get_resolved_references(only.symbol)
                .any(|r| r.is_write());
            if let (false, Some((source, start))) = (assigned, &only.init) {
                self.analysis.places.push(Place {
                    symbol: *symbol,
                    name: name.clone(),
                    binding: Span::new(only.binding_start, *start),
                    source: source.clone(),
                });
            }
        }
        for span in constants {
            self.analysis.edits.push(Edit {
                span,
                text: "let".to_owned(),
            });
        }
    }

    /// Notes `it` as an [`OptionalChain`] and the named members on its
    /// spine as on it: `Window` of `parent?.Window.window` is read off
    /// `parent` only when `parent` is something, and when it is not, the
    /// whole chain is `undefined`. An argument of a call in the chain is
    /// not on the spine.
    fn chain_spine(&mut self, it: &ChainExpression<'_>) {
        let index = self.analysis.optional_chains.len();
        let mut links: Vec<Link> = Vec::new();
        let mut parens: Vec<Span> = Vec::new();
        let mut step = Some(Step::of_chain(&it.expression));
        while let Some(at) = step {
            let (next, optional) = match at {
                Step::Member {
                    object,
                    optional,
                    name,
                    element,
                } => {
                    if let Some(start) = name {
                        self.chain_of.insert(start, index);
                    }
                    if optional {
                        let kind = if element {
                            LinkKind::Element
                        } else {
                            LinkKind::Member
                        };
                        links.push(self.link(object.span(), kind));
                    }
                    (object, optional)
                }
                Step::Call { callee, optional } => {
                    if optional {
                        let this = self.this_of(callee, &mut parens);
                        links.push(self.link(callee.span(), LinkKind::Call(this)));
                    }
                    (callee, optional)
                }
                Step::Through(inner) => (inner, false),
            };
            step = Step::of(next).or_else(|| {
                // `(a?.b)?.c`: where the inner chain stops, so does this
                // one, so the two are one chain.
                let mut around = Vec::new();
                let Expression::ChainExpression(inner) = unwrap(next, &mut around) else {
                    return None;
                };
                if !optional || around.is_empty() {
                    return None;
                }
                parens.extend(around.iter().map(|p| self.local(*p)));
                self.absorbed.push(inner.span);
                Some(Step::of_chain(&inner.expression))
            });
        }
        links.reverse();
        self.analysis.optional_chains.push(OptionalChain {
            span: self.local(it.span),
            links,
            parens,
        });
    }

    /// A `?.` written after `before`.
    fn link(&self, before: Span, kind: LinkKind) -> Link {
        // Spaces and comments may stand between the two.
        let rest = &self.source[before.end as usize..];
        let mut at = 0;
        loop {
            let skipped = rest[at..].len() - rest[at..].trim_start().len();
            at += skipped;
            if rest[at..].starts_with("//") {
                at += rest[at..].find('\n').unwrap_or(rest.len() - at);
            } else if rest[at..].starts_with("/*") {
                at += rest[at..].find("*/").map_or(rest.len() - at, |end| end + 2);
            } else if skipped == 0 {
                break;
            }
        }
        let start = before.end + at as u32;
        Link {
            token: self.local(Span::new(start, start + 2)),
            kind,
        }
    }

    /// What `callee?.()` calls its function on, when that is kept: the
    /// object of a callee that is a member, through parentheses (which are
    /// noted, to go) and TypeScript's assertions.
    fn this_of(&self, callee: &Expression<'_>, parens: &mut Vec<Span>) -> Option<This> {
        let mut around = Vec::new();
        let (object, name_end) = match unwrap(callee, &mut around) {
            Expression::StaticMemberExpression(m) => {
                parens.extend(around.iter().map(|p| self.local(*p)));
                (&m.object, Some(m.property.span.end))
            }
            Expression::ComputedMemberExpression(m) => {
                parens.extend(around.iter().map(|p| self.local(*p)));
                (&m.object, None)
            }
            Expression::PrivateFieldExpression(m) => {
                parens.extend(around.iter().map(|p| self.local(*p)));
                (&m.object, None)
            }
            // `(a?.f)?.()`: the chain in parentheses is taken into this
            // one, parentheses and all (see `chain_spine`).
            Expression::ChainExpression(inner) => match &inner.expression {
                ChainElement::StaticMemberExpression(m) => (&m.object, Some(m.property.span.end)),
                ChainElement::ComputedMemberExpression(m) => (&m.object, None),
                ChainElement::PrivateFieldExpression(m) => (&m.object, None),
                _ => return None,
            },
            _ => return None,
        };
        if let Expression::Super(_) = object {
            return Some(This::Super);
        }
        Some(This::Value {
            object: self.local(object.span()),
            name_end: name_end.map(|end| end - self.offset),
        })
    }

    /// `object.Name`, where it is a value the script reads.
    fn type_member(&mut self, it: &StaticMemberExpression<'_>) {
        let name = it.property.name.as_str();
        if !name.starts_with(|c: char| c.is_ascii_uppercase())
            || it.span.start < self.offset
            || self.not_values.contains(&it.span)
        {
            return;
        }
        let base = match &it.object {
            Expression::Identifier(id) => Some(id.span.start - self.offset),
            _ => None,
        };
        let through = match &it.object {
            Expression::StaticMemberExpression(inner) if !inner.optional => Some((
                inner.property.name.to_string(),
                self.local(inner.object.span()),
            )),
            _ => None,
        };
        self.analysis.type_members.push(TypeMember {
            name: name.to_string(),
            object: self.local(it.object.span()),
            end: it.property.span.end - self.offset,
            base,
            through,
            chain: self.chain_of.get(&it.property.span.start).copied(),
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

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        if it.span.start >= self.offset {
            let constant = (it.kind == VariableDeclarationKind::Const)
                .then(|| self.local(Span::new(it.span.start, it.span.start + 5)));
            for d in &it.declarations {
                let BindingPattern::BindingIdentifier(id) = &d.id else {
                    continue;
                };
                let Some(symbol) = id.symbol_id.get() else {
                    continue;
                };
                let init = d.init.as_ref().and_then(|init| {
                    let source = self.place_of(init)?;
                    Some((source, init.span().start - self.offset))
                });
                self.declarators.push(Declarator {
                    symbol,
                    constant,
                    binding_start: id.span.start - self.offset,
                    init,
                });
            }
        }
        walk::walk_variable_declaration(self, it);
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

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        if it.operator == UnaryOperator::Typeof {
            let mut of = &it.argument;
            while let Expression::ParenthesizedExpression(inner) = of {
                of = &inner.expression;
            }
            if let Expression::Identifier(id) = of {
                self.asked.push(id.span.start);
            }
        }
        walk::walk_unary_expression(self, it);
    }

    fn visit_binary_expression(&mut self, it: &BinaryExpression<'a>) {
        if it.operator == BinaryOperator::Instanceof {
            let mut of = &it.right;
            while let Expression::ParenthesizedExpression(inner) = of {
                of = &inner.expression;
            }
            // `C.ToolTip`: the type through a namespace the file imports.
            if let Expression::StaticMemberExpression(member) = of
                && !member.optional
            {
                of = &member.object;
            }
            if let Expression::Identifier(id) = of {
                self.instance_types.push(id.span.start);
            }
        }
        walk::walk_binary_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        self.not_values.push(it.callee.span());
        let mut base = &it.callee;
        loop {
            base = match base {
                Expression::StaticMemberExpression(member) => &member.object,
                Expression::ComputedMemberExpression(member) => &member.object,
                _ => break,
            };
        }
        if let Expression::Identifier(id) = base {
            self.constructed.push(id.span.start);
        }
        walk::walk_new_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        self.not_values.push(it.tag.span());
        walk::walk_tagged_template_expression(self, it);
    }

    fn visit_chain_expression(&mut self, it: &ChainExpression<'a>) {
        if it.span.start >= self.offset && !self.absorbed.contains(&it.span) {
            self.chain_spine(it);
        }
        walk::walk_chain_expression(self, it);
    }

    fn visit_function(&mut self, it: &oxc_ast::ast::Function<'a>, flags: ScopeFlags) {
        self.functions.push(it.span.start);
        walk::walk_function(self, it, flags);
        self.functions.pop();
    }

    fn visit_arrow_function_expression(&mut self, it: &oxc_ast::ast::ArrowFunctionExpression<'a>) {
        self.functions.push(it.span.start);
        walk::walk_arrow_function_expression(self, it);
        self.functions.pop();
    }

    fn visit_await_expression(&mut self, it: &oxc_ast::ast::AwaitExpression<'a>) {
        self.suspend(it.span.start, false);
        walk::walk_await_expression(self, it);
    }

    fn visit_yield_expression(&mut self, it: &oxc_ast::ast::YieldExpression<'a>) {
        self.suspend(it.span.start, true);
        walk::walk_yield_expression(self, it);
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

/// A property binding written as a block has no `return`: its value is
/// that of the last expression statement it ran, as the completion value of
/// a script is (Qt 6.11.2, by running qml6). The edits that make each such
/// statement of `text` (a block) return its expression: the last statement
/// of a block, of both branches of an `if` and of a label.
pub(crate) fn completion_returns(text: &str) -> Vec<Edit> {
    fn tails(statement: &Statement<'_>, out: &mut Vec<Span>) {
        match statement {
            Statement::ExpressionStatement(it) => out.push(it.expression.span()),
            Statement::BlockStatement(it) => {
                if let Some(last) = it.body.last() {
                    tails(last, out);
                }
            }
            Statement::IfStatement(it) => {
                tails(&it.consequent, out);
                if let Some(alternate) = &it.alternate {
                    tails(alternate, out);
                }
            }
            // `{ k: 1 }` is a block whose statement has a label.
            Statement::LabeledStatement(it) => tails(&it.body, out),
            _ => {}
        }
    }
    let prefix = "function $h() {\n";
    let wrapped = format!("{prefix}{text}\n}}");
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &wrapped, SourceType::ts()).parse_qml_script();
    if !parsed.diagnostics.is_empty() {
        return Vec::new();
    }
    let Some(Statement::FunctionDeclaration(function)) = parsed.program.body.first() else {
        return Vec::new();
    };
    let Some(body) = &function.body else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    if let Some(last) = body.statements.last() {
        tails(last, &mut spans);
    }
    let shift = prefix.len() as u32;
    spans
        .into_iter()
        .flat_map(|span| {
            let (start, end) = (span.start - shift, span.end - shift);
            [
                Edit {
                    span: Span::new(start, start),
                    text: "return (".into(),
                },
                Edit {
                    span: Span::new(end, end),
                    text: ")".into(),
                },
            ]
        })
        .collect()
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
        let text = "function f(a: int, b: string, c: Item): bool { return (a as any)! + <int>b }";
        let analysis = analyze(text, Form::Function, &[]).unwrap();
        // The parameters of types a script's value is one of are made so.
        assert_eq!(
            apply(text, &analysis.edits),
            "function f(a, b, c) { a = Number(a) | 0; b = String(b); return (a) + b }"
        );
        assert_eq!(analysis.function_params, ["a", "b", "c"]);
    }

    #[test]
    fn shorthand_and_arguments_are_noticed() {
        let analysis = analyze("({ width }, arguments[0])", Form::Expression, &[]).unwrap();
        assert!(analysis.uses_arguments);
        assert!(analysis.free[0].shorthand);
    }

    #[test]
    fn a_name_whose_type_is_asked_is_marked() {
        let analysis = analyze(
            "typeof a + typeof (b) + c + typeof d.e",
            Form::Expression,
            &[],
        )
        .unwrap();
        let asked: Vec<bool> = analysis.free.iter().map(|free| free.asked).collect();
        assert_eq!(asked, [true, true, false, false]);
    }

    #[test]
    fn a_type_right_of_instanceof_is_marked_alone_or_through_a_namespace() {
        let analysis = analyze(
            "a instanceof B || c instanceof (D) || E instanceof F.G || h instanceof I.J.K || l instanceof m.n",
            Form::Expression,
            &[],
        )
        .unwrap();
        let typed: Vec<&str> = analysis
            .free
            .iter()
            .filter(|free| free.instance_type)
            .map(|free| free.name.as_str())
            .collect();
        // `I.J.K` is a member of a type, `m.n` of a value; `m` is marked,
        // and resolving it as a type changes nothing, as for `B`.
        assert_eq!(typed, ["B", "D", "F", "m"]);
    }

    #[test]
    fn a_name_that_new_makes_one_of_is_marked() {
        let analysis = analyze(
            "new A(b) + new C.D[e].F() + new (G)()",
            Form::Expression,
            &[],
        )
        .unwrap();
        let made: Vec<String> = analysis
            .free
            .iter()
            .filter(|free| free.constructed)
            .map(|free| free.name.clone())
            .collect();
        assert_eq!(made, ["A", "C"]);
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
        let found: Vec<(&str, &str, bool, Option<&str>)> = analysis
            .type_members
            .iter()
            .map(|m| {
                (
                    &text[m.object.start as usize..m.object.end as usize],
                    &text[m.object.start as usize..m.end as usize],
                    m.base.is_some(),
                    m.chain.map(|c| {
                        &text
                            [m.object.start as usize..analysis.optional_chains[c].span.end as usize]
                    }),
                )
            })
            .collect();
        // A member after `?.` is read too, and knows where its chain ends so
        // the whole rest of it can stop when the object is nothing.
        assert_eq!(
            found,
            [
                ("a", "a.Layout", true, None),
                ("(b.c)", "(b.c).Keys", false, None),
                ("d", "d?.Layout", true, Some("d?.Layout.row")),
            ]
        );
    }

    #[test]
    fn the_links_of_optional_chains_are_noticed() {
        let text = "a?.b.c /* ?. */ ?.[k]?.(1).Keys + (x?.y)?.z + (o.f)?.()";
        let analysis = analyze(text, Form::Expression, &[]).unwrap();
        let cut = |span: Span| &text[span.start as usize..span.end as usize];
        // What the chain up to each `?.` is, the `?.`, and what follows it.
        type Links<'t> = Vec<(&'t str, &'t str, String)>;
        let chains: Vec<(&str, Links, Vec<&str>)> = analysis
            .optional_chains
            .iter()
            .map(|chain| {
                let links = chain
                    .links
                    .iter()
                    .map(|link| {
                        let kind = match &link.kind {
                            LinkKind::Call(Some(This::Value { object, .. })) => {
                                format!("call on {}", cut(*object))
                            }
                            kind => format!("{kind:?}"),
                        };
                        let before = &text[chain.span.start as usize..link.token.start as usize];
                        (before, cut(link.token), kind)
                    })
                    .collect();
                let parens = chain.parens.iter().map(|p| cut(*p)).collect();
                (cut(chain.span), links, parens)
            })
            .collect();
        let call = |on: &str| format!("call on {on}");
        assert_eq!(
            chains,
            [
                (
                    "a?.b.c /* ?. */ ?.[k]?.(1).Keys",
                    vec![
                        ("a", "?.", "Member".to_owned()),
                        ("a?.b.c /* ?. */ ", "?.", "Element".to_owned()),
                        // `[k]` is read off `a?.b.c`, which the call keeps.
                        ("a?.b.c /* ?. */ ?.[k]", "?.", call("a?.b.c")),
                    ],
                    vec![],
                ),
                // The chain in parentheses is taken into the one around it.
                (
                    "(x?.y)?.z",
                    vec![
                        ("(x", "?.", "Member".to_owned()),
                        ("(x?.y)", "?.", "Member".to_owned()),
                    ],
                    vec!["(x?.y)"],
                ),
                ("(o.f)?.()", vec![("(o.f)", "?.", call("o"))], vec!["(o.f)"]),
            ]
        );
        // Each `await` and `yield` knows the function it waits in.
        let text = "async function f() { return p?.q.Keys(await z, async () => await w) }";
        let analysis = analyze(text, Form::Function, &[]).unwrap();
        let suspends: Vec<(u32, Option<u32>, bool)> = analysis
            .suspends
            .iter()
            .map(|s| (s.at, s.function, s.yields))
            .collect();
        let arrow = text.find("async ()").unwrap() as u32;
        assert_eq!(
            suspends,
            [
                (text.find("await z").unwrap() as u32, Some(0), false),
                (text.find("await w").unwrap() as u32, Some(arrow), false),
            ]
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
                ValueSource::Local { span, name, .. } => format!("local {} {name}", at(span)),
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
                "local c c",
                "local v v"
            ]
        );
    }

    #[test]
    fn a_written_constant_is_let_and_keeps_where_it_was_read() {
        let text = "{ const p = a.b, q = c[k], r = d; let s = e.f; s = 1; \
                    const t = g; const u = h.i; var w = 1; \
                    p.x = 1; q.x = 2; r.x = 3; s.x = 4; t.x = 5; w.x = 6 }";
        let analysis = analyze(text, Form::Statement, &[]).unwrap();
        let edited = apply(text, &analysis.edits);
        // `const u` has no member written and stays as it is.
        assert!(edited.starts_with("{ let p = a.b"), "{edited}");
        assert!(edited.contains("let t = g; const u"), "{edited}");
        let at = |span: Span| &text[span.start as usize..span.end as usize];
        let places: Vec<String> = analysis
            .places
            .iter()
            .map(|place| {
                let from = match &place.source {
                    ValueSource::Member { object, name, .. } => format!("{}.{name}", at(*object)),
                    ValueSource::Element { object, key, .. } => {
                        format!("{}[{}]", at(*object), at(*key))
                    }
                    ValueSource::Free(start) => format!("free {}", &text[*start as usize..][..1]),
                    ValueSource::Local { .. } => "local".to_owned(),
                };
                format!("{}|{}|{from}", place.name, at(place.binding))
            })
            .collect();
        // `s` is assigned again and `w` was given no value read from a place.
        assert_eq!(
            places,
            [
                "p|p = |a.b",
                "q|q = |c[k]",
                "r|r = |free d",
                "t|t = |free g"
            ]
        );
    }

    #[test]
    fn function_values_are_noticed() {
        let analysis = analyze("function (m) { m.accepted = true }", Form::Expression, &[]);
        assert!(analysis.unwrap().is_function_value);
    }
}
