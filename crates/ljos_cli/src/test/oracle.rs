//! The types a test file is compiled against: Qt's, and the `QtTest`
//! module's.
//!
//! `TestCase` and `SignalSpy` are native types of `ljos_test`, so they are
//! in no table of Qt's types. The module describes them itself
//! ([`ljos_test::qttest::describe`]); this puts that description beside
//! Qt's table, and answers for what a test type inherits (`Item`) from the
//! table.

use std::rc::Rc;

use ljos_engine::TypeEntry;
use qml_front::{Property, QtType, QtTypeOracle, Special, TypeOracle};

/// The modules a test type's base is looked for in, in order.
const BASE_MODULES: [&str; 2] = ["QtQuick", "QtQml"];

#[derive(Clone, Debug)]
pub struct TestOracle {
    qt: QtTypeOracle,
    types: Rc<Vec<TypeEntry>>,
}

/// One of Qt's types, or one of the test module's by its place in the
/// description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Qt(QtType),
    Test(usize),
}

impl TestOracle {
    pub fn new() -> Self {
        TestOracle {
            qt: QtTypeOracle,
            types: Rc::new(ljos_test::qttest::describe()),
        }
    }

    fn test_type(&self, name: &str) -> Option<Type> {
        self.types
            .iter()
            .position(|t| t.name == name)
            .map(Type::Test)
    }

    /// What a test type inherits.
    fn base(&self, index: usize) -> Option<Type> {
        let base = self.types[index].base.as_deref()?;
        self.test_type(base).or_else(|| {
            BASE_MODULES
                .iter()
                .find_map(|uri| self.qt.module_type(uri, base))
                .map(Type::Qt)
        })
    }

    /// Answers for a type from its own description if `own` finds it
    /// there, and from what it inherits if not.
    fn find<R>(
        &self,
        ty: Type,
        own: impl Fn(&TypeEntry) -> Option<R>,
        qt: impl Fn(QtType) -> Option<R>,
    ) -> Option<R> {
        let mut ty = Some(ty);
        // A description that names itself as its base would never end.
        for _ in 0..=self.types.len() {
            match ty? {
                Type::Qt(q) => return qt(q),
                Type::Test(i) => match own(&self.types[i]) {
                    Some(found) => return Some(found),
                    None => ty = self.base(i),
                },
            }
        }
        None
    }

    /// The Qt type under a type, for what the description does not say
    /// (enums, required properties).
    fn qt_under(&self, ty: Type) -> Option<QtType> {
        self.find(ty, |_| None, Some)
    }
}

fn wrap(property: Property<QtType>) -> Property<Type> {
    Property {
        is_component: property.is_component,
        is_url: property.is_url,
        is_text: property.is_text,
        is_script: property.is_script,
        is_list: property.is_list,
        is_readonly: property.is_readonly,
        value: property.value.map(Type::Qt),
    }
}

impl TypeOracle for TestOracle {
    type Type = Type;

    fn has_module(&self, uri: &str) -> bool {
        uri == ljos_test::qttest::URI || self.qt.has_module(uri)
    }

    fn module_type(&self, uri: &str, name: &str) -> Option<Type> {
        if uri == ljos_test::qttest::URI {
            self.test_type(name)
        } else {
            self.qt.module_type(uri, name).map(Type::Qt)
        }
    }

    fn name(&self, ty: Type) -> &str {
        match ty {
            Type::Qt(q) => self.qt.name(q),
            Type::Test(i) => &self.types[i].name,
        }
    }

    fn special(&self, ty: Type) -> Option<Special> {
        self.qt_under(ty).and_then(|q| self.qt.special(q))
    }

    fn is_singleton(&self, ty: Type) -> bool {
        match ty {
            Type::Qt(q) => self.qt.is_singleton(q),
            Type::Test(_) => false,
        }
    }

    fn property(&self, ty: Type, name: &str) -> Option<Property<Type>> {
        self.find(
            ty,
            |entry| {
                let p = entry.properties.iter().find(|p| p.name == name)?;
                let mut property = Property::declared(&p.ty, false, p.readonly);
                // The description names types as the runtime does.
                property.is_text |= p.ty == "QString";
                property.is_url |= p.ty == "QUrl";
                Some(property)
            },
            |q| self.qt.property(q, name).map(wrap),
        )
    }

