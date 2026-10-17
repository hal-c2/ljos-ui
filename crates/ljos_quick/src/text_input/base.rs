//! What `TextInput` and `TextEdit` share: the properties, the text model
//! behind them and how the two are kept in step, focus, the blinking
//! cursor and the cursor delegate.
//!
//! The model ([`ljos_text::TextModel`]) is the truth for the text, the
//! cursor and the selection. `text` and `cursorPosition` are also
//! properties a program writes: [`reconcile`] takes what was written into
//! the model, and [`publish`] writes what the model holds back out.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, Font, LogLevel, ObjectId, QmlObject, Rect, Timer, Value,
    qml_impl,
};
use ljos_reactive::NodeId;
use ljos_text::{EditLayout, EditParams, StyleSpan, TextModel, TextStyle, reads_right_to_left};

use super::clipboard::Clipboard;
use super::edit::TextEdit;
use super::input::TextInput;
use super::layout::{Geometry, LayoutInputs, Shown, position_at, position_rect, relayout};
use super::ops;
use super::pointer::PointerState;
use super::{cursor_position, selection_mode};
use crate::enums::{align, wrap_mode};
use crate::focus;
use crate::item::{Item, init_item, is_item, is_unset, write_computed};
use crate::tree::set_parent_item;

/// The log category of an editor that cannot do its work.
pub(crate) const CATEGORY: &str = "qt.quick.textinput";

/// How long the cursor is shown, and then hidden, while it blinks: half
/// of Qt's default cursor flash time.
const BLINK_MS: i32 = 500;

pub(crate) type SharedState = Rc<RefCell<State>>;

/// What kind of edit the last one was: edits of one kind that follow each
/// other where the last one ended are undone together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditKind {
    Typed,
    Backspace,
    Delete,
    /// `insert()` and `remove()` from a program.
    Program,
    /// Always a step of its own: a paste, a cut, a new line, a word.
    Other,
}

/// The run of edits the next one may join.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Run {
    pub kind: EditKind,
    /// Where the run ended.
    pub end: usize,
}

/// What an editor keeps besides its properties.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// A `TextEdit`: many lines.
    pub multi: bool,
    pub initialised: bool,
    /// [`publish`] is writing the properties: a change of them is ours.
    pub syncing: bool,
    pub model: TextModel,
    /// What an input method is composing, shown at the cursor and not
    /// part of the text.
    pub preedit: String,
    /// The input method's own cursor in the composition (UTF-16 units).
    pub preedit_cursor: Option<usize>,
    /// `PasswordEchoOnEdit`: the user edited the text since the field got
    /// focus, so it is shown.
    pub editing: bool,
    pub shown: Shown,
    pub layout: Option<Rc<EditLayout>>,
    /// What `layout` was made with.
    pub laid: Option<(TextStyle, EditParams)>,
    /// The highlight spans `layout` was made with, as the layout has them.
    pub laid_spans: Vec<StyleSpan>,
    pub geo: Geometry,
    pub cursor_rect: Rect,
    /// The implicit width a one-line editor last gave, and its text's
    /// width plus padding to the fraction that was rounded up.
    pub natural_width: Option<(f64, f64)>,
    /// How far a one-line text is moved left to keep the cursor in view.
    pub hscroll: f64,
    /// The blinking cursor is in its shown half.
    pub blink_on: bool,
    pub blink_timer: Option<ObjectId>,
    pub layout_epoch: Option<NodeId>,
    /// Counts the changes of highlight spans that change the glyphs, which
    /// the layout reads.
    pub shape_epoch: Option<NodeId>,
    /// Counts what changes the picture without changing a property.
    pub paint_epoch: Option<NodeId>,
    /// Counts changes of a rich text, which `text` may not show yet: what
    /// follows the characters reads it instead of `text`.
    pub rich_epoch: Option<NodeId>,
    /// The editor's own subscription to `text`, which does not count as
    /// someone reading it.
    pub text_watch: Option<NodeId>,
    pub run: Option<Run>,
    /// The column Up and Down keep to, and the cursor position it was
    /// taken at: it holds only while the cursor is still there.
    pub sticky: Option<(f64, usize)>,
    pub pointer: PointerState,
    /// The component of `cursorDelegate` and what was made from it.
    pub delegate: Option<(ObjectId, ObjectId)>,
    /// `textDocument` of a `TextEdit`.
    pub document: Option<ObjectId>,
    /// The colours a program gave stretches of the text.
    pub highlights: super::highlight::Highlights,
    /// The formats of a `TextEdit`'s rich text; `None` for plain text.
    pub rich: Option<super::rich::RichEdit>,
    /// `cursorSelection` of a `TextEdit`, and what it last showed.
    pub selection: Option<ObjectId>,
    pub selection_shown: Option<super::selection::Shown>,
    /// The `source` of the document a file was last read from or saved
    /// to by the editor itself.
    pub doc_source: String,
}

