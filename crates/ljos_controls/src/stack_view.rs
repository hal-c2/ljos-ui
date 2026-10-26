//! `StackView` (QtQuick.Templates): a stack of pages, of which the top one
//! is shown.
//!
//! A page is pushed as an item, as a `Component` or as the URL of a file;
//! several pushed at once are made only when they are needed (the top one
//! at once, the others when they come to the top or are asked for with
//! `StackView.ForceLoad`). A page the view made is destroyed when it leaves
//! the stack; an item pushed as it is goes back to its parent, hidden.
//!
//! A page under the top one is hidden and frozen (ARCHITECTURE.md §8.3):
//! its effects, timers and animations wait until it is shown again. Only
//! the pages a transition moves are awake under the top one.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-stackview.html. The
//! order of the attached signals, which pages are made when, and where a
//! page that leaves goes were read off a running Qt 6.11.2;
//! `features/controls/stack-view.feature` pins them.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use ljos_engine::{
    Component, Cx, Engine, EngineError, MetaRef, Migrate, NodeId, ObjectId, OverrideContent,
    OverrideId, PropIndex, QmlObject, QmlType, RestoreMode, Value, WriteKind, qml_impl,
};
use ljos_quick::{Item, Passage, Transition, follows_implicit_size, is_item, set_parent_item};

use crate::control::Control;
use crate::util::{check, is_a, num, object, set_computed, warn};

fn stack_view_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "Status",
            vec![
                ("Inactive", 0),
                ("Deactivating", 1),
                ("Activating", 2),
                ("Active", 3),
            ],
        ),
        ("LoadBehavior", vec![("DontLoad", 0), ("ForceLoad", 1)]),
        (
            "Operation",
            vec![
                ("Transition", -1),
                ("Immediate", 0),
                ("PushTransition", 1),
                ("ReplaceTransition", 2),
                ("PopTransition", 3),
            ],
        ),
    ]
}

mod status {
    pub const INACTIVE: i32 = 0;
    pub const DEACTIVATING: i32 = 1;
    pub const ACTIVATING: i32 = 2;
    pub const ACTIVE: i32 = 3;
}

mod op {
    pub const IMMEDIATE: i32 = 0;
    pub const PUSH: i32 = 1;
    pub const REPLACE: i32 = 2;
    pub const POP: i32 = 3;
}

const FORCE_LOAD: i32 = 1;

/// Where a page comes from.
#[derive(Debug, Clone)]
enum Source {
    /// An item pushed as it is.
    Given(ObjectId),
    Component(ObjectId),
    Url(String),
}

/// A page of the stack, made or still to be made.
#[derive(Debug)]
struct Entry {
    source: Source,
    props: Vec<(String, Value)>,
    item: Option<ObjectId>,
    /// Made by the view, so destroyed when it leaves.
    owned: bool,
    /// The parent a given item had before it was pushed.
    home: Option<ObjectId>,
    width: Option<OverrideId>,
    height: Option<OverrideId>,
    /// What shows, hides and freezes the page as `StackView.visible` says.
    follow: Option<NodeId>,
}

impl Entry {
    fn new(source: Source, props: Vec<(String, Value)>) -> Self {
        Self {
            source,
            props,
            item: None,
            owned: false,
            home: None,
            width: None,
            height: None,
            follow: None,
        }
    }
}

/// What a page is told when its transition ends, or what waits for the
/// view to be still.
type Finish = Box<dyn FnOnce(&Engine)>;

/// A page an operation moves, until it is told its end.
struct PageRun {
    /// The operation, counted.
    op: u64,
    page: ObjectId,
    /// What moves it. `None`: it is told its end with the last run of its
    /// operation.
    transition: Option<ObjectId>,
    end: Finish,
}

#[derive(Default)]
pub(crate) struct StackState {
    entries: RefCell<Vec<Entry>>,
    /// The pages under way, in the order Qt tells pages whose runs end
    /// together.
    runs: RefCell<Vec<PageRun>>,
    /// What waits until nothing moves: pages that left the stack are
    /// removed after `busy` goes false (qml6).
    after: RefCell<Vec<Finish>>,
    ops: Cell<u64>,
    /// How many pages `initialItem` made, and how many of them are still
    /// at the bottom of the stack: what a reload makes again by itself.
    first: Cell<usize>,
    kept: Cell<usize>,
    done: Cell<bool>,
    /// What a reload carried, until the view completes.
    restore: RefCell<Option<CarriedPages>>,
}

impl fmt::Debug for StackState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StackState")
            .field("entries", &self.entries)
            .field("runs", &self.runs.borrow().len())
            .finish_non_exhaustive()
    }
}

/// `StackView`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "StackView",
    extends = Control,
    enums = stack_view_enums,
    attached = StackViewAttached
)]
pub struct StackView {
    #[qml(base)]
    pub control: Control,
    #[qml(notify, readonly)]
    pub busy: bool,
    #[qml(notify, readonly)]
    pub current_item: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub depth: i32,
    #[qml(notify, readonly)]
    pub empty: bool,
    /// The first page: an item, a `Component` or a URL.
    #[qml(notify)]
    pub initial_item: Value,
    #[qml(notify)]
    pub push_enter: Option<ObjectId>,
    #[qml(notify)]
    pub push_exit: Option<ObjectId>,
    #[qml(notify)]
    pub pop_enter: Option<ObjectId>,
    #[qml(notify)]
    pub pop_exit: Option<ObjectId>,
    #[qml(notify)]
    pub replace_enter: Option<ObjectId>,
    #[qml(notify)]
    pub replace_exit: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) state: Rc<StackState>,
}

