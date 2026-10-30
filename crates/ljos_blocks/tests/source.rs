//! Blocks written in QML and a project's registry (the `source` feature).
#![cfg(feature = "source")]

use std::path::Path;

use ljos_blocks::{
    ActionEntry, BlockContract, BlockRegistry, EventEntry, Origin, Param, Problem, SlotEntry,
    StateEntry, Version, read_qmldir,
};

const PLAYER: &str = r#"import QtQuick

Item {
    id: root
    readonly property bool playing: false
    readonly property list<string> queue: []
    property url source
    property var options
    property list<Contribution> toolbar
    signal finished(real position)
    signal stalled(why)
    function play() { }
    function seek(to: real, exact): bool { return true }
    width: 10
    Rectangle { property int ignored: 1 }
}
"#;

fn write(dir: &Path, path: &str, text: &str) {
    let path = dir.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn prop(name: &str, ty: &str) -> StateEntry {
    StateEntry {
        name: name.into(),
        ty: ty.into(),
        notify: Some(format!("{name}Changed")),
    }
}

fn param(name: &str, ty: Option<&str>) -> Param {
    Param {
        name: name.into(),
        ty: ty.map(Into::into),
    }
}

#[test]
fn a_qml_blocks_contract_is_its_root_objects_members() {
    let read = read_qmldir("module App.Media\n# block Player 1.0 Player.qml Media \"Plays\"\n");
    let decl = &read.blocks[0].1;
    let contract =
        ljos_blocks::qml_contract("App.Media", decl, "media/Player.qml", PLAYER).unwrap();
    assert_eq!(
        contract,
        BlockContract {
            module: "App.Media".into(),
            name: "Player".into(),
            version: Version { major: 1, minor: 0 },
            origin: Origin::Qml {
                file: "media/Player.qml".into()
            },
            category: Some("Media".into()),
            summary: Some("Plays".into()),
            state: vec![prop("playing", "bool"), prop("queue", "list<string>")],
            inputs: vec![prop("source", "url"), prop("options", "var")],
            actions: vec![
                ActionEntry {
                    name: "play".into(),
                    params: vec![],
                    returns: None,
                },
                ActionEntry {
                    name: "seek".into(),
                    params: vec![param("to", Some("real")), param("exact", None)],
                    returns: Some("bool".into()),
                },
            ],
            events: vec![
                EventEntry {
                    name: "finished".into(),
                    params: vec![param("position", Some("real"))],
                },
                EventEntry {
                    name: "stalled".into(),
                    params: vec![param("why", None)],
                },
            ],
            slots: vec![SlotEntry {
                name: "toolbar".into(),
                accepts: "Contribution".into(),
            }],
        }
    );
}

#[test]
fn a_file_that_does_not_parse_has_no_contract() {
    let read = read_qmldir("module M\n# block Player 1.0 Player.qml\n");
    let e = ljos_blocks::qml_contract("M", &read.blocks[0].1, "Player.qml", "Item { property }")
        .unwrap_err();
    assert!(e.starts_with("Player.qml:1:"), "{e}");
}

#[test]
fn a_project_lists_its_qml_and_rust_blocks_and_what_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "qml/App/Media/qmldir",
        "module App.Media\n\
         Player 1.0 Player.qml\n\
         # block Player 1.0 Player.qml Media \"Plays one source\"\n\
         # block Missing 1.0 Missing.qml\n\
         # block bad 1.0 Bad.qml\n\
         Plain 1.0 Plain.qml\n",
    );
    write(root, "qml/App/Media/Player.qml", PLAYER);
    write(root, "qml/App/Media/Plain.qml", "import QtQuick\nItem {}\n");
    write(
        root,
        "ljostypes.json",
        include_str!("fixtures/ljostypes.json"),
    );
    // Build output is not the project's.
    write(
        root,
        "target/qml/qmldir",
        "module Copy\n# block Player 1.0 Player.qml\n",
    );

    let registry = BlockRegistry::from_project(root).unwrap();
    let names: Vec<(&str, &str, &Origin)> = registry
        .blocks()
        .map(|b| (b.module.as_str(), b.name.as_str(), &b.origin))
        .collect();
    let qml = Origin::Qml {
        file: "qml/App/Media/Player.qml".into(),
    };
    assert_eq!(
        names,
        [
            ("App.Media", "Clock", &Origin::Rust),
            ("App.Media", "Player", &Origin::Rust),
        ]
    );
    // The dump's Player came first, so the QML one is a second declaration.
    let problems: Vec<&Problem> = registry.problems().iter().collect();
    let messages: Vec<String> = problems
        .iter()
        .map(|p| format!("{}:{}: {}", p.file, p.line.unwrap_or(0), p.message))
        .collect();
    assert_eq!(
        messages[0],
        "qml/App/Media/qmldir:5: `bad` is not a type name; a block is \
         `# block <Name> <major.minor> <File.qml> [<Category>] [\"<summary>\"]`"
    );
    assert_eq!(
        messages[1..],
        [
            "qml/App/Media/qmldir:3: block App.Media.Player is declared twice",
            "qml/App/Media/qmldir:4: block Missing: qml/App/Media/Missing.qml does not exist",
        ]
    );

    // Without the dump, the QML block is the one listed.
    std::fs::remove_file(root.join("ljostypes.json")).unwrap();
    let registry = BlockRegistry::from_project(root).unwrap();
    let player = registry.get("App.Media", "Player").unwrap();
    assert_eq!(player.origin, qml);
    assert_eq!(player.summary.as_deref(), Some("Plays one source"));
    assert!(registry.get("App.Media", "Plain").is_none());
}

#[test]
fn a_block_file_outside_the_project_is_not_read() {
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "Secret.qml", PLAYER);
    let dir = tempfile::tempdir().unwrap();
    let up = outside.path().join("Secret.qml");
    write(
        dir.path(),
        "qmldir",
        &format!("module M\n# block Secret 1.0 {}\n", up.display()),
    );
    let registry = BlockRegistry::from_project(dir.path()).unwrap();
    assert!(registry.is_empty());
    assert!(
        registry.problems()[0]
            .message
            .ends_with("is outside the project"),
        "{:?}",
        registry.problems()
    );
}

#[test]
fn a_block_line_in_a_qmldir_without_a_module_is_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "qmldir", "# block Player 1.0 Player.qml\n");
    let registry = BlockRegistry::from_project(dir.path()).unwrap();
    assert!(registry.is_empty());
    assert_eq!(registry.problems().len(), 1);
    assert!(
        registry.problems()[0]
            .message
            .contains("without a `module` line")
    );
}

#[test]
fn add_qml_block_reads_one_file() {
    let read = read_qmldir("module M\n# block Player 1.0 Player.qml\n");
    let mut registry = BlockRegistry::new();
    registry.add_qml_block("M", &read.blocks[0].1, "Player.qml", PLAYER);
    registry.add_qml_block("M", &read.blocks[0].1, "Player.qml", "Item {");
    assert_eq!(registry.blocks().count(), 1);
    assert_eq!(registry.problems().len(), 1);
}
