//! How the cost of building a tree grows with the number of children.
//! Adding a child must not depend on how many the parent already has
//! (creating n delegates is then linear).
//!
//! Ignored by default:
//!
//! `cargo test --release -p ljos_quick --test children_bench -- --ignored --nocapture`

mod common;

use std::time::Instant;

use common::*;

/// A Column of a Repeater with `rows` rows (an integer model, so the model
/// itself costs nothing).
fn column_of(rows: usize) -> String {
    format!(
        r##"import QtQuick
Column {{
  Repeater {{
    objectName: "rep"; model: {rows}
    Rectangle {{
      width: 100 + index; height: 20; color: index % 2 ? "#eeeeee" : "#ffffff"
      property int row: index
    }}
  }}
}}
"##
    )
}

fn create_ms(rows: usize) -> f64 {
    let e = engine();
    compile(&e, &[("Main", &column_of(rows))]);
    let t = Instant::now();
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let ms = t.elapsed().as_secs_f64() * 1e3;
    assert_eq!(list(&e, root, "children").len(), rows + 1);
    ms
}

#[test]
#[ignore]
fn bench_repeater_10000_rows() {
    // Warm the allocator and the type tables.
    create_ms(100);
    for rows in [1_000, 10_000] {
        let ms = create_ms(rows);
        println!(
            "Repeater over {rows} rows: {ms:.1} ms to create ({:.1} us per delegate)",
            ms * 1e3 / rows as f64
        );
    }
}

/// The guard that runs with the other tests: ten times the rows may cost
/// little more than ten times the time. Loose, so a busy machine does not
/// fail it, but a quadratic add (100 times the time) does.
#[test]
fn creating_delegates_is_linear_in_their_number() {
    create_ms(100);
    let small = (0..3).map(|_| create_ms(400)).fold(f64::MAX, f64::min);
    let large = (0..3).map(|_| create_ms(4_000)).fold(f64::MAX, f64::min);
    assert!(
        large < small * 40.0,
        "400 rows took {small:.1} ms and 4000 rows {large:.1} ms"
    );
}
