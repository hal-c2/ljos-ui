//! Drags from other applications where the window system asks the target
//! for its answer inside the call that tells it of the drag, and takes
//! what that call returns: AppKit's `NSDraggingDestination` on macOS
//! (`appkit_drag`), OLE's `IDropTarget` on Windows (`win32_drag`).
//!
//! The answer comes from the scene, which the loop holds, and the loop is
//! the thread the window system is calling on, so it cannot be asked in
//! that call. Each call returns the answer the loop gave last, and posts
//! the step for the loop to answer anew; both systems ask again as the
//! pointer moves and while it rests, so the answer the source shows is
//! one step behind at most. A drop takes the last answer, which is the
//! action qml6 reports too: its source's, of the last status before the
//! drop.
//!
//! The data is read once the loop has answered from the formats alone
//! ([`DragStep::Offered`]), on the loop's thread, which is the one the
//! system's objects for it (an `NSPasteboard`, an `IDataObject`) belong
//! to.

// The core is built everywhere so that it is tested everywhere; only
// macOS and Windows follow drags with it.
#![cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ljos_platform::{DragActions, DragData, DropAction, DropActions};

use crate::drags::{At, DragStep, Reply};

/// Reads a drag's data, on the loop's thread.
pub(crate) type Read = Box<dyn FnOnce() -> DragData>;

/// What a source that allows none of copy, move and link allows.
pub(crate) const NO_ACTIONS: DragActions = DragActions {
    allowed: DropActions::NONE,
    proposed: DropAction::Copy,
};

thread_local! {
    /// The read of the drag the loop is answering, which is not `Send`
    /// (the system's objects for the drag stay on their thread), with the
    /// number of the drag it reads.
    static PENDING: RefCell<Option<(u64, Read)>> = const { RefCell::new(None) };
}

/// What the loop's answers and the window system's calls share.
struct Shared {
    /// The number of the drag over a window now; each new one, and each
    /// end, takes the next, so an answer to an older one is not taken.
    drag: AtomicU64,
    state: Mutex<State>,
}

#[derive(Clone, Copy)]
struct State {
    answer: Option<DropAction>,
    at: At,
    actions: DragActions,
    /// The scene was told of the drag: its data is read.
    entered: bool,
}

impl Shared {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Follows one drag at a time over the application's windows.
pub(crate) struct Follower {
    post: Arc<dyn Fn(DragStep) + Send + Sync>,
    shared: Arc<Shared>,
    /// A drag is over a window.
    over: bool,
}

impl Follower {
    pub(crate) fn new(post: Arc<dyn Fn(DragStep) + Send + Sync>) -> Self {
        Self {
            post,
            shared: Arc::new(Shared {
                drag: AtomicU64::new(0),
                state: Mutex::new(State {
                    answer: None,
                    at: At::Logical(0.0, 0.0),
                    actions: NO_ACTIONS,
                    entered: false,
                }),
            }),
            over: false,
        }
    }

    /// A drag offering `formats` came over the window `target` names.
    /// The answer is `None`: the loop has not seen it yet.
    pub(crate) fn entered(
        &mut self,
        target: usize,
        at: At,
        formats: Vec<String>,
        actions: DragActions,
        read: Read,
    ) -> Option<DropAction> {
        if self.over {
            self.left();
        }
        self.over = true;
        let drag = self.shared.drag.fetch_add(1, Ordering::AcqRel) + 1;
        *self.shared.state() = State {
            answer: None,
            at,
            actions,
            entered: false,
        };
        PENDING.with(|p| p.replace(Some((drag, read))));
        let (shared, post) = (Arc::clone(&self.shared), Arc::clone(&self.post));
        (self.post)(DragStep::Offered {
            target,
            at,
            formats,
            actions,
            answer: self.answer(drag),
            answered: Box::new(move || read_and_enter(&shared, &post, drag, target)),
        });
        None
    }

