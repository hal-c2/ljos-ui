//! What a delegate's `model` is, so that the pass can write it in Ljós's
//! terms (docs/DIALECT.md §4.14): Qt's `model.<role>` is `modelData.<role>`,
//! `model.index` is `index` and `model.modelData` is `modelData`.
//!
//! `modelData` is the row object only of a model of several roles; of a
//! model of one role it is that role's value, and of an array its element.
//! So `model.city` is written `modelData.city` only where the file shows a
//! model of several roles, and the file is kept where it does not.

use oxc_ast::ast::{ArrayExpressionElement, Expression};
use qml_syntax::ast::{QmlBindingValue, QmlMember, QmlObject};

/// What the rows of a view's model are, as far as the file shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Rows {
    /// A count, or an array of values that are not objects: `modelData` is
    /// the value, and there are no roles.
    Values,
    /// An array of objects: `modelData` is the object, and there are no
    /// roles.
    Objects,
    /// A `ListModel` of this one role, which is then `modelData`.
    OneRole(String),
    /// A `ListModel` of several roles: `modelData` is the row.
    Roles,
    /// Not shown.
    Unknown,
}

/// The roles a `ListModel` written in the file gives: those its
/// `ListElement`s bind.
pub(crate) fn listed_roles(model: &QmlObject<'_>) -> Vec<String> {
    let mut roles: Vec<String> = Vec::new();
    for member in &model.members {
        let QmlMember::Object(element) = member else {
            continue;
        };
        if element.type_name.as_simple() != Some("ListElement") {
            continue;
        }
        for member in &element.members {
            if let QmlMember::Binding(binding) = member
                && let Some(name) = binding.name.as_simple()
                && !roles.iter().any(|r| r == name)
            {
                roles.push(name.to_string());
            }
        }
    }
    roles
}

/// What `roles` make of a `ListModel`. One that has an id may be given
/// other roles by a script (`append`), which would make a model of one
/// listed role one of several: only an inline one is known to keep it.
pub(crate) fn of_roles(mut roles: Vec<String>, has_id: bool) -> Rows {
    match roles.len() {
        0 => Rows::Unknown,
        1 if has_id => Rows::Unknown,
        1 => Rows::OneRole(roles.remove(0)),
        _ => Rows::Roles,
    }
}

/// What the `model:` binding of the view `view` gives, with `listed` the
/// ListModels of the file that have ids and their rows, and `tables` where
/// the ListModels written as lists of objects start: the rows of those are
/// objects with the roles as members, read as a model of several roles is.
pub(crate) fn of_view(view: &QmlObject<'_>, listed: &[(String, Rows)], tables: &[u32]) -> Rows {
    let value = view.members.iter().find_map(|member| match member {
        QmlMember::Binding(binding) if binding.name.as_simple() == Some("model") => {
            Some(&binding.value)
        }
        _ => None,
    });
    match value {
        Some(QmlBindingValue::Expression(expression)) => of_expression(expression, listed),
        Some(QmlBindingValue::Object(object)) if tables.contains(&object.span.start) => Rows::Roles,
        Some(QmlBindingValue::Object(object))
            if object.type_name.as_simple() == Some("ListModel") =>
        {
            let has_id = object.members.iter().any(|member| {
                matches!(member, QmlMember::Binding(b) if b.name.as_simple() == Some("id"))
            });
            of_roles(listed_roles(object), has_id)
        }
        _ => Rows::Unknown,
    }
}

fn of_expression(expression: &Expression<'_>, listed: &[(String, Rows)]) -> Rows {
    match expression {
        Expression::NumericLiteral(_) => Rows::Values,
        Expression::ParenthesizedExpression(inner) => of_expression(&inner.expression, listed),
        Expression::ArrayExpression(array) => {
            let objects = array
                .elements
                .iter()
                .filter(|e| matches!(e, ArrayExpressionElement::ObjectExpression(_)))
                .count();
            let values = array
                .elements
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        ArrayExpressionElement::StringLiteral(_)
                            | ArrayExpressionElement::NumericLiteral(_)
                            | ArrayExpressionElement::BooleanLiteral(_)
                            | ArrayExpressionElement::TemplateLiteral(_)
                    )
                })
                .count();
            let all = array.elements.len();
            if all > 0 && objects == all {
                Rows::Objects
            } else if values == all {
                Rows::Values
            } else {
                Rows::Unknown
            }
        }
        Expression::Identifier(id) => listed
            .iter()
            .find(|(name, _)| *name == id.name.as_str())
            .map_or(Rows::Unknown, |(_, rows)| rows.clone()),
        _ => Rows::Unknown,
    }
}

/// Whether the names a delegate's row gives are members of an object, as
/// far as the pass can tell: such a member is read instead of the row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Owns {
    pub model: Option<bool>,
    pub model_data: Option<bool>,
    pub index: Option<bool>,
}

impl Owns {
    pub(crate) const NONE: Owns = Owns {
        model: Some(false),
        model_data: Some(false),
        index: Some(false),
    };

    pub(crate) fn get(self, name: &str) -> Option<bool> {
        match name {
            "model" => self.model,
            "modelData" => self.model_data,
            "index" => self.index,
            _ => Some(false),
        }
    }

    /// Either: the root of a component made in a delegate hides the row
    /// too.
    pub(crate) fn or(self, other: Owns) -> Owns {
        let or = |a: Option<bool>, b: Option<bool>| match (a, b) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        };
        Owns {
            model: or(self.model, other.model),
            model_data: or(self.model_data, other.model_data),
            index: or(self.index, other.index),
        }
    }
}

/// The delegate the pass is in.
#[derive(Clone, Debug)]
pub(crate) struct Delegate {
    pub rows: Rows,
    /// What its root has.
    pub owns: Owns,
}
