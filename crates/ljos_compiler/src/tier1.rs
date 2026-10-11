//! Tier-1 code generation (ARCHITECTURE.md §5.4): an expression as a register
//! program for the engine's VM, where the whole of it is in the subset the VM
//! runs exactly as JavaScript would.
//!
//! The generator reads what the compiler has already resolved: the
//! expression's function in the file's module, whose free names are its
//! parameters, and [`Binding::params`], which say what each parameter is. So
//! a name here is a parameter, a variable of the script, or one of the few
//! globals the VM knows; nothing is looked up by name when the program runs.
//!
//! An expression is compiled whole or not at all. What is outside the subset
//! (a loop, a closure, a call of anything but a builtin or a function of the
//! file that can be inlined) leaves the expression to tier 2, with the
//! [`Reason`] for the coverage report.
//!
//! Two rules keep a program safe to abandon half way, which the VM does when
//! a value is not what it can handle exactly:
//!
//! - It reads in the order JavaScript reads, so what it has read when it
//!   gives up is what the JS function reads first.
//! - It writes a property at most once, and after the write it only moves
//!   values and returns: nothing that could fail follows an effect.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
};

use ljos_ir::{
    Arg, BinaryOp, Binding, BindingKind, Builtin, CompIdx, CompiledFile, Const, ExprId, InlineArg,
    ObjRef, Op, Reg, Tier1, UnaryOp,
};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionBody, AssignmentTarget, BindingPattern,
    CallExpression, ChainElement, ComputedMemberExpression, Declaration, Expression,
    FormalParameter, Function, IdentifierReference, Program, SimpleAssignmentTarget, Statement,
    StaticMemberExpression, TemplateLiteral, VariableDeclaration, VariableDeclarationKind,
};
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::operator::{
    AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
};

/// Functions of the file inlined inside one another, at most.
const MAX_INLINE_DEPTH: usize = 4;

/// A program longer than this is not worth a second form.
const MAX_CODE: usize = 4096;

/// Why an expression runs in tier 2.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Reason {
    /// The construct, in a few words: what the report groups by.
    pub kind: &'static str,
    /// Which one: the statement, the function called. May be empty.
    pub detail: String,
}

impl Reason {
    fn new(kind: &'static str) -> Self {
        Self {
            kind,
            detail: String::new(),
        }
    }

    fn with(kind: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.detail.is_empty() {
            f.write_str(self.kind)
        } else {
            write!(f, "{}: {}", self.kind, self.detail)
        }
    }
}

/// What became of one expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Tier1(Tier1),
    Tier2(Reason),
}

/// Gives every expression of the file that is in the subset its tier-1
/// program.
pub fn compile(file: &mut CompiledFile) {
    let outcomes = analyze(file);
    for (binding, outcome) in file.exprs.iter_mut().zip(outcomes) {
        binding.tier1 = match outcome {
            Outcome::Tier1(program) => Some(program),
            Outcome::Tier2(_) => None,
        };
    }
}

/// The tier of every expression of the file, in [`ExprId`] order, with why
/// for those left to tier 2.
pub fn analyze(file: &CompiledFile) -> Vec<Outcome> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &file.js.source, SourceType::mjs()).parse();
    if let Some(error) = parsed.diagnostics.first() {
        let reason = Reason::with("the module does not parse", error.message.to_string());
        return vec![Outcome::Tier2(reason); file.exprs.len()];
    }
    let module = Module::of(&parsed.program);
    (0..file.exprs.len())
        .map(|index| {
            let id = ExprId(index as u32);
            match Gen::new(file, &module).binding(id) {
                Ok(program) => Outcome::Tier1(program),
                Err(reason) => Outcome::Tier2(reason),
            }
        })
        .collect()
}

/// The file's module, by what it exports.
struct Module<'p, 'a> {
    functions: HashMap<&'p str, &'p Function<'a>>,
    /// The context of the file's own `qsTr`, when the module binds one.
    context: Option<String>,
}