/// What `StackView.status` and its neighbours read on a page.
#[derive(Debug, QmlObject)]
#[qml(name = "StackViewAttached", enums = stack_view_enums)]
pub struct StackViewAttached {
    #[qml(notify, readonly)]
    pub index: i32,
    #[qml(notify, readonly)]
    pub view: Option<ObjectId>,
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    /// Whether the page is shown. The view hides a page it covers; a page
    /// that wants to be seen under the one coming in can say so; taking it
    /// back leaves it to the view again.
    #[qml(notify, reset = SHOWN_BY_VIEW)]
    pub visible: bool,
}

const SHOWN_BY_VIEW: ljos_engine::Reset = ljos_engine::Reset::Absent {
    absent: |_| false,
    value: shown_by_view,
};

/// Whether the view shows the page: it shows the current one only.
fn shown_by_view(e: &Engine, att: ObjectId, _: PropIndex) -> Value {
    let page = e.owner(att);
    let shown = match object(e, att, StackViewAttached::VIEW.index) {
        Some(view) => object(e, view, StackView::CURRENT_ITEM.index) == page,
        None => true,
    };
    Value::Bool(shown)
}

impl Default for StackViewAttached {
    fn default() -> Self {
        Self {
            index: -1,
            view: None,
            status: status::INACTIVE,
            visible: true,
        }
    }
}

#[qml_impl]
impl StackViewAttached {
    #[qml(signal)]
    fn activated();
    #[qml(signal)]
    fn activating();
    #[qml(signal)]
    fn deactivated();
    #[qml(signal)]
    fn deactivating();
    #[qml(signal)]
    fn removed();
}

/// A request read from the arguments of a method: the pages, and the
/// operation if one was given.
#[derive(Debug, Default)]
struct Request {
    pages: Vec<Entry>,
    operation: Option<i32>,
}

fn is_number(v: &Value) -> bool {
    matches!(v, Value::Int(_) | Value::Real(_) | Value::Enum(_))
}

/// Reads pages and an operation from arguments as `push` takes them: an
/// item, a component or a URL, each followed by its properties, or an
/// array of those, then the operation.
fn request(e: &Engine, me: ObjectId, args: &[Value]) -> Request {
    let mut out = Request::default();
    let mut flat: Vec<Value> = Vec::new();
    for a in args {
        match a {
            Value::List(l) => flat.extend(l.iter().cloned()),
            other => flat.push(other.clone()),
        }
    }
    for v in flat {
        match &v {
            Value::Object(o) if e.is_alive(*o) && is_a::<Component>(e, *o) => {
                out.pages
                    .push(Entry::new(Source::Component(*o), Vec::new()));
            }
            Value::Object(o) if e.is_alive(*o) && is_item(e, *o) => {
                out.pages.push(Entry::new(Source::Given(*o), Vec::new()));
            }
            Value::String(s) | Value::Url(s) => {
                let url = e.resolve_url(me, s);
                out.pages.push(Entry::new(Source::Url(url), Vec::new()));
            }
            Value::Js(_) => {
                if let Some(last) = out.pages.last_mut().filter(|p| p.props.is_empty()) {
                    last.props = e.js_object_entries(&v);
                }
            }
            v if is_number(v) => out.operation = Some(v.to_number() as i32),
            _ => {}
        }
    }
    out
}

fn state_of(e: &Engine, me: ObjectId) -> Option<Rc<StackState>> {
    e.with::<StackView, _>(me, |s| s.state.clone()).ok()
}

fn attacher() -> MetaRef {
    MetaRef::Static(StackView::static_meta())
}

fn depth_of(e: &Engine, me: ObjectId) -> usize {
    state_of(e, me).map_or(0, |st| st.entries.borrow().len())
}

fn item_of(e: &Engine, me: ObjectId, index: usize) -> Option<ObjectId> {
    let st = state_of(e, me)?;
    let entries = st.entries.borrow();
    entries.get(index)?.item.filter(|i| e.is_alive(*i))
}

fn index_of(e: &Engine, me: ObjectId, item: ObjectId) -> Option<usize> {
    let st = state_of(e, me)?;
    let entries = st.entries.borrow();
    entries.iter().position(|en| en.item == Some(item))
}

fn reply(item: Option<ObjectId>) -> Value {
    item.map_or(Value::Null, Value::Object)
}

#[qml_impl]
impl StackView {
    /// `push(item, properties, operation)`, `push([items], operation)` and
    /// the other forms: the pages go on top, and the last is shown.
    #[qml(invokable)]
    fn push(cx: &Cx, a: Value, b: Value, c: Value, d: Value) -> Result<Value, EngineError> {
        let r = request(cx.engine, cx.this, &[a, b, c, d]);
        push(cx.engine, cx.this, r)
    }

    #[qml(invokable, name = "pushItem")]
    fn push_item(
        cx: &Cx,
        item: Value,
        props: Value,
        operation: Value,
    ) -> Result<Value, EngineError> {
        let r = request(cx.engine, cx.this, &[item, props, operation]);
        push(cx.engine, cx.this, r)
    }

    #[qml(invokable, name = "pushItems")]
    fn push_items(cx: &Cx, items: Value, operation: Value) -> Result<Value, EngineError> {
        let r = request(cx.engine, cx.this, &[items, operation]);
        push(cx.engine, cx.this, r)
    }