    /// It moved, or the source changed what it allows: the answer the
    /// loop gave last.
    pub(crate) fn moved(&mut self, at: At, actions: DragActions) -> Option<DropAction> {
        if !self.over {
            return None;
        }
        let mut state = self.shared.state();
        let changed = state.at != at || state.actions != actions;
        state.at = at;
        state.actions = actions;
        let State {
            answer, entered, ..
        } = *state;
        drop(state);
        // Before the data is read the scene has not been told of the drag,
        // and is told where it is then.
        if entered && changed {
            let drag = self.shared.drag.load(Ordering::Acquire);
            (self.post)(DragStep::Moved {
                at,
                actions: Some(actions),
                answer: self.answer(drag),
            });
        }
        answer
    }

    /// It left the window, or was given up.
    pub(crate) fn left(&mut self) {
        if std::mem::take(&mut self.over) {
            self.end();
            (self.post)(DragStep::Left);
        }
    }

    /// It was let go: the answer the drop is taken with, which the loop
    /// gave last. A drag the scene has not been told of, or one it would
    /// not take, leaves instead.
    pub(crate) fn dropped(&mut self) -> Option<DropAction> {
        if !std::mem::take(&mut self.over) {
            return None;
        }
        let State {
            answer, entered, ..
        } = *self.shared.state();
        let drag = self.end();
        match answer {
            Some(_) if entered => (self.post)(DragStep::Dropped {
                answer: self.answer(drag),
            }),
            _ => (self.post)(DragStep::Left),
        }
        answer.filter(|_| entered)
    }

    /// Ends the drag over a window: answers and reads for it are not
    /// taken after this. Returns its number.
    fn end(&self) -> u64 {
        PENDING.with(|p| p.replace(None));
        self.shared.drag.fetch_add(1, Ordering::AcqRel)
    }

    fn answer(&self, drag: u64) -> Box<dyn Reply> {
        Box::new(Answer {
            shared: Arc::clone(&self.shared),
            drag,
        })
    }
}

/// After the loop answered from the formats: reads the data and tells the
/// scene, where the drag is by then.
fn read_and_enter(
    shared: &Arc<Shared>,
    post: &Arc<dyn Fn(DragStep) + Send + Sync>,
    drag: u64,
    target: usize,
) {
    let Some(read) = PENDING.with(|p| {
        let mut p = p.borrow_mut();
        match p.take() {
            Some((d, read)) if d == drag => Some(read),
            other => {
                *p = other;
                None
            }
        }
    }) else {
        return;
    };
    let data = read();
    if shared.drag.load(Ordering::Acquire) != drag {
        return;
    }
    let (at, actions) = {
        let mut state = shared.state();
        state.entered = true;
        (state.at, state.actions)
    };
    post(DragStep::Entered {
        target,
        at,
        data,
        actions,
        answer: Box::new(Answer {
            shared: Arc::clone(shared),
            drag,
        }),
    });
}

/// The loop's answer for one drag, kept for the window system's next
/// call.
struct Answer {
    shared: Arc<Shared>,
    drag: u64,
}

impl Reply for Answer {
    fn taken(&self, action: Option<DropAction>) {
        let mut state = self.shared.state();
        if self.shared.drag.load(Ordering::Acquire) == self.drag {
            state.answer = action;
        }
    }

    /// The window system was answered when the drop came.
    fn dropped(self: Box<Self>, _action: Option<DropAction>) {}
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    /// A follower whose steps go to a channel, which a test plays the
    /// loop on.
    fn follower() -> (Follower, mpsc::Receiver<DragStep>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let post: Arc<dyn Fn(DragStep) + Send + Sync> = Arc::new(move |step| {
            let _ = tx.lock().unwrap().send(step);
        });
        (Follower::new(post), rx)
    }

    fn copy_or_move() -> DragActions {
        DragActions {
            allowed: [DropAction::Copy, DropAction::Move].into_iter().collect(),
            proposed: DropAction::Copy,
        }
    }

    fn text() -> Read {
        Box::new(|| DragData::text("hi"))
    }

