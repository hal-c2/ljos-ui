//! `ComboBox` and `ItemDelegate` (QtQuick.Templates).
//!
//! A combo box shows one row of a model and has a popup with a delegate
//! for every row. It is not editable yet.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-combobox.html and
//! `-itemdelegate.html`. What a click, a key and the wheel do, what a
//! change of the model does to the current row, and how `find` reads its
//! flags were read off a running Qt 6.11.2; `features/controls/
//! combobox.feature` and `combobox-pointer.feature` pin them.
//!
//! TODO(listview): the delegates are children of the popup's content
//! item, in the order of the rows, and all of them are made when the
//! popup first opens. Qt's are in a `ListView` on `delegateModel`, which
//! reads null here until there is one.
//! TODO(scroll): a popup with more rows than fit in the window does not
//! scroll.
//! TODO(text-input): `editable` and what goes with it are stored.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_a11y::Role;
use ljos_engine::{
    Atom, ConnectionId, Cx, DelegateHost, Delegates, Engine, EngineError, ItemModel, MetaRef,
    ModelChange, ObjectId, PropIndex, QmlObject, QmlType, RemovalOrder, Value, WriteKind, qml_impl,
};
use ljos_quick::flickable::{WheelItem, register_wheel_item};
use ljos_quick::focus::{FocusReason, force_active_focus};
use ljos_quick::keys::{KeyItem, key, register_key_item};
use ljos_quick::{
    Item, KeyEvent, PointerEvent, PointerItem, WheelEvent, contains_point, is_item,
    register_pointer_item, set_parent_item,
};
use ljos_render::kurbo;

use crate::button::AbstractButton;
use crate::control::{Control, hover, is_left, takes_input};
use crate::parts::{PartKind, declared_by_style, manage_part};
use crate::popup::{self, Popup, close_policy};
use crate::util::{bind_default, check, flag, is_a, is_unset, num, object, set_computed, warn};

/// `Qt.MatchFlag`, as `find` reads it.
mod match_flag {
    pub const CONTAINS: i32 = 1;
    pub const STARTS_WITH: i32 = 2;
    pub const ENDS_WITH: i32 = 3;
    pub const REGULAR_EXPRESSION: i32 = 4;
    pub const WILDCARD: i32 = 5;
    /// The bits that say how to compare.
    pub const HOW: i32 = 0x0f;
    pub const CASE_SENSITIVE: i32 = 16;
}

// --------------------------------------------------------- ItemDelegate

/// `ItemDelegate`: a button that is one row of a list.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "ItemDelegate", extends = AbstractButton)]
pub struct ItemDelegate {
    #[qml(base)]
    pub button: AbstractButton,
    /// The row is the one the keys or the pointer are on.
    #[qml(notify)]
    pub highlighted: bool,
}

// ------------------------------------------------------------- ComboBox

/// What a combo box remembers that QML does not see.
#[derive(Default)]
pub(crate) struct ComboState {
    /// The rows, read from `model`.
    model: RefCell<Option<Rc<dyn ItemModel>>>,
    /// The combo box's own ear on the model.
    subscription: Cell<u64>,
    /// The delegates in the popup, once it was opened.
    delegates: RefCell<Option<Delegates>>,
    /// The value the combo box last gave `currentValue`: a write of
    /// anything else is the application's.
    own_value: RefCell<Value>,
    /// A press of the pointer that landed on the combo box has not ended.
    held: Cell<bool>,
    /// Space, Return or Enter is down.
    key_held: Cell<bool>,
    /// The popup the combo box listens to.
    wired: RefCell<Option<(ObjectId, ConnectionId)>>,
}

impl Drop for ComboState {
    fn drop(&mut self) {
        // The model may outlive the combo box, and would go on telling it.
        if let Some(model) = self.model.get_mut().take() {
            model.unsubscribe(self.subscription.get());
        }
    }
}

impl std::fmt::Debug for ComboState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComboState")
            .field("rows", &self.rows())
            .field("held", &self.held.get())
            .finish()
    }
}

impl ComboState {
    fn model(&self) -> Option<Rc<dyn ItemModel>> {
        self.model.borrow().clone()
    }

    fn rows(&self) -> usize {
        self.model.borrow().as_ref().map_or(0, |m| m.row_count())
    }

    fn delegates(&self) -> Option<Delegates> {
        self.delegates.borrow().clone()
    }
}

fn combo_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "ImplicitContentWidthPolicy",
        vec![
            ("ContentItemImplicitWidth", 0),
            ("WidestText", 1),
            ("WidestTextWhenCompleted", 2),
        ],
    )]
}

