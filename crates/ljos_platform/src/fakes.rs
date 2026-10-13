//! Stand-ins for the system, for tests: a system that shows the dialogs a
//! desktop shows (files, folders and the common messages) and lets the
//! test answer them as its user.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::dialogs::{
    DialogAnswer, DialogKind, DialogReply, DialogRequest, DialogService, button_named,
    system_buttons,
};

/// A dialog the fake system is showing.
#[derive(Debug)]
struct Showing {
    request: DialogRequest,
    answer: DialogAnswer,
}

/// The system's dialogs, answered by a test. Clones share what is shown.
#[derive(Clone, Debug, Default)]
pub struct FakeDialogs {
    showing: Rc<RefCell<Vec<Showing>>>,
    /// Whether the system ignores being told to take a dialog away, as
    /// rfd's does.
    stays: Rc<Cell<bool>>,
}

impl FakeDialogs {
    /// From now on the system cannot take away a dialog it shows: one the
    /// application closes stays until the user answers it.
    pub fn cannot_close(&self) {
        self.stays.set(true);
    }

    /// The kind and title of each dialog the system is showing, oldest
    /// first.
    pub fn shown(&self) -> Vec<(DialogKind, String)> {
        self.showing
            .borrow()
            .iter()
            .filter_map(|s| Some((s.request.kind?, s.request.title.clone())))
            .collect()
    }

    /// The request of the newest dialog shown.
    pub fn request(&self) -> Option<DialogRequest> {
        self.showing.borrow().last().map(|s| s.request.clone())
    }

    fn take(&self) -> Option<DialogAnswer> {
        self.showing.borrow_mut().pop().map(|s| s.answer)
    }

    /// The user chooses these paths in the newest dialog. `false` when no
    /// dialog is shown.
    pub fn choose(&self, paths: Vec<PathBuf>) -> bool {
        self.take()
            .map(|a| a.send(DialogReply::Files(paths)))
            .is_some()
    }

    /// The user presses the button with this text (`"OK"`) in the newest
    /// dialog. `false` when no dialog is shown or it has no such button.
    pub fn answer(&self, button: &str) -> bool {
        let Some(button) = button_named(button) else {
            return false;
        };
        self.take()
            .map(|a| a.send(DialogReply::Button(button)))
            .is_some()
    }

    /// The user closes the newest dialog without choosing.
    pub fn dismiss(&self) -> bool {
        self.take()
            .map(|a| a.send(DialogReply::Dismissed))
            .is_some()
    }
}

impl DialogService for FakeDialogs {
    fn can_show(&self, request: &DialogRequest) -> bool {
        // What a desktop's portal shows: no colour or font chooser, and a
        // message only with the buttons every system has.
        match request.kind {
            Some(DialogKind::Message) => system_buttons(request.buttons),
            Some(DialogKind::Color | DialogKind::Font) | None => false,
            Some(_) => true,
        }
    }

    fn show(&self, request: DialogRequest, answer: DialogAnswer) {
        self.showing.borrow_mut().push(Showing { request, answer });
    }

    fn close(&self, ticket: u64) {
        if self.stays.get() {
            return;
        }
        let mut showing = self.showing.borrow_mut();
        if let Some(i) = showing.iter().position(|s| s.answer.ticket() == ticket) {
            let closed = showing.remove(i);
            drop(showing);
            // Its dismissal goes nowhere: the application has already
            // ended the dialog its own way and forgotten the ticket.
            drop(closed);
        }
    }
}
