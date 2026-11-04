//! The checker's types: those of docs/DIALECT.md's TypeScript subset, as
//! the brief's TYPES.md maps QML onto them, interned so that a type is a
//! number and the same type is made once.

use std::collections::HashMap;

/// A type, by its place in the [`Arena`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct Ty(u32);

impl Ty {
    /// What an error already reported gives: it takes and goes anywhere,
    /// so one mistake is said once.
    pub(crate) const ERROR: Ty = Ty(0);
    pub(crate) const UNKNOWN: Ty = Ty(1);
    pub(crate) const NEVER: Ty = Ty(2);
    pub(crate) const UNDEFINED: Ty = Ty(3);
    pub(crate) const VOID: Ty = Ty(4);
    pub(crate) const BOOLEAN: Ty = Ty(5);
    pub(crate) const NUMBER: Ty = Ty(6);
    pub(crate) const STRING: Ty = Ty(7);
    /// A `color`: a string to TypeScript, said as a colour.
    pub(crate) const COLOR: Ty = Ty(8);
    pub(crate) const TRUE: Ty = Ty(9);
    pub(crate) const FALSE: Ty = Ty(10);
}

/// A built-in global of TYPES.md.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Global {
    Math,
    Number,
    String,
    Console,
    /// `Qt`, for its enums' keys.
    Qt,
    QsTr,
    QsTrId,
    /// `Map`, to be made with `new`.
    Map,
}