/// `ComboBox`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ComboBox",
    extends = Control,
    enums = combo_enums
)]
pub struct ComboBox {
    #[qml(base)]
    pub control: Control,
    #[qml(notify, readonly)]
    pub count: i32,
    /// A number, a list or a model object: what `Repeater` takes.
    #[qml(notify)]
    pub model: Value,
    /// TODO(listview): the model of the popup's list; null until then.
    /// It says it changed whenever a row did, which is what a binding on
    /// `textAt` or `valueAt` waits for.
    #[qml(notify, readonly)]
    pub delegate_model: Value,
    #[qml(notify, readonly)]
    pub pressed: bool,
    /// The row of the open popup the keys or the pointer are on.
    #[qml(notify, readonly)]
    pub highlighted_index: i32,
    #[qml(notify)]
    pub current_index: i32,
    #[qml(notify, readonly)]
    pub current_text: Atom,
    /// Follows `currentText` until it is given a value.
    #[qml(notify)]
    pub display_text: Atom,
    #[qml(notify)]
    pub text_role: Atom,
    #[qml(notify)]
    pub value_role: Atom,
    /// Writing it makes the row that has the value current.
    #[qml(notify)]
    pub current_value: Value,
    #[qml(notify)]
    pub delegate: Option<ObjectId>,
    #[qml(notify)]
    pub indicator: Option<ObjectId>,
    #[qml(notify)]
    pub popup: Option<ObjectId>,
    #[qml(notify)]
    pub flat: bool,
    /// Follows `pressed` and the popup until it is given a value.
    #[qml(notify)]
    pub down: bool,
    #[qml(notify)]
    pub editable: bool,
    #[qml(notify)]
    pub edit_text: Atom,
    #[qml(notify)]
    pub validator: Option<ObjectId>,
    #[qml(notify)]
    pub input_method_hints: i32,
    #[qml(notify, readonly)]
    pub input_method_composing: bool,
    #[qml(notify, readonly)]
    pub acceptable_input: bool,
    #[qml(notify)]
    pub select_text_by_mouse: bool,
    #[qml(notify, readonly)]
    pub implicit_indicator_width: f64,
    #[qml(notify, readonly)]
    pub implicit_indicator_height: f64,
    /// Stored: the content item's own implicit width is what counts.
    #[qml(notify, enumeration = "ImplicitContentWidthPolicy")]
    pub implicit_content_width_policy: i32,
    #[qml(skip)]
    pub(crate) state: Rc<ComboState>,
}

