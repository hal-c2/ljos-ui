//! Editing a cell, as qml6 does: the delegate's `TableView.editDelegate`
//! is made a child of the cell's delegate, which rises above the others
//! and is told `editing`, and the editor takes the focus. Return or Enter
//! in the editor sends its attached `commit` and closes it, Escape closes
//! it without, and Tab or Backtab commits and edits the next or the
//! previous cell. What the editor does with those keys itself does not
//! count, as the view sees them first.

use ljos_engine::{Engine, LogLevel, ObjectId, Value};

use super::{
    TableState, TableView, TableViewAttached, TreeView, attached_of, cell_of_index, layout, nav,
    state_of, tell_attached,
};
use crate::focus::{FocusReason, tree as focus};
use crate::input::{KeyEvent, KeyKind};
use crate::item::{Item, is_a, write_computed};
use crate::keys::{self, KeyFilterId, key};
use crate::tree;

/// An editor open on a cell.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Editing {
    pub cell: (usize, usize),
    pub delegate: ObjectId,
    pub editor: ObjectId,
    filter: KeyFilterId,
}

fn refuse(e: &Engine, view: ObjectId, why: &str) {
    let name = if is_a::<TreeView>(e, view) {
        "TreeView"
    } else {
        "TableView"
    };
    e.log(
        LogLevel::Warning,
        "qml",
        format!("QML {name}: cannot edit: {why}"),
    );
}

/// `edit(index)`: opens an editor on the cell of a model index.
pub(crate) fn edit_index(e: &Engine, view: ObjectId, index: &Value) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    match cell_of_index(e, &st, index) {
        Some(cell) if cell.1 < st.cells.columns() => {
            open(e, view, &st, cell);
        }
        _ => refuse(e, view, "index is not valid!"),
    }
}

/// Opens an editor on `cell`, committing the one open first. False when
/// the cell has none.
pub(crate) fn open(e: &Engine, view: ObjectId, st: &TableState, cell: (usize, usize)) -> bool {
    let cols = st.cells.columns().max(1);
    let delegate = || st.delegates()?.object_at(cell.0 * cols + cell.1);
    if delegate().is_none() {
        layout::show_row(e, view, cell.0 as i32);
        e.untrack(|| layout::pass(e, view, st));
    }
    let Some(delegate) = delegate() else {
        return false;
    };
    let component = attached_of(e, delegate).and_then(|att| {
        e.untrack(|| {
            e.read(att, TableViewAttached::EDIT_DELEGATE.index)
                .as_object()
        })
    });
    let Some(component) = component.filter(|c| e.is_alive(*c)) else {
        refuse(e, view, "no TableView.editDelegate set!");
        return false;
    };
    close(e, view, st, true);
    nav::make_current(e, view, st, cell);

    let made = e.component_create_owned(component, delegate, |e, root| {
        super::warn(e, "editor", tree::set_parent_item(e, root, Some(delegate)));
    });
    let editor = match made {
        Ok(editor) => editor,
        Err(err) => {
            super::warn(e, "editDelegate", Err::<(), _>(err));
            return false;
        }
    };
    if let Some(att) = attached_of(e, editor) {
        tell_attached(e, view, att);
    }
    let filter = keys::add_key_filter(e, move |e, _, ev| keys_of_editor(e, view, ev));
    st.editing.set(Some(Editing {
        cell,
        delegate,
        editor,
        filter,
    }));
    raise(e, delegate, 2.0);
    layout::decorate(e, view, st, cell.0 * cols + cell.1, delegate, false);
    focus::force_active_focus(e, editor, FocusReason::Other);
    true
}

/// Closes the open editor, if there is one; with `commit`, its attached
/// `commit` is sent first.
pub(crate) fn close(e: &Engine, view: ObjectId, st: &TableState, commit: bool) {
    let Some(ed) = st.editing.get() else {
        return;
    };
    if commit
        && e.is_alive(ed.editor)
        && let Some(att) = attached_of(e, ed.editor)
    {
        super::warn(e, "commit", e.emit_by_name(att, "commit", &[]).map(drop));
    }
    // A commit handler may have closed it already.
    if st.editing.get().is_none_or(|now| now.editor != ed.editor) {
        return;
    }
    st.editing.set(None);
    keys::remove_key_filter(e, ed.filter);
    let had_focus = e.is_alive(ed.editor) && holds_focus(e, ed.editor);
    if e.is_alive(ed.editor) {
        let _ = tree::set_parent_item(e, ed.editor, None);
        // At once, as on qml6: an editor opened next is not told its
        // predecessor's destruction after its own completion.
        e.destroy_now(ed.editor);
    }
    if e.is_alive(ed.delegate) {
        raise(e, ed.delegate, 1.0);
        let cols = st.cells.columns().max(1);
        layout::decorate(
            e,
            view,
            st,
            ed.cell.0 * cols + ed.cell.1,
            ed.delegate,
            false,
        );
    }
    if had_focus {
        focus::force_active_focus(e, view, FocusReason::Other);
    }
}

