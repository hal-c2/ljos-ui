//! The controls that hold other items: `Pane`, `Frame`, `GroupBox`,
//! `ToolBar` and `Page` (QtQuick.Templates).
//!
//! What is declared inside one goes into its `contentItem`, not into the
//! control itself, and the control is as big as that content wants.
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-pane.html and the
//! pages of the other four; pinned by `features/controls/pane.feature`
//! and its neighbours.

use ljos_engine::{
    Atom, Cx, Engine, EngineError, ObjectFlags, ObjectId, PropIndex, QmlList, QmlObject, Value,
    WriteKind, qml_impl,
};
use ljos_quick::{Item, is_item, set_parent_item};

use crate::control::Control;
use crate::parts::{PartKind, manage_part};
use crate::util::{bind_default, check, flag, num, object};

/// Where a type that holds content keeps it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Host {
    pub content_item: PropIndex,
    pub content_data: PropIndex,
    pub content_children: PropIndex,
}

/// The append hook of a host's `contentData`, its default property: an
/// item goes into the content item, anything else is only kept.
pub(crate) fn host_append(cx: &Cx, child: ObjectId, host: Host) -> Result<(), EngineError> {
    let e = cx.engine;
    e.list_push(cx.this, host.content_data, child)?;
    if !is_item(e, child) {
        return Ok(());
    }
    e.list_push(cx.this, host.content_children, child)?;
    // Until the host is complete its content item may still be replaced
    // (the style's, then the application's); `host_complete` moves what
    // was declared in.
    let complete = e
        .flags(cx.this)
        .is_some_and(|f| f.contains(ObjectFlags::COMPLETED));
    if complete && let Some(ci) = e.untrack(|| object(e, cx.this, host.content_item)) {
        set_parent_item(e, child, Some(holder(e, cx.this, ci)))?;
    }
    Ok(())
}

/// The item that what is declared in a host becomes a child of: its
/// content item, or in a `ScrollView` what that content item scrolls.
fn holder(engine: &Engine, host: ObjectId, content_item: ObjectId) -> ObjectId {
    if !crate::scroll_view::is_scroll_view(engine, host) {
        return content_item;
    }
    ljos_quick::flickable::content_item(engine, content_item).unwrap_or(content_item)
}

fn content_children(engine: &Engine, obj: ObjectId, host: Host) -> Vec<ObjectId> {
    match engine.read(obj, host.content_children) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}

/// Gives the host a content item if it has none, moves the declared
/// children into it, and moves them again when the content item is
/// replaced.
pub(crate) fn host_complete(engine: &Engine, obj: ObjectId, host: Host) -> Result<(), EngineError> {
    if engine
        .untrack(|| object(engine, obj, host.content_item))
        .is_none()
    {
        let ci = engine.create_native(Item::default(), Some(obj))?;
        engine.complete(ci)?;
        engine.write_internal(
            obj,
            host.content_item,
            Value::Object(ci),
            WriteKind::Initial,
        )?;
    }
    let adopt = move |e: &Engine, ci: Option<ObjectId>| -> Result<(), EngineError> {
        let Some(ci) = ci.filter(|c| is_item(e, *c)) else {
            return Ok(());
        };
        let into = holder(e, obj, ci);
        for child in e.untrack(|| content_children(e, obj, host)) {
            if child != ci {
                set_parent_item(e, child, Some(into))?;
            }
        }
        Ok(())
    };
    adopt(
        engine,
        engine.untrack(|| object(engine, obj, host.content_item)),
    )?;
    engine.watch(obj, host.content_item, move |e, v| {
        let ci = v.as_object().filter(|c| e.is_alive(*c));
        check(e, "contentItem", adopt(e, ci));
    })?;
    Ok(())
}

/// How big the content of a host wants to be along one axis: what its
/// content item asks for, or else what the one item inside it asks for.
/// Several items say nothing; the author positions them and gives the
/// size (checked on qml6).
pub(crate) fn content_size(engine: &Engine, obj: ObjectId, host: Host, size: PropIndex) -> f64 {
    let Some(ci) = object(engine, obj, host.content_item).filter(|c| is_item(engine, *c)) else {
        return 0.0;
    };
    let own = num(engine, ci, size);
    if own > 0.0 {
        return own;
    }
    match content_children(engine, obj, host).as_slice() {
        [one] if is_item(engine, *one) => num(engine, *one, size),
        _ => 0.0,
    }
}

// ----------------------------------------------------------------- Pane

/// `Pane`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "Pane",
    extends = Control,
    default = "contentData"
)]
pub struct Pane {
    #[qml(base)]
    pub control: Control,
    #[qml(notify, list, append = pane_append)]
    pub content_data: QmlList,
    #[qml(notify, list)]
    pub content_children: QmlList,
    #[qml(notify)]
    pub content_width: f64,
    #[qml(notify)]
    pub content_height: f64,
}

const PANE: Host = Host {
    content_item: Control::CONTENT_ITEM.index,
    content_data: Pane::CONTENT_DATA.index,
    content_children: Pane::CONTENT_CHILDREN.index,
};

fn pane_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    host_append(cx, child, PANE)
}