impl Default for ComboBox {
    fn default() -> Self {
        // `Qt.StrongFocus`: a combo box takes the keyboard on a click and
        // on Tab.
        let mut control = Control {
            focus_policy: 11,
            ..Control::default()
        };
        control.item.active_focus_on_tab = true;
        Self {
            control,
            count: 0,
            model: Value::Undefined,
            delegate_model: Value::Null,
            pressed: false,
            highlighted_index: -1,
            current_index: -1,
            current_text: Atom::EMPTY,
            display_text: Atom::EMPTY,
            text_role: Atom::EMPTY,
            value_role: Atom::EMPTY,
            current_value: Value::Undefined,
            delegate: None,
            indicator: None,
            popup: None,
            flat: false,
            down: false,
            editable: false,
            edit_text: Atom::EMPTY,
            validator: None,
            input_method_hints: 0,
            input_method_composing: false,
            acceptable_input: true,
            select_text_by_mouse: false,
            implicit_indicator_width: 0.0,
            implicit_indicator_height: 0.0,
            implicit_content_width_policy: 0,
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl ComboBox {
    /// The user picked a row: with the pointer, with the keys or with the
    /// wheel. The row may be the one that was current already.
    #[qml(signal)]
    fn activated(index: i32);

    /// The user moved to a row of the open popup.
    #[qml(signal)]
    fn highlighted(index: i32);

    /// TODO(text-input): Return in an editable combo box.
    #[qml(signal)]
    fn accepted();

    /// What the Down key does: the next row becomes current, or
    /// highlighted while the popup is open.
    #[qml(invokable, name = "incrementCurrentIndex")]
    fn increment_current_index(cx: &Cx) -> Result<(), EngineError> {
        step(cx.engine, cx.this, Step::Next);
        Ok(())
    }

    #[qml(invokable, name = "decrementCurrentIndex")]
    fn decrement_current_index(cx: &Cx) -> Result<(), EngineError> {
        step(cx.engine, cx.this, Step::Previous);
        Ok(())
    }

    /// The first row whose text matches `text` the way `flags` (a
    /// `Qt.MatchFlag`) says; -1 when none does.
    #[qml(invokable)]
    fn find(cx: &Cx, text: Value, flags: Value) -> Value {
        let (e, combo) = (cx.engine, cx.this);
        depend_on_rows(e, combo);
        let flags = match flags {
            Value::Undefined => 0,
            f => f.to_number() as i32,
        };
        let wanted = e.to_js_string(&text);
        Value::Int(find(e, combo, wanted.as_str(), flags))
    }

    /// The row whose value is `value`; -1 when none has it.
    #[qml(invokable, name = "indexOfValue")]
    fn index_of_value(cx: &Cx, value: Value) -> Value {
        depend_on_rows(cx.engine, cx.this);
        Value::Int(index_of_value(cx.engine, cx.this, &value))
    }

    /// TODO(text-input): selects the text of an editable combo box.
    #[qml(invokable, name = "selectAll")]
    fn select_all(cx: &Cx) -> Result<(), EngineError> {
        let _ = cx;
        Ok(())
    }

    #[qml(invokable, name = "textAt")]
    fn text_at(cx: &Cx, index: Value) -> Value {
        let (e, combo) = (cx.engine, cx.this);
        depend_on_rows(e, combo);
        let text = row_arg(&index).map_or(Atom::EMPTY, |row| text_of(e, combo, row));
        Value::String(text)
    }

    #[qml(invokable, name = "valueAt")]
    fn value_at(cx: &Cx, index: Value) -> Value {
        let (e, combo) = (cx.engine, cx.this);
        depend_on_rows(e, combo);
        row_arg(&index).map_or(Value::Undefined, |row| value_of(e, combo, row))
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_combo(cx.engine, cx.this)
    }
}

fn state_of(engine: &Engine, combo: ObjectId) -> Option<Rc<ComboState>> {
    engine.with::<ComboBox, _>(combo, |c| c.state.clone()).ok()
}

/// A binding that asks about a row is run again when the rows or the
/// roles change: a delegate's `text: control.textAt(index)` follows a row
/// that is renamed.
fn depend_on_rows(engine: &Engine, combo: ObjectId) {
    for p in [
        ComboBox::DELEGATE_MODEL.index,
        ComboBox::TEXT_ROLE.index,
        ComboBox::VALUE_ROLE.index,
    ] {
        let _ = engine.read(combo, p);
    }
}

/// Runs what depends on the rows again ([`depend_on_rows`]).
fn rows_changed(engine: &Engine, combo: ObjectId) {
    let r = engine.notify(combo, ComboBox::DELEGATE_MODEL.index);
    check(engine, "delegateModel", r);
}

fn row_arg(index: &Value) -> Option<usize> {
    let n = index.as_number()?;
    (n >= 0.0 && n.is_finite()).then_some(n as usize)
}

fn int(engine: &Engine, obj: ObjectId, p: PropIndex) -> i32 {
    engine.untrack(|| num(engine, obj, p)) as i32
}

fn popup_of(engine: &Engine, combo: ObjectId) -> Option<ObjectId> {
    engine
        .untrack(|| object(engine, combo, ComboBox::POPUP.index))
        .filter(|p| is_a::<Popup>(engine, *p))
}

fn is_open(engine: &Engine, combo: ObjectId) -> bool {
    popup_of(engine, combo)
        .is_some_and(|p| engine.untrack(|| flag(engine, p, Popup::VISIBLE.index)))
}

/// Writes a property the application may have bound, as the type's own
/// code does: the binding stays.
fn put(engine: &Engine, combo: ObjectId, prop: PropIndex, v: Value) {
    check(engine, "ComboBox", popup::put(engine, combo, prop, v));
}

fn emit(engine: &Engine, combo: ObjectId, signal: &str, index: i32) {
    if engine.is_alive(combo) {
        let r = engine.emit_by_name(combo, signal, &[Value::Int(index)]);
        check(engine, signal, r);
    }
}

// ------------------------------------------------------------- the rows

/// One role of a row. A row of a list of JS objects has its roles in the
/// object.
fn role_of(engine: &Engine, model: &dyn ItemModel, row: usize, role: &str) -> Value {
    match model.data(engine, row, role) {
        Value::Undefined => match model.model_data(engine, row) {
            Some(data @ Value::Js(_)) => engine.js_member(&data, role).unwrap_or_default(),
            _ => Value::Undefined,
        },
        v => v,
    }
}

/// The text of a row: its text role, or the row itself where a row is
/// one value. A row that has neither reads as no text.
fn text_of(engine: &Engine, combo: ObjectId, row: usize) -> Atom {
    let Some(model) = state_of(engine, combo).and_then(|s| s.model()) else {
        return Atom::EMPTY;
    };
    if row >= model.row_count() {
        return Atom::EMPTY;
    }
    let role = engine.untrack(|| engine.get(combo, ComboBox::TEXT_ROLE).unwrap_or_default());
    let v = if role.is_empty() {
        model.model_data(engine, row).unwrap_or_default()
    } else {
        role_of(engine, &*model, row, role.as_str())
    };
    match v {
        Value::Undefined | Value::Null | Value::Object(_) | Value::Js(_) | Value::List(_) => {
            Atom::EMPTY
        }
        v => engine.to_js_string(&v),
    }
}

fn value_of(engine: &Engine, combo: ObjectId, row: usize) -> Value {
    let Some(model) = state_of(engine, combo).and_then(|s| s.model()) else {
        return Value::Undefined;
    };
    if row >= model.row_count() {
        return Value::Undefined;
    }
    let role = engine.untrack(|| engine.get(combo, ComboBox::VALUE_ROLE).unwrap_or_default());
    if role.is_empty() {
        model.model_data(engine, row).unwrap_or_default()
    } else {
        role_of(engine, &*model, row, role.as_str())
    }
}

/// `==` as far as a value of a row goes: numbers, strings and booleans
/// compare by what they say, anything else by what it is.
fn same_value(a: &Value, b: &Value) -> bool {
    if a.strict_equals(b) {
        return true;
    }
    let plain = |v: &Value| {
        matches!(
            v,
            Value::Bool(_) | Value::Int(_) | Value::Real(_) | Value::String(_)
        )
    };
    if !plain(a) || !plain(b) {
        return false;
    }
    match (a, b) {
        (Value::String(_), Value::String(_)) => false,
        (Value::String(s), n) | (n, Value::String(s)) => {
            s.as_str().trim().parse::<f64>().ok() == Some(n.to_number())
        }
        _ => a.to_number() == b.to_number(),
    }
}

fn index_of_value(engine: &Engine, combo: ObjectId, value: &Value) -> i32 {
    if matches!(value, Value::Undefined | Value::Null) {
        return -1;
    }
    let rows = state_of(engine, combo).map_or(0, |s| s.rows());
    (0..rows)
        .find(|row| same_value(&value_of(engine, combo, *row), value))
        .map_or(-1, |row| row as i32)
}

/// Whether `text` matches `pattern`, in which `*` stands for any run of
/// characters and `?` for any one.
fn wildcard(pattern: &[char], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|skip| wildcard(rest, &text[skip..])),
        Some(('?', rest)) => !text.is_empty() && wildcard(rest, &text[1..]),
        Some((c, rest)) => text.first() == Some(c) && wildcard(rest, &text[1..]),
    }
}

fn find(engine: &Engine, combo: ObjectId, wanted: &str, flags: i32) -> i32 {
    let how = flags & match_flag::HOW;
    if how == match_flag::REGULAR_EXPRESSION {
        warn(
            engine,
            "ComboBox.find: Qt.MatchRegularExpression is not supported",
        );
        return -1;
    }
    // Looking exactly compares the strings as they are, whatever the
    // flag for the case says (checked on qml6).
    let fold = how != 0 && flags & match_flag::CASE_SENSITIVE == 0;
    let cased = |s: &str| if fold { s.to_lowercase() } else { s.to_owned() };
    let wanted = cased(wanted);
    let pattern: Vec<char> = wanted.chars().collect();
    let rows = state_of(engine, combo).map_or(0, |s| s.rows());
    (0..rows)
        .find(|row| {
            let text = cased(text_of(engine, combo, *row).as_str());
            match how {
                match_flag::CONTAINS => text.contains(&wanted),
                match_flag::STARTS_WITH => text.starts_with(&wanted),
                match_flag::ENDS_WITH => text.ends_with(&wanted),
                match_flag::WILDCARD => wildcard(&pattern, &text.chars().collect::<Vec<char>>()),
                // Exactly (0) and as a fixed string (8): the whole text.
                _ => text == wanted,
            }
        })
        .map_or(-1, |row| row as i32)
}

/// Shows the row `currentIndex` names: its text and its value, or none
/// where the index names no row.
fn refresh(engine: &Engine, combo: ObjectId) {
    let Some(st) = state_of(engine, combo) else {
        return;
    };
    let current = int(engine, combo, ComboBox::CURRENT_INDEX.index);
    let (text, value) = match usize::try_from(current) {
        Ok(row) if row < st.rows() => (text_of(engine, combo, row), value_of(engine, combo, row)),
        _ => (Atom::EMPTY, Value::Undefined),
    };
    let r = set_computed(
        engine,
        combo,
        ComboBox::CURRENT_TEXT.index,
        Value::String(text),
    );
    check(engine, "currentText", r);
    *st.own_value.borrow_mut() = value.clone();
    put(engine, combo, ComboBox::CURRENT_VALUE.index, value);
}

fn set_count(engine: &Engine, combo: ObjectId, st: &ComboState) {
    let n = Value::Int(st.rows() as i32);
    check(
        engine,
        "count",
        set_computed(engine, combo, ComboBox::COUNT.index, n),
    );
}

fn set_current(engine: &Engine, combo: ObjectId, index: i32) {
    put(
        engine,
        combo,
        ComboBox::CURRENT_INDEX.index,
        Value::Int(index),
    );
    // Now, not when the watch gets to it: what is said next (`activated`)
    // finds the row's text in place.
    refresh(engine, combo);
}

/// Takes the rows of `model`. With `restart` the first row becomes
/// current, as Qt does for a model given after the combo box was made.
fn adopt_model(engine: &Engine, combo: ObjectId, restart: bool) {
    let Some(st) = state_of(engine, combo) else {
        return;
    };
    let value = engine.untrack(|| engine.read(combo, ComboBox::MODEL.index));
    let model = engine.untrack(|| engine.model_from_value(&value, Some(combo)));
    let id = model.subscribe(Rc::new(move |e: &Engine, change: &ModelChange| {
        model_changed(e, combo, change);
    }));
    let old = st.model.replace(Some(model));
    if let Some(old) = old {
        old.unsubscribe(st.subscription.get());
    }
    st.subscription.set(id);
    set_count(engine, combo, &st);
    if restart {
        let first = if st.rows() > 0 { 0 } else { -1 };
        put(
            engine,
            combo,
            ComboBox::CURRENT_INDEX.index,
            Value::Int(first),
        );
    }
    refresh(engine, combo);
    rows_changed(engine, combo);
    resource(engine, combo, &st);
}

/// The model's rows changed. The index stays where it is, whatever row
/// it then names, except that an emptied model has no current row.
fn model_changed(engine: &Engine, combo: ObjectId, _change: &ModelChange) {
    if !engine.is_alive(combo) {
        return;
    }
    let Some(st) = state_of(engine, combo) else {
        return;
    };
    set_count(engine, combo, &st);
    if st.rows() == 0 {
        put(engine, combo, ComboBox::CURRENT_INDEX.index, Value::Int(-1));
    }
    refresh(engine, combo);
    rows_changed(engine, combo);
}

// -------------------------------------------------------- the delegates

/// Where the delegates go: the content item of the popup.
fn content_of(engine: &Engine, combo: ObjectId) -> Option<ObjectId> {
    let popup = popup_of(engine, combo)?;
    engine
        .untrack(|| object(engine, popup, Popup::CONTENT_ITEM.index))
        .filter(|c| is_item(engine, *c))
}

/// Puts `items` at the end of `parent`'s lists of children, in their
/// order: a positioner lays its children out in the order of the list.
fn order(engine: &Engine, parent: ObjectId, items: &[ObjectId]) -> Result<(), EngineError> {
    for p in [Item::CHILDREN.index, Item::DATA.index] {
        let Value::List(old) = engine.untrack(|| engine.read(parent, p)) else {
            continue;
        };
        let mine = |v: &Value| v.as_object().is_some_and(|o| items.contains(&o));
        let mut new: Vec<Value> = old.iter().filter(|v| !mine(v)).cloned().collect();
        new.extend(
            items
                .iter()
                .filter(|i| old.iter().any(|v| v.as_object() == Some(**i)))
                .map(|i| Value::Object(*i)),
        );
        if new.as_slice() != old.as_slice() {
            engine.write_internal(parent, p, Value::list(new), WriteKind::Initial)?;
        }
    }
    Ok(())
}

struct Host(ObjectId);

impl Host {
    /// The delegates that exist, with `object` where row `index` puts it.
    fn with(&self, engine: &Engine, object: ObjectId, before: Option<ObjectId>) -> Vec<ObjectId> {
        let mut all = state_of(engine, self.0)
            .and_then(|s| s.delegates())
            .map(|d| d.objects())
            .unwrap_or_default();
        all.retain(|o| *o != object);
        let at = before
            .and_then(|b| all.iter().position(|o| *o == b))
            .unwrap_or(all.len());
        all.insert(at, object);
        all
    }
}

impl DelegateHost for Host {
    fn allocated(&self, e: &Engine, _: usize, object: ObjectId, before: Option<ObjectId>) {
        let combo = self.0;
        if !is_item(e, object) {
            warn(e, "ComboBox: Delegate must be of Item type");
            return;
        }
        let Some(content) = content_of(e, combo) else {
            return;
        };
        // Before the delegate's bindings run, so that `parent` reads
        // right in them.
        let placed = e.batch(|| {
            set_parent_item(e, object, Some(content))?;
            order(e, content, &self.with(e, object, before))
        });
        check(e, "delegate", placed);
    }

