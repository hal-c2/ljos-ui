//! Taking a property back with `undefined`: a type says which of its
//! properties have a reset and how; the others refuse `undefined`. What Qt
//! 6.11.2 does (by running qml6) is in
//! `features/bindings/property-reset.feature`.

use ljos_engine::{
    Atom, Engine, EngineError, ObjectId, PropIndex, Provenance, QmlObject, QmlType, Reset, Value,
    WriteKind,
};

/// A side follows `margins` until it is given a value of its own.
fn margins(e: &Engine, obj: ObjectId, _side: PropIndex) -> Value {
    e.read(obj, Shelf::MARGINS.index)
}

fn natural(e: &Engine, obj: ObjectId, _width: PropIndex) -> Value {
    e.read(obj, Shelf::NATURAL.index)
}

/// Counts in `resets` how often it ran, which only the type can know.
fn loosen(e: &Engine, obj: ObjectId, width: PropIndex) -> Result<(), EngineError> {
    let n = e.get(obj, Shelf::RESETS)?;
    e.set(obj, Shelf::RESETS, n + 1)?;
    e.bind_computed(obj, width, move |e| e.read(obj, Shelf::NATURAL.index))
}

fn edge_margin(e: &Engine, obj: ObjectId, _own: PropIndex) -> Value {
    e.read(obj, Shelf::MARGINS.index)
}

#[derive(Debug, QmlObject)]
#[qml(name = "ShelfEdge", uncreatable)]
pub struct Edge {
    #[qml(notify, reset)]
    pub style: i32,
    #[qml(notify, reset = Reset::Follow(edge_margin))]
    pub inset: f64,
    #[qml(notify)]
    pub plain: f64,
}

#[derive(Debug, QmlObject)]
#[qml(module = "Test", name = "Shelf")]
pub struct Shelf {
    /// Three by default, which no other number says.
    #[qml(notify, reset)]
    pub depth: f64,
    #[qml(notify)]
    pub margins: f64,
    #[qml(notify, reset = Reset::Follow(margins))]
    pub left_margin: f64,
    #[qml(notify)]
    pub natural: f64,
    #[qml(notify, reset = Reset::Custom { apply: loosen, value: natural })]
    pub width: f64,
    #[qml(notify)]
    pub resets: i32,
    #[qml(notify)]
    pub plain: f64,
    #[qml(notify, reset)]
    pub label: String,
    #[qml(group)]
    pub edge: Edge,
}

impl Default for Shelf {
    fn default() -> Self {
        Shelf {
            depth: 3.0,
            margins: 0.0,
            left_margin: 0.0,
            natural: 10.0,
            width: 0.0,
            resets: 0,
            plain: 1.0,
            label: "unnamed".into(),
            edge: Edge {
                style: 2,
                inset: 0.0,
                plain: 0.0,
            },
        }
    }
}

fn unnamed(v: &Value) -> bool {
    matches!(v, Value::String(s) if s.is_empty())
}

fn by_place(e: &Engine, obj: ObjectId, _tag: PropIndex) -> Value {
    let place = e.read(obj, Cart::PLACE.index).to_number();
    Value::String(Atom::intern(&format!("cart {place}")))
}

/// A shelf on wheels. It works out the `plain` a shelf only stores, and
/// goes by its place until it is given a tag.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "Test",
    name = "Cart",
    extends = Shelf,
    reset_inherited(plain = Reset::Follow(natural))
)]
pub struct Cart {
    #[qml(base)]
    pub shelf: Shelf,
    #[qml(notify)]
    pub place: f64,
    #[qml(notify, reset = Reset::Absent { absent: unnamed, value: by_place })]
    pub tag: String,
}

fn engine() -> Engine {
    let e = Engine::new();
    e.register_module(ljos_engine::module!("Test" 1.0 => [Shelf, Cart]));
    e
}

fn cart(e: &Engine) -> ObjectId {
    e.create_by_name("Test", "Cart", None).unwrap()
}

fn shelf(e: &Engine) -> ObjectId {
    e.create_by_name("Test", "Shelf", None).unwrap()
}

fn undefine(e: &Engine, obj: ObjectId, name: &str) -> Result<(), EngineError> {
    e.write_by_name(obj, name, Value::Undefined)
}

fn num(e: &Engine, obj: ObjectId, name: &str) -> f64 {
    e.read_by_name(obj, name).to_number()
}

#[test]
fn the_type_says_which_properties_have_a_reset() {
    let m = Shelf::static_meta();
    let reset = |name: &str| m.property(m.property_index(name).unwrap()).unwrap().reset;
    assert!(matches!(reset("depth"), Some(Reset::Default)));
    assert!(matches!(reset("leftMargin"), Some(Reset::Follow(_))));
    assert!(matches!(reset("width"), Some(Reset::Custom { .. })));
    assert!(reset("plain").is_none());
    // A member of a group has the reset its group's type gave it.
    assert!(matches!(reset("edge.style"), Some(Reset::Default)));
    assert!(matches!(reset("edge.inset"), Some(Reset::Follow(_))));
    assert!(reset("edge.plain").is_none());
    assert!(reset("edge").is_none());
}

