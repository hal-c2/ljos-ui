//! `SelectionRectangle` (QtQuick.Templates): selects the cells of its
//! `target`, a `TableView` or a `TreeView`, with the pointer.
//!
//! The view does the selecting (`ljos_quick::CellSelector`): by drag, or
//! by press and hold, as `selectionMode` says. Every block it selects for
//! the rectangle makes it `active`, with a handle made from
//! `topLeftHandle` and one from `bottomRightHandle` centred on the
//! block's corners, in the view's content above the cells; a view whose
//! selection model has nothing selected makes it inactive, and the
//! handles are hidden. Each handle's `SelectionRectangle.control` is the
//! rectangle, and its `SelectionRectangle.dragging` says the pointer
//! drags it. Learned by running `qml6` (Qt 6.11.2), pinned by
//! `features/controls/selection-rectangle.feature`; documentation:
//! doc.qt.io/qt-6/qml-qtquick-controls-selectionrectangle.html.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, ItemSelectionModel, MetaRef, NodeId, ObjectId, QmlObject, QmlType,
    Value, Version, qml_impl,
};
use ljos_quick::{
    CellBlock, CellSelector, Item, TableView, cell_block_rect, flickable, select_by,
    set_cell_selector, set_parent_item,
};

use crate::control::Control;
use crate::util::{check, is_a, object, set_computed};

fn selection_rectangle_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "SelectionMode",
        vec![
            ("Drag", i64::from(select_by::DRAG)),
            ("PressAndHold", i64::from(select_by::PRESS_AND_HOLD)),
            ("Auto", i64::from(select_by::AUTO)),
        ],
    )]
}

/// What a rectangle keeps that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct RectState {
    /// The view it selects in now.
    target: Cell<Option<ObjectId>>,
    /// The top-left handle and the bottom-right one, once made.
    handles: [Cell<Option<ObjectId>>; 2],
    /// The block last selected, as `[top, left, bottom, right]`, for the
    /// handles to follow.
    block: Cell<Option<NodeId>>,
}

/// `SelectionRectangle`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "SelectionRectangle",
    extends = Control,
    enums = selection_rectangle_enums,
    attached = SelectionRectangleAttached
)]
pub struct SelectionRectangle {
    #[qml(base)]
    pub control: Control,
    #[qml(notify, enumeration = "SelectionMode")]
    pub selection_mode: i32,
    #[qml(notify, object = "Item")]
    pub target: Option<ObjectId>,
    #[qml(notify, object = "Component")]
    pub top_left_handle: Option<ObjectId>,
    #[qml(notify, object = "Component")]
    pub bottom_right_handle: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub active: bool,
    #[qml(notify, readonly)]
    pub dragging: bool,
    #[qml(skip)]
    pub(crate) state: Rc<RectState>,
}

impl Default for SelectionRectangle {
    fn default() -> Self {
        Self {
            control: Control::default(),
            selection_mode: select_by::AUTO,
            target: None,
            top_left_handle: None,
            bottom_right_handle: None,
            active: false,
            dragging: false,
            state: Rc::default(),
        }
    }
}

/// What `SelectionRectangle.` reads on a handle.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "SelectionRectangleAttached")]
pub struct SelectionRectangleAttached {
    #[qml(notify, readonly, object = "SelectionRectangle")]
    pub control: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub dragging: bool,
}

#[qml_impl]
impl SelectionRectangle {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        start(cx.engine, cx.this)
    }
}

fn state_of(e: &Engine, me: ObjectId) -> Option<Rc<RectState>> {
    e.with(me, |r: &SelectionRectangle| r.state.clone()).ok()
}

fn attacher() -> MetaRef {
    MetaRef::Static(SelectionRectangle::static_meta())
}

