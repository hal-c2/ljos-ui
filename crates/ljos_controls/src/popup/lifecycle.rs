//! A popup's way between closed and open, as one table.
//!
//! Every way a popup is opened or closed (`open()`, `close()`, being
//! turned away from, `visible` written, declared open, a drawer the
//! pointer brings in or lets go) is one [`Event`]. What the popup does
//! about it is decided by [`plan`], which reads a [`Snapshot`] and no
//! engine, and is done by [`run`], which stops as soon as a handler of
//! what the popup said has destroyed it. The orders of signals are
//! therefore written once, here, and tested without a window.
//!
//! Two steps look again when they are done: [`Effect::Arrive`] and
//! [`Effect::Leave`] run a transition only if the popup has one and is
//! on a screen *then*, since a handler of the signal before them may
//! have given it one or taken it off.

use std::rc::Rc;

use ljos_engine::{Engine, EngineError, Interception, Interceptor, ObjectId, PropIndex, Value};
use ljos_quick::Passage;
use smallvec::SmallVec;

use super::kind::PopupKind;
use super::{
    Popup, PopupState, give_back_keyboard, item_of, state_of, take_keyboard, try_present, withdraw,
};
use crate::util::{check, emit, flag, is_a, num, object, set_computed, set_derived};

/// Where a popup is between shown and hidden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Phase {
    /// Open or closed: no transition runs.
    #[default]
    Rest,
    /// Its enter transition runs: visible, not yet opened.
    Coming,
    /// Its exit transition runs: still visible, no longer opened.
    Going,
}

/// What happened to a popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Event {
    /// `open()`.
    Open,
    /// `close()`.
    Close,
    /// The user turned away from it: Escape, or a press beside it.
    Dismiss,
    /// `visible` was assigned, or its binding gave a new value, and no
    /// interceptor took it: the property is written already.
    VisibleWritten(bool),
    /// It was declared `visible: true` and has just been completed.
    Declared,
    /// The pointer brings a closed drawer in where it is.
    ShowStill,
    /// The pointer took a drawer that was coming in.
    Hold,
    /// The pointer let go of a drawer that goes the rest of the way in;
    /// `again` if it had been open.
    SettleOpen { again: bool },
    /// The pointer let go of a drawer where nothing of it is in.
    CloseAtOnce,
}

/// What [`plan`] reads: the popup as it is when the event comes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub visible: bool,
    pub phase: Phase,
    /// It is on a screen: open, in an overlay.
    pub shown: bool,
    pub has_enter: bool,
    pub has_exit: bool,
    pub kind: PopupKind,
    /// A tool tip that would wait for its delay before it is shown.
    pub delays: bool,
    /// A tool tip that waits for its delay now.
    pub waiting: bool,
}

/// A signal a popup gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Signal {
    AboutToShow,
    AboutToHide,
    /// `visibleChanged`, said by the popup itself and not by a write.
    VisibleChanged,
    Closed,
}

impl Signal {
    fn name(self) -> &'static str {
        match self {
            Self::AboutToShow => "aboutToShow",
            Self::AboutToHide => "aboutToHide",
            Self::VisibleChanged => "visibleChanged",
            Self::Closed => "closed",
        }
    }
}

/// One step a popup takes; [`run`] does them in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Effect {
    /// A tool tip starts to wait for its delay, if it does not already.
    Wait,
    /// A tool tip stops waiting for its delay.
    CancelWait,
    /// The enter transition stops where it is.
    StopEnter,
    /// The exit transition stops where it is.
    StopExit,
    /// What the exit transition began from is not put back.
    ForgetExit,
    /// `visible` becomes true, `opened` too if `opened`, and then the
    /// popup says it is about to show, all as one change.
    Announce {
        opened: bool,
    },
    Say(Signal),
    /// `visible` becomes false, written by the popup.
    Hide,
    /// `opened` becomes false.
    Unopen,
    /// Into the overlay of its window, and on the screen.
    Present,
    /// The popup takes the keyboard, if it wants it and has not yet.
    TakeKeyboard,
    /// Its shade, which went clear as it started to go, is shown again.
    ShowShade,
    /// It is opened when its enter transition has run, at once if it has
    /// none or is on no screen.
    Arrive,
    /// It is gone when its exit transition has run, at once if it has
    /// none or is on no screen; the keyboard goes back as it starts.
    Leave,
    /// It is gone: hidden, and says it is closed.
    Gone,
    /// Off the screen, if it is on one.
    Withdraw,
    /// A dialog that is turned away from is rejected.
    Reject,
}

