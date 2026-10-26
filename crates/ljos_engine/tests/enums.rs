//! Properties that hold a key of an enum of their type: a string names a
//! key, a number is taken as it is, and anything else is refused with the
//! enum's name. What Qt 6.11.2 does (by running qml6) is in
//! `features/language/enum-properties.feature`.

use ljos_engine::{Engine, ObjectId, QmlObject, QmlType, TypeTag, Value, WriteKind};

fn shape_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Corner",
        vec![("TopLeft", 0), ("TopRight", 1), ("BottomLeft", 2)],
    )]
}

fn tile_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![("Mode", vec![("Plain", 0), ("Dotted", 1), ("Striped", 4)])]
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Shape", enums = shape_enums)]
pub struct Shape {
    #[qml(notify, enumeration = "Corner")]
    pub corner: i32,
    pub plain: i32,
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Tile", extends = Shape, enums = tile_enums)]
pub struct Tile {
    #[qml(base)]
    pub shape: Shape,
    /// A key of the enum the base type has.
    #[qml(notify, enumeration = "Corner")]
    pub fold: i32,
    #[qml(notify, enumeration = "Mode")]
    pub mode: i32,
    /// Several keys at once.
    #[qml(notify, flags = "Mode")]
    pub modes: i32,
    /// Enums of the `Qt` namespace, which no type lists.
    #[qml(notify, flags = "Qt.Alignment")]
    pub alignment: i32,
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
}

fn engine() -> Engine {
    let e = Engine::new();
    e.register_module(ljos_engine::module!("Test" 1.0 => [Shape, Tile]));
    e
}

fn tile(e: &Engine) -> ObjectId {
    e.create_by_name("Test", "Tile", None).unwrap()
}

fn text(s: &str) -> Value {
    Value::String(s.into())
}

fn refusal(e: &Engine, obj: ObjectId, name: &str, v: Value) -> String {
    e.write_by_name(obj, name, v)
        .expect_err("refused")
        .to_string()
}

#[test]
fn the_field_stays_a_number_and_the_property_says_which_enum() {
    let m = Tile::static_meta();
    let ty = |name: &str| {
        let index = m.property_index(name).unwrap();
        m.property(index).unwrap().ty.clone()
    };
    assert_eq!(ty("corner"), TypeTag::Enum("Corner".into()));
    assert_eq!(ty("mode"), TypeTag::Enum("Mode".into()));
    assert_eq!(ty("plain"), TypeTag::Int);
    // The typed handle is the field's: a number.
    let e = engine();
    let t = tile(&e);
    e.set(t, Tile::MODE, 4).unwrap();
    assert_eq!(e.get(t, Tile::MODE), Ok(4));
}

#[test]
fn a_string_names_a_key_and_reads_as_its_number() {
    let e = engine();
    let t = tile(&e);
    e.write_by_name(t, "corner", text("BottomLeft")).unwrap();
    assert_eq!(e.read(t, Shape::CORNER.index), Value::Int(2));
    assert_eq!(e.get(t, Shape::CORNER), Ok(2));
    // An inherited enum, and the type's own.
    e.write_by_name(t, "fold", text("TopRight")).unwrap();
    assert_eq!(e.get(t, Tile::FOLD), Ok(1));
    e.write_by_name(t, "mode", text("Striped")).unwrap();
    assert_eq!(e.get(t, Tile::MODE), Ok(4));
}

#[test]
fn numbers_are_taken_as_qt_takes_them() {
    let e = engine();
    let t = tile(&e);
    for (given, want) in [
        (Value::Int(1), 1),
        // Rounded, half away from zero.
        (Value::Real(1.5), 2),
        (Value::Real(1.4), 1),
        (Value::Bool(true), 1),
        (Value::Bool(false), 0),
        // No key has this number; the number is kept.
        (Value::Int(9), 9),
    ] {
        e.write_by_name(t, "corner", given.clone()).unwrap();
        assert_eq!(e.get(t, Shape::CORNER), Ok(want), "{given:?}");
    }
}

#[test]
fn what_is_no_key_is_refused_with_the_name_of_the_enum() {
    let e = engine();
    let t = tile(&e);
    e.write_by_name(t, "corner", text("TopRight")).unwrap();
    assert_eq!(
        refusal(&e, t, "corner", text("Nowhere")),
        "Unable to assign QString to Shape::Corner"
    );
    for (given, kind) in [
        // Keys are matched as written.
        (text("topright"), "QString"),
        (text(""), "QString"),
        (text("1"), "QString"),
        // A key of another enum of the type.
        (text("Striped"), "QString"),
        (Value::Null, "null"),
        (Value::Undefined, "[undefined]"),
        (Value::Real(f64::NAN), "double"),
    ] {
        assert_eq!(
            refusal(&e, t, "corner", given),
            format!("Unable to assign {kind} to Shape::Corner")
        );
    }
    assert_eq!(e.get(t, Shape::CORNER), Ok(1));
    // The enum is named with the type that declares it.
    assert_eq!(
        refusal(&e, t, "fold", text("Striped")),
        "Unable to assign QString to Shape::Corner"
    );
    assert_eq!(
        refusal(&e, t, "mode", text("TopLeft")),
        "Unable to assign QString to Tile::Mode"
    );
    // A plain number property knows no keys.
    let _ = e.write_by_name(t, "plain", text("TopRight"));
    assert_eq!(e.get(t, Shape::PLAIN), Ok(0));
}

#[test]
fn a_binding_gives_a_key_by_name_and_keeps_the_value_when_it_gives_none() {
    let e = engine();
    let t = tile(&e);
    e.bind(t, Shape::CORNER.index, |_| text("TopRight"))
        .unwrap();
    assert_eq!(e.get(t, Shape::CORNER), Ok(1));
    assert!(e.take_warnings().is_empty());
    e.bind(t, Shape::CORNER.index, |_| text("Nowhere")).unwrap();
    assert_eq!(e.get(t, Shape::CORNER), Ok(1));
    let w = e.take_warnings();
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("Unable to assign QString to Shape::Corner"),
        "{w:?}"
    );
}