impl<'p, 'a> Module<'p, 'a> {
    fn of(program: &'p Program<'a>) -> Self {
        let mut module = Module {
            functions: HashMap::new(),
            context: None,
        };
        for statement in &program.body {
            match statement {
                Statement::ExportDeclaration(export) => {
                    if let Declaration::FunctionDeclaration(function) = &export.declaration
                        && let Some(id) = &function.id
                    {
                        module.functions.insert(id.name.as_str(), &**function);
                    }
                }
                // `const qsTr = __ljos_qsTr("Context");`
                Statement::VariableDeclaration(declaration) => {
                    for declarator in &declaration.declarations {
                        if let BindingPattern::BindingIdentifier(id) = &declarator.id
                            && id.name == "qsTr"
                            && let Some(Expression::CallExpression(call)) = &declarator.init
                            && let [Argument::StringLiteral(context)] = &call.arguments[..]
                        {
                            module.context = Some(context.value.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        module
    }
}

/// A script as its function has it: the names of the call's arguments and
/// the code.
struct Shape<'p, 'a> {
    formals: Vec<&'p str>,
    /// The name a function expression gives itself.
    own: Option<&'p str>,
    body: Body<'p, 'a>,
}

#[derive(Clone, Copy)]
enum Body<'p, 'a> {
    Statements(&'p [Statement<'a>]),
    /// The body of an arrow function written without braces.
    Expression(&'p Expression<'a>),
}

fn plain<'p>(parameter: &'p FormalParameter<'_>) -> Option<&'p str> {
    if parameter.initializer.is_some() {
        return None;
    }
    match &parameter.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn odd_shape(what: &str) -> Reason {
    Reason::with("function shape", what)
}

/// Takes the wrapping the compiler gave a script apart (`compile.rs`,
/// `function`): the parameters for its free names, then either the names of
/// the call's arguments, or `...$args` applied to the function the script is.
fn shape<'p, 'a>(function: &'p Function<'a>, binding: &Binding) -> Result<Shape<'p, 'a>, Reason> {
    let Some(body) = &function.body else {
        return Err(odd_shape("no body"));
    };
    let items = &function.params.items;
    if binding.params.is_empty()
        && items.is_empty()
        && matches!(&body.statements[..], [Statement::ThrowStatement(_)])
    {
        return Err(Reason::new("the script does not parse"));
    }
    if items.len() < binding.params.len() {
        return Err(odd_shape("parameters"));
    }
    for (item, param) in items.iter().zip(&binding.params) {
        if plain(item) != Some(param.name.as_str()) {
            return Err(odd_shape("parameters"));
        }
    }
    let known = &items[binding.params.len()..];
    let names = |items: &'p [FormalParameter<'a>]| {
        items
            .iter()
            .map(|item| {
                plain(item).ok_or_else(|| Reason::with("parameter", "a pattern or a default value"))
            })
            .collect::<Result<Vec<_>, _>>()
    };
    if function.params.rest.is_none() {
        return Ok(Shape {
            formals: names(known)?,
            own: None,
            body: Body::Statements(&body.statements),
        });
    }
    // `return (FUNCTION).apply(this, $args);`
    let [Statement::ReturnStatement(ret)] = &body.statements[..] else {
        return Err(odd_shape("applied function"));
    };
    let Some(Expression::CallExpression(call)) =
        ret.argument.as_ref().map(Expression::without_parentheses)
    else {
        return Err(odd_shape("applied function"));
    };
    let Expression::StaticMemberExpression(apply) = &call.callee else {
        return Err(odd_shape("applied function"));
    };
    if !known.is_empty() || apply.property.name != "apply" || call.arguments.len() != 2 {
        return Err(odd_shape("applied function"));
    }
    match apply.object.without_parentheses() {
        Expression::FunctionExpression(inner) => {
            if inner.r#async || inner.generator {
                return Err(Reason::with("function", "async or a generator"));
            }
            if inner.params.rest.is_some() {
                return Err(Reason::with("parameter", "rest"));
            }
            let Some(body) = &inner.body else {
                return Err(odd_shape("no body"));
            };
            Ok(Shape {
                formals: names(&inner.params.items)?,
                own: inner.id.as_ref().map(|id| id.name.as_str()),
                body: Body::Statements(&body.statements),
            })
        }
        Expression::ArrowFunctionExpression(inner) => {
            if inner.r#async {
                return Err(Reason::with("function", "async or a generator"));
            }
            if inner.params.rest.is_some() {
                return Err(Reason::with("parameter", "rest"));
            }
            let body = match &inner.body {
                ArrowFunctionBody::FunctionBody(body) => Body::Statements(&body.statements),
                other => match other.as_expression() {
                    Some(expression) => Body::Expression(expression),
                    None => return Err(odd_shape("arrow body")),
                },
            };
            Ok(Shape {
                formals: names(&inner.params.items)?,
                own: None,
                body,
            })
        }
        _ => Err(odd_shape("applied function")),
    }
}

/// Every name the code refers to, declared in it or not.
#[derive(Default)]
struct Used(HashSet<String>);

impl<'a> Visit<'a> for Used {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.0.insert(it.name.to_string());
    }
}

/// A variable of the script.
#[derive(Clone, Copy)]
enum Var {
    Reg {
        reg: Reg,
        constant: bool,
    },
    /// An argument of the call the script never names. It is not loaded: it
    /// may be a value the VM would have to give up on.
    Unused,
}

/// A parameter of the function.
#[derive(Clone)]
enum Slot {
    /// Loaded when the function is entered. `object` when it is an object
    /// of the file.
    Reg {
        reg: Reg,
        object: Option<ObjRef>,
    },
    /// A type, by the index [`Op::EnumKey`] names it with: only its enum
    /// keys can be read. `singleton` when the name is a singleton's.
    Type {
        param: u16,
        singleton: bool,
    },
    Unusable(Reason),
}

/// What a name is where it is used.
enum Name {
    Var {
        reg: Reg,
        constant: bool,
    },
    Param {
        reg: Reg,
        object: Option<ObjRef>,
    },
    Type {
        param: u16,
        singleton: bool,
    },
    /// Nothing of the function's: a global, if anything.
    Global,
}

/// Where an inlined function returns to.
struct Ret {
    dst: Reg,
    /// The jumps to patch to the end of the body.
    jumps: Vec<usize>,
}

struct Frame {
    params: HashMap<String, Slot>,
    /// Innermost last.
    scopes: Vec<HashMap<String, Var>>,
    /// None for the expression itself: it returns from the program.
    ret: Option<Ret>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Continues,
    Returns,
}

/// What a write puts in the property.
enum Write<'p, 'a> {
    /// `=`, or with an operator `+=` and the like.
    Assign(Option<BinaryOp>, &'p Expression<'a>),
    /// `++`, `--`.
    Update { op: BinaryOp, prefix: bool },
}

/// What goes before the arguments of a builtin.
enum Lead<'p, 'a> {
    None,
    Receiver(&'p Expression<'a>),
    Context(String),
}

struct Gen<'p, 'a> {
    file: &'p CompiledFile,
    module: &'p Module<'p, 'a>,
    /// How many parameters the expression itself has: those of inlined
    /// functions are numbered after them.
    top_params: usize,
    code: Vec<Op>,
    consts: Vec<Const>,
    const_at: HashMap<Const, u16>,
    names: Vec<String>,
    name_at: HashMap<String, u16>,
    args: Vec<InlineArg>,
    next: u32,
    max: u32,
    frames: Vec<Frame>,
    /// A property has been written on the path here: nothing that can fail
    /// may follow.
    sealed: bool,
    /// The functions being compiled, the expression itself first.
    inlining: Vec<ExprId>,
    /// For each optional chain being compiled, the jumps that leave it.
    chains: Vec<Vec<usize>>,
}

impl<'p, 'a> Gen<'p, 'a> {
    fn new(file: &'p CompiledFile, module: &'p Module<'p, 'a>) -> Self {
        Gen {
            file,
            module,
            top_params: 0,
            code: Vec::new(),
            consts: Vec::new(),
            const_at: HashMap::new(),
            names: Vec::new(),
            name_at: HashMap::new(),
            args: Vec::new(),
            next: 0,
            max: 0,
            frames: Vec::new(),
            sealed: false,
            inlining: Vec::new(),
            chains: Vec::new(),
        }
    }

    fn function_of(&self, id: ExprId) -> Result<(&'p Binding, &'p Function<'a>), Reason> {
        let binding = self.file.expr(id);
        self.file
            .js
            .functions
            .get(binding.js_fn.index())
            .and_then(|f| self.module.functions.get(f.export.as_str()))
            .map(|function| (binding, *function))
            .ok_or_else(|| odd_shape("not in the module"))
    }

    fn binding(mut self, id: ExprId) -> Result<Tier1, Reason> {
        let (binding, function) = self.function_of(id)?;
        let shape = shape(function, binding)?;
        self.top_params = binding.params.len();
        self.inlining.push(id);
        self.enter(binding, &shape, None, None)?;
        if self.body(&shape)? == Flow::Continues {
            let src = self.alloc()?;
            self.constant(src, Const::Undefined)?;
            self.emit(Op::Return { src })?;
        }
        Ok(Tier1 {
            registers: u16::try_from(self.max).map_err(|_| Reason::new("too large"))?,
            consts: self.consts,
            names: self.names,
            args: self.args,
            code: self.code,
        })
    }

    // ---- registers, pools, code -------------------------------------------

    fn alloc(&mut self) -> Result<Reg, Reason> {
        self.alloc_n(1)
    }

    /// `count` registers in a row; the first.
    fn alloc_n(&mut self, count: usize) -> Result<Reg, Reason> {
        let first = self.next;
        let end = first as usize + count;
        if end >= Reg::MAX as usize {
            return Err(Reason::new("too large"));
        }
        self.next = end as u32;
        self.max = self.max.max(self.next);
        Ok(first as Reg)
    }

    fn emit(&mut self, op: Op) -> Result<usize, Reason> {
        if self.sealed
            && !matches!(
                op,
                Op::Const { .. }
                    | Op::Move { .. }
                    | Op::Pass { .. }
                    | Op::Jump { .. }
                    | Op::JumpIf { .. }
                    | Op::JumpIfNot { .. }
                    | Op::JumpIfNullish { .. }
                    | Op::Return { .. }
            )
        {
            return Err(Reason::new("more than one effect"));
        }
        if self.code.len() >= MAX_CODE {
            return Err(Reason::new("too large"));
        }
        self.code.push(op);
        Ok(self.code.len() - 1)
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    /// Points the jump at `at` to the next instruction.
    fn land(&mut self, at: usize) {
        let here = self.here();
        match &mut self.code[at] {
            Op::Jump { to }
            | Op::JumpIf { to, .. }
            | Op::JumpIfNot { to, .. }
            | Op::JumpIfNullish { to, .. } => *to = here,
            _ => {}
        }
    }

    fn constant(&mut self, dst: Reg, value: Const) -> Result<(), Reason> {
        let k = match self.const_at.get(&value) {
            Some(k) => *k,
            None => {
                let k = u16::try_from(self.consts.len()).map_err(|_| Reason::new("too large"))?;
                self.consts.push(value.clone());
                self.const_at.insert(value, k);
                k
            }
        };
        self.emit(Op::Const { dst, k })?;
        Ok(())
    }

    fn number(&mut self, dst: Reg, value: f64) -> Result<(), Reason> {
        self.constant(dst, Const::number(value))
    }

    fn name(&mut self, name: &str) -> Result<u16, Reason> {
        if let Some(index) = self.name_at.get(name) {
            return Ok(*index);
        }
        let index = u16::try_from(self.names.len()).map_err(|_| Reason::new("too large"))?;
        self.names.push(name.to_string());
        self.name_at.insert(name.to_string(), index);
        Ok(index)
    }

    /// The names of a path, in a row; where they start and how many.
    fn path(&mut self, names: &[&str]) -> Result<(u16, u8), Reason> {
        if let [one] = names {
            return Ok((self.name(one)?, 1));
        }
        let len = u8::try_from(names.len()).map_err(|_| Reason::new("too large"))?;
        let start = u16::try_from(self.names.len()).map_err(|_| Reason::new("too large"))?;
        if start as usize + names.len() > u16::MAX as usize {
            return Err(Reason::new("too large"));
        }
        self.names.extend(names.iter().map(|name| name.to_string()));
        Ok((start, len))
    }

    // ---- names ------------------------------------------------------------

    fn frame(&mut self) -> &mut Frame {
        // `enter` pushes one before any code is generated.
        let last = self.frames.len() - 1;
        &mut self.frames[last]
    }

    fn inlined(&self) -> bool {
        self.frames.len() > 1
    }

    fn lookup(&self, name: &str) -> Result<Name, Reason> {
        let Some(frame) = self.frames.last() else {
            return Ok(Name::Global);
        };
        for scope in frame.scopes.iter().rev() {
            match scope.get(name) {
                Some(Var::Reg { reg, constant }) => {
                    return Ok(Name::Var {
                        reg: *reg,
                        constant: *constant,
                    });
                }
                Some(Var::Unused) => return Err(odd_shape("an argument nothing was seen to use")),
                None => {}
            }
        }
        match frame.params.get(name) {
            Some(Slot::Reg { reg, object }) => Ok(Name::Param {
                reg: *reg,
                object: *object,
            }),
            Some(Slot::Type { param, singleton }) => Ok(Name::Type {
                param: *param,
                singleton: *singleton,
            }),
            Some(Slot::Unusable(reason)) => Err(reason.clone()),
            None => Ok(Name::Global),
        }
    }

    /// Whether the function has the name already: declaring it again would
    /// shadow it, and JavaScript's rules for that (hoisting, the dead zone)
    /// are not worth following here.
    fn visible(&self, name: &str) -> bool {
        self.frames.last().is_some_and(|frame| {
            frame.params.contains_key(name)
                || frame.scopes.iter().any(|scope| scope.contains_key(name))
        }) || is_known_global(name)
    }

    /// Sets up the frame of a function: its parameters loaded in the order
    /// the JS call evaluates them, then the arguments of the call. `inline`
    /// is the component an inlined function is declared in and the registers
    /// its arguments are in.
    fn enter(
        &mut self,
        binding: &'p Binding,
        shape: &Shape<'p, 'a>,
        inline: Option<(CompIdx, Vec<Reg>)>,
        ret: Option<Ret>,
    ) -> Result<(), Reason> {
        let mut frame = Frame {
            params: HashMap::new(),
            scopes: vec![HashMap::new()],
            ret,
        };
        for (index, param) in binding.params.iter().enumerate() {
            let number = match &inline {
                None => index,
                Some((component, _)) => {
                    self.args.push(InlineArg {
                        component: *component,
                        arg: param.arg.clone(),
                    });
                    self.top_params + self.args.len() - 1
                }
            };
            let number = u16::try_from(number).map_err(|_| Reason::new("too large"))?;
            let slot = match &param.arg {
                Arg::Holder { name, .. } => {
                    return Err(Reason::with("assignment to a name", name.clone()));
                }
                Arg::Namespace { .. } => Slot::Unusable(Reason::new("import namespace")),
                Arg::Type(_) => Slot::Type {
                    param: number,
                    singleton: false,
                },
                Arg::Singleton(_) => Slot::Type {
                    param: number,
                    singleton: true,
                },
                Arg::Object(_)
                | Arg::Property { .. }
                | Arg::Context { .. }
                | Arg::Attached { .. }
                | Arg::Dynamic(_) => {
                    let reg = self.alloc()?;
                    self.emit(Op::Param {
                        dst: reg,
                        param: number,
                    })?;
                    let object = match &param.arg {
                        Arg::Object(object) => Some(*object),
                        _ => None,
                    };
                    Slot::Reg { reg, object }
                }
            };
            frame.params.insert(param.name.clone(), slot);
        }
        if let Some(own) = shape.own {
            // Inside itself the name is the function, whatever else has it.
            frame.params.insert(
                own.to_string(),
                Slot::Unusable(Reason::with("function value", "its own name")),
            );
        }
        match inline {
            Some((_, registers)) => {
                for (name, reg) in shape.formals.iter().zip(registers) {
                    frame.scopes[0].insert(
                        name.to_string(),
                        Var::Reg {
                            reg,
                            constant: false,
                        },
                    );
                }
            }
            None => {
                let mut used = Used::default();
                match shape.body {
                    Body::Statements(statements) => {
                        for statement in statements {
                            used.visit_statement(statement);
                        }
                    }
                    Body::Expression(expression) => used.visit_expression(expression),
                }
                for (index, name) in shape.formals.iter().enumerate() {
                    let var = if used.0.contains(*name) {
                        let reg = self.alloc()?;
                        let index = u16::try_from(index).map_err(|_| Reason::new("too large"))?;
                        self.emit(Op::Local { dst: reg, index })?;
                        Var::Reg {
                            reg,
                            constant: false,
                        }
                    } else {
                        Var::Unused
                    };
                    frame.scopes[0].insert(name.to_string(), var);
                }
            }
        }
        self.frames.push(frame);
        Ok(())
    }

    fn body(&mut self, shape: &Shape<'p, 'a>) -> Result<Flow, Reason> {
        match shape.body {
            Body::Statements(statements) => self.statements(statements, true),
            Body::Expression(expression) => {
                self.give_back(Some(expression))?;
                Ok(Flow::Returns)
            }
        }
    }

    // ---- statements -------------------------------------------------------

    fn statements(&mut self, list: &'p [Statement<'a>], outermost: bool) -> Result<Flow, Reason> {
        let mut flow = Flow::Continues;
        for statement in list {
            if flow == Flow::Returns {
                // A declaration there would still take its name from the
                // code before it.
                return Err(Reason::with("statement", "code after a return"));
            }
            flow = self.statement(statement, outermost)?;
        }
        Ok(flow)
    }

    fn scoped(&mut self, statement: &'p Statement<'a>) -> Result<Flow, Reason> {
        let mark = self.next;
        self.frame().scopes.push(HashMap::new());
        let flow = self.statement(statement, false);
        self.frame().scopes.pop();
        self.next = mark;
        flow
    }

    fn statement(&mut self, statement: &'p Statement<'a>, outermost: bool) -> Result<Flow, Reason> {
        let mark = self.next;
        let what = match statement {
            Statement::EmptyStatement(_) => return Ok(Flow::Continues),
            Statement::ExpressionStatement(it) => {
                self.effect(&it.expression)?;
                self.next = mark;
                return Ok(Flow::Continues);
            }
            Statement::ReturnStatement(it) => {
                self.give_back(it.argument.as_ref())?;
                self.next = mark;
                return Ok(Flow::Returns);
            }
            Statement::BlockStatement(it) => {
                self.frame().scopes.push(HashMap::new());
                let flow = self.statements(&it.body, false);
                self.frame().scopes.pop();
                self.next = mark;
                return flow;
            }
            Statement::IfStatement(it) => {
                let cond = self.expr(&it.test, None)?;
                let skip = self.emit(Op::JumpIfNot { cond, to: 0 })?;
                self.next = mark;
                let before = self.sealed;
                let then = self.scoped(&it.consequent)?;
                let then_sealed = self.sealed;
                let Some(alternate) = &it.alternate else {
                    self.land(skip);
                    self.sealed = before || (then == Flow::Continues && then_sealed);
                    return Ok(Flow::Continues);
                };
                let end = match then {
                    Flow::Continues => Some(self.emit(Op::Jump { to: 0 })?),
                    Flow::Returns => None,
                };
                self.land(skip);
                self.sealed = before;
                let other = self.scoped(alternate)?;
                let other_sealed = self.sealed;
                if let Some(end) = end {
                    self.land(end);
                }
                // A path that returned has nothing after it to keep safe.
                self.sealed = (then == Flow::Continues && then_sealed)
                    || (other == Flow::Continues && other_sealed);
                return Ok(if then == Flow::Returns && other == Flow::Returns {
                    Flow::Returns
                } else {
                    Flow::Continues
                });
            }
            // Its registers are its variables': they last as long as the
            // block does.
            Statement::VariableDeclaration(it) => return self.declare(it, outermost),
            Statement::ForStatement(_) => "for",
            Statement::ForInStatement(_) => "for in",
            Statement::ForOfStatement(_) => "for of",
            Statement::WhileStatement(_) => "while",
            Statement::DoWhileStatement(_) => "do while",
            Statement::SwitchStatement(_) => "switch",
            Statement::TryStatement(_) => "try",
            Statement::ThrowStatement(_) => "throw",
            Statement::BreakStatement(_) => "break",
            Statement::ContinueStatement(_) => "continue",
            Statement::LabeledStatement(_) => "label",
            Statement::FunctionDeclaration(_) => "function declaration",
            Statement::ClassDeclaration(_) => "class",
            Statement::DebuggerStatement(_) => "debugger",
            Statement::WithStatement(_) => "with",
            _ => "other",
        };
        Err(Reason::with("statement", what))
    }

    fn declare(
        &mut self,
        it: &'p VariableDeclaration<'a>,
        outermost: bool,
    ) -> Result<Flow, Reason> {
        let constant = match it.kind {
            // `var` belongs to the whole function: only where it is written
            // at the top of one is it the same as `let` there.
            VariableDeclarationKind::Var if !outermost => {
                return Err(Reason::with("statement", "var in a block"));
            }
            VariableDeclarationKind::Var | VariableDeclarationKind::Let => false,
            VariableDeclarationKind::Const => true,
            _ => return Err(Reason::with("statement", "using")),
        };
        for declarator in &it.declarations {
            let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                return Err(Reason::new("destructuring"));
            };
            let name = id.name.as_str();
            if self.visible(name) {
                return Err(Reason::with("statement", "a variable that shadows a name"));
            }
            let reg = self.alloc()?;
            match &declarator.init {
                Some(init) => {
                    self.expr(init, Some(reg))?;
                }
                None => self.constant(reg, Const::Undefined)?,
            }
            if let Some(scope) = self.frame().scopes.last_mut() {
                scope.insert(name.to_string(), Var::Reg { reg, constant });
            }
        }
        Ok(Flow::Continues)
    }

    /// `return value`: from the program, or from an inlined function to
    /// where it was called.
    fn give_back(&mut self, value: Option<&'p Expression<'a>>) -> Result<(), Reason> {
        let dst = self.frame().ret.as_ref().map(|ret| ret.dst);
        match dst {
            None => {
                let src = match value {
                    Some(value) => self.value(value, None)?,
                    None => {
                        let src = self.alloc()?;
                        self.constant(src, Const::Undefined)?;
                        src
                    }
                };
                self.emit(Op::Return { src })?;
            }
            Some(dst) => {
                match value {
                    Some(value) => {
                        self.value(value, Some(dst))?;
                    }
                    None => self.constant(dst, Const::Undefined)?,
                }
                let jump = self.emit(Op::Jump { to: 0 })?;
                if let Some(ret) = &mut self.frame().ret {
                    ret.jumps.push(jump);
                }
            }
        }
        Ok(())
    }

    /// An expression where a statement is: the one place a script may
    /// assign.
    fn effect(&mut self, expression: &'p Expression<'a>) -> Result<(), Reason> {
        self.value(expression, None).map(|_| ())
    }

    /// As [`Gen::expr`], for an expression that is a statement or what a
    /// `return` gives: it may be an assignment.
    fn value(&mut self, expression: &'p Expression<'a>, dst: Option<Reg>) -> Result<Reg, Reason> {
        let inner = expression.without_parentheses();
        if !matches!(
            inner,
            Expression::AssignmentExpression(_) | Expression::UpdateExpression(_)
        ) {
            return self.expr(expression, dst);
        }
        let reg = self.assign(inner)?;
        match dst {
            Some(dst) if dst != reg => {
                self.emit(Op::Move { dst, src: reg })?;
                Ok(dst)
            }
            _ => Ok(reg),
        }
    }

    // ---- assignments ------------------------------------------------------

    fn variable(&self, name: &str) -> Result<Reg, Reason> {
        match self.lookup(name)? {
            Name::Var {
                constant: false,
                reg,
            } => Ok(reg),
            Name::Var { .. } => Err(Reason::with("assignment", "to a constant")),
            _ => Err(Reason::with("assignment", "to a name that is no variable")),
        }
    }

    /// An assignment or an update; the register its value is in.
    fn assign(&mut self, expression: &'p Expression<'a>) -> Result<Reg, Reason> {
        match expression {
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
                    AssignmentOperator::LogicalOr
                    | AssignmentOperator::LogicalAnd
                    | AssignmentOperator::LogicalNullish => {
                        return Err(Reason::with("operator", "a logical assignment"));
                    }
                };
                match &it.left {
                    AssignmentTarget::AssignmentTargetIdentifier(id) => {
                        let reg = self.variable(&id.name)?;
                        // Through a register of its own: the value may read
                        // the variable after part of it is computed.
                        let value = self.expr(&it.right, None)?;
                        match op {
                            None if value == reg => {}
                            None => {
                                self.emit(Op::Move {
                                    dst: reg,
                                    src: value,
                                })?;
                            }
                            Some(op) => {
                                self.emit(Op::Binary {
                                    op,
                                    dst: reg,
                                    a: reg,
                                    b: value,
                                })?;
                            }
                        }
                        Ok(reg)
                    }
                    AssignmentTarget::StaticMemberExpression(member) => {
                        self.set(member, Write::Assign(op, &it.right))
                    }
                    AssignmentTarget::ComputedMemberExpression(_) => {
                        Err(Reason::with("assignment", "to an element"))
                    }
                    _ => Err(Reason::new("destructuring")),
                }
            }
            Expression::UpdateExpression(it) => {
                let op = match it.operator {
                    UpdateOperator::Increment => BinaryOp::Add,
                    UpdateOperator::Decrement => BinaryOp::Sub,
                };
                match &it.argument {
                    SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                        let reg = self.variable(&id.name)?;
                        let old = self.alloc()?;
                        self.emit(Op::Unary {
                            op: UnaryOp::Plus,
                            dst: old,
                            src: reg,
                        })?;
                        let one = self.alloc()?;
                        self.number(one, 1.0)?;
                        self.emit(Op::Binary {
                            op,
                            dst: reg,
                            a: old,
                            b: one,
                        })?;
                        Ok(if it.prefix { reg } else { old })
                    }
                    SimpleAssignmentTarget::StaticMemberExpression(member) => self.set(
                        member,
                        Write::Update {
                            op,
                            prefix: it.prefix,
                        },
                    ),
                    SimpleAssignmentTarget::ComputedMemberExpression(_) => {
                        Err(Reason::with("assignment", "to an element"))
                    }
                    _ => Err(Reason::with("assignment", "to an expression")),
                }
            }
            _ => Err(Reason::with("assignment", "inside an expression")),
        }
    }

    /// A write to `target`, a member of an object; the register the value
    /// of the assignment is in.
    fn set(
        &mut self,
        target: &'p StaticMemberExpression<'a>,
        write: Write<'p, 'a>,
    ) -> Result<Reg, Reason> {
        if self.inlined() {
            return Err(Reason::new("write in a function called"));
        }
        if target.optional {
            return Err(Reason::with("assignment", "to an optional member"));
        }
        let member = target.property.name.as_str();
        let name = self.name(member)?;
        let old_wanted = !matches!(write, Write::Assign(None, _));
        // `__ljos_vt(object, "group").member`: what the compiler makes of
        // `object.group.member` where it is written.
        let (obj, group, old) = match value_at(&target.object) {
            Some((object, through)) => {
                let object = self.expr(object, None)?;
                let dst = self.alloc()?;
                let grouped = self.alloc()?;
                let through_name = self.name(through)?;
                self.emit(Op::Target {
                    dst,
                    grouped,
                    obj: object,
                    name: through_name,
                })?;
                let old = if old_wanted {
                    // From the object again: a grouped property has no
                    // object of its own to read the member of.
                    let old = self.alloc()?;
                    let (path, len) = self.path(&[through, member])?;
                    self.emit(Op::Get {
                        dst: old,
                        obj: object,
                        path,
                        len,
                    })?;
                    Some(old)
                } else {
                    None
                };
                (dst, Some((grouped, through_name)), old)
            }
            None => {
                let object = self.expr(&target.object, None)?;
                let old = if old_wanted {
                    let old = self.alloc()?;
                    self.emit(Op::Get {
                        dst: old,
                        obj: object,
                        path: name,
                        len: 1,
                    })?;
                    Some(old)
                } else {
                    None
                };
                (object, None, old)
            }
        };
        let (val, result) = match (write, old) {
            (Write::Assign(Some(op), value), Some(old)) => {
                let value = self.expr(value, None)?;
                let dst = self.alloc()?;
                self.emit(Op::Binary {
                    op,
                    dst,
                    a: old,
                    b: value,
                })?;
                (dst, dst)
            }
            (Write::Assign(_, value), _) => {
                let value = self.expr(value, None)?;
                (value, value)
            }
            (Write::Update { op, prefix }, Some(old)) => {
                let number = self.alloc()?;
                self.emit(Op::Unary {
                    op: UnaryOp::Plus,
                    dst: number,
                    src: old,
                })?;
                let one = self.alloc()?;
                self.number(one, 1.0)?;
                let dst = self.alloc()?;
                self.emit(Op::Binary {
                    op,
                    dst,
                    a: number,
                    b: one,
                })?;
                (dst, if prefix { dst } else { number })
            }
            (Write::Update { .. }, None) => return Err(odd_shape("update")),
        };
        self.emit(Op::Set {
            obj,
            name,
            val,
            group,
        })?;
        self.sealed = true;
        Ok(result)
    }

    // ---- expressions ------------------------------------------------------

    /// The register of a name that is in one already.
    fn register_of(&self, expression: &Expression<'a>) -> Option<Reg> {
        let Expression::Identifier(id) = expression.without_parentheses() else {
            return None;
        };
        match self.lookup(&id.name) {
            Ok(Name::Var { reg, .. } | Name::Param { reg, .. }) => Some(reg),
            _ => None,
        }
    }

    /// Whether a variable or an argument lives in the register: an operand
    /// may not be worked out in it, for the rest of the expression may read
    /// the name.
    fn named(&self, reg: Reg) -> bool {
        self.frames.iter().any(|frame| {
            let var = |var: &Var| matches!(var, Var::Reg { reg: it, .. } if *it == reg);
            let slot = |slot: &Slot| matches!(slot, Slot::Reg { reg: it, .. } if *it == reg);
            frame.scopes.iter().any(|scope| scope.values().any(var))
                || frame.params.values().any(slot)
        })
    }

    /// The first operand of an operator whose value goes to `dst`: worked
    /// out in `dst` itself where nothing else can see that register, so a
    /// chain of operators needs two registers and not one for each.
    fn operand(&mut self, expression: &'p Expression<'a>, dst: Reg) -> Result<Reg, Reason> {
        if self.register_of(expression).is_none() && !self.named(dst) {
            return self.expr(expression, Some(dst));
        }
        self.expr(expression, None)
    }

    /// Compiles an expression; the register its value is in. With `dst` it
    /// is that one. Without, the caller must only read the register: it may
    /// be a variable's.
    fn expr(&mut self, expression: &'p Expression<'a>, dst: Option<Reg>) -> Result<Reg, Reason> {
        let dst = match dst {
            Some(dst) => dst,
            None => match self.register_of(expression) {
                Some(reg) => return Ok(reg),
                None => self.alloc()?,
            },
        };
        let mark = self.next;
        self.expr_into(expression, dst)?;
        self.next = mark;
        Ok(dst)
    }

    fn expr_into(&mut self, expression: &'p Expression<'a>, dst: Reg) -> Result<(), Reason> {
        match expression {
            Expression::ParenthesizedExpression(it) => self.expr_into(&it.expression, dst),
            Expression::BooleanLiteral(it) => self.constant(dst, Const::Bool(it.value)),
            Expression::NullLiteral(_) => self.constant(dst, Const::Null),
            Expression::NumericLiteral(it) => self.number(dst, it.value),
            Expression::StringLiteral(it) => {
                if it.lone_surrogates {
                    return Err(Reason::with("literal", "a string that is not Unicode"));
                }
                self.constant(dst, Const::String(it.value.to_string()))
            }
            Expression::TemplateLiteral(it) => self.template(it, dst),
            Expression::Identifier(it) => self.identifier(&it.name, dst),
            Expression::ThisExpression(_) => {
                if self.inlined() {
                    return Err(Reason::new("this in a function called"));
                }
                self.emit(Op::This { dst })?;
                Ok(())
            }
            Expression::StaticMemberExpression(it) => self.member(it, dst),
            Expression::ComputedMemberExpression(it) => self.element(it, dst),
            Expression::ChainExpression(it) => {
                self.chains.push(Vec::new());
                let done = match &it.expression {
                    ChainElement::StaticMemberExpression(it) => self.member(it, dst),
                    ChainElement::ComputedMemberExpression(it) => self.element(it, dst),
                    ChainElement::CallExpression(it) => self.call(it, dst),
                    _ => Err(Reason::with("expression", "a private or typed member")),
                };
                let exits = self.chains.pop().unwrap_or_default();
                done?;
                if !exits.is_empty() {
                    let over = self.emit(Op::Jump { to: 0 })?;
                    for exit in exits {
                        self.land(exit);
                    }
                    self.constant(dst, Const::Undefined)?;
                    self.land(over);
                }
                Ok(())
            }
            Expression::UnaryExpression(it) => {
                let op = match it.operator {
                    UnaryOperator::Void => {
                        self.expr(&it.argument, None)?;
                        return self.constant(dst, Const::Undefined);
                    }
                    UnaryOperator::Delete => return Err(Reason::with("operator", "delete")),
                    UnaryOperator::UnaryNegation => UnaryOp::Neg,
                    UnaryOperator::UnaryPlus => UnaryOp::Plus,
                    UnaryOperator::LogicalNot => UnaryOp::Not,
                    UnaryOperator::BitwiseNot => UnaryOp::BitNot,
                    UnaryOperator::Typeof => UnaryOp::Typeof,
                };
                let src = self.operand(&it.argument, dst)?;
                self.emit(Op::Unary { op, dst, src })?;
                Ok(())
            }
            Expression::BinaryExpression(it) => {
                let op = match it.operator {
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
                    BinaryOperator::In => return Err(Reason::with("operator", "in")),
                    BinaryOperator::Instanceof => {
                        return Err(Reason::with("operator", "instanceof"));
                    }
                };
                let a = self.operand(&it.left, dst)?;
                let b = self.expr(&it.right, None)?;
                self.emit(Op::Binary { op, dst, a, b })?;
                Ok(())
            }
            Expression::LogicalExpression(it) => {
                self.expr(&it.left, Some(dst))?;
                match it.operator {
                    LogicalOperator::And => {
                        let end = self.emit(Op::JumpIfNot { cond: dst, to: 0 })?;
                        self.expr(&it.right, Some(dst))?;
                        self.land(end);
                    }
                    LogicalOperator::Or => {
                        let end = self.emit(Op::JumpIf { cond: dst, to: 0 })?;
                        self.expr(&it.right, Some(dst))?;
                        self.land(end);
                    }
                    LogicalOperator::Coalesce => {
                        let other = self.emit(Op::JumpIfNullish { src: dst, to: 0 })?;
                        let end = self.emit(Op::Jump { to: 0 })?;
                        self.land(other);
                        self.expr(&it.right, Some(dst))?;
                        self.land(end);
                    }
                }
                Ok(())
            }
            Expression::ConditionalExpression(it) => {
                let mark = self.next;
                let cond = self.expr(&it.test, None)?;
                let other = self.emit(Op::JumpIfNot { cond, to: 0 })?;
                self.next = mark;
                self.expr(&it.consequent, Some(dst))?;
                let end = self.emit(Op::Jump { to: 0 })?;
                self.land(other);
                self.expr(&it.alternate, Some(dst))?;
                self.land(end);
                Ok(())
            }
            Expression::SequenceExpression(it) => {
                let Some((last, before)) = it.expressions.split_last() else {
                    return self.constant(dst, Const::Undefined);
                };
                for expression in before {
                    let mark = self.next;
                    self.expr(expression, None)?;
                    self.next = mark;
                }
                self.expr_into(last, dst)
            }
            Expression::ArrayExpression(it) => {
                let count =
                    u16::try_from(it.elements.len()).map_err(|_| Reason::new("too large"))?;
                let base = self.alloc_n(it.elements.len())?;
                for (index, element) in it.elements.iter().enumerate() {
                    let element = match element {
                        ArrayExpressionElement::SpreadElement(_) => {
                            return Err(Reason::new("spread"));
                        }
                        ArrayExpressionElement::Elision(_) => {
                            return Err(Reason::with("literal", "an array with a hole"));
                        }
                        other => other
                            .as_expression()
                            .ok_or_else(|| Reason::with("literal", "an array with a hole"))?,
                    };
                    self.expr(element, Some(base + index as Reg))?;
                }
                self.emit(Op::Array { dst, base, count })?;
                Ok(())
            }
            Expression::CallExpression(it) => self.call(it, dst),
            Expression::AssignmentExpression(_) | Expression::UpdateExpression(_) => {
                Err(Reason::with("assignment", "inside an expression"))
            }
            Expression::ObjectExpression(_) => Err(Reason::new("object literal")),
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
                Err(Reason::new("function value"))
            }
            Expression::NewExpression(it) => Err(Reason::with("new", callee_text(&it.callee))),
            Expression::RegExpLiteral(_) => Err(Reason::with("literal", "a regular expression")),
            Expression::BigIntLiteral(_) => Err(Reason::with("literal", "a BigInt")),
            Expression::TaggedTemplateExpression(_) => {
                Err(Reason::with("literal", "a tagged template"))
            }
            Expression::AwaitExpression(_) => Err(Reason::with("expression", "await")),
            Expression::YieldExpression(_) => Err(Reason::with("expression", "yield")),
            Expression::ClassExpression(_) => Err(Reason::with("expression", "class")),
            _ => Err(Reason::with("expression", "other")),
        }
    }

    fn identifier(&mut self, name: &str, dst: Reg) -> Result<(), Reason> {
        match self.lookup(name)? {
            Name::Var { reg, .. } | Name::Param { reg, .. } => {
                if reg != dst {
                    self.emit(Op::Move { dst, src: reg })?;
                }
                Ok(())
            }
            Name::Type {
                singleton: true, ..
            } => Err(Reason::new("singleton as a value")),
            Name::Type { .. } => Err(Reason::new("type as a value")),
            Name::Global => match name {
                "undefined" => self.constant(dst, Const::Undefined),
                "NaN" => self.number(dst, f64::NAN),
                "Infinity" => self.number(dst, f64::INFINITY),
                _ => Err(Reason::with("global", name)),
            },
        }
    }

    fn template(&mut self, it: &'p TemplateLiteral<'a>, dst: Reg) -> Result<(), Reason> {
        enum Part<'p, 'a> {
            Text(String),
            Value(&'p Expression<'a>),
        }
        let mut parts = Vec::new();
        for (index, quasi) in it.quasis.iter().enumerate() {
            let Some(text) = &quasi.value.cooked else {
                return Err(Reason::with("literal", "a string that is not Unicode"));
            };
            if quasi.lone_surrogates {
                return Err(Reason::with("literal", "a string that is not Unicode"));
            }
            if !text.is_empty() {
                parts.push(Part::Text(text.to_string()));
            }
            if let Some(expression) = it.expressions.get(index) {
                parts.push(Part::Value(expression));
            }
        }
        if it.expressions.is_empty() {
            let text = match parts.pop() {
                Some(Part::Text(text)) => text,
                _ => String::new(),
            };
            return self.constant(dst, Const::String(text));
        }
        let count = u16::try_from(parts.len()).map_err(|_| Reason::new("too large"))?;
        let base = self.alloc_n(parts.len())?;
        for (index, part) in parts.into_iter().enumerate() {
            let reg = base + index as Reg;
            match part {
                Part::Text(text) => self.constant(reg, Const::String(text))?,
                Part::Value(expression) => {
                    self.expr(expression, Some(reg))?;
                }
            }
        }
        self.emit(Op::Concat { dst, base, count })?;
        Ok(())
    }

    /// A jump out of the optional chain being compiled, when `src` is
    /// `null` or `undefined`.
    fn leave_chain(&mut self, src: Reg) -> Result<(), Reason> {
        if self.chains.is_empty() {
            return Err(odd_shape("an optional member outside a chain"));
        }
        let jump = self.emit(Op::JumpIfNullish { src, to: 0 })?;
        if let Some(chain) = self.chains.last_mut() {
            chain.push(jump);
        }
        Ok(())
    }

    /// `base.a.b.c`: one read of the whole path.
    fn member(&mut self, it: &'p StaticMemberExpression<'a>, dst: Reg) -> Result<(), Reason> {
        let mut names = vec![it.property.name.as_str()];
        let mut optional = it.optional;
        let mut base = &it.object;
        // Not through parentheses: `(a?.b).c` is not part of the chain.
        while !optional && let Expression::StaticMemberExpression(inner) = base {
            names.push(inner.property.name.as_str());
            optional = inner.optional;
            base = &inner.object;
        }
        names.reverse();
        if let Expression::Identifier(id) = base {
            match self.lookup(&id.name)? {
                Name::Global => {
                    if let Some(rest) = self.global_member(&id.name, &names, dst)? {
                        if optional {
                            return Err(Reason::with("member", "optional, of a global"));
                        }
                        return self.rest(dst, rest);
                    }
                }
                Name::Type { param, singleton } => {
                    let key = names[0];
                    if optional || !is_key_name(key) {
                        // What a singleton has or a type attaches: tier 2
                        // gives the type's enums and nothing else, and the
                        // engine has no singleton instance to read yet.
                        return Err(if singleton {
                            Reason::new("member of a singleton")
                        } else {
                            Reason::with("member", "of a type, not an enum key")
                        });
                    }
                    let key = self.name(key)?;
                    self.emit(Op::EnumKey { dst, param, key })?;
                    return self.rest(dst, &names[1..]);
                }
                _ => {}
            }
        }
        let obj = self.expr(base, None)?;
        if optional {
            self.leave_chain(obj)?;
        }
        let (path, len) = self.path(&names)?;
        self.emit(Op::Get {
            dst,
            obj,
            path,
            len,
        })?;
        Ok(())
    }

    /// What is read off a value already in `dst`.
    fn rest(&mut self, dst: Reg, names: &[&str]) -> Result<(), Reason> {
        if names.is_empty() {
            return Ok(());
        }
        let (path, len) = self.path(names)?;
        self.emit(Op::Get {
            dst,
            obj: dst,
            path,
            len,
        })?;
        Ok(())
    }

    /// A member of a global the VM knows, into `dst`; the names still to
    /// read off it. `None` when the global is not one of those.
    fn global_member<'n, 's>(
        &mut self,
        global: &str,
        names: &'n [&'s str],
        dst: Reg,
    ) -> Result<Option<&'n [&'s str]>, Reason> {
        let first = names[0];
        let value = match global {
            "Math" => match first {
                "PI" => std::f64::consts::PI,
                "E" => std::f64::consts::E,
                "LN2" => std::f64::consts::LN_2,
                "LN10" => std::f64::consts::LN_10,
                "LOG2E" => std::f64::consts::LOG2_E,
                "LOG10E" => std::f64::consts::LOG10_E,
                "SQRT2" => std::f64::consts::SQRT_2,
                "SQRT1_2" => std::f64::consts::FRAC_1_SQRT_2,
                _ => return Err(Reason::with("member", format!("Math.{first}"))),
            },
            "Number" => match first {
                "MAX_VALUE" => f64::MAX,
                "MIN_VALUE" => f64::from_bits(1),
                "EPSILON" => f64::EPSILON,
                "MAX_SAFE_INTEGER" => 9_007_199_254_740_991.0,
                "MIN_SAFE_INTEGER" => -9_007_199_254_740_991.0,
                "POSITIVE_INFINITY" => f64::INFINITY,
                "NEGATIVE_INFINITY" => f64::NEG_INFINITY,
                "NaN" => f64::NAN,
                _ => return Err(Reason::with("member", format!("Number.{first}"))),
            },
            "Qt" => {
                if !is_qt_key(first) {
                    return Err(Reason::with("member", format!("Qt.{first}")));
                }
                let key = self.name(first)?;
                self.emit(Op::QtKey { dst, key })?;
                return Ok(Some(&names[1..]));
            }
            _ => return Ok(None),
        };
        self.number(dst, value)?;
        Ok(Some(&names[1..]))
    }

    /// `object[index]`.
    fn element(&mut self, it: &'p ComputedMemberExpression<'a>, dst: Reg) -> Result<(), Reason> {
        let obj = self.expr(&it.object, None)?;
        if it.optional {
            self.leave_chain(obj)?;
        }
        let index = self.expr(&it.expression, None)?;
        self.emit(Op::Index { dst, obj, index })?;
        Ok(())
    }

    // ---- calls ------------------------------------------------------------

    fn call(&mut self, it: &'p CallExpression<'a>, dst: Reg) -> Result<(), Reason> {
        if it.optional {
            return Err(Reason::with("function call", "optional"));
        }
        match &it.callee {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                if !matches!(self.lookup(name)?, Name::Global) {
                    return Err(Reason::with("function call", "of a value"));
                }
                let f = match name {
                    "qsTr" => {
                        // Without the module's own there is no context to
                        // look the text up in.
                        let Some(context) = self.module.context.clone() else {
                            return Err(Reason::with("global function call", name));
                        };
                        return self.builtin(Builtin::QsTr, Lead::Context(context), it, dst);
                    }
                    "qsTranslate" => Builtin::QsTranslate,
                    "qsTrId" => Builtin::QsTrId,
                    "QT_TR_NOOP" | "QT_TRID_NOOP" => Builtin::TrNoop,
                    "QT_TRANSLATE_NOOP" => Builtin::TranslateNoop,
                    "Number" => Builtin::Number,
                    "String" => Builtin::String,
                    "Boolean" => Builtin::Boolean,
                    "isNaN" => Builtin::IsNaN,
                    "isFinite" => Builtin::IsFinite,
                    "__ljos_vt" | "__ljos_vl" => {
                        return Err(Reason::new("write to a member of a value"));
                    }
                    "__ljos_att" => return Err(Reason::new("attached object of a value")),
                    _ => return Err(Reason::with("global function call", name)),
                };
                self.builtin(f, Lead::None, it, dst)
            }
            Expression::StaticMemberExpression(member) => {
                if member.optional {
                    return Err(Reason::with("function call", "optional"));
                }
                let method = member.property.name.as_str();
                let mut known_object = false;
                if let Expression::Identifier(id) = &member.object {
                    match self.lookup(&id.name)? {
                        Name::Global => {
                            let f = match id.name.as_str() {
                                "Math" => math(method),
                                "Qt" => qt(method),
                                _ => None,
                            };
                            return match f {
                                Some(f) => self.builtin(f, Lead::None, it, dst),
                                None => Err(Reason::with(
                                    "global function call",
                                    format!("{}.{method}", id.name),
                                )),
                            };
                        }
                        Name::Param {
                            reg,
                            object: Some(object),
                        } => {
                            if let Some(callee) = self.file_function(object, method) {
                                return self.inline(reg, method, callee, it, dst);
                            }
                            known_object = true;
                        }
                        Name::Type {
                            singleton: true, ..
                        } => {
                            return Err(Reason::with(
                                "method call of a singleton",
                                format!(".{method}()"),
                            ));
                        }
                        Name::Type { .. } => {
                            return Err(Reason::with("method call", format!(".{method}()")));
                        }
                        _ => {}
                    }
                }
                match method_builtin(method, it.arguments.len()) {
                    // An object of the file is no string and no number.
                    Some(f) if !known_object => {
                        self.builtin(f, Lead::Receiver(&member.object), it, dst)
                    }
                    _ => Err(Reason::with("method call", format!(".{method}()"))),
                }
            }
            _ => Err(Reason::with("function call", "of a value")),
        }
    }

    /// A call of a builtin: what leads, then the arguments, in registers in
    /// a row.
    fn builtin(
        &mut self,
        f: Builtin,
        lead: Lead<'p, 'a>,
        call: &'p CallExpression<'a>,
        dst: Reg,
    ) -> Result<(), Reason> {
        let leading = usize::from(!matches!(lead, Lead::None));
        let total = leading + call.arguments.len();
        let count = u8::try_from(total).map_err(|_| Reason::new("too large"))?;
        let base = self.alloc_n(total)?;
        match lead {
            Lead::None => {}
            Lead::Receiver(receiver) => {
                self.expr(receiver, Some(base))?;
                self.emit(Op::Check { src: base, f })?;
            }
            Lead::Context(context) => self.constant(base, Const::String(context))?,
        }
        for (index, argument) in call.arguments.iter().enumerate() {
            let argument = argument.as_expression().ok_or(Reason::new("spread"))?;
            self.expr(argument, Some(base + (leading + index) as Reg))?;
        }
        self.emit(Op::Call {
            dst,
            f,
            base,
            count,
        })?;
        Ok(())
    }

    /// The function an object of the file declares by that name: the last,
    /// as the engine has it.
    fn file_function(&self, object: ObjRef, name: &str) -> Option<ExprId> {
        let component = self.file.components.get(object.component.index())?;
        let def = component.objects.get(object.object.index())?;
        def.functions
            .iter()
            .rev()
            .find(|function| function.name == name)
            .map(|function| function.expr)
    }

    /// `object.method(...)` where the method is a function of the file: its
    /// body in place of the call. The program checks first that the object
    /// still has that very function.
    fn inline(
        &mut self,
        obj: Reg,
        method: &str,
        callee: ExprId,
        call: &'p CallExpression<'a>,
        dst: Reg,
    ) -> Result<(), Reason> {
        const KIND: &str = "call of a function that is tier 2";
        if self.inlining.contains(&callee) {
            return Err(Reason::with(KIND, "it calls itself"));
        }
        if self.frames.len() > MAX_INLINE_DEPTH {
            return Err(Reason::with(KIND, "calls too deep"));
        }
        let (binding, function) = self.function_of(callee)?;
        if binding.kind != BindingKind::Function {
            return Err(odd_shape("a method that is no function"));
        }
        let shape = shape(function, binding).map_err(|why| Reason::with(KIND, why.to_string()))?;
        let name = self.name(method)?;
        self.emit(Op::Guard {
            obj,
            name,
            expr: callee,
        })?;
        // Each argument in a register of its own: the function may assign
        // to its parameters.
        let mut registers = Vec::new();
        for argument in &call.arguments {
            let argument = argument.as_expression().ok_or(Reason::new("spread"))?;
            let reg = self.alloc()?;
            self.expr(argument, Some(reg))?;
            // The call goes through the engine: so does the argument.
            self.emit(Op::Pass { reg })?;
            registers.push(reg);
        }
        while registers.len() < shape.formals.len() {
            let reg = self.alloc()?;
            self.constant(reg, Const::Undefined)?;
            registers.push(reg);
        }
        self.inlining.push(callee);
        let ret = Ret {
            dst,
            jumps: Vec::new(),
        };
        let inner = (|| {
            self.enter(
                binding,
                &shape,
                Some((binding.scope.component, registers)),
                Some(ret),
            )?;
            if self.body(&shape)? == Flow::Continues {
                self.constant(dst, Const::Undefined)?;
            }
            Ok(())
        })();
        self.inlining.pop();
        inner.map_err(|why: Reason| {
            if why.kind == KIND {
                why
            } else {
                Reason::with(KIND, why.to_string())
            }
        })?;
        if let Some(frame) = self.frames.pop()
            && let Some(ret) = frame.ret
        {
            for jump in ret.jumps {
                self.land(jump);
            }
        }
        // And so does what it gives back.
        self.emit(Op::Pass { reg: dst })?;
        Ok(())
    }
}