/// The edited delegate is going: its editor goes with it, uncommitted.
pub(crate) fn forget(e: &Engine, st: &TableState, delegate: ObjectId) {
    if let Some(ed) = st.editing.get()
        && ed.delegate == delegate
    {
        st.editing.set(None);
        keys::remove_key_filter(e, ed.filter);
    }
}

fn raise(e: &Engine, item: ObjectId, z: f64) {
    super::warn(
        e,
        "z",
        write_computed(e, item, Item::Z.index, Value::Real(z)),
    );
}

/// Whether the item with the active focus is `item` or within it.
fn holds_focus(e: &Engine, item: ObjectId) -> bool {
    let root = focus::top_of(e, item);
    let mut cur = focus::active_focus_item(e, root);
    while let Some(o) = cur {
        if o == item {
            return true;
        }
        cur = e.visual_parent(o);
    }
    false
}

/// The keys the view takes from its open editor.
fn keys_of_editor(e: &Engine, view: ObjectId, ev: &KeyEvent) -> bool {
    if ev.kind != KeyKind::Press {
        return false;
    }
    let Some(st) = state_of(e, view) else {
        return false;
    };
    let Some(ed) = st.editing.get() else {
        return false;
    };
    if !matches!(
        ev.key,
        key::RETURN | key::ENTER | key::ESCAPE | key::TAB | key::BACKTAB
    ) || !holds_focus(e, ed.editor)
    {
        return false;
    }
    match ev.key {
        key::RETURN | key::ENTER => close(e, view, &st, true),
        key::ESCAPE => close(e, view, &st, false),
        _ => {
            close(e, view, &st, true);
            let next = nav::next_cell(&st, ed.cell, ev.key == key::TAB);
            nav::forget(e, view, &st);
            open(e, view, &st, next);
        }
    }
    true
}

/// A key pressed on the view, as its edit triggers say: an edit key
/// opens the current cell's editor, and with `AnyKeyPressed` a key that
/// types does and is typed into it.
pub(crate) fn key_opens(
    e: &Engine,
    view: ObjectId,
    st: &TableState,
    current: (usize, usize),
    ev: &KeyEvent,
) -> bool {
    use super::edit_trigger as t;
    let triggers = e.untrack(|| e.read(view, TableView::EDIT_TRIGGERS.index).to_number() as i32);
    let edit_key = matches!(ev.key, key::RETURN | key::ENTER) || ev.key == key::F1 + 1;
    let types = !ev.text.is_empty() && !ev.text.chars().any(char::is_control);
    if edit_key && triggers & (t::EDIT_KEY_PRESSED | t::ANY_KEY_PRESSED) != 0 {
        return open(e, view, st, current);
    }
    if types && triggers & t::ANY_KEY_PRESSED != 0 && open(e, view, st, current) {
        if let Some(ed) = st.editing.get() {
            let root = focus::top_of(e, ed.editor);
            keys::deliver_key(e, root, ev);
        }
        return true;
    }
    false
}

/// Whether a tap at `(x, y)` of the content falls on the open editor.
pub(crate) fn on_editor(e: &Engine, st: &TableState, x: f64, y: f64) -> bool {
    let Some(ed) = st.editing.get() else {
        return false;
    };
    let (Some(top), Some((left, _))) = (
        st.row_y.borrow().get(ed.cell.0).copied(),
        st.column_x.borrow().get(ed.cell.1).copied(),
    ) else {
        return false;
    };
    let read = |p| e.untrack(|| e.read(ed.editor, p).to_number());
    let (ex, ey) = (left + read(Item::X.index), top + read(Item::Y.index));
    x >= ex && x < ex + read(Item::WIDTH.index) && y >= ey && y < ey + read(Item::HEIGHT.index)
}

/// Taps edit as the edit triggers say: `SingleTapped` opens the tapped
/// cell, `SelectedTapped` the tapped cell when it was selected before the
/// tap, and `DoubleTapped` a cell tapped twice.
pub(crate) fn tap_opens(
    e: &Engine,
    view: ObjectId,
    st: &TableState,
    cell: (usize, usize),
    double: bool,
    was_selected: bool,
) {
    use super::edit_trigger as t;
    let triggers = e.untrack(|| e.read(view, TableView::EDIT_TRIGGERS.index).to_number() as i32);
    let opens = if double {
        triggers & t::DOUBLE_TAPPED != 0
    } else {
        triggers & t::SINGLE_TAPPED != 0 || (triggers & t::SELECTED_TAPPED != 0 && was_selected)
    };
    if opens {
        open(e, view, st, cell);
    }
}
