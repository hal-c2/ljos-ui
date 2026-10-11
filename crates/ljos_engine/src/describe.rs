//! What the engine says of itself that the registry's dump does not carry,
//! for whoever writes a table of types from a running engine (the front end
//! reads QML against such a table, and cannot link the engine to ask).
//!
//! The registry knows the types and their members. A table also needs the
//! value types and what is under them (`font.bold`), the enums of the `Qt`
//! namespace, which module comes with which, which types are one object or
//! a QML file, and which take more in their body than their own members.
//! Each is said here by the code that implements it, so that none has to be
//! kept in a list beside the table.

use std::cell::RefCell;

use crate::engine::Engine;
use crate::meta::MetaObject;
use crate::types::TypeTag;

/// A value type, by the name QML has for it.
#[derive(Clone, Copy, Debug)]
pub struct ValueType {
    pub tag: fn() -> TypeTag,
    /// The module that has the type, as Qt documents the value types
    /// (doc.qt.io/qt-6/qmlvaluetypes.html): the language's own and the
    /// geometry ones come with `QtQml`, the visual ones with `QtQuick`.
    pub module: &'static str,
    /// The members that can be written one at a time (`font.bold: true`),
    /// each with the name of its type. What [`crate::Value::with_member`]
    /// takes.
    pub members: &'static [(&'static str, &'static str)],
}

const fn plain(tag: fn() -> TypeTag, module: &'static str) -> ValueType {
    ValueType {
        tag,
        module,
        members: &[],
    }
}

const QML: &str = "QtQml";
const QUICK: &str = "QtQuick";

/// Every value type the engine has. A property declared with one holds a
/// copy, not a reference.
pub const VALUE_TYPES: &[ValueType] = &[
    plain(|| TypeTag::Var, QML),
    plain(|| TypeTag::Bool, QML),
    plain(|| TypeTag::Int, QML),
    plain(|| TypeTag::Real, QML),
    plain(|| TypeTag::String, QML),
    plain(|| TypeTag::Url, QML),
    plain(|| TypeTag::Color, QUICK),
    ValueType {
        tag: || TypeTag::Font,
        module: QUICK,
        members: &[
            ("family", "string"),
            ("pointSize", "real"),
            ("pixelSize", "int"),
            ("weight", "int"),
            ("bold", "bool"),
            ("italic", "bool"),
            ("underline", "bool"),
            ("strikeout", "bool"),
            ("letterSpacing", "real"),
            ("wordSpacing", "real"),
            ("capitalization", "int"),
            ("hintingPreference", "int"),
            ("scaleWithSystem", "bool"),
        ],
    },
    ValueType {
        tag: || TypeTag::Point,
        module: QML,
        members: &[("x", "real"), ("y", "real")],
    },
    ValueType {
        tag: || TypeTag::Size,
        module: QML,
        members: &[("width", "real"), ("height", "real")],
    },
    ValueType {
        tag: || TypeTag::Rect,
        module: QML,
        members: &[
            ("x", "real"),
            ("y", "real"),
            ("width", "real"),
            ("height", "real"),
        ],
    },
    ValueType {
        tag: || TypeTag::Vector2d,
        module: QUICK,
        members: &[("x", "real"), ("y", "real")],
    },
    ValueType {
        tag: || TypeTag::Vector3d,
        module: QUICK,
        members: &[("x", "real"), ("y", "real"), ("z", "real")],
    },
    ValueType {
        tag: || TypeTag::Vector4d,
        module: QUICK,
        members: &[("x", "real"), ("y", "real"), ("z", "real"), ("w", "real")],
    },
    ValueType {
        tag: || TypeTag::Quaternion,
        module: QUICK,
        members: &[
            ("scalar", "real"),
            ("x", "real"),
            ("y", "real"),
            ("z", "real"),
        ],
    },
];

