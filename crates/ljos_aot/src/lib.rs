//! Bindings and handlers of a shipped QML file as Rust (ARCHITECTURE.md
//! §5.4, the amendment for shipped files).
//!
//! An application's build compiles its QML to the IR, and this crate turns
//! the expressions of each file into Rust functions the engine runs in
//! place of the dynamic tiers. It reads the IR and nothing else: the
//! compiler has lowered the module's functions to trees
//! ([`ljos_ir::aot::Module`]), and [`Types`] answers for the types the
//! runtime provides.
//!
//! # Typed by the declarations
//!
//! A value is a number, a bool or a string, and the generator knows which
//! from what the file declares (`property int ran`, `signal picked(value:
//! string)`) and from the engine's type table (`width` of an `Item` is a
//! `real`). There are no annotations in the source. Anything else (a `var`,
//! an object held by a property, a parameter with no type) is refused, by
//! name, and the expression runs in the dynamic tiers as before.
//!
//! # Exact or not at all
//!
//! The code means what the expression's JavaScript means, for the values
//! it lets through. Every object, property, signal and inlined function
//! the code uses is looked up before it does anything ([`Frame`] in the
//! runtime), so where the running program is not what the build saw, the
//! code stops before an effect and the dynamic tiers run the expression
//! whole. The operators keep JavaScript's arithmetic: IEEE doubles,
//! `ToInt32` for the bit operators, UTF-16 lengths and order for strings.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

pub use ljos_ir::aot::Refused;
use ljos_ir::aot::{BinaryOp, Body, Code, Expr, Module, Stmt, UnaryOp};
use ljos_ir::{
    Arg, Binding, BindingKind, CompiledFile, ComponentKind, ExprId, Members, ObjRef, ObjectDef,
    Resolved, TypeRef,
};

/// Functions of the file inlined inside one another, at most.
const MAX_INLINE_DEPTH: usize = 4;

/// Files a type is written in, one based on the next, followed at most.
const MAX_TYPE_DEPTH: usize = 16;

/// What the compiler calls to have a singleton's object
/// (`__ljos_single($s_Theme)`).
const SINGLETON: &str = "__ljos_single";

/// What the generator asks of the types the runtime provides.
pub trait Types {
    /// The property `name` of objects of the type `ty`: its type as the
    /// type table spells it (`real`, `string`), and whether it is
    /// read-only. `None` for a property the type does not have, or a type
    /// the table does not know.
    fn property(&self, ty: &TypeRef, name: &str) -> Option<PropertyType>;

    /// The application's file of the key `key` (`Theme`), compiled: what a
    /// type written in QML declares. `None` for one the build does not
    /// have, whose members are then refused.
    fn file(&self, key: &str) -> Option<&CompiledFile> {
        let _ = key;
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyType {
    pub ty: String,
    pub readonly: bool,
    /// The type of the object the property refers to (`parent`): what the
    /// code reads of that object is typed by it.
    pub object: Option<TypeRef>,
}

/// What became of one expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Compiled: the engine runs the Rust function.
    Compiled,
    Refused(Refused),
}

/// A file's compiled code.
#[derive(Debug, Clone)]
pub struct Generated {
    /// The items of a module: the file's table, `FILE`, and its functions.
    pub rust: String,
    /// By [`ExprId`].
    pub outcomes: Vec<Outcome>,
}

impl Generated {
    pub fn compiled(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| matches!(o, Outcome::Compiled))
            .count()
    }
}

/// Compiles the file whose content key is `content`. `rt` is the path the
/// runtime is at in the crate the code is built in (`::ljos::aot`).
pub fn generate(
    file: &CompiledFile,
    module: &Module,
    content: [u8; 32],
    types: &dyn Types,
    rt: &str,
) -> Generated {
    let mut sites = Sites::default();
    let mut functions = String::new();
    let mut outcomes = Vec::with_capacity(file.exprs.len());
    for index in 0..file.exprs.len() {
        let id = ExprId(index as u32);
        let mut gen_ = Gen {
            file,
            module,
            types,
            sites: &mut sites,
            objects: Vec::new(),
            vias: Vec::new(),
            props: Vec::new(),
            signals: Vec::new(),
            guards: Vec::new(),
            args: Vec::new(),
            frames: Vec::new(),
            labels: 0,
            locals: 0,
            effects: false,
        };
        let sites_before = gen_.sites.clone();
        match gen_.expression(id) {
            Ok(code) => {
                functions.push_str(&code);
                outcomes.push(Outcome::Compiled);
            }
            Err(why) => {
                // Sites only a refused expression used are not the file's.
                sites = sites_before;
                outcomes.push(Outcome::Refused(why));
            }
        }
    }
    let mut rust = String::new();
    let _ = writeln!(rust, "use {rt} as rt;");
    let _ = writeln!(rust);
    let _ = writeln!(rust, "pub static FILE: rt::File = rt::File {{");
    let _ = writeln!(rust, "    name: {:?},", file.file);
    let _ = writeln!(rust, "    content: {content:?},");
    let _ = writeln!(
        rust,
        "    context: {:?},",
        module.context.as_deref().unwrap_or("")
    );
    let _ = writeln!(rust, "    texts: &{:?},", sites.texts);
    rust.push_str("    props: &[");
    for (name, kind, write) in &sites.props {
        let _ = write!(
            rust,
            "\n        rt::PropSite {{ name: {name:?}, kind: rt::Kind::{}, write: {write} }},",
            kind.rust()
        );
    }
    rust.push_str("\n    ],\n");
    let _ = writeln!(rust, "    signals: &{:?},", sites.signals);
    rust.push_str("    singletons: &[");
    for ty in &sites.singletons {
        let _ = match ty {
            TypeRef::Native { module, name } => write!(
                rust,
                "\n        rt::Single::Native {{ module: {module:?}, name: {name:?} }},"
            ),
            TypeRef::Component { file, .. } => {
                write!(rust, "\n        rt::Single::File {{ file: {file:?} }},")
            }
            TypeRef::Unresolved(_) => unreachable!("a singleton site is of a resolved type"),
        };
    }
    rust.push_str("\n    ],\n");
    rust.push_str("    methods: &[");
    for (name, expr) in &sites.methods {
        let _ = write!(
            rust,
            "\n        rt::MethodSite {{ name: {name:?}, expr: {expr} }},"
        );
    }
    rust.push_str("\n    ],\n    exprs: &[");
    for (index, outcome) in outcomes.iter().enumerate() {
        match outcome {
            Outcome::Compiled => {
                let _ = write!(rust, "\n        Some(e{index}),");
            }
            Outcome::Refused(why) => {
                let _ = write!(rust, "\n        None, // {}", one_line(&why.to_string()));
            }
        }
    }
    rust.push_str("\n    ],\n};\n");
    rust.push_str(&functions);
    Generated { rust, outcomes }
}

/// The table of an application's compiled files, with each file's code in
/// a module of its own: what the build writes and the application
/// includes.
pub fn assemble(rt: &str, files: &[(&str, &Generated)]) -> String {
    let mut out = String::new();
    out.push_str(
        "// Generated by ljos_aot from the application's QML: do not edit.\n\
         #[allow(\n    unused_mut,\n    unused_variables,\n    unused_assignments,\n    \
         unreachable_code,\n    unused_labels,\n    unused_parens,\n    unused_braces,\n    clippy::all,\n    clippy::pedantic\n)]\nmod files {\n",
    );
    for (index, (_, generated)) in files.iter().enumerate() {
        let _ = writeln!(out, "pub mod f{index} {{");
        out.push_str(&generated.rust);
        out.push_str("}\n");
    }
    out.push_str("}\n\n");
    let _ = writeln!(out, "pub static COMPILED: &[&{rt}::File] = &[");
    for (index, (name, _)) in files.iter().enumerate() {
        let _ = writeln!(out, "    &files::f{index}::FILE, // {}", one_line(name));
    }
    out.push_str("];\n");
    out
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

/// What a site is read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Kind {
    Number,
    Bool,
    String,
    /// An object the code reads members of; never a value of its own.
    Object,
}

