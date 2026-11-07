//! Tests of a program's own native types and the natives that take the API's names.

use super::*;

/// A program's own module `Studio.Kit`: `Tone` with a `color` of its own,
/// `Swatch` built on Qt's `Rectangle`, and the singleton `Look` holding a
/// `Palette` of `Tone`s and a `Swatch`. And `Studio.Print` and
/// `Studio.Press`, whose types are named as Qt's are.
struct Kit;

/// A type's name, whether it is a singleton, the toolkit type it is built
/// on, and its properties with the type of the object each holds.
type KitType = (
    &'static str,
    bool,
    Option<&'static str>,
    &'static [(&'static str, Option<&'static str>)],
);

const KIT: &[KitType] = &[
    ("Tone", false, None, &[("color", None), ("label", None)]),
    ("Palette", false, None, &[("accent", Some("Tone"))]),
    ("Swatch", false, Some("Rectangle"), &[("label", None)]),
    (
        "Look",
        true,
        None,
        &[("palette", Some("Palette")), ("card", Some("Swatch"))],
    ),
];

/// A `Text` with no `color`, a `MouseArea` and a `Button` with one, built
/// on nothing of the toolkit's.
const PRINT: &[KitType] = &[
    ("Text", false, None, &[("text", None)]),
    ("MouseArea", false, None, &[("pressed", None)]),
    ("Button", false, None, &[("text", None), ("color", None)]),
];

/// A `Text` with a `color`.
const PRESS: &[KitType] = &[("Text", false, None, &[("text", None), ("color", None)])];

const MODULES: &[(&str, &[KitType])] = &[
    ("Studio.Kit", KIT),
    ("Studio.Print", PRINT),
    ("Studio.Press", PRESS),
];

/// A type of `MODULES` is its module's place times this, plus its own.
const MODULE_STRIDE: u32 = 100;

fn kit_type(ty: Native) -> &'static KitType {
    let (_, types) = MODULES[(ty.0 / MODULE_STRIDE) as usize];
    &types[(ty.0 % MODULE_STRIDE) as usize]
}

impl Natives for Kit {
    fn has_module(&self, uri: &str) -> bool {
        MODULES.iter().any(|(m, _)| *m == uri)
    }

    fn module_type(&self, uri: &str, name: &str) -> Option<Native> {
        let module = MODULES.iter().position(|(m, _)| *m == uri)?;
        let at = MODULES[module].1.iter().position(|(n, ..)| *n == name)?;
        Some(Native(module as u32 * MODULE_STRIDE + at as u32))
    }

    fn name(&self, ty: Native) -> String {
        kit_type(ty).0.to_string()
    }

    fn is_singleton(&self, ty: Native) -> bool {
        kit_type(ty).1
    }

    fn has_member(&self, ty: Native, name: &str) -> bool {
        let name = name.strip_suffix("Changed").unwrap_or(name);
        self.property(ty, name).is_some()
    }

    fn property(&self, ty: Native, name: &str) -> Option<Holds> {
        let (_, held) = kit_type(ty).3.iter().find(|(n, _)| *n == name)?;
        Some(match held {
            Some(held) => Holds::Native(self.module_type("Studio.Kit", held)?),
            None => Holds::Value,
        })
    }

    fn toolkit_base(&self, ty: Native) -> Option<(String, String)> {
        let base = kit_type(ty).2?;
        Some(("QtQuick".to_string(), base.to_string()))
    }
}

fn with_kit(source: &str) -> Outcome {
    lower(
        source,
        &Styled {
            locals: &no_locals,
            style: CONTROLS_STYLE,
            natives: Some(&Kit),
        },
    )
}