/// The base of `TextInput` and `TextEdit`. Not a QML type of its own.
/// The alignment of a text nobody aligned: right-to-left text is aligned
/// right.
fn natural_alignment(engine: &Engine, obj: ObjectId, _p: ljos_engine::PropIndex) -> Value {
    // Rich text reads as its characters, not as its markup, and follows
    // its changes without reading `text`, which is made only when read.
    let (rich, epoch) = state_of(engine, obj)
        .and_then(|st| {
            let s = st.try_borrow().ok()?;
            let rich = s.rich.as_ref().map(|_| reads_right_to_left(s.model.text()));
            Some((rich, s.rich_epoch))
        })
        .unwrap_or_default();
    if let Some(n) = epoch {
        let _ = engine.graph().get(n);
    }
    let rtl = rich.unwrap_or_else(|| {
        let text = engine.read(obj, TextEditor::TEXT.index).to_js_string();
        reads_right_to_left(text.as_str())
    });
    Value::Int(if rtl { align::RIGHT } else { align::LEFT })
}

/// What a side's padding is while it has no value of its own.
fn all_padding(engine: &Engine, obj: ObjectId, _side: ljos_engine::PropIndex) -> Value {
    engine.read(obj, TextEditor::PADDING.index)
}

#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "TextEditorBase",
    extends = Item,
    enums = super::editor_enums,
    uncreatable
)]
pub struct TextEditor {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub font: Font,
    #[qml(notify)]
    pub selection_color: Color,
    #[qml(notify)]
    pub selected_text_color: Color,
    /// In UTF-16 code units, as every position here.
    #[qml(notify)]
    pub cursor_position: i32,
    #[qml(notify, readonly)]
    pub selection_start: i32,
    #[qml(notify, readonly)]
    pub selection_end: i32,
    #[qml(notify, readonly)]
    pub selected_text: Atom,
    #[qml(notify, readonly)]
    pub length: i32,
    #[qml(notify)]
    pub read_only: bool,
    /// Follows active focus until it is assigned.
    #[qml(notify)]
    pub cursor_visible: bool,
    /// A `Component`; with one the editor draws no cursor of its own.
    #[qml(notify, object = "Component")]
    pub cursor_delegate: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub cursor_rectangle: Rect,
    /// Until it is assigned it follows the direction of the text.
    #[qml(
        notify, enumeration = "HAlignment",
        reset = ljos_engine::Reset::Follow(natural_alignment)
    )]
    pub horizontal_alignment: i32,
    #[qml(notify, readonly, enumeration = "HAlignment")]
    pub effective_horizontal_alignment: i32,
    #[qml(notify, enumeration = "VAlignment")]
    pub vertical_alignment: i32,
    /// Only a `TextEdit` wraps.
    #[qml(notify, enumeration = "WrapMode")]
    pub wrap_mode: i32,
    #[qml(notify, readonly)]
    pub content_width: f64,
    #[qml(notify, readonly)]
    pub content_height: f64,
    #[qml(notify, readonly)]
    pub can_undo: bool,
    #[qml(notify, readonly)]
    pub can_redo: bool,
    #[qml(notify, readonly)]
    pub can_paste: bool,
    #[qml(notify)]
    pub select_by_mouse: bool,
    #[qml(notify, enumeration = "SelectionMode")]
    pub mouse_selection_mode: i32,
    #[qml(notify)]
    pub persistent_selection: bool,
    /// `Qt.InputMethodHint` flags.
    #[qml(notify, flags = "Qt.InputMethodHints")]
    pub input_method_hints: i32,
    #[qml(notify, readonly)]
    pub input_method_composing: bool,
    #[qml(notify, readonly)]
    pub preedit_text: Atom,
    #[qml(notify)]
    pub overwrite_mode: bool,
    #[qml(notify)]
    pub active_focus_on_press: bool,
    #[qml(notify, reset)]
    pub padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub left_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub top_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub right_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub bottom_padding: f64,
    /// Kept; there is one way of drawing text.
    #[qml(notify, enumeration = "RenderType")]
    pub render_type: i32,
    #[qml(skip)]
    pub(crate) st: SharedState,
}

