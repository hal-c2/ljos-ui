//! What the types are that no QML file of the project declares.
//!
//! `Item`, `Timer` and `ListView` are not QML files: they are whatever the
//! runtime provides. Which members they have is the runtime's to say, so the
//! front end asks a [`TypeOracle`]. Ljós answers from its own registry of
//! native types; [`QtTypeOracle`] answers from Qt's description of its own
//! types ([`qt_types`]), which is what a file written for Qt expects.

use std::fmt;

/// The types a runtime provides, by the modules that export them.
///
/// Every lookup on a type also sees what it inherits: a `Rectangle` has the
/// `width` of an `Item`.
pub trait TypeOracle {
    /// A handle to one type. Equal handles are the same type.
    type Type: Copy + Eq + fmt::Debug;

    /// Whether `uri` is a module the oracle knows.
    fn has_module(&self, uri: &str) -> bool;

    /// The type `name` is in a file that imports the module `uri`: one of the
    /// module's own, or of a module that comes with it.
    fn module_type(&self, uri: &str, name: &str) -> Option<Self::Type>;

    /// The type's name, for messages.
    fn name(&self, ty: Self::Type) -> &str;

    /// What the front end treats specially about the type, if anything.
    fn special(&self, ty: Self::Type) -> Option<Special>;

    /// The type is one object, there before it is named (`pragma Singleton`,
    /// `QML_SINGLETON`).
    fn is_singleton(&self, ty: Self::Type) -> bool;

    fn property(&self, ty: Self::Type, name: &str) -> Option<Property<Self::Type>>;

    /// The names a handler of the signal `name` may call its arguments; None
    /// when the type has no such signal. A property's change signal is one.
    fn signal(&self, ty: Self::Type, name: &str) -> Option<Vec<String>>;

    fn has_method(&self, ty: Self::Type, name: &str) -> bool;

    /// The property objects written inside one of the type go to.
    fn default_property(&self, ty: Self::Type) -> Option<&str>;

    /// The type of `Name.` written on another object: `Layout.fillWidth` is a
    /// property of what `Layout` attaches.
    fn attached(&self, ty: Self::Type) -> Option<Self::Type>;

    /// Whether `Type.key`, or `Type.scope.key` with `scope`, is a key of an
    /// enum of the type.
    fn has_enum_key(&self, ty: Self::Type, scope: Option<&str>, key: &str) -> bool;

    /// Whether `Type.name` is an enum of the type, of `Type.name.Key`.
    fn has_enum(&self, ty: Self::Type, name: &str) -> bool;

    /// The properties whoever makes an object of the type has to set.
    fn required(&self, ty: Self::Type) -> Vec<String>;

    /// Whether some type has a member called `name`: a name that is most
    /// likely that member wherever it is written.
    fn is_member_name(&self, name: &str) -> bool;
}

/// Types whose objects change how the objects around them are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    /// `Component { }`: what is in it is a template, made later.
    Component,
    /// What it is given are bindings of its `target`'s.
    PropertyChanges,
    /// What it gives `anchors` are anchors of its `target`'s.
    AnchorChanges,
    /// Its handlers are of its `target`'s signals.
    Connections,
}

/// What a property is, as far as reading what is written in it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Property<T> {
    /// What is written there is a template, not an object: a `delegate`.
    pub is_component: bool,
    /// A path written there is taken from the file it is written in.
    pub is_url: bool,
    /// What is written there is text, also where it reads as a path.
    pub is_text: bool,
    /// What is written there is run when its time comes and worth nothing
    /// until then: the `script` of a ScriptAction or a StateChangeScript.
    pub is_script: bool,
    /// An object written there is a list of one.
    pub is_list: bool,
    pub is_readonly: bool,
    /// The type of the value, for what is under it: `font.bold`.
    pub value: Option<T>,
}

impl<T> Property<T> {
    /// A property a QML file declares, by the type it is declared with.
    pub fn declared(type_name: &str, is_list: bool, is_readonly: bool) -> Self {
        Property {
            is_component: type_name == "Component" && !is_list,
            is_url: type_name == "url" && !is_list,
            is_text: type_name == "string" && !is_list,
            is_script: false,
            is_list,
            is_readonly,
            value: None,
        }
    }
}