pub(super) type Effects = SmallVec<[Effect; 8]>;

/// What a popup in `s` does about `event`.
pub(super) fn plan(s: &Snapshot, event: Event) -> Effects {
    use Effect::*;
    let mut out = Effects::new();
    match event {
        Event::Open if s.phase == Phase::Going => {
            // A popup that is going is still visible, and comes back from
            // where it is: what the exit transition did so far stays. Qt
            // says it is visible again, though it never stopped being, and
            // its shade, which went clear, is back at once.
            out.extend([
                StopExit,
                ForgetExit,
                Say(Signal::AboutToShow),
                Say(Signal::VisibleChanged),
                TakeKeyboard,
                ShowShade,
                Arrive,
            ]);
        }
        Event::Open if s.visible => {}
        // A tool tip that waits for its delay opens when that has passed.
        Event::Open if s.delays => out.push(Wait),
        Event::Open => {
            // Visible and, without an enter transition, opened already as
            // it says it is about to show (measured on Qt 6.11.2,
            // `popup-parity.feature`).
            out.extend([
                Announce {
                    opened: !s.has_enter,
                },
                Present,
                Arrive,
            ]);
        }
        // Still waiting for its delay: it was never shown, so there is
        // nothing to hide and nothing to say.
        Event::Close if s.waiting => out.extend([CancelWait, Hide]),
        Event::Close if !s.visible || s.phase == Phase::Going => {}
        Event::Close => {
            if s.phase == Phase::Coming {
                // It goes from where it has come to.
                out.push(StopEnter);
            }
            out.extend([Say(Signal::AboutToHide), Unopen, Leave]);
        }
        // Turned away from already.
        Event::Dismiss if s.phase == Phase::Going => {}
        Event::Dismiss if s.kind.is_rejected_when_dismissed() => out.push(Reject),
        Event::Dismiss => return plan(s, Event::Close),
        Event::VisibleWritten(true) if s.delays => out.push(Wait),
        // Declared closed: nothing to show.
        Event::Declared if !s.visible => {}
        Event::VisibleWritten(true) | Event::Declared => {
            // `visible` is written already, before the popup says it is
            // about to show.
            out.extend([Say(Signal::AboutToShow), Present, Arrive]);
        }
        // A tool tip that is still waiting has nothing to hide.
        Event::VisibleWritten(false) if s.waiting => out.push(CancelWait),
        Event::VisibleWritten(false) => {
            // Reached only by a write that no interceptor saw: the popup
            // is invisible already, so there is nothing left to go.
            out.extend([
                StopEnter,
                StopExit,
                ForgetExit,
                Say(Signal::AboutToHide),
                Unopen,
                Withdraw,
                Say(Signal::Closed),
            ]);
        }
        Event::ShowStill if s.visible => {}
        Event::ShowStill => {
            // Visible already as it says it is about to show, as `open` is
            // (measured on Qt 6.11.2 and 6.12.0, `drawer-drag.feature`);
            // the pointer brings it in, not its enter transition.
            out.extend([Announce { opened: false }, Present]);
        }
        Event::Hold if s.phase == Phase::Coming => out.push(StopEnter),
        Event::Hold => {}
        Event::SettleOpen { again } => {
            match s.phase {
                Phase::Coming => out.push(StopEnter),
                Phase::Going => out.extend([StopExit, ForgetExit]),
                Phase::Rest => {}
            }
            // One that was open says once more that it is about to show,
            // as Qt does.
            if again {
                out.push(Say(Signal::AboutToShow));
            }
            out.push(Arrive);
        }
        Event::CloseAtOnce if !s.visible => {}
        Event::CloseAtOnce => out.extend([
            StopEnter,
            StopExit,
            ForgetExit,
            Say(Signal::AboutToHide),
            Unopen,
            Gone,
        ]),
    }
    out
}

