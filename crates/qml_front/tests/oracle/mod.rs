//! A small, hand-written runtime for the tests: enough types to read the
//! files they write, and no dependency on a local Qt.

use qml_front::{Property, Special, TypeOracle};

#[derive(Default)]
struct Ty {
    name: &'static str,
    module: &'static str,
    prototype: Option<usize>,
    special: Option<Special>,
    singleton: bool,
    /// name, value type, is_component
    properties: Vec<(&'static str, Option<&'static str>, bool)>,
    signals: Vec<(&'static str, Vec<&'static str>)>,
    methods: Vec<&'static str>,
    default: Option<&'static str>,
    attached: Option<&'static str>,
    enums: Vec<(&'static str, Vec<&'static str>)>,
    required: Vec<&'static str>,
    /// The runtime does not list all of its members.
    loose: bool,
}

pub struct Runtime {
    types: Vec<Ty>,
}

impl Runtime {
    pub fn new() -> Self {
        let p = |name, value: Option<&'static str>| (name, value, false);
        let types = vec![
            Ty {
                name: "QtObject",
                module: "QtQml",
                ..Ty::default()
            },
            Ty {
                name: "Component",
                module: "QtQml",
                special: Some(Special::Component),
                signals: vec![("completed", vec![])],
                ..Ty::default()
            },
            Ty {
                name: "Timer",
                module: "QtQml",
                properties: vec![p("interval", None), p("running", None)],
                signals: vec![("triggered", vec![])],
                ..Ty::default()
            },
            Ty {
                name: "Item",
                module: "QtQuick",
                prototype: Some(0),
                properties: vec![
                    p("width", None),
                    p("height", None),
                    p("x", None),
                    p("parent", Some("Item")),
                    p("visible", None),
                    p("data", None),
                    p("anchors", Some("Anchors")),
                    p("state", None),
                    p("states", None),
                ],
                signals: vec![("widthChanged", vec![]), ("visibleChanged", vec![])],
                methods: vec!["forceActiveFocus"],
                default: Some("data"),
                enums: vec![("TransformOrigin", vec!["TopLeft", "Center"])],
                ..Ty::default()
            },
            Ty {
                name: "Rectangle",
                module: "QtQuick",
                prototype: Some(3),
                properties: vec![
                    p("color", None),
                    p("border", Some("Border")),
                    p("radius", None),
                ],
                ..Ty::default()
            },
            Ty {
                name: "Text",
                module: "QtQuick",
                prototype: Some(3),
                properties: vec![p("text", None), p("font", None)],
                signals: vec![("textChanged", vec!["text"])],
                ..Ty::default()
            },
            Ty {
                name: "MouseArea",
                module: "QtQuick",
                prototype: Some(3),
                properties: vec![p("pressed", None)],
                signals: vec![("clicked", vec!["mouse"]), ("pressed", vec!["mouse"])],
                ..Ty::default()
            },
            Ty {
                name: "Repeater",
                module: "QtQuick",
                prototype: Some(3),
                properties: vec![p("model", None), ("delegate", None, true), p("count", None)],
                default: Some("delegate"),
                ..Ty::default()
            },
            Ty {
                name: "ListView",
                module: "QtQuick",
                prototype: Some(3),
                properties: vec![
                    p("model", None),
                    ("delegate", None, true),
                    p("currentIndex", None),
                ],
                attached: Some("ListViewAttached"),
                enums: vec![("Orientation", vec!["Horizontal", "Vertical"])],
                ..Ty::default()
            },
            Ty {
                name: "PropertyChanges",
                module: "QtQuick",
                special: Some(Special::PropertyChanges),
                properties: vec![p("target", Some("Item")), p("explicit", None)],
                ..Ty::default()
            },
            Ty {
                name: "State",
                module: "QtQuick",
                properties: vec![p("name", None)],
                ..Ty::default()
            },
            Ty {
                name: "Connections",
                module: "QtQml",
                special: Some(Special::Connections),
                properties: vec![p("target", None)],
                ..Ty::default()
            },
            Ty {
                name: "Border",
                properties: vec![p("width", None), p("color", None)],
                ..Ty::default()
            },
            Ty {
                name: "Anchors",
                properties: vec![p("fill", Some("Item")), p("bottom", None)],
                ..Ty::default()
            },
            Ty {
                name: "ListViewAttached",
                properties: vec![p("isCurrentItem", None), p("view", None)],
                ..Ty::default()
            },
            Ty {
                name: "Loose",
                module: "QtQuick",
                prototype: Some(3),
                properties: vec![p("known", None)],
                loose: true,
                ..Ty::default()
            },
            Ty {
                name: "Theme",
                module: "Styles",
                singleton: true,
                properties: vec![p("accent", None)],
                ..Ty::default()
            },
            Ty {
                name: "Cell",
                module: "Tables",
                prototype: Some(3),
                properties: vec![p("row", None), p("column", None)],
                required: vec!["row", "column"],
                ..Ty::default()
            },
        ];
        Runtime { types }
    }