/// `__ljos_vt(object, "name")`: the object and the name.
fn value_at<'p, 'a>(expression: &'p Expression<'a>) -> Option<(&'p Expression<'a>, &'p str)> {
    let Expression::CallExpression(call) = expression else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name != "__ljos_vt" {
        return None;
    }
    let [object, Argument::StringLiteral(name)] = &call.arguments[..] else {
        return None;
    };
    if name.lone_surrogates {
        return None;
    }
    Some((object.as_expression()?, name.value.as_str()))
}

/// The names the generator gives a meaning of its own.
fn is_known_global(name: &str) -> bool {
    matches!(
        name,
        "undefined"
            | "NaN"
            | "Infinity"
            | "Math"
            | "Qt"
            | "Number"
            | "String"
            | "Boolean"
            | "isNaN"
            | "isFinite"
            | "qsTr"
            | "qsTranslate"
            | "qsTrId"
            | "QT_TR_NOOP"
            | "QT_TRID_NOOP"
            | "QT_TRANSLATE_NOOP"
            | "arguments"
            | "__ljos_vt"
            | "__ljos_vl"
            | "__ljos_att"
    )
}

fn is_key_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// Whether `Qt.name` is one of the numbers the `Qt` object has: an enum key,
/// or one of the global colours. Anything else it has is a function or an
/// object.
fn is_qt_key(name: &str) -> bool {
    is_key_name(name)
        || matches!(
            name,
            "color0"
                | "color1"
                | "black"
                | "white"
                | "darkGray"
                | "gray"
                | "lightGray"
                | "red"
                | "green"
                | "blue"
                | "cyan"
                | "magenta"
                | "yellow"
                | "darkRed"
                | "darkGreen"
                | "darkBlue"
                | "darkCyan"
                | "darkMagenta"
                | "darkYellow"
                | "transparent"
        )
}

