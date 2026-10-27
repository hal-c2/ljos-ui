//! Back end B: the typed subset to Rust, at build time.
//!
//! Reads `world.d.ts` for what the objects hold and each case file for its
//! functions, and writes Rust that calls the engine directly: a read is
//! `Engine::read`, a write `Engine::write`, an emit `Engine::emit`. It knows
//! the constructs the eight cases use and says so by name for any other,
//! which is the answer to "what can't it compile".

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use oxc_syntax::operator::{
    AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Num,
    Bool,
    Str,
    List(Box<Ty>),
    NumSet,
    Void,
    Row(Vec<(String, Ty)>),
}

impl Ty {
    fn rust(&self) -> String {
        match self {
            Ty::Num => "f64".into(),
            Ty::Bool => "bool".into(),
            Ty::Str => "Atom".into(),
            Ty::List(t) => format!("Vec<{}>", t.rust()),
            Ty::NumSet => "crate::rt::NumSet".into(),
            Ty::Void => "()".into(),
            Ty::Row(_) => "Value".into(),
        }
    }

    fn copy(&self) -> bool {
        matches!(self, Ty::Num | Ty::Bool | Ty::Void)
    }
}

#[derive(Default)]
pub struct Obj {
    props: Vec<(String, Ty)>,
    signals: Vec<(String, Vec<Ty>)>,
    /// The row type, when the object is a `ListModel`.
    model: Option<Vec<(String, Ty)>>,
}

pub type World = BTreeMap<String, Obj>;

/// What a `Qt.` constant is: the values of Qt's public enum documentation.
fn qt_constant(name: &str) -> Option<f64> {
    Some(match name {
        "ControlModifier" => f64::from(0x0400_0000),
        "Key_1" => f64::from(0x31),
        "Key_9" => f64::from(0x39),
        "Key_Down" => f64::from(0x0100_0015),
        "Key_Backspace" => f64::from(0x0100_0003),
        _ => return None,
    })
}

fn key_name(k: &PropertyKey) -> Result<String, String> {
    match k {
        PropertyKey::StaticIdentifier(id) => Ok(id.name.to_string()),
        _ => Err("a computed member name".into()),
    }
}

fn ty_of(t: &TSType) -> Result<Ty, String> {
    Ok(match t {
        TSType::TSNumberKeyword(_) => Ty::Num,
        TSType::TSBooleanKeyword(_) => Ty::Bool,
        TSType::TSStringKeyword(_) => Ty::Str,
        TSType::TSVoidKeyword(_) => Ty::Void,
        TSType::TSArrayType(a) => Ty::List(Box::new(ty_of(&a.element_type)?)),
        TSType::TSTypeOperatorType(o) => ty_of(&o.type_annotation)?,
        TSType::TSTypeLiteral(l) => Ty::Row(row_of(l)?),
        TSType::TSUnionType(_) => return Err("a union type".into()),
        _ => return Err("a type other than number, boolean, string or an array of them".into()),
    })
}

fn row_of(l: &TSTypeLiteral) -> Result<Vec<(String, Ty)>, String> {
    let mut fields = Vec::new();
    for m in &l.members {
        if let TSSignature::TSPropertySignature(p) = m {
            let t = p
                .type_annotation
                .as_ref()
                .ok_or("a member without a type")?;
            fields.push((key_name(&p.key)?, ty_of(&t.type_annotation)?));
        }
    }
    Ok(fields)
}

fn params_of(ps: &FormalParameters) -> Result<Vec<(String, Ty)>, String> {
    let mut out = Vec::new();
    for p in &ps.items {
        let BindingPattern::BindingIdentifier(id) = &p.pattern else {
            return Err("a destructuring parameter".into());
        };
        let t = p
            .type_annotation
            .as_ref()
            .ok_or_else(|| format!("parameter `{}` has no type", id.name))?;
        out.push((id.name.to_string(), ty_of(&t.type_annotation)?));
    }
    Ok(out)
}

pub fn parse_world(src: &str) -> Result<World, String> {
    let alloc = Allocator::default();
    let ret = Parser::new(&alloc, src, SourceType::d_ts()).parse();
    if ret.diagnostics.has_errors() {
        return Err(format!(
            "world.d.ts: {:?}",
            ret.diagnostics
                .errors()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
        ));
    }
    let mut world = World::new();
    for s in &ret.program.body {
        let Statement::VariableDeclaration(d) = s else {
            continue;
        };
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(id) = &decl.id else {
                continue;
            };
            let name = id.name.to_string();
            if name == "Qt" {
                continue;
            }
            let Some(t) = &decl.type_annotation else {
                continue;
            };
            let mut obj = Obj::default();
            match &t.type_annotation {
                TSType::TSTypeLiteral(l) => {
                    for m in &l.members {
                        match m {
                            TSSignature::TSPropertySignature(p) => {
                                let t = p.type_annotation.as_ref().ok_or("untyped member")?;
                                obj.props
                                    .push((key_name(&p.key)?, ty_of(&t.type_annotation)?));
                            }
                            TSSignature::TSMethodSignature(m) => {
                                let ps = params_of(&m.params)?.into_iter().map(|p| p.1).collect();
                                obj.signals.push((key_name(&m.key)?, ps));
                            }
                            _ => {}
                        }
                    }
                }
                TSType::TSTypeReference(r) => {
                    let row = r
                        .type_arguments
                        .as_ref()
                        .and_then(|a| a.params.first())
                        .ok_or("ListModel without a row type")?;
                    let TSType::TSTypeLiteral(l) = row else {
                        return Err("a row type that is not a literal".into());
                    };
                    obj.props.push(("count".into(), Ty::Num));
                    obj.model = Some(row_of(l)?);
                }
                _ => continue,
            }
            world.insert(name, obj);
        }
    }
    Ok(world)
}

