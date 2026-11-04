//! The checker's half that reads scripts: the type of each expression,
//! what statements bind and narrow, functions, arrows and annotations.

use qml_front::{TypeOracle, names::Target};
use qml_syntax::{
    GetSpan, Span,
    js::{
        Argument, ArrayExpressionElement, ArrowFunctionBody, AssignmentOperator, AssignmentTarget,
        BinaryOperator, BindingPattern, CallExpression, ChainElement, Expression, FormalParameters,
        Function, IdentifierReference, LogicalOperator, ObjectPropertyKind, PropertyKey,
        SimpleAssignmentTarget, Statement, TSLiteral, TSSignature, TSType, TSTypeAnnotation,
        TSTypeName, TSTypeOperatorOperator, UnaryOperator, VariableDeclaration,
        VariableDeclarationKind,
    },
};

use super::ty::{Field, Global, Kind, Method, Param, Sig, Ty};
use super::{Binding, Checker, Fact, FnCtx, Obj, builtins, primitive};
use crate::Code;

/// What a function's body is: statements, or an arrow's expression.
#[derive(Clone, Copy)]
enum Body<'b, 'a> {
    Statements(&'b [Statement<'a>]),
    Expression(&'b Expression<'a>),
    None,
}

/// A place an assignment writes: its name, type, the words for its type,
/// and the path a fact about it is keyed by.
struct Place<'a> {
    name: String,
    ty: Ty,
    word: String,
    key: Option<(&'a str, Option<u32>)>,
}