    fn added(&self, e: &Engine, _: usize, object: ObjectId) {
        let combo = self.0;
        if is_a::<AbstractButton>(e, object) {
            let r = e.connect_by_name(object, "clicked", move |e, _| {
                if let Some(row) = row_of(e, combo, object) {
                    pick(e, combo, row);
                }
            });
            check(e, "delegate", r.map(|_| ()));
        }
        if is_a::<Control>(e, object) {
            let r = e.connect_by_name(object, "hoveredChanged", move |e, _| {
                let over = e.untrack(|| flag(e, object, Control::HOVERED.index));
                if over
                    && is_open(e, combo)
                    && let Some(row) = row_of(e, combo, object)
                {
                    highlight(e, combo, row, true);
                }
            });
            check(e, "delegate", r.map(|_| ()));
        }
    }

    fn removed(&self, e: &Engine, _: usize, object: ObjectId) {
        // Out of the popup now; the object itself goes at the next frame.
        if is_item(e, object) {
            let _ = set_parent_item(e, object, None);
        }
    }

    fn moved(&self, e: &Engine) {
        let combo = self.0;
        let (Some(content), Some(delegates)) = (
            content_of(e, combo),
            state_of(e, combo).and_then(|s| s.delegates()),
        ) else {
            return;
        };
        check(e, "delegate", order(e, content, &delegates.objects()));
    }