    fn named(&self, name: &str) -> Option<usize> {
        self.types.iter().position(|ty| ty.name == name)
    }

    fn chain(&self, ty: usize) -> impl Iterator<Item = &Ty> {
        std::iter::successors(Some(&self.types[ty]), |ty| {
            ty.prototype.map(|index| &self.types[index])
        })
    }
}

impl TypeOracle for Runtime {
    type Type = usize;

    fn has_module(&self, uri: &str) -> bool {
        self.types.iter().any(|ty| ty.module == uri)
    }

    fn module_type(&self, uri: &str, name: &str) -> Option<usize> {
        // QtQuick comes with QtQml, as Qt's does.
        let modules: &[&str] = match uri {
            "QtQuick" => &["QtQuick", "QtQml"],
            uri => &[uri][..],
        };
        self.types
            .iter()
            .position(|ty| ty.name == name && !ty.module.is_empty() && modules.contains(&ty.module))
    }

    fn name(&self, ty: usize) -> &str {
        self.types[ty].name
    }

    fn special(&self, ty: usize) -> Option<Special> {
        self.chain(ty).find_map(|ty| ty.special)
    }

    fn is_singleton(&self, ty: usize) -> bool {
        self.types[ty].singleton
    }

    fn property(&self, ty: usize, name: &str) -> Option<Property<usize>> {
        let (_, value, is_component) = self
            .chain(ty)
            .find_map(|ty| ty.properties.iter().find(|property| property.0 == name))?;
        Some(Property {
            is_component: *is_component,
            is_url: false,
            is_text: name == "text",
            is_script: false,
            is_list: false,
            is_readonly: false,
            value: value.and_then(|value| self.named(value)),
        })
    }

    fn signal(&self, ty: usize, name: &str) -> Option<Vec<String>> {
        let (_, parameters) = self
            .chain(ty)
            .find_map(|ty| ty.signals.iter().find(|signal| signal.0 == name))?;
        Some(parameters.iter().map(|name| name.to_string()).collect())
    }

    fn has_method(&self, ty: usize, name: &str) -> bool {
        self.chain(ty).any(|ty| ty.methods.contains(&name))
    }

    fn default_property(&self, ty: usize) -> Option<&str> {
        self.chain(ty).find_map(|ty| ty.default)
    }

    fn attached(&self, ty: usize) -> Option<usize> {
        self.chain(ty)
            .find_map(|ty| ty.attached)
            .and_then(|name| self.named(name))
    }

    fn has_enum_key(&self, ty: usize, scope: Option<&str>, key: &str) -> bool {
        self.chain(ty).any(|ty| {
            ty.enums
                .iter()
                .any(|(name, keys)| scope.is_none_or(|scope| scope == *name) && keys.contains(&key))
        })
    }

    fn has_enum(&self, ty: usize, name: &str) -> bool {
        self.chain(ty)
            .any(|ty| ty.enums.iter().any(|(known, _)| *known == name))
    }

    fn required(&self, ty: usize) -> Vec<String> {
        self.chain(ty)
            .flat_map(|ty| ty.required.iter().map(|name| name.to_string()))
            .collect()
    }

    fn is_member_name(&self, name: &str) -> bool {
        self.types
            .iter()
            .any(|ty| ty.properties.iter().any(|property| property.0 == name))
    }

    fn property_type(&self, ty: usize, name: &str) -> Option<String> {
        self.chain(ty)
            .find_map(|ty| ty.properties.iter().find(|property| property.0 == name))
            .map(|(_, value, _)| value.unwrap_or("var").to_string())
    }

    /// The number of a key is its place in the enum.
    fn enum_value(&self, ty: usize, scope: Option<&str>, key: &str) -> Option<i64> {
        self.chain(ty).find_map(|ty| {
            ty.enums
                .iter()
                .filter(|(name, _)| scope.is_none_or(|scope| scope == *name))
                .find_map(|(_, keys)| keys.iter().position(|known| *known == key))
                .map(|place| place as i64)
        })
    }

    fn module(&self, ty: usize) -> Option<String> {
        let module = self.types[ty].module;
        (!module.is_empty()).then(|| module.to_string())
    }

    fn members_known(&self, ty: usize) -> bool {
        !self.chain(ty).any(|ty| ty.loose)
    }
}
