//! What the types are that no QML file of the project declares.
//!
//! `Item`, `Timer` and `ListView` are not QML files: they are whatever the
//! runtime provides. Which members they have is the runtime's to say, so the
//! front end asks a [`TypeOracle`]. [`QtTypeOracle`] answers from the table
//! [`qt_types`] was built with: Qt's description of its own types where the
//! build has one, which is what a file written for Qt expects, and Ljós's
//! description of the types it implements where it has not.

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

    /// Whether a property that holds the enum `name` of the type holds
    /// several keys at once (`Qt.Alignment`), so that a string written for
    /// it may name more than one. An oracle that cannot tell says it may.
    fn enum_is_flags(&self, _ty: Self::Type, _name: &str) -> bool {
        true
    }

    /// The properties whoever makes an object of the type has to set.
    fn required(&self, ty: Self::Type) -> Vec<String>;

    /// Whether some type has a member called `name`: a name that is most
    /// likely that member wherever it is written.
    fn is_member_name(&self, name: &str) -> bool;

    /// The type of a property's value as the runtime names it: `color`,
    /// `QColor`, `int`. Whether it is a list or read-only is in the
    /// [`Property`]. None when the oracle does not say; a back end then lets
    /// the runtime decide (a colour stays a string).
    fn property_type(&self, _ty: Self::Type, _name: &str) -> Option<String> {
        None
    }

    /// The number of `Type.key` (`Type.scope.key`). None when the key has no
    /// number the oracle knows, which is not the same as no key: that is
    /// [`has_enum_key`](Self::has_enum_key).
    fn enum_value(&self, _ty: Self::Type, _scope: Option<&str>, _key: &str) -> Option<i64> {
        None
    }

    /// The module that has the type, where it differs from the one a file
    /// imports it by (`Timer` is `QtQml`'s, imported with `QtQuick`).
    fn module(&self, _ty: Self::Type) -> Option<String> {
        None
    }

    /// Whether the oracle lists every member of the type. A name it does not
    /// have is then not a member; where this is false (a type the runtime
    /// writes in QML and describes only in part, a type that reads its body
    /// itself) it may be.
    fn members_known(&self, _ty: Self::Type) -> bool {
        true
    }

    /// Whether a name written as a member of the type that it does not
    /// have is an error where it is written, not something left for the
    /// running program to find. The oracle says so of a type it describes
    /// in full, as Qt's table does every type whose members it lists, and
    /// an application's native type, whose members are all registered. Qt
    /// fails to make an object a file binds such a name on.
    fn members_checked(&self, _ty: Self::Type) -> bool {
        false
    }

    /// Whether a script that reads `Type.member` or `Singleton.member` of
    /// what the type does not have is an error where it is written. Qt
    /// reads such a name of its own types as `undefined`, and a file that
    /// asks `typeof Text.Nowhere` loads; an application's native type,
    /// described in full, says the name is wrong.
    fn reads_checked(&self, ty: Self::Type) -> bool {
        self.members_checked(ty)
    }

    /// Whether such a name is a warning where it is written: the oracle
    /// lists every member the runtime has, but not every member QML may
    /// name (Ljós's own description of a Qt type, which may lack what Qt
    /// has).
    fn members_warned(&self, _ty: Self::Type) -> bool {
        false
    }

    /// Whether the type is a part of a module the runtime implements that
    /// the runtime's description of that module does not list: a file of
    /// Qt's that only Qt's table names. Its members are not known, so a
    /// name written bare in one of its bindings is looked up around it, and
    /// the runtime cannot make it. The compiler warns where it is written.
    fn is_unlisted(&self, _ty: Self::Type) -> bool {
        false
    }

    /// Whether the runtime gives `ty` a member called `name` that the
    /// description of `ty` does not list: what a back end adds to one of
    /// Qt's types. Such a name is not reported as missing.
    fn adds_member(&self, _ty: Self::Type, _name: &str) -> bool {
        false
    }

    /// What a message about `name`, a type that no import of the file has,
    /// should add: set when the oracle cannot tell a type nobody has from
    /// one it was not told about.
    fn unknown_type_note(&self, _name: &str) -> Option<&'static str> {
        None
    }

    /// Whether the module `uri` is one the runtime does not have and whose
    /// types stand in as an empty `Item` of `QtQuick`: a name of it that is
    /// nothing else the file imports is such an Item, and what only the
    /// missing type has is left out.
    fn stands_in(&self, _uri: &str) -> bool {
        false
    }

    /// Whether the type `name` of such a module is an Item where it is
    /// written, placed as one; otherwise it keeps nothing of the Item but
    /// its name, its children and what the file declares on it.
    fn stands_in_as_item(&self, _name: &str) -> bool {
        true
    }

    /// Whether the property `name` of the type holds one object, not a
    /// value: what is written for it is an object or `null`, never a
    /// number or a string. An oracle that cannot tell says it does not.
    fn holds_object(&self, _ty: Self::Type, _name: &str) -> bool {
        false
    }

    /// The class Qt's messages name the type by (`QQuickItem` for `Item`),
    /// where the oracle knows it.
    fn class_name(&self, _ty: Self::Type) -> Option<&str> {
        None
    }
}

/// The message for `name`, which is written as a type and is none of
/// anything the file imports.
pub fn unknown_type<O: TypeOracle>(oracle: &O, name: &str) -> String {
    let last = name.rsplit('.').next().unwrap_or(name);
    match oracle.unknown_type_note(last) {
        Some(note) => format!("`{name}` is not a type of anything the file imports ({note})"),
        None => format!("`{name}` is not a type of anything the file imports"),
    }
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
    /// A key of an enum written there is a constant, not a name to look
    /// up: the property holds an enum or an `int`.
    pub takes_key: bool,
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
            takes_key: type_name == "int" && !is_list,
            is_list,
            is_readonly,
            value: None,
        }
    }
}

