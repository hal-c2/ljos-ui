//! When the Android host's pump, which carries the activity's events to
//! the application, ends. Only the Android host uses it; it builds
//! everywhere so that its tests run on every host.

/// Whether the pump goes on. Once it has asked the activity to finish, it
/// goes on after the application's loop has ended, until the activity is
/// destroyed: what follows `android_main` must not race its teardown.
pub(crate) fn keeps_pumping(destroyed: bool, closed: bool, finishing: bool) -> bool {
    !destroyed && (!closed || finishing)
}

/// The pump's `closed` and `finishing` once the application asked to exit.
/// The application is ending, so nothing more goes to it. A finish the
/// activity did not take brings no Destroy to wait for: the pump ends now,
/// not at the next event, and `main` ends the process.
pub(crate) fn exiting(finish_taken: bool) -> (bool, bool) {
    (true, finish_taken)
}

#[cfg(test)]
mod tests {
    use super::{exiting, keeps_pumping};

    #[test]
    fn an_exit_the_activity_did_not_finish_ends_the_pump() {
        let (closed, finishing) = exiting(false);
        assert!(!keeps_pumping(false, closed, finishing));
        let (closed, finishing) = exiting(true);
        assert!(keeps_pumping(false, closed, finishing));
        assert!(!keeps_pumping(true, closed, finishing));
    }

    #[test]
    fn a_pump_that_asked_for_finish_waits_for_destroy_after_the_application_ends() {
        assert!(keeps_pumping(false, true, true));
        assert!(!keeps_pumping(true, true, true));
    }

    #[test]
    fn a_pump_ends_when_the_application_ends_without_a_finish() {
        assert!(!keeps_pumping(false, true, false));
        assert!(keeps_pumping(false, false, false));
    }
}
