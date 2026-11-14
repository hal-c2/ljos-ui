//! The checker's half that reads scripts: the type of each expression,
//! what statements bind and narrow, functions, arrows and annotations.

use oxc_ast_visit::Visit;
use qml_front::{TypeOracle, names::Target, types::Kind as QKind};
use qml_syntax::{
    GetSpan, Span,
    js::{
        Argument, ArrayExpressionElement, ArrowFunctionBody, AssignmentOperator, AssignmentTarget,
        AssignmentTargetMaybeDefault, AssignmentTargetProperty, BinaryOperator, BindingPattern,
        CallExpression, ChainElement, Expression, FormalParameters, Function, IdentifierReference,
        LogicalOperator, ObjectPropertyKind, PropertyKey, SimpleAssignmentTarget, SpreadElement,
        Statement, StaticMemberExpression, TSLiteral, TSSignature, TSType, TSTypeAnnotation,
        TSTypeName, TSTypeOperatorOperator, TSTypePredicateName, UnaryOperator,
        VariableDeclaration, VariableDeclarationKind,
    },
};

use super::module::{Space, listed};
use super::shadow::{Between, Shadows};
use super::ty::{Field, Global, Guard, Kind, Method, Param, Sig, Ty, Web};
use super::{Binding, Checker, Fact, FnCtx, Obj, builtins, primitive, web};
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
    /// For an optional member of an object, its type without the
    /// `undefined` its absence reads as.
    exact: Option<Ty>,
}

/// The name a record of the checker's gives an object literal's
/// `[Symbol.customMatcher]` member, which no name written can be.
pub(super) const MATCHER: &str = "[Symbol.customMatcher]";

/// The names a record gives an object literal's `[Symbol.dispose]` and
/// `[Symbol.asyncDispose]` members (DIALECT.md §4.22).
pub(super) const DISPOSE: &str = "[Symbol.dispose]";
pub(super) const ASYNC_DISPOSE: &str = "[Symbol.asyncDispose]";

/// The symbols a script names, each only as the key of a method in an
/// object literal, with the member's name in a record and a method written
/// with it.
const SYMBOLS: [(&str, &str, &str); 3] = [
    (
        "customMatcher",
        MATCHER,
        "{ [Symbol.customMatcher](subject: unknown, hint: string): boolean { … } }",
    ),
    ("dispose", DISPOSE, "{ [Symbol.dispose](): void { … } }"),
    (
        "asyncDispose",
        ASYNC_DISPOSE,
        "{ async [Symbol.asyncDispose](): Promise<void> { … } }",
    ),
];

