//! The engine through derived native types: meta layout, properties,
//! bindings, signals, methods, lifetimes, contexts and the type dump.

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use common::{Border, Item, Rectangle, engine};
use ljos_engine::{
    Color, Cx, EngineError, MetaBuilder, MetaRef, PropIndex, PropertyFlags, Provenance, QmlType,
    TypeTag, Value, Version, WriteKind,
};

fn rect(e: &ljos_engine::Engine) -> ljos_engine::ObjectId {
    e.create_by_name("Test", "Rectangle", None).unwrap()
}

#[test]
fn meta_tables_are_flattened() {
    // QtObject's objectName, which a type QML makes by name builds on
    // without a field for it, and Item's own five.
    assert_eq!(Item::PROPERTY_COUNT, 7);
    assert_eq!(Item::static_meta().base.as_ref().unwrap().name, "QtObject");
    // Own: color, radius, border, border.width, border.color, antialiasing.
    assert_eq!(Rectangle::PROPERTY_COUNT, 13);
    // A group's type is not an object of its own and has no base.
    assert_eq!(Border::PROPERTY_COUNT, 2);

    let m = Rectangle::static_meta();
    assert_eq!(m.properties.len(), 13);
    assert_eq!(m.property_offset, 7);
    assert_eq!(m.base.as_ref().unwrap().name, "Item");
    assert!(m.inherits(Item::static_meta()));
    assert!(m.inherits_name("Item"));
    assert!(!Item::static_meta().inherits(m));

    // The same index in the base and the derived type.
    assert_eq!(m.property_index("width"), Some(Item::WIDTH.index));
    assert_eq!(
        Item::static_meta().property_index("width"),
        Some(Item::WIDTH.index)
    );
    assert_eq!(m.property_index("radius"), Some(Rectangle::RADIUS.index));
    assert_eq!(Rectangle::RADIUS.index, PropIndex(8));
    assert_eq!(
        m.property_index("antialiasing"),
        Some(Rectangle::SMOOTH_EDGES.index)
    );
    assert_eq!(m.property_index("smoothEdges"), None);
    assert_eq!(m.property_index("layoutPasses"), None);
    assert_eq!(
        m.property_index("implicitWidth"),
        Some(Item::IMPLICIT_WIDTH.index)
    );
    assert_eq!(m.property_index("border.width"), Some(PropIndex(10)));

    let border = m.property(PropIndex(9)).unwrap();
    assert!(border.flags.contains(PropertyFlags::GROUP));
    assert_eq!(border.ty, TypeTag::Group("Border".into()));
    let iw = m.property(Item::IMPLICIT_WIDTH.index).unwrap();
    assert!(
        iw.flags
            .contains(PropertyFlags::READONLY | PropertyFlags::FINAL)
    );

    // Change signals come with `notify`; declared signals after them.
    let wc = m.signal_index("widthChanged").unwrap();
    assert_eq!(m.signal(wc).unwrap().notify_of, Some(Item::WIDTH.index));
    assert_eq!(m.property(Item::WIDTH.index).unwrap().notify, Some(wc));
    let clicked = m.signal(m.signal_index("clicked").unwrap()).unwrap();
    assert_eq!(clicked.params.len(), 2);
    assert_eq!(clicked.params[0].0, "x");
    assert_eq!(clicked.params[1].1, TypeTag::Real);
    assert!(m.signal_index("doubleClicked").is_some());
    assert!(
        m.property(Rectangle::SMOOTH_EDGES.index)
            .unwrap()
            .notify
            .is_none()
    );

    let names: Vec<_> = m.own_methods().iter().map(|x| x.name.to_string()).collect();
    assert_eq!(names, ["area", "grow", "setSize", "fail", "bump"]);
    assert_eq!(
        m.default_property().map(|a| a.to_string()),
        Some("data".into())
    );
    assert!(m.create.is_some());
    assert!(Border::static_meta().create.is_none());
}