    /// `pop(item, operation)`: pops the top page, or every page above
    /// `item`, or every page but the first for `null`. Never the last.
    #[qml(invokable)]
    fn pop(cx: &Cx, a: Value, b: Value) -> Result<Value, EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let depth = depth_of(e, me);
        let (keep, operation) = match &a {
            Value::Object(o) => (index_of(e, me, *o).map(|i| i + 1), number(&b)),
            Value::Null => (Some(1), number(&b)),
            v if is_number(v) => (Some(depth.saturating_sub(1)), number(v)),
            _ => (Some(depth.saturating_sub(1)), number(&b)),
        };
        let Some(keep) = keep.filter(|k| *k >= 1 && *k < depth) else {
            return Ok(Value::Null);
        };
        pop_to(e, me, keep, operation).map(reply)
    }

    #[qml(invokable, name = "popToItem")]
    fn pop_to_item(
        cx: &Cx,
        item: Option<ObjectId>,
        operation: Value,
    ) -> Result<Value, EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let Some(keep) = item.and_then(|i| index_of(e, me, i)).map(|i| i + 1) else {
            return Ok(Value::Null);
        };
        pop_to(e, me, keep, number(&operation)).map(reply)
    }

    #[qml(invokable, name = "popToIndex")]
    fn pop_to_index(cx: &Cx, index: Value, operation: Value) -> Result<Value, EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let Some(i) = number(&index).and_then(|i| usize::try_from(i).ok()) else {
            return Ok(Value::Null);
        };
        pop_to(e, me, i + 1, number(&operation)).map(reply)
    }

    /// Pops the top page, the last one too.
    #[qml(invokable, name = "popCurrentItem")]
    fn pop_current_item(cx: &Cx, operation: Value) -> Result<Value, EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let depth = depth_of(e, me);
        if depth == 0 {
            return Ok(Value::Null);
        }
        pop_to(e, me, depth - 1, number(&operation)).map(reply)
    }

    /// `replace(target, item, properties, operation)`: the pages from
    /// `target` up (every page for `null`), or without a target the top
    /// one, are replaced.
    #[qml(invokable)]
    fn replace(cx: &Cx, a: Value, b: Value, c: Value, d: Value) -> Result<Value, EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let depth = depth_of(e, me);
        let pages_follow = |v: &Value| {
            matches!(
                v,
                Value::Object(_) | Value::String(_) | Value::Url(_) | Value::List(_)
            )
        };
        let target = match &a {
            Value::Null if pages_follow(&b) => Some(0),
            Value::Object(o) if pages_follow(&b) => index_of(e, me, *o),
            _ => None,
        };
        let (keep, r) = match target {
            Some(keep) => (keep, request(e, me, &[b, c, d])),
            None => (depth.saturating_sub(1), request(e, me, &[a, b, c, d])),
        };
        replace(e, me, keep, r)
    }

    #[qml(invokable, name = "replaceCurrentItem")]
    fn replace_current_item(cx: &Cx, a: Value, b: Value, c: Value) -> Result<Value, EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let keep = depth_of(e, me).saturating_sub(1);
        let r = request(e, me, &[a, b, c]);
        replace(e, me, keep, r)
    }

    /// Every page leaves; they are only told that they were removed.
    #[qml(invokable)]
    fn clear(cx: &Cx, _operation: Value) -> Result<(), EngineError> {
        clear(cx.engine, cx.this)
    }

    /// `get(index, behavior)`: the page at `index`, made first with
    /// `StackView.ForceLoad` if it was not.
    #[qml(invokable)]
    fn get(cx: &Cx, index: Value, behavior: Value) -> Result<Value, EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let Some(i) = number(&index).and_then(|i| usize::try_from(i).ok()) else {
            return Ok(Value::Null);
        };
        if i >= depth_of(e, me) {
            return Ok(Value::Null);
        }
        if number(&behavior) == Some(FORCE_LOAD) {
            return load(e, me, i).map(reply);
        }
        Ok(reply(item_of(e, me, i)))
    }

    /// `find(callback, behavior)`: the first page from the top for which
    /// `callback(item, index)` is true.
    #[qml(invokable)]
    fn find(cx: &Cx, callback: Value, behavior: Value) -> Result<Value, EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let force = number(&behavior) == Some(FORCE_LOAD);
        for i in (0..depth_of(e, me)).rev() {
            let item = if force {
                load(e, me, i)?
            } else {
                item_of(e, me, i)
            };
            let Some(item) = item else { continue };
            let said = e.call_js(&callback, &[Value::Object(item), Value::Int(i as i32)]);
            if said.is_some_and(|v| v.to_boolean()) {
                return Ok(Value::Object(item));
            }
        }
        Ok(Value::Null)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.effect_in(me, move |e| {
            let w = num(e, me, Control::AVAILABLE_WIDTH.index);
            let h = num(e, me, Control::AVAILABLE_HEIGHT.index);
            check(e, "StackView", e.untrack(|| fit(e, me, w, h)));
        })?;
        let initial = e.untrack(|| e.read(me, StackView::INITIAL_ITEM.index));
        let r = request(e, me, &[initial]);
        let Some(st) = state_of(e, me) else {
            return Ok(());
        };
        st.first.set(r.pages.len());
        st.kept.set(r.pages.len());
        st.entries.borrow_mut().extend(r.pages);
        restored(&st);
        st.done.set(true);
        let Some(top) = depth_of(e, me).checked_sub(1) else {
            return update(e, me);
        };
        let item = load(e, me, top)?;
        update(e, me)?;
        // The first page is only told that it is active (qml6).
        if let Some(item) = item {
            tell(e, item, status::ACTIVE, "activated");
        }
        Ok(())
    }
}

