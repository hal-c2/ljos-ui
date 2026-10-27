//! A small, hand-written runtime for the tests: enough types to compile the
//! files they write, the same with or without a local Qt.

use qml_front::{Property, Special, TypeOracle};

#[derive(Default)]
struct Ty {
    name: &'static str,
    module: &'static str,
    prototype: Option<&'static str>,
    special: Option<Special>,
    singleton: bool,
    /// name, the runtime's type name, the type of what is under it
    properties: Vec<(&'static str, &'static str, Option<&'static str>)>,
    signals: Vec<(&'static str, Vec<&'static str>)>,
    methods: Vec<&'static str>,
    default: Option<&'static str>,
    attached: Option<&'static str>,
    enums: Vec<(&'static str, Vec<(&'static str, i64)>)>,
}

#[derive(Debug)]
pub struct Runtime {
    types: Vec<Ty>,
}

impl std::fmt::Debug for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

impl Runtime {
    pub fn new() -> Self {
        let p = |name, ty| (name, ty, None);
        let g = |name, ty, value| (name, ty, Some(value));
        let types = vec![
            Ty {
                name: "QtObject",
                module: "QtQml",
                properties: vec![p("objectName", "QString")],
                ..Ty::default()
            },
            Ty {
                name: "Qt",
                module: "QtQml",
                enums: vec![
                    (
                        "AlignmentFlag",
                        vec![("AlignLeft", 1), ("AlignRight", 2), ("AlignHCenter", 4)],
                    ),
                    (
                        "MouseButtons",
                        vec![("LeftButton", 1), ("RightButton", 2), ("MiddleButton", 4)],
                    ),
                ],
                methods: vec!["rgba", "darker"],
                ..Ty::default()
            },
            Ty {
                name: "Component",
                module: "QtQml",
                special: Some(Special::Component),
                attached: Some("ComponentAttached"),
                ..Ty::default()
            },
            Ty {
                name: "ComponentAttached",
                signals: vec![("completed", vec![]), ("destruction", vec![])],
                ..Ty::default()
            },
            Ty {
                name: "Timer",
                module: "QtQml",
                prototype: Some("QtObject"),
                properties: vec![p("interval", "int"), p("running", "bool")],
                signals: vec![("triggered", vec![])],
                ..Ty::default()
            },
            Ty {
                name: "Connections",
                module: "QtQml",
                special: Some(Special::Connections),
                properties: vec![p("target", "QObject*")],
                ..Ty::default()
            },
            Ty {
                name: "State",
                module: "QtQuick",
                prototype: Some("QtObject"),
                properties: vec![p("name", "QString"), p("changes", "QQuickStateOperation*")],
                default: Some("changes"),
                ..Ty::default()
            },
            Ty {
                name: "PropertyChanges",
                module: "QtQuick",
                special: Some(Special::PropertyChanges),
                properties: vec![p("target", "QObject*")],
                ..Ty::default()
            },
            Ty {
                name: "Item",
                module: "QtQuick",
                prototype: Some("QtObject"),
                properties: vec![
                    p("width", "double"),
                    p("height", "double"),
                    p("x", "double"),
                    p("opacity", "double"),
                    g("parent", "QQuickItem*", "Item"),
                    p("visible", "bool"),
                    p("focus", "bool"),
                    p("data", "QObject*"),
                    g("anchors", "QQuickAnchors*", "Anchors"),
                    p("state", "QString"),
                    p("states", "QQuickState*"),
                    p("transformOrigin", "TransformOrigin"),
                ],
                signals: vec![("widthChanged", vec![])],
                methods: vec!["forceActiveFocus"],
                default: Some("data"),
                enums: vec![("TransformOrigin", vec![("TopLeft", 0), ("Center", 4)])],
                ..Ty::default()
            },
            Ty {
                name: "Rectangle",
                module: "QtQuick",
                prototype: Some("Item"),
                properties: vec![
                    p("color", "QColor"),
                    g("border", "QQuickPen*", "Pen"),
                    p("radius", "double"),
                ],
                ..Ty::default()
            },
            Ty {
                name: "Text",
                module: "QtQuick",
                prototype: Some("Item"),
                properties: vec![
                    p("text", "QString"),
                    p("color", "QColor"),
                    g("font", "QFont", "Font"),
                    p("horizontalAlignment", "HAlignment"),
                ],
                signals: vec![("linkActivated", vec!["link"])],
                enums: vec![(
                    "HAlignment",
                    vec![("AlignLeft", 1), ("AlignRight", 2), ("AlignHCenter", 4)],
                )],
                ..Ty::default()
            },
            Ty {
                name: "MouseArea",
                module: "QtQuick",
                prototype: Some("Item"),
                properties: vec![
                    p("pressed", "bool"),
                    p("acceptedButtons", "Qt::MouseButtons"),
                ],
                signals: vec![("clicked", vec!["mouse"]), ("pressed", vec!["mouse"])],
                ..Ty::default()
            },
            Ty {
                name: "ListView",
                module: "QtQuick",
                prototype: Some("Item"),
                properties: vec![
                    p("model", "QVariant"),
                    p("delegate", "QQmlComponent*"),
                    p("currentIndex", "int"),
                ],
                attached: Some("ListViewAttached"),
                ..Ty::default()
            },
            Ty {
                name: "ListViewAttached",
                properties: vec![
                    p("isCurrentItem", "bool"),
                    g("view", "QQuickListView*", "ListView"),
                ],
                ..Ty::default()
            },
            Ty {
                name: "Keys",
                module: "QtQuick",
                attached: Some("KeysAttached"),
                ..Ty::default()
            },
            Ty {
                name: "KeysAttached",
                properties: vec![p("enabled", "bool")],
                signals: vec![("pressed", vec!["event"]), ("released", vec!["event"])],
                ..Ty::default()
            },
            Ty {
                name: "Anchors",
                properties: vec![
                    g("fill", "QQuickItem*", "Item"),
                    p("left", "QQuickAnchorLine"),
                    p("right", "QQuickAnchorLine"),
                    p("margins", "double"),
                ],
                ..Ty::default()
            },
            Ty {
                name: "Pen",
                properties: vec![p("width", "double"), p("color", "QColor")],
                ..Ty::default()
            },
            Ty {
                name: "Font",
                properties: vec![p("pixelSize", "int"), p("bold", "bool")],
                ..Ty::default()
            },
            Ty {
                name: "Theme",
                module: "Styles",
                singleton: true,
                properties: vec![p("accent", "QColor")],
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
            ty.prototype
                .and_then(|name| self.named(name))
                .map(|i| &self.types[i])
        })
    }

