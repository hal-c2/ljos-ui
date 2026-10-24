//! A generated program: an object tree with typed properties, bindings,
//! functions, signals and handlers, and the steps run on it after the load.
//!
//! Expressions are kept as a small tree of typed nodes, each printed from a
//! template, so the shrinker can replace any node with one of its children
//! or a literal of the same kind without knowing JavaScript.

use std::collections::BTreeSet;

/// The JavaScript kind of an expression's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Num,
    Str,
    Bool,
    /// An array of numbers or strings.
    Arr,
    /// A plain object with keys `k0`…`k3`.
    Obj,
    Color,
    Point,
    Size,
    Rect,
    Font,
    Url,
    /// Anything a `var` holds, including `null` and `undefined`.
    Any,
}

impl Kind {
    /// The simplest literal of the kind, which a removed node becomes.
    #[must_use]
    pub fn literal(self) -> Expr {
        let text = match self {
            Self::Num => "0",
            Self::Str => "\"\"",
            Self::Bool => "false",
            Self::Arr => "[]",
            Self::Obj => "({})",
            Self::Color => "\"#000000\"",
            Self::Point => "Qt.point(0, 0)",
            Self::Size => "Qt.size(0, 0)",
            Self::Rect => "Qt.rect(0, 0, 0, 0)",
            Self::Font => "Qt.font({pixelSize: 10})",
            Self::Url => "\"a.png\"",
            Self::Any => "null",
        };
        Expr::Lit(text.into(), self)
    }
}

/// A declared property's type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PType {
    Int,
    Real,
    Double,
    Bool,
    String,
    Url,
    Color,
    Var,
    ListInt,
    ListReal,
    ListString,
    Point,
    Size,
    Rect,
    Font,
}

impl PType {
    pub const ALL: [Self; 15] = [
        Self::Int,
        Self::Real,
        Self::Double,
        Self::Bool,
        Self::String,
        Self::Url,
        Self::Color,
        Self::Var,
        Self::ListInt,
        Self::ListReal,
        Self::ListString,
        Self::Point,
        Self::Size,
        Self::Rect,
        Self::Font,
    ];

    /// The name a declaration writes.
    #[must_use]
    pub fn qml(&self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Real => "real",
            Self::Double => "double",
            Self::Bool => "bool",
            Self::String => "string",
            Self::Url => "url",
            Self::Color => "color",
            Self::Var => "var",
            Self::ListInt => "list<int>",
            Self::ListReal => "list<real>",
            Self::ListString => "list<string>",
            Self::Point => "point",
            Self::Size => "size",
            Self::Rect => "rect",
            Self::Font => "font",
        }
    }

    /// The kind of value a binding of this type is written with.
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Self::Int | Self::Real | Self::Double => Kind::Num,
            Self::Bool => Kind::Bool,
            Self::String => Kind::Str,
            Self::Url => Kind::Url,
            Self::Color => Kind::Color,
            Self::Var => Kind::Any,
            Self::ListInt | Self::ListReal | Self::ListString => Kind::Arr,
            Self::Point => Kind::Point,
            Self::Size => Kind::Size,
            Self::Rect => Kind::Rect,
            Self::Font => Kind::Font,
        }
    }

    /// The helper of the case that serialises a value of this type.
    #[must_use]
    pub fn serialiser(&self) -> &'static str {
        match self {
            Self::Url => "__url",
            Self::Color => "__color",
            Self::ListInt | Self::ListReal | Self::ListString => "__list",
            Self::Point => "__point",
            Self::Size => "__size",
            Self::Rect => "__rect",
            Self::Font => "__font",
            _ => "__ser",
        }
    }

    /// The members a value of this type has that can be read and written
    /// one at a time, with their kind.
    #[must_use]
    pub fn members(&self) -> &'static [(&'static str, Kind)] {
        match self {
            Self::Point => &[("x", Kind::Num), ("y", Kind::Num)],
            Self::Size => &[("width", Kind::Num), ("height", Kind::Num)],
            Self::Rect => &[
                ("x", Kind::Num),
                ("y", Kind::Num),
                ("width", Kind::Num),
                ("height", Kind::Num),
            ],
            Self::Color => &[
                ("r", Kind::Num),
                ("g", Kind::Num),
                ("b", Kind::Num),
                ("a", Kind::Num),
            ],
            Self::Font => &[
                ("pixelSize", Kind::Num),
                ("bold", Kind::Bool),
                ("italic", Kind::Bool),
                ("underline", Kind::Bool),
            ],
            _ => &[],
        }
    }
}

