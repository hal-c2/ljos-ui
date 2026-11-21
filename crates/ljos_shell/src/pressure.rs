//! What a window gives back when the system is short of memory
//! (ARCHITECTURE.md §8.5). Every host reports it the same way
//! (`Event::MemoryPressure`): Android's trim and low-memory callbacks, an
//! iOS memory warning. Most of them do not say how short (Android's
//! GameActivity glue drops the trim level), so the level is chosen from
//! what the window shows.

use ljos_runtime::MemoryPressure;

/// How much a window gives back: all that can be rebuilt when it is not on
/// the screen; only what it did not draw lately when it is, so that its
/// next frame does not build the screen again.
pub(crate) fn pressure_for(shown: bool) -> MemoryPressure {
    if shown {
        MemoryPressure::Moderate
    } else {
        MemoryPressure::Critical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_on_the_screen_keeps_what_it_draws() {
        assert_eq!(pressure_for(true), MemoryPressure::Moderate);
    }

    #[test]
    fn a_hidden_window_gives_back_all_it_can_rebuild() {
        assert_eq!(pressure_for(false), MemoryPressure::Critical);
    }
}
