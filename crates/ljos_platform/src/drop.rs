//! Drags from other applications: the seam between a host's window system
//! and the application's drop targets.
//!
//! A host turns what its window system says into [`ExternalDrag`]s and
//! hands them to [`external_drag`]: whatever the drag offers, as
//! [`DragData`] (files, other URLs, text, any MIME type), and the target
//! decides. The drag-and-drop of the items (`DropArea`) installs itself
//! as the handler ([`crate::Platform::set_drop_handler`]) and treats them
//! as drags that came from outside.
//!
//! Not carried yet: which action the source allows (copy, move, link) and
//! a way to tell it which one the target took, and drags out of the
//! application into another.
//!
//! A host whose window system brings no drops from outside at all says so
//! ([`Platform::set_external_drops_unavailable`]), and the first drop
//! target that would take files is warned of once
//! ([`Platform::note_external_drop_target`]).

use std::path::PathBuf;

use ljos_engine::{Engine, LogLevel};

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

/// A drag from another application, as a host reports it.
#[derive(Clone, Debug, PartialEq)]
pub enum ExternalDrag {
    /// Something is dragged over the window, at `at` in the window's
    /// logical coordinates when the host knows where.
    Entered {
        data: DragData,
        at: Option<(f64, f64)>,
    },
    /// The drag moved.
    Moved { at: (f64, f64) },
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
        }
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

/// Hands a drag from outside to the application. Returns whether a target
/// took it: for `Entered` and `Moved`, whether a target is under the drag
/// that takes it now (a drop there would go to it); for `Dropped`,
/// whether the target took the drop. With no drag-and-drop installed
/// nothing does.
pub fn external_drag(engine: &Engine, drag: &ExternalDrag) -> bool {
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
            false
        }
    }
}
