//! An application's QML compiled ahead of time against the type dump
//! file its own test writes: the build step reads the file and runs none
//! of the application's code, and a member the native types do not have
//! fails the build.

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "Example",
    name = "Counter",
    enums = counter_enums,
    attached = Tally
)]
pub struct Counter {
    #[qml(notify)]
    pub count: i32,
    #[qml(notify)]
    pub label: Atom,
    #[qml(group)]
    pub limits: Limits,
}

/// `Counter`'s `limits` group: a type of the application's that no module
/// exports by name.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "CounterLimits", uncreatable)]
pub struct Limits {
    #[qml(notify)]
    pub max: i32,
}

/// What `Counter.` attaches to an object.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "CounterTally", uncreatable)]
pub struct Tally {
    #[qml(notify)]
    pub weight: i32,
}

#[qml_impl]
impl Tally {
    #[qml(signal)]
    fn tallied();
}

fn counter_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![("Step", vec![("One", 1), ("Ten", 10)])]
}

#[qml_impl]
impl Counter {
    #[qml(signal)]
    fn bumped(by: i32);

    #[qml(invokable)]
    fn increment(cx: &Cx) -> Result<(), EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)
    }
}

/// A native singleton: a file reads its members and makes none.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Settings")]
#[qml_singleton]
pub struct Settings {
    #[qml(notify)]
    pub start: i32,
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
    engine.register_singleton::<Settings>("Example", Version::new(1, 0));
}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    Counter {
        id: counter
        count: 41 + Settings.start
        label: "n"
        limits.max: 100
        onBumped: (by) => counter.count += by
        onCountChanged: root.changes++
    }
    id: root
    property int changes: 0
    property int seen: counter.count + 1
    property int step: Counter.Ten
    Counter.weight: 2
    Counter.onTallied: changes++
}
"#;

/// A QML directory with `Main.qml` holding `main`, and the type dump
/// file written beside it as the application's test writes it.
fn project(main: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("qml")).unwrap();
    std::fs::write(dir.path().join("qml/Main.qml"), main).unwrap();
    let dump = ljos::types::dump(register);
    std::fs::write(dir.path().join("ljostypes.json"), dump.to_json()).unwrap();
    dir
}

/// The build's error, where a build that succeeds is the failure.
fn build_error(dir: &tempfile::TempDir) -> String {
    // Not `expect_err`, which would print the whole pack.
    match compile(dir) {
        Ok(_) => panic!("the build succeeded"),
        Err(e) => e,
    }
}

fn compile(dir: &tempfile::TempDir) -> Result<Vec<u8>, String> {
    let pack = dir.path().join("qml.pack");
    ljos::build::compile_dir_with_types(
        &dir.path().join("qml"),
        &dir.path().join("ljostypes.json"),
        &pack,
    )
    .map_err(|e| e.to_string())?;
    Ok(std::fs::read(pack).unwrap())
}

#[test]
fn the_dump_holds_the_applications_types_and_not_the_toolkits() {
    let dump = ljos::types::dump(register);
    let names: Vec<(&str, &str)> = dump
        .modules
        .iter()
        .flat_map(|m| m.types.iter().map(|t| (m.uri.as_str(), t.name.as_str())))
        .collect();
    assert_eq!(names, [("Example", "Counter"), ("Example", "Settings")]);
    let settings = &dump.modules[0].types[1];
    assert!(settings.singleton);
    let text = dump.to_json();
    assert_eq!(ljos::types::TypeDump::from_json(&text).unwrap(), dump);
}

#[test]
fn qml_compiled_against_the_dump_runs_with_the_native_types() {
    let dir = project(MAIN);
    let pack = compile(&dir).unwrap();
    let (engine, root) = ljos::App::new("Ahead of time")
        .register(register)
        .source(ljos::MapSource::from_pack(&pack).unwrap())
        .root("Main")
        .build()
        .unwrap();
    assert_eq!(engine.read_by_name(root, "seen").to_number(), 42.0);
    assert_eq!(engine.read_by_name(root, "step").to_number(), 10.0);
}