/// Whether `visible` being written `on` is for the popup to see to, as
/// `open()` or `close()`, rather than a plain write: a popup that is told
/// to be invisible stays visible until it has gone, one that is told to be
/// visible while it goes comes back, and one that is told to be visible
/// says that it is about to show before it is. Asked only of a popup with
/// a transition.
pub(super) fn intercepts(s: &Snapshot, on: bool) -> bool {
    if on {
        // A tool tip that is made visible waits for its delay first, which
        // is its own to see to.
        s.phase == Phase::Going || (!s.kind.waits_for_delay() && !s.visible)
    } else {
        s.shown && s.has_exit && s.visible
    }
}

/// The popup as [`plan`] sees it. Read untracked: the `visible`
/// interceptor builds one on every write, which may come from an effect,
/// and what the popup is about to do is no dependency of whoever wrote.
fn snapshot(engine: &Engine, popup: ObjectId, state: &PopupState) -> Snapshot {
    let kind = super::kind::of(engine, popup, state);
    engine.untrack(|| Snapshot {
        visible: flag(engine, popup, Popup::VISIBLE.index),
        phase: state.phase.get(),
        shown: state.shown.get(),
        has_enter: transition(engine, popup, Popup::ENTER.index).is_some(),
        has_exit: transition(engine, popup, Popup::EXIT.index).is_some(),
        kind,
        delays: kind.delays(engine, popup),
        waiting: kind.is_waiting(engine, popup),
    })
}

/// Does what `popup` does about `event`.
pub(super) fn handle(engine: &Engine, popup: ObjectId, event: Event) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    let effects = plan(&snapshot(engine, popup, &state), event);
    run(engine, popup, &state, &effects)
}

/// Does `effects` in order. A handler of what the popup says may destroy
/// it; nothing more is done to it then.
fn run(
    engine: &Engine,
    popup: ObjectId,
    state: &Rc<PopupState>,
    effects: &[Effect],
) -> Result<(), EngineError> {
    for &effect in effects {
        apply(engine, popup, state, effect)?;
        if !engine.is_alive(popup) {
            return Ok(());
        }
    }
    Ok(())
}

fn apply(
    engine: &Engine,
    popup: ObjectId,
    state: &Rc<PopupState>,
    effect: Effect,
) -> Result<(), EngineError> {
    match effect {
        Effect::Wait => super::kind::of(engine, popup, state).wait(engine, popup),
        Effect::CancelWait => {
            super::kind::of(engine, popup, state).cancel_wait(engine, popup);
        }
        Effect::StopEnter => stop(engine, popup, state, Popup::ENTER.index),
        Effect::StopExit => stop(engine, popup, state, Popup::EXIT.index),
        Effect::ForgetExit => state.before_exit.set(None),
        Effect::Announce { opened } => {
            state.switching.set(true);
            let said = engine.batch(|| -> Result<(), EngineError> {
                set_derived(engine, popup, Popup::VISIBLE.index, Value::Bool(true))?;
                if opened {
                    set_computed(engine, popup, Popup::OPENED.index, Value::Bool(true))?;
                }
                emit(engine, popup, Signal::AboutToShow.name());
                Ok(())
            });
            state.switching.set(false);
            said?;
        }
        Effect::Say(Signal::VisibleChanged) => {
            state.switching.set(true);
            emit(engine, popup, Signal::VisibleChanged.name());
            state.switching.set(false);
        }
        Effect::Say(signal) => emit(engine, popup, signal.name()),
        Effect::Hide => write_visible(engine, popup, state, false)?,
        Effect::Unopen => {
            set_computed(engine, popup, Popup::OPENED.index, Value::Bool(false))?;
        }
        Effect::Present => try_present(engine, popup)?,
        Effect::TakeKeyboard => {
            if let Some(item) = item_of(engine, popup) {
                engine.untrack(|| take_keyboard(engine, popup, item));
            }
        }
        Effect::ShowShade => {
            engine.untrack(|| super::overlay::show_shade_again(engine, popup, state))?;
        }
        Effect::Arrive => arrive(engine, popup, state)?,
        Effect::Leave => leave(engine, popup, state)?,
        Effect::Gone => gone(engine, popup, state)?,
        Effect::Withdraw => {
            if state.shown.get() {
                engine.untrack(|| withdraw(engine, popup, state))?;
            }
        }
        Effect::Reject => crate::dialog::reject(engine, popup)?,
    }
    Ok(())
}

