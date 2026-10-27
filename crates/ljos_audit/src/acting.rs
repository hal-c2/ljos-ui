//! The checks that act on the scene to see what it does: focus an item
//! and look at what changed, flip a control's state, let an animation
//! run. What the rest of the audit reads off one frame cannot say whether
//! a focused control looks focused, or how long a `Behavior` runs.
//!
//! Each check puts back what it changed: focus where it was, with the
//! reason it had, and a state as it was. What it cannot put back is the
//! scene's clock: to see an animation to its end the scene is framed later
//! than `now`. The rest of the audit goes on from the time the checks
//! reached, so that what a later point of the matrix starts runs forward
//! from there, and the report says that time ([`crate::Report::until`]): a
//! host that keeps its own clock moves it there. A `Timer` keeps the
//! engine's clock, which the audit leaves alone, so none fires early.

use std::time::Duration;

use ljos_engine::{Engine, ObjectId, Value, WriteKind};
use ljos_quick::focus::{FocusReason, active_focus_item, focus_reason, force_active_focus};
use ljos_quick::{Item, QuickScene};
use ljos_runtime::Time;

use crate::Options;
use crate::cx::Cx;
use crate::finding::Finding;

/// What an acting check works with.
pub(crate) struct Act<'a> {
    pub engine: &'a Engine,
    pub scene: &'a mut QuickScene,
    pub options: &'a Options,
    /// The scene's clock as the audit has moved it; only ever forwards.
    pub now: Time,
    pub out: Vec<Finding>,
    /// Each open popup closed and opened again, once an audit: the rules
    /// that read a popup's ways out and in share what was seen
    /// ([`crate::motion_watch::reopened`]).
    pub reopened: Option<std::rc::Rc<Vec<crate::motion_watch::Reopened>>>,
}

/// An acting check: the rules it reports and what runs it.
pub(crate) struct Acting {
    pub codes: &'static [&'static str],
    pub run: fn(&mut Act),
}

impl<'a> Act<'a> {
    /// The scene as the reading checks see it.
    pub(crate) fn cx(&self) -> Cx<'_> {
        Cx::new(self.engine, self.scene, self.options)
    }

    /// Adds a finding, named and placed as [`Cx::report`] does it.
    pub(crate) fn report(
        &mut self,
        code: &'static str,
        item: Option<ObjectId>,
        message: String,
    ) -> &mut Finding {
        let mut cx = Cx::new(self.engine, self.scene, self.options);
        cx.report(code, item, message);
        let f = cx.out.pop().expect("just reported");
        self.out.push(f);
        self.out.last_mut().expect("just pushed")
    }

    /// Frames the scene until it waits for nothing at the audit's time.
    pub(crate) fn settle(&mut self) {
        crate::apply::settle(self.engine, self.scene, self.now);
    }

    /// Lets what is animating end before the scene is looked at, so that
    /// a popup is seen open rather than halfway in.
    pub(crate) fn rest(&mut self) {
        if ljos_quick::running_animation_count(self.engine) > 0 || self.scene.needs_frame() {
            self.finish();
        }
    }

    /// Lets every animation the last change started run to its end, or
    /// for as long as the audit waits for one.
    /// One frame that long after the last is enough: each animation steps
    /// by the time that passed, and is at its end.
    pub(crate) fn finish(&mut self) {
        let wait = ljos_design::limits::audit::ANIMATION_WAIT_MS;
        self.now += Duration::from_millis(u64::from(wait));
        self.scene.frame(self.engine, self.now);
        self.settle();
    }
}

/// Where keyboard focus is, to be put back.
pub(crate) struct HeldFocus {
    active: Option<ObjectId>,
    reason: FocusReason,
    /// Every item's `focus`, as the scene shows them.
    flags: Vec<(ObjectId, bool)>,
}

pub(crate) fn hold_focus(act: &Act) -> HeldFocus {
    let e = act.engine;
    let active = act.scene.root().and_then(|r| active_focus_item(e, r));
    let reason = active.map(|i| focus_reason(e, i)).unwrap_or_default();
    let flags = every_item(act)
        .into_iter()
        .map(|i| (i, e.untrack(|| e.read(i, Item::FOCUS.index)).to_boolean()))
        .collect();
    HeldFocus {
        active,
        reason,
        flags,
    }
}

/// Puts focus back as [`hold_focus`] found it: what lost `focus` gets it
/// again, what got it loses it, and the item that had active focus has it
/// once more, for the same reason.
pub(crate) fn restore_focus(act: &mut Act, held: &HeldFocus) {
    let e = act.engine;
    let now = |i: ObjectId| e.untrack(|| e.read(i, Item::FOCUS.index)).to_boolean();
    // Written as the focus chain writes it, not as an assignment: a
    // binding on `focus` (`focus: root.editing`) stays, and still decides
    // once the audit is done.
    for want in [false, true] {
        for (item, was) in &held.flags {
            if *was == want && e.is_alive(*item) && now(*item) != want {
                let _ = e.write(*item, Item::FOCUS.index, Value::Bool(want), WriteKind::Host);
            }
        }
    }
    if let Some(item) = held.active.filter(|i| e.is_alive(*i)) {
        force_active_focus(e, item, held.reason);
    }
    act.settle();
}

/// Every item under the scene's root, popups' too: what focus can be on.
pub(crate) fn every_item(act: &Act) -> Vec<ObjectId> {
    let e = act.engine;
    let mut out = Vec::new();
    let Some(root) = act.scene.root() else {
        return out;
    };
    let mut stack = vec![root];
    while let Some(i) = stack.pop() {
        if out.len() > crate::cx::DEPTH * 16 {
            break;
        }
        if !ljos_quick::is_item(e, i) {
            continue;
        }
        out.push(i);
        stack.extend(ljos_quick::child_items(e, i));
    }
    out
}

/// Runs the acting checks the options ask for, after the reading ones,
/// with the scene's clock at `now`. Returns what they found and the time
/// the scene was last framed at, from which the audit goes on.
pub(crate) fn run(
    engine: &Engine,
    scene: &mut QuickScene,
    now: Time,
    options: &Options,
) -> (Vec<Finding>, Time) {
    if !options.act {
        return (Vec::new(), now);
    }
    let mut act = Act {
        engine,
        scene,
        options,
        now,
        out: Vec::new(),
        reopened: None,
    };
    for check in crate::rules::ACTS {
        if check.codes.iter().any(|c| options.wants(c)) {
            (check.run)(&mut act);
        }
    }
    // Each instance of a delegate or a component says the same of the one
    // place in the source that makes them all: one fix, one finding.
    let mut seen = std::collections::HashSet::new();
    act.out
        .retain(|f| seen.insert((f.code, f.place.clone(), f.message.clone())));
    (act.out, act.now)
}