#[test]
fn a_property_the_native_type_lacks_fails_the_build() {
    let dir = project(&MAIN.replace("label: \"n\"", "lable: \"n\""));
    let err = build_error(&dir);
    assert!(err.contains("lable"), "{err}");
}

#[test]
fn a_signal_the_native_type_lacks_fails_the_build() {
    let dir = project(&MAIN.replace("onBumped:", "onBumpd:"));
    let err = build_error(&dir);
    assert!(err.contains("onBumpd"), "{err}");
}

#[test]
fn an_enum_key_the_native_type_lacks_fails_the_build() {
    let dir = project(&MAIN.replace("Counter.Ten", "Counter.Eleven"));
    let err = build_error(&dir);
    assert!(err.contains("Eleven"), "{err}");
}

#[test]
fn a_member_of_a_singleton_the_native_type_lacks_fails_the_build() {
    let dir = project(&MAIN.replace("Settings.start", "Settings.strat"));
    let err = build_error(&dir);
    assert!(err.contains("Settings.strat"), "{err}");
}

#[test]
fn a_member_of_a_native_group_the_type_lacks_fails_the_build() {
    let dir = project(&MAIN.replace("limits.max:", "limits.mx:"));
    let err = build_error(&dir);
    assert!(err.contains("`limits.mx`"), "{err}");
}

#[test]
fn a_member_of_a_native_attached_type_it_lacks_fails_the_build() {
    let dir = project(&MAIN.replace("Counter.weight:", "Counter.wieght:"));
    let err = build_error(&dir);
    assert!(err.contains("`Counter.wieght`"), "{err}");
    let dir = project(&MAIN.replace("Counter.onTallied:", "Counter.onTalied:"));
    let err = build_error(&dir);
    assert!(err.contains("`Counter.onTalied`"), "{err}");
}

#[test]
fn the_live_engine_build_checks_groups_and_attached_types_too() {
    let dir = project(&MAIN.replace("limits.max:", "limits.mx:"));
    let err = ljos::build::compile_dir(
        &dir.path().join("qml"),
        register,
        &dir.path().join("qml.pack"),
    )
    .expect_err("the build succeeded")
    .to_string();
    assert!(err.contains("`limits.mx`"), "{err}");
}

#[test]
fn a_component_built_on_a_native_type_is_checked_as_the_type() {
    let dir = project(MAIN);
    std::fs::write(
        dir.path().join("qml/BigCounter.qml"),
        "import Example\nCounter { property int big: 100 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("qml/Use.qml"),
        "import QtQuick\nItem { BigCounter { big: 1; count: 2; lable: \"x\" } }\n",
    )
    .unwrap();
    let err = build_error(&dir);
    assert!(
        err.contains("`lable` is not a property of `BigCounter`"),
        "{err}"
    );
    assert!(!err.contains("`big`") && !err.contains("`count`"), "{err}");
}

#[test]
fn the_build_against_a_live_engine_checks_the_same() {
    let dir = project(&MAIN.replace("label: \"n\"", "lable: \"n\""));
    let err = ljos::build::compile_dir(
        &dir.path().join("qml"),
        register,
        &dir.path().join("qml.pack"),
    )
    .expect_err("the build succeeded")
    .to_string();
    assert!(err.contains("lable"), "{err}");
}

#[test]
fn a_missing_dump_says_how_to_write_it() {
    let dir = project(MAIN);
    std::fs::remove_file(dir.path().join("ljostypes.json")).unwrap();
    let err = build_error(&dir);
    assert!(err.contains("LJOS_TYPES=write"), "{err}");
}

#[test]
fn check_file_accepts_a_current_dump_and_names_a_stale_one() {
    let dir = project(MAIN);
    let file = dir.path().join("ljostypes.json");
    ljos::types::check_file(&file, register).unwrap();
    // The application has grown a type the file does not describe.
    let err = ljos::types::check_file(&file, |e: &Engine| {
        register(e);
        e.register_module(ljos::module!("Example.More" 1.0 => [Counter]));
    })
    .unwrap_err();
    assert!(
        matches!(err, ljos::types::TypesError::Stale { .. }),
        "{err:?}"
    );
    assert!(err.to_string().contains("LJOS_TYPES=write"), "{err}");
}