#[test]
fn typed_and_dynamic_access() {
    let e = engine();
    let r = rect(&e);
    e.set(r, Item::WIDTH, 40.0).unwrap();
    e.set(r, Rectangle::COLOR, Color::from_rgba8(255, 0, 0, 255))
        .unwrap();
    assert_eq!(e.get(r, Item::WIDTH), Ok(40.0));
    assert_eq!(e.read_by_name(r, "width"), Value::Real(40.0));
    assert_eq!(e.read_by_name(r, "color").to_js_string(), "#ff0000");

    // Coerced to the property type, as a QML assignment is.
    e.write_by_name(r, "radius", Value::string("2.5")).unwrap();
    e.write_by_name(r, "color", Value::string("blue")).unwrap();
    assert_eq!(e.get(r, Rectangle::RADIUS), Ok(2.5));
    assert_eq!(
        e.with(r, |x: &Rectangle| x.color),
        Ok(Color::from_rgba8(0, 0, 255, 255))
    );
    let err = e.write_by_name(r, "radius", Value::Undefined).unwrap_err();
    assert_eq!(err.to_string(), "Unable to assign [undefined] to real");

    // Group leaves are properties of the object.
    e.write_by_name(r, "border.width", Value::Int(3)).unwrap();
    assert_eq!(e.read_by_name(r, "border.width"), Value::Real(3.0));
    assert_eq!(e.with(r, |x: &Rectangle| x.border.width), Ok(3.0));
    assert!(matches!(
        e.write_by_name(r, "border", Value::Null),
        Err(EngineError::GroupProperty(_))
    ));

    // Unknown names read as undefined.
    assert_eq!(e.read_by_name(r, "nope"), Value::Undefined);

    // A Rectangle handle on an Item is refused.
    let i = e.create_by_name("Test", "Item", None).unwrap();
    assert!(matches!(
        e.get(i, Rectangle::RADIUS),
        Err(EngineError::WrongType { .. })
    ));
    assert_eq!(e.get(i, Item::WIDTH), Ok(0.0));
}

#[test]
fn readonly_is_rejected_from_qml_but_not_native() {
    let e = engine();
    let r = rect(&e);
    let err = e
        .write_by_name(r, "implicitWidth", Value::Int(5))
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid property assignment: \"implicitWidth\" is a read-only property"
    );
    assert!(matches!(
        e.set(r, Item::IMPLICIT_WIDTH, 5.0),
        Err(EngineError::ReadOnly(_))
    ));
    assert!(matches!(
        e.bind(r, Item::IMPLICIT_WIDTH.index, |_| Value::Int(1)),
        Err(EngineError::ReadOnly(_))
    ));
    e.set_internal(r, Item::IMPLICIT_WIDTH, 5.0).unwrap();
    assert_eq!(e.get(r, Item::IMPLICIT_WIDTH), Ok(5.0));
}

#[test]
fn nodes_are_allocated_lazily() {
    let e = engine();
    let r = rect(&e);
    let before = e.graph().len();
    e.set(r, Item::WIDTH, 1.0).unwrap();
    let _ = e.read_by_name(r, "width");
    assert_eq!(
        e.graph().len(),
        before,
        "untracked reads and writes make no nodes"
    );
    assert_eq!(
        e.provenance(r, Item::WIDTH.index),
        Some(Provenance::Assigned)
    );
    assert_eq!(
        e.provenance(r, Item::HEIGHT.index),
        Some(Provenance::Default)
    );
    let other = rect(&e);
    e.bind(other, Item::WIDTH.index, move |e| {
        e.read(r, Item::WIDTH.index)
    })
    .unwrap();
    assert_eq!(e.read(other, Item::WIDTH.index), Value::Real(1.0));
    assert!(e.graph().len() > before);
    // Provenance carried over when the node appeared.
    assert_eq!(
        e.provenance(r, Item::WIDTH.index),
        Some(Provenance::Assigned)
    );
    assert_eq!(
        e.provenance(other, Item::WIDTH.index),
        Some(Provenance::Bound)
    );
}

#[test]
fn bindings_through_derived_properties() {
    let e = engine();
    let a = rect(&e);
    let b = rect(&e);
    e.set(a, Rectangle::RADIUS, 2.0).unwrap();
    e.set(a, Item::X, 10.0).unwrap();
    // b.width: a.radius * 2 + a.x
    e.bind(b, Item::WIDTH.index, move |e| {
        Value::Real(e.get(a, Rectangle::RADIUS).unwrap() * 2.0 + e.get(a, Item::X).unwrap())
    })
    .unwrap();
    assert_eq!(e.get(b, Item::WIDTH), Ok(14.0));
    e.set(a, Rectangle::RADIUS, 5.0).unwrap();
    assert_eq!(e.get(b, Item::WIDTH), Ok(20.0));
    e.set(a, Item::X, 0.0).unwrap();
    assert_eq!(e.get(b, Item::WIDTH), Ok(10.0));

    // Group leaf, bound to a base property.
    let bw = e.property_index(b, "border.width").unwrap();
    e.bind(b, bw, move |e| e.read(b, Item::WIDTH.index))
        .unwrap();
    assert_eq!(e.read(b, bw), Value::Real(10.0));
    e.set(a, Rectangle::RADIUS, 1.0).unwrap();
    assert_eq!(e.read(b, bw), Value::Real(2.0));

    // Bindings are lazy: the field cache catches up on refresh.
    e.set(a, Rectangle::RADIUS, 3.0).unwrap();
    e.refresh_fields(b);
    assert_eq!(
        e.with(b, |r: &Rectangle| (r.item.width, r.border.width)),
        Ok((6.0, 6.0))
    );

    // A QML assignment breaks the binding.
    e.set(b, Item::WIDTH, 1.0).unwrap();
    assert!(!e.is_bound(b, Item::WIDTH.index));
    e.set(a, Rectangle::RADIUS, 100.0).unwrap();
    assert_eq!(e.get(b, Item::WIDTH), Ok(1.0));
}

