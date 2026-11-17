//! The side streams an application made in its setup ([`Setup::batch`]):
//! each drained after the application inbox, one turn a frame
//! (ARCHITECTURE.md §3.3).
//!
//! [`Setup::batch`]: crate::Setup::batch

use std::any::Any;

use ljos_engine::{Engine, TurnCause};
use ljos_runtime::{BatchSender, Clock, Drain, DrainBudget, DrainReport, UiBatch};

/// What a stream's items are handed to: the whole run of a frame.
pub(crate) type BatchHandler<T> = Box<dyn FnMut(&Engine, Drain<'_, T>)>;

/// A stream of some item type, as the shell keeps it.
pub(crate) trait SideStream {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn has_pending(&self) -> bool;
    /// One turn: at most `budget`'s count of items, at least one.
    fn drain(&mut self, engine: &Engine, budget: DrainBudget, clock: &dyn Clock) -> DrainReport;
}

pub(crate) struct Stream<T> {
    pub(crate) batch: UiBatch<T>,
    pub(crate) handler: Option<BatchHandler<T>>,
    /// Whether the missing handler was warned of: once is enough.
    warned: bool,
}

impl<T> Stream<T> {
    pub(crate) fn new(batch: UiBatch<T>) -> Self {
        Self {
            batch,
            handler: None,
            warned: false,
        }
    }
}

impl<T: Send + 'static> SideStream for Stream<T> {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn has_pending(&self) -> bool {
        self.batch.has_pending()
    }

    fn drain(&mut self, engine: &Engine, budget: DrainBudget, clock: &dyn Clock) -> DrainReport {
        let Self {
            batch,
            handler,
            warned,
        } = self;
        let name = batch.name();
        batch.drain(budget, clock, |run| match handler.as_mut() {
            Some(f) => {
                // One run, one turn: a handler that panics is a fault of
                // the application, its run is dropped, and the stream is
                // drained again next frame.
                let cause = TurnCause::Message {
                    mailbox: name,
                    kind: None,
                };
                engine.turn_for(cause, None, || f(engine, run));
            }
            None => {
                if !*warned {
                    *warned = true;
                    engine.log(
                        ljos_engine::LogLevel::Warning,
                        "ljos.shell",
                        format!(
                            "side stream {name:?} has no handler (Setup::on_batch): its items are dropped"
                        ),
                    );
                }
            }
        })
    }
}

/// The streams of one application, in the order they were made.
#[derive(Default)]
pub(crate) struct Streams(Vec<Box<dyn SideStream>>);

impl Streams {
    pub(crate) fn add<T: Send + 'static>(&mut self, batch: UiBatch<T>) {
        self.0.push(Box::new(Stream::new(batch)));
    }

    /// The stream `sender` sends into, if this application made it.
    pub(crate) fn find<T: Send + 'static>(
        &mut self,
        sender: &BatchSender<T>,
    ) -> Option<&mut Stream<T>> {
        self.0
            .iter_mut()
            .filter_map(|s| s.as_any_mut().downcast_mut::<Stream<T>>())
            .find(|s| s.batch.is_fed_by(sender))
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Each stream with items gets one turn, in the order they were made,
    /// out of what `budget` has left after the inbox took `used`. A
    /// stream always gets its turn: one whose count is spent still takes
    /// one item, so a busy inbox cannot starve a stream (and the stream
    /// left with items wakes the next frame itself).
    pub(crate) fn drain(
        &mut self,
        engine: &Engine,
        budget: DrainBudget,
        used: usize,
        clock: &dyn Clock,
    ) -> DrainReport {
        let mut left = budget.max_messages.saturating_sub(used);
        let mut total = DrainReport::default();
        for stream in &mut self.0 {
            if !stream.has_pending() {
                continue;
            }
            let report = stream.drain(
                engine,
                DrainBudget {
                    max_messages: left,
                    ..budget
                },
                clock,
            );
            left = left.saturating_sub(report.drained);
            total.drained += report.drained;
            total.remaining += report.remaining;
            total.elapsed += report.elapsed;
        }
        total
    }
}
