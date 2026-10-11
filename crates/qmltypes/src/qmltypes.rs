//! `.qmltypes`: Qt's description of the types a module registers from C++.
//!
//! The file is QML (`Module { Component { Property { } } }`), so it is read
//! with the compiler's own parser. Kept is what a QML file can say about a
//! type; where it is in a header and how C++ gets at it are dropped.

use qml_syntax::{
    Allocator,
    ast::*,
    js::{ArrayExpressionElement, Expression, ObjectPropertyKind, PropertyKey, UnaryOperator},
};

use crate::{
    model::{Class, Enum, Property, Semantics, Signal},
    qmldir::version_of,
};

pub fn read(source: &str) -> Result<Vec<Class>, String> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if let Some(diagnostic) = parsed.diagnostics.first() {
        let (line, _) = diagnostic.position(source);
        return Err(format!("line {line}: {diagnostic}"));
    }
    Ok(objects(&parsed.document.root, "Component")
        .map(class)
        .collect())
}

fn class(object: &QmlObject<'_>) -> Class {
    let mut class = Class::default();
    // An extension that is a JavaScript prototype (`Number` on `int`) adds
    // nothing the JavaScript the compiler emits does not already have.
    let mut extension_is_javascript = false;
    for (name, value) in bindings(object) {
        match name {
            "name" => class.name = text(value),
            "aliases" => class.aliases = texts(value),
            "prototype" => class.prototype = Some(text(value)),
            "extension" => class.extension = Some(text(value)),
            "extensionIsNamespace" => class.extension_is_namespace = flag(value),
            "extensionIsJavaScript" => extension_is_javascript = flag(value),
            "attachedType" => class.attached = Some(text(value)),
            "defaultProperty" => class.default_property = Some(text(value)),
            "valueType" => class.element = Some(text(value)),
            "isSingleton" => class.is_singleton = flag(value),
            "isCreatable" => class.is_uncreatable = !flag(value),
            "hasCustomParser" => class.has_custom_parser = flag(value),
            "accessSemantics" => {
                class.semantics = match text(value).as_str() {
                    "value" => Semantics::Value,
                    "sequence" => Semantics::Sequence,
                    "none" => Semantics::None,
                    _ => Semantics::Reference,
                }
            }
            // "QtQuick/Rectangle 2.0". Only the newest version is kept: it
            // decides between two classes that had the same name in turn.
            "exports" => {
                for export in texts(value) {
                    let Some((name, version)) = export.split_once(' ') else {
                        continue;
                    };
                    let Some((module, name)) = name.split_once('/') else {
                        continue;
                    };
                    let version = version_of(version).unwrap_or_default();
                    let newest = class
                        .exports
                        .entry((module.to_string(), name.to_string()))
                        .or_default();
                    *newest = version.max(*newest);
                }
            }
            _ => {}
        }
    }
    if extension_is_javascript {
        class.extension = None;
    }

    for child in objects(object, "Enum") {
        class.enums.push(enumeration(child));
    }
    for child in objects(object, "Property") {
        let property = property(child);
        if !class
            .properties
            .iter()
            .any(|known| known.name == property.name)
        {
            class.properties.push(property);
        }
    }
    // An overload with fewer arguments comes after the full one; a handler
    // may name every argument, so the longest list is the signal's.
    for child in objects(object, "Signal") {
        let signal = signal(child);
        match class
            .signals
            .iter_mut()
            .find(|known| known.name == signal.name)
        {
            Some(known) if known.parameters.len() < signal.parameters.len() => *known = signal,
            Some(_) => {}
            None => class.signals.push(signal),
        }
    }
    for child in objects(object, "Method") {
        let is_constructor =
            bindings(child).any(|(name, value)| name == "isConstructor" && flag(value));
        let name = named(child);
        // `_q_` slots are how a class talks to itself.
        if !is_constructor && !name.starts_with("_q_") && !class.methods.contains(&name) {
            class.methods.push(name);
        }
    }
    class
}