#[test]
fn failed_binding_keeps_value_and_warns() {
    let e = engine();
    let r = rect(&e);
    e.set(r, Item::WIDTH, 7.0).unwrap();
    e.bind(r, Item::WIDTH.index, |_| Value::Undefined).unwrap();
    assert_eq!(e.get(r, Item::WIDTH), Ok(7.0));
    let w = e.take_warnings();
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("Unable to assign [undefined] to real"),
        "{w:?}"
    );
}

#[test]
fn change_signals_follow_the_graph() {
    let e = engine();
    let r = rect(&e);
    let hits = Rc::new(Cell::new(0));
    let h = hits.clone();
    let c = e
        .connect_by_name(r, "widthChanged", move |e, args| {
            // The new value goes along, as Qt's change signals carry it.
            assert_eq!(args.len(), 1);
            assert_eq!(args[0], e.read_by_name(r, "width"));
            h.set(h.get() + 1);
        })
        .unwrap();
    e.set(r, Item::WIDTH, 5.0).unwrap();
    assert_eq!(hits.get(), 1);
    e.set(r, Item::WIDTH, 5.0).unwrap();
    assert_eq!(hits.get(), 1, "equal writes do not notify");

    // A native mutation the engine did not see, then notify.
    e.with_mut(r, |x: &mut Item| x.width = 9.0).unwrap();
    e.notify(r, Item::WIDTH.index).unwrap();
    assert_eq!(hits.get(), 2);
    assert_eq!(e.get(r, Item::WIDTH), Ok(9.0));

    // Batched writes notify once.
    e.batch(|| {
        e.set(r, Item::WIDTH, 1.0).unwrap();
        e.set(r, Item::WIDTH, 2.0).unwrap();
    });
    assert_eq!(hits.get(), 3);

    // Emitting the change signal by hand notifies too.
    e.emit_by_name(r, "widthChanged", &[]).unwrap();
    assert_eq!(hits.get(), 4);

    assert!(e.disconnect(c));
    assert!(!e.disconnect(c));
    e.set(r, Item::WIDTH, 3.0).unwrap();
    assert_eq!(hits.get(), 4);
}

#[test]
fn change_signal_of_a_bound_property() {
    let e = engine();
    let a = rect(&e);
    let b = rect(&e);
    e.bind(b, Item::HEIGHT.index, move |e| e.read(a, Item::WIDTH.index))
        .unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    e.connect_by_name(b, "heightChanged", move |e, _| {
        s.borrow_mut().push(e.get(b, Item::HEIGHT).unwrap());
    })
    .unwrap();
    e.set(a, Item::WIDTH, 4.0).unwrap();
    e.set(a, Item::WIDTH, 8.0).unwrap();
    assert_eq!(*seen.borrow(), [4.0, 8.0]);
}

#[test]
fn invokables() {
    let e = engine();
    let r = rect(&e);
    e.invoke_by_name(r, "setSize", &[Value::Int(3), Value::Real(4.0)])
        .unwrap();
    assert_eq!(e.invoke_by_name(r, "area", &[]), Ok(Value::Real(12.0)));
    assert_eq!(
        e.invoke_by_name(r, "grow", &[Value::Int(2)]),
        Ok(Value::Real(2.0))
    );
    assert_eq!(
        e.invoke_by_name(r, "grow", &[Value::string("1.5")]),
        Ok(Value::Real(3.5))
    );
    assert_eq!(e.get(r, Rectangle::RADIUS), Ok(3.5));
    // Missing argument is undefined, which a real refuses.
    assert!(matches!(
        e.invoke_by_name(r, "grow", &[]),
        Err(EngineError::Convert(_))
    ));
    assert_eq!(
        e.invoke_by_name(r, "fail", &[]),
        Err(EngineError::Invoke("nope".into()))
    );
    assert_eq!(e.invoke_by_name(r, "bump", &[]), Ok(Value::Undefined));
    // 1 from completion on creation, 10 from `bump`.
    assert_eq!(e.with(r, |x: &Rectangle| x.completed_runs), Ok(11));
    assert!(matches!(
        e.invoke_by_name(r, "helper", &[]),
        Err(EngineError::UnknownMethod { .. })
    ));
    // Base-typed objects lack derived methods.
    let i = e.create_by_name("Test", "Item", None).unwrap();
    assert!(e.invoke_by_name(i, "area", &[]).is_err());
}

