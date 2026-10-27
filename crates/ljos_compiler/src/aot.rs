//! A file's functions as the tree compiling it ahead of time reads
//! (`ljos_ir::aot`, ARCHITECTURE.md §5.4).
//!
//! The runtime never parses JavaScript, and neither does the generator:
//! this is where the module's text becomes a tree, through the same
//! unwrapping of each function tier 1 does ([`shape`]). What the tree has
//! no node for (a loop, a closure, an array) is refused here, with the
//! construct named; the generator refuses what it cannot type.

use ljos_ir::CompiledFile;
use ljos_ir::aot::{BinaryOp, Body, Code, Expr, Module, Refused, Stmt, UnaryOp};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, AssignmentTarget, BindingPattern, Expression, SimpleAssignmentTarget, Statement,
    TemplateLiteral, VariableDeclarationKind,
};
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::operator::{
    AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
};

use crate::tier1::{self, Reason};

fn refused(kind: &str, detail: impl Into<String>) -> Refused {
    Refused::new(kind, detail)
}

impl From<Reason> for Refused {
    fn from(reason: Reason) -> Self {
        Refused::new(reason.kind, reason.detail)
    }
}

/// Every expression of the file as a tree, in [`ljos_ir::ExprId`] order.
pub fn lower(file: &CompiledFile) -> Module {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &file.js.source, SourceType::mjs()).parse();
    if let Some(error) = parsed.diagnostics.first() {
        let why = refused("the module does not parse", error.message.to_string());
        return Module {
            functions: vec![Err(why); file.exprs.len()],
            context: None,
        };
    }
    let module = tier1::Module::of(&parsed.program);
    let functions = file
        .exprs
        .iter()
        .map(|binding| {
            let function = file
                .js
                .functions
                .get(binding.js_fn.index())
                .and_then(|f| module.functions.get(f.export.as_str()))
                .ok_or_else(|| refused("function shape", "not in the module"))?;
            let shape = tier1::shape(function, binding)?;
            if shape.own.is_some() {
                return Err(refused("function value", "its own name"));
            }
            let code = match shape.body {
                tier1::Body::Statements(list) => Code::Statements(statements(list, true)?),
                tier1::Body::Expression(e) => Code::Expression(expr(e)?),
            };
            Ok(Body {
                formals: shape.formals.iter().map(|f| f.to_string()).collect(),
                code,
            })
        })
        .collect();
    Module {
        functions,
        context: module.context,
    }
}