fn start(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let st = state_of(e, me).ok_or(EngineError::DeadObject(me))?;
    let scope = e.object_scope(me).ok_or(EngineError::DeadObject(me))?;
    st.block
        .set(Some(e.graph().property(scope, Value::Undefined)?));

    // The view selects for the rectangle while it is its target.
    let s = st.clone();
    e.effect_in(me, move |e| {
        let target =
            object(e, me, SelectionRectangle::TARGET.index).filter(|t| is_a::<TableView>(e, *t));
        e.untrack(|| retarget(e, me, &s, target));
    })?;

    // A view with nothing selected has no selection of the rectangle's.
    e.effect_in(me, move |e| {
        let selected = object(e, me, SelectionRectangle::TARGET.index)
            .and_then(|t| object(e, t, TableView::SELECTION_MODEL.index))
            .is_some_and(|sm| {
                e.read(sm, ItemSelectionModel::HAS_SELECTION.index)
                    .to_boolean()
            });
        if !selected {
            e.untrack(|| set(e, me, SelectionRectangle::ACTIVE.index, false));
        }
    })?;

    // The handles sit on the corners of the block while it is active.
    let s = st.clone();
    e.effect_in(me, move |e| place(e, me, &s))?;
    Ok(())
}

fn set(e: &Engine, obj: ObjectId, p: ljos_engine::PropIndex, on: bool) {
    check(
        e,
        "SelectionRectangle",
        set_computed(e, obj, p, Value::Bool(on)),
    );
}

/// The rectangle selects in `target` from now on, and no longer in the
/// view it did.
fn retarget(e: &Engine, me: ObjectId, st: &Rc<RectState>, target: Option<ObjectId>) {
    if st.target.get() == target {
        return;
    }
    if let Some(old) = st.target.replace(target)
        && e.is_alive(old)
    {
        set_cell_selector(e, old, None);
    }
    for h in &st.handles {
        if let Some(h) = h.take() {
            e.destroy(h);
        }
    }
    set(e, me, SelectionRectangle::ACTIVE.index, false);
    set(e, me, SelectionRectangle::DRAGGING.index, false);
    if let Some(view) = target {
        set_cell_selector(e, view, Some(Rc::new(Selector(me))));
    }
}

/// Makes the handles the rectangle does not have yet, in the content of
/// its view.
fn make_handles(e: &Engine, me: ObjectId, st: &RectState) {
    let Some(content) = st.target.get().and_then(|v| flickable::content_item(e, v)) else {
        return;
    };
    let components = [
        SelectionRectangle::TOP_LEFT_HANDLE.index,
        SelectionRectangle::BOTTOM_RIGHT_HANDLE.index,
    ];
    for (slot, p) in st.handles.iter().zip(components) {
        if slot.get().is_some_and(|h| e.is_alive(h)) {
            continue;
        }
        let Some(c) = e.untrack(|| object(e, me, p)) else {
            continue;
        };
        let made = e.component_create_owned(c, me, |e, h| {
            if ljos_quick::is_item(e, h) {
                check(
                    e,
                    "SelectionRectangle",
                    set_parent_item(e, h, Some(content)),
                );
                check(
                    e,
                    "SelectionRectangle",
                    e.write_internal(
                        h,
                        Item::Z.index,
                        Value::Real(100.0),
                        ljos_engine::WriteKind::Initial,
                    ),
                );
            }
            let r = e.attached_object(h, &attacher()).and_then(|att| {
                set_computed(
                    e,
                    att,
                    SelectionRectangleAttached::CONTROL.index,
                    Value::Object(me),
                )
            });
            check(e, "SelectionRectangle", r);
        });
        match made {
            Ok(h) if ljos_quick::is_item(e, h) => slot.set(Some(h)),
            Ok(h) => {
                e.destroy(h);
            }
            Err(err) => check(e, "SelectionRectangle", Err(err)),
        }
    }
}