#[test]
fn invokable_writes_reach_bindings() {
    let e = engine();
    let r = rect(&e);
    let other = rect(&e);
    e.bind(other, Item::X.index, move |e| e.read(r, Item::WIDTH.index))
        .unwrap();
    e.invoke_by_name(r, "setSize", &[Value::Int(6), Value::Int(1)])
        .unwrap();
    assert_eq!(e.get(other, Item::X), Ok(6.0));
}

#[test]
fn signals_emit_with_arguments() {
    let e = engine();
    let r = rect(&e);
    let got = Rc::new(RefCell::new(Vec::new()));
    let g = got.clone();
    e.connect_by_name(r, "clicked", move |_, args| {
        g.borrow_mut().push(args.to_vec())
    })
    .unwrap();
    let dbl = Rc::new(Cell::new(0));
    let d = dbl.clone();
    e.connect_by_name(r, "doubleClicked", move |_, _| d.set(d.get() + 1))
        .unwrap();

    Rectangle::emit_clicked(&Cx::new(&e, r), 1.0, 2.5).unwrap();
    // By name, with coercion and a missing argument.
    e.emit_by_name(r, "clicked", &[Value::Int(3)]).unwrap();
    Rectangle::emit_double_click(&Cx::new(&e, r)).unwrap();

    assert_eq!(
        *got.borrow(),
        [
            vec![Value::Real(1.0), Value::Real(2.5)],
            vec![Value::Real(3.0), Value::Undefined],
        ]
    );
    assert_eq!(dbl.get(), 1);

    // The typed helper refuses objects of another type.
    let i = e.create_by_name("Test", "Item", None).unwrap();
    assert!(matches!(
        Rectangle::emit_clicked(&Cx::new(&e, i), 0.0, 0.0),
        Err(EngineError::WrongType { .. })
    ));
}

#[test]
fn reentrant_binding_reads_other_objects_and_writes_elsewhere() {
    let e = engine();
    let a = rect(&e);
    let b = rect(&e);
    let c = rect(&e);
    e.set(b, Item::HEIGHT, 2.0).unwrap();
    // a.width reads b, and as a side effect writes c (bad style, but QML
    // allows it and the engine must not hold a borrow across it).
    e.bind(a, Item::WIDTH.index, move |e| {
        let h = e.get(b, Item::HEIGHT).unwrap();
        e.set(c, Item::X, h * 10.0).unwrap();
        let _ = e.invoke_by_name(c, "area", &[]).unwrap();
        Value::Real(h * 2.0)
    })
    .unwrap();
    assert_eq!(e.get(a, Item::WIDTH), Ok(4.0));
    assert_eq!(e.get(c, Item::X), Ok(20.0));
    e.set(b, Item::HEIGHT, 3.0).unwrap();
    assert_eq!(e.get(a, Item::WIDTH), Ok(6.0));
    assert_eq!(e.get(c, Item::X), Ok(30.0));
}

#[test]
fn reentrant_handlers() {
    let e = engine();
    let a = rect(&e);
    let b = rect(&e);
    // A width handler that reads a, writes b, connects and emits.
    let late = Rc::new(Cell::new(0));
    let l = late.clone();
    e.connect_by_name(a, "widthChanged", move |e, _| {
        let w = e.get(a, Item::WIDTH).unwrap();
        e.set(b, Item::WIDTH, w + 1.0).unwrap();
        let l = l.clone();
        e.connect_by_name(a, "clicked", move |_, _| l.set(l.get() + 1))
            .unwrap();
        e.emit_by_name(a, "clicked", &[]).unwrap();
    })
    .unwrap();
    e.set(a, Item::WIDTH, 1.0).unwrap();
    assert_eq!(e.get(b, Item::WIDTH), Ok(2.0));
    assert_eq!(late.get(), 1);

    // A handler that destroys its own sender; later handlers on it are
    // dropped with the connection.
    let after = Rc::new(Cell::new(0));
    e.connect_by_name(b, "clicked", move |e, _| {
        e.destroy_now(b);
    })
    .unwrap();
    let af = after.clone();
    e.connect_by_name(b, "clicked", move |_, _| af.set(af.get() + 1))
        .unwrap();
    e.emit_by_name(b, "clicked", &[]).unwrap();
    assert!(!e.is_alive(b));
    assert_eq!(after.get(), 0);
}

