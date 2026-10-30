//! Blocks read from a type dump, with contracts written out by hand.

use ljos_blocks::{
    ActionEntry, BlockContract, BlockRegistry, BlocksError, EventEntry, Origin, Param, SlotEntry,
    StateEntry, Version,
};

const DUMP: &str = include_str!("fixtures/ljostypes.json");

fn state(name: &str, ty: &str, notify: Option<&str>) -> StateEntry {
    StateEntry {
        name: name.into(),
        ty: ty.into(),
        notify: notify.map(Into::into),
    }
}

fn param(name: &str, ty: &str) -> Param {
    Param {
        name: name.into(),
        ty: Some(ty.into()),
    }
}

#[test]
fn a_rust_blocks_contract_is_its_own_members() {
    let registry = BlockRegistry::from_type_dump(DUMP).unwrap();
    let player = registry.get("App.Media", "Player").unwrap();
    assert_eq!(
        *player,
        BlockContract {
            module: "App.Media".into(),
            name: "Player".into(),
            version: Version { major: 1, minor: 2 },
            origin: Origin::Rust,
            category: Some("Media".into()),
            summary: Some("Plays one source".into()),
            state: vec![
                state("playing", "bool", Some("playingChanged")),
                state("codec", "string", None),
            ],
            inputs: vec![state("source", "url", Some("sourceChanged"))],
            actions: vec![
                ActionEntry {
                    name: "play".into(),
                    params: vec![],
                    returns: None,
                },
                ActionEntry {
                    name: "seek".into(),
                    params: vec![param("to", "real")],
                    returns: Some("bool".into()),
                },
            ],
            events: vec![EventEntry {
                name: "finished".into(),
                params: vec![param("position", "real")],
            }],
            slots: vec![SlotEntry {
                name: "toolbar".into(),
                accepts: "Contribution".into(),
            }],
        }
    );
    assert!(registry.problems().is_empty());
}

#[test]
fn only_types_marked_block_are_listed_in_name_order() {
    let registry = BlockRegistry::from_type_dump(DUMP).unwrap();
    let names: Vec<&str> = registry.blocks().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Clock", "Player"]);
    let clock = registry.get("App.Media", "Clock").unwrap();
    assert_eq!((&clock.category, &clock.summary), (&None, &None));
    assert!(registry.get("App.Media", "Waveform").is_none());
}

#[test]
fn the_json_is_versioned_and_reads_back() {
    let registry = BlockRegistry::from_type_dump(DUMP).unwrap();
    let text = registry.to_json();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["format"], 1);
    assert_eq!(value["errors"], serde_json::json!([]));
    assert_eq!(
        value["blocks"][1]["origin"],
        serde_json::json!({"kind": "rust"})
    );
    assert_eq!(value["blocks"][1]["version"], "1.2");
    assert_eq!(value["blocks"][1]["state"][0]["type"], "bool");
    // A block that says nothing for a palette writes no empty fields.
    assert!(value["blocks"][0].get("category").is_none());
    assert_eq!(BlockRegistry::from_json(&text).unwrap(), registry);
}

#[test]
fn a_module_filter_keeps_that_modules_blocks() {
    let registry = BlockRegistry::from_type_dump(DUMP).unwrap();
    assert_eq!(registry.of_module("App.Media").blocks().count(), 2);
    assert!(registry.of_module("App").is_empty());
}

#[test]
fn a_dump_of_another_format_or_no_dump_is_refused() {
    let e = BlockRegistry::from_type_dump(r#"{"format": 1, "modules": []}"#).unwrap_err();
    assert!(matches!(e, BlocksError::Format { found: 1 }), "{e:?}");
    let e = BlockRegistry::from_type_dump("[]").unwrap_err();
    assert!(matches!(e, BlocksError::Json(_)), "{e:?}");
}

#[test]
fn a_block_declared_twice_is_a_problem_and_the_first_stays() {
    let mut registry = BlockRegistry::from_type_dump(DUMP).unwrap();
    registry
        .add_type_dump(DUMP, "other/ljostypes.json")
        .unwrap();
    assert_eq!(registry.blocks().count(), 2);
    let messages: Vec<String> = registry
        .problems()
        .iter()
        .map(|p| format!("{}: {}", p.file, p.message))
        .collect();
    assert_eq!(
        messages,
        [
            "other/ljostypes.json: block App.Media.Player is declared twice",
            "other/ljostypes.json: block App.Media.Clock is declared twice",
        ]
    );
}