/// The pages a running application pushed, carried across a dev reload:
/// the view made from the new file shows them again, from their sources
/// (a page's own state starts afresh).
#[derive(Debug)]
pub struct CarriedPages {
    kept: usize,
    pages: Vec<Entry>,
}

impl Migrate for StackView {
    const VERSION: u32 = 1;
    type State = CarriedPages;

    fn snapshot(engine: &Engine, obj: ObjectId) -> Option<CarriedPages> {
        // Not for an error boundary's restart in a shipped app: the pages
        // pushed may be what failed.
        if !engine.reload_tracking() {
            return None;
        }
        let st = state_of(engine, obj)?;
        let entries = st.entries.borrow();
        let kept = st.kept.get().min(entries.len());
        if kept == entries.len() && kept == st.first.get() {
            return None;
        }
        let pages = entries[kept..]
            .iter()
            .map(|en| Entry::new(en.source.clone(), en.props.clone()))
            .collect();
        Some(CarriedPages { kept, pages })
    }

    fn migrate(
        engine: &Engine,
        obj: ObjectId,
        _from: u32,
        state: CarriedPages,
    ) -> Result<(), String> {
        let st = state_of(engine, obj).ok_or("not a StackView")?;
        // Translated now: the old tree's objects have counterparts only
        // until the reload is finished.
        let object = |o: ObjectId| match engine.reload_translate(Value::Object(o)) {
            Some(Value::Object(o)) => Some(o),
            _ => None,
        };
        let pages = state
            .pages
            .into_iter()
            .filter_map(|en| {
                let source = match en.source {
                    Source::Given(o) => Source::Given(object(o)?),
                    Source::Component(o) => Source::Component(object(o)?),
                    url @ Source::Url(_) => url,
                };
                let props = en
                    .props
                    .into_iter()
                    .filter_map(|(name, v)| Some((name, engine.reload_translate(v)?)))
                    .collect();
                Some(Entry::new(source, props))
            })
            .collect();
        let carried = CarriedPages {
            kept: state.kept,
            pages,
        };
        if !st.done.get() {
            *st.restore.borrow_mut() = Some(carried);
            return Ok(());
        }
        // Made and completed already (an error boundary's restart).
        let CarriedPages { kept, pages } = carried;
        let immediate = Some(op::IMMEDIATE);
        let r = pop_to(engine, obj, kept.max(1), immediate).and_then(|_| {
            if kept == 0 {
                clear(engine, obj)?;
            }
            push(
                engine,
                obj,
                Request {
                    pages,
                    operation: immediate,
                },
            )
            .map(|_| ())
        });
        r.map_err(|err| err.to_string())
    }
}

/// Puts the carried pages over the ones `initialItem` made, before the top
/// one is loaded.
fn restored(st: &StackState) {
    let Some(CarriedPages { kept, pages }) = st.restore.borrow_mut().take() else {
        return;
    };
    let mut entries = st.entries.borrow_mut();
    let kept = kept.min(entries.len());
    entries.truncate(kept);
    st.kept.set(kept);
    entries.extend(pages);
}

fn number(v: &Value) -> Option<i32> {
    is_number(v).then(|| v.to_number() as i32)
}

/// Makes the page at `index` if it was not made, and gives it its place.
fn load(e: &Engine, me: ObjectId, index: usize) -> Result<Option<ObjectId>, EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(None);
    };
    let (source, props) = {
        let entries = st.entries.borrow();
        let Some(entry) = entries.get(index) else {
            return Ok(None);
        };
        if let Some(item) = entry.item.filter(|i| e.is_alive(*i)) {
            return Ok(Some(item));
        }
        (entry.source.clone(), entry.props.clone())
    };
    let (item, owned, home) = match source {
        Source::Given(item) => {
            if !e.is_alive(item) {
                return Ok(None);
            }
            let home = e.untrack(|| e.visual_parent(item));
            for (name, v) in props {
                check(e, "StackView", e.write_by_name(item, &name, v));
            }
            (item, false, home)
        }
        Source::Component(c) => (e.component_create(c, Some(me), props)?, true, None),
        Source::Url(url) => (from_url(e, me, &url, props)?, true, None),
    };
    if !is_item(e, item) {
        if owned {
            e.destroy(item);
        }
        return Ok(None);
    }
    set_parent_item(e, item, Some(me))?;
    let width = hold(e, item, Item::WIDTH.index)?;
    let height = hold(e, item, Item::HEIGHT.index)?;
    {
        let mut entries = st.entries.borrow_mut();
        if let Some(entry) = entries.get_mut(index) {
            entry.item = Some(item);
            entry.owned = owned;
            entry.home = home;
            entry.width = width;
            entry.height = height;
        }
    }
    let att = e.attached_object(item, &attacher())?;
    set_computed(e, att, StackViewAttached::VIEW.index, Value::Object(me))?;
    set_computed(
        e,
        att,
        StackViewAttached::INDEX.index,
        Value::Int(index as i32),
    )?;
    // Shown before anything follows it: until the view makes it current,
    // the page would read hidden, and every item in it would be covered
    // and uncovered again on its way in.
    show(e, item, true);
    // In the view's scope, not the page's: the page's own effects wait
    // while it is frozen, and this one is what thaws it.
    let follow = e.effect_in(me, move |e| {
        let shown = e.read(att, StackViewAttached::VISIBLE.index).to_boolean();
        e.untrack(|| follow_shown(e, me, item, shown));
    })?;
    if let Some(entry) = st.entries.borrow_mut().get_mut(index) {
        entry.follow = Some(follow);
    }
    let w = e.untrack(|| num(e, me, Control::AVAILABLE_WIDTH.index));
    let h = e.untrack(|| num(e, me, Control::AVAILABLE_HEIGHT.index));
    fit(e, me, w, h)?;
    Ok(Some(item))
}

