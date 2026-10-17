//! What a free name in a script is: QML's context chain, walked before the
//! program runs.
//!
//! A name JavaScript scoping leaves free (no local, parameter or function of
//! the script declares it) is looked up the way QML does at run time:
//!
//! 1. the ids of the component the expression is in, which "always take
//!    precedence over other property names",
//! 2. the object the expression belongs to (its scope object),
//! 3. the root of its component,
//! 4. what a delegate is given (`index`, `modelData`, the model's roles),
//! 5. the components around it, each by its ids, its root and what it is
//!    given, the nearest first,
//! 6. a type or namespace the file imports (capitalised names),
//! 7. what QML and JavaScript put in every scope.
//!
//! `pragma ComponentBehavior: Bound` changes the order: the ids of every
//! component around come before any property, and a delegate is given
//! nothing in its context (what the model has comes through `required`
//! properties).
//!
//! Finding which names are free is the caller's: it needs JavaScript scope
//! analysis of the script, which the back end does anyway.

use crate::{
    oracle::TypeOracle,
    project::Source,
    scope::Tree,
    types::{Found, Types},
};

/// What a name is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target<T> {
    /// The object with that id, by its index in the [`Tree`].
    Id(usize),
    /// A member of an object: one it declares or one of its type.
    Property {
        object: usize,
        name: String,
    },
    /// What the context of a delegate is given: `index`, `model`,
    /// `modelData`, or a role of the model.
    Context {
        context: usize,
        name: String,
    },
    /// A type, or what a singleton is.
    Type(Found<T>),
    /// What the file imports `as` the name.
    Namespace(Source),
    Global(Global),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Global {
    /// `Qt`, `qsTr`, `print`: what QML puts in every script's scope.
    Qml,
    /// `Math`, `JSON`, `undefined`: JavaScript's own.
    Js,
}

/// Where a name is found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution<T> {
    /// A PropertyChanges whose target is not named by its id: the name is
    /// the target's if the target has it, which only the running program
    /// knows, and [`target`](Self::target) otherwise.
    pub aimed: Option<usize>,
    /// The contexts of the delegates whose model may have a role of the name,
    /// the innermost first: what the model has, only the running model knows.
    /// A role found there comes before [`target`](Self::target).
    pub roles: Vec<usize>,
    /// What the name is when no role and no aimed target has it. None when
    /// only those might.
    pub target: Option<Target<T>>,
}

impl<T> Resolution<T> {
    fn found(target: Target<T>) -> Self {
        Self {
            aimed: None,
            roles: Vec::new(),
            target: Some(target),
        }
    }
}

/// Looks names up in one file.
#[derive(Debug)]
pub struct Names<'t, 'a, 'p, O: TypeOracle> {
    pub tree: &'t Tree<'a, O::Type>,
    pub types: Types<'p, O>,
}