impl Kind {
    fn rust(self) -> &'static str {
        match self {
            Kind::Number => "Number",
            Kind::Bool => "Bool",
            Kind::String => "String",
            Kind::Object => "Object",
        }
    }

    fn ty(self) -> Ty {
        match self {
            Kind::Number => Ty::Num,
            Kind::Bool => Ty::Bool,
            Kind::String => Ty::Str,
            Kind::Object => Ty::Void,
        }
    }

    /// The kind of a type as the type table or a declaration spells it.
    fn of(ty: &str) -> Option<Kind> {
        let ty = ty.rsplit('.').next().unwrap_or(ty);
        Some(match ty {
            "int" | "real" | "double" | "number" | "qreal" | "float" | "uint" => Kind::Number,
            "bool" => Kind::Bool,
            "string" | "QString" => Kind::String,
            _ => return None,
        })
    }
}

/// The file's tables, shared by its expressions.
#[derive(Debug, Clone, Default)]
struct Sites {
    props: Vec<(String, Kind, bool)>,
    signals: Vec<String>,
    methods: Vec<(String, u32)>,
    texts: Vec<String>,
    /// The singleton types whose objects the code reads.
    singletons: Vec<TypeRef>,
}

fn site<T: PartialEq>(list: &mut Vec<T>, item: T) -> usize {
    match list.iter().position(|i| *i == item) {
        Some(at) => at,
        None => {
            list.push(item);
            list.len() - 1
        }
    }
}

/// The type of a value in the generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    /// `f64`.
    Num,
    Bool,
    /// `rt::Atom`.
    Str,
    /// `undefined`: `()`.
    Void,
    /// An object: only ever read from or called, never a value.
    Obj(Src),
    /// A name that is a type, for its enum keys.
    Type,
}

impl Ty {
    fn name(self) -> &'static str {
        match self {
            Ty::Num => "number",
            Ty::Bool => "bool",
            Ty::Str => "string",
            Ty::Void => "undefined",
            Ty::Obj(_) => "object",
            Ty::Type => "type",
        }
    }
}

/// Where the code has an object from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Src {
    /// An object of the file, by where it is.
    Here(ObjRef),
    /// A singleton type's object, by the type's site in the file's table.
    Single(usize),
    /// The object a property of another refers to (`parent`), by
    /// [`Gen::vias`].
    Via(usize),
}

/// What a member of an object is to the code.
enum Member {
    /// A value of the kind, and whether it can be written.
    Value(Kind, bool),
    /// An object of the type.
    Object(TypeRef),
}

/// Generated code and its type.
struct V {
    code: String,
    ty: Ty,
}

fn v(code: impl Into<String>, ty: Ty) -> V {
    V {
        code: code.into(),
        ty,
    }
}

fn no(kind: &str, detail: impl Into<String>) -> Refused {
    Refused::new(kind, detail)
}

#[derive(Clone)]
struct Local {
    rust: String,
    ty: Ty,
    constant: bool,
}

/// Where a `return` goes.
enum Ret {
    /// From the generated function.
    Out { value: bool },
    /// From an inlined function to its call: `break 'label value`.
    Inline {
        label: String,
        value: bool,
        ty: Option<Ty>,
    },
}

struct Frame<'p> {
    binding: &'p Binding,
    scopes: Vec<HashMap<String, Local>>,
    ret: Ret,
    /// The `switch` statements around, innermost last: what `break` leaves.
    switches: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Continues,
    Returns,
}

/// What an expression is generated for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Want {
    Value,
    /// Only for its effects: a statement.
    Effect,
}

struct Gen<'p> {
    file: &'p CompiledFile,
    module: &'p Module,
    types: &'p dyn Types,
    sites: &'p mut Sites,
    /// The objects the code uses: `o<index>`.
    objects: Vec<Src>,
    /// The objects read through a property: the object it is of (by
    /// `objects`), the property's site, and the type the table gives it.
    vias: Vec<(usize, usize, TypeRef)>,
    /// Properties: the object, the site, and so `p<index>`.
    props: Vec<(usize, usize)>,
    signals: Vec<(usize, usize)>,
    guards: Vec<(usize, usize)>,
    /// The arguments of the call the code reads: index, rust name, type.
    args: Vec<(usize, String, Ty)>,
    frames: Vec<Frame<'p>>,
    labels: usize,
    locals: usize,
    /// Whether the code writes or emits.
    effects: bool,
}

impl<'p> Gen<'p> {
    fn expression(&mut self, id: ExprId) -> Result<String, Refused> {
        let binding = self.file.expr(id);
        let body = self.body(id)?;
        let kind = binding.kind;
        // A function runs as it is called by the script that calls it,
        // whose arguments have no type the file declares.
        if kind == BindingKind::Function && !body.formals.is_empty() {
            return Err(no(
                "untyped parameter",
                format!("{} of a function", body.formals[0]),
            ));
        }
        let formals = match kind {
            BindingKind::Handler => self.handler_formals(id, body)?,
            _ => Vec::new(),
        };
        let mut scope = HashMap::new();
        for (index, (name, ty)) in body.formals.iter().zip(formals).enumerate() {
            // Read only where the code names it: an argument it never uses
            // may be anything.
            let rust = format!("a{index}");
            if let Some(ty) = ty {
                scope.insert(
                    name.clone(),
                    Local {
                        rust: rust.clone(),
                        ty,
                        constant: false,
                    },
                );
                self.args.push((index, rust, ty));
            } else {
                scope.insert(
                    name.clone(),
                    Local {
                        rust: String::new(),
                        ty: Ty::Void,
                        constant: true,
                    },
                );
            }
        }
        let value = kind != BindingKind::Handler;
        self.frames.push(Frame {
            binding,
            scopes: vec![scope],
            ret: Ret::Out { value },
            switches: Vec::new(),
        });
        let mut code = String::new();
        let flow = match &body.code {
            Code::Statements(list) => self.statements(list, &mut code)?,
            Code::Expression(e) => {
                self.give_back(Some(e), &mut code)?;
                Flow::Returns
            }
        };
        if flow == Flow::Continues {
            code.push_str("Ok(rt::Value::Undefined)\n");
        }
        self.frames.pop();
        if kind == BindingKind::Property && self.effects {
            return Err(no("a binding with an effect", ""));
        }
        // The object is found before the code runs, which is when a
        // script finds it only if nothing the code does moves it.
        if self.effects && !self.vias.is_empty() {
            return Err(no(
                "object read through a property",
                "in code that writes or emits",
            ));
        }

        // Everything the code uses, looked up before it does anything.
        let mut out = String::new();
        let _ = writeln!(
            out,
            "\n// {}:{}:{}\nfn e{}(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {{",
            self.file.file, binding.span.line, binding.span.column, id.0
        );
        for (index, src) in self.objects.iter().enumerate() {
            let _ = match src {
                Src::Here(r) => writeln!(
                    out,
                    "let o{index} = f.object({}, {})?;",
                    r.component.0, r.object.0
                ),
                Src::Single(at) => writeln!(out, "let o{index} = f.singleton({at})?;"),
                Src::Via(at) => {
                    let (of, site, _) = &self.vias[*at];
                    writeln!(out, "let o{index} = f.via(o{of}, {site})?;")
                }
            };
        }
        for (index, (object, at)) in self.props.iter().enumerate() {
            let _ = writeln!(out, "let p{index} = f.prop(o{object}, {at})?;");
        }
        for (index, (object, at)) in self.signals.iter().enumerate() {
            let _ = writeln!(out, "let s{index} = f.signal(o{object}, {at})?;");
        }
        for (object, at) in &self.guards {
            let _ = writeln!(out, "f.guard(o{object}, {at})?;");
        }
        let mut args = self.args.clone();
        args.sort_by_key(|(index, ..)| *index);
        for (index, rust, ty) in args {
            let read = match ty {
                Ty::Num => "arg_num",
                Ty::Bool => "arg_bool",
                Ty::Str => "arg_str",
                _ => continue,
            };
            let _ = writeln!(out, "let mut {rust} = f.{read}({index})?;");
        }
        out.push_str(&code);
        out.push_str("}\n");
        Ok(out)
    }