fn statements(list: &[Statement<'_>], outermost: bool) -> Result<Vec<Stmt>, Refused> {
    let mut out = Vec::with_capacity(list.len());
    for s in list {
        if let Some(s) = statement(s, outermost)? {
            out.push(s);
        }
    }
    Ok(out)
}

fn block(s: &Statement<'_>) -> Result<Vec<Stmt>, Refused> {
    match s {
        Statement::BlockStatement(b) => statements(&b.body, false),
        other => Ok(statement(other, false)?.into_iter().collect()),
    }
}

fn statement(s: &Statement<'_>, outermost: bool) -> Result<Option<Stmt>, Refused> {
    let what = match s {
        Statement::EmptyStatement(_) => return Ok(None),
        Statement::ExpressionStatement(it) => return Ok(Some(Stmt::Expr(expr(&it.expression)?))),
        Statement::ReturnStatement(it) => {
            return Ok(Some(Stmt::Return(
                it.argument.as_ref().map(expr).transpose()?,
            )));
        }
        Statement::BlockStatement(it) => {
            return Ok(Some(Stmt::Block(statements(&it.body, false)?)));
        }
        Statement::IfStatement(it) => {
            return Ok(Some(Stmt::If {
                test: expr(&it.test)?,
                then: block(&it.consequent)?,
                otherwise: match &it.alternate {
                    Some(a) => block(a)?,
                    None => Vec::new(),
                },
            }));
        }
        Statement::SwitchStatement(it) => {
            let mut cases = Vec::with_capacity(it.cases.len());
            for case in &it.cases {
                let test = case.test.as_ref().map(expr).transpose()?;
                cases.push((test, statements(&case.consequent, false)?));
            }
            return Ok(Some(Stmt::Switch {
                on: expr(&it.discriminant)?,
                cases,
            }));
        }
        Statement::BreakStatement(it) => {
            if it.label.is_some() {
                return Err(refused("statement", "labelled break"));
            }
            return Ok(Some(Stmt::Break));
        }
        Statement::VariableDeclaration(it) => {
            let constant = match it.kind {
                // `var` belongs to the whole function: only at the top of
                // one is it the same as `let` there.
                VariableDeclarationKind::Var if !outermost => {
                    return Err(refused("statement", "var in a block"));
                }
                VariableDeclarationKind::Var | VariableDeclarationKind::Let => false,
                VariableDeclarationKind::Const => true,
                _ => return Err(refused("statement", "using")),
            };
            let [declarator] = &it.declarations[..] else {
                return Err(refused("statement", "more than one name declared at once"));
            };
            let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                return Err(refused("destructuring", ""));
            };
            return Ok(Some(Stmt::Let {
                name: id.name.to_string(),
                constant,
                init: declarator.init.as_ref().map(expr).transpose()?,
            }));
        }
        Statement::ForStatement(_)
        | Statement::ForInStatement(_)
        | Statement::ForOfStatement(_)
        | Statement::WhileStatement(_)
        | Statement::DoWhileStatement(_) => return Err(refused("loop", "")),
        Statement::TryStatement(_) => "try",
        Statement::ThrowStatement(_) => "throw",
        Statement::ContinueStatement(_) => "continue",
        Statement::LabeledStatement(_) => "label",
        Statement::FunctionDeclaration(_) => "function declaration",
        Statement::ClassDeclaration(_) => "class",
        Statement::DebuggerStatement(_) => "debugger",
        Statement::WithStatement(_) => "with",
        _ => "other",
    };
    Err(refused("statement", what))
}

fn binary(op: BinaryOperator) -> Result<BinaryOp, Refused> {
    Ok(match op {
        BinaryOperator::Addition => BinaryOp::Add,
        BinaryOperator::Subtraction => BinaryOp::Sub,
        BinaryOperator::Multiplication => BinaryOp::Mul,
        BinaryOperator::Division => BinaryOp::Div,
        BinaryOperator::Remainder => BinaryOp::Rem,
        BinaryOperator::Exponential => BinaryOp::Exp,
        BinaryOperator::Equality => BinaryOp::Eq,
        BinaryOperator::Inequality => BinaryOp::Ne,
        BinaryOperator::StrictEquality => BinaryOp::StrictEq,
        BinaryOperator::StrictInequality => BinaryOp::StrictNe,
        BinaryOperator::LessThan => BinaryOp::Lt,
        BinaryOperator::LessEqualThan => BinaryOp::Le,
        BinaryOperator::GreaterThan => BinaryOp::Gt,
        BinaryOperator::GreaterEqualThan => BinaryOp::Ge,
        BinaryOperator::BitwiseAnd => BinaryOp::BitAnd,
        BinaryOperator::BitwiseOR => BinaryOp::BitOr,
        BinaryOperator::BitwiseXOR => BinaryOp::BitXor,
        BinaryOperator::ShiftLeft => BinaryOp::Shl,
        BinaryOperator::ShiftRight => BinaryOp::Shr,
        BinaryOperator::ShiftRightZeroFill => BinaryOp::UShr,
        BinaryOperator::In => return Err(refused("operator", "in")),
        BinaryOperator::Instanceof => return Err(refused("operator", "instanceof")),
    })
}

fn boxed(e: &Expression<'_>) -> Result<Box<Expr>, Refused> {
    expr(e).map(Box::new)
}

