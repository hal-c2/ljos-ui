//! What an `is` binds is declared before the statement that holds it, in
//! the block around it (a `switch`'s clauses included: they are one scope a
//! jump enters at any of them). A scope between that declaration and the
//! `is` that declares the name itself would take the `is`'s assignment, in
//! its dead zone where it has not run yet: a `switch`'s clauses and a `for`
//! head. A block or a function declares what an `is` in it binds itself,
//! so the walk does not go into one.

use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::{
    Span,
    js::{
        AssignmentOperator, AssignmentTarget, BlockStatement, Class, Expression, ForInStatement,
        ForOfStatement, ForStatement, ForStatementInit, ForStatementLeft, Function, FunctionBody,
        SequenceExpression, Statement, StaticBlock, SwitchStatement, VariableDeclaration,
        VariableDeclarationKind,
    },
};

/// Which scope between it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Between {
    Switch,
    For,
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
    binds: Vec<(&'a str, Span, [u32; 2])>,
}

#[derive(Default)]
pub(super) struct Shadows<'a> {
    open: Vec<Frame<'a>>,
    pub found: Vec<Shadow<'a>>,
}

impl<'a> Shadows<'a> {
    fn open(&mut self, between: Between, declares: Vec<(&'a str, bool)>) {
        self.open.push(Frame {
            between,
            declares,
            binds: Vec::new(),
        });
    }

    fn close(&mut self) {
        let Some(frame) = self.open.pop() else {
            return;
        };
        for (name, span, written) in frame.binds {
            if let Some(&(_, var)) = frame.declares.iter().find(|(own, _)| *own == name) {
                self.found.push(Shadow {
                    name,
                    between: frame.between,
                    var,
                    span,
                    written,
                });
            }
        }
    }
}

fn names<'a>(declaration: &VariableDeclaration<'a>, into: &mut Vec<(&'a str, bool)>) {
    let var = declaration.kind == VariableDeclarationKind::Var;
    for declarator in &declaration.declarations {
        for id in declarator.id.get_binding_identifiers() {
            into.push((id.name.as_str(), var));
        }
    }
}

impl<'a> Visit<'a> for Shadows<'a> {
    fn visit_block_statement(&mut self, _: &BlockStatement<'a>) {}

    fn visit_function(&mut self, _: &Function<'a>, _: ScopeFlags) {}

    fn visit_function_body(&mut self, _: &FunctionBody<'a>) {}

    fn visit_static_block(&mut self, _: &StaticBlock<'a>) {}

    fn visit_class(&mut self, _: &Class<'a>) {}

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        // The subject is read before the clauses' scope is entered.
        self.visit_expression(&it.discriminant);
        let mut declares = Vec::new();
        for case in &it.cases {
            for statement in &case.consequent {
                match statement {
                    Statement::VariableDeclaration(declaration) => {
                        names(declaration, &mut declares);
                    }
                    Statement::FunctionDeclaration(function) => {
                        if let Some(id) = &function.id {
                            declares.push((id.name.as_str(), false));
                        }
                    }
                    Statement::ClassDeclaration(class) => {
                        if let Some(id) = &class.id {
                            declares.push((id.name.as_str(), false));
                        }
                    }
                    _ => {}
                }
            }
        }
        self.open(Between::Switch, declares);
        for case in &it.cases {
            self.visit_switch_case(case);
        }
        self.close();
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        let mut declares = Vec::new();
        if let Some(ForStatementInit::VariableDeclaration(declaration)) = &it.init {
            names(declaration, &mut declares);
        }
        self.open(Between::For, declares);
        walk::walk_for_statement(self, it);
        self.close();
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        let mut declares = Vec::new();
        if let ForStatementLeft::VariableDeclaration(declaration) = &it.left {
            names(declaration, &mut declares);
        }
        self.open(Between::For, declares);
        walk::walk_for_in_statement(self, it);
        self.close();
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        let mut declares = Vec::new();
        if let ForStatementLeft::VariableDeclaration(declaration) = &it.left {
            names(declaration, &mut declares);
        }
        self.open(Between::For, declares);
        walk::walk_for_of_statement(self, it);
        self.close();
    }

    // `((x = …), true)`: what an `is` binds.
    fn visit_sequence_expression(&mut self, it: &SequenceExpression<'a>) {
        if !self.open.is_empty()
            && let [first, Expression::BooleanLiteral(last)] = it.expressions.as_slice()
            && last.value
            && let Expression::AssignmentExpression(assignment) = first.without_parentheses()
            && assignment.operator == AssignmentOperator::Assign
            && let AssignmentTarget::AssignmentTargetIdentifier(id) = &assignment.left
        {
            let bind = (id.name.as_str(), id.span, [it.span.start, last.span.start]);
            for frame in &mut self.open {
                frame.binds.push(bind);
            }
        }
        walk::walk_sequence_expression(self, it);
    }
}
