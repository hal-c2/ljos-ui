//! Keys: the `Qt.Key` codes, key sequences, `StandardKey`, the attached
//! `Keys` and `KeyNavigation`, `Shortcut`, and the delivery of a key
//! event through them (ARCHITECTURE §11).
//!
//! With [`crate::focus`] this is the architecture's `ljos_input`; it
//! starts as modules of this crate (§4, "start small") and uses nothing
//! of the scene but its root, so it can be split off. It never sees egui:
//! the host turns its key events into [`crate::KeyEvent`]s.

mod attached;
mod dispatch;
mod event;
mod filter;
pub mod key;
mod sequence;
mod shortcut;
mod standard;
mod text_input;

pub use attached::{KeyNavigation, KeyNavigationAttached, Keys, KeysAttached, priority};
pub use dispatch::{KeyItem, deliver_key, register_key_item, tab_target};
pub use event::KeyEventObject;
pub use filter::{KeyFilter, KeyFilterId, add_key_filter, remove_key_filter};
pub use sequence::{KeyChord, KeySequence, MAX_CHORDS, Platform};
pub use shortcut::{Shortcut, queue_shortcut_at, shortcut_context, shortcut_ticket};
pub use standard::{
    StandardKey, matches_standard_key, platform, set_platform, standard_key, standard_key_sequences,
};
pub use text_input::{Surrounding, TextInputClient, register_text_input_client, text_input_client};

pub(crate) use dispatch::register;
pub(crate) use shortcut::reset as reset_shortcuts;