impl<O: TypeOracle> Names<'_, '_, '_, O> {
    /// What the free name `name` written at byte `offset` of the file is.
    /// None when it is nothing: an error, unless the program makes the name
    /// at run time (see [`crate::project::Project::dynamic_names`]).
    pub fn lookup(&self, name: &str, offset: u32) -> Option<Resolution<O::Type>> {
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return self.capitalised(name).map(Resolution::found);
        }
        let tree = self.tree;
        let aimed = tree.aimed(offset);
        let mut resolution = if tree.is_bound {
            self.bound(name, offset)
        } else {
            self.unbound(name, offset)
        };
        if resolution.target.is_none() && resolution.roles.is_empty() {
            resolution.target = global(name).map(Target::Global);
        }
        resolution.aimed = aimed;
        if resolution.target.is_none() && resolution.roles.is_empty() && aimed.is_none() {
            return None;
        }
        // A name only the model may have: the outermost model is the last
        // place it is looked for.
        if resolution.target.is_none()
            && let Some(context) = resolution.roles.pop()
        {
            resolution.target = Some(Target::Context {
                context,
                name: name.to_string(),
            });
        }
        Some(resolution)
    }

    /// Whether the object has the member `name`: one its type or the file
    /// says it has, or one Ljós adds to a type of Qt's table, which that
    /// table does not list.
    fn has(&self, object: usize, name: &str) -> bool {
        let tree = self.tree;
        tree.member(self.types, object, name).is_some()
            || tree.objects[object]
                .kind
                .as_ref()
                .and_then(|kind| self.types.base(kind))
                .is_some_and(|ty| self.types.oracle.adds_member(ty, name))
    }

    /// The default order: each context in turn, by its ids, its scope object
    /// (only the innermost has one), its root and what it is given.
    fn unbound(&self, name: &str, offset: u32) -> Resolution<O::Type> {
        let tree = self.tree;
        let inside = tree.object_at(offset);
        let scope = tree.scope_at(offset);
        // The target's names come first, and nothing of the PropertyChanges
        // the change is written in is in sight.
        let aimed = tree.aimed(offset).is_some();
        let mut context = tree.objects[inside].context;
        let mut roles = Vec::new();
        let mut first = true;
        loop {
            if let Some(object) = tree.id_in(context, name) {
                return Resolution {
                    aimed: None,
                    roles,
                    target: Some(Target::Id(object)),
                };
            }
            let root = tree.contexts[context].root;
            let candidates = if first && scope != root && !aimed {
                [Some(scope), Some(root)]
            } else {
                [Some(root), None]
            };
            first = false;
            if let Some(object) = candidates
                .into_iter()
                .flatten()
                .find(|object| self.has(*object, name))
            {
                let target = Target::Property {
                    object,
                    name: name.to_string(),
                };
                return Resolution {
                    aimed: None,
                    roles,
                    target: Some(target),
                };
            }
            if tree.contexts[context].is_delegate {
                // What every delegate is given, whatever the model.
                if matches!(name, "index" | "model" | "modelData") {
                    let target = Target::Context {
                        context,
                        name: name.to_string(),
                    };
                    return Resolution {
                        aimed: None,
                        roles,
                        target: Some(target),
                    };
                }
                roles.push(context);
            }
            match tree.contexts[context].outer {
                Some(outer) => context = outer,
                None => {
                    return Resolution {
                        aimed: None,
                        roles,
                        target: None,
                    };
                }
            }
        }
    }

    /// `pragma ComponentBehavior: Bound`: every id in sight, then the scope
    /// object and the roots, the nearest first. A delegate is given nothing.
    fn bound(&self, name: &str, offset: u32) -> Resolution<O::Type> {
        let tree = self.tree;
        let inside = tree.object_at(offset);
        let scope = tree.scope_at(offset);
        let aimed = tree.aimed(offset).is_some();
        let chain: Vec<usize> =
            std::iter::successors(Some(tree.objects[inside].context), |context| {
                tree.contexts[*context].outer
            })
            .collect();
        if let Some(object) = chain.iter().find_map(|context| tree.id_in(*context, name)) {
            return Resolution::found(Target::Id(object));
        }
        let scope = (scope != tree.contexts[chain[0]].root && !aimed).then_some(scope);
        let objects = scope
            .into_iter()
            .chain(chain.iter().map(|context| tree.contexts[*context].root));
        for object in objects {
            if self.has(object, name) {
                return Resolution::found(Target::Property {
                    object,
                    name: name.to_string(),
                });
            }
        }
        Resolution {
            aimed: None,
            roles: Vec::new(),
            target: None,
        }
    }

    /// A capitalised name: a global, a namespace, or a type.
    fn capitalised(&self, name: &str) -> Option<Target<O::Type>> {
        if let Some(global) = global(name) {
            return Some(Target::Global(global));
        }
        if let Some(source) = self.types.namespace(name) {
            return Some(Target::Namespace(source.clone()));
        }
        self.types.find(&[name]).map(Target::Type)
    }
}

/// Which global `name` is, if it is one.
pub fn global(name: &str) -> Option<Global> {
    if QML_GLOBALS.contains(&name) {
        Some(Global::Qml)
    } else if JS_GLOBALS.contains(&name) {
        Some(Global::Js)
    } else {
        None
    }
}

/// What QML puts in every script's scope, whatever is imported.
pub const QML_GLOBALS: &[&str] = &[
    "Qt",
    "qsTr",
    "qsTrId",
    "qsTranslate",
    "qsTrNoOp",
    "qsTrIdNoOp",
    "qsTranslateNoOp",
    "QT_TR_NOOP",
    "QT_TRID_NOOP",
    "QT_TRANSLATE_NOOP",
    "print",
    "gc",
    "XMLHttpRequest",
];

/// JavaScript's own globals.
pub const JS_GLOBALS: &[&str] = &[
    "undefined",
    "NaN",
    "Infinity",
    "globalThis",
    "console",
    "arguments",
    "eval",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "escape",
    "unescape",
    "encodeURI",
    "encodeURIComponent",
    "decodeURI",
    "decodeURIComponent",
    "Math",
    "JSON",
    "Reflect",
    "Proxy",
    "Object",
    "Function",
    "Array",
    "String",
    "Number",
    "Boolean",
    "Symbol",
    "BigInt",
    "Date",
    "RegExp",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Promise",
    "ArrayBuffer",
    "DataView",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "Error",
    "EvalError",
    "TypeError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "URIError",
    "Intl",
    "URL",
    "URLSearchParams",
];
