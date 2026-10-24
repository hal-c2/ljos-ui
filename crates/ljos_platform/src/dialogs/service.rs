//! The system's dialogs as a service: what a host with native dialogs
//! installs, and what a test stands in with.

use std::path::PathBuf;

use crate::platform::Reply;

/// What kind of dialog is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DialogKind {
    OpenFile,
    OpenFiles,
    SaveFile,
    Folder,
    Message,
    Color,
    Font,
}

impl DialogKind {
    /// How a scenario names it.
    pub fn describe(self) -> &'static str {
        match self {
            DialogKind::OpenFile => "open file",
            DialogKind::OpenFiles => "open files",
            DialogKind::SaveFile => "save file",
            DialogKind::Folder => "folder",
            DialogKind::Message => "message",
            DialogKind::Color => "color",
            DialogKind::Font => "font",
        }
    }
}

/// One entry of `FileDialog.nameFilters`: `"Text files (*.txt *.md)"` is
/// named `Text files` and has the patterns `*.txt` and `*.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameFilter {
    pub name: String,
    pub patterns: Vec<String>,
}

impl NameFilter {
    pub fn parse(filter: &str) -> NameFilter {
        let filter = filter.trim();
        match filter
            .rfind('(')
            .filter(|_| filter.ends_with(')'))
            .map(|open| (&filter[..open], &filter[open + 1..filter.len() - 1]))
        {
            Some((name, patterns)) => NameFilter {
                name: name.trim().to_owned(),
                patterns: patterns.split_whitespace().map(str::to_owned).collect(),
            },
            None => NameFilter {
                name: filter.to_owned(),
                patterns: filter.split_whitespace().map(str::to_owned).collect(),
            },
        }
    }

    /// The extensions of the patterns that are `*.ext`, which is all a
    /// native file dialog filters by. A filter of `*` has none.
    pub fn extensions(&self) -> Vec<String> {
        self.patterns
            .iter()
            .filter_map(|p| p.strip_prefix("*."))
            .filter(|e| !e.is_empty() && !e.contains(['*', '?', '[']))
            .map(str::to_owned)
            .collect()
    }

    /// Whether a pattern takes every file: `*`, or `*.*`, which file
    /// dialogs read the same way.
    pub fn takes_every_file(&self) -> bool {
        self.patterns.iter().any(|p| p == "*" || p == "*.*")
    }
}

/// What a dialog is to show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialogRequest {
    pub kind: Option<DialogKind>,
    pub title: String,
    /// The folder a file or folder dialog starts in.
    pub folder: Option<PathBuf>,
    /// The name a save dialog suggests.
    pub file_name: Option<String>,
    pub filters: Vec<NameFilter>,
    pub accept_label: String,
    /// A message's text, informative text and details.
    pub text: String,
    pub informative_text: String,
    pub detailed_text: String,
    /// A message's buttons, as `MessageDialog.StandardButtons`.
    pub buttons: i32,
}

/// How the user ended a dialog the system showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogReply {
    /// The files or the folder chosen, as absolute paths.
    Files(Vec<PathBuf>),
    /// The button pressed, as a `MessageDialog.StandardButton`.
    Button(i32),
    /// Closed without a choice.
    Dismissed,
}

/// Where the answer to one dialog goes. Sent to the thread that waits on
/// the system; dropping it unanswered counts as a dismissal.
#[derive(Debug)]
pub struct DialogAnswer {
    pub(crate) reply: Option<Reply>,
    pub(crate) ticket: u64,
}

impl DialogAnswer {
    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    pub fn send(mut self, answer: DialogReply) {
        if let Some(reply) = self.reply.take() {
            let ticket = self.ticket;
            reply.send(move |e| super::system_answered(e, ticket, answer));
        }
    }
}

impl Drop for DialogAnswer {
    fn drop(&mut self) {
        if let Some(reply) = self.reply.take() {
            let ticket = self.ticket;
            reply.send(move |e| super::system_answered(e, ticket, DialogReply::Dismissed));
        }
    }
}

/// The system's dialogs.
pub trait DialogService {
    /// Whether the system shows this dialog. When it does not, the
    /// application draws one of its own.
    fn can_show(&self, request: &DialogRequest) -> bool;

    /// Shows the dialog without waiting for it; the answer comes through
    /// `answer`, from any thread.
    fn show(&self, request: DialogRequest, answer: DialogAnswer);

    /// Takes away a dialog QML closed. Its answer, if one still comes, is
    /// not heard. A system that cannot take a dialog away does nothing: the
    /// QML dialog is closed and rejected either way, and the system's stays
    /// on screen until the user ends it.
    fn close(&self, ticket: u64);
}

/// The dialogs of an application whose system has none: every dialog is
/// drawn by the application.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoSystemDialogs;

impl DialogService for NoSystemDialogs {
    fn can_show(&self, _request: &DialogRequest) -> bool {
        false
    }