#[test]
fn handler_disconnecting_another_during_emit() {
    let e = engine();
    let r = rect(&e);
    let second = Rc::new(Cell::new(None));
    let hits = Rc::new(Cell::new(0));
    let s = second.clone();
    e.connect_by_name(r, "doubleClicked", move |e, _| {
        if let Some(c) = s.take() {
            e.disconnect(c);
        }
    })
    .unwrap();
    let h = hits.clone();
    let c2 = e
        .connect_by_name(r, "doubleClicked", move |_, _| h.set(h.get() + 1))
        .unwrap();
    second.set(Some(c2));
    e.emit_by_name(r, "doubleClicked", &[]).unwrap();
    assert_eq!(hits.get(), 0);
}

/// A type declared in QML: `Rectangle { property QtObject target; property var data }`.
fn qml_type() -> MetaRef {
    MetaBuilder::new("Card")
        .base(Rectangle::static_meta())
        .property(
            "target",
            TypeTag::Object(None),
            PropertyFlags::empty(),
            true,
        )
        .property("note", TypeTag::String, PropertyFlags::empty(), true)
        .build_shared()
}

#[test]
fn qml_declared_types_extend_natives() {
    let e = engine();
    let card = qml_type();
    e.register_type("App", Version::new(1, 0), card.clone());
    let o = e.create_by_name("App", "Card", None).unwrap();
    assert_eq!(e.read_by_name(o, "target"), Value::Null);
    assert_eq!(e.read_by_name(o, "note"), Value::string(""));
    e.write_by_name(o, "note", Value::Int(4)).unwrap();
    assert_eq!(e.read_by_name(o, "note"), Value::string("4"));
    // Native handles still work on it.
    e.set(o, Rectangle::RADIUS, 2.0).unwrap();
    assert_eq!(
        e.invoke_by_name(o, "grow", &[Value::Int(1)]),
        Ok(Value::Real(3.0))
    );
    // And signals declared natively.
    let hit = Rc::new(Cell::new(false));
    let h = hit.clone();
    e.connect_by_name(o, "noteChanged", move |_, _| h.set(true))
        .unwrap();
    e.write_by_name(o, "note", Value::string("x")).unwrap();
    assert!(hit.get());
}

#[test]
fn destroyed_objects_read_as_null() {
    let e = engine();
    let card = qml_type();
    let holder = e.create(card, None).unwrap();
    let target = rect(&e);
    e.set(target, Item::WIDTH, 5.0).unwrap();
    e.write_by_name(holder, "target", Value::Object(target))
        .unwrap();
    assert_eq!(e.read_by_name(holder, "target"), Value::Object(target));

    // A binding that reads through the reference.
    e.bind(holder, Item::WIDTH.index, move |e| {
        match e.read_by_name(holder, "target") {
            Value::Object(t) => e.read(t, Item::WIDTH.index),
            _ => Value::Real(-1.0),
        }
    })
    .unwrap();
    assert_eq!(e.get(holder, Item::WIDTH), Ok(5.0));

    // destroy() is deferred.
    assert!(e.destroy(target));
    assert!(e.is_alive(target));
    assert_eq!(e.process_deferred_deletes(), 1);
    assert!(!e.is_alive(target));

    assert_eq!(e.read(target, Item::WIDTH.index), Value::Null);
    assert_eq!(e.read_by_name(target, "width"), Value::Null);
    assert_eq!(e.read_by_name(holder, "target"), Value::Null);
    assert!(matches!(
        e.get(target, Item::WIDTH),
        Err(EngineError::DeadObject(_))
    ));
    assert!(matches!(
        e.write_by_name(target, "width", Value::Int(1)),
        Err(EngineError::DeadObject(_))
    ));
    // The binding re-ran when the source's nodes went away, and now sees
    // `null` through the reference.
    assert_eq!(e.get(holder, Item::WIDTH), Ok(-1.0));
    assert!(!e.destroy(target));
    assert!(!e.destroy_now(target));
}