/// A page made from a file is made in the view's context, so it sees the
/// names around the view, as in Qt.
fn from_url(
    e: &Engine,
    me: ObjectId,
    url: &str,
    props: Vec<(String, Value)>,
) -> Result<ObjectId, EngineError> {
    let Some(ctx) = e.context_of(me) else {
        return e.create_from_url(url, Some(me), props);
    };
    let c = e.create_component(url, Some(me))?;
    let made = e
        .set_component_context(c, ctx)
        .and_then(|()| e.component_create(c, Some(me), props));
    e.destroy(c);
    made
}

/// A page that was given no size is as large as the view.
fn hold(e: &Engine, item: ObjectId, prop: PropIndex) -> Result<Option<OverrideId>, EngineError> {
    let horizontal = prop == Item::WIDTH.index;
    if !e.untrack(|| follows_implicit_size(e, item, horizontal)) {
        return Ok(None);
    }
    let now = e.untrack(|| e.read(item, prop));
    e.push_override(item, prop, OverrideContent::Value(now))
        .map(Some)
}

fn fit(e: &Engine, me: ObjectId, w: f64, h: f64) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    let sized: Vec<(ObjectId, Option<OverrideId>, Option<OverrideId>)> = st
        .entries
        .borrow()
        .iter()
        .filter_map(|en| Some((en.item?, en.width, en.height)))
        .collect();
    for (item, lw, lh) in sized {
        if !e.is_alive(item) {
            continue;
        }
        for (layer, prop, v) in [(lw, Item::WIDTH.index, w), (lh, Item::HEIGHT.index, h)] {
            if let Some(layer) = layer
                && e.untrack(|| e.read(item, prop)).to_number() != v
            {
                e.write_internal(item, prop, Value::Real(v), WriteKind::Override(layer))?;
            }
        }
    }
    Ok(())
}

/// Shows or hides a page, and freezes it while hidden (ARCHITECTURE.md
/// §8.3): it keeps its objects and values, but its effects, timers and
/// animations wait until it is shown again. A page that asks to be seen
/// under the one on top is not frozen.
///
/// Freezing a page freezes what it owns, and a page can own another page
/// of the stack: one declared inside it and pushed over it. A page stays
/// awake while it owns one that is shown.
fn follow_shown(e: &Engine, me: ObjectId, item: ObjectId, shown: bool) {
    if !e.is_alive(item) {
        return;
    }
    let pages: Vec<ObjectId> = state_of(e, me).map_or_else(Vec::new, |st| {
        st.entries
            .borrow()
            .iter()
            .filter_map(|en| en.item)
            .filter(|i| e.is_alive(*i))
            .collect()
    });
    let holders: Vec<ObjectId> = pages
        .iter()
        .copied()
        .filter(|p| *p != item && owns(e, *p, item))
        .collect();
    if shown {
        check(e, "StackView", e.set_frozen(item, false));
        for holder in &holders {
            check(e, "StackView", e.set_frozen(*holder, false));
        }
    }
    let r = set_computed(e, item, Item::VISIBLE.index, Value::Bool(shown));
    check(e, "StackView.visible", r);
    if !shown {
        // Takes hold once the page has seen itself hidden.
        for page in std::iter::once(item).chain(holders) {
            let awake = is_shown(e, page)
                || pages
                    .iter()
                    .any(|p| *p != page && is_shown(e, *p) && owns(e, page, *p));
            if !awake {
                check(e, "StackView", e.set_frozen(page, true));
            }
        }
    }
}

/// Whether `holder` owns `obj`, directly or through what it owns.
fn owns(e: &Engine, holder: ObjectId, obj: ObjectId) -> bool {
    let mut at = e.owner(obj);
    while let Some(o) = at {
        if o == holder {
            return true;
        }
        at = e.owner(o);
    }
    false
}

/// Whether the view shows `page` now.
fn is_shown(e: &Engine, page: ObjectId) -> bool {
    e.existing_attached_object(page, &attacher())
        .is_some_and(|att| e.read(att, StackViewAttached::VISIBLE.index).to_boolean())
}

fn show(e: &Engine, item: ObjectId, on: bool) {
    if let Some(att) = e.existing_attached_object(item, &attacher()) {
        check(
            e,
            "StackView.visible",
            crate::popup::put(e, att, StackViewAttached::VISIBLE.index, Value::Bool(on)),
        );
    }
}

/// Sets the page's status and says so with `signal`.
fn tell(e: &Engine, item: ObjectId, status: i32, signal: &str) {
    if !e.is_alive(item) {
        return;
    }
    let Some(att) = e.existing_attached_object(item, &attacher()) else {
        return;
    };
    check(
        e,
        "StackView.status",
        set_computed(e, att, StackViewAttached::STATUS.index, Value::Int(status)),
    );
    check(e, signal, e.emit_by_name(att, signal, &[]));
}

/// `depth`, `empty`, `currentItem` and each page's index follow the stack.
fn update(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    let (depth, top, made) = {
        let entries = st.entries.borrow();
        let made: Vec<(usize, ObjectId)> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, en)| Some((i, en.item?)))
            .collect();
        (entries.len(), entries.last().and_then(|en| en.item), made)
    };
    for (i, item) in made {
        if let Some(att) = e.existing_attached_object(item, &attacher()) {
            set_computed(e, att, StackViewAttached::INDEX.index, Value::Int(i as i32))?;
        }
    }
    e.batch(|| {
        set_computed(e, me, StackView::DEPTH.index, Value::Int(depth as i32))?;
        set_computed(e, me, StackView::EMPTY.index, Value::Bool(depth == 0))?;
        set_computed(e, me, StackView::CURRENT_ITEM.index, reply(top))
    })
}