fn math(name: &str) -> Option<Builtin> {
    Some(match name {
        "abs" => Builtin::MathAbs,
        "acos" => Builtin::MathAcos,
        "asin" => Builtin::MathAsin,
        "atan" => Builtin::MathAtan,
        "atan2" => Builtin::MathAtan2,
        "cbrt" => Builtin::MathCbrt,
        "ceil" => Builtin::MathCeil,
        "cos" => Builtin::MathCos,
        "exp" => Builtin::MathExp,
        "floor" => Builtin::MathFloor,
        "log" => Builtin::MathLog,
        "log10" => Builtin::MathLog10,
        "log2" => Builtin::MathLog2,
        "max" => Builtin::MathMax,
        "min" => Builtin::MathMin,
        "pow" => Builtin::MathPow,
        "round" => Builtin::MathRound,
        "sign" => Builtin::MathSign,
        "sin" => Builtin::MathSin,
        "sqrt" => Builtin::MathSqrt,
        "tan" => Builtin::MathTan,
        "trunc" => Builtin::MathTrunc,
        _ => return None,
    })
}

fn qt(name: &str) -> Option<Builtin> {
    Some(match name {
        "rgba" => Builtin::QtRgba,
        "hsla" => Builtin::QtHsla,
        "hsva" => Builtin::QtHsva,
        "point" => Builtin::QtPoint,
        "size" => Builtin::QtSize,
        "rect" => Builtin::QtRect,
        "vector2d" => Builtin::QtVector2d,
        "vector3d" => Builtin::QtVector3d,
        "vector4d" => Builtin::QtVector4d,
        "quaternion" => Builtin::QtQuaternion,
        _ => return None,
    })
}