#[test]
fn a_layout_or_animation_write_is_checked_the_same() {
    let e = engine();
    let t = tile(&e);
    let corner = Shape::CORNER.index;
    e.write(t, corner, text("BottomLeft"), WriteKind::Animation)
        .unwrap();
    assert_eq!(e.get(t, Shape::CORNER), Ok(2));
    assert!(
        e.write(t, corner, text("Nowhere"), WriteKind::Animation)
            .is_err()
    );
}

#[test]
fn the_meta_object_converts_for_its_enums() {
    let m = Tile::static_meta();
    assert_eq!(m.coerce_enum("Mode", &text("Striped")), Ok(Value::Int(4)));
    assert_eq!(m.coerce_enum("Corner", &text("TopLeft")), Ok(Value::Int(0)));
    // -1, which an enum reads unsigned.
    assert_eq!(
        m.coerce_enum("Mode", &Value::Real(-0.5)),
        Ok(Value::Real(4_294_967_295.0))
    );
    let e = m.coerce_enum("Mode", &text("TopLeft")).unwrap_err();
    assert_eq!(e.to_string(), "Unable to assign QString to Tile::Mode");
    // An enum the type does not have takes numbers and names no key.
    assert_eq!(m.coerce_enum("Gone", &Value::Int(3)), Ok(Value::Int(3)));
    let e = m.coerce_enum("Gone", &text("TopLeft")).unwrap_err();
    assert_eq!(e.to_string(), "Unable to assign QString to Gone");
    assert_eq!(
        m.enumeration("Corner").map(|c| c.keys.len()),
        Some(3),
        "inherited"
    );
    assert!(Shape::static_meta().enumeration("Mode").is_none());
}