#[test]
fn a_module_of_the_programs_own_types_is_lowered_with_its_import_as_written() {
    let Outcome::Lowered(done) = with_kit(
        "import QtQuick\nimport Studio.Kit 1.0\nRectangle { color: \"red\"; Swatch { label: \"Sea\"; radius: 4 } }\n",
    ) else {
        panic!("not lowered");
    };
    assert_eq!(
        done.text,
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport Studio.Kit 1.0\nRectangle { fill: \"red\"; Swatch { label: \"Sea\"; radius: 4 } }\n"
    );
    // Without the description it is not known what the module's types are.
    let Outcome::Kept(kept) = lower(
        "import QtQuick\nimport Studio.Kit 1.0\nItem { }\n",
        &no_locals,
    ) else {
        panic!("lowered");
    };
    assert!(kept.reason.contains("not lowered yet"), "{}", kept.reason);
}

#[test]
fn a_singletons_members_are_typed_by_what_they_hold() {
    let Outcome::Lowered(done) = with_kit(
        "import QtQuick\nimport Studio.Kit\nRectangle { color: Look.palette.accent.color\n  Tone { id: tone; onColorChanged: {} }\n  property color seen: tone.color }\n",
    ) else {
        panic!("not lowered");
    };
    assert!(
        done.text.contains("fill: Look.palette.accent.color"),
        "{}",
        done.text
    );
    assert!(done.text.contains("onColorChanged"), "{}", done.text);
    assert!(done.text.contains("seen: tone.color"), "{}", done.text);
}

#[test]
fn a_name_ljos_renames_on_a_native_types_toolkit_base_keeps_the_file() {
    for (body, at) in [
        ("Item { Swatch { color: \"red\" } }", "color: \"red\""),
        ("Item { Swatch { onColorChanged: {} } }", "onColorChanged"),
        ("Text { color: Look.card.color }", "color }"),
        (
            "Item { Swatch { id: s }\n  property color c: s.color }",
            "color }",
        ),
    ] {
        let source = format!("import QtQuick\nimport Studio.Kit\n{body}\n");
        let Outcome::Kept(kept) = with_kit(&source) else {
            panic!("lowered: {body}");
        };
        assert!(
            kept.reason
                .contains("is Rectangle's, which Ljos.Ui calls `fill"),
            "{body}: {}",
            kept.reason
        );
        assert_eq!(kept.at, source.find(at).unwrap(), "{body}");
    }
}

#[test]
fn a_type_the_description_lacks_keeps_the_file() {
    let Outcome::Kept(kept) = with_kit("import QtQuick\nimport Studio.Kit\nItem { Gauge { } }\n")
    else {
        panic!("lowered");
    };
    assert_eq!(
        kept.reason,
        "`Gauge` is neither a file beside it nor a type of `Studio.Kit` as the program describes it"
    );
    // A file beside it is no type of the module.
    let locals = |name: &str| (name == "Gauge").then(|| root_of("Item", &[]));
    let Outcome::Lowered(_) = lower(
        "import QtQuick\nimport Studio.Kit\nItem { Gauge { } }\n",
        &Styled {
            locals: &locals,
            style: CONTROLS_STYLE,
            natives: Some(&Kit),
        },
    ) else {
        panic!("not lowered");
    };
}

/// What `source` is lowered to.
fn lowered_with_kit(source: &str) -> String {
    match with_kit(source) {
        Outcome::Lowered(done) => done.text,
        other => panic!("not lowered: {other:?}\n{source}"),
    }
}

#[test]
fn a_native_type_imported_after_qtquick_shadows_qts_type_of_its_name() {
    // The program's `Text` has no `color`: the root's is read, which is
    // `fill`. Qt's has one, which stays.
    let body = "Rectangle { color: \"red\"; Text { text: color } }\n";
    let after = lowered_with_kit(&format!("import QtQuick\nimport Studio.Print\n{body}"));
    assert!(after.contains("Text { text: fill }"), "{after}");
    let before = lowered_with_kit(&format!("import Studio.Print\nimport QtQuick\n{body}"));
    assert!(before.contains("Text { text: color }"), "{before}");
    // A type Ljós renames is renamed only where it is Qt's.
    let body = "Item { MouseArea { } }\n";
    let after = lowered_with_kit(&format!("import QtQuick\nimport Studio.Print\n{body}"));
    assert!(after.contains("MouseArea { }"), "{after}");
    let before = lowered_with_kit(&format!("import Studio.Print\nimport QtQuick\n{body}"));
    assert!(before.contains("PointerArea { }"), "{before}");
}

#[test]
fn a_native_type_imported_after_the_controls_shadows_their_type_of_its_name() {
    // The program's `Button` has a `color`, which stays; Qt's has none, so
    // the root's is read, which is `fill`.
    let body = "Rectangle { color: \"red\"; Button { text: color } }\n";
    let after = lowered_with_kit(&format!(
        "import QtQuick\nimport QtQuick.Controls.Basic\nimport Studio.Print\n{body}"
    ));
    assert!(after.contains("Button { text: color }"), "{after}");
    let before = lowered_with_kit(&format!(
        "import QtQuick\nimport Studio.Print\nimport QtQuick.Controls.Basic\n{body}"
    ));
    assert!(before.contains("Button { text: fill }"), "{before}");
}

#[test]
fn of_two_native_modules_with_a_types_name_the_last_imported_has_it() {
    let body = "Rectangle { color: \"red\"; Text { text: color } }\n";
    // `Studio.Press`'s `Text` has a `color`; `Studio.Print`'s has none.
    let press_last = lowered_with_kit(&format!(
        "import QtQuick\nimport Studio.Print\nimport Studio.Press\n{body}"
    ));
    assert!(press_last.contains("Text { text: color }"), "{press_last}");
    let print_last = lowered_with_kit(&format!(
        "import QtQuick\nimport Studio.Press\nimport Studio.Print\n{body}"
    ));
    assert!(print_last.contains("Text { text: fill }"), "{print_last}");
}

#[test]
fn a_file_beside_it_imported_after_a_native_module_shadows_the_native_type() {
    // The directory's `Text` is a Rectangle, whose `color` is `fill`;
    // `Studio.Press`'s keeps its own.
    let locals = |from: Imported<'_>, name: &str| {
        (from == Imported::Dir("parts") && name == "Text").then(|| root_of("Rectangle", &[]))
    };
    struct Parts<'a>(&'a dyn Fn(Imported<'_>, &str) -> Option<Root>);
    impl Locals for Parts<'_> {
        fn root(&self, _: &str) -> Option<Root> {
            None
        }
        fn imported(&self, from: Imported<'_>, name: &str) -> Option<Root> {
            (self.0)(from, name)
        }
        fn controls_style(&self) -> Option<&str> {
            Some(CONTROLS_STYLE)
        }
        fn natives(&self) -> Option<&dyn Natives> {
            Some(&Kit)
        }
    }
    let body = "Item { Text { text: color } }\n";
    let Outcome::Lowered(done) = lower(
        &format!("import QtQuick\nimport Studio.Press\nimport \"parts\"\n{body}"),
        &Parts(&locals),
    ) else {
        panic!("not lowered");
    };
    assert!(done.text.contains("Text { text: fill }"), "{}", done.text);
}