/// Puts each handle on its corner of the block, shown while the
/// rectangle is active.
fn place(e: &Engine, me: ObjectId, st: &RectState) {
    let active = e.read(me, SelectionRectangle::ACTIVE.index).to_boolean();
    let block = st
        .block
        .get()
        .map(|n| e.graph().get(n))
        .and_then(|v| block_of(e, &v));
    let rect = match (active, st.target.get(), block) {
        (true, Some(view), Some(block)) => cell_block_rect(e, view, block),
        _ => None,
    };
    for (i, slot) in st.handles.iter().enumerate() {
        let Some(h) = slot.get().filter(|h| e.is_alive(*h)) else {
            continue;
        };
        let (w, ht) = (
            e.read(h, Item::WIDTH.index).to_number(),
            e.read(h, Item::HEIGHT.index).to_number(),
        );
        e.untrack(|| {
            set(e, h, Item::VISIBLE.index, rect.is_some());
            let Some(rect) = rect else {
                return;
            };
            let corner = if i == 0 {
                (rect.x0, rect.y0)
            } else {
                (rect.x1, rect.y1)
            };
            for (p, v) in [
                (Item::X.index, corner.0 - w / 2.0),
                (Item::Y.index, corner.1 - ht / 2.0),
            ] {
                check(
                    e,
                    "SelectionRectangle",
                    set_computed(e, h, p, Value::Real(v)),
                );
            }
        });
    }
}

fn block_of(e: &Engine, v: &Value) -> Option<CellBlock> {
    let Value::List(list) = e.plain(v) else {
        return None;
    };
    let n = |i: usize| list.get(i).map(|v| v.to_number() as usize);
    Some(CellBlock {
        top: n(0)?,
        left: n(1)?,
        bottom: n(2)?,
        right: n(3)?,
    })
}

/// Marks handle `handle` as dragged, and the other as not.
fn mark(e: &Engine, st: &RectState, handle: Option<usize>) {
    for (i, slot) in st.handles.iter().enumerate() {
        let Some(h) = slot.get().filter(|h| e.is_alive(*h)) else {
            continue;
        };
        let r = e.attached_object(h, &attacher()).and_then(|att| {
            set_computed(
                e,
                att,
                SelectionRectangleAttached::DRAGGING.index,
                Value::Bool(handle == Some(i)),
            )
        });
        check(e, "SelectionRectangle", r);
    }
}

/// The rectangle as its view's selector.
struct Selector(ObjectId);

impl CellSelector for Selector {
    fn mode(&self, e: &Engine) -> i32 {
        e.untrack(|| e.read(self.0, SelectionRectangle::SELECTION_MODE.index))
            .to_number() as i32
    }

    fn handles(&self, e: &Engine) -> [Option<ObjectId>; 2] {
        state_of(e, self.0).map_or([None; 2], |st| [st.handles[0].get(), st.handles[1].get()])
    }

    fn selected(&self, e: &Engine, block: CellBlock, dragging: bool, handle: Option<usize>) {
        let (me, Some(st)) = (self.0, state_of(e, self.0)) else {
            return;
        };
        e.batch(|| {
            make_handles(e, me, &st);
            if let Some(n) = st.block.get() {
                let corners = [block.top, block.left, block.bottom, block.right]
                    .map(|c| Value::Int(c as i32));
                e.graph().set(n, Value::list(corners.to_vec()));
            }
            set(e, me, SelectionRectangle::ACTIVE.index, true);
            set(e, me, SelectionRectangle::DRAGGING.index, dragging);
            mark(e, &st, handle);
        });
    }

    fn dropped(&self, e: &Engine) {
        let (me, Some(st)) = (self.0, state_of(e, self.0)) else {
            return;
        };
        set(e, me, SelectionRectangle::DRAGGING.index, false);
        mark(e, &st, None);
    }
}

pub(crate) fn register(engine: &Engine) {
    engine.register_type(
        "QtQuick.Templates",
        Version::new(2, 15),
        MetaRef::Static(SelectionRectangle::static_meta()),
    );
}
