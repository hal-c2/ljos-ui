//! `SelectionScope` (module `Ljos`): one selection across several `Text`
//! items (ARCHITECTURE §7.5).
//!
//! ```qml
//! import Ljos
//! SelectionScope {
//!     Column {
//!         Text { text: "A question" }
//!         Text { textFormat: Text.MarkdownText; text: answer }
//!     }
//! }
//! ```
//!
//! Every `Text` below the scope is a member, in the order they are
//! declared. Their texts make one run of offsets, with a line break
//! between one member and the next: a press starts a selection where it
//! lands, a drag carries it across the items, and `selectedText` is what a
//! reader would call the selected text, whatever items it came from. The
//! scope takes the keyboard when it is pressed, so the platform's keys for
//! Copy and Select All work on it.
//!
//! A member keeps a document even when its text is plain: that is what
//! the selection is drawn over and counted in. The scope tells a member
//! only the part of it that is selected, so dragging a selection wakes the
//! items it passes and no others.
//!
//! Offsets count UTF-16 units, as every position in QML does.
//!
//! Members a view has not made (the delegates outside what it shows) are
//! not members: there is no item to ask for their text.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, MetaRef, ObjectId, QmlObject, QmlType, Value, qml_impl,
};
use ljos_reactive::NodeId;
use ljos_render::kurbo;
use ljos_text::rich::SelectionScope as Offsets;

use super::{Text, links, rich};
use crate::focus::{FocusReason, force_active_focus};
use crate::input::{KeyEvent, Modifiers, MouseButtons, PointerEvent};
use crate::item::{Item, is_a, num, scene_transform, write_computed};
use crate::keys::{KeyItem, matches_standard_key, platform, register_key_item, standard_key};
use crate::pointer_item::{PointerItem, register_pointer_item};
use crate::text_input::Clipboard;

/// A scope with more items below it than this is a mistake someone made;
/// the walk stops there.
const MAX_ITEMS: usize = 1 << 20;

/// `SelectionScope` (Ljos).
#[derive(Debug, QmlObject)]
#[qml(module = "Ljos", name = "SelectionScope", extends = Item)]
pub struct SelectionScope {
    #[qml(base)]
    pub item: Item,
    /// The selected text, the texts of different items a line apart.
    #[qml(notify, readonly)]
    pub selected_text: Atom,
    /// Where the selection starts in the texts of the members taken
    /// together.
    #[qml(notify, readonly)]
    pub selection_start: i32,
    #[qml(notify, readonly)]
    pub selection_end: i32,
    /// What is painted behind selected text. Translucent, so that text of
    /// any colour stays readable on it: the text keeps its colours.
    #[qml(notify)]
    pub selection_color: Color,
    /// Whether a press and a drag select.
    #[qml(notify)]
    pub select_by_mouse: bool,
    #[qml(skip)]
    st: Rc<RefCell<ScopeState>>,
}

impl Default for SelectionScope {
    fn default() -> Self {
        Self {
            item: Item::default(),
            selected_text: Atom::EMPTY,
            selection_start: 0,
            selection_end: 0,
            selection_color: Color::from_rgba8(0x33, 0x99, 0xff, 0x66),
            select_by_mouse: true,
            st: Rc::default(),
        }
    }
}

struct MemberInfo {
    obj: ObjectId,
    /// The length of its text in bytes and in UTF-16 units.
    bytes: usize,
    units: usize,
}

#[derive(Default)]
struct ScopeState {
    /// The selection, in bytes of the members' texts.
    sel: Offsets,
    /// The members as of the last time they were looked for.
    members: Vec<MemberInfo>,
}

impl std::fmt::Debug for ScopeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopeState")
            .field("members", &self.members.len())
            .field("selection", &self.sel.range())
            .finish()
    }
}

/// Whether any scope exists: until one does, a `Text` does not look for
/// one above it.
#[derive(Default)]
struct Scopes {
    any: Cell<Option<NodeId>>,
}