    fn body(&self, id: ExprId) -> Result<&'p Body, Refused> {
        match self.module.functions.get(id.index()) {
            Some(Ok(body)) => Ok(body),
            Some(Err(why)) => Err(why.clone()),
            None => Err(no("function shape", "not in the module")),
        }
    }

    /// The types of a handler's arguments, as its signal declares them.
    fn handler_formals(&self, id: ExprId, body: &Body) -> Result<Vec<Option<Ty>>, Refused> {
        if body.formals.is_empty() {
            return Ok(Vec::new());
        }
        let binding = self.file.expr(id);
        let def = self.file.object(binding.scope);
        let Some(signal) = handled(&def.members, id) else {
            return Err(no(
                "untyped parameter",
                format!(
                    "{} of a signal no object of the file declares",
                    body.formals[0]
                ),
            ));
        };
        let Some(decl) = def.signals.iter().rev().find(|s| s.name == signal) else {
            return Err(no(
                "untyped parameter",
                format!("{} of {signal}, a signal of a native type", body.formals[0]),
            ));
        };
        body.formals
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let Some(param) = decl.params.get(index) else {
                    return Ok(Some(Ty::Void));
                };
                match param.ty.as_deref() {
                    None => Err(no("untyped parameter", name.clone())),
                    Some(ty) => match Kind::of(ty) {
                        Some(kind) => Ok(Some(kind.ty())),
                        None => Err(no("type", format!("{ty} parameter {name}"))),
                    },
                }
            })
            .collect()
    }

    fn frame(&mut self) -> &mut Frame<'p> {
        self.frames.last_mut().expect("a frame while generating")
    }

    fn label(&mut self, what: &str) -> String {
        self.labels += 1;
        format!("'{what}{}", self.labels)
    }

    fn local(&mut self) -> String {
        self.locals += 1;
        format!("l{}", self.locals)
    }

    fn object(&mut self, src: Src) -> usize {
        site(&mut self.objects, src)
    }

    // ---- statements -------------------------------------------------------

    fn statements(&mut self, list: &'p [Stmt], out: &mut String) -> Result<Flow, Refused> {
        let mut flow = Flow::Continues;
        for s in list {
            if flow == Flow::Returns {
                return Err(no("statement", "code after a return"));
            }
            flow = self.statement(s, out)?;
        }
        Ok(flow)
    }

    fn scoped(&mut self, list: &'p [Stmt], out: &mut String) -> Result<Flow, Refused> {
        self.frame().scopes.push(HashMap::new());
        out.push_str("{\n");
        let flow = self.statements(list, out);
        out.push_str("}\n");
        self.frame().scopes.pop();
        flow
    }

    fn statement(&mut self, s: &'p Stmt, out: &mut String) -> Result<Flow, Refused> {
        match s {
            Stmt::Expr(e) => {
                let e = self.expr(e, Want::Effect)?;
                let _ = writeln!(out, "let _ = {};", e.code);
                Ok(Flow::Continues)
            }
            Stmt::Let {
                name,
                constant,
                init,
            } => {
                let Some(init) = init else {
                    return Err(no("statement", "a variable declared without a value"));
                };
                if self.visible(name) {
                    return Err(no("statement", "a variable that shadows a name"));
                }
                let init = self.expr(init, Want::Value)?;
                if !matches!(init.ty, Ty::Num | Ty::Bool | Ty::Str) {
                    return Err(no("type", format!("{} variable {name}", init.ty.name())));
                }
                let rust = self.local();
                let _ = writeln!(out, "let mut {rust} = {};", init.code);
                let local = Local {
                    rust,
                    ty: init.ty,
                    constant: *constant,
                };
                if let Some(scope) = self.frame().scopes.last_mut() {
                    scope.insert(name.clone(), local);
                }
                Ok(Flow::Continues)
            }
            Stmt::If {
                test,
                then,
                otherwise,
            } => {
                let test = self.test(test)?;
                let _ = write!(out, "if {test} ");
                let a = self.scoped(then, out)?;
                out.push_str("else ");
                let b = self.scoped(otherwise, out)?;
                Ok(if a == Flow::Returns && b == Flow::Returns {
                    Flow::Returns
                } else {
                    Flow::Continues
                })
            }
            Stmt::Return(value) => {
                self.give_back(value.as_ref(), out)?;
                Ok(Flow::Returns)
            }
            Stmt::Switch { on, cases } => self.switch(on, cases, out),
            Stmt::Break => {
                let Some(label) = self.frame().switches.last().cloned() else {
                    return Err(no("statement", "break out of anything but a switch"));
                };
                let _ = writeln!(out, "break {label};");
                Ok(Flow::Continues)
            }
            Stmt::Block(list) => self.scoped(list, out),
        }
    }

    /// Cases compared with `===` in order; once one matches, its code and
    /// every case's after it run until a `break`.
    fn switch(
        &mut self,
        on: &'p Expr,
        cases: &'p [(Option<Expr>, Vec<Stmt>)],
        out: &mut String,
    ) -> Result<Flow, Refused> {
        if let Some(at) = cases.iter().position(|(test, _)| test.is_none())
            && at + 1 != cases.len()
        {
            return Err(no("statement", "a switch whose default is not last"));
        }
        // A case's variable belongs to the whole switch and is seen by the
        // cases after it, but each case is its own Rust block; hoisting it
        // would lose the error a read before its declaration throws.
        if cases
            .iter()
            .any(|(_, body)| body.iter().any(|s| matches!(s, Stmt::Let { .. })))
        {
            return Err(no("statement", "a variable declared in a switch case"));
        }
        let on = self.expr(on, Want::Value)?;
        if !matches!(on.ty, Ty::Num | Ty::Bool | Ty::Str) {
            return Err(no("type", format!("switch on a {}", on.ty.name())));
        }
        let label = self.label("s");
        let (d, m) = (self.local(), self.local());
        let _ = writeln!(
            out,
            "{label}: {{\nlet {d} = {};\nlet mut {m} = false;",
            on.code
        );
        self.frame().switches.push(label);
        let mut flow = Flow::Continues;
        let mut breaks = false;
        for (test, body) in cases {
            breaks |= body.iter().any(breaks_out);
            match test {
                Some(test) => {
                    let test = self.expr(test, Want::Value)?;
                    let same = strict_equal(&v(d.clone(), on.ty), &test, true);
                    let _ = write!(out, "if {m} || {same} {{\n{m} = true;\n");
                    flow = self.statements(body, out)?;
                    out.push_str("}\n");
                }
                None => {
                    out.push_str("{\n");
                    flow = self.statements(body, out)?;
                    out.push_str("}\n");
                }
            }
        }
        self.frame().switches.pop();
        out.push_str("}\n");
        let has_default = cases.last().is_some_and(|(test, _)| test.is_none());
        // Every way through ends at the default's return, or at a return
        // before it.
        Ok(if has_default && !breaks && flow == Flow::Returns {
            Flow::Returns
        } else {
            Flow::Continues
        })
    }

    fn give_back(&mut self, value: Option<&'p Expr>, out: &mut String) -> Result<(), Refused> {
        let want = match &self.frame().ret {
            Ret::Out { value } | Ret::Inline { value, .. } => *value,
        };
        // An object is a value only as what the code gives back: the
        // engine writes it to the property as it does a script's.
        let out_value = matches!(self.frame().ret, Ret::Out { value: true });
        if let Some(e) = value
            && out_value
            && let Some(src) = self.object_of(e)?
        {
            let object = self.object(src);
            let _ = writeln!(out, "return Ok(o{object}.value());");
            return Ok(());
        }
        let got = match value {
            Some(e) => self.expr(e, if want { Want::Value } else { Want::Effect })?,
            None => v("()", Ty::Void),
        };
        let ret = &mut self.frame().ret;
        match ret {
            Ret::Out { value } => {
                if !*value {
                    let _ = writeln!(
                        out,
                        "let _ = {};\nreturn Ok(rt::Value::Undefined);",
                        got.code
                    );
                    return Ok(());
                }
                let as_value = match got.ty {
                    Ty::Num => format!("rt::Value::Real({})", got.code),
                    Ty::Bool => format!("rt::Value::Bool({})", got.code),
                    Ty::Str => format!("rt::Value::String({})", got.code),
                    Ty::Void => format!("{{ let _ = {}; rt::Value::Undefined }}", got.code),
                    other => return Err(no("type", format!("{} as a result", other.name()))),
                };
                let _ = writeln!(out, "return Ok({as_value});");
            }
            Ret::Inline { label, value, ty } => {
                if !*value {
                    let _ = writeln!(out, "let _ = {};\nbreak {label};", got.code);
                    return Ok(());
                }
                match ty {
                    Some(t) if *t != got.ty => {
                        return Err(no(
                            "a function that returns more than one type",
                            format!("{} and {}", t.name(), got.ty.name()),
                        ));
                    }
                    _ => *ty = Some(got.ty),
                }
                if !matches!(got.ty, Ty::Num | Ty::Bool | Ty::Str | Ty::Void) {
                    return Err(no("type", format!("{} as a result", got.ty.name())));
                }
                let _ = writeln!(out, "break {label} {};", got.code);
            }
        }
        Ok(())
    }

    // ---- names --------------------------------------------------------------

    fn visible(&self, name: &str) -> bool {
        let frame = self.frames.last().expect("a frame");
        frame.scopes.iter().any(|s| s.contains_key(name))
            || frame.binding.params.iter().any(|p| p.name == name)
    }

    fn lookup_local(&self, name: &str) -> Option<Local> {
        let frame = self.frames.last()?;
        frame.scopes.iter().rev().find_map(|s| s.get(name).cloned())
    }

    /// What a name is: a local, or a parameter the engine passes.
    fn name(&mut self, name: &str) -> Result<V, Refused> {
        if let Some(local) = self.lookup_local(name) {
            if local.rust.is_empty() {
                return Err(no("untyped parameter", name));
            }
            return Ok(v(local.rust, local.ty));
        }
        let frame = self.frames.last().expect("a frame");
        if let Some(param) = frame.binding.params.iter().find(|p| p.name == name) {
            return match &param.arg {
                Arg::Object(r) => Ok(v(String::new(), Ty::Obj(Src::Here(*r)))),
                Arg::Type(_) => Ok(v(String::new(), Ty::Type)),
                Arg::Singleton(_) => Err(no("singleton", name)),
                Arg::Attached { .. } => Err(no("attached object", name)),
                Arg::Namespace { .. } => Err(no("import namespace", name)),
                Arg::Context { .. } | Arg::Holder { .. } | Arg::Property { .. } => {
                    Err(no("context property", name))
                }
                Arg::Dynamic(_) => Err(no("a name found at run time", name)),
            };
        }
        match name {
            "Math" | "Qt" => Ok(v(String::new(), Ty::Type)),
            _ => Err(no("global", name)),
        }
    }

    /// `Qt.Key_1`, `Text.AlignLeft`, `Text.WrapMode.WordWrap`: a number the
    /// compiler has already resolved.
    fn enum_key(&self, base: &Expr, key: &str) -> Option<i64> {
        let (name, scope) = match base {
            Expr::Name(name) => (name.as_str(), None),
            Expr::Member(inner, scope) => match &**inner {
                Expr::Name(name) => (name.as_str(), Some(scope.as_str())),
                _ => return None,
            },
            _ => return None,
        };
        let frame = self.frames.last()?;
        frame.binding.refs.iter().find_map(|r| match &r.resolved {
            Resolved::Enum(e)
                if r.name == name
                    && e.key == key
                    && e.scope.as_deref() == scope.or(e.scope.as_deref()) =>
            {
                if scope.is_some() && e.scope.as_deref() != scope {
                    return None;
                }
                e.value
            }
            _ => None,
        })
    }

    // ---- properties -------------------------------------------------------

    /// `__ljos_single($s_Theme)`: the object of the singleton type the
    /// parameter stands for.
    fn single(&mut self, e: &Expr) -> Option<Src> {
        match e {
            Expr::Call { callee, args } => self.single_call(callee, args),
            _ => None,
        }
    }

    fn single_call(&mut self, callee: &Expr, args: &[Expr]) -> Option<Src> {
        let (Expr::Name(callee), [Expr::Name(param)]) = (callee, args) else {
            return None;
        };
        if callee != SINGLETON || self.visible(callee) || self.lookup_local(param).is_some() {
            return None;
        }
        let frame = self.frames.last()?;
        let ty = frame.binding.params.iter().find_map(|p| match &p.arg {
            Arg::Singleton(ty) if p.name == *param => Some(ty),
            _ => None,
        })?;
        match ty {
            TypeRef::Native { .. } | TypeRef::Component { inline: None, .. } => {}
            _ => return None,
        }
        let ty = ty.clone();
        Some(Src::Single(site(&mut self.sites.singletons, ty)))
    }

    /// The object a member is read from or written to: a name, or a
    /// singleton's.
    fn object_of(&mut self, base: &Expr) -> Result<Option<Src>, Refused> {
        if let Some(src) = self.single(base) {
            return Ok(Some(src));
        }
        if let Expr::Name(b) = base
            && let Ty::Obj(src) = self.name(b)?.ty
        {
            return Ok(Some(src));
        }
        if let Expr::Member(inner, name) = base
            && self.enum_key(inner, name).is_none()
            && let Some(src) = self.object_of(inner)?
            && let Member::Object(ty) = self.property_of(src, name)?
        {
            let of = self.object(src);
            let at = site(&mut self.sites.props, (name.clone(), Kind::Object, false));
            let via = site(&mut self.vias, (of, at, ty));
            return Ok(Some(Src::Via(via)));
        }
        Ok(None)
    }

    /// The kind of `name` on the object `src`, and whether it can be
    /// written.
    fn property_of(&self, src: Src, name: &str) -> Result<Member, Refused> {
        match src {
            Src::Here(r) => {
                let def = self.file.object(r);
                match declared_on(def, name)? {
                    Some((kind, writable)) => Ok(Member::Value(kind, writable)),
                    None => self.property_of_type(&def.ty, name),
                }
            }
            Src::Single(at) => self.property_of_type(&self.sites.singletons[at], name),
            Src::Via(at) => self.property_of_type(&self.vias[at].2, name),
        }
    }

    /// The kind of `name` on objects of `ty`: declared by the file the
    /// type is written in, by one it is based on, or by the native type
    /// under them.
    fn property_of_type(&self, ty: &TypeRef, name: &str) -> Result<Member, Refused> {
        let mut ty = ty;
        for _ in 0..MAX_TYPE_DEPTH {
            let (file, inline) = match ty {
                TypeRef::Native { .. } => break,
                TypeRef::Unresolved(t) => return Err(no("unresolved type", t.clone())),
                TypeRef::Component { file, inline } => (file, inline),
            };
            let other = if *file == self.file.file {
                Some(self.file)
            } else {
                self.types.file(file)
            };
            let Some(other) = other else {
                return Err(no("a type in a file the build does not have", file.clone()));
            };
            let component = match inline {
                None => other.components.first(),
                Some(inline) => other
                    .components
                    .iter()
                    .find(|c| c.kind == ComponentKind::Inline && c.name == *inline),
            };
            let Some(component) = component else {
                return Err(no("unresolved type", format!("{file}.{inline:?}")));
            };
            let root = component.root();
            if let Some((kind, writable)) = declared_on(root, name)? {
                return Ok(Member::Value(kind, writable));
            }
            ty = &root.ty;
        }
        if !matches!(ty, TypeRef::Native { .. }) {
            return Err(no(
                "type",
                format!("based on more than {MAX_TYPE_DEPTH} files"),
            ));
        }
        let Some(found) = self.types.property(ty, name) else {
            return Err(no("member the type table does not have", name));
        };
        match (Kind::of(&found.ty), found.object) {
            (Some(kind), _) => Ok(Member::Value(kind, !found.readonly)),
            (None, Some(ty)) => Ok(Member::Object(ty)),
            (None, None) => Err(no("type", format!("{} property {name}", found.ty))),
        }
    }

    fn prop(
        &mut self,
        src: Src,
        name: &str,
        write: bool,
    ) -> Result<(String, String, Kind), Refused> {
        let Member::Value(kind, writable) = self.property_of(src, name)? else {
            return Err(no("object as a value", name));
        };
        if write && !writable {
            return Err(no("write to a read-only property", name));
        }
        let object = self.object(src);
        let at = site(&mut self.sites.props, (name.to_string(), kind, write));
        let index = site(&mut self.props, (object, at));
        Ok((format!("o{object}"), format!("p{index}"), kind))
    }

    fn read(&mut self, src: Src, name: &str) -> Result<V, Refused> {
        let (o, p, kind) = self.prop(src, name, false)?;
        let read = match kind {
            Kind::Number => "num",
            Kind::Bool => "bool",
            Kind::String => "str",
            Kind::Object => return Err(no("object as a value", name)),
        };
        Ok(v(format!("f.{read}({o}, {p})?"), kind.ty()))
    }

    // ---- expressions ------------------------------------------------------

    /// A condition: whatever its type, whether it is truthy. `&&` and `||`
    /// here need not give a value of one type.
    fn test(&mut self, e: &'p Expr) -> Result<String, Refused> {
        if let Expr::Logical { and, a, b } = e {
            let (a, b) = (self.test(a)?, self.test(b)?);
            return Ok(format!("({a} {} {b})", if *and { "&&" } else { "||" }));
        }
        if let Expr::Unary(UnaryOp::Not, inner) = e {
            return Ok(format!("!({})", self.test(inner)?));
        }
        let got = self.expr(e, Want::Value)?;
        truthy(&got)
    }

    fn expr(&mut self, e: &'p Expr, want: Want) -> Result<V, Refused> {
        Ok(match e {
            Expr::Number(n) => v(number(*n), Ty::Num),
            Expr::String(s) => v(format!("rt::Atom::from({s:?})"), Ty::Str),
            Expr::Bool(b) => v(b.to_string(), Ty::Bool),
            Expr::Undefined => v("()", Ty::Void),
            Expr::Null => return Err(no("null", "")),
            Expr::Name(name) => {
                let got = self.name(name)?;
                match got.ty {
                    Ty::Obj(_) => return Err(no("object as a value", name.clone())),
                    Ty::Type => return Err(no("type as a value", name.clone())),
                    _ => {}
                }
                if got.ty == Ty::Str {
                    v(format!("{}.clone()", got.code), Ty::Str)
                } else {
                    got
                }
            }
            Expr::Member(base, name) => self.member(base, name)?,
            Expr::Call { callee, args } => self.call(callee, args, want)?,
            Expr::Unary(op, inner) => self.unary(*op, inner)?,
            Expr::Binary(op, a, b) => {
                let a = self.expr(a, Want::Value)?;
                let b = self.expr(b, Want::Value)?;
                binary(*op, a, b)?
            }
            Expr::Logical { and, a, b } => {
                if want == Want::Effect {
                    let a = self.test(a)?;
                    let b = self.expr(b, Want::Effect)?;
                    let not = if *and { "" } else { "!" };
                    return Ok(v(
                        format!("if {not}({a}) {{ let _ = {}; }}", b.code),
                        Ty::Void,
                    ));
                }
                let a = self.expr(a, Want::Value)?;
                let b = self.expr(b, Want::Value)?;
                if a.ty != b.ty || !matches!(a.ty, Ty::Num | Ty::Bool | Ty::Str) {
                    return Err(no(
                        "operator",
                        format!(
                            "{} of a {} and a {}",
                            if *and { "&&" } else { "||" },
                            a.ty.name(),
                            b.ty.name()
                        ),
                    ));
                }
                let x = self.local();
                let t = truthy(&v(x.clone(), a.ty))?;
                let (yes, otherwise) = if *and {
                    (b.code, x.clone())
                } else {
                    (x.clone(), b.code)
                };
                let x_code = if a.ty == Ty::Str {
                    format!("{x}.clone()")
                } else {
                    x.clone()
                };
                let (yes, otherwise) = if *and {
                    (yes, x_code)
                } else {
                    (x_code, otherwise)
                };
                v(
                    format!(
                        "{{ let {x} = {}; if {t} {{ {yes} }} else {{ {otherwise} }} }}",
                        a.code
                    ),
                    a.ty,
                )
            }
            Expr::Conditional {
                test,
                then,
                otherwise,
            } => {
                let test = self.test(test)?;
                let a = self.expr(then, want)?;
                let b = self.expr(otherwise, want)?;
                if want == Want::Effect {
                    v(
                        format!(
                            "if {test} {{ let _ = {}; }} else {{ let _ = {}; }}",
                            a.code, b.code
                        ),
                        Ty::Void,
                    )
                } else {
                    if a.ty != b.ty || !matches!(a.ty, Ty::Num | Ty::Bool | Ty::Str | Ty::Void) {
                        return Err(no(
                            "a conditional of two types",
                            format!("{} and {}", a.ty.name(), b.ty.name()),
                        ));
                    }
                    v(
                        format!("if {test} {{ {} }} else {{ {} }}", a.code, b.code),
                        a.ty,
                    )
                }
            }
            Expr::Assign { op, target, value } => self.assign(*op, target, value)?,
            Expr::Update {
                increment,
                prefix,
                target,
            } => self.update(*increment, *prefix, target)?,
        })
    }

    fn member(&mut self, base: &'p Expr, name: &str) -> Result<V, Refused> {
        if let Some(n) = self.enum_key(base, name) {
            return Ok(v(number(n as f64), Ty::Num));
        }
        if let Some(src) = self.object_of(base)? {
            return self.read(src, name);
        }
        if let Expr::Name(b) = base
            && self.name(b)?.ty == Ty::Type
        {
            return Err(no("member of a type", format!("{b}.{name}")));
        }
        let got = self.expr(base, Want::Value)?;
        match (got.ty, name) {
            (Ty::Str, "length") => Ok(v(format!("rt::js::length(&{})", got.code), Ty::Num)),
            (ty, _) => Err(no("member of a value", format!("{}.{name}", ty.name()))),
        }
    }

    fn unary(&mut self, op: UnaryOp, inner: &'p Expr) -> Result<V, Refused> {
        if op == UnaryOp::Not {
            return Ok(v(format!("!({})", self.test(inner)?), Ty::Bool));
        }
        let a = self.expr(inner, Want::Value)?;
        Ok(match (op, a.ty) {
            (UnaryOp::Neg, Ty::Num) => v(format!("(-{})", a.code), Ty::Num),
            (UnaryOp::Plus, Ty::Num) => a,
            (UnaryOp::Plus, Ty::Bool) => {
                v(format!("(if {} {{ 1.0 }} else {{ 0.0 }})", a.code), Ty::Num)
            }
            (UnaryOp::BitNot, Ty::Num) => v(format!("rt::js::bit_not({})", a.code), Ty::Num),
            (UnaryOp::Typeof, ty @ (Ty::Num | Ty::Bool | Ty::Str | Ty::Void)) => {
                let name = match ty {
                    Ty::Num => "number",
                    Ty::Bool => "boolean",
                    Ty::Str => "string",
                    _ => "undefined",
                };
                v(
                    format!("{{ let _ = {}; rt::Atom::from({name:?}) }}", a.code),
                    Ty::Str,
                )
            }
            (op, ty) => return Err(no("operator", format!("{op:?} of a {}", ty.name()))),
        })
    }

    fn assign(
        &mut self,
        op: Option<BinaryOp>,
        target: &'p Expr,
        value: &'p Expr,
    ) -> Result<V, Refused> {
        match target {
            Expr::Name(name) => {
                let Some(local) = self.lookup_local(name) else {
                    return Err(no("assignment to a name", name.clone()));
                };
                if local.constant || local.rust.is_empty() {
                    return Err(no("assignment", format!("to {name}")));
                }
                let value = self.expr(value, Want::Value)?;
                let new = match op {
                    None => value,
                    Some(op) => binary(op, v(clone_of(&local.rust, local.ty), local.ty), value)?,
                };
                if new.ty != local.ty {
                    return Err(no(
                        "a variable given another type",
                        format!("{name}: {} to {}", local.ty.name(), new.ty.name()),
                    ));
                }
                let x = self.local();
                Ok(v(
                    format!(
                        "{{ let {x} = {}; {} = {}; {x} }}",
                        new.code,
                        local.rust,
                        clone_of(&x, new.ty)
                    ),
                    new.ty,
                ))
            }
            Expr::Member(base, name) => {
                let Some(r) = self.object_of(base)? else {
                    return Err(no("assignment", "to a member of a value"));
                };
                self.effects = true;
                let old = match op {
                    Some(_) => Some(self.read(r, name)?),
                    None => None,
                };
                let (o, p, kind) = self.prop(r, name, true)?;
                let value = self.expr(value, Want::Value)?;
                let new = match (op, old) {
                    (Some(op), Some(old)) => binary(op, old, value)?,
                    _ => value,
                };
                if new.ty != kind.ty() {
                    return Err(no(
                        "a write that converts",
                        format!("{} to {} property {name}", new.ty.name(), kind.ty().name()),
                    ));
                }
                let x = self.local();
                Ok(v(
                    format!(
                        "{{ let {x} = {}; f.set({o}, {p}, {})?; {x} }}",
                        new.code,
                        to_value(&clone_of(&x, new.ty), new.ty)
                    ),
                    new.ty,
                ))
            }
            _ => Err(no("assignment", "to an expression")),
        }
    }

    fn update(&mut self, increment: bool, prefix: bool, target: &'p Expr) -> Result<V, Refused> {
        let op = if increment { "+" } else { "-" };
        let (x, y) = (self.local(), self.local());
        match target {
            Expr::Name(name) => {
                let Some(local) = self.lookup_local(name) else {
                    return Err(no("assignment to a name", name.clone()));
                };
                if local.constant || local.ty != Ty::Num {
                    return Err(no("assignment", format!("++ or -- of {name}")));
                }
                let r = &local.rust;
                let give = if prefix { &y } else { &x };
                Ok(v(
                    format!("{{ let {x} = {r}; let {y} = {x} {op} 1.0; {r} = {y}; {give} }}"),
                    Ty::Num,
                ))
            }
            Expr::Member(base, name) => {
                let Some(r) = self.object_of(base)? else {
                    return Err(no("assignment", "to a member of a value"));
                };
                self.effects = true;
                let old = self.read(r, name)?;
                let (o, p, kind) = self.prop(r, name, true)?;
                if kind != Kind::Number {
                    return Err(no(
                        "operator",
                        format!("++ or -- of a {}", kind.ty().name()),
                    ));
                }
                let give = if prefix { &y } else { &x };
                Ok(v(
                    format!(
                        "{{ let {x} = {}; let {y} = {x} {op} 1.0; f.set({o}, {p}, rt::Value::Real({y}))?; {give} }}",
                        old.code
                    ),
                    Ty::Num,
                ))
            }
            _ => Err(no("assignment", "to an expression")),
        }
    }

    fn call(&mut self, callee: &'p Expr, args: &'p [Expr], want: Want) -> Result<V, Refused> {
        match callee {
            Expr::Name(name) => {
                if self.lookup_local(name).is_some() || self.visible(name) {
                    return Err(no("function call", "of a value"));
                }
                match name.as_str() {
                    "qsTr" => self.qs_tr(args),
                    "Number" | "String" | "Boolean" | "isNaN" | "isFinite" => {
                        let [a] = args else {
                            return Err(no(
                                "global function call",
                                format!("{name} of {} arguments", args.len()),
                            ));
                        };
                        let a = self.expr(a, Want::Value)?;
                        convert(name, a)
                    }
                    SINGLETON if self.single_call(callee, args).is_some() => {
                        Err(no("object as a value", "a singleton"))
                    }
                    _ => Err(no("global function call", name.clone())),
                }
            }
            Expr::Member(base, method) => {
                if let Expr::Name(b) = &**base {
                    if b == "Math" && self.lookup_local(b).is_none() && !self.visible(b) {
                        return self.math(method, args);
                    }
                    let got = self.name(b)?;
                    if let Ty::Obj(Src::Here(r)) = got.ty {
                        return self.method(r, method, args, want);
                    }
                }
                if self.single(base).is_some() {
                    return Err(no("method call of a singleton", format!(".{method}()")));
                }
                if let Some(Src::Via(_)) = self.object_of(base)? {
                    return Err(no(
                        "method call of an object read through a property",
                        format!(".{method}()"),
                    ));
                }
                Err(no("method call", format!(".{method}()")))
            }
            _ => Err(no("function call", "of a value")),
        }
    }

    fn qs_tr(&mut self, args: &'p [Expr]) -> Result<V, Refused> {
        if self.module.context.is_none() {
            return Err(no(
                "global function call",
                "qsTr without the file's context",
            ));
        }
        let [Expr::String(text)] = args else {
            return Err(no("global function call", "qsTr of anything but one text"));
        };
        let at = site(&mut self.sites.texts, text.clone());
        Ok(v(format!("f.tr({at})"), Ty::Str))
    }

    fn math(&mut self, name: &str, args: &'p [Expr]) -> Result<V, Refused> {
        let mut values = Vec::with_capacity(args.len());
        for a in args {
            let a = self.expr(a, Want::Value)?;
            if a.ty != Ty::Num {
                return Err(no("Math", format!("{name} of a {}", a.ty.name())));
            }
            values.push(a.code);
        }
        let one = |f: &str| -> Result<V, Refused> {
            match &values[..] {
                [a] => Ok(v(format!("rt::js::{f}({a})"), Ty::Num)),
                _ => Err(no("Math", format!("{name} of {} arguments", values.len()))),
            }
        };
        match name {
            "min" | "max" => {
                let mut it = values.iter();
                let Some(first) = it.next() else {
                    return Err(no("Math", format!("{name} of no arguments")));
                };
                // `Math.min(a)` is `+a`, which for a number is the number.
                let mut code = first.clone();
                for next in it {
                    code = format!("rt::js::{name}({code}, {next})");
                }
                Ok(v(code, Ty::Num))
            }
            "pow" => match &values[..] {
                [a, b] => Ok(v(format!("rt::js::pow({a}, {b})"), Ty::Num)),
                _ => Err(no("Math", format!("pow of {} arguments", values.len()))),
            },
            "abs" | "floor" | "ceil" | "round" | "sqrt" | "trunc" | "sign" => one(name),
            _ => Err(no("Math", name)),
        }
    }

    /// `object.name(...)`: a signal of the file's, emitted, or a function
    /// of the file's, inlined.
    fn method(
        &mut self,
        r: ObjRef,
        name: &str,
        args: &'p [Expr],
        want: Want,
    ) -> Result<V, Refused> {
        let def: &'p ObjectDef = self.file.object(r);
        if let Some(decl) = def.signals.iter().rev().find(|s| s.name == name) {
            if args.len() != decl.params.len() {
                return Err(no(
                    "signal",
                    format!(
                        "{name} given {} arguments for {}",
                        args.len(),
                        decl.params.len()
                    ),
                ));
            }
            let mut values = Vec::with_capacity(args.len());
            for (a, param) in args.iter().zip(&decl.params) {
                let a = self.expr(a, Want::Value)?;
                let declared = param.ty.as_deref().and_then(Kind::of);
                if declared.map(Kind::ty) != Some(a.ty) {
                    return Err(no(
                        "signal",
                        format!("{name} given a {} for {}", a.ty.name(), param.name),
                    ));
                }
                values.push(a);
            }
            self.effects = true;
            let object = self.object(Src::Here(r));
            let at = site(&mut self.sites.signals, name.to_string());
            let index = site(&mut self.signals, (object, at));
            let mut code = String::from("{ ");
            let mut list = Vec::new();
            for a in values {
                let x = self.local();
                let _ = write!(code, "let {x} = {}; ", to_value(&a.code, a.ty));
                list.push(x);
            }
            let _ = write!(
                code,
                "f.emit(o{object}, s{index}, &[{}])? }}",
                list.join(", ")
            );
            return Ok(v(code, Ty::Void));
        }
        let Some(callee) = def
            .functions
            .iter()
            .rev()
            .find(|f| f.name == name)
            .map(|f| f.expr)
        else {
            return Err(no("method call", format!(".{name}()")));
        };
        self.inline(r, name, callee, args, want)
    }

    /// The body of a function of the file in place of its call, behind a
    /// check that the object still has that very function.
    fn inline(
        &mut self,
        r: ObjRef,
        name: &str,
        callee: ExprId,
        args: &'p [Expr],
        want: Want,
    ) -> Result<V, Refused> {
        const KIND: &str = "call of a function that is not compiled";
        let wrap = |why: Refused| {
            if why.kind == KIND {
                why
            } else {
                no(KIND, format!("{name}: {why}"))
            }
        };
        if self.frames.len() > MAX_INLINE_DEPTH {
            return Err(no(KIND, "calls too deep"));
        }
        let binding = self.file.expr(callee);
        if binding.kind != BindingKind::Function {
            return Err(no(KIND, format!("{name} is no function")));
        }
        let body = self.body(callee).map_err(wrap)?;
        // Evaluated in the caller, each into a variable the function may
        // assign.
        let mut code = String::from("{\n");
        let mut scope = HashMap::new();
        for (index, formal) in body.formals.iter().enumerate() {
            let rust = self.local();
            let ty = match args.get(index) {
                Some(a) => {
                    let a = self.expr(a, Want::Value)?;
                    if !matches!(a.ty, Ty::Num | Ty::Bool | Ty::Str) {
                        return Err(no(KIND, format!("{name} given a {}", a.ty.name())));
                    }
                    let _ = writeln!(code, "let mut {rust} = {};", a.code);
                    a.ty
                }
                None => {
                    let _ = writeln!(code, "let mut {rust} = ();");
                    Ty::Void
                }
            };
            scope.insert(
                formal.clone(),
                Local {
                    rust,
                    ty,
                    constant: false,
                },
            );
        }
        if args.len() > body.formals.len() {
            return Err(no(
                KIND,
                format!("{name} given more arguments than it names"),
            ));
        }
        let object = self.object(Src::Here(r));
        let at = site(&mut self.sites.methods, (name.to_string(), callee.0));
        site(&mut self.guards, (object, at));
        let label = self.label("r");
        let value = want == Want::Value;
        self.frames.push(Frame {
            binding,
            scopes: vec![scope],
            ret: Ret::Inline {
                label: label.clone(),
                value,
                ty: None,
            },
            switches: Vec::new(),
        });
        let mut inner = String::new();
        let flow = match &body.code {
            Code::Statements(list) => self.statements(list, &mut inner),
            Code::Expression(e) => self.give_back(Some(e), &mut inner).map(|()| Flow::Returns),
        };
        let frame = self.frames.pop().expect("the frame pushed");
        let flow = flow.map_err(wrap)?;
        let Ret::Inline { ty, .. } = frame.ret else {
            unreachable!("an inlined frame returns inline")
        };
        let mut ty = if value { ty } else { Some(Ty::Void) };
        if flow == Flow::Continues && value {
            // Falling off the end gives `undefined`.
            match ty {
                Some(Ty::Void) | None => {
                    ty = Some(Ty::Void);
                    inner.push_str("()\n");
                }
                Some(t) => {
                    return Err(no(
                        KIND,
                        format!("{name} returns a {} or nothing", t.name()),
                    ));
                }
            }
        } else if flow == Flow::Returns {
            inner.push_str("unreachable!()\n");
        }
        let ty = ty.unwrap_or(Ty::Void);
        let _ = write!(code, "{label}: {{\n{inner}}}\n}}");
        Ok(v(code, ty))
    }
}