#[test]
fn binding_on_a_destroyed_source_reruns() {
    let e = engine();
    let src = rect(&e);
    let dst = rect(&e);
    e.set(src, Item::WIDTH, 5.0).unwrap();
    e.bind(dst, Item::WIDTH.index, move |e| {
        let v = e.read(src, Item::WIDTH.index);
        if v.is_nullish() { Value::Real(-1.0) } else { v }
    })
    .unwrap();
    assert_eq!(e.get(dst, Item::WIDTH), Ok(5.0));
    e.destroy_now(src);
    assert_eq!(e.get(dst, Item::WIDTH), Ok(-1.0));
}

#[test]
fn ownership_tree_and_deferred_delete() {
    let e = engine();
    let root = rect(&e);
    let child = e.create_by_name("Test", "Item", Some(root)).unwrap();
    let grandchild = e.create_by_name("Test", "Item", Some(child)).unwrap();
    let other = rect(&e);
    e.set_visual_parent(child, Some(other)).unwrap();
    assert_eq!(e.children(root), [child]);
    assert_eq!(e.owner(grandchild), Some(child));
    assert_eq!(e.visual_children(other), [child]);
    assert!(e.set_owner(root, Some(grandchild)).is_err(), "cycle");

    let fired = Rc::new(Cell::new(0));
    let f = fired.clone();
    e.connect_to(
        other,
        Item::WIDTH.index.into_signal(&e, other),
        grandchild,
        move |_, _| f.set(f.get() + 1),
    )
    .unwrap();
    let conns = e.connection_count();

    e.destroy(root);
    e.destroy(root);
    assert_eq!(e.process_deferred_deletes(), 1);
    for o in [root, child, grandchild] {
        assert!(!e.is_alive(o));
    }
    assert!(e.visual_children(other).is_empty());
    assert_eq!(
        e.connection_count(),
        conns - 1,
        "receiver's connection dropped"
    );
    e.set(other, Item::WIDTH, 1.0).unwrap();
    assert_eq!(fired.get(), 0);
}

/// Finds the change signal of a property (test helper).
trait IntoSignal {
    fn into_signal(
        self,
        e: &ljos_engine::Engine,
        obj: ljos_engine::ObjectId,
    ) -> ljos_engine::SignalIndex;
}

impl IntoSignal for PropIndex {
    fn into_signal(
        self,
        e: &ljos_engine::Engine,
        obj: ljos_engine::ObjectId,
    ) -> ljos_engine::SignalIndex {
        e.meta_of(obj)
            .unwrap()
            .property(self)
            .unwrap()
            .notify
            .unwrap()
    }
}

#[test]
fn completion_hooks_run_once() {
    let e = engine();
    let r = e
        .create_native_incomplete(Rectangle::default(), None)
        .unwrap();
    assert_eq!(e.with(r, |x: &Rectangle| x.completed_runs), Ok(0));
    e.complete(r).unwrap();
    e.complete(r).unwrap();
    assert_eq!(e.with(r, |x: &Rectangle| x.completed_runs), Ok(1));
}

#[test]
fn an_object_made_from_rust_is_complete() {
    let e = engine();
    let by_name = rect(&e);
    let native = e.create_native(Rectangle::default(), None).unwrap();
    for r in [by_name, native] {
        assert_eq!(e.with(r, |x: &Rectangle| x.completed_runs), Ok(1));
        e.complete(r).unwrap();
        assert_eq!(e.with(r, |x: &Rectangle| x.completed_runs), Ok(1));
    }
}

#[test]
fn attached_objects() {
    let e = engine();
    let r = rect(&e);
    let keys = e.lookup_type("Test", "Keys").unwrap();
    let a = e.attached_object(r, &keys).unwrap();
    assert_eq!(e.attached_object(r, &keys), Ok(a));
    assert_eq!(e.meta_of(a).unwrap().name, "KeysAttached");
    assert_eq!(e.owner(a), Some(r));
    let rect_meta = e.lookup_type("Test", "Rectangle").unwrap();
    assert!(matches!(
        e.attached_object(r, &rect_meta),
        Err(EngineError::NoAttachedType(_))
    ));
    e.destroy_now(r);
    assert!(!e.is_alive(a));
}

#[test]
fn contexts_resolve_ids_through_parents() {
    let e = engine();
    let outer = e.create_context(None);
    let inner = e.create_context(Some(outer));
    let a = rect(&e);
    let b = rect(&e);
    e.set_context_id(outer, "a", a);
    e.set_context_id(inner, "b", b);
    e.set_context_id(inner, "a2", a);
    assert_eq!(e.resolve_id(inner, "a"), Some(a));
    assert_eq!(e.resolve_id(inner, "b"), Some(b));
    assert_eq!(e.resolve_id(outer, "b"), None);
    assert_eq!(e.context_parent(inner), Some(outer));
    assert_eq!(e.context_parent(outer), Some(e.root_context()));
    e.set_context(a, inner).unwrap();
    assert_eq!(e.context_of(a), Some(inner));
    // Children start in their owner's context.
    let c = e.create_by_name("Test", "Item", Some(a)).unwrap();
    assert_eq!(e.context_of(c), Some(inner));
    e.destroy_now(a);
    assert_eq!(e.resolve_id(inner, "a"), None);
    assert_eq!(e.resolve_id(inner, "a2"), None);
}

