//! The application starts without a phone: what the activity runs is
//! the same builder.

#[test]
fn the_application_makes_its_window() {
    let (engine, root) = ljos_example_android_hello::app()
        .build()
        .expect("the application starts");
    assert_eq!(engine.read_by_name(root, "taps").as_number(), Some(0.0));
}