#[test]
fn undefined_gives_the_value_a_new_object_has() {
    let e = engine();
    let s = shelf(&e);
    for watched in [false, true] {
        if watched {
            // With an observer the value lives in the graph too.
            e.watch(s, Shelf::DEPTH.index, |_, _| {}).unwrap();
        }
        e.write_by_name(s, "depth", Value::Real(8.0)).unwrap();
        assert_eq!(
            e.provenance(s, Shelf::DEPTH.index),
            Some(Provenance::Assigned)
        );
        undefine(&e, s, "depth").unwrap();
        assert_eq!(num(&e, s, "depth"), 3.0, "watched: {watched}");
        assert_eq!(e.get(s, Shelf::DEPTH), Ok(3.0));
        assert_eq!(
            e.provenance(s, Shelf::DEPTH.index),
            Some(Provenance::Default),
            "it counts as never set"
        );
    }
    e.write_by_name(s, "label", Value::String("top".into()))
        .unwrap();
    undefine(&e, s, "label").unwrap();
    assert_eq!(e.read_by_name(s, "label"), Value::String("unnamed".into()));
    e.write_by_name(s, "edge.style", Value::Int(7)).unwrap();
    undefine(&e, s, "edge.style").unwrap();
    assert_eq!(e.read_by_name(s, "edge.style"), Value::Int(2));
}

#[test]
fn the_change_is_told() {
    let e = engine();
    let s = shelf(&e);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = seen.clone();
    e.watch(s, Shelf::DEPTH.index, move |_, v| {
        log.borrow_mut().push(v.to_number());
    })
    .unwrap();
    e.write_by_name(s, "depth", Value::Real(8.0)).unwrap();
    undefine(&e, s, "depth").unwrap();
    // Taken back twice there is nothing new.
    undefine(&e, s, "depth").unwrap();
    assert_eq!(*seen.borrow(), [8.0, 3.0]);
}

#[test]
fn a_binding_goes_when_the_property_is_taken_back() {
    let e = engine();
    let s = shelf(&e);
    e.bind(s, Shelf::DEPTH.index, move |e| {
        Value::Real(e.read(s, Shelf::PLAIN.index).to_number() * 2.0)
    })
    .unwrap();
    assert_eq!(num(&e, s, "depth"), 2.0);
    undefine(&e, s, "depth").unwrap();
    assert!(!e.is_bound(s, Shelf::DEPTH.index));
    e.write_by_name(s, "plain", Value::Real(50.0)).unwrap();
    assert_eq!(num(&e, s, "depth"), 3.0);
}

#[test]
fn a_property_that_follows_another_follows_it_again() {
    let e = engine();
    let s = shelf(&e);
    e.write_by_name(s, "margins", Value::Real(5.0)).unwrap();
    e.write_by_name(s, "leftMargin", Value::Real(9.0)).unwrap();
    undefine(&e, s, "leftMargin").unwrap();
    assert_eq!(num(&e, s, "leftMargin"), 5.0);
    e.write_by_name(s, "margins", Value::Real(12.0)).unwrap();
    assert_eq!(num(&e, s, "leftMargin"), 12.0);
    // Until it is given a value of its own again.
    e.write_by_name(s, "leftMargin", Value::Real(1.0)).unwrap();
    e.write_by_name(s, "margins", Value::Real(20.0)).unwrap();
    assert_eq!(num(&e, s, "leftMargin"), 1.0);
    // A member of a group does the same, given the object that holds
    // the group.
    e.write_by_name(s, "edge.inset", Value::Real(2.0)).unwrap();
    undefine(&e, s, "edge.inset").unwrap();
    assert_eq!(num(&e, s, "edge.inset"), 20.0);
}

#[test]
fn a_type_takes_its_property_back_its_own_way() {
    let e = engine();
    let s = shelf(&e);
    e.write_by_name(s, "width", Value::Real(80.0)).unwrap();
    undefine(&e, s, "width").unwrap();
    assert_eq!(num(&e, s, "width"), 10.0);
    assert_eq!(e.get(s, Shelf::RESETS), Ok(1));
    e.write_by_name(s, "natural", Value::Real(14.0)).unwrap();
    assert_eq!(num(&e, s, "width"), 14.0);
    // The same through the engine's own name for it.
    e.write_by_name(s, "width", Value::Real(80.0)).unwrap();
    e.reset(s, Shelf::WIDTH.index).unwrap();
    assert_eq!(num(&e, s, "width"), 14.0);
    assert_eq!(e.get(s, Shelf::RESETS), Ok(2));
}