impl TextEditor {
    pub(crate) fn new(multi: bool) -> Self {
        Self {
            // Text is antialiased unless told otherwise, as in Qt.
            item: Item {
                antialiasing: true,
                ..Item::default()
            },
            text: Atom::EMPTY,
            color: Color::BLACK,
            font: Font::default(),
            selection_color: Color::from_rgba8(0, 0, 0x80, 0xff),
            selected_text_color: Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
            cursor_position: 0,
            selection_start: 0,
            selection_end: 0,
            selected_text: Atom::EMPTY,
            length: 0,
            read_only: false,
            cursor_visible: false,
            cursor_delegate: None,
            cursor_rectangle: Rect::default(),
            horizontal_alignment: align::LEFT,
            effective_horizontal_alignment: align::LEFT,
            vertical_alignment: align::TOP,
            wrap_mode: wrap_mode::NO_WRAP,
            content_width: 0.0,
            content_height: 0.0,
            can_undo: false,
            can_redo: false,
            can_paste: false,
            select_by_mouse: true,
            mouse_selection_mode: selection_mode::SELECT_CHARACTERS,
            persistent_selection: false,
            input_method_hints: 0,
            input_method_composing: false,
            preedit_text: Atom::EMPTY,
            overwrite_mode: false,
            active_focus_on_press: true,
            padding: 0.0,
            left_padding: 0.0,
            top_padding: 0.0,
            right_padding: 0.0,
            bottom_padding: 0.0,
            render_type: 0,
            st: Rc::new(RefCell::new(State {
                multi,
                blink_on: true,
                model: new_model(multi),
                ..State::default()
            })),
        }
    }
}

/// The model of a new editor. The model starts at TextInput's
/// `maximumLength`, which a TextEdit does not have: its text is as long as
/// it is (features/text-input/text-edit-length.feature).
fn new_model(multi: bool) -> TextModel {
    let mut model = TextModel::new();
    if multi {
        model.set_max_length(usize::MAX);
    }
    model
}

impl Default for TextEditor {
    fn default() -> Self {
        Self::new(false)
    }
}

/// A position a program gave, when the text has it.
pub(crate) fn position(st: &SharedState, pos: i32) -> Option<usize> {
    let pos = usize::try_from(pos).ok()?;
    (pos <= st.borrow().model.len()).then_some(pos)
}