/// What `def` itself declares `name` to be: its kind, and whether it can
/// be written. `None` for a name it leaves to its type.
fn declared_on(def: &ObjectDef, name: &str) -> Result<Option<(Kind, bool)>, Refused> {
    if let Some(decl) = def.properties.iter().rev().find(|p| p.name == name) {
        if decl.alias.is_some() {
            return Err(no("alias", name));
        }
        if decl.is_list {
            return Err(no("type", format!("list property {name}")));
        }
        return match Kind::of(&decl.ty) {
            Some(kind) => Ok(Some((kind, !decl.is_readonly))),
            None => Err(no("type", format!("{} property {name}", decl.ty))),
        };
    }
    if def.functions.iter().any(|f| f.name == name) || def.signals.iter().any(|s| s.name == name) {
        return Err(no("function value", name));
    }
    Ok(None)
}

/// Which signal the handler `id` of the object is for.
fn handled(members: &Members, id: ExprId) -> Option<&str> {
    members
        .handlers
        .iter()
        .find(|h| h.expr == id)
        .map(|h| h.signal.as_str())
}

fn breaks_out(s: &Stmt) -> bool {
    match s {
        Stmt::Break => true,
        Stmt::If {
            then, otherwise, ..
        } => then.iter().any(breaks_out) || otherwise.iter().any(breaks_out),
        Stmt::Block(list) => list.iter().any(breaks_out),
        // A break there leaves that switch.
        _ => false,
    }
}