/// A method of a built-in type or global, bound to what it is read off.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Method {
    /// Of a string, a number or a boolean.
    Primitive(&'static str),
    Array(&'static str),
    Map(&'static str),
    /// `Math.max`, `Number.isFinite`, `console.log`, …
    Static(Global, &'static str),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Field {
    pub(crate) name: Box<str>,
    pub(crate) ty: Ty,
    pub(crate) optional: bool,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Param {
    pub(crate) name: Box<str>,
    pub(crate) ty: Ty,
    pub(crate) optional: bool,
    /// The type as the file writes it (`real`), for a message.
    pub(crate) written: Option<Box<str>>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Sig {
    pub(crate) params: Box<[Param]>,
    /// The element type of a rest parameter.
    pub(crate) rest: Option<Ty>,
    pub(crate) ret: Ty,
}

impl Sig {
    pub(crate) fn required(&self) -> usize {
        self.params.iter().filter(|p| !p.optional).count()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Kind {
    Error,
    Unknown,
    Never,
    Undefined,
    Void,
    Boolean,
    Number,
    String,
    Color,
    Bool(bool),
    /// A number literal type, by its bits.
    Num(u64),
    Str(Box<str>),
    /// Two or more, flat, without duplicates, in order of [`Ty`].
    Union(Box<[Ty]>),
    /// An object of a QML type, by its place in the checker's objects.
    Object(u32),
    /// A QML type as a value: `Text` of `Text.AlignLeft`.
    Type(u32),
    /// What a file imports `as` the name.
    Namespace(Box<str>),
    /// A TypeScript object type. Its fields are in the order written.
    Record(Box<[Field]>),
    Array {
        elem: Ty,
        readonly: bool,
    },
    Map {
        key: Ty,
        value: Ty,
    },
    Function(Box<Sig>),
    Global(Global),
    Method(Ty, Method),
    /// A value of an enum of a QML type: `Text.VAlignment`.
    Enum {
        object: u32,
        name: Box<str>,
    },
    /// One key of a QML type's enums, as written: `Text.AlignTop`.
    Key {
        object: u32,
        key: Box<str>,
    },
    /// An enum of a QML type named as a value, for its keys:
    /// `Image.FillMode` of `Image.FillMode.Stretch`.
    EnumNs {
        object: u32,
        name: Box<str>,
    },
}

/// Every type the checker has made for one file.
pub(crate) struct Arena {
    kinds: Vec<Kind>,
    /// Each type by itself, so that a lone type's members are a slice.
    alone: Vec<Ty>,
    index: HashMap<Kind, Ty>,
}

impl Arena {
    pub(crate) fn new() -> Self {
        let mut arena = Arena {
            kinds: Vec::new(),
            alone: Vec::new(),
            index: HashMap::new(),
        };
        for kind in [
            Kind::Error,
            Kind::Unknown,
            Kind::Never,
            Kind::Undefined,
            Kind::Void,
            Kind::Boolean,
            Kind::Number,
            Kind::String,
            Kind::Color,
            Kind::Bool(true),
            Kind::Bool(false),
        ] {
            arena.intern(kind);
        }
        arena
    }

    pub(crate) fn kind(&self, ty: Ty) -> &Kind {
        &self.kinds[ty.0 as usize]
    }

    pub(crate) fn intern(&mut self, kind: Kind) -> Ty {
        if let Some(ty) = self.index.get(&kind) {
            return *ty;
        }
        let ty = Ty(self.kinds.len() as u32);
        self.kinds.push(kind.clone());
        self.alone.push(ty);
        self.index.insert(kind, ty);
        ty
    }

    pub(crate) fn num(&mut self, value: f64) -> Ty {
        // `-0` and `0` are one literal type.
        let value = if value == 0.0 { 0.0 } else { value };
        self.intern(Kind::Num(value.to_bits()))
    }

    pub(crate) fn str(&mut self, value: &str) -> Ty {
        self.intern(Kind::Str(value.into()))
    }

    pub(crate) fn boolean(&mut self, value: bool) -> Ty {
        if value { Ty::TRUE } else { Ty::FALSE }
    }

    pub(crate) fn array(&mut self, elem: Ty, readonly: bool) -> Ty {
        self.intern(Kind::Array { elem, readonly })
    }

    pub(crate) fn function(&mut self, sig: Sig) -> Ty {
        self.intern(Kind::Function(Box::new(sig)))
    }

    /// The members of `ty`: itself, unless it is a union.
    pub(crate) fn parts(&self, ty: Ty) -> &[Ty] {
        match self.kind(ty) {
            Kind::Union(parts) => parts,
            _ => std::slice::from_ref(&self.alone[ty.0 as usize]),
        }
    }

    /// The members of `ty`, owned.
    pub(crate) fn members(&self, ty: Ty) -> Vec<Ty> {
        match self.kind(ty) {
            Kind::Union(parts) => parts.to_vec(),
            _ => vec![ty],
        }
    }

    /// `a | b`.
    pub(crate) fn or(&mut self, a: Ty, b: Ty) -> Ty {
        if a == b {
            return a;
        }
        self.union([a, b])
    }

    /// The union of `types`, made flat: an error or `unknown` takes it all,
    /// `never` is nothing, `true | false` is `boolean`, and a literal goes
    /// into its base type when that is there.
    pub(crate) fn union(&mut self, types: impl IntoIterator<Item = Ty>) -> Ty {
        let mut flat: Vec<Ty> = Vec::new();
        for ty in types {
            match self.kind(ty) {
                Kind::Union(parts) => flat.extend(parts.iter().copied()),
                _ => flat.push(ty),
            }
        }
        if flat.contains(&Ty::ERROR) {
            return Ty::ERROR;
        }
        if flat.contains(&Ty::UNKNOWN) {
            return Ty::UNKNOWN;
        }
        flat.retain(|ty| *ty != Ty::NEVER);
        if flat.contains(&Ty::TRUE) && flat.contains(&Ty::FALSE) {
            flat.push(Ty::BOOLEAN);
        }
        let has = |flat: &[Ty], ty: Ty| flat.contains(&ty);
        let boolean = has(&flat, Ty::BOOLEAN);
        let number = has(&flat, Ty::NUMBER);
        let string = has(&flat, Ty::STRING);
        let kinds = &self.kinds;
        flat.retain(|ty| match &kinds[ty.0 as usize] {
            Kind::Bool(_) => !boolean,
            Kind::Num(_) => !number,
            Kind::Str(_) | Kind::Color => !string,
            _ => true,
        });
        flat.sort_unstable();
        flat.dedup();
        match flat.len() {
            0 => Ty::NEVER,
            1 => flat[0],
            _ => self.intern(Kind::Union(flat.into_boxed_slice())),
        }
    }

    /// The members of `ty` that `keep` keeps.
    pub(crate) fn filter(&mut self, ty: Ty, keep: impl Fn(&Arena, Ty) -> bool) -> Ty {
        let kept: Vec<Ty> = self
            .members(ty)
            .into_iter()
            .filter(|part| keep(self, *part))
            .collect();
        self.union(kept)
    }

    /// The type a literal widens to where a variable may change: `let`, an
    /// inferred return, a field of an object literal.
    pub(crate) fn widen(&mut self, ty: Ty) -> Ty {
        match self.kind(ty) {
            Kind::Bool(_) => Ty::BOOLEAN,
            Kind::Num(_) => Ty::NUMBER,
            Kind::Str(_) => Ty::STRING,
            Kind::Union(parts) => {
                let parts = parts.to_vec();
                let widened: Vec<Ty> = parts.into_iter().map(|p| self.widen(p)).collect();
                self.union(widened)
            }
            Kind::Record(fields) => {
                let fields = fields.to_vec();
                let fields: Vec<Field> = fields
                    .into_iter()
                    .map(|f| Field {
                        ty: self.widen(f.ty),
                        ..f
                    })
                    .collect();
                self.intern(Kind::Record(fields.into_boxed_slice()))
            }
            Kind::Array { elem, readonly } => {
                let (elem, readonly) = (*elem, *readonly);
                let elem = self.widen(elem);
                self.array(elem, readonly)
            }
            _ => ty,
        }
    }

    pub(crate) fn without_undefined(&mut self, ty: Ty) -> Ty {
        self.filter(ty, |_, part| part != Ty::UNDEFINED && part != Ty::VOID)
    }

    pub(crate) fn has_undefined(&self, ty: Ty) -> bool {
        self.parts(ty)
            .iter()
            .any(|p| *p == Ty::UNDEFINED || *p == Ty::VOID)
    }

    /// Whether every member of `ty` is a string, a number or a boolean, or
    /// a literal of one.
    pub(crate) fn is_primitive(&self, ty: Ty) -> bool {
        self.parts(ty).iter().all(|p| {
            matches!(
                self.kind(*p),
                Kind::Boolean
                    | Kind::Number
                    | Kind::String
                    | Kind::Color
                    | Kind::Bool(_)
                    | Kind::Num(_)
                    | Kind::Str(_)
            )
        })
    }

    pub(crate) fn is_number(&self, ty: Ty) -> bool {
        self.parts(ty).iter().all(|p| {
            matches!(
                self.kind(*p),
                Kind::Number | Kind::Num(_) | Kind::Enum { .. } | Kind::Key { .. }
            )
        })
    }

    pub(crate) fn is_string(&self, ty: Ty) -> bool {
        self.parts(ty)
            .iter()
            .all(|p| matches!(self.kind(*p), Kind::String | Kind::Color | Kind::Str(_)))
    }

    pub(crate) fn is_boolean(&self, ty: Ty) -> bool {
        self.parts(ty)
            .iter()
            .all(|p| matches!(self.kind(*p), Kind::Boolean | Kind::Bool(_)))
    }

    /// What `typeof` says of a member of a union, where it can tell.
    pub(crate) fn type_of(&self, ty: Ty) -> Option<&'static str> {
        Some(match self.kind(ty) {
            Kind::Undefined | Kind::Void => "undefined",
            Kind::Boolean | Kind::Bool(_) => "boolean",
            Kind::Number | Kind::Num(_) | Kind::Enum { .. } | Kind::Key { .. } => "number",
            Kind::String | Kind::Color | Kind::Str(_) => "string",
            Kind::Function(_) | Kind::Method(..) => "function",
            Kind::Object(_) | Kind::Record(_) | Kind::Array { .. } | Kind::Map { .. } => "object",
            _ => return None,
        })
    }

    /// Whether `ty` is a literal type, for `switch` and `===` narrowing.
    pub(crate) fn is_literal(&self, ty: Ty) -> bool {
        matches!(
            self.kind(ty),
            Kind::Bool(_) | Kind::Num(_) | Kind::Str(_) | Kind::Undefined
        )
    }

    /// The type as TypeScript writes it.
    pub(crate) fn display(&self, ty: Ty, object: &dyn Fn(u32) -> String) -> String {
        match self.kind(ty) {
            Kind::Error => "error".into(),
            Kind::Unknown => "unknown".into(),
            Kind::Never => "never".into(),
            Kind::Undefined => "undefined".into(),
            Kind::Void => "void".into(),
            Kind::Boolean => "boolean".into(),
            Kind::Number => "number".into(),
            Kind::String => "string".into(),
            Kind::Color => "color".into(),
            Kind::Bool(b) => b.to_string(),
            Kind::Num(bits) => number_text(f64::from_bits(*bits)),
            Kind::Str(s) => format!("\"{s}\""),
            Kind::Union(parts) => parts
                .iter()
                .map(|p| self.display(*p, object))
                .collect::<Vec<_>>()
                .join(" | "),
            Kind::Object(o) | Kind::Type(o) => object(*o),
            Kind::Namespace(name) => name.to_string(),
            Kind::Record(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        let optional = if f.optional { "?" } else { "" };
                        format!("{}{optional}: {}", f.name, self.display(f.ty, object))
                    })
                    .collect();
                format!("{{ {} }}", fields.join("; "))
            }
            Kind::Array { elem, readonly } => {
                let elem_text = self.display(*elem, object);
                let elem_text = if matches!(self.kind(*elem), Kind::Union(_) | Kind::Function(_)) {
                    format!("({elem_text})")
                } else {
                    elem_text
                };
                if *readonly {
                    format!("readonly {elem_text}[]")
                } else {
                    format!("{elem_text}[]")
                }
            }
            Kind::Map { key, value } => format!(
                "Map<{}, {}>",
                self.display(*key, object),
                self.display(*value, object)
            ),
            Kind::Function(sig) => {
                let mut params: Vec<String> = sig
                    .params
                    .iter()
                    .map(|p| {
                        let optional = if p.optional { "?" } else { "" };
                        format!("{}{optional}: {}", p.name, self.display(p.ty, object))
                    })
                    .collect();
                if let Some(rest) = sig.rest {
                    params.push(format!("...rest: {}[]", self.display(rest, object)));
                }
                format!(
                    "({}) => {}",
                    params.join(", "),
                    self.display(sig.ret, object)
                )
            }
            Kind::Global(global) => format!("{global:?}"),
            Kind::Enum { object: o, name } | Kind::EnumNs { object: o, name } => {
                format!("{}.{name}", object(*o))
            }
            Kind::Key { object: o, key } => format!("{}.{key}", object(*o)),
            Kind::Method(_, method) => match method {
                Method::Primitive(name)
                | Method::Array(name)
                | Method::Map(name)
                | Method::Static(_, name) => format!("{name}()"),
            },
        }
    }

    /// The type in a message's words: "a number", "a string or undefined".
    pub(crate) fn word(&self, ty: Ty, object: &dyn Fn(u32) -> String) -> String {
        let mut words: Vec<String> = Vec::new();
        for part in self.parts(ty) {
            let word = match self.kind(*part) {
                Kind::Error => "an error".into(),
                Kind::Unknown => "an unknown value".into(),
                Kind::Never => "never".into(),
                Kind::Undefined | Kind::Void => "undefined".into(),
                Kind::Enum { .. } => article(&self.display(*part, object)),
                Kind::Key { .. } => format!("`{}`", self.display(*part, object)),
                Kind::EnumNs { .. } => format!("the enum {}", self.display(*part, object)),
                Kind::Boolean | Kind::Bool(_) => "a boolean".into(),
                Kind::Number | Kind::Num(_) => "a number".into(),
                Kind::String | Kind::Str(_) => "a string".into(),
                Kind::Color => "a color".into(),
                Kind::Object(o) => article(&object(*o)),
                Kind::Type(o) => format!("the type {}", object(*o)),
                Kind::Namespace(name) => format!("the namespace {name}"),
                Kind::Record(_) => "an object".into(),
                Kind::Array { .. } => article(&self.display(*part, object)),
                Kind::Map { .. } => "a Map".into(),
                Kind::Function(_)
                | Kind::Method(..)
                | Kind::Global(Global::QsTr)
                | Kind::Global(Global::QsTrId) => "a function".into(),
                Kind::Global(global) => format!("{global:?}"),
                Kind::Union(_) => unreachable!("a union's parts are not unions"),
            };
            if !words.contains(&word) {
                words.push(word);
            }
        }
        // What may be missing is said last, as TypeScript writes `T | undefined`.
        if let Some(at) = words.iter().position(|word| word == "undefined") {
            let missing = words.remove(at);
            words.push(missing);
        }
        words.join(" or ")
    }
}

/// `a real`, `an Item`.
pub(crate) fn article(word: &str) -> String {
    let an = word
        .chars()
        .next()
        .is_some_and(|c| "aeiouAEIOU".contains(c));
    if an {
        format!("an {word}")
    } else {
        format!("a {word}")
    }
}

fn number_text(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}