// The methods read the model and the layout directly and never a property
// as a dependency: a binding that calls one (`text: getText(0, 3)`) must
// not come to depend on the editor's own state through it.
#[qml_impl]
impl TextEditor {
    /// Emitted when Return is pressed on an acceptable text, and when the
    /// editor loses focus.
    #[qml(signal, name = "editingFinished")]
    fn editing_finished();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_editor(cx.engine, cx.this)
    }

    /// `select(start, end)`: the cursor goes to `end`. Positions the text
    /// does not have are ignored.
    #[qml(invokable)]
    fn select(cx: &Cx, start: i32, end: i32) {
        ops::act(cx.engine, cx.this, |st| {
            if let (Some(a), Some(b)) = (position(st, start), position(st, end)) {
                st.borrow_mut().model.select(a, b);
            }
        });
    }

    #[qml(invokable, name = "selectAll")]
    fn select_all(cx: &Cx) {
        ops::act(cx.engine, cx.this, |st| st.borrow_mut().model.select_all());
    }

    #[qml(invokable, name = "selectWord")]
    fn select_word(cx: &Cx) {
        ops::act(cx.engine, cx.this, |st| {
            let mut s = st.borrow_mut();
            let reach = !s.multi;
            s.model.select_word(reach);
        });
    }

    #[qml(invokable)]
    fn deselect(cx: &Cx) {
        ops::act(cx.engine, cx.this, |st| st.borrow_mut().model.deselect());
    }

    /// `insert(position, text)`. A position the text does not have is
    /// ignored.
    #[qml(invokable)]
    fn insert(cx: &Cx, position: i32, text: Value) {
        ops::program_insert(cx.engine, cx.this, position, text.to_js_string().as_str());
    }

    #[qml(invokable)]
    fn cut(cx: &Cx) {
        ops::cut(cx.engine, cx.this);
    }

    #[qml(invokable)]
    fn copy(cx: &Cx) {
        ops::copy(cx.engine, cx.this);
    }

    #[qml(invokable)]
    fn paste(cx: &Cx) {
        ops::paste(cx.engine, cx.this);
    }

    #[qml(invokable)]
    fn undo(cx: &Cx) {
        ops::undo(cx.engine, cx.this);
    }

    #[qml(invokable)]
    fn redo(cx: &Cx) {
        ops::redo(cx.engine, cx.this);
    }

    /// `clear()`: the text goes, and unlike assigning `text` that can be
    /// undone.
    #[qml(invokable)]
    fn clear(cx: &Cx) {
        ops::clear(cx.engine, cx.this);
    }

    /// `getText(start, end)`, positions in either order and cut to the
    /// text.
    #[qml(invokable, name = "getText")]
    fn get_text(cx: &Cx, start: i32, end: i32) -> String {
        let Some(st) = ops::ready(cx.engine, cx.this) else {
            return String::new();
        };
        let s = st.borrow();
        let at = |p: i32| usize::try_from(p).unwrap_or(0);
        let text = s.model.slice(at(start), at(end));
        if s.rich.is_some() {
            // Paragraphs of rich text are apart by U+2029, as in Qt.
            ljos_text::rich::RichText::plain_of(text)
        } else {
            text.to_owned()
        }
    }

    /// `moveCursorSelection(position, mode)`: the cursor goes to
    /// `position` and the other end of the selection stays; with
    /// `SelectWords` both go out to word boundaries.
    #[qml(invokable, name = "moveCursorSelection")]
    fn move_cursor_selection(cx: &Cx, position: i32, mode: Value) {
        let words = mode.as_number().map(|n| n as i32) == Some(selection_mode::SELECT_WORDS);
        ops::act(cx.engine, cx.this, |st| {
            if let Some(pos) = self::position(st, position) {
                st.borrow_mut().model.move_cursor_selection(pos, words);
            }
        });
    }

    /// `positionAt(x, y, position)`. A `TextInput` is also called with
    /// `(x, position)` as Qt 5 had it: one line has no use for `y`.
    #[qml(invokable, name = "positionAt")]
    fn position_at(cx: &Cx, x: Value, y: Value, mode: Value) -> i32 {
        let Some(st) = ops::ready(cx.engine, cx.this) else {
            return 0;
        };
        let multi = st.borrow().multi;
        let n = |v: &Value| v.as_number().unwrap_or(0.0);
        let (y, mode) = if !multi && mode.is_nullish() && !y.is_nullish() && n(&y).fract() == 0.0 {
            // Two arguments: the second is `y` unless it reads as a
            // cursor mode, and for one line `y` does not matter.
            let m = n(&y) as i32;
            if m == cursor_position::ON_CHARACTER || m == cursor_position::BETWEEN_CHARACTERS {
                (0.0, m)
            } else {
                (n(&y), cursor_position::BETWEEN_CHARACTERS)
            }
        } else {
            (
                n(&y),
                mode.as_number()
                    .map_or(cursor_position::BETWEEN_CHARACTERS, |m| m as i32),
            )
        };
        position_at(&st, ljos_render::kurbo::Point::new(n(&x), y), mode) as i32
    }

    #[qml(invokable, name = "positionToRectangle")]
    fn position_to_rectangle(cx: &Cx, position: i32) -> Rect {
        match ops::ready(cx.engine, cx.this) {
            Some(st) => position_rect(&st, position),
            None => Rect::default(),
        }
    }

    /// `isRightToLeft(start, end)`: whether that part of the text reads
    /// right to left.
    #[qml(invokable, name = "isRightToLeft")]
    fn is_right_to_left(cx: &Cx, start: i32, end: i32) -> bool {
        let Some(st) = ops::ready(cx.engine, cx.this) else {
            return false;
        };
        let s = st.borrow();
        let at = |p: i32| usize::try_from(p).unwrap_or(0);
        reads_right_to_left(s.model.slice(at(start), at(end)))
    }
}

/// The state of an editor.
pub(crate) fn state_of(engine: &Engine, obj: ObjectId) -> Option<SharedState> {
    engine
        .untrack(|| engine.with(obj, |t: &TextEditor| Rc::clone(&t.st)))
        .ok()
}

/// Logs what an editor could not do. Nothing an editor does to itself
/// should fail; if it does, the user hears of it and the editor goes on.
pub(crate) fn check(engine: &Engine, what: &str, r: Result<(), EngineError>) {
    if let Err(err) = r {
        engine.log(LogLevel::Warning, CATEGORY, format!("{what}: {err}"));
    }
}

/// Makes a node that counts changes tell its readers.
pub(crate) fn bump(engine: &Engine, n: NodeId) {
    let g = engine.graph();
    let v = g.get_untracked(n).to_number() as i32;
    g.set(n, Value::Int(v.wrapping_add(1)));
}

