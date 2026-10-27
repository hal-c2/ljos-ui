//! The application starts from its embedded pack, its QML using the
//! native type it was compiled against.

#[test]
fn the_embedded_pack_makes_the_root_with_its_native_type() {
    let (engine, root) = ljos_facade_aot::app().build().unwrap();
    assert_eq!(engine.read_by_name(root, "seen").to_number(), 42.0);
}

#[test]
fn its_bindings_and_handlers_run_the_code_its_build_compiled() {
    let (engine, root) = ljos_facade_aot::app().build().unwrap();
    let counter = engine.children(root)[0];
    engine.invoke_by_name(counter, "increment", &[]).unwrap();
    assert_eq!(engine.read_by_name(root, "seen").to_number(), 43.0);
    assert_eq!(engine.read_by_name(root, "bumps").to_number(), 1.0);
    let url = engine.document_url(root).unwrap();
    assert_eq!(engine.aot_file(&url), Some(true));
    let stats = engine.aot_stats();
    // At least `seen` again, and the handler of `bumped`.
    assert!(stats.runs >= 2, "{stats:?}");
    assert_eq!(stats.mismatches, 0, "{stats:?}");
}

#[test]
fn with_compiled_code_off_the_same_runs_dynamically() {
    let (engine, root) = ljos_facade_aot::app().build().unwrap();
    engine.set_aot(false);
    let counter = engine.children(root)[0];
    engine.invoke_by_name(counter, "increment", &[]).unwrap();
    assert_eq!(engine.read_by_name(root, "seen").to_number(), 43.0);
    assert_eq!(engine.read_by_name(root, "bumps").to_number(), 1.0);
    assert_eq!(engine.aot_stats().runs, 0);
}