/// The types Qt describes for itself, from the table [`qt_types`] was built
/// with. Without a table it knows no module.
#[derive(Debug, Clone, Copy, Default)]
pub struct QtTypeOracle;

/// One of Qt's types. Two are equal when they are the same entry of the
/// table.
#[derive(Clone, Copy)]
pub struct QtType(pub &'static qt_types::Type);

impl PartialEq for QtType {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for QtType {}

impl fmt::Debug for QtType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QtType({})", self.0.class)
    }
}

impl TypeOracle for QtTypeOracle {
    type Type = QtType;

    fn has_module(&self, uri: &str) -> bool {
        qt_types::module(uri).is_some()
    }

    fn module_type(&self, uri: &str, name: &str) -> Option<QtType> {
        qt_types::module(uri)?.type_named(name).map(QtType)
    }

    fn name(&self, ty: QtType) -> &str {
        if ty.0.name.is_empty() {
            ty.0.class
        } else {
            ty.0.name
        }
    }

    fn special(&self, ty: QtType) -> Option<Special> {
        // A style's QML file is its own class: what it is underneath is
        // what it inherits.
        let mut ty = Some(ty.0);
        while let Some(found) = ty {
            match found.class {
                "QQmlComponent" => return Some(Special::Component),
                "QQuickPropertyChanges" => return Some(Special::PropertyChanges),
                "QQuickAnchorChanges" => return Some(Special::AnchorChanges),
                "QQmlConnections" => return Some(Special::Connections),
                _ => ty = found.prototype(),
            }
        }
        None
    }

    fn is_singleton(&self, ty: QtType) -> bool {
        ty.0.is_singleton
    }

    fn property(&self, ty: QtType, name: &str) -> Option<Property<QtType>> {
        ty.0.property(name).map(qt_property)
    }

    fn signal(&self, ty: QtType, name: &str) -> Option<Vec<String>> {
        let signal = ty.0.signal(name)?;
        Some(
            signal
                .parameters
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        )
    }

    fn has_method(&self, ty: QtType, name: &str) -> bool {
        ty.0.has_method(name)
    }

    fn default_property(&self, ty: QtType) -> Option<&str> {
        ty.0.default_property()
    }

    fn attached(&self, ty: QtType) -> Option<QtType> {
        ty.0.attached().map(QtType)
    }

    fn has_enum_key(&self, ty: QtType, scope: Option<&str>, key: &str) -> bool {
        ty.0.enum_value(key).is_some_and(|value| {
            let enumeration = value.enumeration;
            scope.is_none_or(|scope| scope == enumeration.name || Some(scope) == enumeration.alias)
        })
    }

    fn has_enum(&self, ty: QtType, name: &str) -> bool {
        let mut ty = Some(ty.0);
        while let Some(found) = ty {
            if found
                .enums
                .iter()
                .any(|enumeration| enumeration.name == name || enumeration.alias == Some(name))
            {
                return true;
            }
            ty = found.prototype();
        }
        false
    }

    fn required(&self, ty: QtType) -> Vec<String> {
        let mut required: Vec<String> = Vec::new();
        let mut ty = Some(ty.0);
        while let Some(found) = ty {
            for property in found
                .properties
                .iter()
                .filter(|property| property.is_required)
            {
                if !required.iter().any(|name| name == property.name) {
                    required.push(property.name.to_string());
                }
            }
            ty = found.prototype();
        }
        required
    }

    fn is_member_name(&self, name: &str) -> bool {
        qt_types::is_member_name(name)
    }
}

fn qt_property(property: &'static qt_types::Property) -> Property<QtType> {
    Property {
        is_component: property.is_component,
        is_url: property.type_name == "QUrl" && !property.is_list,
        is_text: property.type_name == "QString" && !property.is_list,
        is_script: property.type_name == "QQmlScriptString" && property.name == "script",
        is_list: property.is_list,
        is_readonly: property.is_readonly,
        value: property.value_type().map(QtType),
    }
}
