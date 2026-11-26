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

#[test]
fn the_badge_is_set_as_the_runner_says() {
    use ljos_example_android_scenarios::{BADGE, badge_at_start};

    let files = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (said, unsaid) = (files.join("badge-said"), files.join("badge-unsaid"));
    std::fs::create_dir_all(&said).expect("a directory");
    std::fs::write(said.join(BADGE), "3\n").expect("the count");

    let (engine, _root) = ljos_example_android_scenarios::app()
        .build()
        .expect("the application starts");
    let platform = ljos::prelude::Platform::of(&engine);
    badge_at_start(&engine, &unsaid);
    assert_eq!(platform.badge_number(), 0);
    badge_at_start(&engine, &said);
    assert_eq!(platform.badge_number(), 3);
}

#[test]
fn a_secret_goes_through_the_store_as_the_runner_reaches_it() {
    use std::time::Duration;

    use ljos_example_android_scenarios::keep_secrets_in;
    use ljos_secrets::{MemorySecrets, SecretStore};

    assert!(keep_secrets_in(SecretStore::new(MemorySecrets::new())));
    let (engine, root) = ljos_example_android_scenarios::app()
        .build()
        .expect("the application starts");
    let platform = ljos::prelude::Platform::of(&engine);
    // As the phone's runner does: named in the root's scope by the
    // inspector's scripts, and read again while the store answers on its
    // thread.
    let call = |source: &str| {
        engine
            .eval_js_in(
                Some(root),
                source,
                Duration::from_secs(2),
                [4, 16, 64, 64, 1024],
            )
            .expect("the script runs")
    };
    let answer = |source: &str| {
        let made = call(&format!("scenarioSecrets.{source}"));
        assert!(!made.contains("\"error\""), "{made}");
        assert_eq!(
            call("scenarioSecrets.busy"),
            r#"{"value":true,"truncated":false}"#,
            "{source} answers later"
        );
        platform.finish(&engine);
        call(
            "[scenarioSecrets.busy, scenarioSecrets.found, scenarioSecrets.secret, \
             scenarioSecrets.error]",
        )
    };
    assert_eq!(
        answer(r#"keep("accounts", "token", "s3cret")"#),
        r#"{"value":[false,true,"",""],"truncated":false}"#
    );
    assert_eq!(
        answer(r#"read("accounts", "token")"#),
        r#"{"value":[false,true,"s3cret",""],"truncated":false}"#
    );
    assert_eq!(
        answer(r#"remove("accounts", "token")"#),
        r#"{"value":[false,true,"",""],"truncated":false}"#
    );
    assert_eq!(
        answer(r#"read("accounts", "token")"#),
        r#"{"value":[false,false,"",""],"truncated":false}"#
    );
}