/// The page has left the stack: it is told, hidden, and destroyed if the
/// view made it, or given back to its parent.
fn release(e: &Engine, entry: Entry) {
    if let Some(follow) = entry.follow {
        let _ = e.graph().dispose(follow);
    }
    let Some(item) = entry.item.filter(|i| e.is_alive(*i)) else {
        return;
    };
    show(e, item, false);
    // Nothing follows the page any more: it leaves hidden, and awake.
    let r = set_computed(e, item, Item::VISIBLE.index, Value::Bool(false));
    check(e, "StackView.visible", r);
    check(e, "StackView", e.set_frozen(item, false));
    for (prop, layer) in [
        (Item::WIDTH.index, entry.width),
        (Item::HEIGHT.index, entry.height),
    ] {
        if let Some(layer) = layer {
            let _ = e.remove_override(item, prop, layer, RestoreMode::BindingOrValue);
        }
    }
    if let Some(att) = e.existing_attached_object(item, &attacher()) {
        let r = set_computed(e, att, StackViewAttached::INDEX.index, Value::Int(-1))
            .and_then(|()| set_computed(e, att, StackViewAttached::VIEW.index, Value::Null));
        check(e, "StackView", r);
    }
    if entry.owned {
        e.destroy(item);
    } else {
        check(e, "StackView", set_parent_item(e, item, entry.home));
    }
}

fn removed(e: &Engine, entries: Vec<Entry>) {
    for entry in &entries {
        if let Some(item) = entry.item {
            tell(e, item, status::INACTIVE, "removed");
        }
    }
    for entry in entries {
        release(e, entry);
    }
}

/// Which pair of transitions an operation runs.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Push,
    Pop,
    Replace,
}

fn transitions(
    e: &Engine,
    me: ObjectId,
    kind: Kind,
    operation: Option<i32>,
) -> Option<(Option<ObjectId>, Option<ObjectId>)> {
    let kind = match operation {
        Some(op::IMMEDIATE) => return None,
        Some(op::PUSH) => Kind::Push,
        Some(op::POP) => Kind::Pop,
        Some(op::REPLACE) => Kind::Replace,
        _ => kind,
    };
    let (enter, exit) = match kind {
        Kind::Push => (StackView::PUSH_ENTER, StackView::PUSH_EXIT),
        Kind::Pop => (StackView::POP_ENTER, StackView::POP_EXIT),
        Kind::Replace => (StackView::REPLACE_ENTER, StackView::REPLACE_EXIT),
    };
    let get = |p: PropIndex| {
        e.untrack(|| e.read(me, p))
            .as_object()
            .filter(|t| e.is_alive(*t) && is_a::<Transition>(e, *t))
    };
    let pair = (get(enter.index), get(exit.index));
    (pair.0.is_some() || pair.1.is_some()).then_some(pair)
}

/// An operation comes while pages of an earlier one move (qml6, with
/// transitions of `NumberAnimation`s). The pages it moves again (`taken`)
/// are not told how the earlier one ended, so a page pushed and at once
/// covered is never activated; they go from where they are. The other
/// pages run on to their own end. Only where a transition the new
/// operation runs (`needed`) is still moving another page does that page
/// end at once, hidden if it went under: a `Transition` here moves one
/// page at a time, where Qt's runs on both.
fn interrupt(e: &Engine, me: ObjectId, taken: &[Option<ObjectId>], needed: &[ObjectId]) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    let out: Vec<PageRun> = {
        let mut runs = st.runs.borrow_mut();
        let (out, keep) = std::mem::take(&mut *runs).into_iter().partition(|r| {
            taken.contains(&Some(r.page)) || r.transition.is_some_and(|t| needed.contains(&t))
        });
        *runs = keep;
        out
    };
    if out.is_empty() {
        return;
    }
    let top = st.entries.borrow().last().and_then(|en| en.item);
    for r in out {
        if let Some(t) = r.transition.filter(|t| e.is_alive(*t)) {
            ljos_quick::stop_transition(e, t);
        }
        if taken.contains(&Some(r.page)) {
            continue;
        }
        (r.end)(e);
        if Some(r.page) != top && e.is_alive(r.page) {
            show(e, r.page, false);
        }
    }
    end_still(e, &st);
}

/// The pages of an operation that no transition moves are told their end
/// once its last run has ended.
fn end_still(e: &Engine, st: &StackState) {
    loop {
        let done: Vec<PageRun> = {
            let mut runs = st.runs.borrow_mut();
            let moving: Vec<u64> = runs
                .iter()
                .filter(|r| r.transition.is_some())
                .map(|r| r.op)
                .collect();
            let (done, keep) = std::mem::take(&mut *runs)
                .into_iter()
                .partition(|r| r.transition.is_none() && !moving.contains(&r.op));
            *runs = keep;
            done
        };
        if done.is_empty() {
            return;
        }
        for r in done {
            (r.end)(e);
        }
    }
}

/// Nothing moves any more: `busy` goes false, then what waited for it.
fn idle(e: &Engine, me: ObjectId) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    if !st.runs.borrow().is_empty() {
        return;
    }
    check(
        e,
        "StackView",
        set_computed(e, me, StackView::BUSY.index, Value::Bool(false)),
    );
    loop {
        let after = std::mem::take(&mut *st.after.borrow_mut());
        if after.is_empty() {
            break;
        }
        for f in after {
            f(e);
        }
    }
}

