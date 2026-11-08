//! The names a script binds itself, and where each is seen.
//!
//! A name a script declares means the same thing in the lowered file as in
//! Qt's, whatever type or member of Ljós's has that name, so the pass
//! neither renames it nor keeps the file for it. Which declaration a read
//! sees is a matter of scope: a `var` is its function's, a `let` its
//! block's, a parameter its function's.

use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, BlockStatement, CatchClause, Class, ClassType,
    Expression, ForInStatement, ForOfStatement, ForStatement, FormalParameters, Function,
    FunctionType, Statement, SwitchStatement, VariableDeclaration,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::Span;
use oxc_syntax::scope::ScopeFlags;

/// A script as the QML file holds it: a binding's expression, a handler's
/// statement, or a method.
#[derive(Clone, Copy)]
pub(crate) enum Node<'n, 'a> {
    Expression(&'n Expression<'a>),
    Statement(&'n Statement<'a>),
    Function(&'n Function<'a>),
}

impl<'a> Node<'_, 'a> {
    pub(crate) fn visit(self, visitor: &mut impl Visit<'a>) {
        match self {
            Node::Expression(expression) => visitor.visit_expression(expression),
            Node::Statement(statement) => visitor.visit_statement(statement),
            Node::Function(function) => visitor.visit_function(function, ScopeFlags::Function),
        }
    }
}

/// Each name the script binds, with the span of the scope it is seen in.
#[derive(Debug, Default)]
pub(crate) struct Bindings {
    bound: Vec<(String, Span)>,
}

impl Bindings {
    pub(crate) fn of(script: Node<'_, '_>) -> Self {
        let mut collect = Collect {
            // What the script itself declares is seen all through it.
            scopes: vec![(Span::new(0, u32::MAX), true)],
            bound: Vec::new(),
        };
        script.visit(&mut collect);
        Bindings {
            bound: collect.bound,
        }
    }

    /// Whether `name`, read at `at`, is one of the script's own.
    pub(crate) fn binds(&self, name: &str, at: u32) -> bool {
        self.bound
            .iter()
            .any(|(bound, scope)| bound == name && scope.start <= at && at < scope.end)
    }
}

struct Collect {
    /// The scopes around the node being visited, innermost last, and
    /// whether each is a function's (where a `var` is seen).
    scopes: Vec<(Span, bool)>,
    bound: Vec<(String, Span)>,
}

impl Collect {
    fn block(&self) -> Span {
        self.scopes.last().map_or(Span::new(0, u32::MAX), |s| s.0)
    }

    fn function(&self) -> Span {
        self.scopes
            .iter()
            .rev()
            .find(|(_, function)| *function)
            .map_or(Span::new(0, u32::MAX), |s| s.0)
    }

    fn bind(&mut self, pattern: &BindingPattern<'_>, scope: Span) {
        for id in pattern.get_binding_identifiers() {
            self.bound.push((id.name.to_string(), scope));
        }
    }

    fn within(&mut self, span: Span, function: bool, walk: impl FnOnce(&mut Self)) {
        self.scopes.push((span, function));
        walk(self);
        self.scopes.pop();
    }
}

impl<'a> Visit<'a> for Collect {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if let Some(id) = &it.id {
            // A declaration's name is its block's; an expression's is seen
            // only inside it.
            let scope = match it.r#type {
                FunctionType::FunctionDeclaration | FunctionType::TSDeclareFunction => self.block(),
                FunctionType::FunctionExpression | FunctionType::TSEmptyBodyFunctionExpression => {
                    it.span
                }
            };
            self.bound.push((id.name.to_string(), scope));
        }
        self.within(it.span, true, |c| walk::walk_function(c, it, flags));
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.within(it.span, true, |c| {
            walk::walk_arrow_function_expression(c, it);
        });
    }

    fn visit_formal_parameters(&mut self, it: &FormalParameters<'a>) {
        let scope = self.function();
        for item in &it.items {
            self.bind(&item.pattern, scope);
        }
        if let Some(rest) = &it.rest {
            self.bind(&rest.rest.argument, scope);
        }
        walk::walk_formal_parameters(self, it);
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        let scope = if it.kind.is_var() {
            self.function()
        } else {
            self.block()
        };
        for declarator in &it.declarations {
            self.bind(&declarator.id, scope);
        }
        walk::walk_variable_declaration(self, it);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        if let Some(id) = &it.id {
            let scope = match it.r#type {
                ClassType::ClassDeclaration => self.block(),
                ClassType::ClassExpression => it.span,
            };
            self.bound.push((id.name.to_string(), scope));
        }
        walk::walk_class(self, it);
    }

    fn visit_block_statement(&mut self, it: &BlockStatement<'a>) {
        self.within(it.span, false, |c| walk::walk_block_statement(c, it));
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.within(it.span, false, |c| walk::walk_for_statement(c, it));
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.within(it.span, false, |c| walk::walk_for_in_statement(c, it));
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.within(it.span, false, |c| walk::walk_for_of_statement(c, it));
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.within(it.span, false, |c| walk::walk_switch_statement(c, it));
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.within(it.span, false, |c| {
            if let Some(param) = &it.param {
                let scope = c.block();
                c.bind(&param.pattern, scope);
            }
            walk::walk_catch_clause(c, it);
        });
    }
}