impl Scopes {
    fn node(&self, engine: &Engine) -> Option<NodeId> {
        if self.any.get().is_none() {
            let g = engine.graph();
            self.any.set(g.property(g.root(), Value::Bool(false)).ok());
        }
        self.any.get()
    }
}

fn mark(engine: &Engine) {
    let scopes = engine.extension::<Scopes>();
    if let Some(n) = scopes.node(engine) {
        let g = engine.graph();
        if !g.get_untracked(n).to_boolean() {
            g.set(n, Value::Bool(true));
        }
    }
}

/// What being in a scope means to a `Text`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Member {
    /// The colour behind what is selected of it.
    pub(crate) color: Color,
}

/// The scope `obj` is in, if any (tracked: the answer follows the item
/// through the tree).
pub(crate) fn member(engine: &Engine, obj: ObjectId) -> Option<Member> {
    let scopes = engine.extension::<Scopes>();
    let n = scopes.node(engine)?;
    if !engine.graph().try_get(n)?.to_boolean() {
        return None;
    }
    let mut cur = engine.visual_parent(obj);
    // Bounded so a corrupt parent cycle cannot hang.
    for _ in 0..4096 {
        let o = cur?;
        if is_a::<SelectionScope>(engine, o) {
            let color = match engine.read(o, SelectionScope::SELECTION_COLOR.index) {
                Value::Color(c) => c,
                _ => Color::TRANSPARENT,
            };
            return Some(Member { color });
        }
        cur = engine.visual_parent(o);
    }
    None
}

fn state(engine: &Engine, scope: ObjectId) -> Option<Rc<RefCell<ScopeState>>> {
    engine
        .untrack(|| engine.with(scope, |s: &SelectionScope| Rc::clone(&s.st)))
        .ok()
}

