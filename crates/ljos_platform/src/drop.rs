//! Drags between applications: the seam between a host's window system
//! and the application's drag-and-drop.
//!
//! A host turns what its window system says of a drag from another
//! application into [`ExternalDrag`]s and hands them to [`external_drag`]:
//! whatever the drag offers, as [`DragData`] (files, other URLs, text, HTML,
//! any MIME type), with the actions its source allows and the one it would
//! like ([`DragActions`]). The target decides, and the action it chose is
//! what the host tells the source. The drag-and-drop of the items
//! (`DropArea`) installs itself as the handler
//! ([`crate::Platform::set_drop_handler`]) and treats them as drags that
//! came from outside.
//!
//! A host whose window system carries drags out of the application
//! installs a [`DragOutService`] ([`crate::Platform::set_drag_out`]): an
//! item's automatic drag ([`OutgoingDrag`]) is handed to it, and the host
//! says later which action the application it was let go over took. Such
//! a drag that comes back over the application's own window comes in as
//! any other, and the drag-and-drop knows it for its own.
//!
//! A host whose window system brings no drops from outside at all says so
//! ([`Platform::set_external_drops_unavailable`]), and the first drop
//! target that would take files is warned of once
//! ([`Platform::note_external_drop_target`]).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use ljos_engine::{Engine, LogLevel, ObjectId};

use crate::platform::{CATEGORY, Platform};

/// What a drag carries, by MIME type.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragData {
    /// Files and other addresses (`text/uri-list`).
    pub urls: Vec<String>,
    /// `text/plain`.
    pub text: Option<String>,
    /// `text/html`.
    pub html: Option<String>,
    /// Anything else, as text, by MIME type.
    pub other: Vec<(String, String)>,
}

impl DragData {
    /// Files, as a file manager drags them.
    pub fn files<I: IntoIterator<Item = PathBuf>>(paths: I) -> Self {
        Self {
            urls: paths
                .into_iter()
                .map(|p| ljos_engine::file_url(&p))
                .collect(),
            ..Self::default()
        }
    }