#[test]
fn a_binding_that_yields_undefined_takes_the_property_back_and_stays() {
    let e = engine();
    let s = shelf(&e);
    // `plain` below zero stands for "no value".
    let given = move |e: &Engine| match e.read(s, Shelf::PLAIN.index).to_number() {
        n if n < 0.0 => Value::Undefined,
        n => Value::Real(n),
    };
    for (name, index, reset_to) in [
        ("depth", Shelf::DEPTH.index, 3.0),
        ("leftMargin", Shelf::LEFT_MARGIN.index, 6.0),
        ("width", Shelf::WIDTH.index, 10.0),
    ] {
        e.write_by_name(s, "plain", Value::Real(40.0)).unwrap();
        e.write_by_name(s, "margins", Value::Real(6.0)).unwrap();
        e.bind(s, index, given).unwrap();
        assert_eq!(num(&e, s, name), 40.0, "{name}");
        e.write_by_name(s, "plain", Value::Real(-1.0)).unwrap();
        assert_eq!(num(&e, s, name), reset_to, "{name}");
        assert!(e.is_bound(s, index), "{name}: the binding stays");
        e.write_by_name(s, "plain", Value::Real(41.0)).unwrap();
        assert_eq!(num(&e, s, name), 41.0, "{name}");
    }
    assert!(e.take_log().is_empty(), "nothing is warned about");
    // The type's own way of taking back is for a script: a binding that
    // stays in place only needs the value.
    assert_eq!(e.get(s, Shelf::RESETS), Ok(0));

    // What it follows while it yields undefined moves it.
    e.write_by_name(s, "plain", Value::Real(-1.0)).unwrap();
    e.write_by_name(s, "natural", Value::Real(17.0)).unwrap();
    assert_eq!(num(&e, s, "width"), 17.0);
}

#[test]
fn a_property_without_a_reset_refuses_undefined() {
    let e = engine();
    let s = shelf(&e);
    let err = undefine(&e, s, "plain").unwrap_err();
    assert_eq!(err.to_string(), "Unable to assign [undefined] to double");
    assert_eq!(num(&e, s, "plain"), 1.0);
    let err = e.reset(s, Shelf::PLAIN.index).unwrap_err();
    assert_eq!(err.to_string(), "Unable to assign [undefined] to double");
    let err = undefine(&e, s, "edge.plain").unwrap_err();
    assert_eq!(err.to_string(), "Unable to assign [undefined] to double");
    assert!(e.reset_of(s, Shelf::PLAIN.index).is_none());
    assert!(e.reset_of(s, Shelf::DEPTH.index).is_some());

    // A binding that yields undefined keeps the value and warns.
    e.bind(s, Shelf::PLAIN.index, |_| Value::Undefined).unwrap();
    assert_eq!(num(&e, s, "plain"), 1.0);
    let log = e.take_log();
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(
        log[0]
            .message
            .contains("Unable to assign [undefined] to double"),
        "{log:?}"
    );
}

#[test]
fn what_a_new_object_is_given_as_undefined_is_as_it_was() {
    let e = engine();
    let s = shelf(&e);
    // The creator's write (`depth: undefined` in a file).
    e.write(s, Shelf::DEPTH.index, Value::Undefined, WriteKind::Initial)
        .unwrap();
    assert_eq!(num(&e, s, "depth"), 3.0);
    assert_eq!(
        e.provenance(s, Shelf::DEPTH.index),
        Some(Provenance::Default)
    );
}

#[test]
fn a_derived_type_takes_back_what_its_base_only_stores() {
    let reset = |m: &'static ljos_engine::MetaObject| {
        m.property(m.property_index("plain").unwrap())
            .unwrap()
            .reset
    };
    assert!(reset(Shelf::static_meta()).is_none());
    assert!(matches!(reset(Cart::static_meta()), Some(Reset::Follow(_))));

    let e = engine();
    let c = cart(&e);
    e.write_by_name(c, "natural", Value::Real(7.0)).unwrap();
    e.write_by_name(c, "plain", Value::Real(2.0)).unwrap();
    undefine(&e, c, "plain").unwrap();
    assert_eq!(num(&e, c, "plain"), 7.0);
    e.write_by_name(c, "natural", Value::Real(8.0)).unwrap();
    assert_eq!(num(&e, c, "plain"), 8.0);
    // The base is as it was.
    let s = shelf(&e);
    assert!(undefine(&e, s, "plain").is_err());
    assert_eq!(num(&e, s, "plain"), 1.0);
}

#[test]
fn a_value_that_stands_for_none_takes_the_property_back() {
    let e = engine();
    let c = cart(&e);
    let tag = |e: &Engine| e.read_by_name(c, "tag").to_js_string().to_string();
    e.write_by_name(c, "place", Value::Real(4.0)).unwrap();
    e.write_by_name(c, "tag", Value::String(Atom::intern("red")))
        .unwrap();
    assert_eq!(tag(&e), "red");
    e.write_by_name(c, "tag", Value::String(Atom::EMPTY))
        .unwrap();
    assert_eq!(tag(&e), "cart 4");
    // What it was given is asked for once; the property does not follow.
    e.write_by_name(c, "place", Value::Real(5.0)).unwrap();
    assert_eq!(tag(&e), "cart 4");
    undefine(&e, c, "tag").unwrap();
    assert_eq!(tag(&e), "cart 5");
}
