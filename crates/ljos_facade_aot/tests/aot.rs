//! The application starts from its embedded pack, its QML using the
//! native type it was compiled against.

#[test]
fn the_embedded_pack_makes_the_root_with_its_native_type() {
    let (engine, root) = ljos_facade_aot::app().build().unwrap();
    assert_eq!(engine.read_by_name(root, "seen").to_number(), 42.0);
}
