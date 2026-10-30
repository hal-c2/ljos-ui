//! `type_lookup`: one type of the table of types, as an agent needs to write
//! against it. The table is Qt's where the build has one and Ljós's own
//! description of its types where it has not (`qt_types`), so there is
//! always one to look in; what is not found in a build without Qt's says
//! that Qt may have it all the same.

use std::path::Path;

use ljos_blocks::{DumpedType, DumpedTypes};
use ljos_mcp_core::SurfaceError;
use serde_json::json;

use super::nearest;
use crate::{ModuleInfo, TypeInfo};

/// What a name that was not found is followed by where only Ljós's own
/// types are known.
fn not_found(what: String) -> SurfaceError {
    SurfaceError::not_found(if qt_types::has_qt_table() {
        what
    } else {
        format!("{what} ({})", qt_types::WITHOUT_QT_TABLE)
    })
}

/// The error for a module the table does not have, with the ones it does that
/// are nearest in spelling.
fn no_such_module(module: &str) -> SurfaceError {
    let suggestions = nearest(module, qt_types::modules(), 5);
    not_found(format!("module `{module}` is not in the table of types"))
        .with_details(json!({ "suggestions": suggestions }))
}

pub(super) fn module_info(module: &str) -> Result<ModuleInfo, SurfaceError> {
    if qt_types::module(module).is_none() {
        return Err(no_such_module(module));
    }
    // Native types and types that are QML files are both written the same
    // in a file.
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
    let Some(found) = qt_types::module(module) else {
        return Err(no_such_module(module));
    };
    let Some(ty) = found.type_named(name) else {
        let all = qt_types::native_types(module)
            .into_iter()
            .chain(qt_types::written_types(module))
            .flatten();
        let suggestions = nearest(name, all, 5);
        return Err(
            not_found(format!("`{name}` is not a type of module `{module}`"))
                .with_details(json!({ "suggestions": suggestions })),
        );
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
        .map(|s| {
            let arguments: Vec<String> = s
                .parameters
                .iter()
                .zip(&s.parameter_types)
                .map(|(name, ty)| argument(name, ty))
                .collect();
            format!("{}({})", s.name, arguments.join(", "))
        })
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

/// The types of the project's `ljostypes.json` files, for what the table
/// does not have.
fn project_types(root: &Path) -> Option<DumpedTypes> {
    DumpedTypes::from_project(root).ok()
}

pub(super) fn project_module_info(root: &Path, module: &str) -> Option<ModuleInfo> {
    Some(ModuleInfo {
        module: module.to_owned(),
        types: project_types(root)?.module_types(module)?,
    })
}

/// The type as the project's type dump describes it. As for a type of the
/// table, what it lists is the type's own and `base_chain` says the rest.
///
/// `None` for a module the project does not describe; a name its module
/// does not have is refused as one the table does not have is.
pub(super) fn project_type_info(
    root: &Path,
    module: &str,
    name: &str,
) -> Option<Result<TypeInfo, SurfaceError>> {
    let types = project_types(root)?;
    let names = types.module_types(module)?;
    let Some(ty) = types.type_named(module, name) else {
        let suggestions = nearest(name, names.iter().map(String::as_str), 5);
        return Some(Err(not_found(format!(
            "`{name}` is not a type of module `{module}`"
        ))
        .with_details(json!({ "suggestions": suggestions }))));
    };

    let mut base_chain = Vec::new();
    let mut default_property = ty.default_property.clone();
    let mut at = ty;
    loop {
        let Some(base) = types.base_of(at) else {
            base_chain.extend(toolkit_chain(at));
            break;
        };
        // A chain that comes back to itself has told all it has.
        if base_chain.contains(&base.name) {
            break;
        }
        base_chain.push(base.name.clone());
        default_property = default_property.or_else(|| base.default_property.clone());
        at = base;
    }

    let properties = ty
        .properties
        .iter()
        .map(|p| {
            let mut text = format!("{}: {}", p.name, p.ty);
            if p.list {
                text.push_str(" [list]");
            }
            if p.readonly {
                text.push_str(" [readonly]");
            }
            if p.required {
                text.push_str(" [required]");
            }
            text
        })
        .collect();
    let signals = ty
        .own_signals()
        .iter()
        .map(|s| {
            let arguments: Vec<String> = s
                .parameters
                .iter()
                .map(|p| argument(&p.name, &p.ty))
                .collect();
            format!("{}({})", s.name, arguments.join(", "))
        })
        .collect();
    let enums = ty
        .enums
        .iter()
        .map(|e| {
            let keys: Vec<&str> = e.keys.iter().map(|(k, _)| k.as_str()).collect();
            format!("{}: {}", e.name, keys.join(", "))
        })
        .collect();
    Some(Ok(TypeInfo {
        module: module.to_owned(),
        name: name.to_owned(),
        base_chain,
        default_property,
        properties,
        signals,
        methods: ty.methods.iter().map(|m| m.name.clone()).collect(),
        enums,
        attached: ty.attached.clone(),
    }))
}

/// The bases of a dumped type from the first one the dump leaves to the
/// toolkit: that one's name, whether or not the table has it, and the
/// table's chain above it.
fn toolkit_chain(ty: &DumpedType) -> Vec<String> {
    let Some(base) = &ty.base else {
        return Vec::new();
    };
    let mut chain = vec![base.clone()];
    let found = ty
        .base_module
        .as_deref()
        .and_then(qt_types::module)
        .and_then(|m| m.type_named(base));
    let mut above = found.and_then(qt_types::Type::prototype);
    while let Some(b) = above {
        chain.push(label(b).to_owned());
        above = b.prototype();
    }
    chain
}

/// A type's QML name, or its class when no module exports it.
fn label(ty: &qt_types::Type) -> &str {
    if ty.name.is_empty() {
        ty.class
    } else {
        ty.name
    }
}

/// An argument of a signal as a handler's author reads it: its name, and
/// its type when the table has one.
fn argument(name: &str, ty: &str) -> String {
    let name = if name.is_empty() && !ty.is_empty() {
        "_"
    } else {
        name
    };
    if ty.is_empty() {
        return name.to_owned();
    }
    let known = qml_value_name(ty);
    format!("{name}: {}", known.unwrap_or(ty))
}

/// The name a QML author knows a property's type by.
fn qml_type(property: &qt_types::Property) -> String {
    let name = property.type_name;
    if let Some(mapped) = qml_value_name(name) {
        return mapped.to_owned();
    }
    match property.value_type() {
        Some(ty) => label(ty).to_owned(),
        None => name.to_owned(),
    }
}

/// The QML name of a value type the table writes by its C++ class.
fn qml_value_name(name: &str) -> Option<&'static str> {
    match name {
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
    }
}