fn clone_of(code: &str, ty: Ty) -> String {
    if ty == Ty::Str {
        format!("{code}.clone()")
    } else {
        code.to_string()
    }
}

fn to_value(code: &str, ty: Ty) -> String {
    match ty {
        Ty::Num => format!("rt::Value::Real({code})"),
        Ty::Bool => format!("rt::Value::Bool({code})"),
        Ty::Str => format!("rt::Value::String({code})"),
        _ => "rt::Value::Undefined".to_string(),
    }
}

/// A number as Rust writes it, to the bit.
fn number(n: f64) -> String {
    if n.is_nan() {
        "f64::NAN".into()
    } else if n == f64::INFINITY {
        "f64::INFINITY".into()
    } else if n == f64::NEG_INFINITY {
        "f64::NEG_INFINITY".into()
    } else {
        format!("{n:?}_f64")
    }
}

fn truthy(x: &V) -> Result<String, Refused> {
    Ok(match x.ty {
        Ty::Bool => x.code.clone(),
        Ty::Num => format!("rt::js::truthy({})", x.code),
        Ty::Str => format!("!{}.is_empty()", x.code),
        Ty::Void => format!("{{ let _ = {}; false }}", x.code),
        other => return Err(no("type", format!("a {} as a condition", other.name()))),
    })
}

