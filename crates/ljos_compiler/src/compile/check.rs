//! The checker of a file written in Ljós's own API (docs/DIALECT.md §5).
//!
//! A file that imports `Ljos.Ui` has its scripts held to the strict
//! TypeScript subset, unless it says they are still JavaScript (`pragma
//! Scripts: JavaScript`, which the compat pass and `ljos migrate` write):
//! these are its first rules. Each is an error, said in the file's terms,
//! with the rule's name as its [`Code`].
//!
//! The checker knows a value's type from a literal, a property it reads (its
//! declared type, or its type's), an id, and what `!` and a comparison give.
//! What it cannot type it lets by: a rule that holds of what it can type is
//! not yet one that holds of every script (DIALECT.md, risks).

use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AssignmentTarget,
    BinaryExpression, BindingIdentifier, BlockStatement, CatchClause, ConditionalExpression,
    DoWhileStatement, Expression, ForStatement, Function, IdentifierReference, IfStatement,
    NullLiteral, StaticMemberExpression, SwitchStatement, VariableDeclaration,
    VariableDeclarationKind, WhileStatement,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};
use oxc_syntax::scope::ScopeFlags;
use qml_front::{TypeOracle, names::Target};
use qml_syntax::{GetSpan, Span, ast::QmlDocument, ast::QmlImportSource};

use super::Compiler;
use crate::{Code, Diagnostic};

/// The module whose import makes a file one of Ljós's API.
const MODULE: &str = "Ljos.Ui";

/// Whether `document` is written in Ljós's API: it imports it itself.
pub(crate) fn is_dialect(document: &QmlDocument<'_>) -> bool {
    document.imports.iter().any(|import| {
        matches!(&import.source, QmlImportSource::Module(name) if name.to_string() == MODULE)
    })
}

/// Whether `document` says its scripts are Qt's JavaScript, not yet the
/// subset: `pragma Scripts: JavaScript`.
pub(crate) fn scripts_are_javascript(document: &QmlDocument<'_>) -> bool {
    document
        .pragmas
        .iter()
        .any(|pragma| pragma.name == "Scripts" && pragma.value == Some("JavaScript"))
}

/// What the checker knows a value is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ty {
    Boolean,
    Number,
    String,
    /// A colour: a string names one.
    Color,
}

impl Ty {
    fn word(self) -> &'static str {
        match self {
            Ty::Boolean => "a boolean",
            Ty::Number => "a number",
            Ty::String => "a string",
            Ty::Color => "a color",
        }
    }

    /// The type a property declared as `written` holds, as the checker
    /// knows types. Qt's table names some as C++ does.
    fn of_declared(written: &str) -> Option<Ty> {
        Some(match written {
            "bool" => Ty::Boolean,
            "int" | "real" | "double" | "number" | "qreal" | "float" => Ty::Number,
            "string" | "QString" | "url" | "QUrl" => Ty::String,
            "color" | "QColor" => Ty::Color,
            _ => return None,
        })
    }

    /// Whether a value of type `value` may be written to one of this.
    fn takes(self, value: Ty) -> bool {
        self == value || (self == Ty::Color && value == Ty::String)
    }

    /// The word QML declares it with, for a message: `level` is a real.
    fn declared(written: &str) -> String {
        let article = if written.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {written}")
    }
}

/// What a script of a dialect file breaks, before it is compiled.
pub(super) struct Checker<'c, 't, 'a, 'p, O: TypeOracle> {
    compiler: &'c Compiler<'t, 'a, 'p, O>,
    pub(super) found: Vec<(Code, String, Span)>,
    /// The names the script declares around what is being walked: a
    /// handler's signal arguments, then each function's and block's.
    /// QML's names are behind them (`qml_front::names` leaves the script's
    /// own scopes to its caller).
    locals: Vec<String>,
    /// Where each scope in `locals` starts.
    scopes: Vec<usize>,
}

impl<'c, 't, 'a, 'p, O: TypeOracle> Checker<'c, 't, 'a, 'p, O> {
    /// `outer` are the names the script is given: a handler's arguments.
    pub(super) fn new(compiler: &'c Compiler<'t, 'a, 'p, O>, outer: &[String]) -> Self {
        Self {
            compiler,
            found: Vec::new(),
            locals: outer.to_vec(),
            scopes: Vec::new(),
        }
    }