fn flag(engine: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> bool {
    engine.untrack(|| engine.read(obj, p)).to_boolean()
}

/// What completing an editor does, for one built natively without
/// completion (the scene calls it when it adopts the item). Idempotent.
pub(crate) fn init_editor(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(engine, obj) else {
        return Ok(());
    };
    if std::mem::replace(&mut st.borrow_mut().initialised, true) {
        return Ok(());
    }
    init_item(engine, obj)?;
    let multi = st.borrow().multi;

    // A side without a padding of its own has `padding`.
    for side in [
        TextEditor::LEFT_PADDING,
        TextEditor::TOP_PADDING,
        TextEditor::RIGHT_PADDING,
        TextEditor::BOTTOM_PADDING,
    ] {
        if is_unset(engine, obj, side.index) {
            engine.bind(obj, side.index, move |e| {
                e.read(obj, TextEditor::PADDING.index)
            })?;
        }
    }
    // Right-to-left text is aligned right until an alignment is assigned.
    if is_unset(engine, obj, TextEditor::HORIZONTAL_ALIGNMENT.index) {
        let p = TextEditor::HORIZONTAL_ALIGNMENT.index;
        engine.bind(obj, p, move |e| natural_alignment(e, obj, p))?;
    }
    engine.bind_initial(
        obj,
        TextEditor::EFFECTIVE_HORIZONTAL_ALIGNMENT.index,
        move |e| e.read(obj, TextEditor::HORIZONTAL_ALIGNMENT.index),
    )?;
    engine.bind_initial(obj, TextEditor::CAN_PASTE.index, move |e| {
        let editable = !e.read(obj, TextEditor::READ_ONLY.index).to_boolean();
        Value::Bool(editable && Clipboard::of(e).has_text(e))
    })?;

    // The text the declaration gave. A cursor position it gave too is
    // kept; without one a field starts with the cursor at the end.
    let declared_cursor = (!is_unset(engine, obj, TextEditor::CURSOR_POSITION.index))
        .then(|| engine.untrack(|| engine.read(obj, TextEditor::CURSOR_POSITION.index)))
        .map(|v| v.to_number() as i32);
    if multi {
        TextEdit::init(engine, obj, &st)?;
    } else {
        TextInput::init(engine, obj, &st)?;
    }
    super::a11y::default_role(engine, obj)?;
    let text = engine
        .untrack(|| engine.read(obj, TextEditor::TEXT.index))
        .to_js_string();
    let (format, font) = super::rich::inputs(engine, obj, multi, text.as_str());
    {
        let mut s = st.borrow_mut();
        super::rich::assign(&mut s, format, text.as_str(), &font);
        if multi {
            s.model.move_to(0, false);
        }
    }
    if let Some(pos) = declared_cursor.and_then(|p| position(&st, p)) {
        st.borrow_mut().model.move_to(pos, false);
    }

    let watch = engine.watch(obj, TextEditor::TEXT.index, move |e, _| {
        if let Some(st) = state_of(e, obj) {
            reconcile(e, obj, &st);
        }
    })?;
    st.borrow_mut().text_watch = Some(watch);
    engine.watch(obj, TextEditor::CURSOR_POSITION.index, move |e, _| {
        if let Some(st) = state_of(e, obj) {
            reconcile(e, obj, &st);
        }
    })?;
    engine.watch(obj, Item::ACTIVE_FOCUS.index, move |e, v| {
        focus_changed(e, obj, v.to_boolean());
    })?;
    engine.watch(obj, TextEditor::READ_ONLY.index, move |e, v| {
        // A read-only editor shows no cursor and has nothing to undo.
        let active = flag(e, obj, Item::ACTIVE_FOCUS.index);
        let shows = active && !v.to_boolean();
        check(
            e,
            "cursorVisible",
            e.write_derived(obj, TextEditor::CURSOR_VISIBLE.index, Value::Bool(shows)),
        );
        if let Some(st) = state_of(e, obj) {
            check(e, "readOnly", publish(e, obj, &st));
        }
    })?;
    for p in [Item::VISIBLE.index, TextEditor::CURSOR_VISIBLE.index] {
        engine.watch(obj, p, move |e, _| sync_blink(e, obj))?;
    }
    engine.watch(obj, TextEditor::CURSOR_DELEGATE.index, move |e, _| {
        check(e, "cursorDelegate", sync_delegate(e, obj));
        sync_blink(e, obj);
    })?;

    // The watches above gave the object its scope, so the node dies with
    // the item.
    if let Some(scope) = engine.object_scope(obj) {
        let n = engine.graph().property(scope, Value::Int(0))?;
        st.borrow_mut().paint_epoch = Some(n);
        let n = engine.graph().property(scope, Value::Int(0))?;
        st.borrow_mut().shape_epoch = Some(n);
        if multi {
            let n = engine.graph().property(scope, Value::Int(0))?;
            st.borrow_mut().rich_epoch = Some(n);
        }
    }

    publish(engine, obj, &st)?;
    // Everything the layout depends on besides the text.
    engine.effect_in(obj, move |e| {
        let Some(st) = state_of(e, obj) else { return };
        let inputs = LayoutInputs::read(e, obj, multi);
        check(e, "layout", e.untrack(|| relayout(e, obj, &st, &inputs)));
    })?;

    if flag(engine, obj, Item::ACTIVE_FOCUS.index) {
        focus_changed(engine, obj, true);
    }
    sync_delegate(engine, obj)?;
    sync_blink(engine, obj);
    Ok(())
}

/// Takes what a program wrote to `text` and `cursorPosition` into the
/// model. Every way into an editor calls it first, since a write inside a
/// handler may not have reached the watches yet.
pub(crate) fn reconcile(engine: &Engine, obj: ObjectId, st: &SharedState) {
    {
        let s = st.borrow();
        if s.syncing || !s.initialised {
            return;
        }
    }
    // A rich text whose `text` is still to be made was not written by a
    // program since (a write takes the making away): there is nothing to
    // take in, and reading it would write it out.
    let unwritten = engine.is_made_on_read(obj, TextEditor::TEXT.index);
    let (text, cursor) = if engine.graph().is_tracking() {
        // A binding is being evaluated and called a method of the editor
        // (`cursorPosition: positionAt(x, y)`). It may be the binding of
        // one of these two, and reading that would be a binding loop: what
        // was last written is taken, and the watches bring the rest.
        match engine.with(obj, |t: &TextEditor| (t.text.clone(), t.cursor_position)) {
            Ok((text, cursor)) => (
                (!unwritten).then(|| Value::String(text).to_js_string()),
                cursor,
            ),
            Err(_) => return,
        }
    } else {
        engine.untrack(|| {
            (
                (!unwritten).then(|| engine.read(obj, TextEditor::TEXT.index).to_js_string()),
                engine
                    .read(obj, TextEditor::CURSOR_POSITION.index)
                    .to_number() as i32,
            )
        })
    };
    let text = text.unwrap_or_default();
    let differs = !unwritten && super::rich::differs(&st.borrow(), text.as_str());
    let (format, font) = if differs {
        let multi = st.borrow().multi;
        super::rich::inputs(engine, obj, multi, text.as_str())
    } else {
        (None, std::sync::Arc::default())
    };
    let changed = {
        let mut s = st.borrow_mut();
        let s = &mut *s;
        // Asked before the text is taken: assigning a text moves the
        // cursor, and that is not the program writing `cursorPosition`.
        let cursor_written = usize::try_from(cursor) != Ok(s.model.cursor());
        let mut changed = false;
        if differs {
            super::rich::assign(s, format, text.as_str(), &font);
            if s.multi {
                s.model.move_to(0, false);
            }
            s.preedit.clear();
            s.preedit_cursor = None;
            s.editing = false;
            s.run = None;
            s.sticky = None;
            changed = true;
        }
        if cursor_written {
            if let Some(pos) = usize::try_from(cursor).ok().filter(|p| *p <= s.model.len()) {
                s.model.move_to(pos, false);
                s.run = None;
                s.sticky = None;
            }
            // A position the text does not have is ignored, and the
            // property is given back what the model holds.
            changed = true;
        }
        changed
    };
    if changed {
        check(engine, "text", publish(engine, obj, st));
    }
}

/// Writes what the model holds to the properties, lays the text out again
/// and shows the cursor.
pub(crate) fn publish(engine: &Engine, obj: ObjectId, st: &SharedState) -> Result<(), EngineError> {
    let read_only = flag(engine, obj, TextEditor::READ_ONLY.index);
    super::rich::refresh(engine, obj, st);
    let text = text_to_publish(engine, obj, st)?;
    let (cursor, selection, selected, len, undo, redo, preedit, multi) = {
        let s = st.borrow();
        let m = &s.model;
        let selected = match &s.rich {
            Some(_) => ljos_text::rich::RichText::plain_of(m.selected_text()),
            None => m.selected_text().to_owned(),
        };
        (
            m.cursor() as i32,
            m.selection(),
            selected,
            m.len() as i32,
            m.can_undo() && !read_only,
            m.can_redo() && !read_only,
            s.preedit.clone(),
            s.multi,
        )
    };
    st.borrow_mut().syncing = true;
    let r = engine.batch(|| -> Result<(), EngineError> {
        // `text` and `cursorPosition` may be bound; what the user does to
        // them leaves the binding in place, as in Qt.
        if let Some(text) = text {
            let now = engine.untrack(|| engine.read(obj, TextEditor::TEXT.index));
            if now.to_js_string().as_str() != text {
                engine.write_derived(obj, TextEditor::TEXT.index, Value::from(text.as_str()))?;
            }
        }
        let now = engine.untrack(|| engine.read(obj, TextEditor::CURSOR_POSITION.index));
        if now.to_number() as i32 != cursor {
            engine.write_derived(obj, TextEditor::CURSOR_POSITION.index, Value::Int(cursor))?;
        }
        let put = |p: ljos_engine::PropIndex, v: Value| write_computed(engine, obj, p, v);
        put(
            TextEditor::SELECTION_START.index,
            Value::Int(selection.start as i32),
        )?;
        put(
            TextEditor::SELECTION_END.index,
            Value::Int(selection.end as i32),
        )?;
        put(
            TextEditor::SELECTED_TEXT.index,
            Value::from(selected.as_str()),
        )?;
        put(TextEditor::LENGTH.index, Value::Int(len))?;
        put(TextEditor::CAN_UNDO.index, Value::Bool(undo))?;
        put(TextEditor::CAN_REDO.index, Value::Bool(redo))?;
        put(
            TextEditor::PREEDIT_TEXT.index,
            Value::from(preedit.as_str()),
        )?;
        put(
            TextEditor::INPUT_METHOD_COMPOSING.index,
            Value::Bool(!preedit.is_empty()),
        )?;
        let inputs = engine.untrack(|| LayoutInputs::read(engine, obj, multi));
        relayout(engine, obj, st, &inputs)
    });
    st.borrow_mut().syncing = false;
    super::highlight::sync_modified(engine, st);
    super::selection::sync(engine, obj, st);
    show_cursor(engine, st);
    r
}

/// What `text` is given now; `None`: it stays as it is. A rich text that
/// changed is written out at once only while something besides the editor
/// observes or binds `text`; otherwise it is made when `text` is next read,
/// so that typing does not write out the whole document on every key.
fn text_to_publish(
    engine: &Engine,
    obj: ObjectId,
    st: &SharedState,
) -> Result<Option<String>, EngineError> {
    let (changed, epoch, own) = {
        let s = st.borrow();
        let Some(r) = s.rich.as_ref() else {
            return Ok(Some(s.model.text().to_owned()));
        };
        (r.dirty, s.rich_epoch, s.text_watch)
    };
    if !changed {
        return Ok(None);
    }
    if let Some(n) = epoch {
        let g = engine.graph();
        let v = g.get_untracked(n).to_number() as i32;
        g.set(n, Value::Int(v.wrapping_add(1)));
    }
    let p = TextEditor::TEXT.index;
    let observed =
        engine.is_bound(obj, p) || engine.observers_of(obj, p).iter().any(|n| Some(*n) != own);
    if observed {
        return Ok(Some(super::rich::written_out(engine, obj, st)));
    }
    engine.make_on_read(obj, p, move |e| match state_of(e, obj) {
        Some(st) => Value::from(super::rich::written_out(e, obj, &st).as_str()),
        None => e
            .with(obj, |t: &TextEditor| Value::String(t.text.clone()))
            .unwrap_or_default(),
    })?;
    Ok(None)
}

/// Drops the layout and makes it again: the fonts changed.
pub(crate) fn refresh(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(engine, obj) else {
        return Ok(());
    };
    let multi = {
        let mut s = st.borrow_mut();
        if !s.initialised {
            return Ok(());
        }
        s.layout = None;
        s.laid = None;
        s.multi
    };
    let inputs = engine.untrack(|| LayoutInputs::read(engine, obj, multi));
    engine.untrack(|| relayout(engine, obj, &st, &inputs))
}

// --- focus ---------------------------------------------------------------

fn focus_changed(engine: &Engine, obj: ObjectId, active: bool) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    let read_only = flag(engine, obj, TextEditor::READ_ONLY.index);
    if active {
        check(
            engine,
            "cursorVisible",
            engine.write_derived(
                obj,
                TextEditor::CURSOR_VISIBLE.index,
                Value::Bool(!read_only),
            ),
        );
        return;
    }
    // A window that is no longer the active one takes the focus from its
    // items for a while; the selection waits for it to come back.
    let window_left = !focus::is_window_active(engine, focus::tree::top_of(engine, obj));
    let keeps = window_left || flag(engine, obj, TextEditor::PERSISTENT_SELECTION.index);
    {
        let mut s = st.borrow_mut();
        // What an input method was composing is not the user's text yet.
        s.preedit.clear();
        s.preedit_cursor = None;
        if !keeps {
            s.model.deselect();
        }
        s.editing = false;
        s.run = None;
        s.sticky = None;
        s.pointer = PointerState::default();
    }
    check(engine, "focus", publish(engine, obj, &st));
    check(
        engine,
        "cursorVisible",
        engine.write_derived(obj, TextEditor::CURSOR_VISIBLE.index, Value::Bool(false)),
    );
    // A text its validator does not accept is not finished (the oracle
    // says nothing then, for a field left with "5" where 10..99 is asked).
    if super::keys::acceptable(engine, obj, &st) {
        check(
            engine,
            "editingFinished",
            engine.emit_by_name(obj, "editingFinished", &[]),
        );
    }
}