fn write_visible(
    engine: &Engine,
    popup: ObjectId,
    state: &PopupState,
    on: bool,
) -> Result<(), EngineError> {
    state.switching.set(true);
    let wrote = set_derived(engine, popup, Popup::VISIBLE.index, Value::Bool(on));
    state.switching.set(false);
    wrote
}

/// The `Transition` in `prop` of `popup`.
fn transition(engine: &Engine, popup: ObjectId, prop: PropIndex) -> Option<ObjectId> {
    engine
        .untrack(|| object(engine, popup, prop))
        .filter(|t| is_a::<ljos_quick::Transition>(engine, *t))
}

/// Ends the transition in `prop` where it is.
fn stop(engine: &Engine, popup: ObjectId, state: &PopupState, prop: PropIndex) {
    state.phase.set(Phase::Rest);
    if let Some(t) = transition(engine, popup, prop) {
        ljos_quick::stop_transition(engine, t);
    }
}

fn arrive(engine: &Engine, popup: ObjectId, state: &Rc<PopupState>) -> Result<(), EngineError> {
    let enter = transition(engine, popup, Popup::ENTER.index).filter(|_| state.shown.get());
    let Some(enter) = enter else {
        return arrived(engine, popup);
    };
    state.phase.set(Phase::Coming);
    let st = state.clone();
    ljos_quick::run_passage(engine, enter, popup, Passage::Enter, move |e| {
        if st.phase.get() == Phase::Coming {
            st.phase.set(Phase::Rest);
            check(e, "opened", arrived(e, popup));
        }
    });
    Ok(())
}

fn arrived(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    if !engine.is_alive(popup) {
        return Ok(());
    }
    set_computed(engine, popup, Popup::OPENED.index, Value::Bool(true))?;
    emit(engine, popup, "opened");
    Ok(())
}

fn leave(engine: &Engine, popup: ObjectId, state: &Rc<PopupState>) -> Result<(), EngineError> {
    let exit = transition(engine, popup, Popup::EXIT.index).filter(|_| state.shown.get());
    let Some(exit) = exit else {
        return gone(engine, popup, state);
    };
    // The keyboard goes back as the popup starts to go: what is typed
    // meanwhile is not for something on its way out.
    if let Some(item) = item_of(engine, popup) {
        engine.untrack(|| give_back_keyboard(engine, popup, item))?;
    }
    engine.untrack(|| super::overlay::clear_shade(engine, popup, state))?;
    state.phase.set(Phase::Going);
    let (opacity, scale) = engine.untrack(|| {
        (
            num(engine, popup, Popup::OPACITY.index),
            num(engine, popup, Popup::SCALE.index),
        )
    });
    state.before_exit.set(Some((opacity, scale)));
    let st = state.clone();
    ljos_quick::run_passage(engine, exit, popup, Passage::Exit, move |e| {
        if st.phase.get() == Phase::Going {
            check(e, "closed", gone(e, popup, &st));
        }
    });
    Ok(())
}

