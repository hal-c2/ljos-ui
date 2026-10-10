//! `type_lookup`: one type of Qt's table, as an agent needs to write against
//! it.

use ljos_mcp_core::SurfaceError;
use serde_json::json;

use super::nearest;
use crate::{ModuleInfo, TypeInfo};

fn unavailable() -> SurfaceError {
    SurfaceError::unavailable(
        "no Qt type table was generated for this build, so types cannot be looked up; \
         run `mise run types` and rebuild",
    )
}

/// The error for a module the table does not have, with the ones it does that
/// are nearest in spelling.
fn no_such_module(module: &str) -> SurfaceError {
    let suggestions = nearest(module, qt_types::modules(), 5);
    SurfaceError::not_found(format!("module `{module}` is not in Qt's type table"))
        .with_details(json!({ "suggestions": suggestions }))
}

pub(super) fn module_info(module: &str) -> Result<ModuleInfo, SurfaceError> {
    if !qt_types::is_available() {
        return Err(unavailable());
    }
    if qt_types::module(module).is_none() {
        return Err(no_such_module(module));
    }
    // Types some C++ registers and types Qt writes as QML files are both
    // written the same in a file.
    let mut types: Vec<String> = qt_types::native_types(module)
        .into_iter()
        .chain(qt_types::written_types(module))
        .flatten()
        .map(str::to_owned)
        .collect();
    types.sort();
    types.dedup();
    Ok(ModuleInfo {
        module: module.to_owned(),
        types,
    })
}

pub(super) fn type_info(module: &str, name: &str) -> Result<TypeInfo, SurfaceError> {
    if !qt_types::is_available() {
        return Err(unavailable());
    }
    let Some(found) = qt_types::module(module) else {
        return Err(no_such_module(module));
    };
    let Some(ty) = found.type_named(name) else {
        let all = qt_types::native_types(module)
            .into_iter()
            .chain(qt_types::written_types(module))
            .flatten();
        let suggestions = nearest(name, all, 5);
        return Err(SurfaceError::not_found(format!(
            "`{name}` is not a type of module `{module}`"
        ))
        .with_details(json!({ "suggestions": suggestions })));
    };

    let mut base_chain = Vec::new();
    let mut base = ty.prototype();
    while let Some(b) = base {
        base_chain.push(label(b).to_owned());
        base = b.prototype();
    }

    let properties = ty
        .properties
        .iter()
        .map(|p| {
            let mut text = format!("{}: {}", p.name, qml_type(p));
            if p.is_list {
                text.push_str(" [list]");
            }
            if p.is_readonly {
                text.push_str(" [readonly]");
            }
            if p.is_required {
                text.push_str(" [required]");
            }
            text
        })
        .collect();
    let signals = ty
        .signals
        .iter()
        .map(|s| format!("{}({})", s.name, s.parameters.join(", ")))
        .collect();
    let enums = ty
        .enums
        .iter()
        .map(|e| {
            let keys: Vec<&str> = e.keys.iter().map(|(k, _)| *k).collect();
            let flags = if e.is_flag { " (flags)" } else { "" };
            format!("{}{flags}: {}", e.name, keys.join(", "))
        })
        .collect();

    Ok(TypeInfo {
        module: module.to_owned(),
        name: name.to_owned(),
        base_chain,
        default_property: ty.default_property().map(str::to_owned),
        properties,
        signals,
        methods: ty.methods.iter().map(|m| (*m).to_owned()).collect(),
        enums,
        attached: ty.attached().map(|a| label(a).to_owned()),
    })
}

/// A type's QML name, or its class when no module exports it.
fn label(ty: &qt_types::Type) -> &str {
    if ty.name.is_empty() {
        ty.class
    } else {
        ty.name
    }
}

/// The name a QML author knows a property's type by.
fn qml_type(property: &qt_types::Property) -> String {
    let name = property.type_name;
    let mapped = match name {
        "QString" => Some("string"),
        "double" | "qreal" | "float" => Some("real"),
        "int" | "qint32" => Some("int"),
        "bool" => Some("bool"),
        "QUrl" => Some("url"),
        "QColor" => Some("color"),
        "QFont" => Some("font"),
        "QVariant" => Some("var"),
        "QQmlComponent" => Some("Component"),
        "QDateTime" | "QDate" => Some("date"),
        "QPointF" => Some("point"),
        "QSizeF" => Some("size"),
        "QRectF" => Some("rect"),
        _ => None,
    };
    if let Some(mapped) = mapped {
        return mapped.to_owned();
    }
    match property.value_type() {
        Some(ty) => label(ty).to_owned(),
        None => name.to_owned(),
    }
}