#[qml_impl]
impl Pane {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        crate::scroll_view::prepare(e, obj)?;
        host_complete(e, obj, PANE)?;
        bind_default(e, obj, Pane::CONTENT_WIDTH.index, move |e| {
            Value::Real(content_size(e, obj, PANE, Item::IMPLICIT_WIDTH.index))
        })?;
        bind_default(e, obj, Pane::CONTENT_HEIGHT.index, move |e| {
            Value::Real(content_size(e, obj, PANE, Item::IMPLICIT_HEIGHT.index))
        })?;
        // A pane's content is what is inside its content item, which
        // itself asks for nothing.
        e.bind_initial(obj, Control::IMPLICIT_CONTENT_WIDTH.index, move |e| {
            Value::Real(num(e, obj, Pane::CONTENT_WIDTH.index))
        })?;
        e.bind_initial(obj, Control::IMPLICIT_CONTENT_HEIGHT.index, move |e| {
            Value::Real(num(e, obj, Pane::CONTENT_HEIGHT.index))
        })?;
        crate::a11y::pane_defaults(e, obj)
    }
}

/// `Frame`: a pane with a border.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Frame", extends = Pane)]
pub struct Frame {
    #[qml(base)]
    pub pane: Pane,
}

// ------------------------------------------------------------- GroupBox

/// `GroupBox`: a frame with a title.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "GroupBox", extends = Frame)]
pub struct GroupBox {
    #[qml(base)]
    pub frame: Frame,
    #[qml(notify)]
    pub title: Atom,
    #[qml(notify)]
    pub label: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub implicit_label_width: f64,
    #[qml(notify, readonly)]
    pub implicit_label_height: f64,
}

/// Binds a read-only size of `obj` to the implicit size of the item in
/// `part`, 0 without one or while it is hidden.
fn follow_part_size(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    part: PropIndex,
    size: PropIndex,
) -> Result<(), EngineError> {
    engine.bind_initial(obj, prop, move |e| match object(e, obj, part) {
        Some(p) if is_item(e, p) && part_shows(e, p) => Value::Real(num(e, p, size)),
        _ => Value::Real(0.0),
    })
}

/// Whether the part was left visible by its author. In a scene `visible`
/// also reads false while the control itself is hidden, and a hidden
/// page still has its header.
fn part_shows(engine: &Engine, part: ObjectId) -> bool {
    flag(engine, part, Item::VISIBLE.index)
}

#[qml_impl]
impl GroupBox {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        let label = GroupBox::LABEL.index;
        follow_part_size(
            e,
            obj,
            GroupBox::IMPLICIT_LABEL_WIDTH.index,
            label,
            Item::IMPLICIT_WIDTH.index,
        )?;
        follow_part_size(
            e,
            obj,
            GroupBox::IMPLICIT_LABEL_HEIGHT.index,
            label,
            Item::IMPLICIT_HEIGHT.index,
        )?;
        manage_part(e, obj, label, PartKind::Free)
    }
}

// -------------------------------------------------------------- ToolBar

fn tool_bar_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![("Position", vec![("Header", 0), ("Footer", 1)])]
}

/// `ToolBar`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ToolBar",
    extends = Pane,
    enums = tool_bar_enums
)]
pub struct ToolBar {
    #[qml(base)]
    pub pane: Pane,
    /// `ToolBar.Header` or `ToolBar.Footer`: where the bar is, for a
    /// style that draws the two differently.
    #[qml(notify)]
    pub position: i32,
}

// ----------------------------------------------------------------- Page

/// `Page`: a pane with a header above its content and a footer below.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Page", extends = Pane)]
pub struct Page {
    #[qml(base)]
    pub pane: Pane,
    #[qml(notify)]
    pub title: Atom,
    #[qml(notify)]
    pub header: Option<ObjectId>,
    #[qml(notify)]
    pub footer: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub implicit_header_width: f64,
    #[qml(notify, readonly)]
    pub implicit_header_height: f64,
    #[qml(notify, readonly)]
    pub implicit_footer_width: f64,
    #[qml(notify, readonly)]
    pub implicit_footer_height: f64,
}

#[qml_impl]
impl Page {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        for (prop, part, size) in [
            (
                Page::IMPLICIT_HEADER_WIDTH,
                Page::HEADER,
                Item::IMPLICIT_WIDTH,
            ),
            (
                Page::IMPLICIT_HEADER_HEIGHT,
                Page::HEADER,
                Item::IMPLICIT_HEIGHT,
            ),
            (
                Page::IMPLICIT_FOOTER_WIDTH,
                Page::FOOTER,
                Item::IMPLICIT_WIDTH,
            ),
            (
                Page::IMPLICIT_FOOTER_HEIGHT,
                Page::FOOTER,
                Item::IMPLICIT_HEIGHT,
            ),
        ] {
            follow_part_size(e, obj, prop.index, part.index, size.index)?;
        }
        manage_part(e, obj, Page::HEADER.index, PartKind::Header)?;
        manage_part(e, obj, Page::FOOTER.index, PartKind::Footer)
    }
}

/// How much of `obj`'s height the bar in `part` takes: its height, 0
/// without one or while it is hidden (tracked).
pub(crate) fn bar_height(engine: &Engine, obj: ObjectId, part: PropIndex) -> f64 {
    match object(engine, obj, part) {
        Some(p) if is_item(engine, p) && part_shows(engine, p) => {
            num(engine, p, Item::HEIGHT.index)
        }
        _ => 0.0,
    }
}

/// What a page's header and footer take from its content, with the
/// spacing that separates each from it (tracked).
pub(crate) fn page_bars(engine: &Engine, page: ObjectId) -> (f64, f64) {
    let spacing = num(engine, page, Control::SPACING.index);
    let with_gap = |h: f64| if h > 0.0 { h + spacing } else { 0.0 };
    (
        with_gap(bar_height(engine, page, Page::HEADER.index)),
        with_gap(bar_height(engine, page, Page::FOOTER.index)),
    )
}