    fn signal(&self, ty: Type, name: &str) -> Option<Vec<String>> {
        self.find(
            ty,
            |entry| {
                if let Some(signal) = entry.signals.iter().find(|s| s.name == name) {
                    return Some(signal.parameters.iter().map(|p| p.name.clone()).collect());
                }
                entry
                    .properties
                    .iter()
                    .any(|p| p.notify.as_deref() == Some(name))
                    .then(Vec::new)
            },
            |q| self.qt.signal(q, name),
        )
    }

    fn has_method(&self, ty: Type, name: &str) -> bool {
        self.find(
            ty,
            |entry| entry.methods.iter().any(|m| m.name == name).then_some(()),
            |q| self.qt.has_method(q, name).then_some(()),
        )
        .is_some()
    }

    fn default_property(&self, ty: Type) -> Option<&str> {
        let mut ty = Some(ty);
        for _ in 0..=self.types.len() {
            match ty? {
                Type::Qt(q) => return self.qt.default_property(q),
                Type::Test(i) => match self.types[i].default_property.as_deref() {
                    Some(name) => return Some(name),
                    None => ty = self.base(i),
                },
            }
        }
        None
    }

    fn attached(&self, ty: Type) -> Option<Type> {
        match ty {
            Type::Qt(q) => self.qt.attached(q).map(Type::Qt),
            Type::Test(_) => None,
        }
    }

    fn has_enum_key(&self, ty: Type, scope: Option<&str>, key: &str) -> bool {
        self.qt_under(ty)
            .is_some_and(|q| self.qt.has_enum_key(q, scope, key))
    }

    fn has_enum(&self, ty: Type, name: &str) -> bool {
        self.qt_under(ty).is_some_and(|q| self.qt.has_enum(q, name))
    }

    fn enum_is_flags(&self, ty: Type, name: &str) -> bool {
        self.qt_under(ty)
            .is_none_or(|q| self.qt.enum_is_flags(q, name))
    }

    fn required(&self, ty: Type) -> Vec<String> {
        self.qt_under(ty)
            .map(|q| self.qt.required(q))
            .unwrap_or_default()
    }

    fn is_member_name(&self, name: &str) -> bool {
        self.qt.is_member_name(name)
            || self.types.iter().any(|t| {
                t.properties.iter().any(|p| p.name == name)
                    || t.methods.iter().any(|m| m.name == name)
                    || t.signals.iter().any(|s| s.name == name)
            })
    }

    fn property_type(&self, ty: Type, name: &str) -> Option<String> {
        self.find(
            ty,
            |entry| {
                entry
                    .properties
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.ty.clone())
            },
            |q| self.qt.property_type(q, name),
        )
    }

    fn enum_value(&self, ty: Type, scope: Option<&str>, key: &str) -> Option<i64> {
        self.qt_under(ty)
            .and_then(|q| self.qt.enum_value(q, scope, key))
    }

    fn module(&self, ty: Type) -> Option<String> {
        match ty {
            Type::Qt(q) => self.qt.module(q),
            Type::Test(_) => Some(ljos_test::qttest::URI.to_owned()),
        }
    }

    fn members_known(&self, ty: Type) -> bool {
        match ty {
            Type::Qt(q) => self.qt.members_known(q),
            Type::Test(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_case_has_its_own_members_and_an_items() {
        let oracle = TestOracle::new();
        let case = oracle.module_type("QtTest", "TestCase").expect("TestCase");
        assert_eq!(oracle.name(case), "TestCase");
        assert!(oracle.has_method(case, "compare"));
        assert!(oracle.property(case, "when").is_some());
        assert!(oracle.property(case, "width").is_some(), "an Item's");
        assert!(oracle.signal(case, "whenChanged").is_some());
        assert!(oracle.signal(case, "widthChanged").is_some());
        assert_eq!(oracle.default_property(case), Some("data"));
        assert!(oracle.property(case, "nothing").is_none());
        assert!(!oracle.has_method(case, "nothing"));
    }

    #[test]
    fn qt_types_are_qts_and_other_modules_have_no_test_types() {
        let oracle = TestOracle::new();
        assert!(oracle.has_module("QtTest") && oracle.has_module("QtQuick"));
        assert!(oracle.module_type("QtQuick", "TestCase").is_none());
        let item = oracle.module_type("QtQuick", "Item").expect("Item");
        assert!(matches!(item, Type::Qt(_)));
        assert!(oracle.property(item, "width").is_some());
        assert!(!oracle.has_method(item, "compare"));
    }
}