#[test]
fn registry_versions_and_dump() {
    let e = engine();
    assert!(e.lookup_type("Test", "Keys").is_some());
    assert!(
        e.lookup_type_versioned("Test", Version::new(1, 0), "Keys")
            .is_none()
    );
    assert!(
        e.lookup_type_versioned("Test", Version::new(1, 1), "Keys")
            .is_some()
    );
    assert!(
        e.lookup_type_versioned("Test", Version::new(1, 0), "Rectangle")
            .is_some()
    );
    assert!(matches!(
        e.create_by_name("Test", "Keys", None),
        Err(EngineError::NotCreatable(_))
    ));
    assert!(e.create_by_name("QtQml", "QtObject", None).is_ok());

    let dump = e.type_dump();
    let test = dump.modules.iter().find(|m| m.uri == "Test").unwrap();
    assert_eq!(test.versions, ["1.0", "1.1"]);
    let r = test.types.iter().find(|t| t.name == "Rectangle").unwrap();
    assert_eq!(r.base.as_deref(), Some("Item"));
    assert_eq!(r.since, "1.0");
    let props: Vec<_> = r.properties.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(props, ["color", "radius", "border", "antialiasing"]);
    assert_eq!(r.properties[0].notify.as_deref(), Some("colorChanged"));
    assert_eq!(r.properties[2].ty, "Border");
    let signals: Vec<_> = r.signals.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(signals, ["clicked", "doubleClicked"]);
    assert_eq!(r.methods[0].return_type, "real");
    let keys = test.types.iter().find(|t| t.name == "Keys").unwrap();
    assert_eq!(keys.attached.as_deref(), Some("KeysAttached"));
    assert!(!keys.creatable);

    let json = dump.to_json();
    let back: ljos_engine::TypeDump = serde_json::from_str(&json).unwrap();
    assert_eq!(back, dump);
    assert!(json.contains("\"defaultProperty\": \"data\""), "{json}");
}

#[test]
fn a_read_of_properties_that_never_change_makes_no_nodes() {
    let e = engine();
    let r = rect(&e);
    let props = [Item::X, Item::Y, Item::WIDTH, Item::HEIGHT];
    let before = e.graph().len();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let fx = e
        .effect(move |e| {
            let sum: f64 = props.iter().map(|p| e.get(r, *p).unwrap()).sum();
            let shown = e.get(r, Item::VISIBLE).unwrap();
            s.borrow_mut().push((sum, shown));
        })
        .unwrap();
    // The effect alone: a property read and never changed has no node.
    assert_eq!(e.graph().len(), before + 1);
    // Each way a field changes still reaches the reader, and gives the
    // property its node then.
    e.set(r, Item::X, 1.0).unwrap();
    assert_eq!(seen.borrow().last(), Some(&(1.0, false)));
    e.write(r, Item::Y.index, Value::Real(2.0), WriteKind::Animation)
        .unwrap();
    assert_eq!(seen.borrow().last(), Some(&(3.0, false)));
    e.with_mut::<Item, _>(r, |i| i.width = 4.0).unwrap();
    e.notify(r, Item::WIDTH.index).unwrap();
    assert_eq!(seen.borrow().last(), Some(&(7.0, false)));
    e.bind(r, Item::HEIGHT.index, |_| Value::Real(8.0)).unwrap();
    assert_eq!(seen.borrow().last(), Some(&(15.0, false)));
    e.write_derived(r, Item::VISIBLE.index, Value::Bool(true))
        .unwrap();
    assert_eq!(seen.borrow().last(), Some(&(15.0, true)));
    assert_eq!(seen.borrow().len(), 6, "one run per change");
    assert_eq!(e.graph().len(), before + 1 + props.len() + 1);
    e.graph().check_invariants().unwrap();
    e.graph().dispose(fx).unwrap();
    e.set(r, Item::X, 3.0).unwrap();
    assert_eq!(seen.borrow().len(), 6, "no reader, no run");
}

