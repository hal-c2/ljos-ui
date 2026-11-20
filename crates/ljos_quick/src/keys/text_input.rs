//! The seam between the item that edits text and the platform's input
//! method (ARCHITECTURE §11). Nothing here edits text: `TextInput` and
//! `TextEdit` implement [`TextInputClient`], and a platform bridge asks
//! the scene for the client of the item with active focus.
//!
//! Positions count UTF-16 code units, as QML's `cursorPosition` and the
//! input methods of the platforms do.

use std::ops::Range;
use std::rc::Rc;

use ljos_engine::{Engine, MetaRef, ObjectId};

use crate::by_type::ByType;
use ljos_render::kurbo;

use crate::scene::QuickScene;

/// The text around the cursor, for an input method to look at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surrounding {
    pub text: String,
    /// Where the selection starts; equal to `cursor` without one.
    pub anchor: usize,
    pub cursor: usize,
}

/// What an item that edits text lets an input method do. `item` is the
/// item asked; one client serves every item of its type.
pub trait TextInputClient {
    fn surrounding(&self, engine: &Engine, item: ObjectId) -> Surrounding;
    /// Puts `text` in place of `range` and leaves the cursor after it.
    fn replace(&self, engine: &Engine, item: ObjectId, range: Range<usize>, text: &str);
    fn set_selection(&self, engine: &Engine, item: ObjectId, anchor: usize, cursor: usize);
    /// The cursor in the item's coordinates, for the input method to
    /// place its window by.
    fn cursor_rect(&self, engine: &Engine, item: ObjectId) -> kurbo::Rect;
    /// Where the selection starts, in the item's coordinates. Without a
    /// selection, the cursor.
    fn anchor_rect(&self, engine: &Engine, item: ObjectId) -> kurbo::Rect {
        self.cursor_rect(engine, item)
    }
    /// `inputMethodHints`, as `Qt.InputMethodHint` flags.
    fn hints(&self, engine: &Engine, item: ObjectId) -> i32;
    /// Whether the item takes text now; a read-only editor does not, and
    /// the platform need not offer it a keyboard.
    fn enabled(&self, _engine: &Engine, _item: ObjectId) -> bool {
        true
    }
    /// Shows `text` at the cursor as what the input method is composing,
    /// in place of what it showed before; it is not part of the text yet.
    /// An empty `text` ends the composition. `cursor` is the method's own
    /// cursor in `text`. Returns whether the item took it; one that
    /// cannot show a composition takes none.
    fn set_preedit(
        &self,
        _engine: &Engine,
        _item: ObjectId,
        _text: &str,
        _cursor: Option<usize>,
    ) -> bool {
        false
    }
    /// What the input method is composing here, as last shown with
    /// [`set_preedit`](Self::set_preedit).
    fn preedit(&self, _engine: &Engine, _item: ObjectId) -> String {
        String::new()
    }
    /// The anchor and the cursor, also of a field that hides its text,
    /// whose [`surrounding`](Self::surrounding) shows nothing. Positions
    /// only, and they never leave the process: an input method that typed
    /// into a hidden field is told its own selection in what it typed.
    fn selection(&self, engine: &Engine, item: ObjectId) -> (usize, usize) {
        let s = self.surrounding(engine, item);
        (s.anchor, s.cursor)
    }
    /// Puts `text` in place of `range` as one edit, which a validator
    /// takes or refuses whole, also in a field that hides its text.
    /// Returns whether the item took all of it: a validator, a mask or
    /// `maximumLength` may take part or none.
    fn replace_whole(
        &self,
        engine: &Engine,
        item: ObjectId,
        range: Range<usize>,
        text: &str,
    ) -> bool {
        let end = range.start + text.encode_utf16().count();
        self.replace(engine, item, range, text);
        self.selection(engine, item) == (end, end)
    }
    /// A digest of the item's text and selection, also of a text it hides,
    /// that changes when they do. It never leaves the process: what an
    /// input method typed into a hidden field is kept only for as long as
    /// the field is as the method left it.
    fn digest(&self, engine: &Engine, item: ObjectId) -> u64 {
        use std::hash::{Hash, Hasher};
        let s = self.surrounding(engine, item);
        let mut h = std::hash::DefaultHasher::new();
        (s.text, s.anchor, s.cursor).hash(&mut h);
        h.finish()
    }
    /// Ends the composition and types `text` over the selection. Returns
    /// whether the item took it.
    fn commit(&self, engine: &Engine, item: ObjectId, text: &str) -> bool {
        let s = self.surrounding(engine, item);
        self.replace(
            engine,
            item,
            s.anchor.min(s.cursor)..s.anchor.max(s.cursor),
            text,
        );
        true
    }
}

#[derive(Default)]
struct Registry {
    types: ByType<Rc<dyn TextInputClient>>,
}

/// Makes items of type `meta` (and of the types derived from it) text
/// input clients. A later registration wins.
pub fn register_text_input_client(engine: &Engine, meta: MetaRef, client: Rc<dyn TextInputClient>) {
    engine.extension::<Registry>().types.push(meta, client);
}

/// The client of `item`'s type, if it edits text.
pub fn text_input_client(engine: &Engine, item: ObjectId) -> Option<Rc<dyn TextInputClient>> {
    engine.extension::<Registry>().types.of(engine, item)
}

impl QuickScene {
    /// The item an input method talks to: the one with active focus, if
    /// it edits text.
    pub fn text_input_target(
        &self,
        engine: &Engine,
    ) -> Option<(ObjectId, Rc<dyn TextInputClient>)> {
        let item = self.active_focus_item(engine)?;
        Some((item, text_input_client(engine, item)?))
    }
}
