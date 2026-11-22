//! The application starts without a phone, and without a scenario shows
//! the window built in.

#[test]
fn without_a_scenario_the_window_built_in_shows() {
    let (engine, root) = ljos_example_android_scenarios::app()
        .build()
        .expect("the application starts");
    assert_eq!(
        engine
            .read_by_name(root, "objectName")
            .to_js_string()
            .as_str(),
        "waiting"
    );
}

#[test]
fn the_scenario_files_are_beside_the_token_file() {
    let dir = ljos_example_android_scenarios::scenario_dir();
    assert!(dir.ends_with("qml"));
    assert_eq!(
        ljos_example_android_scenarios::fault_file().parent(),
        dir.parent()
    );
}