fn expr(e: &Expression<'_>) -> Result<Expr, Refused> {
    Ok(match e {
        Expression::ParenthesizedExpression(it) => expr(&it.expression)?,
        Expression::BooleanLiteral(it) => Expr::Bool(it.value),
        Expression::NullLiteral(_) => Expr::Null,
        Expression::NumericLiteral(it) => Expr::Number(it.value),
        Expression::StringLiteral(it) => {
            if it.lone_surrogates {
                return Err(refused("literal", "a string that is not Unicode"));
            }
            Expr::String(it.value.to_string())
        }
        Expression::TemplateLiteral(it) => template(it)?,
        Expression::Identifier(it) => match it.name.as_str() {
            "undefined" => Expr::Undefined,
            "NaN" => Expr::Number(f64::NAN),
            "Infinity" => Expr::Number(f64::INFINITY),
            name => Expr::Name(name.to_string()),
        },
        Expression::ThisExpression(_) => return Err(refused("this", "")),
        Expression::StaticMemberExpression(it) => {
            if it.optional {
                return Err(refused("optional chaining", ""));
            }
            Expr::Member(boxed(&it.object)?, it.property.name.to_string())
        }
        Expression::ComputedMemberExpression(_) => return Err(refused("element access", "")),
        Expression::ChainExpression(_) => return Err(refused("optional chaining", "")),
        Expression::UnaryExpression(it) => {
            let op = match it.operator {
                UnaryOperator::Void => return Err(refused("operator", "void")),
                UnaryOperator::Delete => return Err(refused("operator", "delete")),
                UnaryOperator::UnaryNegation => UnaryOp::Neg,
                UnaryOperator::UnaryPlus => UnaryOp::Plus,
                UnaryOperator::LogicalNot => UnaryOp::Not,
                UnaryOperator::BitwiseNot => UnaryOp::BitNot,
                UnaryOperator::Typeof => UnaryOp::Typeof,
            };
            Expr::Unary(op, boxed(&it.argument)?)
        }
        Expression::BinaryExpression(it) => {
            Expr::Binary(binary(it.operator)?, boxed(&it.left)?, boxed(&it.right)?)
        }
        Expression::LogicalExpression(it) => {
            let and = match it.operator {
                LogicalOperator::And => true,
                LogicalOperator::Or => false,
                LogicalOperator::Coalesce => return Err(refused("operator", "??")),
            };
            Expr::Logical {
                and,
                a: boxed(&it.left)?,
                b: boxed(&it.right)?,
            }
        }
        Expression::ConditionalExpression(it) => Expr::Conditional {
            test: boxed(&it.test)?,
            then: boxed(&it.consequent)?,
            otherwise: boxed(&it.alternate)?,
        },
        Expression::CallExpression(it) => {
            if it.optional {
                return Err(refused("optional chaining", ""));
            }
            let mut args = Vec::with_capacity(it.arguments.len());
            for a in &it.arguments {
                if let Argument::SpreadElement(_) = a {
                    return Err(refused("spread", ""));
                }
                let a = a.as_expression().ok_or_else(|| refused("spread", ""))?;
                args.push(expr(a)?);
            }
            Expr::Call {
                callee: boxed(&it.callee)?,
                args,
            }
        }
        Expression::AssignmentExpression(it) => {
            let op = match it.operator {
                AssignmentOperator::Assign => None,
                AssignmentOperator::Addition => Some(BinaryOp::Add),
                AssignmentOperator::Subtraction => Some(BinaryOp::Sub),
                AssignmentOperator::Multiplication => Some(BinaryOp::Mul),
                AssignmentOperator::Division => Some(BinaryOp::Div),
                AssignmentOperator::Remainder => Some(BinaryOp::Rem),
                AssignmentOperator::Exponential => Some(BinaryOp::Exp),
                AssignmentOperator::ShiftLeft => Some(BinaryOp::Shl),
                AssignmentOperator::ShiftRight => Some(BinaryOp::Shr),
                AssignmentOperator::ShiftRightZeroFill => Some(BinaryOp::UShr),
                AssignmentOperator::BitwiseOR => Some(BinaryOp::BitOr),
                AssignmentOperator::BitwiseXOR => Some(BinaryOp::BitXor),
                AssignmentOperator::BitwiseAnd => Some(BinaryOp::BitAnd),
                _ => return Err(refused("operator", "a logical assignment")),
            };
            let target = match &it.left {
                AssignmentTarget::AssignmentTargetIdentifier(id) => Expr::Name(id.name.to_string()),
                AssignmentTarget::StaticMemberExpression(m) => {
                    Expr::Member(boxed(&m.object)?, m.property.name.to_string())
                }
                AssignmentTarget::ComputedMemberExpression(_) => {
                    return Err(refused("element access", ""));
                }
                _ => return Err(refused("destructuring", "")),
            };
            Expr::Assign {
                op,
                target: Box::new(target),
                value: boxed(&it.right)?,
            }
        }
        Expression::UpdateExpression(it) => {
            let target = match &it.argument {
                SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                    Expr::Name(id.name.to_string())
                }
                SimpleAssignmentTarget::StaticMemberExpression(m) => {
                    Expr::Member(boxed(&m.object)?, m.property.name.to_string())
                }
                SimpleAssignmentTarget::ComputedMemberExpression(_) => {
                    return Err(refused("element access", ""));
                }
                _ => return Err(refused("assignment", "to an expression")),
            };
            Expr::Update {
                increment: it.operator == UpdateOperator::Increment,
                prefix: it.prefix,
                target: Box::new(target),
            }
        }
        Expression::SequenceExpression(_) => return Err(refused("operator", "comma")),
        Expression::ArrayExpression(_) => return Err(refused("array", "")),
        Expression::ObjectExpression(_) => return Err(refused("object literal", "")),
        Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
            return Err(refused("closure", ""));
        }
        Expression::NewExpression(_) => return Err(refused("new", "")),
        Expression::RegExpLiteral(_) => return Err(refused("literal", "a regular expression")),
        Expression::BigIntLiteral(_) => return Err(refused("literal", "a BigInt")),
        Expression::TaggedTemplateExpression(_) => {
            return Err(refused("literal", "a tagged template"));
        }
        _ => return Err(refused("expression", "other")),
    })
}