/// The popup has gone: it is hidden and says that it is closed. It has
/// the opacity and scale again that it began to go from, so that it can
/// be shown again.
fn gone(engine: &Engine, popup: ObjectId, state: &PopupState) -> Result<(), EngineError> {
    state.phase.set(Phase::Rest);
    if !engine.is_alive(popup) {
        return Ok(());
    }
    if state.shown.get() {
        engine.untrack(|| withdraw(engine, popup, state))?;
    }
    if let Some((opacity, scale)) = state.before_exit.take() {
        set_derived(engine, popup, Popup::OPACITY.index, Value::Real(opacity))?;
        set_derived(engine, popup, Popup::SCALE.index, Value::Real(scale))?;
    }
    write_visible(engine, popup, state, false)?;
    emit(engine, popup, Signal::Closed.name());
    Ok(())
}

/// Stands in front of `visible` while the popup has a transition, so that
/// a write of it is seen to as `open()` or `close()` ([`intercepts`]).
pub(super) fn guard_visible(engine: &Engine, popup: ObjectId, state: &Rc<PopupState>) {
    let prop = Popup::VISIBLE.index;
    let has = |p| transition(engine, popup, p).is_some();
    if !has(Popup::ENTER.index) && !has(Popup::EXIT.index) {
        engine.set_interceptor(popup, prop, None);
        return;
    }
    let st = state.clone();
    let guard: Interceptor = Rc::new(move |e, v, how| {
        let on = v.to_boolean();
        if !intercepts(&snapshot(e, popup, &st), on) {
            return false;
        }
        let event = if on { Event::Open } else { Event::Close };
        let act = move |e: &Engine| check(e, "visible", handle(e, popup, event));
        match how {
            Interception::Write => act(e),
            // Its binding is being worked out: nothing may be written yet.
            Interception::Binding => e.after_binding(act),
        }
        true
    });
    engine.set_interceptor(popup, prop, Some(guard));
}

pub(crate) fn open(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    handle(engine, popup, Event::Open)
}

pub(crate) fn close(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    handle(engine, popup, Event::Close)
}

/// Closes a popup because the user turned away from it: Escape, or a
/// press beside it. A dialog is rejected by that.
pub(crate) fn dismiss(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    handle(engine, popup, Event::Dismiss)
}

/// `visible` was assigned, or its binding gave a new value: the same as
/// `open()` and `close()`, with the property already written.
pub(crate) fn visible_written(
    engine: &Engine,
    popup: ObjectId,
    on: bool,
) -> Result<(), EngineError> {
    handle(engine, popup, Event::VisibleWritten(on))
}

/// Shows a closed popup where it is, without its enter transition: a
/// drawer that the pointer brings in.
pub(super) fn show_still(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    handle(engine, popup, Event::ShowStill)
}

/// Holds a popup that is coming where it is: the pointer has it now.
pub(super) fn hold(engine: &Engine, popup: ObjectId) {
    check(engine, "hold", handle(engine, popup, Event::Hold));
}

/// Brings a popup the pointer let go of the rest of the way in with its
/// enter transition; one that was open (`again`) says so once more.
pub(super) fn settle_open(
    engine: &Engine,
    popup: ObjectId,
    again: bool,
) -> Result<(), EngineError> {
    handle(engine, popup, Event::SettleOpen { again })
}

/// Closes a popup without its exit transition: a drawer let go where
/// nothing of it is in.
pub(super) fn close_at_once(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    handle(engine, popup, Event::CloseAtOnce)
}

/// Whether `popup` is on its way out: its exit transition runs.
pub(crate) fn is_going(engine: &Engine, popup: ObjectId) -> bool {
    state_of(engine, popup).is_some_and(|s| s.phase.get() == Phase::Going)
}

#[cfg(test)]
mod tests {
    use super::Effect::*;
    use super::*;

    fn closed() -> Snapshot {
        Snapshot::default()
    }

    fn open() -> Snapshot {
        Snapshot {
            visible: true,
            shown: true,
            ..Snapshot::default()
        }
    }

    fn with(s: Snapshot, f: impl FnOnce(&mut Snapshot)) -> Snapshot {
        let mut s = s;
        f(&mut s);
        s
    }

