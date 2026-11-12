//! What an `is` binds is declared before the statement that holds it, in
//! the block around it (a `switch`'s clauses included: they are one scope a
//! jump enters at any of them). A scope between that declaration and the
//! `is` that declares the name itself would take the `is`'s assignment: a
//! `switch`'s clauses, a `for` head, a `catch` parameter, a function's
//! parameters (where the `is` is in them, or in an arrow's body that is not
//! a block), and the arms of a `match`, which the desugar writes as a
//! function holding a block an arm's names are declared in.
//!
//! A block or a function written by hand declares what an `is` in it binds
//! itself, so the walk goes into one only where the desugar wrote it.

use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::{
    Span,
    js::{
        ArrowFunctionBody, ArrowFunctionExpression, AssignmentOperator, AssignmentTarget,
        BindingPattern, BlockStatement, CatchClause, Class, Expression, ForInStatement,
        ForOfStatement, ForStatement, ForStatementInit, ForStatementLeft, FormalParameters,
        Function, FunctionType, SequenceExpression, Statement, StaticBlock, SwitchStatement,
        VariableDeclaration, VariableDeclarationKind,
    },
};

/// Which scope between it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Between {
    Switch,
    For,
    Catch,
    Params,
    Arm,
}

/// An `is` whose name a scope between declares too.
#[derive(Clone, Copy, Debug)]
pub(super) struct Shadow<'a> {
    pub name: &'a str,
    pub between: Between,
    /// Whether the scope declares it with `var`, which is the function's
    /// rather than the scope's.
    pub var: bool,
    /// The name as the `is` binds it.
    pub span: Span,
    /// Where the `((name = …), true)` the desugar wrote for the `is` starts,
    /// and its `true`: the checker tells it from one written by hand by
    /// whether the desugar wrote these.
    pub written: [u32; 2],
}

struct Frame<'a> {
    between: Between,
    declares: Vec<(&'a str, bool)>,
}

pub(super) struct Shadows<'a, 'w> {
    /// Whether the desugar wrote what is at a position.
    written: &'w dyn Fn(u32) -> bool,
    open: Vec<Frame<'a>>,
    pub found: Vec<Shadow<'a>>,
}

impl<'a, 'w> Shadows<'a, 'w> {
    pub(super) fn new(written: &'w dyn Fn(u32) -> bool) -> Self {
        Self {
            written,
            open: Vec::new(),
            found: Vec::new(),
        }
    }

    fn within(
        &mut self,
        between: Between,
        declares: Vec<(&'a str, bool)>,
        walk: impl FnOnce(&mut Self),
    ) {
        self.open.push(Frame { between, declares });
        walk(self);
        self.open.pop();
    }
}

fn names<'a>(declaration: &VariableDeclaration<'a>, into: &mut Vec<(&'a str, bool)>) {
    let var = declaration.kind == VariableDeclarationKind::Var;
    for declarator in &declaration.declarations {
        pattern(&declarator.id, var, into);
    }
}

fn pattern<'a>(pattern: &BindingPattern<'a>, var: bool, into: &mut Vec<(&'a str, bool)>) {
    for id in pattern.get_binding_identifiers() {
        into.push((id.name.as_str(), var));
    }
}

/// The names `statements` declare in their own scope: `let`, `const`,
/// functions and classes.
fn lexical<'a>(statements: &[Statement<'a>], into: &mut Vec<(&'a str, bool)>) {
    for statement in statements {
        match statement {
            Statement::VariableDeclaration(declaration)
                if declaration.kind != VariableDeclarationKind::Var =>
            {
                names(declaration, into);
            }
            Statement::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    into.push((id.name.as_str(), false));
                }
            }
            Statement::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    into.push((id.name.as_str(), false));
                }
            }
            _ => {}
        }
    }
}

/// The `var`s a function's body declares, in blocks of it too: they are
/// the function's.
#[derive(Default)]
struct Vars<'a>(Vec<(&'a str, bool)>);

