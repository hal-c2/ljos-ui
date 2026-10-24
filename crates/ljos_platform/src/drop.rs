//! Drags from other applications: the seam between a host's window system
//! and the application's drop targets.
//!
//! A host turns what its window system says (winit's `HoveredFile`,
//! `DroppedFile` and `HoveredFileCancelled`) into [`ExternalDrag`]s and
//! hands them to [`external_drag`]. The drag-and-drop of the items
//! (`DropArea`) installs itself as the handler
//! ([`crate::Platform::set_drop_handler`]) and treats them as drags that
//! came from outside.
//!
//! What the window systems winit speaks to do not give, and so a drop from
//! outside lacks:
//! - anything but files: dragged text, URLs and other MIME types;
//! - where the pointer is while a file is dragged over the window (the
//!   last position the window saw is used);
//! - which action the source allows (copy, move, link), and a way to say
//!   which one the target took;
//! - drags out of the application into another.
//!
//! Some window systems bring no drops from outside at all (winit 0.30 has
//! no drag-and-drop on Wayland). A host says so
//! ([`Platform::set_external_drops_unavailable`]), and the first drop
//! target that would take files is warned of once
//! ([`Platform::note_external_drop_target`]).

use std::path::PathBuf;

use ljos_engine::{Engine, LogLevel};

use crate::platform::{CATEGORY, Platform};

/// A drag from another application, as a host reports it.
#[derive(Clone, Debug, PartialEq)]
pub enum ExternalDrag {
    /// Files are dragged over the window, at `at` in the window's logical
    /// coordinates when the host knows where.
    Entered {
        files: Vec<PathBuf>,
        at: Option<(f64, f64)>,
    },
    /// The drag moved.
    Moved { at: (f64, f64) },
    /// The drag left the window, or was given up.
    Left,
    /// The files were let go over the window.
    Dropped {
        files: Vec<PathBuf>,
        at: Option<(f64, f64)>,
    },
}

impl ExternalDrag {
    /// The files as `file:` URLs, the way `drop.urls` has them.
    pub fn urls(&self) -> Vec<String> {
        match self {
            ExternalDrag::Entered { files, .. } | ExternalDrag::Dropped { files, .. } => {
                files.iter().map(|p| crate::paths::file_url(p)).collect()
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
/// took it; with no drag-and-drop installed nothing does.
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