    fn going() -> Snapshot {
        with(open(), |s| {
            s.phase = Phase::Going;
            s.has_exit = true;
        })
    }

    fn coming() -> Snapshot {
        with(open(), |s| {
            s.phase = Phase::Coming;
            s.has_enter = true;
        })
    }

    fn tip(s: Snapshot) -> Snapshot {
        with(s, |s| s.kind = PopupKind::ToolTip)
    }

    fn plan_of(s: Snapshot, event: Event) -> Vec<Effect> {
        plan(&s, event).into_vec()
    }

    #[test]
    fn open_is_visible_and_opened_as_it_says_it_is_about_to_show() {
        assert_eq!(
            plan_of(closed(), Event::Open),
            [Announce { opened: true }, Present, Arrive]
        );
    }

    #[test]
    fn open_with_an_enter_transition_is_opened_when_that_has_run() {
        let s = with(closed(), |s| s.has_enter = true);
        assert_eq!(
            plan_of(s, Event::Open),
            [Announce { opened: false }, Present, Arrive]
        );
    }

    #[test]
    fn open_of_an_open_popup_does_nothing() {
        assert!(plan_of(open(), Event::Open).is_empty());
        assert!(plan_of(coming(), Event::Open).is_empty());
    }

    #[test]
    fn open_while_going_comes_back_from_where_it_is() {
        assert_eq!(
            plan_of(going(), Event::Open),
            [
                StopExit,
                ForgetExit,
                Say(Signal::AboutToShow),
                Say(Signal::VisibleChanged),
                TakeKeyboard,
                ShowShade,
                Arrive
            ]
        );
    }

    #[test]
    fn open_of_a_tool_tip_with_a_delay_waits_for_it() {
        let s = tip(with(closed(), |s| s.delays = true));
        assert_eq!(plan_of(s, Event::Open), [Wait]);
        // One that is going comes back at once.
        let s = tip(with(going(), |s| s.delays = true));
        assert_eq!(plan_of(s, Event::Open)[0], StopExit);
    }

    #[test]
    fn close_says_so_and_leaves() {
        assert_eq!(
            plan_of(open(), Event::Close),
            [Say(Signal::AboutToHide), Unopen, Leave]
        );
    }

    #[test]
    fn close_while_coming_goes_from_where_it_has_come_to() {
        assert_eq!(
            plan_of(coming(), Event::Close),
            [StopEnter, Say(Signal::AboutToHide), Unopen, Leave]
        );
    }

    #[test]
    fn close_of_a_closed_or_going_popup_does_nothing() {
        assert!(plan_of(closed(), Event::Close).is_empty());
        assert!(plan_of(going(), Event::Close).is_empty());
    }

    #[test]
    fn close_of_a_tool_tip_that_waits_stops_waiting_and_says_nothing() {
        let s = tip(with(open(), |s| s.waiting = true));
        assert_eq!(plan_of(s, Event::Close), [CancelWait, Hide]);
        // Even if it was only asked to open, and is not visible.
        let s = tip(with(closed(), |s| s.waiting = true));
        assert_eq!(plan_of(s, Event::Close), [CancelWait, Hide]);
    }

    #[test]
    fn dismiss_closes_a_popup_and_rejects_a_dialog() {
        assert_eq!(
            plan_of(open(), Event::Dismiss),
            plan_of(open(), Event::Close)
        );
        let dialog = with(open(), |s| s.kind = PopupKind::Dialog);
        assert_eq!(plan_of(dialog, Event::Dismiss), [Reject]);
    }

    #[test]
    fn dismiss_of_a_going_popup_does_nothing() {
        assert!(plan_of(going(), Event::Dismiss).is_empty());
        let dialog = with(going(), |s| s.kind = PopupKind::Dialog);
        assert!(plan_of(dialog, Event::Dismiss).is_empty());
    }

    #[test]
    fn visible_written_true_says_so_after_the_write() {
        let s = with(closed(), |s| s.visible = true);
        assert_eq!(
            plan_of(s, Event::VisibleWritten(true)),
            [Say(Signal::AboutToShow), Present, Arrive]
        );
    }