impl<'a> Visit<'a> for Vars<'a> {
    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        if it.kind == VariableDeclarationKind::Var {
            names(it, &mut self.0);
        }
        walk::walk_variable_declaration(self, it);
    }

    fn visit_function(&mut self, _: &Function<'a>, _: ScopeFlags) {}

    fn visit_arrow_function_expression(&mut self, _: &ArrowFunctionExpression<'a>) {}

    fn visit_class(&mut self, _: &Class<'a>) {}
}

fn params<'a>(params: &FormalParameters<'a>, into: &mut Vec<(&'a str, bool)>) {
    for param in &params.items {
        pattern(&param.pattern, false, into);
    }
    if let Some(rest) = &params.rest {
        pattern(&rest.rest.argument, false, into);
    }
}

impl<'a> Visit<'a> for Shadows<'a, '_> {
    fn visit_block_statement(&mut self, it: &BlockStatement<'a>) {
        // A `match` arm that binds a name is a block the desugar wrote.
        if (self.written)(it.span.start) {
            let mut declares = Vec::new();
            lexical(&it.body, &mut declares);
            self.within(Between::Arm, declares, |this| {
                walk::walk_block_statement(this, it);
            });
        }
    }

    fn visit_function(&mut self, it: &Function<'a>, _: ScopeFlags) {
        if (self.written)(it.span.start) {
            // A `match` that yields is a generator the desugar wrote.
            if let Some(body) = &it.body {
                let mut declares = Vars::default();
                declares.visit_function_body(body);
                let mut declares = declares.0;
                lexical(&body.statements, &mut declares);
                self.within(Between::Arm, declares, |this| {
                    this.visit_function_body(body);
                });
            }
            return;
        }
        let mut declares = Vec::new();
        params(&it.params, &mut declares);
        // A function expression's name is its own, in it.
        if it.r#type == FunctionType::FunctionExpression
            && let Some(id) = &it.id
        {
            declares.push((id.name.as_str(), false));
        }
        // Its body is a block, which declares what an `is` in it binds.
        self.within(Between::Params, declares, |this| {
            this.visit_formal_parameters(&it.params);
        });
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let body = match &it.body {
            ArrowFunctionBody::FunctionBody(body) => Some(body),
            _ => None,
        };
        if (self.written)(it.span.start) {
            // The function a `match` is lowered to.
            if let Some(body) = body {
                let mut declares = Vars::default();
                declares.visit_function_body(body);
                let mut declares = declares.0;
                lexical(&body.statements, &mut declares);
                self.within(Between::Arm, declares, |this| {
                    this.visit_function_body(body);
                });
            }
            return;
        }
        let mut declares = Vec::new();
        params(&it.params, &mut declares);
        self.within(Between::Params, declares, |this| {
            this.visit_formal_parameters(&it.params);
            if body.is_none() {
                this.visit_expression(it.body.to_expression());
            }
        });
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        // Its body is a block, which declares what an `is` in it binds.
        if let Some(param) = &it.param {
            let mut declares = Vec::new();
            pattern(&param.pattern, false, &mut declares);
            self.within(Between::Catch, declares, |this| {
                this.visit_catch_parameter(param);
            });
        }
    }

    // Ljós has no classes: `ts-forbidden` says so.
    fn visit_static_block(&mut self, _: &StaticBlock<'a>) {}

    fn visit_class(&mut self, _: &Class<'a>) {}

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        // The subject is read before the clauses' scope is entered.
        self.visit_expression(&it.discriminant);
        let mut declares = Vec::new();
        for case in &it.cases {
            lexical(&case.consequent, &mut declares);
            // A `var` in a clause is the function's, but it is a name there
            // too.
            for statement in &case.consequent {
                if let Statement::VariableDeclaration(declaration) = statement
                    && declaration.kind == VariableDeclarationKind::Var
                {
                    names(declaration, &mut declares);
                }
            }
        }
        self.within(Between::Switch, declares, |this| {
            for case in &it.cases {
                this.visit_switch_case(case);
            }
        });
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        let mut declares = Vec::new();
        if let Some(ForStatementInit::VariableDeclaration(declaration)) = &it.init {
            names(declaration, &mut declares);
        }
        self.within(Between::For, declares, |this| {
            walk::walk_for_statement(this, it);
        });
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        let mut declares = Vec::new();
        if let ForStatementLeft::VariableDeclaration(declaration) = &it.left {
            names(declaration, &mut declares);
        }
        self.within(Between::For, declares, |this| {
            walk::walk_for_in_statement(this, it);
        });
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        let mut declares = Vec::new();
        if let ForStatementLeft::VariableDeclaration(declaration) = &it.left {
            names(declaration, &mut declares);
        }
        self.within(Between::For, declares, |this| {
            walk::walk_for_of_statement(this, it);
        });
    }

    // `((x = …), true)`: what an `is` binds. Its assignment reaches the
    // innermost scope that declares the name, so that one is the scope
    // said, once.
    fn visit_sequence_expression(&mut self, it: &SequenceExpression<'a>) {
        if let [first, Expression::BooleanLiteral(last)] = it.expressions.as_slice()
            && last.value
            && let Expression::AssignmentExpression(assignment) = first.without_parentheses()
            && assignment.operator == AssignmentOperator::Assign
            && let AssignmentTarget::AssignmentTargetIdentifier(id) = &assignment.left
        {
            let name = id.name.as_str();
            let inner = self.open.iter().rev().find_map(|frame| {
                frame
                    .declares
                    .iter()
                    .find(|(own, _)| *own == name)
                    .map(|&(_, var)| (frame.between, var))
            });
            if let Some((between, var)) = inner {
                self.found.push(Shadow {
                    name,
                    between,
                    var,
                    span: id.span,
                    written: [it.span.start, last.span.start],
                });
            }
        }
        walk::walk_sequence_expression(self, it);
    }
}

#[cfg(test)]
mod tests {
    use super::{Between, Shadows};
    use oxc_allocator::Allocator;
    use oxc_ast_visit::Visit;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    /// What the walk finds in lowered text, as name, scope and where in
    /// the text the bound name is; `written` says what the desugar wrote.
    fn found(source: &str, written: &dyn Fn(u32) -> bool) -> Vec<(String, Between, u32)> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "the text does not parse");
        let mut shadows = Shadows::new(written);
        for statement in &parsed.program.body {
            shadows.visit_statement(statement);
        }
        shadows
            .found
            .iter()
            .map(|shadow| (shadow.name.to_owned(), shadow.between, shadow.span.start))
            .collect()
    }

    // The checker's report keeps one finding per place, which hid that
    // each open `switch` that declared the name said it again.
    #[test]
    fn nested_switches_that_both_declare_the_name_find_the_is_once() {
        let source = "let x; switch (v) { case 1: let x = 1; \
                      switch (w) { case 2: let x = 2; if (((x = a), true)) {} } }";
        let at = source.find("x = a").unwrap() as u32;
        assert_eq!(
            found(source, &|_| false),
            [("x".to_owned(), Between::Switch, at)]
        );
    }

    #[test]
    fn a_scope_between_that_does_not_declare_the_name_leaves_it_to_one_that_does() {
        let source = "let x; switch (v) { case 1: let x = 1; \
                      for (const y of ys) if (((x = a), true)) {} }";
        let at = source.find("x = a").unwrap() as u32;
        assert_eq!(
            found(source, &|_| false),
            [("x".to_owned(), Between::Switch, at)]
        );
    }

    // Only a block or a function the desugar wrote is walked into: one
    // written by hand declares what an `is` in it binds itself.
    #[test]
    fn a_block_is_walked_into_only_where_the_desugar_wrote_it() {
        let source = "let n; { const n = v; if (((n = a), true)) {} }";
        let at = source.find("n = a").unwrap() as u32;
        assert_eq!(found(source, &|_| false), []);
        assert_eq!(
            found(source, &|_| true),
            [("n".to_owned(), Between::Arm, at)]
        );
    }
}