/// One page of an operation: the transition it runs, if any, and what it
/// is told at the end.
struct Step {
    page: ObjectId,
    transition: Option<ObjectId>,
    /// Whether the page comes or goes, which reduced motion fades it by.
    passage: Passage,
    end: Finish,
}

fn step(
    page: ObjectId,
    transition: Option<ObjectId>,
    passage: Passage,
    end: impl FnOnce(&Engine) + 'static,
) -> Step {
    Step {
        page,
        transition,
        passage,
        end: Box::new(end),
    }
}

/// Runs the transition of each page. A page is told its end when its own
/// run ends, as in Qt (a quick exit is over before a slow enter), and a
/// page without one when the last run of the operation ends. At once when
/// no page has a transition.
fn animate(e: &Engine, me: ObjectId, steps: Vec<Step>) {
    let Some(st) = state_of(e, me) else {
        for s in steps {
            (s.end)(e);
        }
        return;
    };
    let op = st.ops.get() + 1;
    st.ops.set(op);
    // A transition moves one page at a time.
    let mut used: Vec<ObjectId> = Vec::new();
    let mut runs: Vec<PageRun> = Vec::new();
    let mut moving: Vec<(ObjectId, ObjectId, Passage)> = Vec::new();
    for s in steps {
        let transition = s.transition.filter(|t| !used.contains(t));
        used.extend(transition);
        moving.extend(transition.map(|t| (t, s.page, s.passage)));
        runs.push(PageRun {
            op,
            page: s.page,
            transition,
            end: s.end,
        });
    }
    if used.is_empty() {
        for r in runs {
            (r.end)(e);
        }
        idle(e, me);
        return;
    }
    st.runs.borrow_mut().extend(runs);
    check(
        e,
        "StackView",
        set_computed(e, me, StackView::BUSY.index, Value::Bool(true)),
    );
    for (transition, page, passage) in moving {
        let st = st.clone();
        ljos_quick::run_passage(e, transition, page, passage, move |e| {
            let run = {
                let mut runs = st.runs.borrow_mut();
                let at = runs
                    .iter()
                    .position(|r| r.op == op && r.page == page && r.transition == Some(transition));
                at.map(|i| runs.remove(i))
            };
            let Some(run) = run else {
                return;
            };
            (run.end)(e);
            end_still(e, &st);
            if e.is_alive(me) {
                idle(e, me);
            }
        });
    }
}

fn both(pair: Option<(Option<ObjectId>, Option<ObjectId>)>) -> Vec<ObjectId> {
    pair.map(|(a, b)| a.into_iter().chain(b).collect())
        .unwrap_or_default()
}

/// A page that comes without a transition is where the view shows it.
fn place_still(e: &Engine, item: ObjectId) {
    for p in [Item::X.index, Item::Y.index] {
        if e.untrack(|| e.read(item, p)).to_number() != 0.0 {
            check(
                e,
                "StackView",
                e.write_internal(item, p, Value::Real(0.0), WriteKind::Host),
            );
        }
    }
}

fn push(e: &Engine, me: ObjectId, r: Request) -> Result<Value, EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(Value::Null);
    };
    if r.pages.is_empty() {
        return Ok(Value::Null);
    }
    let old = st.entries.borrow().last().and_then(|en| en.item);
    let pair = old.and_then(|_| transitions(e, me, Kind::Push, r.operation));
    interrupt(e, me, &[old], &both(pair));
    st.entries.borrow_mut().extend(r.pages);
    let top = depth_of(e, me) - 1;
    let made = load(e, me, top);
    let Ok(Some(new)) = made else {
        // Nothing could be made of it: the stack is as it was.
        st.entries.borrow_mut().truncate(top);
        update(e, me)?;
        idle(e, me);
        cannot_make(e, me, "push", made);
        return Ok(Value::Null);
    };
    update(e, me)?;
    match pair {
        None => {
            place_still(e, new);
            tell(e, new, status::ACTIVATING, "activating");
            tell(e, new, status::ACTIVE, "activated");
            if let Some(old) = old {
                tell(e, old, status::DEACTIVATING, "deactivating");
                tell(e, old, status::INACTIVE, "deactivated");
                show(e, old, false);
            }
            idle(e, me);
        }
        Some((enter, exit)) => {
            tell(e, new, status::ACTIVATING, "activating");
            if let Some(old) = old {
                tell(e, old, status::DEACTIVATING, "deactivating");
            }
            let mut steps = vec![step(new, enter, Passage::Enter, move |e| {
                tell(e, new, status::ACTIVE, "activated");
            })];
            if let Some(old) = old {
                steps.push(step(old, exit, Passage::Exit, move |e| {
                    tell(e, old, status::INACTIVE, "deactivated");
                    show(e, old, false);
                }));
            }
            animate(e, me, steps);
        }
    }
    Ok(Value::Object(new))
}

/// Says, as qml6 does, why `op` made no page: a file that cannot be
/// loaded warns, and the code that asked goes on.
fn cannot_make(e: &Engine, me: ObjectId, op: &str, made: Result<Option<ObjectId>, EngineError>) {
    if let Err(err) = made {
        let at = e.declared_at(me).unwrap_or_default();
        let ty = e.warning_type_name(me, "StackView");
        warn(e, format!("{at}QML {ty}: {op}: {err}"));
    }
}

