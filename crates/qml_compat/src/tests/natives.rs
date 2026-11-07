//! Tests of a program's own native types and the natives that take the API's names.

use super::*;

/// A program's own module `Studio.Kit`: `Tone` with a `color` of its own,
/// `Swatch` built on Qt's `Rectangle`, and the singleton `Look` holding a
/// `Palette` of `Tone`s and a `Swatch`.
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

impl Natives for Kit {
    fn has_module(&self, uri: &str) -> bool {
        uri == "Studio.Kit"
    }

    fn module_type(&self, uri: &str, name: &str) -> Option<Native> {
        let at = KIT.iter().position(|(n, ..)| *n == name)?;
        (uri == "Studio.Kit").then_some(Native(at as u32))
    }

    fn name(&self, ty: Native) -> String {
        KIT[ty.0 as usize].0.to_string()
    }

    fn is_singleton(&self, ty: Native) -> bool {
        KIT[ty.0 as usize].1
    }

    fn has_member(&self, ty: Native, name: &str) -> bool {
        let name = name.strip_suffix("Changed").unwrap_or(name);
        self.property(ty, name).is_some()
    }

    fn property(&self, ty: Native, name: &str) -> Option<Holds> {
        let (_, held) = KIT[ty.0 as usize].3.iter().find(|(n, _)| *n == name)?;
        Some(match held {
            Some(held) => Holds::Native(self.module_type("Studio.Kit", held)?),
            None => Holds::Value,
        })
    }

    fn toolkit_base(&self, ty: Native) -> Option<(String, String)> {
        let base = KIT[ty.0 as usize].2?;
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