/// One enum of the `Qt` namespace (`Qt.AlignLeft`).
#[derive(Clone, Debug)]
pub struct QtNamespaceEnum {
    /// What a property is declared with: `Alignment`. Empty for the keys
    /// the engine has without the name of their enum.
    pub name: &'static str,
    /// For flags, the name of one key's enum (`AlignmentFlag`); else empty.
    pub alias: &'static str,
    pub keys: Vec<(&'static str, i64)>,
}

/// The enums a script reads off `Qt`, which is not a registered type: the
/// engine gives every script that object.
pub fn qt_namespace() -> Vec<QtNamespaceEnum> {
    let mut enums: Vec<QtNamespaceEnum> = crate::qt_enums::QT_ENUMS
        .iter()
        .map(|e| QtNamespaceEnum {
            name: e.name,
            alias: e.alias,
            keys: e.keys.to_vec(),
        })
        .collect();
    let named = |key: &str| enums.iter().any(|e| e.keys.iter().any(|(k, _)| *k == key));
    let rest: Vec<(&'static str, i64)> = crate::js::QT_ENUM_KEYS
        .iter()
        .chain(crate::qt_keys::QT_KEY_ENUMS)
        .filter(|(key, _)| !named(key))
        .copied()
        .collect();
    if !rest.is_empty() {
        enums.push(QtNamespaceEnum {
            name: "",
            alias: "",
            keys: rest,
        });
    }
    enums
}

/// The modules that come with a module.
#[derive(Default)]
struct Imports(RefCell<Vec<(String, String)>>);

/// The types whose body is not only members of theirs.
#[derive(Default)]
struct OpenBodies(RefCell<Vec<&'static MetaObject>>);

impl Engine {
    /// Says that what is written in an object of this type is not only
    /// members of the type: a `Connections` has handlers for the signals of
    /// another object, a `ListElement` the roles of its row. Whatever reads
    /// QML before the engine does then holds no name in such a body to be
    /// a mistake.
    pub fn register_open_body(&self, meta: &'static MetaObject) {
        let bodies = self.extension::<OpenBodies>();
        let mut bodies = bodies.0.borrow_mut();
        if !bodies.iter().any(|known| std::ptr::eq(*known, meta)) {
            bodies.push(meta);
        }
    }

    /// Whether the type itself was registered with
    /// [`Engine::register_open_body`]. What derives from it is as open, and
    /// is not asked about here.
    pub fn has_open_body(&self, meta: &MetaObject) -> bool {
        let bodies = self.extension::<OpenBodies>();
        bodies
            .0
            .borrow()
            .iter()
            .any(|known| std::ptr::eq(*known, meta))
    }

    /// Says that a file which imports `uri` has the types of `imported`
    /// too: `import QtQuick` is also `QtQml`. The engine itself finds a
    /// native type wherever it is registered; this is for what reads QML
    /// before the engine does and has to know which names an import gives.
    pub fn register_import(&self, uri: &str, imported: &str) {
        let imports = self.extension::<Imports>();
        let mut imports = imports.0.borrow_mut();
        if !imports.iter().any(|(a, b)| a == uri && b == imported) {
            imports.push((uri.to_owned(), imported.to_owned()));
        }
    }

    /// The modules that come with `uri` ([`Engine::register_import`]), in
    /// the order they were named.
    pub fn imports_of(&self, uri: &str) -> Vec<String> {
        let imports = self.extension::<Imports>();
        let imports = imports.0.borrow();
        imports
            .iter()
            .filter(|(module, _)| module == uri)
            .map(|(_, imported)| imported.clone())
            .collect()
    }

    /// The library file behind the type `name` of `module`
    /// ([`Engine::register_qml_type`]), when that type is written in QML.
    pub fn qml_type_file(&self, module: &str, name: &str) -> Option<String> {
        self.qml_type_key(module, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    /// The list above is what a table is written from; the code that takes
    /// `font.bold: true` is the truth of it.
    #[test]
    fn the_members_listed_are_the_ones_a_value_takes() {
        let every: Vec<&str> = VALUE_TYPES
            .iter()
            .flat_map(|ty| ty.members.iter().map(|(name, _)| *name))
            .chain(["nothing", "r", "g", "b", "a", "length", "left", "top"])
            .collect();
        for ty in VALUE_TYPES {
            let tag = (ty.tag)();
            for name in &every {
                let listed = ty.members.iter().any(|(member, _)| member == name);
                let taken = Value::Undefined
                    .with_member(&tag, name, &Value::Int(1))
                    .is_some();
                assert_eq!(listed, taken, "{}.{name}", tag.name());
            }
        }
    }

    #[test]
    fn a_member_is_of_a_value_type() {
        let names: Vec<String> = VALUE_TYPES.iter().map(|ty| (ty.tag)().name()).collect();
        for ty in VALUE_TYPES {
            for (member, of) in ty.members {
                assert!(names.iter().any(|name| name == of), "{member}: {of}");
            }
        }
    }

    #[test]
    fn every_key_of_the_namespace_is_in_one_enum() {
        let enums = qt_namespace();
        assert!(
            enums
                .iter()
                .any(|e| e.name == "Alignment" && e.alias == "AlignmentFlag")
        );
        let unnamed: Vec<_> = enums.iter().filter(|e| e.name.is_empty()).collect();
        assert_eq!(unnamed.len(), 1);
        assert!(unnamed[0].keys.iter().any(|(key, _)| *key == "Key_Return"));
        // What an enum lists by name is not listed again without one.
        assert!(!unnamed[0].keys.iter().any(|(key, _)| *key == "AlignLeft"));
    }

    #[test]
    fn a_connections_and_a_list_element_take_more_than_their_members() {
        use crate::QmlType;
        let engine = Engine::new();
        assert!(engine.has_open_body(crate::Connections::static_meta()));
        assert!(engine.has_open_body(crate::ListElement::static_meta()));
        assert!(!engine.has_open_body(crate::ListModel::static_meta()));
        assert!(!engine.has_open_body(crate::QtObject::static_meta()));
    }

    #[test]
    fn a_module_names_what_comes_with_it() {
        let engine = Engine::new();
        engine.register_import("A", "B");
        engine.register_import("A", "C");
        engine.register_import("A", "B");
        assert_eq!(engine.imports_of("A"), ["B", "C"]);
        assert!(engine.imports_of("B").is_empty());
    }
}
