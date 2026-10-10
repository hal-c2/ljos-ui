//! Small readers over the QML and JavaScript trees, shared by the checks and
//! the lints.

use std::collections::HashSet;

use oxc_ast_visit::{Visit, walk};
use qml_syntax::{
    ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject},
    js::{
        BindingIdentifier, CallExpression, Expression, IdentifierReference, TSType,
        TSTypeAnnotation, TSTypeName,
    },
};

/// What one script (a binding's expression or statement, or a function)
/// declares, uses and calls.
#[derive(Default)]
pub(super) struct Script<'a> {
    /// Every name the script declares anywhere in it (parameters, `var`,
    /// `let`, functions, catch bindings). Treating them as visible
    /// everywhere is imprecise, and imprecise in the safe direction: it can
    /// only hide an unresolved name, never invent one.
    pub locals: HashSet<&'a str>,
    /// Free-looking references with their byte offsets.
    pub refs: Vec<(&'a str, u32)>,
    /// Offsets of `qsTr(...)` calls whose first argument is not a literal.
    pub dynamic_tr: Vec<u32>,
}

impl<'a> Visit<'a> for Script<'a> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.refs.push((it.name.as_str(), it.span.start));
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.locals.insert(it.name.as_str());
    }

    // QML's `function f(x: int): string` carries TypeScript annotations; the
    // names in them are types, not references.
    fn visit_ts_type(&mut self, _: &TSType<'a>) {}
    fn visit_ts_type_annotation(&mut self, _: &TSTypeAnnotation<'a>) {}
    fn visit_ts_type_name(&mut self, _: &TSTypeName<'a>) {}

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::Identifier(callee) = &it.callee
            && callee.name == "qsTr"
        {
            let literal = match it.arguments.first().and_then(|a| a.as_expression()) {
                Some(Expression::StringLiteral(_)) => true,
                Some(Expression::TemplateLiteral(t)) => t.expressions.is_empty(),
                _ => false,
            };
            if !literal {
                self.dynamic_tr.push(it.span.start);
            }
        }
        walk::walk_call_expression(self, it);
    }
}

/// The value of the binding `path` on `object`, found among its bindings and
/// inside group notation (`icon { source: "x" }`, `Accessible { name: "x" }`)
/// alike.
pub(super) fn find_binding<'o, 'a>(
    object: &'o QmlObject<'a>,
    path: &[&str],
) -> Option<&'o QmlBindingValue<'a>> {
    for member in &object.members {
        match member {
            QmlMember::Binding(QmlBinding { name, value, .. })
                if name.parts.iter().copied().eq(path.iter().copied()) =>
            {
                return Some(value);
            }
            QmlMember::Object(child) if child.on.is_none() => {
                let parts = &child.type_name.parts;
                if parts.len() < path.len()
                    && parts
                        .iter()
                        .copied()
                        .eq(path[..parts.len()].iter().copied())
                    && let Some(found) = find_binding(child, &path[parts.len()..])
                {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn has_binding(object: &QmlObject<'_>, path: &[&str]) -> bool {
    find_binding(object, path).is_some()
}

/// The object says something to assistive technology about itself, or
/// declares that it says nothing.
pub(super) fn is_accessibly_described(object: &QmlObject<'_>) -> bool {
    has_binding(object, &["Accessible", "name"]) || has_binding(object, &["Accessible", "ignored"])
}

/// A string literal's text, if the expression is exactly one.
pub(super) fn string_literal<'e>(value: &'e QmlBindingValue<'_>) -> Option<&'e str> {
    match value {
        QmlBindingValue::Expression(Expression::StringLiteral(s)) => Some(s.value.as_str()),
        QmlBindingValue::Expression(Expression::TemplateLiteral(t)) if t.expressions.is_empty() => {
            t.quasis.first()?.value.cooked.as_ref().map(|c| c.as_str())
        }
        _ => None,
    }
}

/// `onClicked` is a handler; `once` is a property.
pub(super) fn is_handler_name(name: &str) -> bool {
    qml_front::scope::handled(name).is_some()
}

/// Every object written in `object`, directly, in a group or in a value. Inline
/// components are not among them: they have no parent.
pub(super) fn child_objects<'o, 'a>(object: &'o QmlObject<'a>) -> Vec<&'o QmlObject<'a>> {
    let mut out = Vec::new();
    for member in &object.members {
        match member {
            QmlMember::Object(child) if qml_front::scope::is_group(child) => {
                out.extend(child_objects(child));
            }
            QmlMember::Object(child) => out.push(child),
            QmlMember::Binding(QmlBinding { value, .. }) => push_values(value, &mut out),
            QmlMember::Property(p) => {
                if let Some(value) = &p.value {
                    push_values(value, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

fn push_values<'o, 'a>(value: &'o QmlBindingValue<'a>, out: &mut Vec<&'o QmlObject<'a>>) {
    match value {
        QmlBindingValue::Object(o) => out.push(o),
        QmlBindingValue::Objects(os) => out.extend(os.iter()),
        _ => {}
    }
}
