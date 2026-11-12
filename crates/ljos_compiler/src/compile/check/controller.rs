//! `controller({ … })` in a module (docs/DIALECT.md §4.20): the spec of a
//! Controller, typed as the Controller it makes. `host` is an `Item`;
//! `self` is the Controller, its properties and, read only, its bindings.

use qml_front::TypeOracle;
use qml_syntax::{
    GetSpan,
    js::{CallExpression, Expression, ObjectExpression, ObjectPropertyKind},
};

use super::ty::{Field, Kind, Param, Sig, Ty};
use super::{Checker, Obj};
use crate::Code;
use crate::controller::{KEYS, MAKER};

impl<'a, O: TypeOracle> Checker<'_, '_, 'a, '_, O> {
    /// Whether `call` is `controller(…)` as every module has it.
    pub(super) fn is_controller(&self, call: &CallExpression<'a>) -> bool {
        self.module.is_some()
            && matches!(&call.callee, Expression::Identifier(id) if id.name == MAKER)
            && self.local(MAKER).is_none()
    }

    /// `controller({ … })`: its spec's members, each typed as what the
    /// Controller is.
    pub(super) fn controller(&mut self, call: &CallExpression<'a>) -> Ty {
        let spec = match call.arguments.first().and_then(|a| a.as_expression()) {
            Some(Expression::ObjectExpression(spec)) if call.arguments.len() == 1 => spec,
            _ => {
                self.report(
                    Code::TsForbidden,
                    "`controller` takes one object: `controller({ properties, bindings, attached })`"
                        .to_string(),
                    call.span,
                );
                return Ty::ERROR;
            }
        };
        let host = match self.c.types.oracle.module_type("QtQuick", "Item") {
            Some(item) => {
                let item = self.obj(Obj::Native(item));
                self.arena.intern(Kind::Object(item))
            }
            None => Ty::ERROR,
        };
        let mut properties = Vec::new();
        let mut bindings = None;
        let mut lifecycle = Vec::new();
        for (name, value, at) in members(spec) {
            match name {
                "properties" => {
                    let given = self.expr(value, None);
                    if let Kind::Record(fields) = self.arena.kind(given) {
                        properties = fields.to_vec();
                    }
                }
                "bindings" => bindings = Some(value),
                "attached" | "detached" | "updated" => lifecycle.push(value),
                other => {
                    let (last, rest) = KEYS.split_last().expect("five keys");
                    let listed: Vec<String> = rest.iter().map(|k| format!("`{k}`")).collect();
                    self.report(
                        Code::TsForbidden,
                        format!(
                            "a controller has {} and `{last}`; `{other}` is none of them",
                            listed.join(", ")
                        ),
                        at,
                    );
                }
            }
        }
        let own = self
            .arena
            .intern(Kind::Record(properties.clone().into_boxed_slice()));
        let bindings = match bindings.map(Expression::without_parentheses) {
            Some(Expression::ObjectExpression(object)) => Some(&**object),
            Some(other) => {
                self.expr(other, None);
                None
            }
            None => None,
        };
        // What each binding gives is learnt first, quietly, from the
        // properties alone: a binding may read another through `self`.
        if let Some(object) = bindings {
            let want = self.lifecycle_sig(host, own);
            self.quiet += 1;
            for (name, value, _) in members(object) {
                let given = self.expr(value, Some(want));
                let ret = match self.arena.kind(given) {
                    Kind::Function(sig) => sig.ret,
                    _ => Ty::ERROR,
                };
                let ty = self.arena.widen(ret);
                properties.push(Field {
                    name: name.into(),
                    ty,
                    optional: false,
                    readonly: true,
                });
            }
            self.quiet -= 1;
        }
        let me = self
            .arena
            .intern(Kind::Record(properties.into_boxed_slice()));
        let want = self.lifecycle_sig(host, me);
        for (_, value, _) in bindings.into_iter().flat_map(members) {
            self.expr(value, Some(want));
        }
        for value in lifecycle {
            self.expr(value, Some(want));
        }
        Ty::ERROR
    }

    /// A function of the host and the Controller, returning anything.
    fn lifecycle_sig(&mut self, host: Ty, me: Ty) -> Ty {
        let param = |name: &str, ty| Param {
            name: name.into(),
            ty,
            optional: false,
            written: None,
        };
        self.arena.function(Sig {
            params: Box::new([param("host", host), param("self", me)]),
            rest: None,
            ret: Ty::ERROR,
            guard: None,
        })
    }
}

/// The members of an object written `name: value`: each name, its value,
/// and where its name is.
fn members<'b, 'a>(
    object: &'b ObjectExpression<'a>,
) -> impl Iterator<Item = (&'b str, &'b Expression<'a>, qml_syntax::Span)> {
    object.properties.iter().filter_map(|p| match p {
        ObjectPropertyKind::ObjectProperty(p) if !p.computed => {
            let name = match &p.key {
                qml_syntax::js::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                qml_syntax::js::PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => return None,
            };
            Some((name, &p.value, p.key.span()))
        }
        _ => None,
    })
}
