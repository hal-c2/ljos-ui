//! What a type of Ljos.Core or Ljos.Ui has of its own, as a value: its
//! homes for what was `Qt`'s (docs/DIALECT.md §4.3.1). The checker types
//! them, and a script reads them off the type, not off what it attaches.

use qml_front::{TypeOracle, types::Kind};

/// Each type by module and name, with the members it has of its own.
const STATICS: &[(&str, &str, &[&str])] = &[
    ("QtQml", "Locale", &["current", "named"]),
    ("QtQuick", "Font", &["of", "families"]),
    ("QtQml", "Component", &["load", "createFromText"]),
];

/// Whether `member` is one the type `kind` has of its own.
pub(crate) fn has_static<O: TypeOracle>(oracle: &O, kind: &Kind<O::Type>, member: &str) -> bool {
    let Kind::Builtin(ty) = kind else {
        return false;
    };
    STATICS.iter().any(|(uri, name, members)| {
        members.contains(&member) && oracle.module_type(uri, name).as_ref() == Some(ty)
    })
}
