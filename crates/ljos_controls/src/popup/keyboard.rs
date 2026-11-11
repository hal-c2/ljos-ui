//! Who gets the keyboard when a popup that has it goes, as bookkeeping.
//!
//! An overlay keeps one [`Ledger`]: the popups that took the keyboard as
//! they opened, in the order they took it, and the item of the window's
//! content that had it last. When a popup that has the keyboard goes,
//! whether it took it or was given it (an item in it was given focus),
//! the keyboard goes to the last popup in the ledger that is still there,
//! or else back to that item of the content. It never stays in a popup
//! that has gone, and never goes back into one that did not ask for it
//! (measured on Qt 6.11.2 and 6.12.0, `popup-keyboard.feature` and
//! `popup-focus.feature`).
//!
//! A popup leaves the ledger as it starts to go, so one on its way out
//! never takes the keyboard back. `mod.rs` asks and acts on the answer;
//! nothing here knows the engine.

/// Where the keyboard goes when a popup goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Give<Id> {
    /// The popup did not have it: it stays where it is.
    Keep,
    /// To this popup, which took it before and is still open.
    Popup(Id),
    /// Back to this item of the window's content.
    Content(Id),
    /// Nowhere is left: the popup only lets go of it.
    Nobody,
}

/// The keyboard of one overlay. `Id` is an object; tests use numbers.
#[derive(Debug)]
pub(super) struct Ledger<Id> {
    took: Vec<Id>,
    content: Option<Id>,
}

impl<Id> Default for Ledger<Id> {
    fn default() -> Self {
        Self {
            took: Vec::new(),
            content: None,
        }
    }
}

impl<Id: Copy + Eq> Ledger<Id> {
    /// `item`, in the window's content and not in a popup, has the
    /// keyboard now.
    pub(super) fn content_has(&mut self, item: Id) {
        self.content = Some(item);
    }

    /// `popup` takes the keyboard as it opens. False when it already had.
    pub(super) fn take(&mut self, popup: Id) -> bool {
        if self.took.contains(&popup) {
            return false;
        }
        self.took.push(popup);
        true
    }

    /// Whether `popup` took the keyboard and has not gone since.
    pub(super) fn holds(&self, popup: Id) -> bool {
        self.took.contains(&popup)
    }

    /// `popup` goes; `had` says whether it has the keyboard. `alive` is
    /// asked of the popups and the item the keyboard could go to: a
    /// popup destroyed without going is passed over.
    pub(super) fn leave(&mut self, popup: Id, had: bool, alive: impl Fn(Id) -> bool) -> Give<Id> {
        self.took.retain(|p| *p != popup && alive(*p));
        if !had {
            return Give::Keep;
        }
        if let Some(p) = self.took.last() {
            return Give::Popup(*p);
        }
        match self.content.filter(|c| alive(*c)) {
            Some(c) => Give::Content(c),
            None => Give::Nobody,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Give, Ledger};

    /// Where the keyboard is in a test window.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum At {
        Content(u32),
        Popup(u32),
        Nowhere,
    }

    /// What a test does, in the words of the feature files.
    #[derive(Clone, Copy, Debug)]
    enum Step {
        /// A popup opens; `true` when it wants focus.
        Open(u32, bool),
        /// An item of the content is given focus.
        FocusContent(u32),
        /// An item in a popup is given focus.
        FocusIn(u32),
        Close(u32),
    }
    use Step::*;

    /// The content's first item, which has the keyboard at the start.
    const OUTSIDE: u32 = 100;
    const OTHER: u32 = 101;
    /// Popups: `p`, `plain` and `third` of the feature files.
    const P: u32 = 1;
    const PLAIN: u32 = 2;
    const THIRD: u32 = 3;

    fn run(steps: &[Step]) -> At {
        let mut ledger = Ledger::default();
        ledger.content_has(OUTSIDE);
        let mut at = At::Content(OUTSIDE);
        let mut open: Vec<u32> = Vec::new();
        for &step in steps {
            match step {
                Open(p, wants) => {
                    open.push(p);
                    if wants && ledger.take(p) {
                        at = At::Popup(p);
                    }
                }
                FocusContent(c) => {
                    ledger.content_has(c);
                    at = At::Content(c);
                }
                FocusIn(p) => at = At::Popup(p),
                Close(p) => {
                    open.retain(|o| *o != p);
                    let alive = |id: u32| id >= OUTSIDE || open.contains(&id);
                    match ledger.leave(p, at == At::Popup(p), alive) {
                        Give::Keep => {}
                        Give::Popup(q) => at = At::Popup(q),
                        Give::Content(c) => at = At::Content(c),
                        Give::Nobody => at = At::Nowhere,
                    }
                }
            }
        }
        at
    }