#[derive(Clone)]
struct Sig {
    params: Vec<(String, Ty)>,
    ret: Ty,
}

/// A case file's functions, after the first pass.
pub struct Case {
    pub stem: String,
    pub source: String,
}

pub struct Output {
    pub rust: String,
    /// Per case file: nanoseconds to parse and generate it.
    pub timings: Vec<(String, u128)>,
}

struct Gen<'w> {
    world: &'w World,
    sigs: BTreeMap<String, Sig>,
    props: BTreeSet<(String, String)>,
    signals: BTreeSet<(String, String)>,
    methods: BTreeSet<(String, String)>,
    strings: Vec<String>,
    labels: usize,
}

struct Fx {
    ret: Ty,
    scopes: Vec<Vec<(String, Ty)>>,
    /// (label `break` leaves, label `continue` leaves).
    jumps: Vec<(String, Option<String>)>,
}

impl Fx {
    fn local(&self, name: &str) -> Option<Ty> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|(n, _)| n == name)
            .map(|(_, t)| t.clone())
    }
    fn declare(&mut self, name: &str, t: Ty) {
        self.scopes
            .last_mut()
            .expect("a scope")
            .push((name.into(), t));
    }
}

type R<T> = Result<T, String>;

fn sig_of(f: &Function) -> R<(String, Sig)> {
    let name =
        f.id.as_ref()
            .ok_or("an anonymous function")?
            .name
            .to_string();
    let params = params_of(&f.params).map_err(|e| format!("{name}: {e}"))?;
    let ret = match &f.return_type {
        Some(t) => ty_of(&t.type_annotation)?,
        None => return Err(format!("{name}: no return type")),
    };
    Ok((name, Sig { params, ret }))
}

fn functions<'p, 'a>(program: &'p Program<'a>) -> Vec<(&'p Function<'a>, bool)> {
    let mut out = Vec::new();
    for s in &program.body {
        match s {
            Statement::FunctionDeclaration(f) => out.push((&**f, false)),
            Statement::ExportDeclaration(d) => {
                if let Declaration::FunctionDeclaration(f) = &d.declaration {
                    out.push((&**f, true));
                }
            }
            _ => {}
        }
    }
    out
}