    fn find_property(
        &self,
        ty: usize,
        name: &str,
    ) -> Option<&(&'static str, &'static str, Option<&'static str>)> {
        self.chain(ty)
            .find_map(|ty| ty.properties.iter().find(|property| property.0 == name))
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
        let (_, type_name, value) = self.find_property(ty, name)?;
        Some(Property {
            is_component: *type_name == "QQmlComponent*",
            is_url: false,
            is_text: *type_name == "QString",
            is_script: false,
            takes_key: false,
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
        self.enum_value(ty, scope, key).is_some()
    }

    fn enum_is_flags(&self, _ty: usize, name: &str) -> bool {
        // The one enum of this runtime that holds several keys.
        name == "MouseButtons"
    }

    fn has_enum(&self, ty: usize, name: &str) -> bool {
        self.chain(ty)
            .any(|ty| ty.enums.iter().any(|(known, _)| *known == name))
    }

    fn required(&self, _ty: usize) -> Vec<String> {
        Vec::new()
    }

    fn is_member_name(&self, name: &str) -> bool {
        self.types
            .iter()
            .any(|ty| ty.properties.iter().any(|property| property.0 == name))
    }

    fn property_type(&self, ty: usize, name: &str) -> Option<String> {
        self.find_property(ty, name).map(|p| p.1.to_string())
    }

    fn enum_value(&self, ty: usize, scope: Option<&str>, key: &str) -> Option<i64> {
        self.chain(ty).find_map(|ty| {
            ty.enums
                .iter()
                .filter(|(name, _)| scope.is_none_or(|scope| scope == *name))
                .find_map(|(_, keys)| keys.iter().find(|(k, _)| *k == key).map(|(_, v)| *v))
        })
    }

    fn module(&self, ty: usize) -> Option<String> {
        let module = self.types[ty].module;
        (!module.is_empty()).then(|| module.to_string())
    }
}