// --- the cursor ----------------------------------------------------------

/// Whether the cursor blinks now: it is on screen, shown, and the editor
/// draws it itself.
fn blinks(engine: &Engine, obj: ObjectId, st: &SharedState) -> bool {
    st.borrow().delegate.is_none()
        && flag(engine, obj, Item::VISIBLE.index)
        && flag(engine, obj, TextEditor::CURSOR_VISIBLE.index)
}

/// Starts or stops the blink timer. It is an engine `Timer` the editor
/// owns: it runs on the engine's clock, wakes once per blink, and does not
/// run at all for a field without a visible cursor.
fn sync_blink(engine: &Engine, obj: ObjectId) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    let want = blinks(engine, obj, &st);
    let timer = st.borrow().blink_timer.filter(|t| engine.is_alive(*t));
    let r = match (want, timer) {
        (false, None) => Ok(()),
        (false, Some(t)) => engine.set(t, Timer::RUNNING, false),
        (true, Some(t)) => engine.invoke_by_name(t, "restart", &[]).map(|_| ()),
        (true, None) => engine
            .create_native_incomplete(Timer::default(), Some(obj))
            .and_then(|t| {
                engine.complete(t)?;
                engine.set(t, Timer::INTERVAL, BLINK_MS)?;
                engine.set(t, Timer::REPEAT, true)?;
                engine.connect_by_name(t, "triggered", move |e, _| blink(e, obj))?;
                st.borrow_mut().blink_timer = Some(t);
                engine.set(t, Timer::RUNNING, true)
            }),
    };
    check(engine, "cursor", r);
    let epoch = {
        let mut s = st.borrow_mut();
        s.blink_on = true;
        s.paint_epoch
    };
    if let Some(n) = epoch {
        bump(engine, n);
    }
}

