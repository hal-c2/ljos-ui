//! Creation from IR, timed. Ignored by default:
//!
//! `cargo test --release -p ljos_engine --test qml_bench -- --ignored --nocapture`

mod support;

use std::time::Instant;

use support::engine;

/// A 1000-object file: rectangles with literals, a tier-0 binding, a JS
/// binding and a handler each.
fn thousand() -> String {
    let mut s =
        String::from("import QtQuick\nItem {\n  id: root\n  width: 800\n  property int tick: 0\n");
    for i in 0..333 {
        s.push_str(&format!(
            "  Rectangle {{ x: {i}; y: {i}; color: \"#336699\"; radius: 2\n    width: root.width\n    height: root.tick + {i}\n    onWidthChanged: root.tick++\n    Item {{ width: 4; height: 4 }}\n    Item {{ width: parent.width }}\n  }}\n"
        ));
    }
    s.push_str("}\n");
    s
}

#[test]
fn the_thousand_object_file_creates() {
    let h = engine();
    h.files(&[("Main", &thousand())]);
    let before = h.engine.object_count();
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert_eq!(h.engine.object_count() - before, 1000);
    assert_eq!(h.engine.visual_children(root).len(), 333);
    assert!(h.warnings().is_empty());
}

#[test]
#[ignore]
fn bench_create_from_ir_1000_objects() {
    let h = engine();
    h.files(&[("Main", &thousand())]);
    // Warm: the module is loaded and the types are made once per file.
    let r = h.engine.create_from_url("Main", None, vec![]).unwrap();
    h.engine.destroy_now(r);
    let n = 50;
    let t = Instant::now();
    for _ in 0..n {
        let r = h.engine.create_from_url("Main", None, vec![]).unwrap();
        h.engine.destroy_now(r);
    }
    let per = t.elapsed().as_secs_f64() / f64::from(n);
    println!(
        "create+destroy 1000 objects: {:.2} ms/file, {:.0} objects/s",
        per * 1e3,
        1000.0 / per
    );
    let t = Instant::now();
    let mut roots = Vec::new();
    for _ in 0..n {
        roots.push(h.engine.create_from_url("Main", None, vec![]).unwrap());
    }
    let per = t.elapsed().as_secs_f64() / f64::from(n);
    println!(
        "create only 1000 objects:    {:.2} ms/file, {:.0} objects/s",
        per * 1e3,
        1000.0 / per
    );
}