    /// Walks `inner` with what `declare` collects in scope. A scope holds
    /// every name declared anywhere in it outside a nested function, so a
    /// `let` in one block hides a property in its sibling too: a name the
    /// checker takes for a local it lets by, which is never a false error.
    fn scoped(&mut self, declare: impl FnOnce(&mut Declared), inner: impl FnOnce(&mut Self)) {
        let mut declared = Declared(Vec::new());
        declare(&mut declared);
        self.scopes.push(self.locals.len());
        self.locals.append(&mut declared.0);
        inner(self);
        let start = self.scopes.pop().unwrap_or(0);
        self.locals.truncate(start);
    }

    fn is_local(&self, name: &str) -> bool {
        self.locals.iter().any(|local| local == name)
    }

    fn report(&mut self, code: Code, message: String, span: Span) {
        self.found.push((code, message, span));
    }

    /// The type of what `expression` gives, when the checker knows it, and
    /// for a property read the name and type it was declared with.
    fn ty(&self, expression: &Expression<'_>) -> Option<Ty> {
        match expression {
            Expression::BooleanLiteral(_) => Some(Ty::Boolean),
            Expression::NumericLiteral(_) => Some(Ty::Number),
            Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => Some(Ty::String),
            Expression::ParenthesizedExpression(inner) => self.ty(&inner.expression),
            Expression::UnaryExpression(unary) => match unary.operator {
                UnaryOperator::LogicalNot => Some(Ty::Boolean),
                UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus => Some(Ty::Number),
                UnaryOperator::Typeof => Some(Ty::String),
                _ => None,
            },
            Expression::BinaryExpression(binary) => {
                if binary.operator.is_equality() || binary.operator.is_compare() {
                    Some(Ty::Boolean)
                } else {
                    None
                }
            }
            _ => self
                .property(expression)
                .and_then(|(_, written)| Ty::of_declared(&written)),
        }
    }

    /// The property `expression` reads, `id.name` or an unqualified name,
    /// with the type it is declared as.
    fn property(&self, expression: &Expression<'_>) -> Option<(String, String)> {
        match expression {
            Expression::Identifier(id) => self.named(id),
            Expression::StaticMemberExpression(member) => self.member(member),
            _ => None,
        }
    }

    /// The property an unqualified name is.
    fn named(&self, id: &IdentifierReference<'_>) -> Option<(String, String)> {
        if self.is_local(&id.name) {
            return None;
        }
        let compiler = self.compiler;
        let resolution = compiler.lookup(&id.name, id.span.start)?;
        // A role of a model, or a target's member only the running
        // program knows.
        if resolution.aimed.is_some() || !resolution.roles.is_empty() {
            return None;
        }
        match resolution.target? {
            Target::Property { object, name } => compiler
                .declared_type(object, &name)
                .map(|(_, written)| (name, written)),
            _ => None,
        }
    }

    /// The property `id.name` is.
    fn member(&self, member: &StaticMemberExpression<'_>) -> Option<(String, String)> {
        let compiler = self.compiler;
        let Expression::Identifier(id) = &member.object else {
            return None;
        };
        if self.is_local(&id.name) {
            return None;
        }
        let resolution = compiler.lookup(&id.name, id.span.start)?;
        let Some(Target::Id(object)) = resolution.target else {
            return None;
        };
        let name = member.property.name.to_string();
        compiler
            .declared_type(object, &name)
            .map(|(_, written)| (name, written))
    }

    /// A condition is a boolean: TypeScript's subset has no truthiness.
    fn condition(&mut self, test: &Expression<'_>, what: &str) {
        if let Some(ty) = self.ty(test)
            && ty != Ty::Boolean
        {
            let subject = match self.property(test) {
                Some((name, _)) => format!("`{name}` is {}", ty.word()),
                None => format!("this is {}", ty.word()),
            };
            self.report(
                Code::TsConditionBoolean,
                format!("{what} needs a boolean, and {subject}: compare it"),
                test.span(),
            );
        }
    }
}

/// The names a scope declares: its parameters, `let`, `const`, `var`,
/// functions and `catch` bindings, but not what a nested function declares.
struct Declared(Vec<String>);

impl<'a> Visit<'a> for Declared {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.0.push(it.name.to_string());
    }

    fn visit_function(&mut self, it: &Function<'a>, _: ScopeFlags) {
        // A declaration's name is of the scope it is in; the rest is its own.
        if let Some(id) = &it.id {
            self.visit_binding_identifier(id);
        }
    }

    fn visit_arrow_function_expression(&mut self, _: &ArrowFunctionExpression<'a>) {}
}

