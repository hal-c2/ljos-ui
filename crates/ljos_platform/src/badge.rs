//! The application's badge: a count shown on its icon in the dock or the
//! taskbar, as `QGuiApplication::setBadgeNumber` shows one. 0 clears it.
//!
//! [`Platform::set_badge_number`](crate::Platform::set_badge_number) keeps
//! the count and hands it to the installed [`BadgeService`]. Without a
//! host's service nothing is shown: [`RecordedBadge`] keeps what it was
//! given, which is what tests and headless hosts want, and what a desktop
//! with no badge does.

use std::cell::RefCell;
use std::rc::Rc;

/// What shows the badge, or a stand-in. `show` is called on the Engine
/// thread with every count set, 0 to clear; it must not wait for the
/// desktop.
pub trait BadgeService {
    fn show(&self, count: u32);
}

/// Shows nothing and keeps every count it was given, in order. Its clones
/// share one record, so a test keeps one and installs another.
#[derive(Clone, Debug, Default)]
pub struct RecordedBadge {
    counts: Rc<RefCell<Vec<u32>>>,
}

impl RecordedBadge {
    /// Every count given so far.
    pub fn counts(&self) -> Vec<u32> {
        self.counts.borrow().clone()
    }

    /// The last count given; 0 when none was or the badge was cleared.
    pub fn count(&self) -> u32 {
        self.counts.borrow().last().copied().unwrap_or(0)
    }
}

impl BadgeService for RecordedBadge {
    fn show(&self, count: u32) {
        self.counts.borrow_mut().push(count);
    }
}