    fn count_changed(&self, _: &Engine, _: usize) {}
}

fn row_of(engine: &Engine, combo: ObjectId, delegate: ObjectId) -> Option<i32> {
    let delegates = state_of(engine, combo)?.delegates()?;
    let at = delegates.objects().iter().position(|o| *o == delegate)?;
    Some(at as i32)
}

/// Makes the delegates when the popup is first opened: a combo box that
/// is never opened has none.
fn ensure_delegates(engine: &Engine, combo: ObjectId) {
    let Some(st) = state_of(engine, combo) else {
        return;
    };
    if st.delegates.borrow().is_some() {
        return;
    }
    let delegates = Delegates::new(combo, Rc::new(Host(combo)), RemovalOrder::FirstToLast);
    *st.delegates.borrow_mut() = Some(delegates);
    if let Some(p) = popup_of(engine, combo) {
        // The popup is a list of rows, not a dialog.
        check(engine, "popup", popup::set_role(engine, p, Role::List));
    }
    resource(engine, combo, &st);
}

/// Makes the delegates again, of the delegate and the rows there are now.
fn resource(engine: &Engine, combo: ObjectId, st: &ComboState) {
    let Some(delegates) = st.delegates() else {
        return;
    };
    let component = engine.untrack(|| object(engine, combo, ComboBox::DELEGATE.index));
    let model = st.model().filter(|_| content_of(engine, combo).is_some());
    engine.untrack(|| delegates.set_source(engine, model, component));
}

// ------------------------------------------------------------ the popup

fn open_popup(engine: &Engine, combo: ObjectId) {
    if let Some(p) = popup_of(engine, combo) {
        check(engine, "popup", popup::open(engine, p));
    }
}

fn close_popup(engine: &Engine, combo: ObjectId) {
    if let Some(p) = popup_of(engine, combo) {
        check(engine, "popup", popup::close(engine, p));
    }
}

/// Moves the highlight of the open popup. `by_user` says the user moved
/// it, which the application is told; that the popup opened on the
/// current row is not said (checked on qml6).
fn highlight(engine: &Engine, combo: ObjectId, index: i32, by_user: bool) {
    let p = ComboBox::HIGHLIGHTED_INDEX.index;
    if int(engine, combo, p) == index {
        return;
    }
    check(
        engine,
        "highlightedIndex",
        set_computed(engine, combo, p, Value::Int(index)),
    );
    if by_user {
        emit(engine, combo, "highlighted", index);
    }
}

/// The user chose a row of the open popup.
fn pick(engine: &Engine, combo: ObjectId, index: i32) {
    if int(engine, combo, ComboBox::CURRENT_INDEX.index) != index {
        set_current(engine, combo, index);
    }
    emit(engine, combo, "activated", index);
    if engine.is_alive(combo) {
        close_popup(engine, combo);
    }
}

#[derive(Clone, Copy)]
enum Step {
    Previous,
    Next,
    First,
    Last,
    /// The next row whose text starts with the character, going round.
    Letter(char),
}

/// Moves on from the highlighted row of the open popup, or from the
/// current row of a closed one, which the user is then said to have
/// activated. True if there was a row to move to.
fn step(engine: &Engine, combo: ObjectId, step: Step) -> bool {
    let rows = state_of(engine, combo).map_or(0, |s| s.rows()) as i32;
    let open = is_open(engine, combo);
    let from = if open {
        int(engine, combo, ComboBox::HIGHLIGHTED_INDEX.index)
    } else {
        int(engine, combo, ComboBox::CURRENT_INDEX.index)
    };
    let to = match step {
        Step::Previous if from > 0 => from - 1,
        Step::Next if from < rows - 1 => from + 1,
        Step::First if rows > 0 => 0,
        Step::Last if rows > 0 => rows - 1,
        Step::Letter(c) => {
            let starts = |row: &i32| {
                text_of(engine, combo, *row as usize)
                    .as_str()
                    .chars()
                    .next()
                    .is_some_and(|first| first.to_lowercase().eq(c.to_lowercase()))
            };
            let after = from.max(-1) + 1;
            (after..rows)
                .chain(0..after.min(rows))
                .find(starts)
                .unwrap_or(from)
        }
        _ => from,
    };
    if to == from {
        return false;
    }
    if open {
        highlight(engine, combo, to, true);
    } else {
        set_current(engine, combo, to);
        emit(engine, combo, "activated", to);
    }
    true
}

/// The popup opened or closed: it opens with the current row highlighted
/// and closes with none.
fn popup_switched(engine: &Engine, combo: ObjectId, open: bool) {
    if open {
        ensure_delegates(engine, combo);
        let current = int(engine, combo, ComboBox::CURRENT_INDEX.index);
        highlight(engine, combo, current, false);
    } else {
        highlight(engine, combo, -1, false);
    }
}

/// Listens to the popup the combo box has now.
fn wire_popup(engine: &Engine, combo: ObjectId, st: &ComboState) {
    let popup = popup_of(engine, combo);
    if st.wired.borrow().map(|(p, _)| p) == popup {
        return;
    }
    if let Some((_, conn)) = st.wired.take() {
        engine.disconnect(conn);
    }
    let Some(popup) = popup else {
        return;
    };
    // Before the popup is shown: it is placed with its rows in it.
    let conn = engine.connect_by_name(popup, "aboutToShow", move |e, _| {
        if e.is_alive(combo) {
            ensure_delegates(e, combo);
        }
    });
    match conn {
        Ok(c) => *st.wired.borrow_mut() = Some((popup, c)),
        Err(err) => check(engine, "popup", Err(err)),
    }
    // The delegates live in the content item of the popup they were made
    // for.
    resource(engine, combo, st);
}

// ----------------------------------------------------------------- init

fn init_combo(engine: &Engine, combo: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(engine, combo) else {
        return Ok(());
    };
    for (prop, size) in [
        (ComboBox::IMPLICIT_INDICATOR_WIDTH, Item::IMPLICIT_WIDTH),
        (ComboBox::IMPLICIT_INDICATOR_HEIGHT, Item::IMPLICIT_HEIGHT),
    ] {
        engine.bind_initial(combo, prop.index, move |e| {
            match object(e, combo, ComboBox::INDICATOR.index) {
                Some(i) if is_item(e, i) => Value::Real(num(e, i, size.index)),
                _ => Value::Real(0.0),
            }
        })?;
    }
    manage_part(engine, combo, ComboBox::INDICATOR.index, PartKind::Free)?;

    // The popup the style declared, when the application gave its own:
    // it was made, since a style file is instantiated whole, and nothing
    // can reach it.
    let current = popup_of(engine, combo);
    for child in engine.untrack(|| engine.children(combo)) {
        if is_a::<Popup>(engine, child)
            && Some(child) != current
            && declared_by_style(engine, child)
        {
            engine.destroy(child);
        }
    }

    bind_default(engine, combo, ComboBox::DOWN.index, move |e| {
        let open = object(e, combo, ComboBox::POPUP.index)
            .filter(|p| is_a::<Popup>(e, *p))
            .is_some_and(|p| flag(e, p, Popup::VISIBLE.index));
        Value::Bool(flag(e, combo, ComboBox::PRESSED.index) || open)
    })?;
    for p in [ComboBox::DISPLAY_TEXT.index, ComboBox::EDIT_TEXT.index] {
        bind_default(engine, combo, p, move |e| {
            e.read(combo, ComboBox::CURRENT_TEXT.index)
        })?;
    }

    // A declared `currentIndex` is kept, whatever the model has.
    let restart = is_unset(engine, combo, ComboBox::CURRENT_INDEX.index);
    adopt_model(engine, combo, restart);
    engine.watch(combo, ComboBox::MODEL.index, move |e, _| {
        adopt_model(e, combo, true);
    })?;
    for p in [
        ComboBox::CURRENT_INDEX.index,
        ComboBox::TEXT_ROLE.index,
        ComboBox::VALUE_ROLE.index,
    ] {
        engine.watch(combo, p, move |e, _| refresh(e, combo))?;
    }
    engine.watch(combo, ComboBox::CURRENT_VALUE.index, move |e, v| {
        let Some(st) = state_of(e, combo) else {
            return;
        };
        if v.change_equals(&st.own_value.borrow()) {
            return;
        }
        // The application asks for the row that has the value.
        let row = index_of_value(e, combo, v);
        set_current(e, combo, row);
    })?;
    engine.watch(combo, ComboBox::DELEGATE.index, move |e, _| {
        if let Some(st) = state_of(e, combo) {
            resource(e, combo, &st);
        }
    })?;

    // The popup: which one it is, and whether it is open.
    let state = st.clone();
    engine.effect_in(combo, move |e| {
        let popup = object(e, combo, ComboBox::POPUP.index).filter(|p| is_a::<Popup>(e, *p));
        let open = popup.is_some_and(|p| flag(e, p, Popup::VISIBLE.index));
        // Tracked: the delegates move when the popup gets another
        // content item.
        let _ = popup.map(|p| object(e, p, Popup::CONTENT_ITEM.index));
        e.untrack(|| {
            wire_popup(e, combo, &state);
            popup_switched(e, combo, open);
        });
    })?;

    // The popup does not outlive the combo box's being usable, nor its
    // having the keyboard.
    for p in [
        Item::ENABLED.index,
        Item::VISIBLE.index,
        Item::ACTIVE_FOCUS.index,
    ] {
        engine.watch(combo, p, move |e, v| {
            if !v.to_boolean() {
                cancel(e, combo);
                close_popup(e, combo);
            }
        })?;
    }
    crate::a11y::defaults(
        engine,
        combo,
        Role::ComboBox,
        Some(ComboBox::DISPLAY_TEXT.index),
    )
}

// ---------------------------------------------------------------- input

fn set_pressed(engine: &Engine, combo: ObjectId, on: bool) {
    let r = set_computed(engine, combo, ComboBox::PRESSED.index, Value::Bool(on));
    check(engine, "pressed", r);
}

fn toggle_popup(engine: &Engine, combo: ObjectId) {
    if is_open(engine, combo) {
        close_popup(engine, combo);
    } else {
        open_popup(engine, combo);
    }
}

fn press(engine: &Engine, combo: ObjectId, ev: &PointerEvent, _at: kurbo::Point) -> bool {
    if !takes_input(engine, combo) || !is_left(ev) {
        return false;
    }
    let Some(st) = state_of(engine, combo) else {
        return false;
    };
    st.held.set(true);
    force_active_focus(engine, combo, FocusReason::Mouse);
    set_pressed(engine, combo, true);
    true
}

fn moved(engine: &Engine, combo: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    if state_of(engine, combo).is_some_and(|s| s.held.get()) {
        // Leaving the combo box lets go of it; coming back takes it again.
        set_pressed(engine, combo, contains_point(engine, combo, at));
    }
}

fn release(engine: &Engine, combo: ObjectId, _ev: &PointerEvent, _at: kurbo::Point) {
    let Some(st) = state_of(engine, combo) else {
        return;
    };
    if !st.held.replace(false) {
        return;
    }
    let was_pressed = engine.untrack(|| flag(engine, combo, ComboBox::PRESSED.index));
    set_pressed(engine, combo, false);
    if was_pressed {
        toggle_popup(engine, combo);
    }
}

fn cancel(engine: &Engine, combo: ObjectId) {
    let Some(st) = state_of(engine, combo) else {
        return;
    };
    let held = st.held.replace(false);
    if st.key_held.replace(false) || held {
        set_pressed(engine, combo, false);
    }
}

fn no_key(_: &Engine, _: ObjectId, _: &KeyEvent) -> bool {
    false
}

/// The character a key types, when it is one a row could start with.
fn typed(ev: &KeyEvent) -> Option<char> {
    let mut chars = ev.text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_control() && !c.is_whitespace() => Some(c),
        (None, _) if (key::A..=key::Z).contains(&ev.key) => char::from_u32(ev.key as u32),
        _ => None,
    }
}