/// The types of the table [`qt_types`] was built with: Qt's own description
/// of its types, and Ljós's of the modules Qt's does not describe. In a
/// build without Qt's table that is every module Ljós implements, and
/// nothing else: a type Qt has and Ljós has not is then unknown
/// ([`TypeOracle::unknown_type_note`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct QtTypeOracle;

/// One type of the table. Two are equal when they are the same entry of
/// it.
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
        // The class is Qt's in Qt's table and Ljós's in its own.
        use qt_types::class;
        let kinds = [
            (class::COMPONENT, Special::Component),
            (class::PROPERTY_CHANGES, Special::PropertyChanges),
            (class::ANCHOR_CHANGES, Special::AnchorChanges),
            (class::CONNECTIONS, Special::Connections),
        ];
        let mut ty = Some(ty.0);
        while let Some(found) = ty {
            if let Some((_, special)) = kinds.iter().find(|(of, _)| of.contains(&found.class)) {
                return Some(*special);
            }
            ty = found.prototype();
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
        match scope {
            Some(scope) => ty.0.scoped_enum_value(scope, key).is_some(),
            None => ty.0.enum_value(key).is_some(),
        }
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

    fn enum_is_flags(&self, ty: QtType, name: &str) -> bool {
        ty.0.enum_is_flags(name).unwrap_or(false)
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

    fn property_type(&self, ty: QtType, name: &str) -> Option<String> {
        let property = ty.0.property(name)?;
        // A value type of Ljós's own description is named as QML names it
        // (`color`), which is what the runtime calls it too.
        let value = property
            .value_type()
            .filter(|value| value.is_own && value.is_value);
        Some(
            value
                .map_or(property.type_name, |value| value.name)
                .to_string(),
        )
    }

    fn enum_value(&self, ty: QtType, scope: Option<&str>, key: &str) -> Option<i64> {
        match scope {
            Some(scope) => ty.0.scoped_enum_value(scope, key),
            None => ty.0.enum_value(key),
        }
        .and_then(|value| value.number)
    }

    fn module(&self, ty: QtType) -> Option<String> {
        // Qt's table declares `Component`, `QtObject` and the value types
        // in `QML`, a module no file imports: `QtQml` brings them, and is
        // the module a build without the table names them by.
        match ty.0.module {
            "" => None,
            "QML" => Some("QtQml".to_string()),
            module => Some(module.to_string()),
        }
    }

    fn members_known(&self, ty: QtType) -> bool {
        // A style's QML file is not read, so a type written in QML that no
        // native type stands behind has no members here. `ListElement` and
        // the like are read by a parser of their own: anything may be in them.
        let mut ty = Some(ty.0);
        while let Some(found) = ty {
            if (found.qml_file.is_some() && found.prototype().is_none())
                || found.has_custom_parser
                || qt_types::class::LIST_ELEMENT.contains(&found.class)
            {
                return false;
            }
            ty = found.prototype();
        }
        true
    }

    fn members_checked(&self, ty: QtType) -> bool {
        // Ljós's own table describes what the runtime has, which may lack
        // a member Qt has: a name it does not list may still be right.
        !ty.0.is_own && self.members_known(ty)
    }

    fn members_warned(&self, ty: QtType) -> bool {
        ty.0.is_own && self.members_known(ty)
    }

    fn is_unlisted(&self, ty: QtType) -> bool {
        ty.0.is_unlisted
    }

    fn adds_member(&self, ty: QtType, name: &str) -> bool {
        ty.0.adds(name)
    }

    fn reads_checked(&self, _ty: QtType) -> bool {
        false
    }

    fn unknown_type_note(&self, name: &str) -> Option<&'static str> {
        // A name another module of the table has is known for what it is:
        // the file lacks an import, and Qt's table would say no more.
        (!qt_types::has_qt_table() && !qt_types::is_type_name(name))
            .then_some(qt_types::WITHOUT_QT_TABLE)
    }

    fn stands_in(&self, uri: &str) -> bool {
        qt_types::stands_in(uri)
    }

    fn stands_in_as_item(&self, name: &str) -> bool {
        qt_types::stands_in_as_item(name)
    }

    fn holds_object(&self, ty: QtType, name: &str) -> bool {
        ty.0.property(name)
            .is_some_and(|property| property.is_pointer && !property.is_list)
    }

    fn class_name(&self, ty: QtType) -> Option<&str> {
        // Ljós's own table names a class by its module and QML name, and a
        // type that is a QML file is named by a counter Qt makes up.
        (!ty.0.is_own && ty.0.qml_file.is_none()).then_some(ty.0.class)
    }
}

fn qt_property(property: &'static qt_types::Property) -> Property<QtType> {
    use qt_types::class;
    Property {
        is_component: property.is_component,
        is_url: class::URL.contains(&property.type_name) && !property.is_list,
        is_text: class::STRING.contains(&property.type_name) && !property.is_list,
        is_script: property.is_script,
        takes_key: !property.is_list
            && (property.type_name == "int"
                || qt_types::is_enum_name(
                    property
                        .type_name
                        .rsplit("::")
                        .next()
                        .unwrap_or(property.type_name),
                )),
        is_list: property.is_list,
        is_readonly: property.is_readonly,
        value: property.value_type().map(QtType),
    }
}