#[test]
fn a_property_read_without_a_node_is_followed_only_while_it_is_read() {
    let e = engine();
    let r = rect(&e);
    let use_x = Rc::new(Cell::new(true));
    let runs = Rc::new(Cell::new(0));
    let (u, n) = (use_x.clone(), runs.clone());
    let _fx = e
        .effect(move |e| {
            n.set(n.get() + 1);
            e.get(r, Item::VISIBLE).unwrap();
            if u.get() {
                e.get(r, Item::X).unwrap();
            } else {
                e.get(r, Item::Y).unwrap();
            }
        })
        .unwrap();
    use_x.set(false);
    e.set(r, Item::VISIBLE, true).unwrap();
    assert_eq!(runs.get(), 2);
    // The last run read `y`, not `x`.
    e.set(r, Item::X, 1.0).unwrap();
    assert_eq!(runs.get(), 2, "a branch no longer taken");
    e.set(r, Item::Y, 1.0).unwrap();
    assert_eq!(runs.get(), 3);
    // Reads of many runs of a reader that keeps running stay bounded.
    for i in 0..1000 {
        e.set(r, Item::Y, f64::from(i) + 2.0).unwrap();
    }
    assert_eq!(runs.get(), 1003);
    assert_eq!(e.observers_of(r, Item::X.index), []);
    assert_eq!(e.observers_of(r, Item::Y.index).len(), 1);
    e.graph().check_invariants().unwrap();
}

#[test]
fn engine_handles_reach_the_engine_inside_closures() {
    let e = engine();
    let r = rect(&e);
    let runs = Rc::new(Cell::new(0));
    let ru = runs.clone();
    let fx = e
        .effect(move |e| {
            let _ = e.read(r, Item::X.index);
            ru.set(ru.get() + 1);
        })
        .unwrap();
    assert_eq!(runs.get(), 1);
    e.write(r, Item::X.index, Value::Int(3), WriteKind::Imperative)
        .unwrap();
    assert_eq!(runs.get(), 2);
    e.graph().dispose(fx).unwrap();
    e.set(r, Item::X, 4.0).unwrap();
    assert_eq!(runs.get(), 2);
}

#[test]
fn an_effect_in_an_object_dies_with_its_new_owner() {
    let e = engine();
    let first = rect(&e);
    let second = rect(&e);
    let obj = e.create_by_name("Test", "Rectangle", Some(first)).unwrap();
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    e.effect_in(obj, move |e| {
        e.read(second, Item::WIDTH.index);
        r.set(r.get() + 1);
    })
    .unwrap();
    assert_eq!(runs.get(), 1);

    e.set_owner(obj, Some(second)).unwrap();
    assert_eq!(
        e.graph().scope_parent(e.object_scope(obj).unwrap()),
        e.object_scope(second)
    );
    e.destroy_now(first);
    assert!(e.is_alive(obj), "owned by `second` now");
    e.set(second, Item::WIDTH, 5.0).unwrap();
    assert_eq!(runs.get(), 2, "the effect moved with the object");

    e.set_owner(obj, None).unwrap();
    assert_eq!(
        e.graph().scope_parent(e.object_scope(obj).unwrap()),
        Some(e.graph().root())
    );
    e.destroy_now(obj);
    e.set(second, Item::WIDTH, 6.0).unwrap();
    assert_eq!(runs.get(), 2, "and died with it");
    e.graph().check_invariants().unwrap();
}

#[test]
fn asking_whether_a_property_was_given_follows_it() {
    let e = engine();
    let r = rect(&e);
    let before = e.graph().len();
    assert_eq!(
        e.provenance_tracked(r, Item::WIDTH.index),
        Some(Provenance::Default)
    );
    assert_eq!(e.graph().len(), before, "asked outside a reader: no node");

    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    e.effect_in(r, move |e| {
        s.borrow_mut()
            .push(e.provenance_tracked(r, Item::WIDTH.index));
    })
    .unwrap();
    // The value the property had anyway, given on purpose.
    e.set(r, Item::WIDTH, 0.0).unwrap();
    assert_eq!(
        *seen.borrow(),
        [Some(Provenance::Default), Some(Provenance::Assigned)]
    );
    e.set(r, Item::WIDTH, 0.0).unwrap();
    assert_eq!(seen.borrow().len(), 2, "given again is no news");
    e.bind(r, Item::WIDTH.index, |_| Value::Real(0.0)).unwrap();
    assert_eq!(
        seen.borrow().len(),
        2,
        "given in another way is still given"
    );
    assert_eq!(
        e.provenance_tracked(r, Item::WIDTH.index),
        Some(Provenance::Bound)
    );
    e.graph().check_invariants().unwrap();
}