    /// The loop's part: answers `Offered` and runs its read, and answers
    /// the rest with `action`.
    fn play(rx: &mpsc::Receiver<DragStep>, action: Option<DropAction>) -> Vec<&'static str> {
        let mut seen = Vec::new();
        while let Ok(step) = rx.try_recv() {
            match step {
                DragStep::Offered {
                    answer, answered, ..
                } => {
                    seen.push("offered");
                    answer.taken(action);
                    answered();
                }
                DragStep::Entered { answer, data, .. } => {
                    assert_eq!(data.text.as_deref(), Some("hi"));
                    seen.push("entered");
                    answer.taken(action);
                }
                DragStep::Moved { answer, .. } => {
                    seen.push("moved");
                    answer.taken(action);
                }
                DragStep::Left => seen.push("left"),
                DragStep::Dropped { answer } => {
                    seen.push("dropped");
                    answer.dropped(action);
                }
                DragStep::OutEnded => seen.push("out ended"),
            }
        }
        seen
    }

    #[test]
    fn the_window_system_hears_the_loops_last_answer() {
        let (mut f, rx) = follower();
        let at = At::Logical(10.0, 10.0);
        let formats = vec!["text/plain".to_string()];
        assert_eq!(f.entered(1, at, formats, copy_or_move(), text()), None);
        // Asked again before the loop has run: still no answer.
        assert_eq!(f.moved(at, copy_or_move()), None);
        assert_eq!(play(&rx, Some(DropAction::Copy)), ["offered", "entered"]);
        assert_eq!(f.moved(at, copy_or_move()), Some(DropAction::Copy));
        // Resting, it is asked again without a step for the loop.
        assert_eq!(play(&rx, None), Vec::<&str>::new());
        let further = At::Logical(20.0, 10.0);
        assert_eq!(f.moved(further, copy_or_move()), Some(DropAction::Copy));
        assert_eq!(play(&rx, Some(DropAction::Move)), ["moved"]);
        assert_eq!(f.dropped(), Some(DropAction::Move));
        assert_eq!(play(&rx, Some(DropAction::Move)), ["dropped"]);
    }

    #[test]
    fn the_scene_is_told_where_the_drag_is_once_its_data_is_read() {
        let (mut f, rx) = follower();
        f.entered(1, At::Logical(1.0, 1.0), vec![], copy_or_move(), text());
        // Moving before the loop has read the data: no step of its own.
        f.moved(At::Logical(5.0, 5.0), copy_or_move());
        let DragStep::Offered {
            answer, answered, ..
        } = rx.try_recv().unwrap()
        else {
            panic!("offered first");
        };
        answer.taken(Some(DropAction::Copy));
        answered();
        let Ok(DragStep::Entered { at, .. }) = rx.try_recv() else {
            panic!("then entered");
        };
        assert_eq!(at, At::Logical(5.0, 5.0));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_drop_before_the_loop_has_answered_is_refused() {
        let (mut f, rx) = follower();
        f.entered(1, At::Logical(1.0, 1.0), vec![], copy_or_move(), text());
        assert_eq!(f.dropped(), None);
        // The read the loop would run belongs to a drag that is over.
        assert_eq!(play(&rx, Some(DropAction::Copy)), ["offered", "left"]);
    }

    #[test]
    fn an_answer_to_a_drag_that_left_is_not_taken() {
        let (mut f, rx) = follower();
        f.entered(1, At::Logical(1.0, 1.0), vec![], copy_or_move(), text());
        let DragStep::Offered { answer, .. } = rx.try_recv().unwrap() else {
            panic!("offered first");
        };
        f.left();
        f.entered(2, At::Logical(1.0, 1.0), vec![], copy_or_move(), text());
        answer.taken(Some(DropAction::Copy));
        assert_eq!(f.moved(At::Logical(1.0, 1.0), copy_or_move()), None);
        assert_eq!(play(&rx, None), ["left", "offered", "entered"]);
    }
}
