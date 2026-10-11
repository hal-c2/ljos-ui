//! Where the rows of a menu and of a combo box's popup live.
//!
//! A content item that is a view (the Basic style's `ListView`) shows the
//! rows of the control's model itself. Any other content item holds the
//! rows as children, which the control then puts there: of what the item
//! scrolls when it is a `Flickable`, of the item itself otherwise.

use ljos_engine::{Engine, ObjectId};
use ljos_quick::flickable;

/// The item the rows are children of: what `content` scrolls, or
/// `content` itself where it does not scroll.
pub(crate) fn home(engine: &Engine, content: ObjectId) -> ObjectId {
    flickable::content_item(engine, content).unwrap_or(content)
}