impl<'a, O: TypeOracle> Checker<'_, '_, 'a, '_, O> {
    // Functions.

    /// The type of a function from its annotations alone, for a call before
    /// its body is walked.
    pub(super) fn signature(
        &mut self,
        params: &FormalParameters<'a>,
        ret: Option<&TSTypeAnnotation<'a>>,
    ) -> Ty {
        let mut list = Vec::with_capacity(params.items.len());
        for p in &params.items {
            let ty = match &p.type_annotation {
                Some(annotation) => self.annot(&annotation.type_annotation),
                None => Ty::ERROR,
            };
            let optional = p.optional || p.initializer.is_some();
            let ty = if p.optional {
                self.arena.or(ty, Ty::UNDEFINED)
            } else {
                ty
            };
            list.push(Param {
                name: pattern_name(&p.pattern).into(),
                ty,
                optional,
                written: None,
            });
        }
        let rest = params
            .rest
            .as_ref()
            .map(|rest| self.rest_elem(rest.type_annotation.as_deref()));
        // DECISION: a function with no return type written gives what the
        // checker lets by to a call made before its body is read.
        let ret = match ret {
            Some(annotation) => self.annot(&annotation.type_annotation),
            None => Ty::ERROR,
        };
        self.arena.function(Sig {
            params: list.into_boxed_slice(),
            rest,
            ret,
        })
    }

    fn rest_elem(&mut self, annotation: Option<&TSTypeAnnotation<'a>>) -> Ty {
        let Some(annotation) = annotation else {
            return Ty::ERROR;
        };
        let ty = self.annot(&annotation.type_annotation);
        match *self.arena.kind(ty) {
            Kind::Array { elem, .. } => elem,
            _ => Ty::ERROR,
        }
    }

    /// A function a QML object declares, or a function expression.
    pub(super) fn function(&mut self, function: &Function<'a>, want: Option<Ty>) -> Ty {
        let body = match &function.body {
            Some(body) => Body::Statements(&body.statements),
            None => Body::None,
        };
        self.callable(
            &function.params,
            function.return_type.as_deref(),
            body,
            want,
        )
    }

    /// Walks a function: its parameters typed by their annotations, else by
    /// the function `want` is; its returns checked against its return
    /// type, written or wanted. Gives its type.
    fn callable(
        &mut self,
        params: &FormalParameters<'a>,
        ret: Option<&TSTypeAnnotation<'a>>,
        body: Body<'_, 'a>,
        want: Option<Ty>,
    ) -> Ty {
        let want = want.filter(|want| matches!(self.arena.kind(*want), Kind::Function(_)));
        let mut list = Vec::with_capacity(params.items.len());
        for (at, p) in params.items.iter().enumerate() {
            let ty = match &p.type_annotation {
                Some(annotation) => self.annot(&annotation.type_annotation),
                None => want
                    .and_then(|want| self.sig_param(want, at))
                    .map_or(Ty::ERROR, |(ty, _)| ty),
            };
            let ty = if p.optional {
                self.arena.or(ty, Ty::UNDEFINED)
            } else {
                ty
            };
            list.push(Param {
                name: pattern_name(&p.pattern).into(),
                ty,
                optional: p.optional || p.initializer.is_some(),
                written: None,
            });
        }
        let rest = params
            .rest
            .as_ref()
            .map(|rest| self.rest_elem(rest.type_annotation.as_deref()));
        let written = ret.map(|annotation| self.annot(&annotation.type_annotation));
        let wanted = want.map(|want| self.sig_ret(want));
        let declared = written.or(wanted.filter(|ret| *ret != Ty::VOID && *ret != Ty::ERROR));
        let mut inferred = Ty::VOID;
        self.body(|checker| {
            for (p, param) in params.items.iter().zip(&list) {
                let inner = match &p.initializer {
                    Some(initializer) => {
                        let given = checker.expr(initializer, Some(param.ty));
                        if param.ty == Ty::ERROR {
                            checker.arena.widen(given)
                        } else {
                            param.ty
                        }
                    }
                    None => param.ty,
                };
                checker.bind_pattern(&p.pattern, inner, Binding::Param);
            }
            if let (Some(rest), Some(elem)) = (&params.rest, rest) {
                let all = checker.arena.array(elem, true);
                checker.bind_pattern(&rest.rest.argument, all, Binding::Param);
            }
            checker.fns.push(FnCtx {
                ret: declared,
                returns: Vec::new(),
                target: None,
            });
            match body {
                Body::Expression(expression) => {
                    let given = checker.expr(expression, declared);
                    if let Some(declared) = declared {
                        checker.check_return(declared, given, expression.span());
                    }
                    inferred = checker.arena.widen(given);
                }
                Body::Statements(statements) => {
                    let falls = checker.stmts(statements);
                    let ctx = checker.fns.last().expect("pushed above");
                    if !ctx.returns.is_empty() {
                        let mut returns = ctx.returns.clone();
                        if falls {
                            returns.push(Ty::UNDEFINED);
                        }
                        let all = checker.arena.union(returns);
                        inferred = checker.arena.widen(all);
                    }
                }
                Body::None => {}
            }
            checker.fns.pop();
        });
        self.arena.function(Sig {
            params: list.into_boxed_slice(),
            rest,
            ret: written.unwrap_or(inferred),
        })
    }

    fn sig_param(&self, f: Ty, at: usize) -> Option<(Ty, bool)> {
        match self.arena.kind(f) {
            Kind::Function(sig) => sig
                .params
                .get(at)
                .map(|p| (p.ty, p.optional))
                .or(sig.rest.map(|rest| (rest, true))),
            _ => None,
        }
    }

    fn sig_ret(&self, f: Ty) -> Ty {
        match self.arena.kind(f) {
            Kind::Function(sig) => sig.ret,
            _ => Ty::ERROR,
        }
    }

    fn check_return(&mut self, declared: Ty, given: Ty, span: Span) {
        if self.takes(declared, given) {
            return;
        }
        let target = self.fns.last().and_then(|ctx| ctx.target.clone());
        let given = self.word(given);
        let message = match target {
            Some((name, word)) => format!("`{name}` is {word}; this gives it {given}"),
            None => format!(
                "this function returns {}; this gives it {given}",
                self.word(declared)
            ),
        };
        self.report(Code::TsAssignType, message, span);
    }

    // Names.

    fn name(&mut self, id: &IdentifierReference<'a>) -> Ty {
        let name = id.name.as_str();
        if let Some(local) = self.local(name) {
            return match local.binding {
                Binding::Dead | Binding::Type => Ty::ERROR,
                _ => self.fact(name, Some(local.id)).unwrap_or(local.ty),
            };
        }
        match name {
            "undefined" => return Ty::UNDEFINED,
            "NaN" | "Infinity" => return Ty::NUMBER,
            _ => {}
        }
        // DECISION: the globals TYPES.md lists are these, whatever QML's
        // scope has of the name.
        if let Some(global) = builtins::global(name) {
            return self.arena.intern(Kind::Global(global));
        }
        let Some(resolution) = self.c.lookup(name, id.span.start) else {
            return Ty::ERROR;
        };
        // A role of a model, or a target's member only the running program
        // knows.
        if resolution.aimed.is_some() || !resolution.roles.is_empty() {
            return Ty::ERROR;
        }
        match resolution.target {
            Some(Target::Id(index)) => {
                let o = self.obj(Obj::Instance(index));
                self.arena.intern(Kind::Object(o))
            }
            Some(Target::Property {
                object,
                name: property,
            }) => {
                let base = self
                    .instance_member(object, &property)
                    .map_or(Ty::ERROR, |own| own.ty);
                self.fact(name, None).unwrap_or(base)
            }
            Some(Target::Type(found)) => {
                if self.c.types.is_singleton(&found.kind) {
                    self.object_ty(&found.kind)
                } else {
                    let o = self.of_kind(&found.kind);
                    self.arena.intern(Kind::Type(o))
                }
            }
            Some(Target::Namespace(_)) => self.arena.intern(Kind::Namespace(name.into())),
            _ => Ty::ERROR,
        }
    }

    // Members.

    /// What `name` is on a value of type `receiver`.
    pub(super) fn member(&mut self, receiver: Ty, name: &str) -> Ty {
        let parts = self.arena.members(receiver);
        let mut found = Vec::with_capacity(parts.len());
        for part in parts {
            // DECISION: reading a member of what may be undefined is not
            // yet reported: its other parts give the type.
            if part == Ty::UNDEFINED || part == Ty::VOID || part == Ty::NEVER {
                continue;
            }
            found.push(self.member_one(part, name).unwrap_or(Ty::ERROR));
        }
        if found.is_empty() {
            return Ty::ERROR;
        }
        self.arena.union(found)
    }

    fn member_one(&mut self, part: Ty, name: &str) -> Option<Ty> {
        use builtins::Primitive as P;
        match self.arena.kind(part).clone() {
            Kind::Object(o) => self.object_member(o, name).map(|own| own.ty),
            Kind::Type(o) => self.type_member(o, name),
            Kind::Namespace(namespace) => {
                let found = self.c.types.find(&[&namespace, name])?;
                if self.c.types.is_singleton(&found.kind) {
                    Some(self.object_ty(&found.kind))
                } else {
                    let o = self.of_kind(&found.kind);
                    Some(self.arena.intern(Kind::Type(o)))
                }
            }
            Kind::Record(fields) => {
                let field = fields.iter().find(|field| &*field.name == name)?;
                Some(if field.optional {
                    self.arena.or(field.ty, Ty::UNDEFINED)
                } else {
                    field.ty
                })
            }
            Kind::Array { .. } => builtins::array_member(&mut self.arena, part, name),
            Kind::Map { .. } => builtins::map_member(&mut self.arena, part, name),
            Kind::String | Kind::Str(_) | Kind::Color => {
                builtins::primitive_member(&mut self.arena, part, P::String, name)
            }
            Kind::Number | Kind::Num(_) => {
                builtins::primitive_member(&mut self.arena, part, P::Number, name)
            }
            Kind::Boolean | Kind::Bool(_) => {
                builtins::primitive_member(&mut self.arena, part, P::Boolean, name)
            }
            Kind::Global(Global::Qt) => {
                if let Some(qt) = self.c.qt
                    && self.c.types.oracle.has_enum_key(qt, None, name)
                {
                    let object = self.obj(Obj::Native(qt));
                    return Some(self.arena.intern(Kind::Key {
                        object,
                        key: name.into(),
                    }));
                }
                // DECISION: Qt's functions have no types in the oracle.
                Some(self.loose())
            }
            Kind::Global(global) => builtins::global_member(&mut self.arena, global, name),
            Kind::EnumNs { object, .. } => Some(self.arena.intern(Kind::Key {
                object,
                key: name.into(),
            })),
            _ => None,
        }
    }

    /// A member expression or call, link by link: `short` is set where a
    /// `?.` met what may be undefined, and the chain then gives undefined.
    fn link(&mut self, e: &Expression<'a>, short: &mut bool) -> Ty {
        match e {
            Expression::StaticMemberExpression(m) => {
                let object = self.link(&m.object, short);
                let object = self.optional(object, m.optional, short);
                let base = self.member(object, m.property.name.as_str());
                match self.root_local(&m.object) {
                    Some(local) => self.fact(self.text(m.span), local).unwrap_or(base),
                    None => base,
                }
            }
            Expression::ComputedMemberExpression(m) => {
                let object = self.link(&m.object, short);
                let object = self.optional(object, m.optional, short);
                let index = self.expr(&m.expression, None);
                self.index(object, index)
            }
            Expression::CallExpression(call) => self.call(call, short),
            Expression::TSNonNullExpression(n) => {
                let ty = self.link(&n.expression, short);
                self.arena.without_undefined(ty)
            }
            _ => self.expr(e, None),
        }
    }

    fn optional(&mut self, object: Ty, optional: bool, short: &mut bool) -> Ty {
        if optional && self.arena.has_undefined(object) {
            *short = true;
            return self.arena.without_undefined(object);
        }
        object
    }

    /// `object[index]`.
    fn index(&mut self, object: Ty, index: Ty) -> Ty {
        let parts = self.arena.members(object);
        let mut found = Vec::new();
        for part in parts {
            let ty = match *self.arena.kind(part) {
                Kind::Array { elem, .. } if self.arena.is_number(index) || index == Ty::ERROR => {
                    self.arena.or(elem, Ty::UNDEFINED)
                }
                Kind::String | Kind::Str(_) => self.arena.or(Ty::STRING, Ty::UNDEFINED),
                _ => Ty::ERROR,
            };
            found.push(ty);
        }
        self.arena.union(found)
    }

    /// The local a path starts at: `Some(Some(id))` for a local,
    /// `Some(None)` for a name of QML's scope, None for what is no path.
    fn root_local(&self, e: &Expression<'a>) -> Option<Option<u32>> {
        match e {
            Expression::Identifier(id) => Some(self.local(id.name.as_str()).map(|local| local.id)),
            Expression::StaticMemberExpression(m) if !m.optional => self.root_local(&m.object),
            Expression::ParenthesizedExpression(p) => self.root_local(&p.expression),
            _ => None,
        }
    }

    // Calls.

    fn call(&mut self, call: &CallExpression<'a>, short: &mut bool) -> Ty {
        let callee = self.link(&call.callee, short);
        let callee = self.optional(callee, call.optional, short);
        match self.arena.kind(callee).clone() {
            Kind::Function(_) => self.apply(callee, &call.arguments, call.span),
            Kind::Method(receiver, Method::Array("map")) => self.map(receiver, call),
            Kind::Method(receiver, Method::Array("reduce")) => self.reduce(receiver, call),
            Kind::Method(receiver, method) => {
                match builtins::method_sig(&mut self.arena, receiver, method) {
                    Some(sig) => {
                        let f = self.arena.function(sig);
                        self.apply(f, &call.arguments, call.span)
                    }
                    None => self.loose_args(&call.arguments),
                }
            }
            Kind::Global(global) => match builtins::global_call(&mut self.arena, global) {
                Some(sig) => {
                    let f = self.arena.function(sig);
                    self.apply(f, &call.arguments, call.span)
                }
                None => self.loose_args(&call.arguments),
            },
            _ => self.loose_args(&call.arguments),
        }
    }

    fn loose_args(&mut self, arguments: &[Argument<'a>]) -> Ty {
        for argument in arguments {
            self.argument(argument, None);
        }
        Ty::ERROR
    }

    fn argument(&mut self, argument: &Argument<'a>, want: Option<Ty>) -> Ty {
        match argument {
            Argument::SpreadElement(spread) => {
                self.expr(&spread.argument, None);
                Ty::ERROR
            }
            _ => self.expr(argument.to_expression(), want),
        }
    }

    /// A call of the function of type `f`: its arguments' number and types.
    fn apply(&mut self, f: Ty, arguments: &[Argument<'a>], span: Span) -> Ty {
        let (count, required, rest, ret) = match self.arena.kind(f) {
            Kind::Function(sig) => (sig.params.len(), sig.required(), sig.rest, sig.ret),
            _ => return self.loose_args(arguments),
        };
        let given = arguments.len();
        let spread = arguments
            .iter()
            .any(|argument| matches!(argument, Argument::SpreadElement(_)));
        if !spread && rest.is_none() && given > count {
            self.report(
                Code::TsAssignType,
                format!("this call takes {count} arguments, and is given {given}"),
                span,
            );
        } else if !spread && given < required {
            self.report(
                Code::TsAssignType,
                format!("this call needs {required} arguments, and is given {given}"),
                span,
            );
        }
        for (at, argument) in arguments.iter().enumerate() {
            let want = self.sig_param(f, at).map(|(ty, _)| ty);
            let ty = self.argument(argument, want);
            if let Some(want) = want
                && !self.takes(want, ty)
            {
                let name = match self.arena.kind(f) {
                    Kind::Function(sig) => sig
                        .params
                        .get(at)
                        .map_or_else(|| "rest".to_string(), |p| p.name.to_string()),
                    _ => String::new(),
                };
                let (wanted, given) = (self.word(want), self.word(ty));
                self.report(
                    Code::TsAssignType,
                    format!("`{name}` is {wanted}; this gives it {given}"),
                    argument.span(),
                );
            }
        }
        ret
    }

    /// `array.map(callback)`: an array of what the callback gives.
    fn map(&mut self, receiver: Ty, call: &CallExpression<'a>) -> Ty {
        let Kind::Array { elem, .. } = *self.arena.kind(receiver) else {
            return self.loose_args(&call.arguments);
        };
        let want = self.callback(
            vec![("value", elem, false), ("index", Ty::NUMBER, true)],
            Ty::ERROR,
        );
        let mut result = Ty::ERROR;
        for (at, argument) in call.arguments.iter().enumerate() {
            let ty = self.argument(argument, (at == 0).then_some(want));
            if at == 0 {
                let given = self.sig_ret(ty);
                let given = self.arena.widen(given);
                result = self.arena.array(given, true);
            }
        }
        result
    }

    /// `array.reduce(callback, initial)`: what the callback gives, which it
    /// takes back.
    fn reduce(&mut self, receiver: Ty, call: &CallExpression<'a>) -> Ty {
        let Kind::Array { elem, .. } = *self.arena.kind(receiver) else {
            return self.loose_args(&call.arguments);
        };
        let acc = match call.arguments.get(1) {
            Some(initial) => {
                let ty = self.argument(initial, None);
                self.arena.widen(ty)
            }
            None => elem,
        };
        let want = self.callback(
            vec![
                ("acc", acc, false),
                ("value", elem, false),
                ("index", Ty::NUMBER, true),
            ],
            acc,
        );
        for (at, argument) in call.arguments.iter().enumerate() {
            if at != 1 {
                self.argument(argument, (at == 0).then_some(want));
            }
        }
        acc
    }

    fn callback(&mut self, params: Vec<(&str, Ty, bool)>, ret: Ty) -> Ty {
        let params = params
            .into_iter()
            .map(|(name, ty, optional)| Param {
                name: name.into(),
                ty,
                optional,
                written: None,
            })
            .collect();
        self.arena.function(Sig {
            params,
            rest: None,
            ret,
        })
    }

    // Expressions.

    /// The type of what `e` gives, `want` the type it is written for where
    /// there is one.
    pub(super) fn expr(&mut self, e: &Expression<'a>, want: Option<Ty>) -> Ty {
        match e {
            Expression::BooleanLiteral(b) => self.arena.boolean(b.value),
            Expression::NullLiteral(null) => {
                self.report(
                    Code::TsNoNull,
                    "`null` is not a value here: a value that may be missing is optional (`?`)"
                        .into(),
                    null.span,
                );
                Ty::ERROR
            }
            Expression::NumericLiteral(n) => self.arena.num(n.value),
            Expression::StringLiteral(s) => self.arena.str(s.value.as_str()),
            Expression::TemplateLiteral(t) => {
                for expression in &t.expressions {
                    self.expr(expression, None);
                }
                Ty::STRING
            }
            Expression::Identifier(id) => self.name(id),
            Expression::ParenthesizedExpression(p) => self.expr(&p.expression, want),
            Expression::ArrayExpression(array) => {
                let elem_want = want.and_then(|want| match *self.arena.kind(want) {
                    Kind::Array { elem, .. } => Some(elem),
                    _ => None,
                });
                let mut elems = Vec::with_capacity(array.elements.len());
                for element in &array.elements {
                    let ty = match element {
                        ArrayExpressionElement::SpreadElement(spread) => {
                            let ty = self.expr(&spread.argument, want);
                            match *self.arena.kind(ty) {
                                Kind::Array { elem, .. } => elem,
                                _ => Ty::ERROR,
                            }
                        }
                        ArrayExpressionElement::Elision(_) => Ty::UNDEFINED,
                        _ => self.expr(element.to_expression(), elem_want),
                    };
                    elems.push(ty);
                }
                if elems.is_empty() {
                    return want.unwrap_or(Ty::ERROR);
                }
                let all = self.arena.union(elems);
                if let Some(want) = want
                    && let Some(elem) = elem_want
                    && self.takes(elem, all)
                {
                    return want;
                }
                // DECISION: an array literal is a mutable array, as in
                // TypeScript; one is a readonly one where it goes.
                let all = self.arena.widen(all);
                self.arena.array(all, false)
            }
            Expression::ObjectExpression(object) => {
                let mut fields: Vec<Field> = Vec::with_capacity(object.properties.len());
                for property in &object.properties {
                    let ObjectPropertyKind::ObjectProperty(property) = property else {
                        return Ty::ERROR;
                    };
                    let name = match &property.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                        PropertyKey::StringLiteral(s) => s.value.as_str(),
                        _ => {
                            self.expr(&property.value, None);
                            return Ty::ERROR;
                        }
                    };
                    let field_want = want.and_then(|want| self.record_field(want, name));
                    let ty = self.expr(&property.value, field_want);
                    let ty = if field_want.is_some() {
                        ty
                    } else {
                        self.arena.widen(ty)
                    };
                    fields.push(Field {
                        name: name.into(),
                        ty,
                        optional: false,
                    });
                }
                self.arena.intern(Kind::Record(fields.into_boxed_slice()))
            }
            Expression::UnaryExpression(unary) => {
                let ty = self.expr(&unary.argument, None);
                match unary.operator {
                    UnaryOperator::LogicalNot => Ty::BOOLEAN,
                    UnaryOperator::Typeof => Ty::STRING,
                    UnaryOperator::Void => Ty::UNDEFINED,
                    UnaryOperator::Delete => Ty::BOOLEAN,
                    operator => {
                        if let (UnaryOperator::UnaryNegation, Kind::Num(bits)) =
                            (operator, self.arena.kind(ty))
                        {
                            let value = -f64::from_bits(*bits);
                            return self.arena.num(value);
                        }
                        self.need_number(operator.as_str(), ty, unary.argument.span());
                        Ty::NUMBER
                    }
                }
            }
            Expression::BinaryExpression(binary) => {
                let left = self.expr(&binary.left, None);
                let right = self.expr(&binary.right, None);
                self.binary(
                    binary.operator,
                    (left, binary.left.span()),
                    (right, binary.right.span()),
                    binary.span,
                )
            }
            Expression::LogicalExpression(logical) => {
                let left = self.expr(&logical.left, None);
                let mark = self.facts.len();
                let left_part = match logical.operator {
                    LogicalOperator::And => {
                        self.facts(&logical.left, true);
                        self.falsy(left)
                    }
                    LogicalOperator::Or => {
                        self.facts(&logical.left, false);
                        self.truthy(left)
                    }
                    LogicalOperator::Coalesce => self.arena.without_undefined(left),
                };
                let right = self.expr(&logical.right, want);
                self.facts.truncate(mark);
                self.arena.or(left_part, right)
            }
            Expression::ConditionalExpression(conditional) => {
                self.condition(&conditional.test, "`?:`");
                let mark = self.facts.len();
                self.facts(&conditional.test, true);
                let yes = self.expr(&conditional.consequent, want);
                self.facts.truncate(mark);
                self.facts(&conditional.test, false);
                let no = self.expr(&conditional.alternate, want);
                self.facts.truncate(mark);
                self.arena.or(yes, no)
            }
            Expression::AssignmentExpression(assignment) => {
                let place = match &assignment.left {
                    AssignmentTarget::AssignmentTargetIdentifier(id) => self.place_name(id),
                    AssignmentTarget::StaticMemberExpression(m) => {
                        let mut short = false;
                        let object = self.link(&m.object, &mut short);
                        let name = m.property.name.as_str();
                        let key = self
                            .root_local(&m.object)
                            .map(|local| (self.text(m.span), local));
                        self.place_member(object, name, key)
                    }
                    AssignmentTarget::ComputedMemberExpression(m) => {
                        let object = self.expr(&m.object, None);
                        let index = self.expr(&m.expression, None);
                        let ty = self.index(object, index);
                        let ty = self.arena.without_undefined(ty);
                        Some(Place {
                            name: String::new(),
                            word: self.word(ty),
                            ty,
                            key: None,
                        })
                    }
                    _ => None,
                };
                let Some(place) = place else {
                    return self.expr(&assignment.right, None);
                };
                let given = match assignment.operator {
                    AssignmentOperator::Assign => self.expr(&assignment.right, Some(place.ty)),
                    operator => {
                        let right = self.expr(&assignment.right, Some(place.ty));
                        if let Some(binary) = operator.to_binary_operator() {
                            self.binary(
                                binary,
                                (place.ty, assignment.left.span()),
                                (right, assignment.right.span()),
                                assignment.span,
                            )
                        } else {
                            let ty = self.arena.without_undefined(place.ty);
                            self.arena.or(ty, right)
                        }
                    }
                };
                if !place.name.is_empty() {
                    self.check_assign(
                        &place.name,
                        place.ty,
                        &place.word,
                        given,
                        assignment.right.span(),
                    );
                }
                if let Some((key, local)) = place.key {
                    let now = self.narrowed(place.ty, given);
                    let fact = Fact {
                        key,
                        local,
                        ty: now,
                        base: place.ty,
                    };
                    self.facts.push(fact);
                    self.assigned.push(fact);
                }
                given
            }
            Expression::SequenceExpression(sequence) => {
                let mut last = Ty::UNDEFINED;
                for expression in &sequence.expressions {
                    last = self.expr(expression, None);
                }
                last
            }
            Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::CallExpression(_) => {
                let mut short = false;
                let ty = self.link(e, &mut short);
                if short {
                    self.arena.or(ty, Ty::UNDEFINED)
                } else {
                    ty
                }
            }
            Expression::ChainExpression(chain) => {
                let mut short = false;
                let ty = match &chain.expression {
                    ChainElement::CallExpression(call) => self.call(call, &mut short),
                    ChainElement::TSNonNullExpression(n) => {
                        let ty = self.link(&n.expression, &mut short);
                        self.arena.without_undefined(ty)
                    }
                    element => match element.as_member_expression() {
                        Some(member) => {
                            // The member's own expression, as a link.
                            let object = self.link(member.object(), &mut short);
                            let object = self.optional(object, member.optional(), &mut short);
                            match member.static_property_name() {
                                Some(name) => self.member(object, name),
                                None => Ty::ERROR,
                            }
                        }
                        None => Ty::ERROR,
                    },
                };
                if short {
                    self.arena.or(ty, Ty::UNDEFINED)
                } else {
                    ty
                }
            }
            Expression::NewExpression(new) => {
                if let Expression::Identifier(id) = &new.callee
                    && id.name.as_str() == "Map"
                    && self.local("Map").is_none()
                    && let Some(arguments) = &new.type_arguments
                    && let [key, value] = arguments.params.as_slice()
                {
                    let (key, value) = (self.annot(key), self.annot(value));
                    self.loose_args(&new.arguments);
                    return self.arena.intern(Kind::Map { key, value });
                }
                self.expr(&new.callee, None);
                self.loose_args(&new.arguments)
            }
            Expression::ArrowFunctionExpression(arrow) => {
                let body = match &arrow.body {
                    ArrowFunctionBody::FunctionBody(body) => Body::Statements(&body.statements),
                    expression => Body::Expression(expression.to_expression()),
                };
                self.callable(&arrow.params, arrow.return_type.as_deref(), body, want)
            }
            Expression::FunctionExpression(function) => self.function(function, want),
            Expression::TSAsExpression(cast) => {
                self.expr(&cast.expression, None);
                self.annot(&cast.type_annotation)
            }
            Expression::TSTypeAssertion(cast) => {
                self.expr(&cast.expression, None);
                self.annot(&cast.type_annotation)
            }
            Expression::TSSatisfiesExpression(satisfies) => {
                let ty = self.annot(&satisfies.type_annotation);
                self.expr(&satisfies.expression, Some(ty))
            }
            Expression::TSNonNullExpression(n) => {
                let ty = self.expr(&n.expression, None);
                self.arena.without_undefined(ty)
            }
            Expression::UpdateExpression(update) => {
                let ty = match &update.argument {
                    SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => self.name(id),
                    target => match target.as_member_expression() {
                        Some(member) => {
                            let object = self.expr(member.object(), None);
                            match member.static_property_name() {
                                Some(name) => self.member(object, name),
                                None => Ty::ERROR,
                            }
                        }
                        None => Ty::ERROR,
                    },
                };
                self.need_number(update.operator.as_str(), ty, update.argument.span());
                Ty::NUMBER
            }
            Expression::AwaitExpression(inner) => {
                self.expr(&inner.argument, None);
                Ty::ERROR
            }
            _ => Ty::ERROR,
        }
    }

    fn record_field(&self, record: Ty, name: &str) -> Option<Ty> {
        match self.arena.kind(record) {
            Kind::Record(fields) => fields.iter().find(|f| &*f.name == name).map(|f| f.ty),
            _ => None,
        }
    }

    /// What a value of type `given` written to a place of type `declared`
    /// leaves it, for the facts after the assignment.
    fn narrowed(&mut self, declared: Ty, given: Ty) -> Ty {
        if !matches!(self.arena.kind(declared), Kind::Union(_)) || !self.takes(declared, given) {
            return declared;
        }
        let kept: Vec<Ty> = self
            .arena
            .members(declared)
            .into_iter()
            .filter(|part| self.takes(*part, given))
            .collect();
        if kept.is_empty() {
            declared
        } else {
            self.arena.union(kept)
        }
    }

    fn place_name(&mut self, id: &IdentifierReference<'a>) -> Option<Place<'a>> {
        let name = id.name.as_str();
        if let Some(local) = self.local(name) {
            if matches!(local.binding, Binding::Dead | Binding::Type) {
                return None;
            }
            return Some(Place {
                name: name.to_string(),
                ty: local.ty,
                word: self.word(local.ty),
                key: Some((name, Some(local.id))),
            });
        }
        let resolution = self.c.lookup(name, id.span.start)?;
        if resolution.aimed.is_some() || !resolution.roles.is_empty() {
            return None;
        }
        let Some(Target::Property {
            object,
            name: property,
        }) = resolution.target
        else {
            return None;
        };
        let own = self.instance_member(object, &property)?;
        Some(Place {
            name: name.to_string(),
            ty: own.ty,
            word: self.qml_word(&own),
            key: Some((name, None)),
        })
    }

    fn place_member(
        &mut self,
        object: Ty,
        name: &str,
        key: Option<(&'a str, Option<u32>)>,
    ) -> Option<Place<'a>> {
        // A member of a QML object is said in QML's words; of anything
        // else, in TypeScript's.
        if let Kind::Object(o) = *self.arena.kind(object) {
            let own = self.object_member(o, name)?;
            return Some(Place {
                name: name.to_string(),
                ty: own.ty,
                word: self.qml_word(&own),
                key,
            });
        }
        let ty = self.member(object, name);
        Some(Place {
            name: name.to_string(),
            ty,
            word: self.word(ty),
            key,
        })
    }

    fn need_number(&mut self, operator: &str, ty: Ty, span: Span) {
        if ty != Ty::ERROR && ty != Ty::NEVER && !self.arena.is_number(ty) {
            let word = self.word(ty);
            self.report(
                Code::TsAssignType,
                format!("`{operator}` needs numbers, and this is {word}"),
                span,
            );
        }
    }

    fn binary(
        &mut self,
        operator: BinaryOperator,
        (left, left_span): (Ty, Span),
        (right, right_span): (Ty, Span),
        span: Span,
    ) -> Ty {
        let op = operator.as_str();
        let lets_by = |ty: Ty| ty == Ty::ERROR || ty == Ty::NEVER;
        match operator {
            BinaryOperator::Equality | BinaryOperator::Inequality => {
                let strict = if operator == BinaryOperator::Equality {
                    "==="
                } else {
                    "!=="
                };
                self.report(
                    Code::TsLooseEquality,
                    format!("`{op}` converts what it compares; write `{strict}`"),
                    span,
                );
                Ty::BOOLEAN
            }
            BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
            | BinaryOperator::In
            | BinaryOperator::Instanceof => Ty::BOOLEAN,
            BinaryOperator::LessThan
            | BinaryOperator::LessEqualThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterEqualThan => {
                let arena = &self.arena;
                let number = |ty: Ty| lets_by(ty) || arena.is_number(ty);
                let string = |ty: Ty| lets_by(ty) || arena.is_string(ty);
                let fits = (number(left) && number(right)) || (string(left) && string(right));
                if !fits {
                    let (bad, bad_span) = if number(left) || string(left) {
                        (right, right_span)
                    } else {
                        (left, left_span)
                    };
                    let word = self.word(bad);
                    self.report(
                        Code::TsAssignType,
                        format!("`{op}` compares numbers or strings, and this is {word}"),
                        bad_span,
                    );
                }
                Ty::BOOLEAN
            }
            BinaryOperator::Addition => {
                let arena = &self.arena;
                let (ln, rn) = (arena.is_number(left), arena.is_number(right));
                let (ls, rs) = (arena.is_string(left), arena.is_string(right));
                let (lp, rp) = (
                    lets_by(left) || arena.is_primitive(left),
                    lets_by(right) || arena.is_primitive(right),
                );
                if ln && rn {
                    Ty::NUMBER
                } else if (ls && rp) || (rs && lp) {
                    Ty::STRING
                } else if lets_by(left) || lets_by(right) {
                    if ln || rn { Ty::NUMBER } else { Ty::ERROR }
                } else {
                    let (bad, bad_span) = if !lp || (rp && !ln && !ls) {
                        (left, left_span)
                    } else {
                        (right, right_span)
                    };
                    let word = self.word(bad);
                    self.report(
                        Code::TsAssignType,
                        format!("`+` needs numbers or a string, and this is {word}"),
                        bad_span,
                    );
                    Ty::ERROR
                }
            }
            _ => {
                let arena = &self.arena;
                let fine = |ty: Ty| lets_by(ty) || arena.is_number(ty);
                if !fine(left) {
                    self.need_number(op, left, left_span);
                } else if !fine(right) {
                    self.need_number(op, right, right_span);
                }
                Ty::NUMBER
            }
        }
    }

    /// What `a && b` gives of `a`: the parts of it that are falsy.
    fn falsy(&mut self, ty: Ty) -> Ty {
        let parts = self.arena.members(ty);
        let mut kept = Vec::with_capacity(parts.len());
        for part in parts {
            kept.push(match self.arena.kind(part) {
                Kind::Boolean | Kind::Bool(false) => Ty::FALSE,
                Kind::Bool(true)
                | Kind::Object(_)
                | Kind::Record(_)
                | Kind::Array { .. }
                | Kind::Map { .. }
                | Kind::Function(_)
                | Kind::Method(..) => Ty::NEVER,
                Kind::Num(bits) if f64::from_bits(*bits) != 0.0 => Ty::NEVER,
                Kind::Str(text) if !text.is_empty() => Ty::NEVER,
                _ => part,
            });
        }
        self.arena.union(kept)
    }

    /// What `a || b` gives of `a`: the parts of it that are truthy.
    fn truthy(&mut self, ty: Ty) -> Ty {
        let parts = self.arena.members(ty);
        let mut kept = Vec::with_capacity(parts.len());
        for part in parts {
            kept.push(match self.arena.kind(part) {
                Kind::Boolean | Kind::Bool(true) => Ty::TRUE,
                Kind::Bool(false) | Kind::Undefined | Kind::Void => Ty::NEVER,
                _ => part,
            });
        }
        self.arena.union(kept)
    }

    /// A condition is a boolean: TypeScript's subset has no truthiness.
    fn condition(&mut self, test: &Expression<'a>, what: &str) {
        self.quiet += 1;
        let ty = self.expr(test, None);
        self.quiet -= 1;
        if ty == Ty::ERROR || ty == Ty::NEVER || self.arena.is_boolean(ty) {
            return;
        }
        let subject = match test.without_parentheses() {
            Expression::Identifier(id) => format!("`{}`", id.name),
            Expression::StaticMemberExpression(m) => format!("`{}`", m.property.name),
            _ => "this".to_string(),
        };
        let word = self.word(ty);
        self.report(
            Code::TsConditionBoolean,
            format!("{what} needs a boolean, and {subject} is {word}: compare it"),
            test.span(),
        );
    }

    /// A test walked for what it reports, then read for its facts.
    fn test(&mut self, test: &Expression<'a>, what: &str) {
        self.condition(test, what);
        self.expr(test, None);
    }

    // Facts.

    /// The path `e` is, with its key, its local, what it is now and what it
    /// is declared as.
    fn path(&mut self, e: &Expression<'a>) -> Option<(&'a str, Option<u32>, Ty, Ty)> {
        self.quiet += 1;
        let found = match e.without_parentheses() {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                match self.local(name) {
                    Some(local) => {
                        let now = self.name(id);
                        Some((name, Some(local.id), now, local.ty))
                    }
                    None => {
                        let now = self.name(id);
                        let base = match self.c.lookup(name, id.span.start).and_then(|r| r.target) {
                            Some(Target::Property { object, name }) => self
                                .instance_member(object, &name)
                                .map_or(Ty::ERROR, |own| own.ty),
                            _ => Ty::ERROR,
                        };
                        Some((name, None, now, base))
                    }
                }
            }
            Expression::StaticMemberExpression(m) if !m.optional => {
                match self.root_local(&m.object) {
                    Some(local) => {
                        let object = self.expr(&m.object, None);
                        let base = self.member(object, m.property.name.as_str());
                        let key = self.text(m.span);
                        let now = self.fact(key, local).unwrap_or(base);
                        Some((key, local, now, base))
                    }
                    None => None,
                }
            }
            _ => None,
        };
        self.quiet -= 1;
        found.filter(|(_, _, now, _)| *now != Ty::ERROR)
    }

    fn push_fact(&mut self, key: &'a str, local: Option<u32>, ty: Ty, base: Ty) {
        self.facts.push(Fact {
            key,
            local,
            ty,
            base,
        });
    }

    /// Pushes what `e` being `sense` says of the paths it tests.
    pub(super) fn facts(&mut self, e: &Expression<'a>, sense: bool) {
        match e {
            Expression::ParenthesizedExpression(p) => self.facts(&p.expression, sense),
            Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
                self.facts(&unary.argument, !sense);
            }
            Expression::LogicalExpression(logical) => match (logical.operator, sense) {
                (LogicalOperator::And, true) | (LogicalOperator::Or, false) => {
                    self.facts(&logical.left, sense);
                    self.facts(&logical.right, sense);
                }
                _ => {}
            },
            Expression::BinaryExpression(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::StrictEquality | BinaryOperator::StrictInequality
                ) =>
            {
                let equal = (binary.operator == BinaryOperator::StrictEquality) == sense;
                self.compare_fact(&binary.left, &binary.right, equal);
                self.compare_fact(&binary.right, &binary.left, equal);
            }
            _ => {}
        }
    }

    /// What `subject === other` (or `!==`, when `equal` is false) says of
    /// `subject`.
    fn compare_fact(&mut self, subject: &Expression<'a>, other: &Expression<'a>, equal: bool) {
        let other = other.without_parentheses();
        // `typeof p === "number"`.
        if let Expression::UnaryExpression(unary) = subject.without_parentheses()
            && unary.operator == UnaryOperator::Typeof
            && let Expression::StringLiteral(text) = other
        {
            let Some((key, local, now, base)) = self.path(&unary.argument) else {
                return;
            };
            let name = text.value.as_str();
            let ty = if now == Ty::UNKNOWN {
                if !equal {
                    return;
                }
                match name {
                    "number" => Ty::NUMBER,
                    "string" => Ty::STRING,
                    "boolean" => Ty::BOOLEAN,
                    "undefined" => Ty::UNDEFINED,
                    _ => return,
                }
            } else {
                self.arena.filter(now, |arena, part| {
                    let same = arena.type_of(part).is_none_or(|of| of == name);
                    same == equal || arena.type_of(part).is_none()
                })
            };
            self.push_fact(key, local, ty, base);
            return;
        }
        // `p.kind === "a"`, of a union of records.
        if let Expression::StaticMemberExpression(m) = subject.without_parentheses()
            && !m.optional
            && let Some(literal) = self.literal(other)
            && let Some((key, local, now, base)) = self.path(&m.object)
            && self
                .arena
                .members(now)
                .iter()
                .all(|part| matches!(self.arena.kind(*part), Kind::Record(_)))
            && matches!(self.arena.kind(now), Kind::Union(_))
        {
            let field = m.property.name.as_str();
            let parts = self.arena.members(now);
            let kept: Vec<Ty> = parts
                .into_iter()
                .filter(|part| {
                    let ty = self.record_field(*part, field).unwrap_or(Ty::ERROR);
                    if equal {
                        self.takes(ty, literal)
                    } else {
                        ty != literal
                    }
                })
                .collect();
            let ty = self.arena.union(kept);
            self.push_fact(key, local, ty, base);
            return;
        }
        let Some((key, local, now, base)) = self.path(subject) else {
            return;
        };
        let undefined = matches!(other, Expression::Identifier(id) if id.name == "undefined"
            && self.local("undefined").is_none());
        let ty = if undefined {
            if equal {
                self.arena
                    .filter(now, |_, part| part == Ty::UNDEFINED || part == Ty::VOID)
            } else {
                self.arena.without_undefined(now)
            }
        } else if let Some(literal) = self.literal(other) {
            if equal {
                if !self.takes(now, literal) {
                    return;
                }
                literal
            } else {
                self.arena.filter(now, |_, part| part != literal)
            }
        } else {
            return;
        };
        self.push_fact(key, local, ty, base);
    }

    fn literal(&mut self, e: &Expression<'a>) -> Option<Ty> {
        Some(match e.without_parentheses() {
            Expression::BooleanLiteral(b) => self.arena.boolean(b.value),
            Expression::NumericLiteral(n) => self.arena.num(n.value),
            Expression::StringLiteral(s) => self.arena.str(s.value.as_str()),
            _ => return None,
        })
    }

    /// Where branches meet with both going on: what either assigned is
    /// what it is declared as again.
    fn forget_assigned(&mut self, from: usize) {
        let assigned: Vec<Fact<'a>> = self.assigned[from..].to_vec();
        for fact in assigned {
            self.facts.push(Fact {
                ty: fact.base,
                ..fact
            });
        }
    }

    // Statements.

    /// Walks statements in a scope; whether the end of them is reached.
    pub(super) fn stmts(&mut self, statements: &[Statement<'a>]) -> bool {
        let locals = self.locals.len();
        // `let`, `const`, functions and types are the block's before their
        // declarations run.
        for statement in statements {
            match statement {
                Statement::VariableDeclaration(declaration)
                    if declaration.kind != VariableDeclarationKind::Var =>
                {
                    for declarator in &declaration.declarations {
                        self.bind_dead(&declarator.id);
                    }
                }
                Statement::FunctionDeclaration(function) => {
                    if let Some(id) = &function.id {
                        let ty = self.signature(&function.params, function.return_type.as_deref());
                        self.bind(id.name.as_str(), ty, Binding::Function);
                    }
                }
                Statement::TSTypeAliasDeclaration(alias) => {
                    let ty = self.annot(&alias.type_annotation);
                    self.bind(alias.id.name.as_str(), ty, Binding::Type);
                }
                Statement::TSInterfaceDeclaration(interface) => {
                    let ty = self.record(&interface.body.body);
                    self.bind(interface.id.name.as_str(), ty, Binding::Type);
                }
                _ => {}
            }
        }
        let mut live = true;
        for statement in statements {
            live &= self.statement(statement);
        }
        self.locals.truncate(locals);
        live
    }

    fn bind_dead(&mut self, pattern: &BindingPattern<'a>) {
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                self.bind(id.name.as_str(), Ty::ERROR, Binding::Dead);
            }
            BindingPattern::ObjectPattern(object) => {
                for property in &object.properties {
                    self.bind_dead(&property.value);
                }
                if let Some(rest) = &object.rest {
                    self.bind_dead(&rest.argument);
                }
            }
            BindingPattern::ArrayPattern(array) => {
                for element in array.elements.iter().flatten() {
                    self.bind_dead(element);
                }
                if let Some(rest) = &array.rest {
                    self.bind_dead(&rest.argument);
                }
            }
            BindingPattern::AssignmentPattern(assignment) => self.bind_dead(&assignment.left),
        }
    }

    /// Binds the names of `pattern` to the parts of a value of type `ty`.
    fn bind_pattern(&mut self, pattern: &BindingPattern<'a>, ty: Ty, binding: Binding) {
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                self.bind(id.name.as_str(), ty, binding);
            }
            BindingPattern::ObjectPattern(object) => {
                for property in &object.properties {
                    let field = match &property.key {
                        PropertyKey::StaticIdentifier(id) => {
                            self.quiet += 1;
                            let field = self.member(ty, id.name.as_str());
                            self.quiet -= 1;
                            field
                        }
                        _ => Ty::ERROR,
                    };
                    self.bind_pattern(&property.value, field, binding);
                }
                if let Some(rest) = &object.rest {
                    self.bind_pattern(&rest.argument, Ty::ERROR, binding);
                }
            }
            BindingPattern::ArrayPattern(array) => {
                let elem = match *self.arena.kind(ty) {
                    Kind::Array { elem, .. } => elem,
                    Kind::String | Kind::Str(_) => Ty::STRING,
                    _ => Ty::ERROR,
                };
                for element in array.elements.iter().flatten() {
                    self.bind_pattern(element, elem, binding);
                }
                if let Some(rest) = &array.rest {
                    let all = self.arena.array(elem, true);
                    self.bind_pattern(&rest.argument, all, binding);
                }
            }
            BindingPattern::AssignmentPattern(assignment) => {
                let inner = self.arena.without_undefined(ty);
                let fallback = self.expr(&assignment.right, Some(inner));
                let ty = if ty == Ty::ERROR {
                    self.arena.widen(fallback)
                } else {
                    inner
                };
                self.bind_pattern(&assignment.left, ty, binding);
            }
        }
    }

    fn declaration(&mut self, declaration: &VariableDeclaration<'a>) {
        let binding = match declaration.kind {
            VariableDeclarationKind::Var => {
                let start = declaration.span.start;
                self.report(
                    Code::TsNoVar,
                    "`var` is the function's, wherever it is written; write `let` or `const`"
                        .into(),
                    Span::new(start, start + 3),
                );
                Binding::Let
            }
            VariableDeclarationKind::Const => Binding::Const,
            _ => Binding::Let,
        };
        for declarator in &declaration.declarations {
            let written = declarator
                .type_annotation
                .as_ref()
                .map(|annotation| self.annot(&annotation.type_annotation));
            let ty = match &declarator.init {
                Some(init) => {
                    let given = self.expr(init, written);
                    match written {
                        Some(written) => {
                            if !self.takes(written, given) {
                                let name = pattern_name(&declarator.id);
                                let (word, given) = (self.word(written), self.word(given));
                                self.report(
                                    Code::TsAssignType,
                                    format!("`{name}` is {word}; this gives it {given}"),
                                    init.span(),
                                );
                            }
                            written
                        }
                        None if binding == Binding::Const => given,
                        None => self.arena.widen(given),
                    }
                }
                None => written.unwrap_or(Ty::ERROR),
            };
            self.bind_pattern(&declarator.id, ty, binding);
        }
    }

    /// Walks a statement; whether what follows it is reached.
    pub(super) fn statement(&mut self, statement: &Statement<'a>) -> bool {
        match statement {
            Statement::BlockStatement(block) => self.stmts(&block.body),
            Statement::ExpressionStatement(statement) => {
                self.expr(&statement.expression, None);
                true
            }
            Statement::VariableDeclaration(declaration) => {
                self.declaration(declaration);
                true
            }
            Statement::FunctionDeclaration(function) => {
                self.function(function, None);
                true
            }
            Statement::IfStatement(statement) => {
                self.test(&statement.test, "`if`");
                let mark = self.facts.len();
                let assigned = self.assigned.len();
                self.facts(&statement.test, true);
                let yes = self.statement(&statement.consequent);
                let yes_facts: Vec<Fact<'a>> = self.facts.drain(mark..).collect();
                self.facts(&statement.test, false);
                let no = match &statement.alternate {
                    Some(alternate) => self.statement(alternate),
                    None => true,
                };
                let no_facts: Vec<Fact<'a>> = self.facts.drain(mark..).collect();
                match (yes, no) {
                    (true, false) => self.facts.extend(yes_facts),
                    (false, true) => self.facts.extend(no_facts),
                    (true, true) => self.forget_assigned(assigned),
                    (false, false) => {}
                }
                yes || no
            }
            Statement::WhileStatement(statement) => {
                self.test(&statement.test, "`while`");
                self.lap(Some(&statement.test), |checker| {
                    checker.statement(&statement.body);
                });
                true
            }
            Statement::DoWhileStatement(statement) => {
                self.lap(None, |checker| {
                    checker.statement(&statement.body);
                });
                self.test(&statement.test, "`while`");
                true
            }
            Statement::ForStatement(statement) => {
                let locals = self.locals.len();
                if let Some(init) = &statement.init {
                    match init.as_expression() {
                        Some(expression) => {
                            self.expr(expression, None);
                        }
                        None => {
                            if let qml_syntax::js::ForStatementInit::VariableDeclaration(d) = init {
                                self.declaration(d);
                            }
                        }
                    }
                }
                if let Some(test) = &statement.test {
                    self.test(test, "`for`");
                }
                self.lap(statement.test.as_ref(), |checker| {
                    checker.statement(&statement.body);
                    if let Some(update) = &statement.update {
                        checker.expr(update, None);
                    }
                });
                self.locals.truncate(locals);
                true
            }
            Statement::ForOfStatement(statement) => {
                let over = self.expr(&statement.right, None);
                let elem = match *self.arena.kind(over) {
                    Kind::Array { elem, .. } => elem,
                    Kind::String | Kind::Str(_) => Ty::STRING,
                    _ => Ty::ERROR,
                };
                self.each(&statement.left, elem, &statement.body);
                true
            }
            Statement::ForInStatement(statement) => {
                self.expr(&statement.right, None);
                self.each(&statement.left, Ty::STRING, &statement.body);
                true
            }
            Statement::ReturnStatement(statement) => {
                let declared = self.fns.last().and_then(|ctx| ctx.ret);
                let given = match &statement.argument {
                    Some(argument) => self.expr(argument, declared),
                    None => Ty::UNDEFINED,
                };
                if let Some(declared) = declared {
                    let span = statement
                        .argument
                        .as_ref()
                        .map_or(statement.span, |argument| argument.span());
                    self.check_return(declared, given, span);
                }
                if let Some(ctx) = self.fns.last_mut() {
                    ctx.returns.push(given);
                }
                false
            }
            Statement::ThrowStatement(statement) => {
                self.expr(&statement.argument, None);
                false
            }
            Statement::TryStatement(statement) => {
                let block = self.stmts(&statement.block.body);
                let caught = match &statement.handler {
                    Some(handler) => {
                        let locals = self.locals.len();
                        if let Some(param) = &handler.param {
                            self.bind_pattern(&param.pattern, Ty::UNKNOWN, Binding::Let);
                        }
                        let live = self.stmts(&handler.body.body);
                        self.locals.truncate(locals);
                        live
                    }
                    None => false,
                };
                let finally = match &statement.finalizer {
                    Some(finalizer) => self.stmts(&finalizer.body),
                    None => true,
                };
                (block || caught) && finally
            }
            Statement::SwitchStatement(statement) => {
                let over = self.expr(&statement.discriminant, None);
                let path = self.path(&statement.discriminant);
                let locals = self.locals.len();
                let assigned = self.assigned.len();
                for case in &statement.cases {
                    let mark = self.facts.len();
                    if let Some(test) = &case.test {
                        let given = self.expr(test, Some(over));
                        if let Some((key, local, now, base)) = path
                            && self.arena.is_literal(given)
                            && self.takes(now, given)
                        {
                            self.push_fact(key, local, given, base);
                        }
                    }
                    for statement in &case.consequent {
                        self.statement(statement);
                    }
                    self.facts.truncate(mark);
                }
                self.locals.truncate(locals);
                self.forget_assigned(assigned);
                true
            }
            Statement::LabeledStatement(statement) => {
                self.statement(&statement.body);
                true
            }
            Statement::BreakStatement(_) | Statement::ContinueStatement(_) => false,
            _ => true,
        }
    }

    /// A loop's body: what its test says holds in it, and what the body
    /// assigns is what it is declared as after it.
    ///
    /// DECISION: what the test narrows holds in the whole body, though the
    /// body may assign it after a first lap.
    fn lap(&mut self, test: Option<&Expression<'a>>, walk: impl FnOnce(&mut Self)) {
        let mark = self.facts.len();
        let assigned = self.assigned.len();
        if let Some(test) = test {
            self.facts(test, true);
        }
        walk(self);
        self.facts.truncate(mark);
        self.forget_assigned(assigned);
    }

    fn each(
        &mut self,
        left: &qml_syntax::js::ForStatementLeft<'a>,
        elem: Ty,
        body: &Statement<'a>,
    ) {
        let locals = self.locals.len();
        if let qml_syntax::js::ForStatementLeft::VariableDeclaration(declaration) = left {
            if declaration.kind == VariableDeclarationKind::Var {
                let start = declaration.span.start;
                self.report(
                    Code::TsNoVar,
                    "`var` is the function's, wherever it is written; write `let` or `const`"
                        .into(),
                    Span::new(start, start + 3),
                );
            }
            let binding = if declaration.kind == VariableDeclarationKind::Const {
                Binding::Const
            } else {
                Binding::Let
            };
            for declarator in &declaration.declarations {
                self.bind_pattern(&declarator.id, elem, binding);
            }
        }
        self.lap(None, |checker| {
            checker.statement(body);
        });
        self.locals.truncate(locals);
    }

    // Annotations.

    /// The type a TypeScript annotation names.
    pub(super) fn annot(&mut self, ty: &TSType<'a>) -> Ty {
        match ty {
            TSType::TSAnyKeyword(_) => Ty::ERROR,
            TSType::TSUnknownKeyword(_) => Ty::UNKNOWN,
            TSType::TSNeverKeyword(_) => Ty::NEVER,
            TSType::TSUndefinedKeyword(_) => Ty::UNDEFINED,
            TSType::TSVoidKeyword(_) => Ty::VOID,
            TSType::TSBooleanKeyword(_) => Ty::BOOLEAN,
            TSType::TSNumberKeyword(_) => Ty::NUMBER,
            TSType::TSStringKeyword(_) => Ty::STRING,
            TSType::TSNullKeyword(null) => {
                self.report(
                    Code::TsNoNull,
                    "`null` is not a value here: a value that may be missing is optional (`?`)"
                        .into(),
                    null.span,
                );
                Ty::ERROR
            }
            TSType::TSArrayType(array) => {
                let elem = self.annot(&array.element_type);
                self.arena.array(elem, false)
            }
            TSType::TSTypeOperatorType(operator)
                if operator.operator == TSTypeOperatorOperator::Readonly =>
            {
                let inner = self.annot(&operator.type_annotation);
                match *self.arena.kind(inner) {
                    Kind::Array { elem, .. } => self.arena.array(elem, true),
                    _ => inner,
                }
            }
            TSType::TSUnionType(union) => {
                let parts: Vec<Ty> = union.types.iter().map(|part| self.annot(part)).collect();
                self.arena.union(parts)
            }
            TSType::TSParenthesizedType(inner) => self.annot(&inner.type_annotation),
            TSType::TSLiteralType(literal) => match &literal.literal {
                TSLiteral::BooleanLiteral(b) => self.arena.boolean(b.value),
                TSLiteral::NumericLiteral(n) => self.arena.num(n.value),
                TSLiteral::StringLiteral(s) => self.arena.str(s.value.as_str()),
                TSLiteral::TemplateLiteral(_) => Ty::STRING,
                TSLiteral::UnaryExpression(unary) => match &unary.argument {
                    Expression::NumericLiteral(n)
                        if unary.operator == UnaryOperator::UnaryNegation =>
                    {
                        self.arena.num(-n.value)
                    }
                    _ => Ty::ERROR,
                },
                TSLiteral::BigIntLiteral(_) => Ty::ERROR,
            },
            TSType::TSTypeLiteral(literal) => self.record(&literal.members),
            TSType::TSFunctionType(function) => {
                self.signature(&function.params, Some(&function.return_type))
            }
            TSType::TSTypeReference(reference) => {
                let arguments: &[TSType<'a>] = match &reference.type_arguments {
                    Some(arguments) => &arguments.params,
                    None => &[],
                };
                match &reference.type_name {
                    TSTypeName::IdentifierReference(id) => {
                        self.named_type(id.name.as_str(), arguments)
                    }
                    TSTypeName::QualifiedName(qualified) => {
                        let TSTypeName::IdentifierReference(left) = &qualified.left else {
                            return Ty::ERROR;
                        };
                        let (left, right) = (left.name.as_str(), qualified.right.name.as_str());
                        let types = self.c.types;
                        if let Some(found) = types.find(&[left])
                            && types.declared_enum(&found.kind, right) == Some(true)
                        {
                            let object = self.of_kind(&found.kind);
                            return self.arena.intern(Kind::Enum {
                                object,
                                name: right.into(),
                            });
                        }
                        match types.find(&[left, right]) {
                            Some(found) => self.object_ty(&found.kind),
                            None => Ty::ERROR,
                        }
                    }
                    TSTypeName::ThisExpression(_) => Ty::ERROR,
                }
            }
            _ => Ty::ERROR,
        }
    }

    fn named_type(&mut self, name: &str, arguments: &[TSType<'a>]) -> Ty {
        if let Some(ty) = self.type_local(name) {
            return ty;
        }
        match (name, arguments) {
            ("Array", [elem]) => {
                let elem = self.annot(elem);
                return self.arena.array(elem, false);
            }
            ("ReadonlyArray" | "list", [elem]) => {
                let elem = self.annot(elem);
                return self.arena.array(elem, true);
            }
            ("Map", [key, value]) => {
                let (key, value) = (self.annot(key), self.annot(value));
                return self.arena.intern(Kind::Map { key, value });
            }
            _ => {}
        }
        if let Some(ty) = primitive(name) {
            return ty;
        }
        match self.c.types.find(&[name]) {
            Some(found) => self.object_ty(&found.kind),
            None => Ty::ERROR,
        }
    }

    fn record(&mut self, members: &[TSSignature<'a>]) -> Ty {
        let mut fields = Vec::with_capacity(members.len());
        for member in members {
            let TSSignature::TSPropertySignature(property) = member else {
                return Ty::ERROR;
            };
            let name = match &property.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => return Ty::ERROR,
            };
            let ty = match &property.type_annotation {
                Some(annotation) => self.annot(&annotation.type_annotation),
                None => Ty::ERROR,
            };
            fields.push(Field {
                name: name.into(),
                ty,
                optional: property.optional,
            });
        }
        self.arena.intern(Kind::Record(fields.into_boxed_slice()))
    }
}

/// The name a parameter or declarator is written with, for a message.
fn pattern_name<'a>(pattern: &BindingPattern<'a>) -> &'a str {
    match pattern {
        BindingPattern::BindingIdentifier(id) => id.name.as_str(),
        BindingPattern::AssignmentPattern(assignment) => pattern_name(&assignment.left),
        _ => "this",
    }
}
