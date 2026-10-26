//! `Graph::census`: the live nodes by kind, which budget tests pin.

use ljos_reactive::{Census, Graph};

#[test]
fn the_census_counts_values_bindings_effects_and_scopes() {
    let g = Graph::new();
    let r = g.root();
    assert_eq!(
        g.census(),
        Census {
            scopes: 1,
            ..Census::default()
        }
    );
    let s = g.create_scope(r).unwrap();
    let a = g.property(s, 1).unwrap();
    let b = g.binding(s, 0, move |g| g.get(a) + 1).unwrap();
    g.effect(s, move |g| {
        g.get(b);
    })
    .unwrap();
    g.effect_told(
        s,
        move |g| {
            g.get(a);
        },
        || {},
    )
    .unwrap();
    assert_eq!(
        g.census(),
        Census {
            values: 1,
            bindings: 1,
            effects: 2,
            scopes: 2,
        }
    );
    // A write over a binding replaces it: the node is then a value.
    g.set(b, 7);
    assert_eq!((g.census().values, g.census().bindings), (2, 0));
    g.dispose_scope(s).unwrap();
    assert_eq!(
        g.census(),
        Census {
            scopes: 1,
            ..Census::default()
        }
    );
    assert_eq!(g.len(), 0);
}

/// A page made while the one it replaces is still there doubles the
/// nodes for a moment; once the old one goes, the room its nodes had
/// goes too, and the page after reuses the low room again.
#[test]
fn the_room_of_a_page_replaced_goes_with_it() {
    fn page(g: &Graph<i64>) -> ljos_reactive::ScopeId {
        let s = g.create_scope(g.root()).unwrap();
        for i in 0..20_000 {
            let a = g.property(s, i).unwrap();
            g.binding(s, 0, move |g| g.get(a) + 1).unwrap();
        }
        s
    }
    let g = Graph::new();
    let base = g.bytes();
    let mut shown = page(&g);
    let one = g.bytes() - base;
    for _ in 0..3 {
        let next = page(&g);
        g.dispose_scope(std::mem::replace(&mut shown, next))
            .unwrap();
        let held = g.bytes() - base;
        assert!(held < one * 5 / 4, "{held} bytes held for a page of {one}");
    }
}