    /// The scenarios of `popup-focus.feature` and `popup-keyboard.feature`
    /// that move the keyboard between popups, and where it ends.
    const ROWS: &[(&str, &[Step], At)] = &[
        (
            "one popup",
            &[Open(P, true), Close(P)],
            At::Content(OUTSIDE),
        ),
        (
            "a popup that does not want focus",
            &[Open(PLAIN, false)],
            At::Content(OUTSIDE),
        ),
        (
            "the second closes",
            &[Open(P, true), Open(PLAIN, true), Close(PLAIN)],
            At::Popup(P),
        ),
        (
            "the first closes under the second",
            &[Open(P, true), Open(PLAIN, true), Close(P)],
            At::Popup(PLAIN),
        ),
        (
            "out of order",
            &[Open(P, true), Open(PLAIN, true), Close(P), Close(PLAIN)],
            At::Content(OUTSIDE),
        ),
        (
            "given the keyboard, then closed",
            &[Open(PLAIN, false), FocusIn(PLAIN), Close(PLAIN)],
            At::Content(OUTSIDE),
        ),
        (
            "given the keyboard over one that took it",
            &[
                Open(P, true),
                Open(PLAIN, false),
                FocusIn(PLAIN),
                Close(PLAIN),
            ],
            At::Popup(P),
        ),
        (
            "one that took it closed beneath",
            &[
                Open(P, true),
                Open(PLAIN, false),
                FocusIn(PLAIN),
                Close(P),
                Close(PLAIN),
            ],
            At::Content(OUTSIDE),
        ),
        (
            "taken from one that was given it",
            &[Open(PLAIN, false), FocusIn(PLAIN), Open(P, true), Close(P)],
            At::Content(OUTSIDE),
        ),
        (
            "taken from one that closed meanwhile",
            &[
                Open(PLAIN, false),
                FocusIn(PLAIN),
                Open(P, true),
                Close(PLAIN),
                Close(P),
            ],
            At::Content(OUTSIDE),
        ),
        (
            "the content moved first",
            &[
                FocusContent(OTHER),
                Open(PLAIN, false),
                FocusIn(PLAIN),
                Open(P, true),
                Close(P),
            ],
            At::Content(OTHER),
        ),
        (
            "the content took it back",
            &[Open(P, true), FocusContent(OUTSIDE), Close(P)],
            At::Content(OUTSIDE),
        ),
        (
            "the middle of three, then the top",
            &[
                Open(P, true),
                Open(PLAIN, true),
                Open(THIRD, true),
                Close(PLAIN),
                Close(THIRD),
            ],
            At::Popup(P),
        ),
        (
            "the bottom of three, then the top",
            &[
                Open(P, true),
                Open(PLAIN, true),
                Open(THIRD, true),
                Close(P),
                Close(THIRD),
            ],
            At::Popup(PLAIN),
        ),
        (
            "the two beneath, then the top",
            &[
                Open(P, true),
                Open(PLAIN, true),
                Open(THIRD, true),
                Close(P),
                Close(PLAIN),
                Close(THIRD),
            ],
            At::Content(OUTSIDE),
        ),
    ];

    #[test]
    fn the_keyboard_goes_to_the_last_popup_that_took_it_or_the_content() {
        for (name, steps, want) in ROWS {
            assert_eq!(run(steps), *want, "{name}");
        }
    }

    #[test]
    fn a_popup_takes_the_keyboard_once() {
        let mut l = Ledger::default();
        assert!(l.take(P));
        assert!(!l.take(P));
        assert!(l.holds(P));
        assert_eq!(l.leave(P, true, |_| true), Give::Nobody);
        assert!(!l.holds(P));
    }

    #[test]
    fn a_destroyed_popup_or_item_is_passed_over() {
        let mut l = Ledger::default();
        l.content_has(OUTSIDE);
        l.take(P);
        l.take(PLAIN);
        assert_eq!(l.leave(PLAIN, true, |id| id != P), Give::Content(OUTSIDE));
        let mut l = Ledger::default();
        l.content_has(OUTSIDE);
        l.take(P);
        assert_eq!(l.leave(P, true, |id| id != OUTSIDE), Give::Nobody);
    }
}