    /// Addresses, as a browser drags a link.
    pub fn urls<I: IntoIterator<Item = S>, S: Into<String>>(urls: I) -> Self {
        Self {
            urls: urls.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Text, as an editor drags a selection.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    /// What a window system offers: the bytes of each MIME type, in the
    /// order the source gave them. The names X11 gives plain text
    /// (`UTF8_STRING`, `STRING`, `TEXT`) and `text/plain;charset=utf-8`
    /// are `text/plain`; the first of them wins.
    pub fn from_formats<I, F, B>(formats: I) -> Self
    where
        I: IntoIterator<Item = (F, B)>,
        F: AsRef<str>,
        B: AsRef<[u8]>,
    {
        let mut data = Self::default();
        for (format, bytes) in formats {
            let (format, bytes) = (format.as_ref(), bytes.as_ref());
            let text = || decode(bytes);
            match format.to_ascii_lowercase().as_str() {
                "text/uri-list" => data.urls = uri_list(&text()),
                "text/plain" | "text/plain;charset=utf-8" | "utf8_string" | "string" | "text" => {
                    if data.text.is_none() {
                        data.text = Some(text());
                    }
                }
                "text/html" => data.html = Some(text()),
                _ if data.other.iter().any(|(k, _)| k == format) => {}
                _ => data.other.push((format.to_owned(), text())),
            }
        }
        data
    }

    /// The MIME types it has, as `DragEvent.formats` and `keys` tell
    /// them.
    pub fn formats(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.urls.is_empty() {
            out.push("text/uri-list".to_string());
        }
        if self.text.is_some() {
            out.push("text/plain".to_string());
        }
        if self.html.is_some() {
            out.push("text/html".to_string());
        }
        out.extend(self.other.iter().map(|(k, _)| k.clone()));
        out
    }

    /// The data of one MIME type, as text.
    pub fn data(&self, format: &str) -> Option<String> {
        match format {
            // The list leaves the process, so each address in it is written
            // the strict way a reader can parse without guessing.
            "text/uri-list" if !self.urls.is_empty() => Some(
                self.urls
                    .iter()
                    .map(|u| ljos_engine::encoded_url(u))
                    .collect::<Vec<_>>()
                    .join("\r\n"),
            ),
            "text/plain" => self.text.clone(),
            "text/html" => self.html.clone(),
            _ => self
                .other
                .iter()
                .find(|(k, _)| k == format)
                .map(|(_, v)| v.clone()),
        }
    }
}

/// `text/uri-list` (RFC 2483): one address a line, `#` lines are comments.
pub fn uri_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Text a source wrote: UTF-8, or UTF-16 when it starts with a byte order
/// mark (as browsers write `text/html`). A NUL some sources end with is
/// not part of it.
fn decode(bytes: &[u8]) -> String {
    let utf16 = |le: bool| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if le {
                    u16::from_le_bytes(c)
                } else {
                    u16::from_be_bytes(c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    let text = match bytes {
        [0xFF, 0xFE, ..] => utf16(true),
        [0xFE, 0xFF, ..] => utf16(false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    text.trim_end_matches('\0').to_owned()
}

/// What a drop does with what it gets, numbered as `Qt.DropAction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DropAction {
    Copy = 1,
    Move = 2,
    Link = 4,
}

impl DropAction {
    pub const ALL: [DropAction; 3] = [DropAction::Copy, DropAction::Move, DropAction::Link];

    /// Its `Qt.DropAction` number.
    pub fn qt(self) -> i32 {
        self as i32
    }

    /// The action a `Qt.DropAction` number names; `None` for
    /// `Qt.IgnoreAction` and anything that is not one action.
    pub fn from_qt(n: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.qt() == n)
    }

    /// `copy`, `move` or `link`.
    pub fn name(self) -> &'static str {
        match self {
            DropAction::Copy => "copy",
            DropAction::Move => "move",
            DropAction::Link => "link",
        }
    }

    /// The action [`DropAction::name`] gives `name`.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }
}

/// Actions, as `Qt.DropActions` sets them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DropActions(u8);

impl DropActions {
    pub const NONE: DropActions = DropActions(0);
    pub const ALL: DropActions = DropActions(1 | 2 | 4);

    /// The actions of a `Qt.DropActions` number; other bits are left out.
    pub fn from_qt(n: i32) -> Self {
        Self((n & Self::ALL.qt()) as u8)
    }

    pub fn qt(self) -> i32 {
        i32::from(self.0)
    }

    pub fn contains(self, action: DropAction) -> bool {
        self.qt() & action.qt() != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Its actions: copy, then move, then link.
    pub fn iter(self) -> impl Iterator<Item = DropAction> {
        DropAction::ALL
            .into_iter()
            .filter(move |a| self.contains(*a))
    }
}

impl From<DropAction> for DropActions {
    fn from(action: DropAction) -> Self {
        Self(action.qt() as u8)
    }
}

impl FromIterator<DropAction> for DropActions {
    fn from_iter<I: IntoIterator<Item = DropAction>>(iter: I) -> Self {
        Self(iter.into_iter().fold(0, |bits, a| bits | a.qt() as u8))
    }
}

/// The actions a drag's source allows, and the one it would like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragActions {
    pub allowed: DropActions,
    pub proposed: DropAction,
}

impl Default for DragActions {
    /// A source that says nothing of actions: any, a copy preferred.
    fn default() -> Self {
        Self {
            allowed: DropActions::ALL,
            proposed: DropAction::Copy,
        }
    }
}

impl DragActions {
    /// What the source is told of a target that chose `chosen`. An action
    /// it does not allow is not one the target can take: as Qt's
    /// `QDropEvent` takes only "one of the possible actions", that leaves
    /// the proposed one, else the first it allows.
    pub fn told(self, chosen: Option<DropAction>) -> Option<DropAction> {
        let chosen = chosen?;
        if self.allowed.contains(chosen) {
            Some(chosen)
        } else if self.allowed.contains(self.proposed) {
            Some(self.proposed)
        } else {
            self.allowed.iter().next()
        }
    }
}

/// A drag from another application, as a host reports it.
#[derive(Clone, Debug, PartialEq)]
pub enum ExternalDrag {
    /// Something is dragged over the window, at `at` in the window's
    /// logical coordinates when the host knows where.
    Entered {
        data: DragData,
        at: Option<(f64, f64)>,
        actions: DragActions,
    },
    /// The drag moved, or its source now allows or would like other
    /// actions (`actions`, when it says so), as a modifier key changes
    /// them.
    Moved {
        at: (f64, f64),
        actions: Option<DragActions>,
    },
    /// The drag left the window, or was given up.
    Left,
    /// It was let go over the window. A host that only learns what was
    /// dragged when it is let go says so here; otherwise `data` is what
    /// `Entered` said.
    Dropped {
        data: DragData,
        at: Option<(f64, f64)>,
    },
}

impl ExternalDrag {
    /// Files dragged in, as a file manager drags them.
    pub fn files_entered(files: impl IntoIterator<Item = PathBuf>, at: Option<(f64, f64)>) -> Self {
        ExternalDrag::Entered {
            data: DragData::files(files),
            at,
            actions: DragActions::default(),
        }
    }

    /// The drag moved to `at`.
    pub fn moved(at: (f64, f64)) -> Self {
        ExternalDrag::Moved { at, actions: None }
    }

    /// Files let go.
    pub fn files_dropped(files: impl IntoIterator<Item = PathBuf>, at: Option<(f64, f64)>) -> Self {
        ExternalDrag::Dropped {
            data: DragData::files(files),
            at,
        }
    }

    /// The addresses it carries, the way `drop.urls` has them.
    pub fn urls(&self) -> Vec<String> {
        match self {
            ExternalDrag::Entered { data, .. } | ExternalDrag::Dropped { data, .. } => {
                data.urls.clone()
            }
            _ => Vec::new(),
        }
    }
}

/// Whether drops from outside can come, and whether that was said.
#[derive(Debug, Default)]
pub(crate) struct ExternalDrops {
    /// Why they cannot, as the host says it; none while they can.
    unavailable: Option<String>,
    /// A drop target that would take files from outside exists.
    target: bool,
    warned: bool,
}

impl ExternalDrops {
    /// The warning that is due now, once: when there is a target and
    /// drops cannot come, in whichever order the two were learned.
    fn due(&mut self) -> Option<String> {
        let why = self
            .unavailable
            .as_deref()
            .filter(|_| self.target && !self.warned)?;
        self.warned = true;
        Some(format!(
            "files dragged in from other applications cannot reach a DropArea: {why}"
        ))
    }
}

impl Platform {
    /// The host's window system brings no files dragged in from other
    /// applications, for `reason` (what to run under instead, if
    /// anything). A drop target that would take them is warned of once.
    pub fn set_external_drops_unavailable(&self, engine: &Engine, reason: impl Into<String>) {
        let due = {
            let mut drops = self.external_drops.borrow_mut();
            drops.unavailable = Some(reason.into());
            drops.due()
        };
        warn(engine, due);
    }

    /// A drop target that would take files from other applications was
    /// made: the drag-and-drop of the items says so of each.
    pub fn note_external_drop_target(&self, engine: &Engine) {
        let due = {
            let mut drops = self.external_drops.borrow_mut();
            drops.target = true;
            drops.due()
        };
        warn(engine, due);
    }
}

// Logged with no borrow held: a log handler may reach the platform.
fn warn(engine: &Engine, due: Option<String>) {
    if let Some(message) = due {
        engine.log(LogLevel::Warning, CATEGORY, message);
    }
}

/// Hands a drag from outside to the application. Returns the action the
/// target took, which the host tells the source: for `Entered` and
/// `Moved`, the one a target under the drag takes it with now (a drop
/// there would go to it); for `Dropped`, the one it took the drop with.
/// `None` when no target takes it; with no drag-and-drop installed none
/// does.
pub fn external_drag(engine: &Engine, drag: &ExternalDrag) -> Option<DropAction> {
    match Platform::of(engine).drop_handler() {
        Some(handler) => handler(engine, drag),
        None => {
            if matches!(drag, ExternalDrag::Dropped { .. }) {
                engine.log(
                    LogLevel::Debug,
                    CATEGORY,
                    "a drop from another application found no drop target",
                );
            }
            None
        }
    }
}

/// A drag out of the application, as the host's window system is asked
/// to carry it.
#[derive(Clone, Debug, PartialEq)]
pub struct OutgoingDrag {
    /// What it offers, by MIME type.
    pub data: DragData,
    pub actions: DragActions,
    /// The window it starts from, as the top item of its tree.
    pub window: Option<ObjectId>,
}

/// How a drag out of the application ended: told once, on the Engine
/// thread, with the action the application it was let go over took;
/// `None` when none took it.
pub type DragOutDone = Box<dyn FnOnce(&Engine, Option<DropAction>)>;

/// A host's way of carrying drags out of the application.
pub trait DragOutService {
    /// Hands `drag` to the window system. `false` when it cannot carry it
    /// now (no press to start it from, or another drag under way), and
    /// `done` goes untold. Otherwise `done` is told when the drag ends,
    /// never from inside `start`: the application is in the middle of
    /// starting it.
    fn start(&self, engine: &Engine, drag: OutgoingDrag, done: DragOutDone) -> bool;

    /// The application gives the drag under way up. Its `done` goes
    /// untold: the application ended the drag itself.
    fn cancel(&self, engine: &Engine);
}

/// Carries nothing and keeps the drag it was asked to carry, until the
/// test ends it with [`RecordedDragOut::end`]. Its clones share one
/// record, so a test keeps one and installs another.
#[derive(Clone, Default)]
pub struct RecordedDragOut {
    record: Rc<RefCell<DragOutRecord>>,
}

#[derive(Default)]
struct DragOutRecord {
    asked: Option<OutgoingDrag>,
    done: Option<DragOutDone>,
    given_up: bool,
    started: usize,
}

impl std::fmt::Debug for RecordedDragOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let r = self.record.borrow();
        f.debug_struct("RecordedDragOut")
            .field("asked", &r.asked)
            .field("under_way", &r.done.is_some())
            .field("given_up", &r.given_up)
            .finish()
    }
}

impl RecordedDragOut {
    /// The last drag it was asked to carry.
    pub fn asked(&self) -> Option<OutgoingDrag> {
        self.record.borrow().asked.clone()
    }