/// A method of a string or a number that the VM has, for a call with that
/// many arguments.
fn method_builtin(name: &str, arguments: usize) -> Option<Builtin> {
    Some(match (name, arguments) {
        ("arg", 1) => Builtin::Arg,
        ("toFixed", 0 | 1) => Builtin::ToFixed,
        ("toString", 0) => Builtin::ToString,
        ("toUpperCase", 0) => Builtin::ToUpperCase,
        ("toLowerCase", 0) => Builtin::ToLowerCase,
        ("trim", 0) => Builtin::Trim,
        _ => return None,
    })
}

/// What is called, as far as it can be said in a word: `Date`, `Qt.font`.
fn callee_text(callee: &Expression<'_>) -> String {
    match callee {
        Expression::Identifier(id) => id.name.to_string(),
        Expression::StaticMemberExpression(member) => match &member.object {
            Expression::Identifier(id) => format!("{}.{}", id.name, member.property.name),
            _ => format!(".{}", member.property.name),
        },
        _ => "a value".to_string(),
    }
}

// ---- coverage ---------------------------------------------------------------

/// How many expressions of a kind there are, and how many are tier 1.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub tier1: usize,
    pub total: usize,
}

impl Tally {
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            return 100.0;
        }
        self.tier1 as f64 * 100.0 / self.total as f64
    }

    fn add(&mut self, other: Tally) {
        self.tier1 += other.tier1;
        self.total += other.total;
    }
}