fn key_press(engine: &Engine, combo: ObjectId, ev: &KeyEvent) -> bool {
    if !takes_input(engine, combo) {
        return false;
    }
    let Some(st) = state_of(engine, combo) else {
        return false;
    };
    match ev.key {
        key::SPACE | key::RETURN | key::ENTER => {
            if !ev.auto_repeat {
                st.key_held.set(true);
                set_pressed(engine, combo, true);
            }
            true
        }
        key::ESCAPE => {
            // The popup does not have the keyboard, so its own Escape
            // never comes.
            let Some(p) = popup_of(engine, combo).filter(|_| is_open(engine, combo)) else {
                return false;
            };
            let policy = int(engine, p, Popup::CLOSE_POLICY.index);
            if policy & close_policy::ON_ESCAPE == 0 {
                return false;
            }
            close_popup(engine, combo);
            true
        }
        key::DOWN => {
            step(engine, combo, Step::Next);
            true
        }
        key::UP => {
            step(engine, combo, Step::Previous);
            true
        }
        key::HOME => {
            step(engine, combo, Step::First);
            true
        }
        key::END => {
            step(engine, combo, Step::Last);
            true
        }
        _ => match typed(ev) {
            Some(c) => {
                step(engine, combo, Step::Letter(c));
                true
            }
            None => false,
        },
    }
}

