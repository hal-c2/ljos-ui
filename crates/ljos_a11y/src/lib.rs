//! Accessibility for Ljós, free of any windowing or widget toolkit.
//!
//! Three things live here: Qt's accessible roles with their AccessKit
//! counterparts ([`Role`], [`ROLES`]), and a retained tree of exposed
//! nodes ([`Tree`]) that records which nodes changed, so a host only says
//! what is new. The scene fills the tree; a host (an egui widget, a
//! platform adapter, a test) reads it. [`SystemPrefs`] is what a host tells
//! the application about the system's settings.
//!
//! See `docs/ARCHITECTURE.md` §14.

mod desktop;
mod prefs;
mod read;
mod role;
mod text;
mod tree;

pub use accesskit;
pub use desktop::{DesktopFont, DesktopSettings, SettingValue};
pub use prefs::{ColorScheme, Contrast, HostPrefs, SystemPrefs};
pub use read::AccessibleNode;
pub use role::{ROLE_ALIASES, ROLES, Role, RoleFlags, RoleInfo, qt_role_keys, role_word};
pub use text::{TextRun, TextValue};
pub use tree::{Bounds, Changes, NodeData, NodeKey, Tree, TreeStats};
pub use tree::{Handled, Politeness, Range, Scroll};
