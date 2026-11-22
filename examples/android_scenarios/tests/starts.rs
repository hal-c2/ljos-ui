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

#[test]
fn local_files_are_read_only_where_the_runner_says_so() {
    use ljos_example_android_scenarios::{XHR_LOCAL_READ, allow_local_file_read};

    let files = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (said, unsaid) = (files.join("xhr-said"), files.join("xhr-unsaid"));
    std::fs::create_dir_all(&said).expect("a directory");
    std::fs::write(said.join(XHR_LOCAL_READ), "").expect("the mark");

    let (engine, _root) = ljos_example_android_scenarios::app()
        .build()
        .expect("the application starts");
    let platform = ljos::prelude::Platform::of(&engine);
    platform.set_local_file_read(false);
    allow_local_file_read(&engine, &unsaid);
    assert!(!platform.local_file_read());
    allow_local_file_read(&engine, &said);
    assert!(platform.local_file_read());
}