/// An expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// Literal JavaScript.
    Lit(String, Kind),
    /// A property of an object, by id: `o1.p3`; the object itself when
    /// `prop` is empty.
    Ref {
        obj: String,
        prop: String,
        kind: Kind,
    },
    /// A parameter of the enclosing function or handler.
    Param(String, Kind),
    /// A node printed from `tpl`, where `$0`… are its arguments.
    Op {
        label: &'static str,
        tpl: String,
        args: Vec<Expr>,
        kind: Kind,
    },
}

impl Expr {
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Self::Lit(_, k) | Self::Param(_, k) => *k,
            Self::Ref { kind, .. } | Self::Op { kind, .. } => *kind,
        }
    }

    /// What the node is, for a bucket's signature.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Lit(..) => "literal",
            Self::Ref { .. } => "ref",
            Self::Param(..) => "param",
            Self::Op { label, .. } => label,
        }
    }

    pub fn op(label: &'static str, tpl: impl Into<String>, kind: Kind, args: Vec<Expr>) -> Self {
        Self::Op {
            label,
            tpl: tpl.into(),
            args,
            kind,
        }
    }

    /// Every node, depth first, parents before children.
    pub fn visit_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        f(self);
        if let Self::Op { args, .. } = self {
            for a in args {
                a.visit_mut(f);
            }
        }
    }

    pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
        f(self);
        if let Self::Op { args, .. } = self {
            for a in args {
                a.visit(f);
            }
        }
    }

    /// The labels of every node.
    pub fn labels(&self, out: &mut BTreeSet<&'static str>) {
        self.visit(&mut |e| {
            out.insert(e.label());
        });
    }
}

/// How a property comes to be.
#[derive(Clone, Debug, PartialEq)]
pub enum Decl {
    /// `[readonly] property <type> name`.
    New { ty: PType, readonly: bool },
    /// `property alias name: target`, where target is `obj.prop` or
    /// `obj.prop.member`.
    Alias { target: String, ty: PType },
    /// A property the object's type has: `width`, `font.bold`.
    Builtin { ty: PType },
    /// An enum value, written `Text.AlignRight` or `Case.Two`.
    Enum,
}

/// A property of an object and what it is bound to.
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub name: String,
    pub decl: Decl,
    pub init: Option<Expr>,
    /// Part of a planted binding loop, whose value is not compared.
    pub in_loop: bool,
}

impl Prop {
    /// The type its value is observed as.
    #[must_use]
    pub fn ty(&self) -> PType {
        match &self.decl {
            Decl::New { ty, .. } | Decl::Alias { ty, .. } | Decl::Builtin { ty } => ty.clone(),
            Decl::Enum => PType::Int,
        }
    }

    #[must_use]
    pub fn readonly(&self) -> bool {
        matches!(self.decl, Decl::New { readonly: true, .. })
    }
}

/// What a statement does, in a step, a handler or `Component.onCompleted`.
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// `obj.prop[.member] = value`
    Assign {
        obj: String,
        prop: String,
        member: Option<String>,
        value: Expr,
    },
    /// `obj.prop = Qt.binding(function () { return value })`
    Bind {
        obj: String,
        prop: String,
        value: Expr,
    },
    /// `obj.prop.push(value)` on a list or an array in a `var`.
    Push {
        obj: String,
        prop: String,
        value: Expr,
    },
    /// `return obj.fun(args)`; the result is the step's.
    Call {
        obj: String,
        fun: String,
        args: Vec<Expr>,
        ret: Kind,
    },
    /// `obj.sig(args)`
    Emit {
        obj: String,
        sig: String,
        args: Vec<Expr>,
    },
    /// `root.log += "text" + value + ";"`, which orders what ran.
    Log { text: String, value: Option<Expr> },
}

impl Stmt {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Assign {
                member: Some(_), ..
            } => "member-write",
            Self::Assign { .. } => "set",
            Self::Bind { .. } => "qt-binding",
            Self::Push { .. } => "push",
            Self::Call { .. } => "call",
            Self::Emit { .. } => "emit",
            Self::Log { .. } => "log",
        }
    }

    /// The property it writes, if it writes one.
    #[must_use]
    pub fn target(&self) -> Option<(&str, &str)> {
        match self {
            Self::Assign { obj, prop, .. }
            | Self::Bind { obj, prop, .. }
            | Self::Push { obj, prop, .. } => Some((obj, prop)),
            _ => None,
        }
    }

    pub fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Self::Assign { value, .. } | Self::Bind { value, .. } | Self::Push { value, .. } => {
                vec![value]
            }
            Self::Call { args, .. } | Self::Emit { args, .. } => args.iter_mut().collect(),
            Self::Log { value, .. } => value.iter_mut().collect(),
        }
    }

    pub fn exprs(&self) -> Vec<&Expr> {
        match self {
            Self::Assign { value, .. } | Self::Bind { value, .. } | Self::Push { value, .. } => {
                vec![value]
            }
            Self::Call { args, .. } | Self::Emit { args, .. } => args.iter().collect(),
            Self::Log { value, .. } => value.iter().collect(),
        }
    }
}