    #[test]
    fn visible_written_true_on_a_tool_tip_with_a_delay_waits_for_it() {
        let s = tip(with(closed(), |s| {
            s.visible = true;
            s.delays = true;
        }));
        assert_eq!(plan_of(s, Event::VisibleWritten(true)), [Wait]);
    }

    #[test]
    fn visible_written_false_is_gone_at_once() {
        let s = with(open(), |s| s.visible = false);
        assert_eq!(
            plan_of(s, Event::VisibleWritten(false)),
            [
                StopEnter,
                StopExit,
                ForgetExit,
                Say(Signal::AboutToHide),
                Unopen,
                Withdraw,
                Say(Signal::Closed)
            ]
        );
    }

    #[test]
    fn visible_written_false_on_a_tool_tip_that_waits_stops_waiting() {
        let s = tip(with(closed(), |s| s.waiting = true));
        assert_eq!(plan_of(s, Event::VisibleWritten(false)), [CancelWait]);
    }

    #[test]
    fn declared_open_says_so_after_the_write() {
        let s = with(closed(), |s| s.visible = true);
        assert_eq!(
            plan_of(s, Event::Declared),
            [Say(Signal::AboutToShow), Present, Arrive]
        );
        assert!(plan_of(closed(), Event::Declared).is_empty());
        // A tool tip declared visible does not wait.
        let s = tip(with(closed(), |s| {
            s.visible = true;
            s.delays = true;
        }));
        assert_eq!(plan_of(s, Event::Declared)[0], Say(Signal::AboutToShow));
    }

    #[test]
    fn show_still_is_visible_as_it_says_so_and_runs_no_transition() {
        let s = with(closed(), |s| s.has_enter = true);
        assert_eq!(
            plan_of(s, Event::ShowStill),
            [Announce { opened: false }, Present]
        );
        assert!(plan_of(open(), Event::ShowStill).is_empty());
    }

    #[test]
    fn hold_stops_only_a_popup_that_is_coming() {
        assert_eq!(plan_of(coming(), Event::Hold), [StopEnter]);
        assert!(plan_of(open(), Event::Hold).is_empty());
        assert!(plan_of(going(), Event::Hold).is_empty());
    }

    #[test]
    fn settle_open_stops_what_runs_and_arrives() {
        assert_eq!(
            plan_of(open(), Event::SettleOpen { again: false }),
            [Arrive]
        );
        assert_eq!(
            plan_of(coming(), Event::SettleOpen { again: false }),
            [StopEnter, Arrive]
        );
        assert_eq!(
            plan_of(going(), Event::SettleOpen { again: true }),
            [StopExit, ForgetExit, Say(Signal::AboutToShow), Arrive]
        );
    }

    #[test]
    fn close_at_once_runs_no_transition() {
        assert_eq!(
            plan_of(coming(), Event::CloseAtOnce),
            [
                StopEnter,
                StopExit,
                ForgetExit,
                Say(Signal::AboutToHide),
                Unopen,
                Gone
            ]
        );
        assert!(plan_of(closed(), Event::CloseAtOnce).is_empty());
    }

    #[test]
    fn a_write_of_visible_is_the_popups_to_see_to() {
        // Told to be visible: when closed, or when going.
        assert!(intercepts(&closed(), true));
        assert!(intercepts(&going(), true));
        assert!(!intercepts(&open(), true));
        // A tool tip sees to its delay itself, unless it is going.
        assert!(!intercepts(&tip(closed()), true));
        assert!(intercepts(&tip(going()), true));
        // Told to be invisible: when shown, with an exit to run.
        let exits = with(open(), |s| s.has_exit = true);
        assert!(intercepts(&exits, false));
        assert!(!intercepts(&open(), false));
        assert!(!intercepts(&with(exits, |s| s.shown = false), false));
        assert!(!intercepts(&with(exits, |s| s.visible = false), false));
    }
}