fn enumeration(object: &QmlObject<'_>) -> Enum {
    let mut enumeration = Enum::default();
    for (name, value) in bindings(object) {
        match name {
            "name" => enumeration.name = text(value),
            "alias" => enumeration.alias = Some(text(value)),
            "isFlag" => enumeration.is_flag = flag(value),
            "values" => enumeration.keys = keys(value),
            _ => {}
        }
    }
    enumeration
}

fn property(object: &QmlObject<'_>) -> Property {
    let mut property = Property::default();
    for (name, value) in bindings(object) {
        match name {
            "name" => property.name = text(value),
            "type" => property.type_name = text(value),
            "isList" => property.is_list = flag(value),
            "isReadonly" => property.is_readonly = flag(value),
            "isPointer" => property.is_pointer = flag(value),
            "isRequired" => property.is_required = flag(value),
            // The function's name is Qt's own business; that there is one
            // is what QML shows (`width = undefined`).
            "reset" => property.has_reset = true,
            _ => {}
        }
    }
    property
}

fn signal(object: &QmlObject<'_>) -> Signal {
    Signal {
        name: named(object),
        parameters: objects(object, "Parameter").map(named).collect(),
    }
}

/// The object's `name`, empty when it has none (a signal argument Qt's
/// header leaves unnamed).
fn named(object: &QmlObject<'_>) -> String {
    bindings(object)
        .find(|(name, _)| *name == "name")
        .map(|(_, value)| text(value))
        .unwrap_or_default()
}

fn bindings<'a, 'b>(
    object: &'b QmlObject<'a>,
) -> impl Iterator<Item = (&'a str, &'b Expression<'a>)> {
    object.members.iter().filter_map(|member| match member {
        QmlMember::Binding(QmlBinding {
            name,
            value: QmlBindingValue::Expression(value),
            ..
        }) => Some((name.as_simple()?, value)),
        _ => None,
    })
}

fn objects<'a, 'b>(
    object: &'b QmlObject<'a>,
    type_name: &'static str,
) -> impl Iterator<Item = &'b QmlObject<'a>> {
    object
        .members
        .iter()
        .filter_map(move |member| match member {
            QmlMember::Object(child) if child.type_name.as_simple() == Some(type_name) => {
                Some(child)
            }
            _ => None,
        })
}

fn text(value: &Expression<'_>) -> String {
    match value {
        Expression::StringLiteral(literal) => literal.value.to_string(),
        _ => String::new(),
    }
}

fn flag(value: &Expression<'_>) -> bool {
    matches!(value, Expression::BooleanLiteral(literal) if literal.value)
}

fn texts(value: &Expression<'_>) -> Vec<String> {
    let Expression::ArrayExpression(array) = value else {
        return Vec::new();
    };
    array
        .elements
        .iter()
        .filter_map(ArrayExpressionElement::as_expression)
        .map(text)
        .collect()
}

/// An enum's keys: a list of names, or in descriptions older than Qt 6 an
/// object of names and numbers.
fn keys(value: &Expression<'_>) -> Vec<(String, Option<i64>)> {
    let Expression::ObjectExpression(object) = value else {
        return texts(value).into_iter().map(|key| (key, None)).collect();
    };
    object
        .properties
        .iter()
        .filter_map(|property| {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return None;
            };
            let key = match &property.key {
                PropertyKey::StringLiteral(key) => key.value.to_string(),
                PropertyKey::StaticIdentifier(key) => key.name.to_string(),
                _ => return None,
            };
            Some((key, number(&property.value)))
        })
        .collect()
}

fn number(value: &Expression<'_>) -> Option<i64> {
    match value {
        Expression::NumericLiteral(literal) => Some(literal.value as i64),
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryNegation => {
            number(&unary.argument).map(|number| -number)
        }
        _ => None,
    }
}
