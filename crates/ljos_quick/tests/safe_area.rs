//! What a host that covers parts of the window tells the items in it.

mod common;

use common::*;
use ljos_quick::set_safe_area_insets;

const AREA: &str = r#"
import QtQuick
Item {
    objectName: "root"
    width: 200; height: 100
    property real top: SafeArea.margins.top
    property real innerTop: inner.SafeArea.margins.top
    property real innerBottom: inner.SafeArea.margins.bottom
    property real lowTop: low.SafeArea.margins.top
    Item { id: inner; width: 200; height: 30 }
    Item { id: low; y: 50; width: 200; height: 30 }
}
"#;

#[test]
fn what_the_host_covers_reaches_the_items_at_that_edge() {
    let e = engine();
    let root = load(&e, AREA);
    assert_eq!(num(&e, root, "top"), 0.0);
    set_safe_area_insets(&e, 0.0, 20.0, 0.0, 10.0);
    assert_eq!(num(&e, root, "top"), 20.0);
    assert_eq!(num(&e, root, "innerTop"), 20.0);
    // The item at the bottom of the window is not covered at the top.
    assert_eq!(num(&e, root, "lowTop"), 0.0);
    set_safe_area_insets(&e, 0.0, 0.0, 0.0, 0.0);
    assert_eq!(num(&e, root, "innerTop"), 0.0);
}