/// `a === b`; `strict` false is `==`, for operands of one type the same.
fn strict_equal(a: &V, b: &V, equal: bool) -> String {
    let not = if equal { "" } else { "!" };
    if a.ty != b.ty {
        // Of two types, never equal: both are still evaluated, in order.
        return format!("{{ let _ = {}; let _ = {}; {} }}", a.code, b.code, !equal);
    }
    match a.ty {
        Ty::Void => format!("{{ let _ = {}; let _ = {}; {equal} }}", a.code, b.code),
        _ => format!("{not}({} == {})", a.code, b.code),
    }
}

fn binary(op: BinaryOp, a: V, b: V) -> Result<V, Refused> {
    use BinaryOp as B;
    let refuse = |a: &V, b: &V| {
        Err(no(
            "operator",
            format!("{op:?} of a {} and a {}", a.ty.name(), b.ty.name()),
        ))
    };
    Ok(match op {
        B::Add => match (a.ty, b.ty) {
            (Ty::Num, Ty::Num) => v(format!("({} + {})", a.code, b.code), Ty::Num),
            (Ty::Str | Ty::Num | Ty::Bool, Ty::Str) | (Ty::Str, Ty::Num | Ty::Bool) => v(
                format!("rt::js::concat(&{}, &{})", as_text(&a), as_text(&b)),
                Ty::Str,
            ),
            _ => return refuse(&a, &b),
        },
        B::Sub | B::Mul | B::Div | B::Rem => {
            if (a.ty, b.ty) != (Ty::Num, Ty::Num) {
                return refuse(&a, &b);
            }
            let o = match op {
                B::Sub => "-",
                B::Mul => "*",
                B::Div => "/",
                // Rust's `%` of two doubles is C's `fmod`, which is
                // JavaScript's: the sign of the dividend.
                _ => "%",
            };
            v(format!("({} {o} {})", a.code, b.code), Ty::Num)
        }
        B::Exp | B::BitAnd | B::BitOr | B::BitXor | B::Shl | B::Shr | B::UShr => {
            if (a.ty, b.ty) != (Ty::Num, Ty::Num) {
                return refuse(&a, &b);
            }
            let f = match op {
                B::Exp => "pow",
                B::BitAnd => "bit_and",
                B::BitOr => "bit_or",
                B::BitXor => "bit_xor",
                B::Shl => "shl",
                B::Shr => "shr",
                _ => "ushr",
            };
            v(format!("rt::js::{f}({}, {})", a.code, b.code), Ty::Num)
        }
        B::StrictEq | B::StrictNe => v(strict_equal(&a, &b, op == B::StrictEq), Ty::Bool),
        B::Eq | B::Ne => {
            // Between two types `==` converts, and that is not followed.
            if a.ty != b.ty {
                return refuse(&a, &b);
            }
            v(strict_equal(&a, &b, op == B::Eq), Ty::Bool)
        }
        B::Lt | B::Le | B::Gt | B::Ge => {
            let o = match op {
                B::Lt => "<",
                B::Le => "<=",
                B::Gt => ">",
                _ => ">=",
            };
            match (a.ty, b.ty) {
                (Ty::Num, Ty::Num) => v(format!("({} {o} {})", a.code, b.code), Ty::Bool),
                (Ty::Str, Ty::Str) => v(
                    format!("(rt::js::order(&{}, &{}) {o} 0)", a.code, b.code),
                    Ty::Bool,
                ),
                _ => return refuse(&a, &b),
            }
        }
    })
}