/// What a script may do with a symbol (DIALECT.md §5.1).
const SYMBOL_RULE: &str = "a script names only `Symbol.customMatcher`, `Symbol.dispose` and `Symbol.asyncDispose`, each as the key of a method in an object literal";

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
        let (ret, guard) = match ret {
            Some(annotation) => self.returns(&annotation.type_annotation, params, false),
            None => (Ty::ERROR, None),
        };
        self.arena.function(Sig {
            params: list.into_boxed_slice(),
            rest,
            ret,
            guard,
        })
    }

    /// The return type `written` says, and what it says of a parameter
    /// where it is a type predicate: `v is T` returns a boolean, `asserts
    /// v is T` nothing. `report` says what TypeScript refuses of one.
    fn returns(
        &mut self,
        written: &TSType<'a>,
        params: &FormalParameters<'a>,
        report: bool,
    ) -> (Ty, Option<Guard>) {
        let TSType::TSTypePredicate(predicate) = written else {
            return (self.annot(written), None);
        };
        let ret = if predicate.asserts {
            Ty::VOID
        } else {
            Ty::BOOLEAN
        };
        // DECISION: `this is T` has no `this` to narrow in the subset, and
        // `asserts v` asserts truthiness, which it has none of: both are
        // the boolean or void they return, narrowing nothing.
        let (TSTypePredicateName::Identifier(name), Some(annotation)) =
            (&predicate.parameter_name, &predicate.type_annotation)
        else {
            return (ret, None);
        };
        let ty = self.annot(&annotation.type_annotation);
        let Some(param) = params
            .items
            .iter()
            .position(|p| pattern_name(&p.pattern) == name.name.as_str())
        else {
            if report {
                self.report(
                    Code::TsAssignType,
                    format!("`{}` is not a parameter of this function", name.name),
                    name.span,
                );
            }
            return (ret, None);
        };
        (
            ret,
            Some(Guard {
                param,
                ty,
                asserts: predicate.asserts,
            }),
        )
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
            function.r#async,
            true,
        )
    }

    /// Walks a function: its parameters typed by their annotations, else by
    /// the function `want` is; its returns checked against its return
    /// type, written or wanted. Gives its type, whose return a literal
    /// widens in unless `widen` is false. An `asynchronous` one returns a
    /// promise of what its body returns.
    fn callable(
        &mut self,
        params: &FormalParameters<'a>,
        ret: Option<&TSTypeAnnotation<'a>>,
        body: Body<'_, 'a>,
        want: Option<Ty>,
        asynchronous: bool,
        widen: bool,
    ) -> Ty {
        let want = want.filter(|want| matches!(self.arena.kind(*want), Kind::Function(_)));
        let mut list = Vec::with_capacity(params.items.len());
        for (at, p) in params.items.iter().enumerate() {
            let ty = match &p.type_annotation {
                Some(annotation) => self.annot(&annotation.type_annotation),
                None => match want.and_then(|want| self.sig_param(want, at)) {
                    Some((ty, _)) => ty,
                    // TypeScript infers a parameter's type from its default.
                    None if p.initializer.is_some() => Ty::ERROR,
                    None => {
                        let name = pattern_name(&p.pattern);
                        self.untyped(
                            name,
                            &format!("write its type (`{name}: number`, say)"),
                            p.pattern.span(),
                        )
                    }
                },
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
        let rest = params.rest.as_ref().map(|rest| {
            if rest.type_annotation.is_none() {
                let name = pattern_name(&rest.rest.argument);
                return self.untyped(
                    name,
                    &format!("write its type (`...{name}: number[]`, say)"),
                    rest.rest.argument.span(),
                );
            }
            self.rest_elem(rest.type_annotation.as_deref())
        });
        let (written, guard) = match ret {
            Some(annotation) => {
                let (ty, guard) = self.returns(&annotation.type_annotation, params, true);
                (Some(ty), guard)
            }
            None => (None, None),
        };
        if let Some(guard) = guard
            && let Some(param) = list.get(guard.param)
            && !self.takes(param.ty, guard.ty)
            && let Some(annotation) = ret
        {
            let (name, word, to) = (param.name.clone(), self.word(param.ty), self.word(guard.ty));
            let span = annotation.type_annotation.span();
            let text = self.text(span).trim_start_matches("asserts ").to_string();
            self.report(
                Code::TsAssignType,
                format!("`{text}` narrows `{name}`, {word}, to what it cannot be: {to}"),
                span,
            );
        }
        if asynchronous
            && let (Some(written), Some(annotation)) = (written, ret)
            && !matches!(self.arena.kind(written), Kind::Promise(_) | Kind::Error)
        {
            let text = self.text(annotation.type_annotation.span()).to_string();
            self.report(
                Code::TsAssignType,
                format!("an async function returns a promise: write `Promise<{text}>`"),
                annotation.type_annotation.span(),
            );
        }
        // The body of an async function returns what its promise gives.
        let unwrap = |checker: &mut Self, ty: Ty| {
            if asynchronous {
                web::awaited(&mut checker.arena, ty)
            } else {
                ty
            }
        };
        let wanted = want.map(|want| self.sig_ret(want));
        let wanted = wanted.map(|ret| unwrap(self, ret));
        let declared_written = written.map(|ty| unwrap(self, ty));
        let declared =
            declared_written.or(wanted.filter(|ret| *ret != Ty::VOID && *ret != Ty::ERROR));
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
                asynchronous,
            });
            match body {
                Body::Expression(expression) => {
                    let given = checker.expr(expression, declared);
                    let given = unwrap(checker, given);
                    if let Some(declared) = declared {
                        checker.check_return(declared, given, expression.span());
                    }
                    inferred = if widen {
                        checker.arena.widen(given)
                    } else {
                        given
                    };
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
                        inferred = if widen { checker.arena.widen(all) } else { all };
                    }
                }
                Body::None => {}
            }
            checker.fns.pop();
        });
        let ret = match written {
            Some(written) => written,
            None if asynchronous => self.arena.intern(Kind::Promise(inferred)),
            None => inferred,
        };
        self.arena.function(Sig {
            params: list.into_boxed_slice(),
            rest,
            ret,
            guard,
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
                Binding::IsBound => match self.fact(name, Some(local.id)) {
                    Some(ty) => ty,
                    None => {
                        self.report(
                            Code::TsIsBinding,
                            format!("`{name}` is bound only where the `is` that binds it was true"),
                            id.span,
                        );
                        local.ty
                    }
                },
                _ => self.fact(name, Some(local.id)).unwrap_or(local.ty),
            };
        }
        match name {
            "undefined" => return Ty::UNDEFINED,
            "NaN" | "Infinity" => return Ty::NUMBER,
            "eval" => return self.forbidden(name, "write the code it would run", id.span),
            "Symbol" => return self.forbidden(name, SYMBOL_RULE, id.span),
            "XMLHttpRequest" => return self.forbidden(name, "use `fetch`", id.span),
            "arguments" => {
                return self.forbidden(
                    name,
                    "write a rest parameter (`...args: number[]`)",
                    id.span,
                );
            }
            _ => {}
        }
        // DECISION: the globals TYPES.md lists are these, whatever QML's
        // scope has of the name.
        if let Some(global) = builtins::global(name) {
            return self.arena.intern(Kind::Global(global));
        }
        // DECISION: `fetch` and its classes are the page's, so a name of
        // the file's own, a function `fetch` or a type `Headers`, is that
        // one, as qml_front resolves it for the program (§4.19).
        let web = super::web::global(name);
        let web_or_untyped = |checker: &mut Self| match web {
            Some(global) => checker.arena.intern(Kind::Global(global)),
            None => checker.not_in_scope(name, id.span),
        };
        // A module has no QML scope: what it does not declare or import is
        // not there.
        if self.module.is_some() {
            return web_or_untyped(self);
        }
        let Some(resolution) = self.c.lookup(name, id.span.start) else {
            return web_or_untyped(self);
        };
        if let Some(Target::Global(qml_front::names::Global::Js)) = resolution.target
            && web.is_some()
        {
            return web_or_untyped(self);
        }
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
            Some(Target::Namespace(qml_front::project::Source::Path(path)))
                if crate::script::is_module(&path) =>
            {
                let key = self.c.project.front().import_dir(self.c.key, &path);
                match self.module_at(&key) {
                    Some(index) => self.module_value(index),
                    // DECISION: a module that is not there is the
                    // compiler's to say; one being read, reached again
                    // through a cycle, is let by.
                    None => Ty::ERROR,
                }
            }
            Some(Target::Namespace(qml_front::project::Source::Path(path)))
                if crate::script::module::is_javascript(&path) =>
            {
                let key = self.c.project.front().import_dir(self.c.key, &path);
                self.javascript_value(&key)
            }
            Some(Target::Namespace(_)) => self.arena.intern(Kind::Namespace(name.into())),
            Some(Target::Global(qml_front::names::Global::Core)) => self.core_global(name),
            _ => Ty::ERROR,
        }
    }

    /// What `Ljos.Core` gives a file that imports it, by name
    /// (docs/DIALECT.md §4.3).
    fn core_global(&mut self, name: &str) -> Ty {
        let numbers = |names: &[&str]| -> Vec<Param> {
            names
                .iter()
                .map(|name| Param {
                    name: (*name).into(),
                    ty: Ty::NUMBER,
                    optional: false,
                    written: None,
                })
                .collect()
        };
        let made = |checker: &mut Self, params: Vec<Param>, ret: Ty| {
            checker.arena.function(Sig {
                params: params.into_boxed_slice(),
                rest: None,
                ret,
                guard: None,
            })
        };
        let value = |checker: &mut Self, ty: &str| {
            let oracle = checker.c.types.oracle;
            // The vectors and `quaternion` are QtQuick's value types.
            match oracle
                .module_type("QtQml", ty)
                .or_else(|| oracle.module_type("QtQuick", ty))
            {
                Some(ty) => checker.object_ty(&qml_front::types::Kind::Builtin(ty)),
                None => Ty::ERROR,
            }
        };
        let colour = |checker: &mut Self, names: &[&str]| {
            let mut params = numbers(names);
            params.push(Param {
                name: "a".into(),
                ty: Ty::NUMBER,
                optional: true,
                written: None,
            });
            made(checker, params, Ty::COLOR)
        };
        match name {
            "later" => self.arena.function(Sig {
                params: Box::new([Param {
                    name: "f".into(),
                    ty: Ty::UNKNOWN,
                    optional: false,
                    written: None,
                }]),
                rest: Some(Ty::UNKNOWN),
                ret: Ty::VOID,
                guard: None,
            }),
            "point" | "vector2d" => {
                let ret = value(self, name);
                made(self, numbers(&["x", "y"]), ret)
            }
            "size" => {
                let ret = value(self, name);
                made(self, numbers(&["width", "height"]), ret)
            }
            "rect" => {
                let ret = value(self, name);
                made(self, numbers(&["x", "y", "width", "height"]), ret)
            }
            "vector3d" => {
                let ret = value(self, name);
                made(self, numbers(&["x", "y", "z"]), ret)
            }
            "vector4d" => {
                let ret = value(self, name);
                made(self, numbers(&["x", "y", "z", "w"]), ret)
            }
            "quaternion" => {
                let ret = value(self, name);
                made(self, numbers(&["scalar", "x", "y", "z"]), ret)
            }
            "rgba" => colour(self, &["r", "g", "b"]),
            "hsla" => colour(self, &["h", "s", "l"]),
            "hsva" => colour(self, &["h", "s", "v"]),
            "url" => made(
                self,
                vec![Param {
                    name: "url".into(),
                    ty: Ty::STRING,
                    optional: false,
                    written: None,
                }],
                Ty::STRING,
            ),
            "App" => self.app(),
            // docs/DIALECT.md §4.3.1: what was `Qt`'s.
            // DECISION: a binding goes to a property of any type, as
            // `Qt.binding`'s does; what it gives is not checked yet.
            "bind" => {
                let f = self.arena.function(Sig {
                    params: Box::new([]),
                    rest: None,
                    ret: Ty::UNKNOWN,
                    guard: None,
                });
                made(
                    self,
                    vec![Param {
                        name: "f".into(),
                        ty: f,
                        optional: false,
                        written: None,
                    }],
                    Ty::ERROR,
                )
            }
            // Sixteen numbers, an array of them, or none for the identity.
            "matrix4x4" => {
                let ret = value(self, name);
                self.arena.function(Sig {
                    params: Box::new([]),
                    rest: Some(Ty::UNKNOWN),
                    ret,
                    guard: None,
                })
            }
            "Color" => {
                let named = self.arena.or(Ty::STRING, Ty::COLOR);
                let of = made(
                    self,
                    vec![Param {
                        name: "color".into(),
                        ty: named,
                        optional: false,
                        written: None,
                    }],
                    Ty::COLOR,
                );
                self.arena.intern(Kind::Record(Box::new([Field {
                    name: "of".into(),
                    ty: of,
                    optional: false,
                    readonly: true,
                }])))
            }
            _ => Ty::ERROR,
        }
    }

    /// `App`: the application, with what was on `Qt` about it.
    fn app(&mut self) -> Ty {
        let field = |name: &str, ty: Ty, readonly: bool| Field {
            name: name.into(),
            ty,
            optional: false,
            readonly,
        };
        let sig = |checker: &mut Self, params: Vec<Param>, ret: Ty| {
            checker.arena.function(Sig {
                params: params.into_boxed_slice(),
                rest: None,
                ret,
                guard: None,
            })
        };
        let param = |name: &str, ty: Ty, optional: bool| Param {
            name: name.into(),
            ty,
            optional,
            written: None,
        };
        let platform = self.arena.intern(Kind::Record(Box::new([
            field("os", Ty::STRING, true),
            field("pluginName", Ty::STRING, true),
        ])));
        let open = sig(self, vec![param("url", Ty::STRING, false)], Ty::BOOLEAN);
        let quit = sig(self, Vec::new(), Ty::VOID);
        let exit = sig(self, vec![param("code", Ty::NUMBER, true)], Ty::VOID);
        let arguments = self.arena.array(Ty::STRING, true);
        // DECISION: the input method, the style hints, the screens and the
        // application's font are Qt's objects with no type in the oracle
        // yet: what is read off them is not checked.
        self.arena.intern(Kind::Record(Box::new([
            field("arguments", arguments, true),
            field("displayName", Ty::STRING, false),
            field("domain", Ty::STRING, false),
            field("exit", exit, true),
            field("font", Ty::ERROR, true),
            field("hints", Ty::ERROR, true),
            field("inputMethod", Ty::ERROR, true),
            field("language", Ty::STRING, false),
            field("layoutDirection", Ty::NUMBER, true),
            field("name", Ty::STRING, false),
            field("openUrlExternally", open, true),
            field("organization", Ty::STRING, false),
            field("platform", platform, true),
            field("quit", quit, true),
            field("quitting", Ty::ERROR, true),
            field("screens", Ty::ERROR, true),
            field("state", Ty::NUMBER, true),
            field("supportsMultipleWindows", Ty::BOOLEAN, true),
            field("version", Ty::STRING, false),
        ])))
    }

    /// What a type of Ljos.Core or Ljos.Ui has of what was `Qt`'s
    /// (docs/DIALECT.md §4.3.1): `Locale.current`, `Locale.named(n)`,
    /// `Font.of(o)`, `Font.families()`.
    pub(super) fn type_static(&mut self, kind: &QKind<O::Type>, name: &str) -> Option<Ty> {
        let QKind::Builtin(ty) = kind else {
            return None;
        };
        let oracle = self.c.types.oracle;
        let is = |uri: &str, type_name: &str| oracle.module_type(uri, type_name) == Some(*ty);
        let param = |name: &str, ty: Ty| Param {
            name: name.into(),
            ty,
            optional: false,
            written: None,
        };
        if is("QtQml", "Locale") {
            return match name {
                "current" => Some(self.locale()),
                "named" => {
                    let ret = self.locale();
                    Some(self.arena.function(Sig {
                        params: Box::new([param("name", Ty::STRING)]),
                        rest: None,
                        ret,
                        guard: None,
                    }))
                }
                _ => None,
            };
        }
        if is("QtQuick", "Font") {
            return match name {
                "of" => {
                    let ret = match oracle.module_type("QtQuick", "font") {
                        Some(font) => self.object_ty(&QKind::Builtin(font)),
                        None => Ty::ERROR,
                    };
                    Some(self.arena.function(Sig {
                        params: Box::new([param("font", Ty::UNKNOWN)]),
                        rest: None,
                        ret,
                        guard: None,
                    }))
                }
                "families" => {
                    let ret = self.arena.array(Ty::STRING, true);
                    Some(self.arena.function(Sig {
                        params: Box::new([]),
                        rest: None,
                        ret,
                        guard: None,
                    }))
                }
                _ => None,
            };
        }
        None
    }

    /// A locale, as `Locale.current` and `Locale.named(n)` give it.
    fn locale(&mut self) -> Ty {
        let field = |name: &str, ty: Ty| Field {
            name: name.into(),
            ty,
            optional: false,
            readonly: true,
        };
        let param = |name: &str, ty: Ty, optional: bool| Param {
            name: name.into(),
            ty,
            optional,
            written: None,
        };
        let sig = |checker: &mut Self, params: Vec<Param>, ret: Ty| {
            checker.arena.function(Sig {
                params: params.into_boxed_slice(),
                rest: None,
                ret,
                guard: None,
            })
        };
        // A format string, or a Locale.FormatType.
        let format = self.arena.or(Ty::STRING, Ty::NUMBER);
        let date = sig(
            self,
            vec![
                param("date", Ty::UNKNOWN, false),
                param("format", format, true),
            ],
            Ty::STRING,
        );
        let named = sig(
            self,
            vec![
                param("index", Ty::NUMBER, false),
                param("format", Ty::NUMBER, true),
            ],
            Ty::STRING,
        );
        let pattern = sig(self, vec![param("format", Ty::NUMBER, true)], Ty::STRING);
        let strings = self.arena.array(Ty::STRING, true);
        let numbers = self.arena.array(Ty::NUMBER, true);
        self.arena.intern(Kind::Record(Box::new([
            field("amText", Ty::STRING),
            field("dateFormat", pattern),
            field("dateTimeFormat", pattern),
            field("dayName", named),
            field("decimalPoint", Ty::STRING),
            field("exponential", Ty::STRING),
            field("firstDayOfWeek", Ty::NUMBER),
            field("formatDate", date),
            field("formatDateTime", date),
            field("formatTime", date),
            field("groupSeparator", Ty::STRING),
            field("measurementSystem", Ty::NUMBER),
            field("monthName", named),
            field("name", Ty::STRING),
            field("negativeSign", Ty::STRING),
            field("percent", Ty::STRING),
            field("pmText", Ty::STRING),
            field("positiveSign", Ty::STRING),
            field("standaloneDayName", named),
            field("standaloneMonthName", named),
            field("textDirection", Ty::NUMBER),
            field("timeFormat", pattern),
            field("uiLanguages", strings),
            field("weekDays", numbers),
            field("zeroDigit", Ty::STRING),
        ])))
    }

    /// What a colour has of its own: its channels, and the methods that
    /// make another colour from it.
    fn color_member(&mut self, name: &str) -> Option<Ty> {
        let colour_of = |checker: &mut Self, param: &str, ty: Ty, optional: bool| {
            checker.arena.function(Sig {
                params: Box::new([Param {
                    name: param.into(),
                    ty,
                    optional,
                    written: None,
                }]),
                rest: None,
                ret: Ty::COLOR,
                guard: None,
            })
        };
        Some(match name {
            "r" | "g" | "b" | "a" | "hsvHue" | "hsvSaturation" | "hsvValue" | "hslHue"
            | "hslSaturation" | "hslLightness" => Ty::NUMBER,
            "valid" => Ty::BOOLEAN,
            "darker" | "lighter" => colour_of(self, "factor", Ty::NUMBER, true),
            "alpha" => colour_of(self, "alpha", Ty::NUMBER, false),
            "tint" => colour_of(self, "tint", Ty::COLOR, false),
            "equals" => {
                let other = self.arena.or(Ty::STRING, Ty::COLOR);
                self.arena.function(Sig {
                    params: Box::new([Param {
                        name: "other".into(),
                        ty: other,
                        optional: false,
                        written: None,
                    }]),
                    rest: None,
                    ret: Ty::BOOLEAN,
                    guard: None,
                })
            }
            _ => return None,
        })
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

    /// `member`, where the member is written: one the value does not have
    /// has no type. Where the compiler reports it too, its report is kept.
    /// `object` is the receiver as written, to name it in a fix.
    fn member_at(
        &mut self,
        receiver: Ty,
        property: &qml_syntax::js::IdentifierName<'a>,
        object: &Expression<'a>,
    ) -> Ty {
        let name = property.name.as_str();
        if self.forbid_member(name, property.span) {
            return Ty::ERROR;
        }
        let parts = self.arena.members(receiver);
        for &part in &parts {
            if let Kind::Array {
                elem,
                readonly: true,
            } = *self.arena.kind(part)
                && builtins::ARRAY_WRITE.contains(&name)
            {
                let word = self.word(part);
                self.report(
                    Code::TsReadonly,
                    format!("`{name}` changes the array, and it is {word}: build a new one"),
                    property.span,
                );
                let mutable = self.arena.array(elem, false);
                return self.member(mutable, name);
            }
        }
        if self.script_member(receiver, name, property.span) {
            return Ty::ERROR;
        }
        for &part in &parts {
            if let Some(index) = self.module_of(part)
                && self.exported(index, name).0.is_none()
            {
                self.not_exported(index, name, Space::Value, property.span);
                return Ty::ERROR;
            }
        }
        for part in parts {
            let checked = !matches!(
                self.arena.kind(part),
                Kind::Error | Kind::Never | Kind::Undefined | Kind::Void | Kind::Global(Global::Qt)
            );
            if checked && self.member_one(part, name).is_none() {
                let of = self.text(object.span());
                // What is not known to have the member is narrowed, as
                // TypeScript narrows it: the fix names how. Only a path
                // keeps what it is narrowed to; a call gives a new value
                // each time.
                let fix = match self.arena.kind(part) {
                    Kind::Unknown | Kind::NonNullish if self.root_local(object).is_none() => {
                        format!(
                            "`{of}` is unknown: keep it in a `const` and narrow that, or decode it with a type guard"
                        )
                    }
                    Kind::Unknown | Kind::NonNullish => format!(
                        "`{of}` is unknown: narrow it first (`typeof {of} === \"object\" && {of} !== null && \"{name}\" in {of}`), or decode it with a type guard"
                    ),
                    Kind::NonPrimitive => format!(
                        "`{of}` is an object not known to have it: check `\"{name}\" in {of}` first"
                    ),
                    Kind::Null => format!("`{of}` may be null: compare it with `null` first"),
                    _ => format!("{} has no member of that name", self.word(part)),
                };
                return self.untyped(name, &fix, property.span);
            }
        }
        self.member(receiver, name)
    }

    /// Whether `receiver` is a JavaScript file with no declarations, so
    /// that `name` read of it, however it is read (`.name`, `["name"]`,
    /// `const { name } =`), is `any` to TypeScript: reported, as
    /// `ts-boundary`, at `span`.
    fn script_member(&mut self, receiver: Ty, name: &str, span: Span) -> bool {
        let script = self.arena.members(receiver).into_iter().find_map(|part| {
            if let Kind::Script(key) = self.arena.kind(part) {
                Some(key.clone())
            } else {
                None
            }
        });
        let Some(key) = script else {
            return false;
        };
        self.boundary(&key, name, span);
        true
    }

    /// `script_member` for `object[index]`: the name is the string
    /// written, or else the index as written.
    fn script_index(&mut self, object: Ty, index: &Expression<'a>) -> bool {
        let name = match index {
            Expression::StringLiteral(s) => s.value.to_string(),
            other => format!("[{}]", self.text(other.span())),
        };
        self.script_member(object, &name, index.span())
    }

    /// `card.DragSource`: the object the type `name` attaches to an object
    /// read off it, as QML reads one.
    fn attached_off(&mut self, name: &str) -> Option<Ty> {
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let types = self.c.types;
        let found = types.find(&[name])?;
        let attached = types
            .base(&found.kind)
            .and_then(|base| types.oracle.attached(base))?;
        Some(self.object_ty(&QKind::Builtin(attached)))
    }

    /// `Easing.OutCubic`: a singleton's name is its one object, and QML
    /// reads the keys of the enums its type declares through that name as
    /// through a type's. Beside Qt's table `Easing` is a singleton; without
    /// it, it is a type that is not, so a key looked for on the type alone
    /// was found on one table and not the other.
    fn singleton_key(&mut self, o: u32, name: &str) -> Option<Ty> {
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let kind = self.kind_of(o)?;
        if !self.c.types.is_singleton(&kind) {
            return None;
        }
        self.type_member(o, name)
    }

    fn member_one(&mut self, part: Ty, name: &str) -> Option<Ty> {
        use builtins::Primitive as P;
        match self.arena.kind(part).clone() {
            Kind::Object(o) => self
                .object_member(o, name)
                .map(|own| own.ty)
                .or_else(|| self.singleton_key(o, name))
                .or_else(|| self.attached_off(name)),
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
            Kind::Module(_) => {
                let index = self.module_of(part)?;
                self.exported(index, name).0
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
            Kind::Promise(_) => web::promise_member(&mut self.arena, part, name),
            Kind::Web(class) => web::member(&mut self.arena, part, class, name),
            Kind::Color => self
                .color_member(name)
                .or_else(|| builtins::primitive_member(&mut self.arena, part, P::String, name)),
            Kind::String | Kind::Str(_) => {
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
            Expression::StaticMemberExpression(m) => self.static_member(m, short).1,
            Expression::ComputedMemberExpression(m) => {
                let object = self.link(&m.object, short);
                let object = self.optional(object, m.optional, short);
                let object = self.defined(object, m.object.span());
                let index = self.expr(&m.expression, None);
                if self.script_index(object, &m.expression) {
                    return Ty::ERROR;
                }
                let ty = self.index(object, index);
                // The desugar reads an item of an array pattern after it
                // tested the array's length.
                if self.desugared(m.expression.span().start) {
                    return self.arena.without_undefined(ty);
                }
                self.indexed = Some(m.span);
                ty
            }
            Expression::CallExpression(call) => self.call(call, short),
            Expression::TSNonNullExpression(n) => {
                let ty = self.link(&n.expression, short);
                self.bang(n.span);
                self.arena.without_undefined(ty)
            }
            _ => self.expr(e, None),
        }
    }

    /// `object.name`: the object's type, and the member's.
    fn static_member(&mut self, m: &StaticMemberExpression<'a>, short: &mut bool) -> (Ty, Ty) {
        if let Some(name) = self.symbol_member(&m.object, &m.property) {
            self.forbid_symbol(name, m.span);
            return (Ty::ERROR, Ty::ERROR);
        }
        let object = self.link(&m.object, short);
        let object = self.optional(object, m.optional, short);
        let object = self.defined(object, m.object.span());
        let base = self.member_at(object, &m.property, &m.object);
        let member = match self.root_local(&m.object) {
            Some(local) => self.fact(self.text(m.span), local).unwrap_or(base),
            None => base,
        };
        (object, member)
    }

    fn optional(&mut self, object: Ty, optional: bool, short: &mut bool) -> Ty {
        if optional && self.arena.has_undefined(object) {
            *short = true;
            return self.arena.without_undefined(object);
        }
        object
    }

    /// `value`, written at `span`, read or called: TypeScript refuses it
    /// where it may be undefined (`strictNullChecks`), so it is said here,
    /// once, and the rest is read as the value it is when it is there.
    fn defined(&mut self, value: Ty, span: Span) -> Ty {
        if !self.arena.has_undefined(value) {
            return value;
        }
        let text = self.text(span);
        if self.indexed == Some(span) {
            self.report(
                Code::TsIndexUndefined,
                format!(
                    "`{text}` may be undefined, as an index may be past the end: check it first"
                ),
                span,
            );
        } else {
            self.report(
                Code::TsAssignType,
                format!("`{text}` may be undefined: compare it with `undefined` first"),
                span,
            );
        }
        self.arena.without_undefined(value)
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
        if self.is_controller(call) {
            return self.controller(call);
        }
        if self.is_sugar(call, "$is") {
            return self.is_test(call);
        }
        if self.is_sugar(call, "$nomatch") {
            return self.no_match(call);
        }
        if self.is_sugar(call, "$extract") {
            return self.extract(call);
        }
        let (receiver, callee) = match &call.callee {
            Expression::StaticMemberExpression(m) => {
                let (object, member) = self.static_member(m, short);
                (Some((object, m.property.name.as_str())), member)
            }
            callee => (None, self.link(callee, short)),
        };
        let callee = self.optional(callee, call.optional, short);
        let callee = self.defined(callee, call.callee.span());
        if let Some((object, name)) = receiver {
            self.task_call(object, name, call);
        }
        match self.arena.kind(callee).clone() {
            Kind::Function(_) => self.apply(callee, &call.arguments, call.span),
            Kind::Method(receiver, Method::Array("map")) => self.map(receiver, call),
            Kind::Method(receiver, Method::Array("reduce")) => self.reduce(receiver, call),
            Kind::Method(receiver, Method::Array("filter")) => self.filter(receiver, call),
            Kind::Method(receiver, Method::Promise(name)) => {
                self.promise_call(receiver, name, call)
            }
            Kind::Method(
                _,
                Method::Web(
                    class @ (Web::DisposableStack | Web::AsyncDisposableStack),
                    name @ ("use" | "adopt"),
                ),
            ) => self.stack_hold(class, name, call),
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
            Kind::Unknown => {
                self.loose_args(&call.arguments);
                let callee = self.text(call.callee.span());
                self.untyped(
                    callee,
                    "an unknown value is not a function: narrow it first",
                    call.span,
                )
            }
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
        self.apply_with(f, arguments, span).0
    }

    /// `apply`, with the types of the arguments given.
    fn apply_with(&mut self, f: Ty, arguments: &[Argument<'a>], span: Span) -> (Ty, Vec<Ty>) {
        let (count, required, rest, ret) = match self.arena.kind(f) {
            Kind::Function(sig) => (sig.params.len(), sig.required(), sig.rest, sig.ret),
            _ => return (self.loose_args(arguments), Vec::new()),
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
        let mut given_types = Vec::with_capacity(arguments.len());
        for (at, argument) in arguments.iter().enumerate() {
            let want = self.sig_param(f, at).map(|(ty, _)| ty);
            let ty = self.argument(argument, want);
            given_types.push(ty);
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
                let wanted = self.word(want);
                self.check_assign(&name, want, &wanted, ty, argument.span());
            }
        }
        (ret, given_types)
    }

    /// `array.filter(predicate)`: as TypeScript 5.5 does for a predicate
    /// that says what it keeps (`isRow`, of `v is Row`), an array of that.
    /// DECISION: a predicate is not inferred from an arrow's body, as
    /// TypeScript 5.5 infers one: a guard says `v is T` where it is written.
    fn filter(&mut self, receiver: Ty, call: &CallExpression<'a>) -> Ty {
        let Some(sig) = builtins::method_sig(&mut self.arena, receiver, Method::Array("filter"))
        else {
            return self.loose_args(&call.arguments);
        };
        let f = self.arena.function(sig);
        let (ret, given) = self.apply_with(f, &call.arguments, call.span);
        let Kind::Array { elem, .. } = *self.arena.kind(receiver) else {
            return ret;
        };
        if let Some(&predicate) = given.first()
            && let Kind::Function(sig) = self.arena.kind(predicate)
            && let Some(guard) = sig.guard
            && guard.param == 0
            && !guard.asserts
            && self.takes(elem, guard.ty)
        {
            return self.arena.array(guard.ty, true);
        }
        ret
    }

    /// The guard of the function `call` calls, and the argument it narrows.
    fn guard_of<'c>(
        &mut self,
        call: &'c CallExpression<'a>,
    ) -> Option<(Guard, &'c Expression<'a>)> {
        self.quiet += 1;
        let callee = self.expr(&call.callee, None);
        self.quiet -= 1;
        let Kind::Function(sig) = self.arena.kind(callee) else {
            return None;
        };
        let guard = sig.guard?;
        match call.arguments.get(guard.param)? {
            Argument::SpreadElement(_) => None,
            argument => Some((guard, argument.to_expression())),
        }
    }

    /// Whether TypeScript narrows by an assertion called as `callee`: only
    /// where each name of it is declared with a written type (TS2775). A
    /// function of QML's scope is reached through the object it is a member
    /// of, whose type the file does not write, so it is reported, and
    /// narrows nothing.
    fn asserts_through_typed(&mut self, callee: &Expression<'a>) -> bool {
        let mut root = callee.without_parentheses();
        while let Expression::StaticMemberExpression(m) = root {
            root = m.object.without_parentheses();
        }
        let Expression::Identifier(id) = root else {
            return true;
        };
        let name = id.name.as_str();
        let typed = match self.local(name) {
            Some(local) => !self.inferred.contains(&local.id),
            None if self.module.is_some() => true,
            // An import of a module is declared by the module.
            None => matches!(
                self.c.lookup(name, id.span.start).and_then(|r| r.target),
                Some(Target::Namespace(_) | Target::Global(_)) | None
            ),
        };
        if !typed {
            let fix = if self.local(name).is_some() {
                format!("write the type of `{name}` where it is declared")
            } else {
                "move it into a TypeScript module the file imports".into()
            };
            self.report(
                Code::TsAssignType,
                format!(
                    "`{}` asserts, and TypeScript narrows by an assertion only through names declared with a type, which `{name}` is not (TS2775): {fix}",
                    self.text(callee.span())
                ),
                callee.span(),
            );
        }
        typed
    }

    /// What a guard of `to` being `sense` leaves of `now`, as TypeScript
    /// narrows by a type predicate: the parts that are a `to`, else `to`
    /// itself; or, where it is false, the parts that are not.
    fn narrow_guard(&mut self, now: Ty, to: Ty, sense: bool) -> Ty {
        if now == Ty::UNKNOWN || now == Ty::ERROR {
            return if sense { to } else { now };
        }
        let parts = self.arena.members(now);
        // What is not null or undefined may still be anything else: a
        // guard narrows it as it narrows unknown.
        let kept: Vec<Ty> = parts
            .into_iter()
            .filter_map(|part| match part {
                Ty::NON_NULLISH => Some(if sense { to } else { part }),
                _ => (self.takes(to, part) == sense).then_some(part),
            })
            .collect();
        if sense && kept.is_empty() {
            return to;
        }
        self.arena.union(kept)
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

    /// `promise.then(f)`, `.catch(f)`, `.finally(f)`: a promise of what
    /// the callback gives, awaited, as TypeScript's lib types them.
    fn promise_call(&mut self, receiver: Ty, name: &str, call: &CallExpression<'a>) -> Ty {
        let Kind::Promise(value) = *self.arena.kind(receiver) else {
            return self.loose_args(&call.arguments);
        };
        let fulfilled = self.callback(vec![("value", value, false)], Ty::ERROR);
        let rejected = self.callback(vec![("reason", Ty::UNKNOWN, false)], Ty::ERROR);
        let settled = self.callback(vec![], Ty::ERROR);
        let wants: &[Ty] = match name {
            "then" => &[fulfilled, rejected],
            "catch" => &[rejected],
            _ => &[settled],
        };
        let wants = wants.to_vec();
        let count = wants.len();
        if call.arguments.len() > count {
            self.report(
                Code::TsAssignType,
                format!(
                    "this call takes {count} arguments, and is given {}",
                    call.arguments.len()
                ),
                call.span,
            );
        }
        let mut gives = Vec::with_capacity(count);
        for (at, argument) in call.arguments.iter().enumerate() {
            let given = self.argument(argument, wants.get(at).copied());
            let given = match self.arena.kind(given) {
                Kind::Function(sig) => sig.ret,
                _ => Ty::ERROR,
            };
            gives.push(web::awaited(&mut self.arena, given));
        }
        let result = match name {
            // What the promise gives where no callback is given for it.
            "then" => {
                let done = gives.first().copied().unwrap_or(value);
                match gives.get(1) {
                    Some(failed) => self.arena.or(done, *failed),
                    None => done,
                }
            }
            "catch" => match gives.first() {
                Some(failed) => self.arena.or(value, *failed),
                None => value,
            },
            _ => value,
        };
        let result = self.arena.widen(result);
        self.arena.intern(Kind::Promise(result))
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
            guard: None,
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
                    return match want {
                        Some(want) => want,
                        None => self.untyped(
                            "[]",
                            "write the array's type where it goes (`const xs: number[] = []`)",
                            array.span,
                        ),
                    };
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
                let want = want.map(|want| self.discriminated(want, object));
                let mut fields: Vec<Field> = Vec::with_capacity(object.properties.len());
                for property in &object.properties {
                    let property = match property {
                        ObjectPropertyKind::ObjectProperty(property) => property,
                        ObjectPropertyKind::SpreadProperty(spread) => {
                            if !self.spread_into(&mut fields, spread) {
                                return Ty::ERROR;
                            }
                            continue;
                        }
                    };
                    let name = match &property.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                        PropertyKey::StringLiteral(s) => s.value.as_str(),
                        PropertyKey::StaticMemberExpression(m)
                            if property.computed
                                && let Some(name) = self.symbol_member(&m.object, &m.property) =>
                        {
                            let Some((_, field, _)) =
                                SYMBOLS.iter().find(|(symbol, ..)| *symbol == name)
                            else {
                                self.expr(&property.value, None);
                                return self.forbidden(
                                    &format!("Symbol.{name}"),
                                    SYMBOL_RULE,
                                    m.span,
                                );
                            };
                            field
                        }
                        _ => {
                            self.expr(&property.value, None);
                            let key = self.text(property.key.span());
                            return self.untyped(
                                key,
                                "write each member of the object as `name: value`",
                                property.key.span(),
                            );
                        }
                    };
                    let wanted = want.and_then(|want| self.record_field(want, name));
                    let field_want = wanted.map(|field| field.0);
                    let ty = self.expr(&property.value, field_want);
                    let ty = if field_want.is_some() {
                        ty
                    } else {
                        self.arena.widen(ty)
                    };
                    // A member variance alone refuses is said at its value,
                    // and the object then has the type it is given.
                    let ty = match field_want {
                        Some(field_want)
                            if !self.takes(field_want, ty)
                                && self.takes_as(field_want, ty, false) =>
                        {
                            let word = self.word(field_want);
                            self.check_assign(name, field_want, &word, ty, property.value.span());
                            field_want
                        }
                        _ => ty,
                    };
                    let mut optional = false;
                    if let Some((wanted, true)) = wanted
                        && self.arena.has_undefined(ty)
                        && !self.arena.has_undefined(wanted)
                    {
                        self.optional_exact(name, property.value.span());
                        optional = true;
                    }
                    let ty = if optional {
                        self.arena.without_undefined(ty)
                    } else {
                        ty
                    };
                    fields.push(Field {
                        name: name.into(),
                        ty,
                        optional,
                        readonly: false,
                    });
                }
                // A member written after a spread replaces the one it gave.
                let mut seen = std::collections::HashSet::new();
                let mut kept: Vec<Field> = fields
                    .into_iter()
                    .rev()
                    .filter(|field| seen.insert(field.name.clone()))
                    .collect();
                kept.reverse();
                self.arena.intern(Kind::Record(kept.into_boxed_slice()))
            }
            Expression::UnaryExpression(unary) => {
                if unary.operator == UnaryOperator::Delete {
                    let start = unary.span.start;
                    self.forbidden(
                        "delete",
                        "make the member optional, and build the object without it",
                        Span::new(start, start + 6),
                    );
                }
                let ty = self.expr(&unary.argument, None);
                match unary.operator {
                    UnaryOperator::LogicalNot => {
                        self.need_boolean(ty, &unary.argument, "`!`");
                        Ty::BOOLEAN
                    }
                    UnaryOperator::Typeof => Ty::STRING,
                    UnaryOperator::Void => Ty::UNDEFINED,
                    UnaryOperator::Delete => Ty::BOOLEAN,
                    operator => {
                        // As TypeScript, only a written `-1` or `+1` is a
                        // literal: the sign of a value of a literal type
                        // gives a number.
                        if let Some(value) = signed_literal(operator, &unary.argument) {
                            return self.arena.num(value);
                        }
                        self.need_number(operator.as_str(), ty, unary.argument.span());
                        Ty::NUMBER
                    }
                }
            }
            Expression::BinaryExpression(binary) => {
                let strict = matches!(
                    binary.operator,
                    BinaryOperator::StrictEquality | BinaryOperator::StrictInequality
                );
                // DECISION: `null` is written only to be compared with: a
                // decoder tells `typeof v === "object"` from null by it.
                let operand = |checker: &mut Self, side: &Expression<'a>| match side {
                    Expression::NullLiteral(_) if strict => Ty::NULL,
                    _ => checker.expr(side, None),
                };
                let left = operand(self, &binary.left);
                let right = operand(self, &binary.right);
                match binary.operator {
                    BinaryOperator::Instanceof => {
                        let (end, start) = (binary.left.span().end, binary.right.span().start);
                        let at = self
                            .text(Span::new(end, start))
                            .find("instanceof")
                            .map_or(end, |at| end + at as u32);
                        return self.forbidden(
                            "instanceof",
                            "narrow with `typeof`, `in`, `Array.isArray` or a type guard",
                            Span::new(at, at + 10),
                        );
                    }
                    BinaryOperator::In => {
                        self.in_object(right, binary.right.span());
                        return Ty::BOOLEAN;
                    }
                    _ => {}
                }
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
                self.test(&conditional.test, "`?:`");
                let mark = self.facts.len();
                self.facts(&conditional.test, true);
                let yes = self.expr(&conditional.consequent, want);
                self.facts.truncate(mark);
                self.facts(&conditional.test, false);
                let no = self.expr(&conditional.alternate, want);
                self.facts.truncate(mark);
                self.arena.or(yes, no)
            }
            // `(x = s.k, true)` of `s is { let k: x }`: what the `is` binds.
            Expression::AssignmentExpression(assignment)
                if let AssignmentTarget::AssignmentTargetIdentifier(id) = &assignment.left
                    && let Some(local) = self.local(id.name.as_str())
                    && local.binding == Binding::IsBound =>
            {
                let given = self.expr(&assignment.right, None);
                self.bound(local.id, given);
                given
            }
            Expression::AssignmentExpression(assignment) => {
                if let AssignmentTarget::AssignmentTargetIdentifier(id) = &assignment.left
                    && self.constant_write(id)
                {
                    self.expr(&assignment.right, None);
                    return self.name(id);
                }
                let place = match &assignment.left {
                    AssignmentTarget::AssignmentTargetIdentifier(id) => self.place_name(id),
                    AssignmentTarget::StaticMemberExpression(m) => {
                        let mut short = false;
                        let object = self.link(&m.object, &mut short);
                        let name = m.property.name.as_str();
                        if self.forbid_member(name, m.property.span)
                            || self.write_member(object, name, m.property.span)
                        {
                            return self.expr(&assignment.right, None);
                        }
                        let key = self
                            .root_local(&m.object)
                            .map(|local| (self.text(m.span), local));
                        self.place_member(object, name, key)
                    }
                    AssignmentTarget::ComputedMemberExpression(m) => {
                        let object = self.expr(&m.object, None);
                        let index = self.expr(&m.expression, None);
                        if self.script_index(object, &m.expression)
                            || self.write_index(object, m.object.span())
                        {
                            return self.expr(&assignment.right, None);
                        }
                        let ty = self.index(object, index);
                        let ty = self.arena.without_undefined(ty);
                        Some(Place {
                            name: String::new(),
                            word: self.word(ty),
                            ty,
                            key: None,
                            exact: None,
                        })
                    }
                    // JavaScript evaluates the value before the target's
                    // defaults, so what it reads is narrowed as before them.
                    target => {
                        let given = self.expr(&assignment.right, None);
                        self.constant_writes(target);
                        return given;
                    }
                };
                let Some(place) = place else {
                    return self.expr(&assignment.right, None);
                };
                let given = match assignment.operator {
                    AssignmentOperator::Assign => {
                        let given =
                            self.expr(&assignment.right, Some(place.exact.unwrap_or(place.ty)));
                        if let Some(exact) = place.exact
                            && self.arena.has_undefined(given)
                            && !self.arena.has_undefined(exact)
                        {
                            self.optional_exact(&place.name, assignment.right.span());
                            return given;
                        }
                        given
                    }
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
                        self.bang(n.span);
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
                // DECISION: `new Error(message)` is made too, for an
                // assertion function to throw; it is an object, its members
                // not read.
                if let Expression::Identifier(id) = &new.callee
                    && id.name.as_str() == "Error"
                    && self.local("Error").is_none()
                {
                    let message = self.arena.or(Ty::STRING, Ty::UNDEFINED);
                    let f = self.arena.function(Sig {
                        params: Box::new([Param {
                            name: "message".into(),
                            ty: message,
                            optional: true,
                            written: None,
                        }]),
                        rest: None,
                        ret: Ty::OBJECT,
                        guard: None,
                    });
                    return self.apply(f, &new.arguments, new.span);
                }
                if let Expression::Identifier(id) = &new.callee
                    && self.local(id.name.as_str()).is_none()
                    && let Some(class) = Web::named(id.name.as_str())
                    && let Some(sig) = web::construct(&mut self.arena, class)
                {
                    let f = self.arena.function(sig);
                    return self.apply(f, &new.arguments, new.span);
                }
                // The class is a name like any other, which `name` bans;
                // `new` reads its callee itself, so it says so here too.
                if let Expression::Identifier(id) = &new.callee
                    && id.name.as_str() == "XMLHttpRequest"
                    && self.local("XMLHttpRequest").is_none()
                {
                    self.loose_args(&new.arguments);
                    return self.forbidden("XMLHttpRequest", "use `fetch`", id.span);
                }
                self.loose_args(&new.arguments);
                let callee = self.text(new.callee.span());
                self.untyped(
                    callee,
                    "`new` makes only a `Map<K, V>()`, an `Error(message)`, a `Request`, `Headers`, an `AbortController`, a `Uint8Array`, a `DisposableStack` or an `AsyncDisposableStack` in Ljós's API",
                    new.span,
                )
            }
            Expression::ArrowFunctionExpression(arrow) => {
                let body = match &arrow.body {
                    ArrowFunctionBody::FunctionBody(body) => Body::Statements(&body.statements),
                    expression => Body::Expression(expression.to_expression()),
                };
                // A `match` is an expression, the arrow the desugar calls at
                // once: its arms keep their literal types, as the arms of
                // `?:` do, so `match (thick) { when true: 3; … }` is `3 | 1`.
                let widen = !self.desugared(arrow.span.start);
                self.callable(
                    &arrow.params,
                    arrow.return_type.as_deref(),
                    body,
                    want,
                    arrow.r#async,
                    widen,
                )
            }
            Expression::FunctionExpression(function) => {
                self.nested += 1;
                let ty = self.function(function, want);
                self.nested -= 1;
                ty
            }
            Expression::TSAsExpression(cast) => {
                let ty = self.expr(&cast.expression, None);
                if is_const(&cast.type_annotation) {
                    return ty;
                }
                let (end, start) = (
                    cast.expression.span().end,
                    cast.type_annotation.span().start,
                );
                let at = self
                    .text(Span::new(end, start))
                    .find("as")
                    .map_or(end, |at| end + at as u32);
                self.cast("as", Span::new(at, at + 2));
                self.annot(&cast.type_annotation)
            }
            Expression::TSTypeAssertion(cast) => {
                self.expr(&cast.expression, None);
                let written = Span::new(cast.span.start, cast.expression.span().start);
                let text = self.text(written).trim_end();
                self.cast(text, written);
                self.annot(&cast.type_annotation)
            }
            Expression::TSSatisfiesExpression(satisfies) => {
                let ty = self.annot(&satisfies.type_annotation);
                self.expr(&satisfies.expression, Some(ty))
            }
            Expression::TSNonNullExpression(n) => {
                let ty = self.expr(&n.expression, None);
                self.bang(n.span);
                self.arena.without_undefined(ty)
            }
            Expression::ThisExpression(this) => {
                if self.nested > 0 {
                    return self.forbidden(
                        "this",
                        "in a `function` it is whatever calls it; name the object by its id",
                        this.span,
                    );
                }
                match self.this {
                    Some(index) => {
                        let o = self.obj(Obj::Instance(index));
                        self.arena.intern(Kind::Object(o))
                    }
                    None => Ty::ERROR,
                }
            }
            Expression::ClassExpression(class) => {
                let start = class.span.start;
                self.forbidden(
                    "class",
                    "write a function, or a component",
                    Span::new(start, start + 5),
                )
            }
            Expression::UpdateExpression(update) => {
                let ty = match &update.argument {
                    SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                        self.constant_write(id);
                        self.name(id)
                    }
                    target => match target.as_member_expression() {
                        Some(member) => {
                            let object = self.expr(member.object(), None);
                            match member.static_property_name() {
                                Some(name) => {
                                    self.write_member(object, name, member.span());
                                    self.member(object, name)
                                }
                                None => {
                                    self.write_index(object, member.object().span());
                                    Ty::ERROR
                                }
                            }
                        }
                        None => Ty::ERROR,
                    },
                };
                self.need_number(update.operator.as_str(), ty, update.argument.span());
                Ty::NUMBER
            }
            Expression::AwaitExpression(inner) => {
                let ty = self.expr(&inner.argument, None);
                web::awaited(&mut self.arena, ty)
            }
            // A `match` or `is` that yields is a generator the desugar
            // delegates to, `(function* () { … }).apply(this, arguments)`:
            // its arms are checked, and it is what it returns, as the arrow
            // of one that does not yield is.
            Expression::YieldExpression(inner)
                if inner.delegate
                    && self.desugared(inner.span.start)
                    && let Some(Expression::CallExpression(call)) = &inner.argument
                    && let Expression::StaticMemberExpression(member) = &call.callee
                    && let Expression::FunctionExpression(function) =
                        member.object.without_parentheses()
                    && let Some(body) = &function.body =>
            {
                self.nested += 1;
                let f = self.callable(
                    &function.params,
                    None,
                    Body::Statements(&body.statements),
                    None,
                    function.r#async,
                    false,
                );
                self.nested -= 1;
                // `yield*` on an `async` generator gives what its body
                // returns, not the promise an `async` function would.
                let ret = self.sig_ret(f);
                if function.r#async {
                    web::awaited(&mut self.arena, ret)
                } else {
                    ret
                }
            }
            _ => Ty::ERROR,
        }
    }

    /// Adds the members `...value` gives to `fields`, as TypeScript does:
    /// a required member replaces one before it, an optional one may leave
    /// it, so it is either type. Whether the spread has a type.
    fn spread_into(&mut self, fields: &mut Vec<Field>, spread: &SpreadElement<'a>) -> bool {
        let given = self.expr(&spread.argument, None);
        if given == Ty::ERROR {
            return false;
        }
        // Spreading what may be undefined gives nothing when it is.
        let maybe = self.arena.has_undefined(given);
        let given = self.arena.without_undefined(given);
        let Kind::Record(spread_fields) = self.arena.kind(given).clone() else {
            let what = format!("...{}", self.text(spread.argument.span()));
            self.untyped(
                &what,
                "only an object of a `{ … }` type spreads",
                spread.span,
            );
            return false;
        };
        for field in spread_fields {
            let optional = field.optional || maybe;
            match fields.iter().position(|f| f.name == field.name) {
                Some(at) if optional => {
                    let before = fields[at].ty;
                    fields[at].ty = self.arena.or(before, field.ty);
                }
                // A spread's members are its own copies: none is readonly.
                Some(at) => {
                    fields[at] = Field {
                        readonly: false,
                        ..field
                    }
                }
                None => fields.push(Field {
                    optional,
                    readonly: false,
                    ..field
                }),
            }
        }
        true
    }

    /// The type of the member `name` of a record, and whether it is
    /// optional.
    /// The member of `want`, a union of records, that `object` is by the
    /// literals it writes: TypeScript's discriminated union, so that
    /// `{ status: "done", titles: [] }` goes where `{ status: "pending" } |
    /// { status: "done"; titles: readonly string[] }` does. `want` itself
    /// where no one member is.
    fn discriminated(&mut self, want: Ty, object: &oxc_ast::ast::ObjectExpression<'a>) -> Ty {
        let Kind::Union(parts) = self.arena.kind(want).clone() else {
            return want;
        };
        let mut fits: Vec<Ty> = parts
            .iter()
            .copied()
            .filter(|part| matches!(self.arena.kind(*part), Kind::Record(_)))
            .collect();
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                continue;
            };
            let name = match &property.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            let literal = match &property.value {
                Expression::StringLiteral(s) => self.arena.str(s.value.as_str()),
                Expression::NumericLiteral(n) => self.arena.num(n.value),
                Expression::BooleanLiteral(b) => self.arena.boolean(b.value),
                _ => continue,
            };
            fits.retain(|part| {
                self.record_field(*part, name)
                    .is_some_and(|(field, _)| self.takes(field, literal))
            });
        }
        match fits.as_slice() {
            [one] => *one,
            _ => want,
        }
    }

    fn record_field(&self, record: Ty, name: &str) -> Option<(Ty, bool)> {
        match self.arena.kind(record) {
            Kind::Record(fields) => fields
                .iter()
                .find(|f| &*f.name == name)
                .map(|f| (f.ty, f.optional)),
            _ => None,
        }
    }

    // The rules of the subset that are not types.

    /// `x!`, reported at the `!`.
    fn bang(&mut self, span: Span) {
        self.report(
            Code::TsNoCast,
            "`!` tells the checker a value is not undefined: compare it with `undefined` first"
                .into(),
            Span::new(span.end - 1, span.end),
        );
    }

    /// `x as T` or `<T>x`, reported at `written`.
    fn cast(&mut self, what: &str, written: Span) {
        self.report(
            Code::TsNoCast,
            format!(
                "`{what}` tells the checker a type it cannot see: narrow the value with `typeof` or a comparison"
            ),
            written,
        );
    }

    /// Reports what the subset leaves out of JavaScript.
    pub(super) fn forbidden(&mut self, what: &str, fix: &str, span: Span) -> Ty {
        self.report(
            Code::TsForbidden,
            format!("`{what}` is not in Ljós's API: {fix}"),
            span,
        );
        Ty::ERROR
    }

    /// A member that reaches an object's prototype: whether it is one.
    fn forbid_member(&mut self, name: &str, span: Span) -> bool {
        if !matches!(name, "prototype" | "__proto__") {
            return false;
        }
        self.forbidden(name, "write a function that takes the object", span);
        true
    }

    /// A write to the member `name` of `object`: whether a part of it
    /// declares the member readonly, which is reported.
    fn write_member(&mut self, object: Ty, name: &str, span: Span) -> bool {
        let readonly = self.arena.members(object).into_iter().any(|part| {
            matches!(self.arena.kind(part), Kind::Record(fields)
                if fields.iter().any(|field| &*field.name == name && field.readonly))
        });
        if readonly {
            self.report(
                Code::TsReadonly,
                format!("`{name}` is readonly: build a new object with the value it should have"),
                span,
            );
        }
        readonly
    }

    /// A write to an element of `object`, written at `span`: whether it is
    /// a readonly array, which is reported.
    fn write_index(&mut self, object: Ty, span: Span) -> bool {
        let readonly = self
            .arena
            .members(object)
            .into_iter()
            .find(|part| matches!(self.arena.kind(*part), Kind::Array { readonly: true, .. }));
        let Some(array) = readonly else {
            return false;
        };
        let (text, word) = (self.text(span), self.word(array));
        self.report(
            Code::TsReadonly,
            format!("`{text}` is {word}: build a new array rather than change it"),
            span,
        );
        true
    }

    /// An optional member given `undefined`, written at `span`.
    fn optional_exact(&mut self, name: &str, span: Span) {
        self.report(
            Code::TsOptionalExact,
            format!("`{name}` is optional: leave it out rather than set it to undefined"),
            span,
        );
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
                exact: None,
            });
        }
        if self.module.is_some() {
            return None;
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
            exact: None,
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
                exact: None,
            });
        }
        let ty = self.member(object, name);
        let exact = match self.arena.kind(object) {
            Kind::Record(fields) => fields
                .iter()
                .find(|field| &*field.name == name && field.optional)
                .map(|field| field.ty),
            _ => None,
        };
        Some(Place {
            name: name.to_string(),
            ty,
            word: self.word(ty),
            key,
            exact,
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
            BinaryOperator::StrictEquality | BinaryOperator::StrictInequality => {
                if self.never_equal(left, right) {
                    let display = |ty| self.arena.display(ty, &|o| self.obj_name(o));
                    let (left, right) = (display(left), display(right));
                    self.report(
                        Code::TsAssignType,
                        format!("`{op}` compares `{left}` with `{right}`, which it never is"),
                        span,
                    );
                }
                Ty::BOOLEAN
            }
            BinaryOperator::In | BinaryOperator::Instanceof => Ty::BOOLEAN,
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

    /// The right of `key in value`: TypeScript looks only into what is
    /// known to be an object.
    fn in_object(&mut self, value: Ty, span: Span) {
        let objects = self.arena.parts(value).iter().all(|part| {
            matches!(
                self.arena.kind(*part),
                Kind::Error
                    | Kind::Never
                    | Kind::NonPrimitive
                    | Kind::Object(_)
                    | Kind::Record(_)
                    | Kind::Array { .. }
                    | Kind::Map { .. }
                    | Kind::Promise(_)
                    | Kind::Web(_)
            )
        });
        if objects {
            return;
        }
        let of = self.text(span);
        if value == Ty::UNKNOWN {
            self.untyped(
                of,
                &format!(
                    "it is unknown, and `in` looks into an object: check `typeof {of} === \"object\" && {of} !== null` first"
                ),
                span,
            );
            return;
        }
        if self.arena.parts(value).contains(&Ty::NON_NULLISH) {
            self.untyped(
                of,
                &format!(
                    "it is unknown, and `in` looks into an object: check `typeof {of} === \"object\"` first"
                ),
                span,
            );
            return;
        }
        let word = self.word(value);
        self.report(
            Code::TsAssignType,
            format!("`in` looks into an object, and `{of}` is {word}"),
            span,
        );
    }

    /// What `"name" in value` being `sense` leaves of `value`, as
    /// TypeScript 4.9 narrows it: where a part declares the member, the
    /// parts that may have it; where none does, each part with the member
    /// added as `unknown` (TypeScript's `& Record<"name", unknown>`).
    fn narrow_in(&mut self, value: Ty, name: &str, sense: bool) -> Ty {
        let parts = self.arena.members(value);
        let declares = |arena: &super::ty::Arena, part: Ty| -> Option<bool> {
            match arena.kind(part) {
                Kind::Record(fields) => Some(
                    fields
                        .iter()
                        .find(|field| &*field.name == name)
                        .map_or(!sense, |field| field.optional || sense),
                ),
                _ => None,
            }
        };
        let known = parts.iter().any(|part| {
            matches!(self.arena.kind(*part), Kind::Record(fields)
                if fields.iter().any(|field| &*field.name == name))
        });
        if known {
            return self
                .arena
                .filter(value, |arena, part| declares(arena, part).unwrap_or(true));
        }
        if !sense {
            return value;
        }
        let added = Field {
            name: name.into(),
            ty: Ty::UNKNOWN,
            optional: false,
            readonly: false,
        };
        let widened: Vec<Ty> = parts
            .into_iter()
            .map(|part| match self.arena.kind(part).clone() {
                Kind::NonPrimitive => self
                    .arena
                    .intern(Kind::Record(vec![added.clone()].into_boxed_slice())),
                Kind::Record(fields) => {
                    let mut fields = fields.to_vec();
                    fields.push(added.clone());
                    self.arena.intern(Kind::Record(fields.into_boxed_slice()))
                }
                _ => part,
            })
            .collect();
        self.arena.union(widened)
    }

    /// What `Array.isArray(value)` being `sense` leaves of `value`.
    /// DECISION: of what is unknown, an `unknown[]`, where TypeScript says
    /// `any[]`: the subset has no `any`, so each element is narrowed too.
    fn narrow_array(&mut self, value: Ty, sense: bool) -> Ty {
        let is_array =
            |arena: &super::ty::Arena, part: Ty| matches!(arena.kind(part), Kind::Array { .. });
        if value == Ty::UNKNOWN {
            return if sense {
                self.arena.array(Ty::UNKNOWN, false)
            } else {
                value
            };
        }
        let unknown_array = self.arena.array(Ty::UNKNOWN, false);
        let parts: Vec<Ty> = self
            .arena
            .members(value)
            .into_iter()
            .filter_map(|part| match self.arena.kind(part) {
                Kind::NonPrimitive | Kind::NonNullish => {
                    Some(if sense { unknown_array } else { part })
                }
                _ if is_array(&self.arena, part) == sense => Some(part),
                _ => None,
            })
            .collect();
        self.arena.union(parts)
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
                | Kind::NonPrimitive
                | Kind::Array { .. }
                | Kind::Map { .. }
                | Kind::Promise(_)
                | Kind::Web(_)
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
                Kind::Bool(false) | Kind::Undefined | Kind::Void | Kind::Null => Ty::NEVER,
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
        self.need_boolean(ty, test, what);
    }

    /// `test`, of type `ty`, is where `what` takes a boolean.
    fn need_boolean(&mut self, ty: Ty, test: &Expression<'a>, what: &str) {
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
                        let found = if self.module.is_some() {
                            None
                        } else {
                            self.c.lookup(name, id.span.start).and_then(|r| r.target)
                        };
                        let base = match found {
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
                (LogicalOperator::And, false) | (LogicalOperator::Or, true) => {
                    self.either_facts(&logical.left, &logical.right, sense);
                }
                _ => {}
            },
            // `(x = s.k, true)`: where it is true, `x` holds what it was
            // given.
            Expression::SequenceExpression(sequence) if sense => {
                if let Some(Expression::AssignmentExpression(assignment)) = sequence
                    .expressions
                    .first()
                    .map(Expression::without_parentheses)
                    && let AssignmentTarget::AssignmentTargetIdentifier(id) = &assignment.left
                    && let Some(local) = self.local(id.name.as_str())
                    && local.binding == Binding::IsBound
                {
                    self.push_fact(id.name.as_str(), Some(local.id), local.ty, local.ty);
                }
            }
            // `$is(s, p)` of a pattern that is a name: a predicate's
            // guard, or the value's.
            Expression::CallExpression(call) if self.is_sugar(call, "$is") => {
                if let [Argument::SpreadElement(_), ..] | [_, Argument::SpreadElement(_)] =
                    call.arguments.as_slice()
                {
                    return;
                }
                let [subject, pattern] = call.arguments.as_slice() else {
                    return;
                };
                let (subject, pattern) = (subject.to_expression(), pattern.to_expression());
                // A custom matcher says what it matches only at run time.
                if self.matcher_of(pattern).is_some() {
                    return;
                }
                if let Some((guard, _)) = self.guard_of_callee(pattern) {
                    // An arm whose predicate failed may still have been the
                    // case: a predicate removes nothing.
                    if !guard.asserts
                        && sense
                        && let Some((key, local, now, base)) = self.path(subject)
                    {
                        let ty = self.narrow_guard(now, guard.ty, sense);
                        self.push_fact(key, local, ty, base);
                    }
                    return;
                }
                self.compare_fact(subject, pattern, sense);
            }
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
            // `"tag" in v`.
            Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::In => {
                let Expression::StringLiteral(name) = binary.left.without_parentheses() else {
                    return;
                };
                let Some((key, local, now, base)) = self.path(&binary.right) else {
                    return;
                };
                let ty = self.narrow_in(now, name.value.as_str(), sense);
                self.push_fact(key, local, ty, base);
            }
            // `Array.isArray(v)`.
            Expression::CallExpression(call) if self.is_array_test(call) => {
                if let Some(argument) = call.arguments.first()
                    && !matches!(argument, Argument::SpreadElement(_))
                    && let Some((key, local, now, base)) = self.path(argument.to_expression())
                {
                    let ty = self.narrow_array(now, sense);
                    self.push_fact(key, local, ty, base);
                }
            }
            // `isRow(v)`, of a type guard.
            Expression::CallExpression(call) => {
                if let Some((guard, subject)) = self.guard_of(call)
                    && !guard.asserts
                    && let Some((key, local, now, base)) = self.path(subject)
                {
                    let ty = self.narrow_guard(now, guard.ty, sense);
                    self.push_fact(key, local, ty, base);
                }
            }
            _ => {}
        }
    }

    /// `Symbol.name`'s name, where `Symbol` is the global.
    fn symbol_member(
        &self,
        object: &Expression<'a>,
        property: &qml_syntax::js::IdentifierName<'a>,
    ) -> Option<&'a str> {
        match object.without_parentheses() {
            Expression::Identifier(id) if id.name == "Symbol" && self.local("Symbol").is_none() => {
                Some(property.name.as_str())
            }
            _ => None,
        }
    }

    fn forbid_symbol(&mut self, name: &str, span: Span) {
        let fix = match SYMBOLS.iter().find(|(symbol, ..)| *symbol == name) {
            Some((_, _, example)) => {
                format!("it is only the key of a method in an object literal, as in `{example}`")
            }
            None => SYMBOL_RULE.to_owned(),
        };
        self.forbidden(&format!("Symbol.{name}"), &fix, span);
    }

    /// Holds `ty`, what `using` (`await using` where `asynchronous`) or a
    /// stack's `use` is given, to be disposable, as the proposal's
    /// `GetDisposeMethod` has it: `undefined` (or `null`), a stack, or an
    /// object whose `[Symbol.dispose]` (or, asynchronously, first
    /// `[Symbol.asyncDispose]`) member is a method that needs no argument.
    pub(super) fn disposable(&mut self, ty: Ty, asynchronous: bool, who: &str, span: Span) {
        let parts = self.arena.members(ty);
        let fits = parts.iter().all(|&part| {
            if part == Ty::UNDEFINED || part == Ty::NULL || part == Ty::ERROR {
                return true;
            }
            match self.arena.kind(part) {
                Kind::Web(Web::DisposableStack) => true,
                Kind::Web(Web::AsyncDisposableStack) => asynchronous,
                Kind::Record(fields) => fields.iter().any(|field| {
                    (&*field.name == DISPOSE || (asynchronous && &*field.name == ASYNC_DISPOSE))
                        && !field.optional
                        && matches!(
                            self.arena.kind(field.ty),
                            Kind::Function(sig) if sig.params.iter().all(|p| p.optional)
                        )
                }),
                _ => false,
            }
        });
        if fits {
            return;
        }
        // A resource disposed of only asynchronously, given to a
        // synchronous `using`, is told to await rather than to grow a
        // method it has the asynchronous form of.
        let only_async = !asynchronous
            && parts.iter().any(|&part| match self.arena.kind(part) {
                Kind::Web(Web::AsyncDisposableStack) => true,
                Kind::Record(fields) => fields.iter().any(|f| &*f.name == ASYNC_DISPOSE),
                _ => false,
            });
        let word = self.word(ty);
        let message = if only_async {
            format!(
                "{who} disposes of what it is given at once, and this is {word} disposed of asynchronously: write `await using`, or give it a `[Symbol.dispose]()` method"
            )
        } else if asynchronous {
            format!(
                "{who} disposes of what it is given, and this is {word} with no `[Symbol.asyncDispose]()` or `[Symbol.dispose]()` method: give an object with one, or `undefined`"
            )
        } else {
            format!(
                "{who} disposes of what it is given, and this is {word} with no `[Symbol.dispose]()` method: give an object with one, or `undefined`"
            )
        };
        self.report(Code::TsAssignType, message, span);
    }

    /// `stack.use(value)` and `stack.adopt(value, onDispose)`, which give
    /// `value` as they were given it: `use` holds it to be disposable, and
    /// `adopt` calls `onDispose` with it.
    fn stack_hold(&mut self, class: Web, name: &str, call: &CallExpression<'a>) -> Ty {
        let asynchronous = class == Web::AsyncDisposableStack;
        let Some(value) = call.arguments.first().and_then(|a| a.as_expression()) else {
            // No value, or a spread: the method's own signature still says
            // what is missing.
            let Some(sig) = web::method_sig(&mut self.arena, class, name) else {
                return self.loose_args(&call.arguments);
            };
            let f = self.arena.function(sig);
            return self.apply(f, &call.arguments, call.span);
        };
        self.quiet += 1;
        let ty = self.expr(value, None);
        self.quiet -= 1;
        let param = |name: &str, ty: Ty| Param {
            name: name.into(),
            ty,
            optional: false,
            written: None,
        };
        let mut params = vec![param("value", ty)];
        if name == "adopt" {
            params.push(param(
                "onDispose",
                web::on_dispose(&mut self.arena, class, ty),
            ));
        }
        let f = self.arena.function(Sig {
            params: params.into_boxed_slice(),
            rest: None,
            ret: ty,
            guard: None,
        });
        let ret = self.apply(f, &call.arguments, call.span);
        if name == "use" {
            self.disposable(ty, asynchronous, &format!("`{name}`"), value.span());
        }
        ret
    }

    /// The custom matcher of `pattern`'s object, quietly: its
    /// `[Symbol.customMatcher]` method's signature.
    fn matcher_of(&mut self, pattern: &Expression<'a>) -> Option<Ty> {
        self.quiet += 1;
        let ty = self.expr(pattern, None);
        self.quiet -= 1;
        let Kind::Record(fields) = self.arena.kind(ty) else {
            return None;
        };
        fields
            .iter()
            .find(|field| &*field.name == MATCHER)
            .map(|field| field.ty)
    }

    /// `$extract(s, p, n, reads)`, of the extractor `p(…)`: the list `p`'s
    /// matcher gives, without the `false` (or `true`) it gives where it does
    /// not match, or `undefined`. A primitive constructor's is its one
    /// value.
    fn extract(&mut self, call: &CallExpression<'a>) -> Ty {
        let [
            subject,
            pattern,
            Argument::NumericLiteral(_),
            Argument::NumericLiteral(_),
        ] = call.arguments.as_slice()
        else {
            self.loose_args(&call.arguments);
            return Ty::ERROR;
        };
        if matches!(subject, Argument::SpreadElement(_))
            || matches!(pattern, Argument::SpreadElement(_))
        {
            self.loose_args(&call.arguments);
            return Ty::ERROR;
        }
        self.expr(subject.to_expression(), None);
        let pattern = pattern.to_expression();
        if let Expression::Identifier(id) = pattern.without_parentheses()
            && self.local(id.name.as_str()).is_none()
        {
            let item = match id.name.as_str() {
                "String" => Some(Ty::STRING),
                "Number" => Some(Ty::NUMBER),
                "Boolean" => Some(Ty::BOOLEAN),
                _ => None,
            };
            if let Some(item) = item {
                let list = self.arena.array(item, true);
                return self.arena.or(list, Ty::UNDEFINED);
            }
        }
        let function = match self.matcher_of(pattern) {
            Some(method) => Some(method),
            None => {
                if let Expression::Identifier(id) = pattern.without_parentheses() {
                    self.pattern_name = Some(id.span);
                }
                let ty = self.expr(pattern, None);
                self.pattern_name = None;
                if ty == Ty::ERROR {
                    return Ty::ERROR;
                }
                Some(ty)
            }
        };
        let ret = function.and_then(|ty| match self.arena.kind(ty) {
            Kind::Function(sig) => Some(sig.ret),
            _ => None,
        });
        let Some(ret) = ret else {
            let written = self.as_written(pattern.span());
            self.report(
                Code::TsAssignType,
                format!(
                    "`{written}` has no custom matcher to take apart what it matches: give it a `[Symbol.customMatcher]` method, or write a function"
                ),
                pattern.span(),
            );
            return Ty::ERROR;
        };
        let list = self.arena.filter(ret, |arena, part| {
            !matches!(arena.kind(part), Kind::Boolean | Kind::Bool(_))
        });
        // A matcher that gives only `true` or `false` has no list: `true`
        // would be a `TypeError` where the extractor runs.
        if list == Ty::NEVER {
            let written = self.as_written(pattern.span());
            self.report(
                Code::TsAssignType,
                format!(
                    "`{written}` gives `true` or `false`, not a list to take apart: name it without `(…)`, or give a list where it matches"
                ),
                pattern.span(),
            );
            return Ty::ERROR;
        }
        // A primitive is no list either, even a string, which is iterable:
        // the extractor reads only an object's iterator.
        let primitive = self.arena.filter(list, |arena, part| {
            matches!(
                arena.kind(part),
                Kind::Number
                    | Kind::String
                    | Kind::Color
                    | Kind::Num(_)
                    | Kind::Str(_)
                    | Kind::Null
                    | Kind::Undefined
                    | Kind::Void
            )
        });
        if primitive != Ty::NEVER {
            let written = self.as_written(pattern.span());
            let word = self.word(primitive);
            self.report(
                Code::TsAssignType,
                format!(
                    "`{written}` gives {word} where it matches, not a list to take apart: give an array or another iterable object"
                ),
                pattern.span(),
            );
            return Ty::ERROR;
        }
        self.arena.or(list, Ty::UNDEFINED)
    }

    /// The names a destructuring assignment writes, `[size] = …` or
    /// `({ held } = …)`, each held to `constant_write`.
    fn constant_writes(&mut self, target: &AssignmentTarget<'a>) {
        match target {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.constant_write(id);
            }
            AssignmentTarget::ArrayAssignmentTarget(array) => {
                for element in array.elements.iter().flatten() {
                    self.constant_writes_maybe_default(element);
                }
                if let Some(rest) = &array.rest {
                    self.constant_writes(&rest.target);
                }
            }
            AssignmentTarget::ObjectAssignmentTarget(object) => {
                for property in &object.properties {
                    match property {
                        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(it) => {
                            self.constant_write(&it.binding);
                            if let Some(init) = &it.init {
                                self.default_value(init);
                            }
                        }
                        AssignmentTargetProperty::AssignmentTargetPropertyProperty(it) => {
                            self.constant_writes_maybe_default(&it.binding);
                        }
                    }
                }
                if let Some(rest) = &object.rest {
                    self.constant_writes(&rest.target);
                }
            }
            _ => {}
        }
    }

    /// A destructuring default, which runs only where the value it stands
    /// in for is `undefined`: after it, a name it assigns is what it was
    /// before or anything the default gave it. Every assignment in the
    /// default joins, since one in a branch of `?:` may be the one that ran.
    fn default_value(&mut self, init: &Expression<'a>) {
        let assigned = self.assigned.len();
        let facts = self.facts.len();
        self.expr(init, None);
        let given: Vec<Fact<'a>> = self.assigned[assigned..].to_vec();
        let after = self.facts.split_off(facts);
        let mut joined: Vec<Fact<'a>> = Vec::new();
        for fact in given {
            match joined
                .iter_mut()
                .find(|j| j.key == fact.key && j.local == fact.local)
            {
                Some(j) => j.ty = self.arena.or(j.ty, fact.ty),
                None => {
                    let before = self.fact(fact.key, fact.local).unwrap_or(fact.base);
                    let ty = self.arena.or(before, fact.ty);
                    joined.push(Fact { ty, ..fact });
                }
            }
        }
        self.facts.extend(after);
        self.facts.extend(joined);
    }

    fn constant_writes_maybe_default(&mut self, target: &AssignmentTargetMaybeDefault<'a>) {
        match target {
            AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(it) => {
                self.constant_writes(&it.binding);
                self.default_value(&it.init);
            }
            target => {
                if let Some(target) = target.as_assignment_target() {
                    self.constant_writes(target);
                }
            }
        }
    }

    /// `name = …` or `name++`, where `name` is a constant, which
    /// JavaScript refuses with a `TypeError` where it runs. Whether it is
    /// one, reported.
    fn constant_write(&mut self, id: &IdentifierReference<'a>) -> bool {
        if self.desugared(id.span.start) {
            return false;
        }
        let name = id.name.as_str();
        let message = match self.local(name).map(|local| local.binding) {
            Some(Binding::Const) => format!(
                "`{name}` is a constant, and keeps the value it is declared with: declare it with `let` to assign it"
            ),
            Some(Binding::Import) => format!(
                "`{name}` is imported, and only the module that exports it changes it: declare a `let` with its value to change it here"
            ),
            Some(Binding::Using) => format!(
                "`{name}` is declared with `using`, which disposes of the value it is given when its block ends: it is not assigned; declare another name"
            ),
            _ => return false,
        };
        self.report(Code::TsReadonly, message, id.span);
        true
    }

    /// Whether `call` is the desugar's call of `name`: `$is`, `$extract` or
    /// `$nomatch`.
    fn is_sugar(&self, call: &CallExpression<'a>, name: &str) -> bool {
        matches!(&call.callee, Expression::Identifier(id)
            if id.name == name && self.desugared(id.span.start))
    }

    /// A type guard `pattern` is: `when isRow:`.
    fn guard_of_callee(&mut self, pattern: &Expression<'a>) -> Option<(Guard, ())> {
        self.quiet += 1;
        let ty = self.expr(pattern, None);
        self.quiet -= 1;
        let Kind::Function(sig) = self.arena.kind(ty) else {
            return None;
        };
        sig.guard
            .filter(|guard| guard.param == 0)
            .map(|guard| (guard, ()))
    }

    /// `$is(s, p)`, which the desugar writes for a pattern that is a name
    /// or a path: `when Gauge.Access.Granted:`, `when isRow:`.
    fn is_test(&mut self, call: &CallExpression<'a>) -> Ty {
        for (n, argument) in call.arguments.iter().enumerate() {
            let Argument::SpreadElement(_) = argument else {
                let e = argument.to_expression();
                if n == 1
                    && let Expression::Identifier(id) = e.without_parentheses()
                {
                    self.pattern_name = Some(id.span);
                }
                self.expr(e, None);
                self.pattern_name = None;
                continue;
            };
        }
        Ty::BOOLEAN
    }

    /// `` `name` `` is not in scope. A pattern that is a name tests the
    /// value against it, which the name was likely meant to bind.
    fn not_in_scope(&mut self, name: &str, span: Span) -> Ty {
        if self.pattern_name == Some(span) {
            self.report(
                Code::TsNoAny,
                format!("`{name}` is not defined; to bind the value, write `let {name}`"),
                span,
            );
            return Ty::ERROR;
        }
        self.untyped(name, "it is not in scope", span)
    }

    /// `$nomatch(s as never)`, where no arm of a `match` matched: what is
    /// left of the subject there is what the arms leave out.
    fn no_match(&mut self, call: &CallExpression<'a>) -> Ty {
        let Some(Argument::TSAsExpression(cast)) = call.arguments.first() else {
            self.loose_args(&call.arguments);
            return Ty::NEVER;
        };
        let subject = &cast.expression;
        let left = self.expr(subject, None);
        let left = self.cases(left);
        if left == Ty::NEVER || left == Ty::ERROR {
            return Ty::NEVER;
        }
        let at = call.callee.span();
        let parts = self.arena.members(left);
        let wide = parts.iter().any(|part| match self.arena.kind(*part) {
            Kind::Str(_) | Kind::Num(_) | Kind::Bool(_) | Kind::Undefined | Kind::Null => false,
            Kind::Key { .. } => false,
            Kind::Record(fields) => !fields.iter().any(|f| self.arena.is_literal(f.ty)),
            _ => true,
        });
        if wide {
            let shown = self.arena.display(left, &|o| self.obj_name(o));
            self.report(
                Code::TsMatchExhaustive,
                format!("`match` on `{shown}` needs a `default:`"),
                at,
            );
            return Ty::NEVER;
        }
        let declared = self.path(subject).map_or(Ty::ERROR, |(_, _, _, base)| base);
        let case = self.case_text(parts[0], declared);
        let written = self.as_written(subject.span());
        self.report(
            Code::TsMatchExhaustive,
            format!(
                "`match` does not cover every case of `{written}`; for example, this case is not handled: `when {case}:`"
            ),
            at,
        );
        Ty::NEVER
    }

    /// How a `match` arm writes the case `part` is: a key of an enum named
    /// by the type the subject is declared as, a record by its literal
    /// fields.
    fn case_text(&self, part: Ty, declared: Ty) -> String {
        match self.arena.kind(part) {
            Kind::Key { object, key } => {
                let name = self.arena.members(declared).into_iter().find_map(|d| {
                    match self.arena.kind(d) {
                        Kind::Enum { object: o, name } if o == object => Some(name.clone()),
                        _ => None,
                    }
                });
                match name {
                    Some(name) => format!("{}.{name}.{key}", self.obj_name(*object)),
                    None => format!("{}.{key}", self.obj_name(*object)),
                }
            }
            Kind::Record(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .filter(|f| self.arena.is_literal(f.ty))
                    .map(|f| {
                        format!(
                            "{}: {}",
                            f.name,
                            self.arena.display(f.ty, &|_| String::new())
                        )
                    })
                    .collect();
                format!("{{ {} }}", fields.join(", "))
            }
            _ => self.arena.display(part, &|o| self.obj_name(o)),
        }
    }

    /// Whether `call` is `Array.isArray(…)`, of the global `Array`.
    fn is_array_test(&self, call: &CallExpression<'a>) -> bool {
        matches!(&call.callee, Expression::StaticMemberExpression(m)
            if m.property.name == "isArray"
                && matches!(&m.object, Expression::Identifier(id)
                    if id.name == "Array" && self.local("Array").is_none()))
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
                    "object" => self.arena.or(Ty::OBJECT, Ty::NULL),
                    // DECISION: what `typeof v === "function"` leaves is
                    // not called in the subset, so it is not narrowed.
                    _ => return,
                }
            } else {
                let parts = self.arena.members(now);
                let kept: Vec<Ty> = parts
                    .into_iter()
                    .filter_map(|part| {
                        if part == Ty::NON_NULLISH {
                            // Neither null nor undefined, so `"object"`
                            // leaves TypeScript's `object` alone.
                            return Some(match (equal, name) {
                                (true, "number") => Ty::NUMBER,
                                (true, "string") => Ty::STRING,
                                (true, "boolean") => Ty::BOOLEAN,
                                (true, "object") => Ty::OBJECT,
                                (true, "undefined") => Ty::NEVER,
                                _ => part,
                            });
                        }
                        let of = self.arena.type_of(part);
                        let same = of.is_none_or(|of| of == name);
                        (same == equal || of.is_none()).then_some(part)
                    })
                    .collect();
                self.arena.union(kept)
            };
            self.push_fact(key, local, ty, base);
            return;
        }
        // `p.kind === "a"`, of a union of records, or of the one record
        // earlier tests left of it.
        if let Expression::StaticMemberExpression(m) = subject.without_parentheses()
            && !m.optional
            && let Some(literal) = self.literal(other)
            && let Some((key, local, now, base)) = self.path(&m.object)
            && self
                .arena
                .members(now)
                .iter()
                .all(|part| matches!(self.arena.kind(*part), Kind::Record(_)))
        {
            let field = m.property.name.as_str();
            let parts = self.arena.members(now);
            let kept: Vec<Ty> = parts
                .into_iter()
                .filter(|part| {
                    let ty = self
                        .record_field(*part, field)
                        .map_or(Ty::ERROR, |found| found.0);
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
        let ty = if matches!(other, Expression::NullLiteral(_)) {
            if now == Ty::UNKNOWN {
                // `v !== null` leaves `{} | undefined`, as TypeScript says:
                // nothing can be read off it, and a later `typeof v ===
                // "object"` leaves no null.
                if equal {
                    Ty::NULL
                } else {
                    self.arena.or(Ty::NON_NULLISH, Ty::UNDEFINED)
                }
            } else if equal {
                self.arena.filter(now, |_, part| part == Ty::NULL)
            } else {
                self.arena.filter(now, |_, part| part != Ty::NULL)
            }
        } else if undefined && now == Ty::UNKNOWN {
            if equal {
                Ty::UNDEFINED
            } else {
                self.arena.or(Ty::NON_NULLISH, Ty::NULL)
            }
        } else if undefined {
            if equal {
                self.arena
                    .filter(now, |_, part| part == Ty::UNDEFINED || part == Ty::VOID)
            } else {
                self.arena.without_undefined(now)
            }
        } else if let Some(literal) = self.literal(other).or_else(|| self.enum_key(other)) {
            if equal {
                if !self.takes(now, literal) {
                    return;
                }
                literal
            } else {
                let parts = self.cases(now);
                self.arena.filter(parts, |_, part| part != literal)
            }
        } else {
            return;
        };
        self.push_fact(key, local, ty, base);
    }

    /// A key of an enum, written as a path: `Gauge.Access.Granted`.
    fn enum_key(&mut self, e: &Expression<'a>) -> Option<Ty> {
        if !matches!(e, Expression::StaticMemberExpression(_)) {
            return None;
        }
        self.quiet += 1;
        let ty = self.expr(e, None);
        self.quiet -= 1;
        matches!(self.arena.kind(ty), Kind::Key { .. }).then_some(ty)
    }

    /// `ty` as the cases it is made of: `boolean` as `true | false`, and
    /// an enum a QML file declares as its keys.
    ///
    /// DECISION: an enum of a native type is one value, as in `exhaustive`:
    /// Qt combines its keys as flags.
    fn cases(&mut self, ty: Ty) -> Ty {
        let mut parts = Vec::new();
        for part in self.arena.members(ty) {
            match self.arena.kind(part).clone() {
                Kind::Boolean => {
                    parts.push(self.arena.boolean(true));
                    parts.push(self.arena.boolean(false));
                }
                Kind::Enum { object, name } => match self.declared_keys(object, &name) {
                    Some(keys) => parts.extend(keys.into_iter().map(|key| {
                        self.arena.intern(Kind::Key {
                            object,
                            key: key.into(),
                        })
                    })),
                    None => parts.push(part),
                },
                _ => parts.push(part),
            }
        }
        // Not through `union`, which makes `true | false` `boolean` again.
        match parts.as_slice() {
            [] => Ty::NEVER,
            [one] => *one,
            _ => self.arena.intern(Kind::Union(parts.into_boxed_slice())),
        }
    }

    /// The keys of the enum `name` of the object `o`, where a QML file
    /// declares it.
    fn declared_keys(&self, o: u32, name: &str) -> Option<Vec<String>> {
        let kind = self.kind_of(o)?;
        self.c.types.enum_keys(&kind, name)
    }

    /// Facts of `left && right` being false (or of `||` being true, when
    /// `sense` is true): either `left` was, or `left` was not and `right`
    /// was. A path both say something of is what either says.
    fn either_facts(&mut self, left: &Expression<'a>, right: &Expression<'a>, sense: bool) {
        // `… && (x = s.k, true)`: the right is never what would make the
        // whole `sense`, so only the left can.
        if constant(right) == Some(!sense) {
            self.facts(left, sense);
            return;
        }
        let mark = self.facts.len();
        self.facts(left, sense);
        let first: Vec<Fact<'a>> = self.facts.drain(mark..).collect();
        if first.is_empty() {
            return;
        }
        self.facts(left, !sense);
        self.facts(right, sense);
        let second: Vec<Fact<'a>> = self.facts.drain(mark..).collect();
        let last = |facts: &[Fact<'a>], key: &str, local: Option<u32>| {
            facts
                .iter()
                .rev()
                .find(|fact| fact.key == key && fact.local == local)
                .copied()
        };
        let mut joined: Vec<Fact<'a>> = Vec::new();
        for fact in &first {
            if joined
                .iter()
                .any(|seen| seen.key == fact.key && seen.local == fact.local)
            {
                continue;
            }
            let (Some(one), Some(other)) = (
                last(&first, fact.key, fact.local),
                last(&second, fact.key, fact.local),
            ) else {
                continue;
            };
            let ty = self.arena.or(one.ty, other.ty);
            joined.push(Fact { ty, ..one });
        }
        self.facts.extend(joined);
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
        let params = self.params.take();
        let locals = self.locals.len();
        // `let`, `const`, functions and types are the block's before their
        // declarations run. What an `is` declares is bound where the `is`
        // is true.
        for statement in statements {
            if let Some(declaration) = statement.as_declaration()
                && !matches!(statement, Statement::VariableDeclaration(d) if self.declared_by_is(d))
            {
                self.bind_ahead(declaration);
            }
        }
        self.is_declared_twice(statements, locals, params);
        let mut live = true;
        for statement in statements {
            live &= self.statement(statement);
        }
        self.locals.truncate(locals);
        live
    }

    /// What an `is` in `statements` declares, and a name the block
    /// declares itself (bound from `locals`), or a parameter where the
    /// block is a function's or a `catch`'s (bound from `params`), or a
    /// scope between declares: the file would not load, or the `is`'s
    /// assignment would reach the other name.
    pub(super) fn is_declared_twice(
        &mut self,
        statements: &[Statement<'a>],
        locals: usize,
        params: Option<usize>,
    ) {
        // A `let` an `is` declares is the block's, as one written is: a
        // name the block declares itself, or a parameter where the block is
        // a function's or a `catch`'s, would be declared twice, which does
        // not load.
        for statement in statements {
            let Statement::VariableDeclaration(declaration) = statement else {
                continue;
            };
            if declaration.kind != VariableDeclarationKind::Let || !self.declared_by_is(declaration)
            {
                continue;
            }
            for declarator in &declaration.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    continue;
                };
                let name = id.name.as_str();
                let own = self.locals[locals..]
                    .iter()
                    .any(|local| local.name == name && local.binding != Binding::Type);
                let param = params.is_some_and(|from| {
                    self.locals[from..locals]
                        .iter()
                        .any(|local| local.name == name && local.binding == Binding::Param)
                });
                if own || param {
                    let what = if param {
                        "a parameter"
                    } else {
                        "a name of this block"
                    };
                    self.report(
                        Code::TsIsBinding,
                        format!(
                            "`{name}` is {what} already, and an `is` declares what it binds in the block around it: bind another name"
                        ),
                        id.span,
                    );
                }
            }
        }
        self.shadowed_is(statements);
    }

    /// An `is` whose name a scope between it and where what it binds is
    /// declared declares too (a `switch`'s clauses, a `for` head, a
    /// `catch` parameter, a function's parameters, a `match` arm): the
    /// assignment would reach that name instead, in its dead zone where
    /// its declaration has not run, or a parameter or a `const`.
    fn shadowed_is(&mut self, statements: &[Statement<'a>]) {
        let mut declared: Vec<(&'a str, VariableDeclarationKind)> = Vec::new();
        for statement in statements {
            if let Statement::VariableDeclaration(declaration) = statement
                && self.declared_by_is(declaration)
            {
                for declarator in &declaration.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                        declared.push((id.name.as_str(), declaration.kind));
                    }
                }
            }
        }
        if declared.is_empty() {
            return;
        }
        let found = {
            let written = |at| self.desugared(at);
            let mut shadows = Shadows::new(&written);
            for statement in statements {
                shadows.visit_statement(statement);
            }
            shadows.found
        };
        for shadow in found {
            let Some(&(_, kind)) = declared.iter().find(|(name, _)| *name == shadow.name) else {
                continue;
            };
            // DECISION: a `var` in the clauses or the head is the
            // function's, so it reaches past the `let` before them and the
            // file would not load, as with a `var` beside; with what an
            // `is … var` binds it is one variable.
            if (shadow.var && kind == VariableDeclarationKind::Var)
                || !shadow.written.iter().all(|&at| self.desugared(at))
            {
                continue;
            }
            let name = shadow.name;
            let message = match shadow.between {
                Between::Switch => format!(
                    "`{name}` is a name of this `switch`'s clauses already, and an `is` in them declares what it binds before the `switch`: bind another name"
                ),
                Between::For => format!(
                    "`{name}` is a name this `for` declares already, and an `is` in it declares what it binds before the `for`: bind another name"
                ),
                Between::Catch => format!(
                    "`{name}` is a name this `catch` declares already, and an `is` in its parameter declares what it binds before the `try`: bind another name"
                ),
                Between::Params => format!(
                    "`{name}` is a name this function's head declares already, and an `is` outside its block declares what it binds before the function: bind another name"
                ),
                Between::Arm => format!(
                    "`{name}` is a name this `match` arm binds already, and an `is` in the arm declares what it binds before the `match`: bind another name"
                ),
            };
            self.report(Code::TsIsBinding, message, shadow.span);
        }
    }

    pub(super) fn bind_dead(&mut self, pattern: &BindingPattern<'a>) {
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
                    // Taken apart, a file with no declarations is as untyped
                    // as read by `.`: not left to the quiet read below.
                    let name = match &property.key {
                        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                        PropertyKey::StringLiteral(s) => s.value.to_string(),
                        key => format!("[{}]", self.text(key.span())),
                    };
                    if self.script_member(ty, &name, property.key.span()) {
                        self.bind_pattern(&property.value, Ty::ERROR, binding);
                        continue;
                    }
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
                    let name = self.text(rest.span).to_owned();
                    self.script_member(ty, &name, rest.span);
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

    /// Whether `declaration` is the `let x;` (or, for a `var` pattern,
    /// `var x;`) the desugar writes before a statement whose `is` binds
    /// `x`.
    pub(super) fn declared_by_is(&self, declaration: &VariableDeclaration<'a>) -> bool {
        // Its names are the desugar's, written where the pattern names
        // them; a `var` keeps the pattern's own keyword.
        matches!(
            declaration.kind,
            VariableDeclarationKind::Let | VariableDeclarationKind::Var
        ) && declaration.declarations.iter().all(|declarator| {
            declarator.init.is_none() && self.desugared(declarator.id.span().start)
        })
    }

    pub(super) fn declaration(&mut self, declaration: &VariableDeclaration<'a>) {
        if self.declared_by_is(declaration) {
            if declaration.kind == VariableDeclarationKind::Var {
                let start = declaration.span.start;
                self.report(
                    Code::TsNoVar,
                    "`var` is the function's, wherever it is written; write `let` or `const`"
                        .into(),
                    Span::new(start, start + 3),
                );
            }
            for declarator in &declaration.declarations {
                if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                    self.bind(id.name.as_str(), Ty::NEVER, Binding::IsBound);
                }
            }
            return;
        }
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
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing => Binding::Using,
            VariableDeclarationKind::Let => Binding::Let,
        };
        let using = match declaration.kind {
            VariableDeclarationKind::Using => Some(false),
            VariableDeclarationKind::AwaitUsing => Some(true),
            _ => None,
        };
        for declarator in &declaration.declarations {
            let written = declarator
                .type_annotation
                .as_ref()
                .map(|annotation| self.annot(&annotation.type_annotation));
            let ty = match &declarator.init {
                Some(init) => {
                    let given = self.expr(init, written);
                    if let Some(asynchronous) = using {
                        self.disposable(
                            given,
                            asynchronous,
                            if asynchronous {
                                "`await using`"
                            } else {
                                "`using`"
                            },
                            init.span(),
                        );
                    }
                    match written {
                        Some(written) => {
                            let name = pattern_name(&declarator.id);
                            let word = self.word(written);
                            self.check_assign(name, written, &word, given, init.span());
                            written
                        }
                        None => {
                            // As TypeScript, only a literal written here
                            // widens: a variable given a value of a literal
                            // type keeps it.
                            let mut written = Vec::new();
                            self.fresh(init, &mut written);
                            if binding == Binding::Const {
                                // The desugar's `const $s = subject;` is the
                                // subject itself, as a name it tests is.
                                if !written.is_empty()
                                    && !self.desugared(declaration.span.start)
                                    && let BindingPattern::BindingIdentifier(id) = &declarator.id
                                {
                                    let fresh = self.arena.union(written);
                                    let first = self.next_local;
                                    self.bind_pattern(&declarator.id, given, binding);
                                    self.inferred.extend(first..self.next_local);
                                    if let Some(local) = self.local(id.name.as_str()) {
                                        self.fresh.push((local.id, fresh));
                                    }
                                    continue;
                                }
                                given
                            } else {
                                self.arena.widen_parts(given, &written)
                            }
                        }
                    }
                }
                None => match written {
                    Some(written) => written,
                    None => self.untyped(
                        pattern_name(&declarator.id),
                        "write its type or give it a value",
                        declarator.id.span(),
                    ),
                },
            };
            let first = self.next_local;
            self.bind_pattern(&declarator.id, ty, binding);
            if written.is_none() {
                self.inferred.extend(first..self.next_local);
            }
        }
    }

    /// The literal types `e` gives that are written in it, which a variable
    /// widens (TypeScript's fresh literal types), into `written`: a
    /// literal (`-1` and `+1` too), either side of `?:` or a logical operator, or a
    /// `const` given one. An object or array literal has widened its
    /// members already, and a template or other unary operator gives no
    /// literal.
    fn fresh(&mut self, e: &Expression<'a>, written: &mut Vec<Ty>) {
        match e.without_parentheses() {
            Expression::StringLiteral(s) => written.push(self.arena.str(s.value.as_str())),
            Expression::NumericLiteral(n) => written.push(self.arena.num(n.value)),
            Expression::BooleanLiteral(b) => written.push(self.arena.boolean(b.value)),
            Expression::UnaryExpression(u) => {
                if let Some(value) = signed_literal(u.operator, &u.argument) {
                    written.push(self.arena.num(value));
                }
            }
            Expression::ConditionalExpression(c) => {
                self.fresh(&c.consequent, written);
                self.fresh(&c.alternate, written);
            }
            Expression::LogicalExpression(l) => {
                self.fresh(&l.left, written);
                self.fresh(&l.right, written);
            }
            Expression::Identifier(id) => {
                if let Some(local) = self.local(id.name.as_str())
                    && let Some((_, fresh)) = self.fresh.iter().find(|(f, _)| *f == local.id)
                {
                    written.extend(self.arena.members(*fresh));
                }
            }
            _ => {}
        }
    }

    /// Walks a statement; whether what follows it is reached.
    pub(super) fn statement(&mut self, statement: &Statement<'a>) -> bool {
        match statement {
            Statement::BlockStatement(block) => self.stmts(&block.body),
            Statement::ExpressionStatement(statement) => {
                self.expr(&statement.expression, None);
                // `row(v)`, of an assertion function: it returns only where
                // `v` is what it asserts.
                if let Expression::CallExpression(call) = &statement.expression
                    && let Some((guard, subject)) = self.guard_of(call)
                    && guard.asserts
                    && self.asserts_through_typed(&call.callee)
                    && let Some((key, local, now, base)) = self.path(subject)
                {
                    let ty = self.narrow_guard(now, guard.ty, true);
                    self.push_fact(key, local, ty, base);
                }
                true
            }
            Statement::VariableDeclaration(declaration) => {
                self.declaration(declaration);
                true
            }
            Statement::FunctionDeclaration(function) => {
                self.nested += 1;
                self.function(function, None);
                self.nested -= 1;
                true
            }
            Statement::ClassDeclaration(class) => {
                let start = class.span.start;
                self.forbidden(
                    "class",
                    "write a function, or a component",
                    Span::new(start, start + 5),
                );
                true
            }
            Statement::WithStatement(statement) => {
                let start = statement.span.start;
                self.forbidden("with", "name the object", Span::new(start, start + 4));
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
                self.each(
                    &statement.left,
                    elem,
                    statement.right.span(),
                    &statement.body,
                );
                true
            }
            Statement::ForInStatement(statement) => {
                let start = statement.span.start;
                self.forbidden(
                    "for…in",
                    "write `for…of` over an array or a `Map`",
                    Span::new(start, start + 3),
                );
                self.expr(&statement.right, None);
                self.each(
                    &statement.left,
                    Ty::STRING,
                    statement.right.span(),
                    &statement.body,
                );
                true
            }
            Statement::ReturnStatement(statement) => {
                let declared = self.fns.last().and_then(|ctx| ctx.ret);
                let given = match &statement.argument {
                    Some(argument) => self.expr(argument, declared),
                    None => Ty::UNDEFINED,
                };
                let given = if self.fns.last().is_some_and(|ctx| ctx.asynchronous) {
                    web::awaited(&mut self.arena, given)
                } else {
                    given
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
                        // The parameter is the block's, as a function's
                        // are its body's: a `let` in it of the same name
                        // does not load.
                        if let Some(param) = &handler.param {
                            self.bind_pattern(&param.pattern, Ty::UNKNOWN, Binding::Param);
                        }
                        let params = self.params.replace(locals);
                        let live = self.stmts(&handler.body.body);
                        self.params = params;
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
                let tagged = self.tagged(&statement.discriminant);
                let locals = self.locals.len();
                let assigned = self.assigned.len();
                let mut covered = Vec::with_capacity(statement.cases.len());
                // What the subject may be in the body read next: its label,
                // or, where a case falls into it, the label joined with what
                // the case above left the subject as (which its body may
                // have assigned). `None` once a `default` or a label that is
                // not a literal is among them, which narrows nothing.
                let mut labels: Option<Ty> = None;
                let mut falling = false;
                for case in &statement.cases {
                    let mark = self.facts.len();
                    let label = case.test.as_ref().and_then(|test| {
                        let given = self.expr(test, Some(over));
                        covered.push(given);
                        self.arena.is_literal(given).then_some(given)
                    });
                    labels = match (falling, labels, label) {
                        (false, _, label) => label,
                        (true, Some(before), Some(label)) => Some(self.arena.or(before, label)),
                        _ => None,
                    };
                    if let Some((key, local, now, base)) = path
                        && let Some(labels) = labels
                        && self.takes(now, labels)
                    {
                        self.push_fact(key, local, labels, base);
                    }
                    // `switch (v.kind)`, of a union of records: `v` is the
                    // records whose `kind` is a label.
                    if let Some((key, local, now, base, field)) = tagged
                        && let Some(labels) = labels
                    {
                        let ty = self.records_tagged(now, field, labels);
                        self.push_fact(key, local, ty, base);
                    }
                    let mut live = true;
                    for statement in &case.consequent {
                        live &= self.statement(statement);
                    }
                    falling = live;
                    if let Some((key, local, now, _)) = path
                        && labels.is_some()
                    {
                        labels = Some(self.fact(key, local).unwrap_or(now));
                    }
                    self.facts.truncate(mark);
                }
                self.locals.truncate(locals);
                self.forget_assigned(assigned);
                if statement.cases.iter().all(|case| case.test.is_some()) {
                    self.exhaustive(over, &covered, statement.span);
                }
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

    /// The path of `v` in `v.field`, where `v` is a union of records: what
    /// a `switch` on the field narrows.
    #[allow(clippy::type_complexity)]
    fn tagged(&mut self, e: &Expression<'a>) -> Option<(&'a str, Option<u32>, Ty, Ty, &'a str)> {
        let Expression::StaticMemberExpression(m) = e.without_parentheses() else {
            return None;
        };
        if m.optional {
            return None;
        }
        let (key, local, now, base) = self.path(&m.object)?;
        let records = matches!(self.arena.kind(now), Kind::Union(_))
            && self
                .arena
                .members(now)
                .iter()
                .all(|part| matches!(self.arena.kind(*part), Kind::Record(_)));
        records.then_some((key, local, now, base, m.property.name.as_str()))
    }

    /// The records of `union` whose `field` may be one of `labels`.
    fn records_tagged(&mut self, union: Ty, field: &str, labels: Ty) -> Ty {
        let kept: Vec<Ty> = self
            .arena
            .members(union)
            .into_iter()
            .filter(|part| {
                let ty = self
                    .record_field(*part, field)
                    .map_or(Ty::ERROR, |found| found.0);
                self.takes(labels, ty) || self.takes(ty, labels)
            })
            .collect();
        self.arena.union(kept)
    }

    /// A `switch` with no `default` on a union of literals, whose cases
    /// gave `covered`: the parts it leaves out are reported.
    ///
    /// DECISION: a switch on a wider type, or on a QML enum, whose keys Qt
    /// combines as flags, is not held to cover it.
    fn exhaustive(&mut self, over: Ty, covered: &[Ty], span: Span) {
        let mut parts = Vec::new();
        for part in self.arena.members(over) {
            match self.arena.kind(part) {
                Kind::Str(_) | Kind::Num(_) | Kind::Bool(_) | Kind::Undefined => parts.push(part),
                Kind::Boolean => {
                    parts.push(self.arena.boolean(true));
                    parts.push(self.arena.boolean(false));
                }
                _ => return,
            }
        }
        let missing: Vec<String> = parts
            .into_iter()
            .filter(|part| !covered.contains(part))
            .map(|part| format!("`{}`", self.arena.display(part, &|_| String::new())))
            .collect();
        let (list, each) = match missing.as_slice() {
            [] => return,
            [_] => (listed(&missing), "it"),
            _ => (listed(&missing), "each"),
        };
        let start = span.start;
        self.report(
            Code::TsSwitchExhaustive,
            format!("this `switch` leaves out {list}: add a case for {each}, or a `default`"),
            Span::new(start, start + 6),
        );
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
        right: Span,
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
            let binding = match declaration.kind {
                VariableDeclarationKind::Const => Binding::Const,
                VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing => {
                    Binding::Using
                }
                VariableDeclarationKind::Var | VariableDeclarationKind::Let => Binding::Let,
            };
            // Each item a `for (using x of …)` takes is disposed of at the
            // end of its pass, so each must be disposable.
            let asynchronous = declaration.kind == VariableDeclarationKind::AwaitUsing;
            if (asynchronous || declaration.kind == VariableDeclarationKind::Using)
                && elem != Ty::ERROR
            {
                let who = if asynchronous {
                    "`for (await using …)`"
                } else {
                    "`for (using …)`"
                };
                self.disposable(elem, asynchronous, who, right);
            }
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
            TSType::TSAnyKeyword(any) => self.untyped("any", "write `unknown` or a type", any.span),
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
            // DECISION: a type predicate (`v is Row`) is a boolean to the
            // checker: what it narrows to is a decoder's (T3b).
            TSType::TSTypePredicate(_) => Ty::BOOLEAN,
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
                        if let Some(index) = self.named_module(left, qualified.span.start) {
                            return self.exported_type(index, right, qualified.right.span);
                        }
                        if self.module.is_some() {
                            return Ty::ERROR;
                        }
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
            ("Promise", [value]) => {
                let value = self.annot(value);
                return self.arena.intern(Kind::Promise(value));
            }
            _ => {}
        }
        if let Some(ty) = primitive(name) {
            return ty;
        }
        // A module names no QML type; a file's own types come before the
        // web's.
        if self.module.is_none()
            && let Some(found) = self.c.types.find(&[name])
        {
            return self.object_ty(&found.kind);
        }
        match Web::named(name) {
            Some(class) => self.arena.intern(Kind::Web(class)),
            None => Ty::ERROR,
        }
    }

    pub(super) fn record(&mut self, members: &[TSSignature<'a>]) -> Ty {
        let mut fields = Vec::with_capacity(members.len());
        for member in members {
            if let TSSignature::TSMethodSignature(method) = member
                && let PropertyKey::StaticIdentifier(id) = &method.key
            {
                let ty = self.signature(&method.params, method.return_type.as_deref());
                fields.push(Field {
                    name: id.name.as_str().into(),
                    ty,
                    optional: method.optional,
                    readonly: false,
                });
                continue;
            }
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
                readonly: property.readonly,
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

/// `as const`, which asserts nothing the value is not.
fn is_const(annotation: &TSType<'_>) -> bool {
    matches!(annotation, TSType::TSTypeReference(reference)
        if matches!(&reference.type_name, TSTypeName::IdentifierReference(id) if id.name == "const"))
}

/// The number a written `-1` or `+1` is: the one unary form TypeScript
/// gives a literal type.
fn signed_literal(operator: UnaryOperator, argument: &Expression<'_>) -> Option<f64> {
    let Expression::NumericLiteral(n) = argument else {
        return None;
    };
    match operator {
        UnaryOperator::UnaryNegation => Some(-n.value),
        UnaryOperator::UnaryPlus => Some(n.value),
        _ => None,
    }
}

/// The truth `e` always has, where its text says: `true`, `(x = v, true)`.
fn constant(e: &Expression<'_>) -> Option<bool> {
    match e.without_parentheses() {
        Expression::BooleanLiteral(b) => Some(b.value),
        Expression::SequenceExpression(sequence) => sequence.expressions.last().and_then(constant),
        _ => None,
    }
}
