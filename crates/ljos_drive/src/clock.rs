//! The seam between the driver and time: a test clock that moves only when
//! the driver moves it, or the real clock that moves by itself.

use std::time::Duration;

use ljos_runtime::{Clock, Time, VirtualClock};

use crate::DriveError;

/// How long one settle may take on the wall clock when time is real. A
/// scene that animates for ever never comes to rest, and whoever waits is
/// told so instead of kept waiting.
pub const SETTLE_WALL: Duration = Duration::from_millis(250);

/// The time a driven scene runs on.
pub trait SceneClock {
    fn now(&self) -> Time;

    /// Moves the time on to `t`. Only a test clock can; time never runs
    /// backwards, so an earlier `t` changes nothing.
    fn move_to(&self, t: Time) -> Result<(), DriveError>;

    /// The wall time one settle may take, or `None` when frames take no
    /// time that counts.
    fn settle_wall(&self) -> Option<Duration>;
}

/// The test clock: [`VirtualClock`] stands still until the driver lets
/// time pass, so what a scene does in a second is the same on every
/// machine.
impl SceneClock for VirtualClock {
    fn now(&self) -> Time {
        Clock::now(self)
    }

    fn move_to(&self, t: Time) -> Result<(), DriveError> {
        self.set(t);
        Ok(())
    }

    fn settle_wall(&self) -> Option<Duration> {
        None
    }
}

/// The real clock of a running application: time passes as it does, and
/// cannot be made to pass faster.
#[derive(Clone, Copy)]
pub struct RealClock<'a>(pub &'a dyn Clock);

impl std::fmt::Debug for RealClock<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RealClock").field(&self.0.now()).finish()
    }
}

impl SceneClock for RealClock<'_> {
    fn now(&self) -> Time {
        self.0.now()
    }

    fn move_to(&self, _: Time) -> Result<(), DriveError> {
        Err(DriveError::RealClock)
    }

    fn settle_wall(&self) -> Option<Duration> {
        Some(SETTLE_WALL)
    }
}