fn blink(engine: &Engine, obj: ObjectId) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    let epoch = {
        let mut s = st.borrow_mut();
        s.blink_on = !s.blink_on;
        s.paint_epoch
    };
    if let Some(n) = epoch {
        bump(engine, n);
    }
}

/// After the cursor moved or the text changed the cursor is shown, and a
/// full blink passes before it hides.
fn show_cursor(engine: &Engine, st: &SharedState) {
    let timer = {
        let mut s = st.borrow_mut();
        s.blink_on = true;
        s.blink_timer
    };
    if let Some(t) = timer.filter(|t| engine.is_alive(*t))
        && engine
            .untrack(|| engine.read(t, Timer::RUNNING.index))
            .to_boolean()
    {
        check(
            engine,
            "cursor",
            engine.invoke_by_name(t, "restart", &[]).map(|_| ()),
        );
    }
}

/// Makes the item of `cursorDelegate`, or drops the one of a delegate that
/// is gone. The delegate follows `cursorRectangle` in position and
/// height; its width and whether it shows are its own (doc.qt.io,
/// `cursorDelegate`).
fn sync_delegate(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(engine, obj) else {
        return Ok(());
    };
    let component = match engine.untrack(|| engine.read(obj, TextEditor::CURSOR_DELEGATE.index)) {
        Value::Object(c) if engine.is_alive(c) => Some(c),
        _ => None,
    };
    let old = st.borrow().delegate;
    if old.map(|(c, _)| c) == component {
        return Ok(());
    }
    if let Some((_, item)) = st.borrow_mut().delegate.take()
        && engine.is_alive(item)
    {
        set_parent_item(engine, item, None)?;
        engine.destroy(item);
    }
    let Some(component) = component else {
        return Ok(());
    };
    let item = engine.component_create(component, Some(obj), Vec::new())?;
    if !is_item(engine, item) {
        engine.destroy(item);
        return Ok(());
    }
    let side = |pick: fn(&Rect) -> f64| {
        move |e: &Engine| match e.read(obj, TextEditor::CURSOR_RECTANGLE.index) {
            Value::Rect(r) => Value::Real(pick(&r)),
            _ => Value::Real(0.0),
        }
    };
    engine.bind(item, Item::X.index, side(|r| r.x))?;
    engine.bind(item, Item::Y.index, side(|r| r.y))?;
    engine.bind(item, Item::HEIGHT.index, side(|r| r.height))?;
    set_parent_item(engine, item, Some(obj))?;
    st.borrow_mut().delegate = Some((component, item));
    Ok(())
}