fn key_release(engine: &Engine, combo: ObjectId, ev: &KeyEvent) -> bool {
    if !matches!(ev.key, key::SPACE | key::RETURN | key::ENTER) {
        return false;
    }
    let Some(st) = state_of(engine, combo) else {
        return false;
    };
    if !st.key_held.replace(false) {
        return false;
    }
    set_pressed(engine, combo, false);
    if is_open(engine, combo) {
        // The highlighted row is picked, and the popup closes whether or
        // not there is one.
        let row = int(engine, combo, ComboBox::HIGHLIGHTED_INDEX.index);
        if row >= 0 {
            pick(engine, combo, row);
        } else {
            close_popup(engine, combo);
        }
    } else {
        open_popup(engine, combo);
    }
    true
}

fn wheel(engine: &Engine, combo: ObjectId, ev: &WheelEvent, _at: kurbo::Point) -> bool {
    let wanted = engine.untrack(|| flag(engine, combo, Control::WHEEL_ENABLED.index));
    if !wanted || !takes_input(engine, combo) || is_open(engine, combo) {
        return false;
    }
    // Away from the user is up the list.
    if ev.angle_delta.y > 0.0 {
        step(engine, combo, Step::Previous);
    } else if ev.angle_delta.y < 0.0 {
        step(engine, combo, Step::Next);
    }
    true
}

pub(crate) fn register(engine: &Engine) {
    let meta = || MetaRef::Static(ComboBox::static_meta());
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
            shortcut_override: no_key,
            press: key_press,
            release: key_release,
        },
    );
    register_wheel_item(engine, meta(), WheelItem { wheel });
}
