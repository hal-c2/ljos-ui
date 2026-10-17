//! The application's badge, a count on its icon, set from Rust through
//! the one service the host installs, as `QGuiApplication::setBadgeNumber`.

use std::rc::Rc;

use ljos::prelude::*;

#[test]
fn a_count_reaches_the_service_and_zero_clears_it() {
    let engine = ljos::engine();
    let platform = Platform::of(&engine);
    let shown = RecordedBadge::default();
    platform.set_badge(Rc::new(shown.clone()));
    assert_eq!(platform.badge_number(), 0);

    platform.set_badge_number(3);
    assert_eq!(platform.badge_number(), 3);
    assert_eq!(shown.count(), 3);

    platform.set_badge_number(0);
    assert_eq!(platform.badge_number(), 0);
    assert_eq!(shown.counts(), [3, 0]);
}

#[test]
fn a_count_set_before_the_host_installs_its_service_is_shown_by_it() {
    let engine = ljos::engine();
    let platform = Platform::of(&engine);
    platform.set_badge_number(5);

    let shown = RecordedBadge::default();
    platform.set_badge(Rc::new(shown.clone()));

    assert_eq!(shown.counts(), [5]);
}

#[test]
fn without_a_host_service_the_count_is_only_kept() {
    let engine = ljos::engine();
    let platform = Platform::of(&engine);
    platform.set_badge_number(2);
    assert_eq!(platform.badge_number(), 2);
}