/// The tiers of one file's expressions, or of many files'.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counts {
    /// Property bindings.
    pub bindings: Tally,
    pub handlers: Tally,
    pub functions: Tally,
}

impl Counts {
    pub fn all(&self) -> Tally {
        let mut all = self.bindings;
        all.add(self.handlers);
        all.add(self.functions);
        all
    }

    fn add(&mut self, other: &Counts) {
        self.bindings.add(other.bindings);
        self.handlers.add(other.handlers);
        self.functions.add(other.functions);
    }
}

/// How many expressions of each kind are tier 2 for one reason.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Fell {
    pub bindings: usize,
    pub handlers: usize,
    pub functions: usize,
}

impl Fell {
    pub fn all(&self) -> usize {
        self.bindings + self.handlers + self.functions
    }

    fn add(&mut self, other: Fell) {
        self.bindings += other.bindings;
        self.handlers += other.handlers;
        self.functions += other.functions;
    }
}

/// Tier-1 coverage of a body of QML: per file, in total, and why the rest
/// is tier 2 (ARCHITECTURE.md §5.4, "coverage reporting").
#[derive(Debug, Default, Clone)]
pub struct Coverage {
    /// In the order they were added.
    pub files: Vec<(String, Counts)>,
    pub total: Counts,
    reasons: BTreeMap<Reason, Fell>,
}

