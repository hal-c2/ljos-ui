//! How much of a frame goes to making objects in slices (ARCHITECTURE
//! §6.3).
//!
//! Asynchronous Loaders and `Component.incubateObject` queue creation jobs
//! with the engine; the scene runs them after polish, for a budget that is
//! 3 ms of a 60 Hz frame and the same share of a shorter one. Each job is
//! polled with a slice that ends where the budget does and yields when it
//! is used up, so one large component costs many frames a little instead
//! of one frame a lot. The time is charged to the job's owner (the
//! boundary the object is made in), which is what the `top` overlay
//! shows.

use std::time::Duration;

use crate::scene::INCUBATION_BUDGET;

/// The refresh rate [`INCUBATION_BUDGET`] is for.
pub const BASE_REFRESH_HZ: f64 = 60.0;

/// The least a frame gives creation jobs, whatever the refresh rate: the
/// budget of a 720 Hz display. A budget near nothing is spent before the
/// first job is polled, and then nothing would ever be made.
pub const MIN_INCUBATION_BUDGET: Duration = Duration::from_micros(250);

/// The incubation budget of one frame at `refresh_hz`: 3 ms at 60 Hz and
/// below, and the same share of the frame above it (1.5 ms at 120 Hz),
/// never less than [`MIN_INCUBATION_BUDGET`]. A rate that is no rate gives
/// the 60 Hz budget.
pub fn incubation_budget(refresh_hz: f64) -> Duration {
    if !refresh_hz.is_finite() || refresh_hz <= BASE_REFRESH_HZ {
        return INCUBATION_BUDGET;
    }
    INCUBATION_BUDGET
        .mul_f64(BASE_REFRESH_HZ / refresh_hz)
        .max(MIN_INCUBATION_BUDGET)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_is_three_milliseconds_at_sixty_hertz() {
        assert_eq!(incubation_budget(60.0), Duration::from_millis(3));
    }

    #[test]
    fn no_display_is_fast_enough_to_leave_nothing() {
        assert_eq!(incubation_budget(720.0), MIN_INCUBATION_BUDGET);
        assert_eq!(incubation_budget(10_000.0), MIN_INCUBATION_BUDGET);
        assert_eq!(incubation_budget(f64::MAX), MIN_INCUBATION_BUDGET);
    }

    #[test]
    fn a_faster_display_gets_the_same_share_of_a_shorter_frame() {
        assert_eq!(incubation_budget(120.0), Duration::from_micros(1500));
        assert_eq!(incubation_budget(240.0), Duration::from_micros(750));
    }

    #[test]
    fn a_slower_display_gets_no_more() {
        assert_eq!(incubation_budget(30.0), Duration::from_millis(3));
    }

    #[test]
    fn a_rate_that_is_none_gives_the_base_budget() {
        for hz in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(incubation_budget(hz), Duration::from_millis(3));
        }
    }
}