pub fn generate(world: &World, cases: &[Case]) -> R<Output> {
    let mut g = Gen {
        world,
        sigs: BTreeMap::new(),
        props: BTreeSet::new(),
        signals: BTreeSet::new(),
        methods: BTreeSet::new(),
        strings: Vec::new(),
        labels: 0,
    };
    // Signatures first: a case may call a function another file exports.
    for c in cases {
        let alloc = Allocator::default();
        let ret = Parser::new(&alloc, &c.source, SourceType::ts()).parse();
        if ret.diagnostics.has_errors() {
            return Err(format!(
                "{}: {:?}",
                c.stem,
                ret.diagnostics
                    .errors()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
            ));
        }
        for (f, _) in functions(&ret.program) {
            let (name, sig) = sig_of(f).map_err(|e| format!("{}: {e}", c.stem))?;
            g.sigs.insert(name, sig);
        }
    }
    let mut body = String::new();
    let mut exports = Vec::new();
    let mut timings = Vec::new();
    for c in cases {
        let start = std::time::Instant::now();
        let alloc = Allocator::default();
        let ret = Parser::new(&alloc, &c.source, SourceType::ts()).parse();
        for (f, exported) in functions(&ret.program) {
            let (name, sig) = sig_of(f)?;
            let code = g
                .function(&name, &sig, f)
                .map_err(|e| format!("{}: {name}: {e}", c.stem))?;
            body.push_str(&code);
            if exported && name.contains("__") {
                exports.push((c.stem.clone(), name, sig));
            }
        }
        timings.push((c.stem.clone(), start.elapsed().as_nanos()));
    }
    let install = g.install(&exports)?;
    let mut rust = String::new();
    rust.push_str("use std::sync::LazyLock;\nuse ljos_engine::{Atom, Engine, MethodIndex, ObjectId, PropIndex, SignalIndex, Value};\n\n");
    for (i, s) in g.strings.iter().enumerate() {
        let _ = writeln!(
            rust,
            "static S{i}: LazyLock<Atom> = LazyLock::new(|| Atom::from({s:?}));"
        );
    }
    rust.push_str(&g.world_struct());
    rust.push_str(&body);
    rust.push_str(&install);
    let _ = writeln!(
        rust,
        "/// Per case file, nanoseconds the generator took for it.\npub const CODEGEN_NS: &[(&str, u64)] = &[{}];",
        timings
            .iter()
            .map(|(s, t)| format!("({s:?}, {t})"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(Output { rust, timings })
}

impl Gen<'_> {
    fn world_struct(&self) -> String {
        let objs: BTreeSet<&String> = self
            .props
            .iter()
            .chain(&self.signals)
            .chain(&self.methods)
            .map(|(o, _)| o)
            .collect();
        let mut s = String::from(
            "/// Every object, property, signal and method the cases name, looked up once.\n#[derive(Clone, Copy, Debug)]\npub struct World {\n",
        );
        for o in &objs {
            let _ = writeln!(s, "    pub o_{o}: ObjectId,");
        }
        for (o, p) in &self.props {
            let _ = writeln!(s, "    pub p_{o}_{p}: PropIndex,");
        }
        for (o, p) in &self.signals {
            let _ = writeln!(s, "    pub s_{o}_{p}: SignalIndex,");
        }
        for (o, p) in &self.methods {
            let _ = writeln!(s, "    pub m_{o}_{p}: MethodIndex,");
        }
        s.push_str("}\n\nimpl World {\n    pub fn resolve(e: &Engine, top: ObjectId) -> Result<Self, crate::rt::Missing> {\n");
        for o in &objs {
            let _ = writeln!(s, "        let o_{o} = crate::rt::find(e, top, {o:?})?;");
        }
        s.push_str("        Ok(World {\n");
        for o in &objs {
            let _ = writeln!(s, "            o_{o},");
        }
        for (o, p) in &self.props {
            let _ = writeln!(s, "            p_{o}_{p}: e.property_index(o_{o}, {p:?})?,");
        }
        for (o, p) in &self.signals {
            let _ = writeln!(s, "            s_{o}_{p}: e.signal_index(o_{o}, {p:?})?,");
        }
        for (o, p) in &self.methods {
            let _ = writeln!(s, "            m_{o}_{p}: e.method_index(o_{o}, {p:?})?,");
        }
        s.push_str("        })\n    }\n}\n\n");
        s
    }

    fn install(&mut self, exports: &[(String, String, Sig)]) -> R<String> {
        let mut arms = String::new();
        let mut names = Vec::new();
        for (stem, name, sig) in exports {
            let (obj, member) = name.split_once("__").expect("checked");
            names.push(format!("({stem:?}, {name:?})"));
            if let Some(sig_name) = member
                .strip_prefix("on")
                .filter(|m| m.starts_with(char::is_uppercase))
            {
                let mut sig_name = sig_name.to_string();
                sig_name[..1].make_ascii_lowercase();
                self.signals.insert((obj.into(), sig_name.clone()));
                let args: Vec<String> = sig
                    .params
                    .iter()
                    .enumerate()
                    .map(|(i, (_, t))| match t {
                        Ty::Num => Ok(format!("crate::rt::sig_num(a, {i})")),
                        Ty::Str => Ok(format!("crate::rt::sig_str(a, {i})")),
                        Ty::Bool => Ok(format!("crate::rt::sig_bool(a, {i})")),
                        _ => Err(format!("{name}: a signal argument of type {t:?}")),
                    })
                    .collect::<R<_>>()?;
                let _ = writeln!(
                    arms,
                    "        {name:?} => Some(crate::Export::Handler(w.o_{obj}, w.s_{obj}_{sig_name}, std::rc::Rc::new(move |e: &Engine, a: &[Value]| {{ let _ = a; {name}(e, &w{}{}) }}))),",
                    if args.is_empty() { "" } else { ", " },
                    args.join(", ")
                );
            } else {
                if !self
                    .world
                    .get(obj)
                    .is_some_and(|o| o.props.iter().any(|(p, _)| p == member))
                {
                    return Err(format!("{name}: `{obj}` has no property `{member}`"));
                }
                self.props.insert((obj.into(), member.into()));
                let _ = writeln!(
                    arms,
                    "        {name:?} => Some(crate::Export::Binding(w.o_{obj}, w.p_{obj}_{member}, Box::new(move |e: &Engine| crate::rt::val({name}(e, &w))))),"
                );
            }
        }
        Ok(format!(
            "/// (case file, export) for each binding or handler a case gives.\npub const EXPORTS: &[(&str, &str)] = &[{}];\n\n/// One export, as the binding or handler it gives.\npub fn export(w: &World, export: &str) -> Option<crate::Export> {{\n    let w = *w;\n    match export {{\n{arms}        _ => None,\n    }}\n}}\n\n",
            names.join(", ")
        ))
    }

    fn string(&mut self, s: &str) -> String {
        let i = match self.strings.iter().position(|x| x == s) {
            Some(i) => i,
            None => {
                self.strings.push(s.into());
                self.strings.len() - 1
            }
        };
        format!("S{i}.clone()")
    }

    fn label(&mut self, kind: &str) -> String {
        self.labels += 1;
        format!("'{kind}{}", self.labels)
    }

    fn function(&mut self, name: &str, sig: &Sig, f: &Function) -> R<String> {
        let mut fx = Fx {
            ret: sig.ret.clone(),
            scopes: vec![Vec::new()],
            jumps: Vec::new(),
        };
        let mut params = String::new();
        for (p, t) in &sig.params {
            let _ = write!(params, ", v_{p}: {}", t.rust());
            fx.declare(p, t.clone());
        }
        let ret = if sig.ret == Ty::Void {
            String::new()
        } else {
            format!(" -> {}", sig.ret.rust())
        };
        let mut out = format!(
            "#[inline(never)]\npub fn {name}(e: &Engine, w: &World{params}){ret} {{\n    let _ = (e, w);\n"
        );
        let body = f.body.as_ref().ok_or("no body")?;
        for s in &body.statements {
            self.stmt(&mut fx, s, &mut out, 1)?;
        }
        if sig.ret != Ty::Void
            && !matches!(body.statements.last(), Some(Statement::ReturnStatement(_)))
        {
            out.push_str("    #[allow(unreachable_code)]\n    { unreachable!(\"a function of the typed subset ran off its end\") }\n");
        }
        out.push_str("}\n\n");
        Ok(out)
    }

    fn stmt(&mut self, fx: &mut Fx, s: &Statement, out: &mut String, depth: usize) -> R<()> {
        let pad = "    ".repeat(depth);
        match s {
            Statement::EmptyStatement(_) => {}
            Statement::VariableDeclaration(d) => self.var_decl(fx, d, out, depth)?,
            Statement::ExpressionStatement(x) => {
                let (code, _) = self.expr(fx, &x.expression)?;
                let _ = writeln!(out, "{pad}{code};");
            }
            Statement::ReturnStatement(r) => match &r.argument {
                None => {
                    let _ = writeln!(out, "{pad}return;");
                }
                Some(x) => {
                    let (code, t) = self.expr(fx, x)?;
                    if t != fx.ret {
                        return Err(format!("returns {t:?} where {:?} is declared", fx.ret));
                    }
                    let _ = writeln!(out, "{pad}return {code};");
                }
            },
            Statement::BlockStatement(b) => {
                let _ = writeln!(out, "{pad}{{");
                fx.scopes.push(Vec::new());
                for s in &b.body {
                    self.stmt(fx, s, out, depth + 1)?;
                }
                fx.scopes.pop();
                let _ = writeln!(out, "{pad}}}");
            }
            Statement::IfStatement(i) => {
                let (test, t) = self.expr(fx, &i.test)?;
                if t != Ty::Bool {
                    return Err(format!("an `if` on {t:?}: the subset takes booleans only"));
                }
                let _ = writeln!(out, "{pad}if {test} {{");
                self.nested(fx, &i.consequent, out, depth + 1)?;
                if let Some(a) = &i.alternate {
                    let _ = writeln!(out, "{pad}}} else {{");
                    self.nested(fx, a, out, depth + 1)?;
                }
                let _ = writeln!(out, "{pad}}}");
            }
            Statement::ForStatement(f) => {
                let brk = self.label("l");
                let cont = self.label("c");
                let _ = writeln!(out, "{pad}{{");
                fx.scopes.push(Vec::new());
                match &f.init {
                    Some(ForStatementInit::VariableDeclaration(d)) => {
                        self.var_decl(fx, d, out, depth + 1)?
                    }
                    Some(_) => return Err("a `for` whose start is not a declaration".into()),
                    None => {}
                }
                let _ = writeln!(out, "{pad}    {brk}: loop {{");
                if let Some(t) = &f.test {
                    let (test, ty) = self.expr(fx, t)?;
                    if ty != Ty::Bool {
                        return Err("a `for` test that is not a boolean".into());
                    }
                    let _ = writeln!(out, "{pad}        if !({test}) {{ break {brk}; }}");
                }
                let _ = writeln!(out, "{pad}        {cont}: {{");
                fx.jumps.push((brk.clone(), Some(cont.clone())));
                self.nested(fx, &f.body, out, depth + 3)?;
                fx.jumps.pop();
                let _ = writeln!(out, "{pad}        }}");
                if let Some(u) = &f.update {
                    let (code, _) = self.expr(fx, u)?;
                    let _ = writeln!(out, "{pad}        {code};");
                }
                let _ = writeln!(out, "{pad}    }}");
                fx.scopes.pop();
                let _ = writeln!(out, "{pad}}}");
            }
            Statement::BreakStatement(b) => {
                if b.label.is_some() {
                    return Err("a labelled `break`".into());
                }
                let (brk, _) = fx.jumps.last().ok_or("`break` outside a loop")?;
                let _ = writeln!(out, "{pad}break {brk};");
            }
            Statement::ContinueStatement(c) => {
                if c.label.is_some() {
                    return Err("a labelled `continue`".into());
                }
                let cont = fx
                    .jumps
                    .iter()
                    .rev()
                    .find_map(|(_, c)| c.clone())
                    .ok_or("`continue` outside a loop")?;
                let _ = writeln!(out, "{pad}break {cont};");
            }
            Statement::SwitchStatement(sw) => {
                let (disc, t) = self.expr(fx, &sw.discriminant)?;
                if !matches!(t, Ty::Str | Ty::Num) {
                    return Err(format!("a `switch` on {t:?}"));
                }
                let brk = self.label("sw");
                let _ = writeln!(out, "{pad}{brk}: {{\n{pad}    let d = {disc};");
                fx.jumps.push((brk.clone(), None));
                let mut tests: Vec<String> = Vec::new();
                let mut first = true;
                for case in &sw.cases {
                    match &case.test {
                        Some(x) => {
                            let (c, ct) = self.expr(fx, x)?;
                            if ct != t {
                                return Err("a `case` of another type than the `switch`".into());
                            }
                            tests.push(format!("d == {c}"));
                        }
                        None => tests.push("true".into()),
                    }
                    if case.consequent.is_empty() {
                        continue;
                    }
                    let ends = matches!(
                        case.consequent.last(),
                        Some(Statement::ReturnStatement(_) | Statement::BreakStatement(_))
                    );
                    if !ends {
                        return Err("a `case` that falls through into the next".into());
                    }
                    let _ = writeln!(
                        out,
                        "{pad}    {}if {} {{",
                        if first { "" } else { "else " },
                        tests.join(" || ")
                    );
                    first = false;
                    tests.clear();
                    fx.scopes.push(Vec::new());
                    for s in &case.consequent {
                        self.stmt(fx, s, out, depth + 2)?;
                    }
                    fx.scopes.pop();
                    let _ = writeln!(out, "{pad}    }}");
                }
                fx.jumps.pop();
                let _ = writeln!(out, "{pad}}}");
            }
            other => return Err(format!("the statement at {:?}", other.span())),
        }
        Ok(())
    }

    fn nested(&mut self, fx: &mut Fx, s: &Statement, out: &mut String, depth: usize) -> R<()> {
        fx.scopes.push(Vec::new());
        match s {
            Statement::BlockStatement(b) => {
                for s in &b.body {
                    self.stmt(fx, s, out, depth)?;
                }
            }
            s => self.stmt(fx, s, out, depth)?,
        }
        fx.scopes.pop();
        Ok(())
    }

    fn var_decl(
        &mut self,
        fx: &mut Fx,
        d: &VariableDeclaration,
        out: &mut String,
        depth: usize,
    ) -> R<()> {
        let pad = "    ".repeat(depth);
        for v in &d.declarations {
            let BindingPattern::BindingIdentifier(id) = &v.id else {
                return Err("a destructuring declaration".into());
            };
            let init = v.init.as_ref().ok_or("a declaration without a value")?;
            let (code, t) = self.expr(fx, init)?;
            if let Some(a) = &v.type_annotation
                && ty_of(&a.type_annotation)? != t
            {
                return Err(format!(
                    "`{}` declared as another type than its value",
                    id.name
                ));
            }
            let _ = writeln!(
                out,
                "{pad}#[allow(unused_mut)]\n{pad}let mut v_{}: {} = {code};",
                id.name,
                t.rust()
            );
            fx.declare(&id.name, t);
        }
        Ok(())
    }

    /// A world object's member, by id and name.
    fn world_member(&self, x: &Expression) -> Option<(String, String)> {
        let Expression::StaticMemberExpression(m) = x else {
            return None;
        };
        let Expression::Identifier(o) = &m.object else {
            return None;
        };
        if !self.world.contains_key(o.name.as_str()) {
            return None;
        }
        Some((o.name.to_string(), m.property.name.to_string()))
    }

    fn prop_ty(&self, obj: &str, prop: &str) -> R<Ty> {
        self.world[obj]
            .props
            .iter()
            .find(|(p, _)| p == prop)
            .map(|(_, t)| t.clone())
            .ok_or_else(|| format!("`{obj}` has no property `{prop}`"))
    }

    fn read(&mut self, obj: &str, prop: &str) -> R<(String, Ty)> {
        let t = self.prop_ty(obj, prop)?;
        self.props.insert((obj.into(), prop.into()));
        let at = format!("w.o_{obj}, w.p_{obj}_{prop}");
        let code = match &t {
            Ty::Num => format!("e.read_number({at})"),
            Ty::Bool => format!("e.read({at}).to_boolean()"),
            Ty::Str => format!("e.read({at}).to_js_string()"),
            Ty::List(el) => match **el {
                Ty::Num => format!("crate::rt::list_num(e, &e.read({at}))"),
                Ty::Bool => format!("crate::rt::list_bool(e, &e.read({at}))"),
                Ty::Str => format!("crate::rt::list_str(e, &e.read({at}))"),
                _ => return Err("a list of lists".into()),
            },
            _ => return Err(format!("a property of type {t:?}")),
        };
        Ok((code, t))
    }

    /// An expression to borrow: a local by name, anything else by value.
    fn place(&mut self, fx: &mut Fx, x: &Expression) -> R<(String, Ty)> {
        if let Expression::Identifier(id) = x
            && let Some(t) = fx.local(&id.name)
        {
            return Ok((format!("v_{}", id.name), t));
        }
        let (c, t) = self.expr(fx, x)?;
        Ok((format!("({c})"), t))
    }

    fn args<'b, 'a>(&self, args: &'b [Argument<'a>]) -> R<Vec<&'b Expression<'a>>> {
        args.iter()
            .map(|a| {
                a.as_expression()
                    .ok_or_else(|| "a spread argument".to_string())
            })
            .collect()
    }

    fn expr(&mut self, fx: &mut Fx, x: &Expression) -> R<(String, Ty)> {
        Ok(match x {
            Expression::ParenthesizedExpression(p) => {
                let (c, t) = self.expr(fx, &p.expression)?;
                (format!("({c})"), t)
            }
            Expression::NumericLiteral(n) => (format!("{:?}_f64", n.value), Ty::Num),
            Expression::BooleanLiteral(b) => (b.value.to_string(), Ty::Bool),
            Expression::StringLiteral(s) => (self.string(&s.value), Ty::Str),
            Expression::Identifier(id) => {
                let t = fx.local(&id.name).ok_or_else(|| {
                    format!(
                        "`{}` is not a local (an object is only read through a member)",
                        id.name
                    )
                })?;
                let c = if t.copy() {
                    format!("v_{}", id.name)
                } else {
                    format!("v_{}.clone()", id.name)
                };
                (c, t)
            }
            Expression::StaticMemberExpression(m) => {
                if let Expression::Identifier(o) = &m.object
                    && o.name == "Qt"
                {
                    let v = qt_constant(&m.property.name)
                        .ok_or_else(|| format!("Qt.{} has no value here", m.property.name))?;
                    return Ok((format!("{v:?}_f64"), Ty::Num));
                }
                if let Some((o, p)) = self.world_member(x) {
                    return self.read(&o, &p);
                }
                let (obj, t) = self.place(fx, &m.object)?;
                match (&t, m.property.name.as_str()) {
                    (Ty::Str, "length") => (format!("crate::rt::utf16_len(&{obj})"), Ty::Num),
                    (Ty::List(_), "length") => (format!("({obj}.len() as f64)"), Ty::Num),
                    (Ty::Row(fields), f) => {
                        let ft = fields
                            .iter()
                            .find(|(n, _)| n == f)
                            .map(|(_, t)| t.clone())
                            .ok_or_else(|| format!("a row has no `{f}`"))?;
                        let get = match ft {
                            Ty::Num => "row_num",
                            Ty::Str => "row_str",
                            Ty::Bool => "row_bool",
                            _ => return Err("a row field that is not a scalar".into()),
                        };
                        (format!("crate::rt::{get}(e, &{obj}, {f:?})"), ft)
                    }
                    (t, p) => return Err(format!("`.{p}` of {t:?}")),
                }
            }
            Expression::ComputedMemberExpression(m) => {
                let (obj, t) = self.place(fx, &m.object)?;
                let (i, it) = self.expr(fx, &m.expression)?;
                let (Ty::List(el), Ty::Num) = (&t, it) else {
                    return Err("an index into something other than a list".into());
                };
                let get = match **el {
                    Ty::Num => "at_num",
                    Ty::Str => "at_str",
                    Ty::Bool => "at_bool",
                    _ => return Err("an index into a list of lists".into()),
                };
                (format!("crate::rt::{get}(&{obj}, {i})"), (**el).clone())
            }
            Expression::UnaryExpression(u) => {
                let (c, t) = self.expr(fx, &u.argument)?;
                match (u.operator, t) {
                    (UnaryOperator::LogicalNot, Ty::Bool) => (format!("!{c}"), Ty::Bool),
                    (UnaryOperator::UnaryNegation, Ty::Num) => (format!("-{c}"), Ty::Num),
                    (op, t) => return Err(format!("`{}` on {t:?}", op.as_str())),
                }
            }
            Expression::BinaryExpression(b) => {
                let (l, lt) = self.operand(fx, &b.left)?;
                let (r, rt) = self.operand(fx, &b.right)?;
                use BinaryOperator as B;
                match (b.operator, &lt, &rt) {
                    (
                        B::Addition | B::Subtraction | B::Multiplication | B::Division,
                        Ty::Num,
                        Ty::Num,
                    ) => (format!("{l} {} {r}", b.operator.as_str()), Ty::Num),
                    (B::Remainder, Ty::Num, Ty::Num) => (format!("{l} % {r}"), Ty::Num),
                    (B::Addition, Ty::Str, Ty::Str) => {
                        (format!("crate::rt::concat(&{l}, &{r})"), Ty::Str)
                    }
                    (B::BitwiseAnd, Ty::Num, Ty::Num) => {
                        (format!("crate::rt::bitand({l}, {r})"), Ty::Num)
                    }
                    (
                        B::LessThan | B::GreaterThan | B::LessEqualThan | B::GreaterEqualThan,
                        Ty::Num,
                        Ty::Num,
                    ) => (format!("{l} {} {r}", b.operator.as_str()), Ty::Bool),
                    (B::StrictEquality, a, c)
                        if a == c && matches!(a, Ty::Num | Ty::Bool | Ty::Str) =>
                    {
                        (format!("{l} == {r}"), Ty::Bool)
                    }
                    (B::StrictInequality, a, c)
                        if a == c && matches!(a, Ty::Num | Ty::Bool | Ty::Str) =>
                    {
                        (format!("{l} != {r}"), Ty::Bool)
                    }
                    (B::Equality | B::Inequality, _, _) => {
                        return Err("`==` (it converts; the subset takes `===`)".into());
                    }
                    (op, a, c) => return Err(format!("`{}` on {a:?} and {c:?}", op.as_str())),
                }
            }
            Expression::LogicalExpression(l) => {
                let (a, at) = self.expr(fx, &l.left)?;
                let (b, bt) = self.expr(fx, &l.right)?;
                if at != Ty::Bool || bt != Ty::Bool {
                    return Err("`&&`/`||` on something other than booleans".into());
                }
                let op = match l.operator {
                    LogicalOperator::And => "&&",
                    LogicalOperator::Or => "||",
                    LogicalOperator::Coalesce => return Err("`??`".into()),
                };
                (format!("({a} {op} {b})"), Ty::Bool)
            }
            Expression::ConditionalExpression(c) => {
                let (test, tt) = self.expr(fx, &c.test)?;
                if tt != Ty::Bool {
                    return Err("a `?:` on something other than a boolean".into());
                }
                let (a, at) = self.expr(fx, &c.consequent)?;
                let (b, bt) = self.expr(fx, &c.alternate)?;
                if at != bt {
                    return Err(format!("a `?:` of {at:?} or {bt:?}"));
                }
                if at == Ty::Void {
                    (format!("if {test} {{ {a}; }} else {{ {b}; }}"), Ty::Void)
                } else {
                    (format!("(if {test} {{ {a} }} else {{ {b} }})"), at)
                }
            }
            Expression::AssignmentExpression(a) => self.assign(fx, a)?,
            Expression::UpdateExpression(u) => {
                let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &u.argument else {
                    return Err("`++`/`--` of something other than a local".into());
                };
                if fx.local(&id.name) != Some(Ty::Num) {
                    return Err("`++`/`--` of something other than a number".into());
                }
                let op = if u.operator == UpdateOperator::Increment {
                    "+="
                } else {
                    "-="
                };
                if !u.prefix {
                    return Err("a postfix `++`/`--` (its value is the old one; the subset takes the prefix form)".into());
                }
                (format!("{{ v_{0} {op} 1.0; v_{0} }}", id.name), Ty::Num)
            }
            Expression::ArrayExpression(a) => {
                let mut code = String::from("{ let mut v = Vec::new();");
                let mut el: Option<Ty> = None;
                for item in &a.elements {
                    let (c, t, spread) = match item {
                        ArrayExpressionElement::SpreadElement(s) => {
                            let (c, t) = self.place(fx, &s.argument)?;
                            let Ty::List(inner) = t else {
                                return Err("a spread of something other than a list".into());
                            };
                            (c, *inner, true)
                        }
                        ArrayExpressionElement::Elision(_) => {
                            return Err("a hole in an array".into());
                        }
                        other => {
                            let x = other.as_expression().ok_or("an array element")?;
                            let (c, t) = self.expr(fx, x)?;
                            (c, t, false)
                        }
                    };
                    if el.as_ref().is_some_and(|e| *e != t) {
                        return Err("an array of mixed types".into());
                    }
                    el = Some(t);
                    if spread {
                        let _ = write!(code, " v.extend({c}.iter().cloned());");
                    } else {
                        let _ = write!(code, " v.push({c});");
                    }
                }
                code.push_str(" v }");
                (
                    code,
                    Ty::List(Box::new(
                        el.ok_or("an empty array literal (its type is unknown)")?,
                    )),
                )
            }
            Expression::NewExpression(n) => {
                let Expression::Identifier(c) = &n.callee else {
                    return Err("`new` of an expression".into());
                };
                let args = self.args(&n.arguments)?;
                match (c.name.as_str(), args.as_slice()) {
                    ("Set", [a]) => {
                        let (c, t) = self.place(fx, a)?;
                        if t != Ty::List(Box::new(Ty::Num)) {
                            return Err("a Set of something other than numbers".into());
                        }
                        (format!("crate::rt::NumSet::new(&{c})"), Ty::NumSet)
                    }
                    (name, _) => return Err(format!("`new {name}`")),
                }
            }
            Expression::CallExpression(c) => self.call(fx, c)?,
            Expression::TemplateLiteral(_) => return Err("a template literal".into()),
            Expression::ArrowFunctionExpression(_) => {
                return Err("a function as a value (only a `filter` predicate is taken)".into());
            }
            Expression::ObjectExpression(_) => {
                return Err("an object literal (only a row given to a model is taken)".into());
            }
            Expression::NullLiteral(_) => return Err("`null`".into()),
            other => return Err(format!("the expression at {:?}", other.span())),
        })
    }

    /// An operand of a comparison: a local is compared in place.
    fn operand(&mut self, fx: &mut Fx, x: &Expression) -> R<(String, Ty)> {
        if let Expression::Identifier(id) = x
            && let Some(t) = fx.local(&id.name)
            && matches!(t, Ty::Str)
        {
            return Ok((format!("v_{}", id.name), t));
        }
        self.expr(fx, x)
    }

    fn assign(&mut self, fx: &mut Fx, a: &AssignmentExpression) -> R<(String, Ty)> {
        let (value, vt) = self.expr(fx, &a.right)?;
        let op = match a.operator {
            AssignmentOperator::Assign => None,
            AssignmentOperator::Addition => Some("+"),
            AssignmentOperator::Subtraction => Some("-"),
            o => return Err(format!("`{}`", o.as_str())),
        };
        match &a.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                let t = fx
                    .local(&id.name)
                    .ok_or_else(|| format!("an assignment to `{}`, not a local", id.name))?;
                if t != vt {
                    return Err(format!("`{}` given another type", id.name));
                }
                let code = match op {
                    None => format!("v_{} = {value}", id.name),
                    Some(o) if t == Ty::Num => format!("v_{} {o}= {value}", id.name),
                    Some(_) => return Err("a compound assignment to a non-number".into()),
                };
                Ok((code, Ty::Void))
            }
            AssignmentTarget::StaticMemberExpression(m) => {
                let Expression::Identifier(o) = &m.object else {
                    return Err("an assignment to a member of an expression".into());
                };
                let (obj, prop) = (o.name.to_string(), m.property.name.to_string());
                if !self.world.contains_key(&obj) {
                    return Err(format!("an assignment to `{obj}.{prop}`, not an object's"));
                }
                let t = self.prop_ty(&obj, &prop)?;
                if t != vt {
                    return Err(format!("`{obj}.{prop}` given {vt:?}, declared {t:?}"));
                }
                let value = match op {
                    None => value,
                    Some(o) if t == Ty::Num => {
                        let (cur, _) = self.read(&obj, &prop)?;
                        format!("{cur} {o} {value}")
                    }
                    Some(_) => return Err("a compound assignment to a non-number".into()),
                };
                self.props.insert((obj.clone(), prop.clone()));
                Ok((
                    format!(
                        "crate::rt::write(e, w.o_{obj}, w.p_{obj}_{prop}, crate::rt::val({value}))"
                    ),
                    Ty::Void,
                ))
            }
            _ => Err("an assignment to something other than a local or a property".into()),
        }
    }

    fn call(&mut self, fx: &mut Fx, c: &CallExpression) -> R<(String, Ty)> {
        let args = self.args(&c.arguments)?;
        match &c.callee {
            Expression::Identifier(f) => {
                if f.name == "qsTr" {
                    let [Expression::StringLiteral(s)] = args.as_slice() else {
                        return Err("`qsTr` of something other than one literal".into());
                    };
                    return Ok((
                        format!("crate::rt::qs_tr(e, {:?})", s.value.as_str()),
                        Ty::Str,
                    ));
                }
                let sig = self
                    .sigs
                    .get(f.name.as_str())
                    .cloned()
                    .ok_or_else(|| format!("a call of `{}`, unknown", f.name))?;
                if sig.params.len() != args.len() {
                    return Err(format!("`{}` given {} arguments", f.name, args.len()));
                }
                let mut code = format!("{}(e, w", f.name);
                for (a, (_, t)) in args.iter().zip(&sig.params) {
                    let (ac, at) = self.expr(fx, a)?;
                    if at != *t {
                        return Err(format!("`{}` given {at:?} for {t:?}", f.name));
                    }
                    let _ = write!(code, ", {ac}");
                }
                code.push(')');
                Ok((code, sig.ret))
            }
            Expression::StaticMemberExpression(m) => {
                let method = m.property.name.as_str();
                if let Expression::Identifier(o) = &m.object {
                    if o.name == "Math" {
                        let [a, b] = args.as_slice() else {
                            return Err(format!("Math.{method} with other than two arguments"));
                        };
                        let (a, at) = self.expr(fx, a)?;
                        let (b, bt) = self.expr(fx, b)?;
                        if at != Ty::Num || bt != Ty::Num {
                            return Err("Math of non-numbers".into());
                        }
                        return match method {
                            "min" | "max" => {
                                Ok((format!("crate::rt::math_{method}({a}, {b})"), Ty::Num))
                            }
                            _ => Err(format!("Math.{method}")),
                        };
                    }
                    if let Some(obj) = self.world.get(o.name.as_str()) {
                        return self.world_call(fx, &o.name, obj, method, &args);
                    }
                }
                let (recv, rt) = self.place(fx, &m.object)?;
                match (&rt, method, args.as_slice()) {
                    (Ty::Str, "arg", [a]) => {
                        let (a, at) = self.expr(fx, a)?;
                        let f = match at {
                            Ty::Num => "arg_num",
                            Ty::Str => "arg_str",
                            _ => {
                                return Err(
                                    "`arg` of something other than a number or string".into()
                                );
                            }
                        };
                        Ok((format!("crate::rt::{f}(&{recv}, {a})"), Ty::Str))
                    }
                    (Ty::List(el), "includes", [a]) => {
                        let (a, at) = self.operand(fx, a)?;
                        if at != **el {
                            return Err("`includes` of another type".into());
                        }
                        Ok((
                            format!("{{ let x = &{a}; {recv}.iter().any(|it| it == x) }}"),
                            Ty::Bool,
                        ))
                    }
                    (Ty::List(el), "filter", [Expression::ArrowFunctionExpression(f)]) => {
                        let el = (**el).clone();
                        let [p] = f.params.items.as_slice() else {
                            return Err("a `filter` predicate of other than one parameter".into());
                        };
                        let BindingPattern::BindingIdentifier(pid) = &p.pattern else {
                            return Err("a destructuring predicate".into());
                        };
                        let Some(body) = f.body.as_expression() else {
                            return Err(
                                "a `filter` predicate with a body (the subset takes an expression)"
                                    .into(),
                            );
                        };
                        fx.scopes.push(vec![(pid.name.to_string(), el.clone())]);
                        let (test, tt) = self.expr(fx, body)?;
                        fx.scopes.pop();
                        if tt != Ty::Bool {
                            return Err("a `filter` predicate that is not a boolean".into());
                        }
                        Ok((
                            format!(
                                "{{ let mut out = Vec::new(); for it in {recv}.iter() {{ let v_{0} = it.clone(); if {test} {{ out.push(v_{0}); }} }} out }}",
                                pid.name
                            ),
                            Ty::List(Box::new(el)),
                        ))
                    }
                    (Ty::NumSet, "has", [a]) => {
                        let (a, at) = self.expr(fx, a)?;
                        if at != Ty::Num {
                            return Err("`has` of a non-number".into());
                        }
                        Ok((format!("{recv}.has({a})"), Ty::Bool))
                    }
                    (t, m, _) => Err(format!("`.{m}()` of {t:?}")),
                }
            }
            _ => Err("a call of an expression".into()),
        }
    }

    fn world_call(
        &mut self,
        fx: &mut Fx,
        name: &str,
        obj: &Obj,
        method: &str,
        args: &[&Expression],
    ) -> R<(String, Ty)> {
        if let Some((_, params)) = obj.signals.iter().find(|(s, _)| s == method) {
            if params.len() != args.len() {
                return Err(format!("`{name}.{method}` given {} arguments", args.len()));
            }
            let mut vals = Vec::new();
            for (a, t) in args.iter().zip(params) {
                let (c, at) = self.expr(fx, a)?;
                if at != *t {
                    return Err(format!("`{name}.{method}` given {at:?} for {t:?}"));
                }
                vals.push(format!("crate::rt::val({c})"));
            }
            self.signals.insert((name.into(), method.into()));
            return Ok((
                format!(
                    "crate::rt::emit(e, w.o_{name}, w.s_{name}_{method}, &[{}])",
                    vals.join(", ")
                ),
                Ty::Void,
            ));
        }
        let Some(row) = &obj.model else {
            return Err(format!("`{name}.{method}()`: not a signal"));
        };
        let mut vals = Vec::new();
        let (want, ret): (&[Ty], Ty) = match method {
            "get" => (&[Ty::Num], Ty::Row(row.clone())),
            "remove" => (&[Ty::Num], Ty::Void),
            "insert" => (&[Ty::Num, Ty::Row(Vec::new())], Ty::Void),
            "move" => (&[Ty::Num, Ty::Num, Ty::Num], Ty::Void),
            m => return Err(format!("ListModel.{m}")),
        };
        if want.len() != args.len() {
            return Err(format!("`{name}.{method}` given {} arguments", args.len()));
        }
        for (a, t) in args.iter().zip(want) {
            if let Ty::Row(_) = t {
                let Expression::ObjectExpression(o) = a else {
                    return Err("a row that is not an object literal".into());
                };
                let mut fields = Vec::new();
                for p in &o.properties {
                    let ObjectPropertyKind::ObjectProperty(p) = p else {
                        return Err("a spread in a row".into());
                    };
                    let key = key_name(&p.key)?;
                    let want = row
                        .iter()
                        .find(|(n, _)| *n == key)
                        .ok_or_else(|| format!("a row has no `{key}`"))?;
                    let (c, ct) = self.expr(fx, &p.value)?;
                    if ct != want.1 {
                        return Err(format!("row field `{key}` given {ct:?}"));
                    }
                    let k = self.string(&key);
                    fields.push(format!("({k}, crate::rt::val({c}))"));
                }
                vals.push(format!(
                    "Value::Map(std::sync::Arc::new(vec![{}]))",
                    fields.join(", ")
                ));
            } else {
                let (c, ct) = self.expr(fx, a)?;
                if ct != *t {
                    return Err(format!("`{name}.{method}` given {ct:?}"));
                }
                vals.push(format!("crate::rt::val({c})"));
            }
        }
        self.methods.insert((name.into(), method.into()));
        let call = format!(
            "crate::rt::invoke(e, w.o_{name}, w.m_{name}_{method}, &[{}])",
            vals.join(", ")
        );
        Ok(if ret == Ty::Void {
            (format!("{{ {call}; }}"), ret)
        } else {
            (call, ret)
        })
    }
}
