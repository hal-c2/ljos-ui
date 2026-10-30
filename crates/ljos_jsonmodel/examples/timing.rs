//! How long a 10 000-row file takes to become rows, and how long a save
//! that changes one of them takes to say so. Run it optimised:
//!
//! ```text
//! cargo run --release -p ljos_jsonmodel --example timing
//! ```
//!
//! The clock runs from asking for the file to the model being Ready, which
//! is the read, the parse, the comparison and every listener of the change.

use std::time::{Duration, Instant};

use ljos::prelude::{Engine, ObjectId, Platform};
use ljos_compiler::{Project, compile_file};

const ROWS: usize = 10_000;
const RUNS: usize = 7;

fn document(changed: Option<usize>) -> String {
    let mut text = String::from("[");
    for i in 0..ROWS {
        if i > 0 {
            text.push(',');
        }
        let name = if changed == Some(i) { "saved" } else { "row" };
        text.push_str(&format!(
            r#"{{"id":{i},"name":"{name} {i}","price":{i}.5,"stocked":true,"note":null,"tag":"t{}"}}"#,
            i % 7
        ));
    }
    text.push(']');
    text
}

fn load(url: &str) -> (Engine, ObjectId) {
    let e = ljos::engine();
    ljos_jsonmodel::register(&e);
    let qml = format!(
        r#"
import QtQuick
import Ljos.Models
Item {{
    id: root
    property int changes: 0
    property int resets: 0
    property alias m: model
    JsonListModel {{
        id: model
        onDataChanged: root.changes += 1
        onModelReset: root.resets += 1
        source: "{url}"
    }}
    ListView {{ model: model; width: 100; height: 100; delegate: Item {{ height: 10 }} }}
}}
"#
    );
    let mut project = Project::new(ljos::dev::EngineOracle::from_engine(&e));
    project.add("Main", qml.as_str());
    let compiled = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!compiled.has_errors(), "{:?}", compiled.diagnostics);
    let mut source = ljos::MapSource::new();
    source.insert("Main", compiled.file);
    e.set_component_source(source);
    let root = e
        .create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"));
    (e, root)
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn main() -> std::io::Result<()> {
    let dir = std::env::temp_dir().join(format!("ljos-json-timing-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("rows.json");
    let url = format!("file://{}", path.display());
    let whole = document(None);
    let one_differs = document(Some(ROWS / 2));
    println!("{ROWS} rows, {} bytes", whole.len());

    let (mut loads, mut saves) = (Vec::new(), Vec::new());
    for _ in 0..RUNS {
        std::fs::write(&path, &whole)?;
        let start = Instant::now();
        let (e, root) = load(&url);
        Platform::of(&e).finish(&e);
        loads.push(start.elapsed());
        let m = e.read_by_name(root, "m").as_object().expect("the model");
        assert_eq!(e.read_by_name(m, "count").to_number() as usize, ROWS);

        std::fs::write(&path, &one_differs)?;
        let start = Instant::now();
        e.invoke_by_name(m, "reload", &[]).expect("reload");
        Platform::of(&e).finish(&e);
        saves.push(start.elapsed());
        assert_eq!(e.read_by_name(root, "changes").to_number() as i64, 1);
        assert_eq!(e.read_by_name(root, "resets").to_number() as i64, 0);
    }
    println!(
        "load (with engine and QML compile): median {:?}, best {:?}",
        median(loads.clone()),
        loads.iter().min().expect("runs")
    );
    println!(
        "save of one changed row: median {:?}, best {:?}",
        median(saves.clone()),
        saves.iter().min().expect("runs")
    );
    std::fs::remove_dir_all(&dir)
}