/// A value as the string `+` makes of it.
fn as_text(x: &V) -> String {
    match x.ty {
        Ty::Num => format!("rt::js::number_text({})", x.code),
        Ty::Bool => format!(
            "rt::Atom::from(if {} {{ \"true\" }} else {{ \"false\" }})",
            x.code
        ),
        _ => format!("({})", x.code),
    }
}

/// `Number(a)`, `String(a)`, `Boolean(a)`, `isNaN(a)`, `isFinite(a)`.
fn convert(name: &str, a: V) -> Result<V, Refused> {
    Ok(match (name, a.ty) {
        ("Number", Ty::Num) => a,
        ("Number", Ty::Bool) => v(format!("(if {} {{ 1.0 }} else {{ 0.0 }})", a.code), Ty::Num),
        ("String", Ty::Str) => a,
        ("String", Ty::Num | Ty::Bool) => v(as_text(&a), Ty::Str),
        ("Boolean", Ty::Num | Ty::Bool | Ty::Str) => v(truthy(&a)?, Ty::Bool),
        ("isNaN", Ty::Num) => v(format!("{}.is_nan()", a.code), Ty::Bool),
        ("isFinite", Ty::Num) => v(format!("{}.is_finite()", a.code), Ty::Bool),
        _ => {
            return Err(no(
                "global function call",
                format!("{name} of a {}", a.ty.name()),
            ));
        }
    })
}

/// The files of an application by name, with the counts a report gives.
#[derive(Debug, Default, Clone)]
pub struct Coverage {
    pub compiled: usize,
    pub refused: usize,
    /// By kind, then detail.
    pub reasons: BTreeMap<String, BTreeMap<String, usize>>,
}

impl Coverage {
    pub fn add(&mut self, outcomes: &[Outcome]) {
        for o in outcomes {
            match o {
                Outcome::Compiled => self.compiled += 1,
                Outcome::Refused(why) => {
                    self.refused += 1;
                    *self
                        .reasons
                        .entry(why.kind.clone())
                        .or_default()
                        .entry(why.detail.clone())
                        .or_default() += 1;
                }
            }
        }
    }

    /// The kinds, most refused first.
    pub fn top(&self) -> Vec<(&str, usize)> {
        let mut out: Vec<(&str, usize)> = self
            .reasons
            .iter()
            .map(|(k, d)| (k.as_str(), d.values().sum()))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        out
    }
}