fn floor(text: &str, mut i: usize) -> usize {
    i = i.min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn units_of(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The byte at which `unit` UTF-16 units of `text` have gone by.
fn byte_at_unit(text: &str, unit: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= unit {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

fn text_of(engine: &Engine, member: ObjectId) -> String {
    rich::state(engine, member)
        .map(|s| s.borrow().doc.text())
        .unwrap_or_default()
}

/// Looks for the members again: the `Text` items below the scope that are
/// shown, in the order they are declared. A scope inside the scope keeps
/// its own.
fn refresh(engine: &Engine, scope: ObjectId, st: &Rc<RefCell<ScopeState>>) {
    let mut found = Vec::new();
    let mut stack = engine.untrack(|| crate::tree::child_items(engine, scope));
    stack.reverse();
    let mut seen = 0;
    while let Some(o) = stack.pop() {
        seen += 1;
        if seen > MAX_ITEMS {
            break;
        }
        if is_a::<SelectionScope>(engine, o) {
            continue;
        }
        let shown = engine.untrack(|| engine.read(o, Item::VISIBLE.index).to_boolean());
        if !shown {
            continue;
        }
        if is_a::<Text>(engine, o) {
            found.push(o);
        }
        let mut kids = engine.untrack(|| crate::tree::child_items(engine, o));
        kids.reverse();
        stack.extend(kids);
    }
    let members: Vec<MemberInfo> = found
        .into_iter()
        .filter_map(|obj| {
            // Brings the member's document up to its text.
            rich::current(engine, obj)?;
            let text = text_of(engine, obj);
            Some(MemberInfo {
                obj,
                bytes: text.len(),
                units: units_of(&text),
            })
        })
        .collect();
    let mut s = st.borrow_mut();
    // A member that left shows no selection any more.
    let left: Vec<ObjectId> = s
        .members
        .iter()
        .map(|m| m.obj)
        .filter(|o| !members.iter().any(|m| m.obj == *o))
        .collect();
    s.sel
        .set_members(members.iter().enumerate().map(|(i, m)| (i as u64, m.bytes)));
    s.members = members;
    drop(s);
    for obj in left {
        show(engine, obj, None);
    }
}

/// Tells a member what is selected of it.
fn show(engine: &Engine, member: ObjectId, range: Option<Range<usize>>) {
    if !engine.is_alive(member) {
        return;
    }
    if let Some(shared) = rich::state(engine, member) {
        let changed = {
            let mut s = shared.borrow_mut();
            let changed = s.selection != range;
            s.selection = range;
            changed
        };
        if changed {
            rich::bump(engine, member, &shared);
        }
    }
}

/// The scope's offset in UTF-16 units of an offset in bytes.
fn to_units(engine: &Engine, s: &ScopeState, bytes: usize) -> usize {
    let (mut b, mut u) = (0, 0);
    for m in &s.members {
        if bytes <= b + m.bytes {
            let text = text_of(engine, m.obj);
            return u + units_of(&text[..floor(&text, bytes - b)]);
        }
        b += m.bytes + 1;
        u += m.units + 1;
    }
    u.saturating_sub(1)
}

/// The scope's offset in bytes of an offset in UTF-16 units.
fn to_bytes(engine: &Engine, s: &ScopeState, units: usize) -> usize {
    let (mut b, mut u) = (0, 0);
    for m in &s.members {
        if units <= u + m.units {
            let text = text_of(engine, m.obj);
            return b + byte_at_unit(&text, units - u).min(m.bytes);
        }
        b += m.bytes + 1;
        u += m.units + 1;
    }
    b.saturating_sub(1)
}

/// Makes the members and the scope's properties say what the selection is.
fn publish(engine: &Engine, scope: ObjectId, st: &Rc<RefCell<ScopeState>>) {
    let (parts, start, end, text) = {
        let s = st.borrow();
        let parts: Vec<(ObjectId, Option<Range<usize>>)> = s
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| (m.obj, s.sel.range_in(i as u64)))
            .collect();
        let range = s.sel.range();
        let text = s.sel.selected_text(|i, range| {
            s.members
                .get(i as usize)
                .and_then(|m| rich::state(engine, m.obj))
                .map(|d| d.borrow().doc.text_of(range))
                .unwrap_or_default()
        });
        (
            parts,
            to_units(engine, &s, range.start),
            to_units(engine, &s, range.end),
            text,
        )
    };
    for (member, range) in parts {
        show(engine, member, range);
    }
    let r = engine.batch(|| {
        let put = |p, v| write_computed(engine, scope, p, v);
        put(
            SelectionScope::SELECTION_START.index,
            Value::Int(i32::try_from(start).unwrap_or(i32::MAX)),
        )?;
        put(
            SelectionScope::SELECTION_END.index,
            Value::Int(i32::try_from(end).unwrap_or(i32::MAX)),
        )?;
        put(
            SelectionScope::SELECTED_TEXT.index,
            Value::String(Atom::from(text.as_str())),
        )
    });
    if let Err(err) = r {
        engine.log(
            ljos_engine::LogLevel::Warning,
            crate::scene::SCENE_CATEGORY,
            format!("SelectionScope: {err}"),
        );
    }
}

/// A point of the scope in the coordinates of a member.
fn to_member(engine: &Engine, scope: ObjectId, member: ObjectId, at: kurbo::Point) -> kurbo::Point {
    engine.untrack(|| {
        scene_transform(engine, member).inverse() * (scene_transform(engine, scope) * at)
    })
}

/// The member a point of the scope belongs to, and the point in it: the
/// member the point is in, else the nearest, lines before columns, as a
/// reader would look for it.
fn member_at(
    engine: &Engine,
    scope: ObjectId,
    s: &ScopeState,
    at: kurbo::Point,
) -> Option<(usize, kurbo::Point)> {
    let mut best: Option<(usize, kurbo::Point, (f64, f64))> = None;
    for (i, m) in s.members.iter().enumerate() {
        if !engine.is_alive(m.obj) {
            continue;
        }
        let p = to_member(engine, scope, m.obj, at);
        let (w, h) = engine.untrack(|| {
            (
                num(engine, m.obj, Item::WIDTH.index),
                num(engine, m.obj, Item::HEIGHT.index),
            )
        });
        let off = |v: f64, size: f64| (-v).max(v - size).max(0.0);
        let d = (off(p.y, h), off(p.x, w));
        if best.as_ref().is_none_or(|(_, _, old)| d < *old) {
            best = Some((i, p, d));
        }
    }
    best.map(|(i, p, _)| (i, p))
}

/// The scope's offset (in bytes) nearest to a point of the scope.
fn offset_at(
    engine: &Engine,
    scope: ObjectId,
    st: &Rc<RefCell<ScopeState>>,
    at: kurbo::Point,
) -> Option<usize> {
    let (i, member, p) = {
        let s = st.borrow();
        let (i, p) = member_at(engine, scope, &s, at)?;
        (i, s.members[i].obj, p)
    };
    let (shared, view, top) = rich::current(engine, member)?;
    let local = shared.borrow().doc.position_at(p.x, p.y - top, &view);
    st.borrow().sel.offset_of(i as u64, local)
}

fn select_units(engine: &Engine, scope: ObjectId, start: i32, end: i32) {
    let Some(st) = state(engine, scope) else {
        return;
    };
    mark(engine);
    refresh(engine, scope, &st);
    {
        let mut s = st.borrow_mut();
        let at = |u: i32| to_bytes(engine, &s, usize::try_from(u).unwrap_or(0));
        let (a, b) = (at(start), at(end));
        s.sel.select(a, b);
    }
    publish(engine, scope, &st);
}

#[qml_impl]
impl SelectionScope {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        mark(cx.engine);
        Ok(())
    }

    /// `select(start, end)`: selects from one offset to the other.
    #[qml(invokable)]
    fn select(cx: &Cx, start: i32, end: i32) {
        select_units(cx.engine, cx.this, start, end);
    }

    /// `selectAll()`: everything in every member.
    #[qml(invokable, name = "selectAll")]
    fn select_all(cx: &Cx) {
        let Some(st) = state(cx.engine, cx.this) else {
            return;
        };
        mark(cx.engine);
        refresh(cx.engine, cx.this, &st);
        st.borrow_mut().sel.select_all();
        publish(cx.engine, cx.this, &st);
    }

    /// `deselect()`: nothing is selected.
    #[qml(invokable)]
    fn deselect(cx: &Cx) {
        let Some(st) = state(cx.engine, cx.this) else {
            return;
        };
        st.borrow_mut().sel.clear();
        publish(cx.engine, cx.this, &st);
    }

    /// `copy()`: puts the selected text on the clipboard. With nothing
    /// selected the clipboard is left alone.
    #[qml(invokable)]
    fn copy(cx: &Cx) {
        copy(cx.engine, cx.this);
    }

    /// `positionAt(x, y)`: the offset nearest to a point of the scope.
    #[qml(invokable, name = "positionAt")]
    fn position_at(cx: &Cx, x: f64, y: f64) -> i32 {
        let Some(st) = state(cx.engine, cx.this) else {
            return 0;
        };
        mark(cx.engine);
        refresh(cx.engine, cx.this, &st);
        let Some(bytes) = offset_at(cx.engine, cx.this, &st, kurbo::Point::new(x, y)) else {
            return 0;
        };
        let units = to_units(cx.engine, &st.borrow(), bytes);
        i32::try_from(units).unwrap_or(i32::MAX)
    }

    /// `linkAt(x, y)`: the address of the link at a point of the scope,
    /// whichever member it is in, or an empty string.
    #[qml(invokable, name = "linkAt")]
    fn link_at(cx: &Cx, x: f64, y: f64) -> String {
        let Some(st) = state(cx.engine, cx.this) else {
            return String::new();
        };
        refresh(cx.engine, cx.this, &st);
        let found = {
            let s = st.borrow();
            member_at(cx.engine, cx.this, &s, kurbo::Point::new(x, y))
                .map(|(i, p)| (s.members[i].obj, p))
        };
        found
            .and_then(|(member, p)| links::link_at(cx.engine, member, p))
            .map(|href| href.to_string())
            .unwrap_or_default()
    }
}

fn copy(engine: &Engine, scope: ObjectId) -> bool {
    let text = engine
        .untrack(|| engine.read(scope, SelectionScope::SELECTED_TEXT.index))
        .to_js_string();
    if text.is_empty() {
        return false;
    }
    Clipboard::of(engine).copy(engine, text.as_str());
    true
}

fn selects_by_mouse(engine: &Engine, scope: ObjectId) -> bool {
    engine
        .untrack(|| engine.read(scope, SelectionScope::SELECT_BY_MOUSE.index))
        .to_boolean()
}

fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if ev.button.is_some_and(|b| b != MouseButtons::LEFT) || !selects_by_mouse(engine, obj) {
        return false;
    }
    let Some(st) = state(engine, obj) else {
        return false;
    };
    refresh(engine, obj, &st);
    let Some(offset) = offset_at(engine, obj, &st, at) else {
        // Nothing to select in.
        return false;
    };
    {
        let mut s = st.borrow_mut();
        if ev.modifiers.contains(Modifiers::SHIFT) {
            s.sel.extend_to(offset);
        } else {
            s.sel.select(offset, offset);
        }
    }
    // The keys for Copy and Select All come here from now on.
    force_active_focus(engine, obj, FocusReason::Mouse);
    publish(engine, obj, &st);
    true
}

fn moved(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = state(engine, obj) else {
        return;
    };
    let Some(offset) = offset_at(engine, obj, &st, at) else {
        return;
    };
    if st.borrow().sel.focus() == offset {
        return;
    }
    st.borrow_mut().sel.extend_to(offset);
    publish(engine, obj, &st);
}

fn release(_engine: &Engine, _obj: ObjectId, _ev: &PointerEvent, _at: kurbo::Point) {}

fn cancel(_engine: &Engine, _obj: ObjectId) {}

fn hover(_engine: &Engine, _obj: ObjectId, _over: bool) {}

fn is_standard(engine: &Engine, ev: &KeyEvent, name: &str) -> bool {
    standard_key(name)
        .is_some_and(|k| matches_standard_key(ev.key, ev.modifiers, k, platform(engine)))
}

fn key_press(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    if is_standard(engine, ev, "Copy") {
        return copy(engine, obj);
    }
    if is_standard(engine, ev, "SelectAll") {
        let Some(st) = state(engine, obj) else {
            return false;
        };
        refresh(engine, obj, &st);
        st.borrow_mut().sel.select_all();
        publish(engine, obj, &st);
        return true;
    }
    false
}

/// Copy must reach the scope even where a `Shortcut` is on the same keys:
/// with a selection, the scope is what the user means.
fn shortcut_override(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    is_standard(engine, ev, "Copy")
        && state(engine, obj).is_some_and(|st| st.borrow().sel.has_selection())
}

fn no_key(_engine: &Engine, _obj: ObjectId, _ev: &KeyEvent) -> bool {
    false
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("Ljos" 1.0 => [SelectionScope]));
    let meta = || MetaRef::Static(SelectionScope::static_meta());
    register_pointer_item(
        engine,
        meta(),
        PointerItem {
            press,
            moved,
            release,
            cancel,
            hover,
        },
    );
    register_key_item(
        engine,
        meta(),
        KeyItem {
            shortcut_override,
            press: key_press,
            release: no_key,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_in_units_and_bytes_agree_on_whole_characters() {
        let text = "a\u{e9}\u{1F600}b";
        assert_eq!(units_of(text), 5);
        assert_eq!(byte_at_unit(text, 0), 0);
        assert_eq!(byte_at_unit(text, 1), 1);
        assert_eq!(byte_at_unit(text, 2), 3);
        // Half of a pair is the character it is half of.
        assert_eq!(byte_at_unit(text, 3), 7);
        assert_eq!(byte_at_unit(text, 4), 7);
        assert_eq!(byte_at_unit(text, 5), 8);
        assert_eq!(byte_at_unit(text, 99), 8);
        assert_eq!(floor(text, 2), 1);
        assert_eq!(floor(text, 99), text.len());
    }
}