    fn show(&self, _request: DialogRequest, _answer: DialogAnswer) {}

    fn close(&self, _ticket: u64) {}
}

/// The buttons of a message box, as Qt names them and as they read in
/// English, and the role each has.
pub const STANDARD_BUTTONS: [(i32, &str, &str, i32); 18] = [
    (0x0000_0400, "Ok", "OK", role::ACCEPT),
    (0x0000_0800, "Save", "Save", role::ACCEPT),
    (0x0000_1000, "SaveAll", "Save All", role::ACCEPT),
    (0x0000_2000, "Open", "Open", role::ACCEPT),
    (0x0000_4000, "Yes", "Yes", role::YES),
    (0x0000_8000, "YesToAll", "Yes to All", role::YES),
    (0x0001_0000, "No", "No", role::NO),
    (0x0002_0000, "NoToAll", "No to All", role::NO),
    (0x0004_0000, "Abort", "Abort", role::REJECT),
    (0x0008_0000, "Retry", "Retry", role::ACCEPT),
    (0x0010_0000, "Ignore", "Ignore", role::ACCEPT),
    (0x0020_0000, "Close", "Close", role::REJECT),
    (0x0040_0000, "Cancel", "Cancel", role::REJECT),
    // What QtQuick.Controls' DialogButtonBox calls it, which the drawn
    // message uses.
    (
        0x0080_0000,
        "Discard",
        "Close without Saving",
        role::DESTRUCTIVE,
    ),
    (0x0100_0000, "Help", "Help", role::HELP),
    (0x0200_0000, "Apply", "Apply", role::APPLY),
    (0x0400_0000, "Reset", "Reset", role::RESET),
    (
        0x0800_0000,
        "RestoreDefaults",
        "Restore Defaults",
        role::RESET,
    ),
];

/// `MessageDialog.ButtonRole`.
pub mod role {
    pub const INVALID: i32 = -1;
    pub const ACCEPT: i32 = 0;
    pub const REJECT: i32 = 1;
    pub const DESTRUCTIVE: i32 = 2;
    pub const ACTION: i32 = 3;
    pub const HELP: i32 = 4;
    pub const YES: i32 = 5;
    pub const NO: i32 = 6;
    pub const RESET: i32 = 7;
    pub const APPLY: i32 = 8;
}

pub const OK: i32 = 0x0000_0400;
pub const YES: i32 = 0x0000_4000;
pub const NO: i32 = 0x0001_0000;
pub const CANCEL: i32 = 0x0040_0000;

/// The role of a standard button.
pub fn button_role(button: i32) -> i32 {
    STANDARD_BUTTONS
        .iter()
        .find(|(flag, ..)| *flag == button)
        .map_or(role::INVALID, |b| b.3)
}

/// The button whose English text is `text`, or whose Qt name is.
pub fn button_named(text: &str) -> Option<i32> {
    STANDARD_BUTTONS
        .iter()
        .find(|(_, name, label, _)| *label == text || *name == text)
        .map(|b| b.0)
}

/// The sets of buttons every system's message box has: OK, OK and Cancel,
/// Yes and No, Yes, No and Cancel. A message with others is drawn by the
/// application.
pub fn system_buttons(buttons: i32) -> bool {
    [OK, OK | CANCEL, YES | NO, YES | NO | CANCEL].contains(&buttons)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_filter_has_a_name_and_patterns() {
        let f = NameFilter::parse("Text files (*.txt *.md)");
        assert_eq!(f.name, "Text files");
        assert_eq!(f.patterns, vec!["*.txt", "*.md"]);
        assert_eq!(f.extensions(), vec!["txt", "md"]);
        assert!(NameFilter::parse("All Files (*)").extensions().is_empty());
        assert_eq!(NameFilter::parse("*.png").extensions(), vec!["png"]);
    }

    #[test]
    fn a_filter_of_star_or_star_dot_star_takes_every_file() {
        assert!(NameFilter::parse("All files (*)").takes_every_file());
        assert!(NameFilter::parse("Anything (*.txt *.*)").takes_every_file());
        assert!(!NameFilter::parse("Images (*.png)").takes_every_file());
        assert!(!NameFilter::parse("plain").takes_every_file());
    }

    #[test]
    fn buttons_are_found_by_text_and_have_roles() {
        assert_eq!(button_named("OK"), Some(OK));
        assert_eq!(button_named("Cancel"), Some(CANCEL));
        assert_eq!(button_named("Discard"), Some(0x0080_0000));
        assert_eq!(button_role(CANCEL), role::REJECT);
        assert_eq!(button_role(0x0080_0000), role::DESTRUCTIVE);
        assert_eq!(button_role(3), role::INVALID);
        assert!(system_buttons(OK | CANCEL));
        assert!(!system_buttons(0x0800 | 0x0080_0000 | CANCEL));
    }
}