/// `` `a${x}b` `` is `"a" + x + "b"`: for the strings, numbers and booleans
/// the generator lets through, the two convert alike.
fn template(it: &TemplateLiteral<'_>) -> Result<Expr, Refused> {
    let text = |i: usize| -> Result<Expr, Refused> {
        let cooked = it.quasis[i]
            .value
            .cooked
            .as_ref()
            .ok_or_else(|| refused("literal", "a template with an invalid escape"))?;
        Ok(Expr::String(cooked.to_string()))
    };
    let mut out = text(0)?;
    for (i, e) in it.expressions.iter().enumerate() {
        out = Expr::Binary(BinaryOp::Add, Box::new(out), Box::new(expr(e)?));
        let next = text(i + 1)?;
        if next != Expr::String(String::new()) {
            out = Expr::Binary(BinaryOp::Add, Box::new(out), Box::new(next));
        }
    }
    Ok(out)
}

/// The types of a runtime as the generator asks for them: through the
/// oracle the file was compiled against.
struct OracleTypes<'o, O>(&'o O);

impl<O: qml_front::TypeOracle> ljos_aot::Types for OracleTypes<'_, O> {
    fn property(&self, ty: &ljos_ir::TypeRef, name: &str) -> Option<ljos_aot::PropertyType> {
        let ljos_ir::TypeRef::Native { module, name: ty } = ty else {
            return None;
        };
        let ty = self.0.module_type(module, ty)?;
        let property = self.0.property(ty, name)?;
        Some(ljos_aot::PropertyType {
            // A property whose type the oracle does not name is refused by
            // the generator as untyped.
            ty: self
                .0
                .property_type(ty, name)
                .unwrap_or_else(|| "unnamed".to_string()),
            readonly: property.is_readonly,
        })
    }
}

/// The file's bindings and handlers as Rust, for the runtime at `rt`
/// (`::ljos::aot`), typed against `oracle`.
pub fn generate<O: qml_front::TypeOracle>(
    file: &CompiledFile,
    oracle: &O,
    rt: &str,
) -> ljos_aot::Generated {
    let module = lower(file);
    ljos_aot::generate(
        file,
        &module,
        ljos_ir::aot::content_key(file),
        &OracleTypes(oracle),
        rt,
    )
}