/// Pops every page above the first `keep`, and returns the one that was
/// on top.
fn pop_to(
    e: &Engine,
    me: ObjectId,
    keep: usize,
    operation: Option<i32>,
) -> Result<Option<ObjectId>, EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(None);
    };
    if keep >= depth_of(e, me) {
        return Ok(None);
    }
    let taken = {
        let entries = st.entries.borrow();
        let below = keep.checked_sub(1).and_then(|k| entries.get(k));
        [
            entries.last().and_then(|en| en.item),
            below.and_then(|en| en.item),
        ]
    };
    let pair = (keep >= 1)
        .then(|| transitions(e, me, Kind::Pop, operation))
        .flatten();
    interrupt(e, me, &taken, &both(pair));
    let mut gone: Vec<Entry> = st.entries.borrow_mut().drain(keep..).collect();
    st.kept.set(st.kept.get().min(keep));
    // The top page first: the others were covered and are only removed.
    let top = gone.pop();
    let old = top.as_ref().and_then(|en| en.item);
    let new = match keep {
        0 => None,
        k => load(e, me, k - 1)?,
    };
    update(e, me)?;
    let pair = new.and(old).and(pair);
    match pair {
        None => {
            if let Some(old) = old {
                tell(e, old, status::DEACTIVATING, "deactivating");
                tell(e, old, status::INACTIVE, "deactivated");
            }
            let mut all: Vec<Entry> = top.into_iter().collect();
            all.extend(gone);
            removed(e, all);
            if let Some(new) = new {
                show(e, new, true);
                place_still(e, new);
                tell(e, new, status::ACTIVATING, "activating");
                tell(e, new, status::ACTIVE, "activated");
            }
            idle(e, me);
        }
        Some((enter, exit)) => {
            if let Some(old) = old {
                tell(e, old, status::DEACTIVATING, "deactivating");
            }
            if let Some(new) = new {
                show(e, new, true);
                tell(e, new, status::ACTIVATING, "activating");
            }
            st.after.borrow_mut().push(Box::new(move |e| {
                let mut all: Vec<Entry> = top.into_iter().collect();
                all.extend(gone);
                removed(e, all);
            }));
            let mut steps = Vec::new();
            if let Some(old) = old {
                steps.push(step(old, exit, Passage::Exit, move |e| {
                    tell(e, old, status::INACTIVE, "deactivated");
                }));
            }
            if let Some(new) = new {
                steps.push(step(new, enter, Passage::Enter, move |e| {
                    tell(e, new, status::ACTIVE, "activated");
                }));
            }
            animate(e, me, steps);
        }
    }
    Ok(old)
}

/// Replaces the pages from the `keep`-th up with the requested ones, and
/// returns the new top page.
fn replace(e: &Engine, me: ObjectId, keep: usize, r: Request) -> Result<Value, EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(Value::Null);
    };
    if r.pages.is_empty() {
        return Ok(Value::Null);
    }
    let old = st.entries.borrow().last().and_then(|en| en.item);
    let pair = transitions(e, me, Kind::Replace, r.operation);
    interrupt(e, me, &[old], &both(pair));
    let keep = keep.min(depth_of(e, me));
    let mut gone: Vec<Entry> = st.entries.borrow_mut().drain(keep..).collect();
    let kept = st.kept.replace(st.kept.get().min(keep));
    st.entries.borrow_mut().extend(r.pages);
    let at = depth_of(e, me) - 1;
    let made = load(e, me, at);
    let Ok(Some(new)) = made else {
        // The pages it would replace stay: out of the stack they would
        // be held, never removed, for as long as the application runs.
        let mut entries = st.entries.borrow_mut();
        entries.truncate(keep);
        entries.extend(gone);
        drop(entries);
        st.kept.set(kept);
        update(e, me)?;
        idle(e, me);
        cannot_make(e, me, "replace", made);
        return Ok(reply(old));
    };
    let top = gone.pop();
    update(e, me)?;
    let pair = old.and(pair);
    let mut all: Vec<Entry> = top.into_iter().collect();
    all.extend(gone);
    match pair {
        None => {
            if let Some(old) = old {
                tell(e, old, status::DEACTIVATING, "deactivating");
                tell(e, old, status::INACTIVE, "deactivated");
            }
            removed(e, all);
            place_still(e, new);
            tell(e, new, status::ACTIVATING, "activating");
            tell(e, new, status::ACTIVE, "activated");
            idle(e, me);
        }
        Some((enter, exit)) => {
            tell(e, new, status::ACTIVATING, "activating");
            if let Some(old) = old {
                tell(e, old, status::DEACTIVATING, "deactivating");
            }
            st.after
                .borrow_mut()
                .push(Box::new(move |e| removed(e, all)));
            let mut steps = vec![step(new, enter, Passage::Enter, move |e| {
                tell(e, new, status::ACTIVE, "activated");
            })];
            if let Some(old) = old {
                steps.push(step(old, exit, Passage::Exit, move |e| {
                    tell(e, old, status::INACTIVE, "deactivated");
                }));
            }
            animate(e, me, steps);
        }
    }
    Ok(Value::Object(new))
}

/// Every page leaves at once, transitions under way or not.
fn clear(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    let all: Vec<ObjectId> = st
        .runs
        .borrow()
        .iter()
        .filter_map(|r| r.transition)
        .collect();
    interrupt(e, me, &[], &all);
    let all: Vec<Entry> = st.entries.borrow_mut().drain(..).collect();
    st.kept.set(0);
    update(e, me)?;
    removed(e, all);
    idle(e, me);
    Ok(())
}