#[test]
fn flags_take_several_keys_and_an_enum_takes_one() {
    let m = Tile::static_meta();
    let ty = |name: &str| {
        let index = m.property_index(name).unwrap();
        m.property(index).unwrap().ty.clone()
    };
    assert_eq!(ty("modes"), TypeTag::Flags("Mode".into()));
    assert_eq!(ty("modes").name(), ty("mode").name());
    let e = engine();
    let t = tile(&e);
    for (given, want) in [
        (text("Striped"), 4),
        (text("Striped|Dotted"), 5),
        // Qt allows space around the bar.
        (text("Dotted | Striped"), 5),
        (text("Plain"), 0),
        (text("Dotted|Dotted"), 1),
        // A number is any combination, a key's or not.
        (Value::Int(7), 7),
        (Value::Int(-1), -1),
        (Value::Real(2.7), 3),
        (Value::Bool(false), 0),
    ] {
        e.write_by_name(t, "modes", given.clone()).unwrap();
        assert_eq!(e.get(t, Tile::MODES), Ok(want), "{given:?}");
    }
    e.write_by_name(t, "modes", text("Dotted")).unwrap();
    for (given, kind) in [
        (text("Nowhere"), "QString"),
        (text("Striped|Nowhere"), "QString"),
        (text("Striped|"), "QString"),
        (text("Striped,Dotted"), "QString"),
        (text("striped"), "QString"),
        (text(""), "QString"),
        (text("5"), "QString"),
        (Value::Null, "null"),
        (Value::Undefined, "[undefined]"),
    ] {
        assert_eq!(
            refusal(&e, t, "modes", given),
            format!("Unable to assign {kind} to Tile::Mode")
        );
    }
    assert_eq!(
        e.get(t, Tile::MODES),
        Ok(1),
        "what is refused changes nothing"
    );
    // The same keys, where one is held.
    assert_eq!(
        refusal(&e, t, "mode", text("Striped|Dotted")),
        "Unable to assign QString to Tile::Mode"
    );
}

#[test]
fn an_enum_of_the_qt_namespace_is_named_with_qt_in_front() {
    let e = engine();
    let t = tile(&e);
    e.write_by_name(t, "alignment", text("AlignLeft|AlignVCenter"))
        .unwrap();
    assert_eq!(e.get(t, Tile::ALIGNMENT), Ok(129));
    e.write_by_name(t, "orientation", text("Vertical")).unwrap();
    assert_eq!(e.get(t, Tile::ORIENTATION), Ok(2));
    assert_eq!(
        refusal(&e, t, "alignment", text("AlignLeft|Vertical")),
        "Unable to assign QString to Qt::Alignment"
    );
    assert_eq!(
        refusal(&e, t, "orientation", text("Horizontal|Vertical")),
        "Unable to assign QString to Qt::Orientation"
    );
    // A key of the type itself is no key of a Qt enum.
    assert_eq!(
        refusal(&e, t, "orientation", text("Striped")),
        "Unable to assign QString to Qt::Orientation"
    );
    let m = Tile::static_meta();
    assert!(m.has_enumeration("Qt.Alignment") && m.has_enumeration("Qt.Orientation"));
    assert!(m.has_enumeration("Mode") && m.has_enumeration("Corner"));
    assert!(!m.has_enumeration("Qt.Mode") && !m.has_enumeration("Alignment"));
    assert_eq!(
        m.coerce_flags("Qt.MouseButtons", &text("LeftButton|MiddleButton")),
        Ok(Value::Int(5))
    );
}

#[test]
fn a_value_is_written_as_the_key_it_is() {
    let m = Tile::static_meta();
    let key = |ty: TypeTag, n: i32| m.key_text(&ty, &Value::Int(n));
    // The type that declares it names it, inherited or not.
    assert_eq!(
        key(TypeTag::Enum("Mode".into()), 4).as_deref(),
        Some("Tile.Striped")
    );
    assert_eq!(
        key(TypeTag::Enum("Corner".into()), 2).as_deref(),
        Some("Shape.BottomLeft")
    );
    assert_eq!(key(TypeTag::Enum("Mode".into()), 3), None, "no key");
    assert_eq!(key(TypeTag::Int, 4), None);
    let buttons = || TypeTag::Flags("Qt.MouseButtons".into());
    assert_eq!(key(buttons(), 1).as_deref(), Some("Qt.LeftButton"));
    assert_eq!(
        key(buttons(), 5).as_deref(),
        Some("Qt.LeftButton | Qt.MiddleButton")
    );
    // Of two keys with one number, the first declared.
    assert_eq!(key(buttons(), 8).as_deref(), Some("Qt.BackButton"));
    assert_eq!(key(buttons(), 0).as_deref(), Some("Qt.NoButton"));
    // A combination is no key of an enum that is not flags.
    assert_eq!(key(TypeTag::Enum("Qt.MouseButtons".into()), 3), None);
    let align = TypeTag::Flags("Qt.Alignment".into());
    assert_eq!(
        key(align, 0x81).as_deref(),
        Some("Qt.AlignLeft | Qt.AlignVCenter")
    );
}