    /// The application gave a drag up.
    pub fn given_up(&self) -> bool {
        self.record.borrow().given_up
    }

    /// How many drags it has carried, so that a host knows when one took
    /// the pointer.
    pub fn started(&self) -> usize {
        self.record.borrow().started
    }

    /// The drag under way ends, taken with `action` (`None`: turned
    /// down). `false` when none is under way.
    pub fn end(&self, engine: &Engine, action: Option<DropAction>) -> bool {
        // Taken out first: the application may start another drag.
        let done = self.record.borrow_mut().done.take();
        match done {
            Some(done) => {
                done(engine, action);
                true
            }
            None => false,
        }
    }
}

impl DragOutService for RecordedDragOut {
    fn start(&self, _engine: &Engine, drag: OutgoingDrag, done: DragOutDone) -> bool {
        let mut r = self.record.borrow_mut();
        if r.done.is_some() {
            return false;
        }
        r.asked = Some(drag);
        r.done = Some(done);
        r.started += 1;
        true
    }

    fn cancel(&self, _engine: &Engine) {
        let mut r = self.record.borrow_mut();
        if r.done.take().is_some() {
            r.given_up = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy_or_move(proposed: DropAction) -> DragActions {
        DragActions {
            allowed: [DropAction::Copy, DropAction::Move].into_iter().collect(),
            proposed,
        }
    }

    #[test]
    fn the_action_told_is_one_the_source_allows() {
        let a = copy_or_move(DropAction::Move);
        assert_eq!(a.told(Some(DropAction::Copy)), Some(DropAction::Copy));
        assert_eq!(a.told(Some(DropAction::Link)), Some(DropAction::Move));
        assert_eq!(a.told(None), None);
        // A proposal it does not allow itself gives the first it does.
        let odd = DragActions {
            allowed: DropActions::from(DropAction::Move),
            proposed: DropAction::Copy,
        };
        assert_eq!(odd.told(Some(DropAction::Link)), Some(DropAction::Move));
    }

    #[test]
    fn actions_are_qt_flags() {
        let all: DropActions = DropAction::ALL.into_iter().collect();
        assert_eq!(all.qt(), 7);
        assert_eq!(
            DropActions::from_qt(5).iter().collect::<Vec<_>>(),
            [DropAction::Copy, DropAction::Link]
        );
        assert_eq!(DropAction::named("move"), Some(DropAction::Move));
        assert_eq!(DropAction::from_qt(4).map(DropAction::name), Some("link"));
    }

    #[test]
    fn a_recorded_drag_out_ends_once_and_not_after_it_is_given_up() {
        let engine = Engine::new();
        let out = RecordedDragOut::default();
        let told = Rc::new(RefCell::new(Vec::new()));
        let start = |out: &RecordedDragOut| {
            let told = told.clone();
            out.start(
                &engine,
                OutgoingDrag {
                    data: DragData::text("hi"),
                    actions: DragActions::default(),
                    window: None,
                },
                Box::new(move |_, a| told.borrow_mut().push(a)),
            )
        };
        assert!(start(&out));
        assert!(!start(&out), "one drag at a time");
        assert!(out.end(&engine, Some(DropAction::Copy)));
        assert!(!out.end(&engine, None));
        assert!(start(&out));
        out.cancel(&engine);
        assert!(out.given_up());
        assert!(!out.end(&engine, None));
        assert_eq!(*told.borrow(), [Some(DropAction::Copy)]);
        assert_eq!(
            out.asked().map(|d| d.data.formats()),
            Some(vec!["text/plain".to_owned()])
        );
    }
}