impl<'a, O: TypeOracle> Visit<'a> for Checker<'_, '_, '_, '_, O> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        self.scoped(
            |declared| {
                declared.visit_formal_parameters(&it.params);
                if let Some(body) = &it.body {
                    declared.visit_function_body(body);
                }
                // A function expression's own name is in its scope.
                if let Some(id) = &it.id {
                    declared.visit_binding_identifier(id);
                }
            },
            |checker| walk::walk_function(checker, it, flags),
        );
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.scoped(
            |declared| {
                declared.visit_formal_parameters(&it.params);
                declared.visit_arrow_function_body(&it.body);
            },
            |checker| walk::walk_arrow_function_expression(checker, it),
        );
    }

    fn visit_block_statement(&mut self, it: &BlockStatement<'a>) {
        self.scoped(
            |declared| declared.visit_block_statement(it),
            |checker| walk::walk_block_statement(checker, it),
        );
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.scoped(
            |declared| declared.visit_switch_statement(it),
            |checker| walk::walk_switch_statement(checker, it),
        );
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.scoped(
            |declared| declared.visit_catch_clause(it),
            |checker| walk::walk_catch_clause(checker, it),
        );
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        let target = match &it.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => self.named(id),
            AssignmentTarget::StaticMemberExpression(member) => self.member(member),
            _ => None,
        };
        if it.operator == AssignmentOperator::Assign
            && let Some((name, written)) = target
            && let Some(held) = Ty::of_declared(&written)
            && let Some(given) = self.ty(&it.right)
            && !held.takes(given)
        {
            self.report(
                Code::TsAssignType,
                format!(
                    "`{name}` is {}; this gives it {}",
                    Ty::declared(&written),
                    given.word()
                ),
                it.right.span(),
            );
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_null_literal(&mut self, it: &NullLiteral) {
        self.report(
            Code::TsNoNull,
            "`null` is not a value here: a value that may be missing is optional (`?`)".into(),
            it.span,
        );
    }

    fn visit_binary_expression(&mut self, it: &BinaryExpression<'a>) {
        let strict = match it.operator {
            BinaryOperator::Equality => Some("==="),
            BinaryOperator::Inequality => Some("!=="),
            _ => None,
        };
        if let Some(strict) = strict {
            self.report(
                Code::TsLooseEquality,
                format!(
                    "`{}` converts what it compares; write `{strict}`",
                    it.operator.as_str()
                ),
                it.span,
            );
        }
        walk::walk_binary_expression(self, it);
    }

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.condition(&it.test, "`if`");
        walk::walk_if_statement(self, it);
    }

    fn visit_while_statement(&mut self, it: &WhileStatement<'a>) {
        self.condition(&it.test, "`while`");
        walk::walk_while_statement(self, it);
    }

    fn visit_do_while_statement(&mut self, it: &DoWhileStatement<'a>) {
        self.condition(&it.test, "`while`");
        walk::walk_do_while_statement(self, it);
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        if let Some(test) = &it.test {
            self.condition(test, "`for`");
        }
        walk::walk_for_statement(self, it);
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.condition(&it.test, "`?:`");
        walk::walk_conditional_expression(self, it);
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        if it.kind == VariableDeclarationKind::Var {
            self.report(
                Code::TsNoVar,
                "`var` is the function's, wherever it is written; write `let` or `const`".into(),
                Span::new(it.span.start, it.span.start + 3),
            );
        }
        walk::walk_variable_declaration(self, it);
    }
}

impl<O: TypeOracle> Compiler<'_, '_, '_, O> {
    /// Holds a script of a dialect file to the checker's rules.
    /// `outer` are the names the script is given: a handler's arguments.
    pub(super) fn check(
        &mut self,
        outer: &[String],
        walk: impl FnOnce(&mut Checker<'_, '_, '_, '_, O>),
    ) {
        if !self.dialect {
            return;
        }
        let mut checker = Checker::new(self, outer);
        walk(&mut checker);
        let found = std::mem::take(&mut checker.found);
        for (code, message, span) in found {
            let span = self.span(span);
            self.diagnostics
                .push(Diagnostic::error(code, message, span));
        }
    }
}