impl Coverage {
    /// Adds a compiled file in, under `label`.
    pub fn add(&mut self, label: &str, file: &CompiledFile) {
        let mut counts = Counts::default();
        for (binding, outcome) in file.exprs.iter().zip(analyze(file)) {
            let mut one = Fell::default();
            let (tally, fell) = match binding.kind {
                BindingKind::Property => (&mut counts.bindings, &mut one.bindings),
                BindingKind::Handler => (&mut counts.handlers, &mut one.handlers),
                BindingKind::Function => (&mut counts.functions, &mut one.functions),
            };
            tally.total += 1;
            match outcome {
                Outcome::Tier1(_) => tally.tier1 += 1,
                Outcome::Tier2(reason) => {
                    *fell = 1;
                    self.reasons.entry(reason).or_default().add(one);
                }
            }
        }
        self.total.add(&counts);
        self.files.push((label.to_string(), counts));
    }

    /// Why expressions are tier 2, by construct, the most common first.
    pub fn reasons(&self) -> Vec<(&'static str, usize)> {
        self.reasons_by_kind()
            .into_iter()
            .map(|(kind, fell)| (kind, fell.all()))
            .collect()
    }

    /// As [`Coverage::reasons`], with how many of them are bindings,
    /// handlers and functions.
    pub fn reasons_by_kind(&self) -> Vec<(&'static str, Fell)> {
        let mut kinds: BTreeMap<&'static str, Fell> = BTreeMap::new();
        for (reason, fell) in &self.reasons {
            kinds.entry(reason.kind).or_default().add(*fell);
        }
        let mut kinds: Vec<_> = kinds.into_iter().collect();
        kinds.sort_by(|a, b| b.1.all().cmp(&a.1.all()).then(a.0.cmp(b.0)));
        kinds
    }

    /// The same in full (`global function call: console.log`), the most
    /// common first.
    pub fn details(&self) -> Vec<(String, usize)> {
        self.details_by_kind()
            .into_iter()
            .map(|(reason, fell)| (reason, fell.all()))
            .collect()
    }

    /// As [`Coverage::details`], with how many of them are bindings,
    /// handlers and functions.
    pub fn details_by_kind(&self) -> Vec<(String, Fell)> {
        let mut all: Vec<_> = self
            .reasons
            .iter()
            .map(|(reason, fell)| (reason.to_string(), *fell))
            .collect();
        all.sort_by(|a, b| b.1.all().cmp(&a.1.all()).then(a.0.cmp(&b.0)));
        all
    }
}