/// A JavaScript function of an object.
#[derive(Clone, Debug, PartialEq)]
pub struct Func {
    pub name: String,
    /// Name, kind, and the type annotation if it has one.
    pub params: Vec<(String, Kind, Option<&'static str>)>,
    /// `if (guard) throw new <error>("…")` before the return.
    pub throws: Option<(Expr, &'static str)>,
    pub ret: Expr,
}

/// A signal with typed parameters and its handler.
#[derive(Clone, Debug, PartialEq)]
pub struct Signal {
    pub name: String,
    pub params: Vec<(String, PType)>,
    /// Absent once the shrinker has removed the handler.
    pub handler: Option<Vec<Stmt>>,
}

/// `on<Prop>Changed` of a property of the same object.
#[derive(Clone, Debug, PartialEq)]
pub struct Changed {
    pub prop: String,
    pub body: Vec<Stmt>,
}

/// The types an object can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjType {
    Item,
    QtObject,
    Text,
}

impl ObjType {
    #[must_use]
    pub fn qml(self) -> &'static str {
        match self {
            Self::Item => "Item",
            Self::QtObject => "QtObject",
            Self::Text => "Text",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Obj {
    pub id: String,
    pub ty: ObjType,
    pub props: Vec<Prop>,
    pub funcs: Vec<Func>,
    pub signals: Vec<Signal>,
    pub changed: Vec<Changed>,
    pub completed: Option<Vec<Stmt>>,
    pub children: Vec<Obj>,
}

impl Obj {
    #[must_use]
    pub fn new(id: String, ty: ObjType) -> Self {
        Self {
            id,
            ty,
            props: Vec::new(),
            funcs: Vec::new(),
            signals: Vec::new(),
            changed: Vec::new(),
            completed: None,
            children: Vec::new(),
        }
    }

    /// This object and all below it, parents first.
    pub fn all(&self) -> Vec<&Obj> {
        let mut out = vec![self];
        for c in &self.children {
            out.extend(c.all());
        }
        out
    }

    /// Calls `f` on this object and every one below it, parents first.
    pub fn each_mut(&mut self, f: &mut dyn FnMut(&mut Obj)) {
        f(self);
        for c in &mut self.children {
            c.each_mut(f);
        }
    }

    /// Every expression the object has itself (not its children's).
    pub fn exprs_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        for p in &mut self.props {
            if let Some(e) = &mut p.init {
                f(e);
            }
        }
        for func in &mut self.funcs {
            if let Some((guard, _)) = &mut func.throws {
                f(guard);
            }
            f(&mut func.ret);
        }
        let bodies = self
            .signals
            .iter_mut()
            .filter_map(|s| s.handler.as_mut())
            .chain(self.changed.iter_mut().map(|c| &mut c.body))
            .chain(self.completed.as_mut());
        for body in bodies {
            for st in body {
                for e in st.exprs_mut() {
                    f(e);
                }
            }
        }
    }

    /// The statements of its handlers, for removing those that write a
    /// property that is gone.
    pub fn bodies_mut(&mut self) -> Vec<&mut Vec<Stmt>> {
        self.signals
            .iter_mut()
            .filter_map(|s| s.handler.as_mut())
            .chain(self.changed.iter_mut().map(|c| &mut c.body))
            .chain(self.completed.as_mut())
            .collect()
    }
}

/// A whole case.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub root: Obj,
    /// `enum Shade { … }` of the root, with the value each key is given.
    pub enums: Vec<(String, Vec<(String, Option<i64>)>)>,
    pub steps: Vec<Stmt>,
}

impl Program {
    /// Every expression in the program, objects first, then the steps.
    pub fn exprs_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        self.root.each_mut(&mut |o| o.exprs_mut(f));
        for st in &mut self.steps {
            for e in st.exprs_mut() {
                f(e);
            }
        }
    }

    /// The object with the id.
    #[must_use]
    pub fn obj(&self, id: &str) -> Option<&Obj> {
        self.root.all().into_iter().find(|o| o.id == id)
    }

    #[must_use]
    pub fn prop(&self, obj: &str, prop: &str) -> Option<&Prop> {
        self.obj(obj)?.props.iter().find(|p| p.name == prop)
    }

    /// How many expression nodes there are, which is what the shrinker
    /// makes smaller.
    #[must_use]
    pub fn size(&self) -> usize {
        let mut n = 0;
        let mut copy = self.clone();
        copy.exprs_mut(&mut |e| e.visit(&mut |_| n += 1));
        n + self
            .root
            .all()
            .iter()
            .map(|o| 1 + o.props.len())
            .sum::<usize>()
            + self.steps.len()
    }
}
