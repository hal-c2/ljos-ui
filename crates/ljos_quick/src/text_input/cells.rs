//! Selecting the cells of a table in a rich `TextEdit`, as Qt 6.11 does
//! (features/text-input/rich-text-table-selection.feature).
//!
//! A selection from one cell into another of its table selects every cell
//! of the rectangle the two span. Its ends are moved: the cursor to the
//! start of its cell, the anchor to the start of its own, or to its end
//! when the cursor is before it. A selection from a cell to outside its
//! table takes in the whole table instead. The anchor the user set is
//! kept, so that moving the cursor on moves the ends from there again.

use std::ops::Range;

use ljos_text::rich::{CELL, CELL_PARAGRAPH, LINE_BREAK, TableCell};
use ljos_text::{TextModel, byte_to_utf16, utf16_to_byte};

use super::base::State;

/// The anchor's and the cursor's cells when the selection is of cells.
pub(crate) fn selected(s: &State) -> Option<(TableCell, TableCell)> {
    let r = s.rich.as_ref()?;
    let text = s.model.text();
    if r.doc.text() != text || !s.model.has_selection() {
        return None;
    }
    let a = r.doc.cell_at(utf16_to_byte(text, s.model.anchor()))?;
    let c = r.doc.cell_at(s.model.cursor_byte())?;
    (a.rows == c.rows && (a.row, a.column) != (c.row, c.column)).then_some((a, c))
}

/// Moves the ends of the selection as a selection that reaches into a
/// table has them, from the anchor the user set.
pub(crate) fn snap(s: &mut State) {
    super::rich::sync_doc(s);
    let Some(r) = s.rich.as_ref() else {
        return;
    };
    let (anchor, cursor) = (s.model.anchor(), s.model.cursor());
    let real = match s.cell_anchor {
        Some((real, at)) if at == anchor => real,
        _ => anchor,
    };
    let doc = &r.doc;
    let text = s.model.text();
    let (a, c) = (utf16_to_byte(text, real), s.model.cursor_byte());
    let back = c < a;
    let (ca, cc) = (doc.cell_at(a), doc.cell_at(c));
    let (a, c) = match (&ca, &cc) {
        (Some(x), Some(y)) if x.rows == y.rows => {
            if (x.row, x.column) == (y.row, y.column) {
                (a, c)
            } else if back {
                (x.text.end, y.text.start)
            } else {
                (x.text.start, y.text.start)
            }
        }
        // An end in a table goes to the table's edge away from the other.
        _ => {
            let edge = |cell: &Option<TableCell>, at: usize, to_end: bool| {
                cell.as_ref().map_or(at, |cell| {
                    let t = doc.table_range(cell.rows.clone());
                    if to_end { t.end } else { t.start }
                })
            };
            (edge(&ca, a, back), edge(&cc, c, !back))
        }
    };
    let (a, c) = (byte_to_utf16(text, a), byte_to_utf16(text, c));
    if (a, c) != (anchor, cursor) {
        s.model.select(a, c);
    }
    s.cell_anchor = (a != real).then_some((real, a));
}

/// What Ctrl with Backspace or Delete does over a table's edge.
pub(crate) enum OverEdge {
    /// From a cell out of its table: nothing.
    Stays,
    /// From one cell into another: the cells are selected, to be emptied.
    Cells,
    /// From outside a table into it: this span (anchor, cursor, in UTF-16
    /// units) takes in the whole table, to go as a selection does.
    Span(usize, usize),
}

/// Ctrl with Backspace or Delete, taking the text from the cursor `to`
/// where it says, over a table's edge as Qt 6.11 does
/// (features/text-input/rich-text-cell-paragraphs.feature): the span is
/// taken as a selection into a table is. `None` where the span crosses no
/// table's edge.
pub(crate) fn word_over_edge(
    s: &mut State,
    to: impl FnOnce(&TextModel, usize) -> usize,
) -> Option<OverEdge> {
    super::rich::sync_doc(s);
    let doc = &s.rich.as_ref()?.doc;
    let text = s.model.text();
    if doc.text() != text || s.model.has_selection() {
        return None;
    }
    let cursor = s.model.cursor();
    let to = to(&s.model, cursor);
    let (at, end) = (s.model.cursor_byte(), utf16_to_byte(text, to));
    let span = at.min(end)..at.max(end);
    let crosses = text[span.clone()]
        .char_indices()
        .any(|(i, _)| doc.is_table_edge(span.start + i));
    if !crosses {
        return None;
    }
    if let Some(cell) = doc.cell_at(at)
        && doc.cell_at(end).is_none_or(|other| other.rows != cell.rows)
    {
        return Some(OverEdge::Stays);
    }
    s.cell_anchor = None;
    s.model.select(cursor, to);
    snap(s);
    if selected(s).is_some() {
        return Some(OverEdge::Cells);
    }
    let span = (s.model.anchor(), s.model.cursor());
    s.model.select(cursor, cursor);
    s.cell_anchor = None;
    Some(OverEdge::Span(span.0, span.1))
}

/// Where Shift with Left or Right takes the cursor in a table: along a row
/// from cell to cell when cells are selected, and in a cell never past the
/// row's ends. `None` when the move is an ordinary one; `Some(None)` when
/// the cursor stays.
pub(crate) fn shift_step(s: &State, forward: bool) -> Option<Option<usize>> {
    let r = s.rich.as_ref()?;
    let text = s.model.text();
    if r.doc.text() != text {
        return None;
    }
    let units = |b: usize| byte_to_utf16(text, b);
    if let Some((_, c)) = selected(s) {
        let row = r.doc.cells_of_row(c.row);
        let next = if forward {
            row.get(c.column + 1).map(|n| n.start)
        } else {
            c.column.checked_sub(1).map(|p| row[p].end)
        };
        return Some(next.map(units));
    }
    let at = s.model.cursor_byte();
    let c = r.doc.cell_at(at)?;
    let last = r.doc.cells_of_row(c.row).len() - 1;
    let stuck = if forward {
        c.column == last && at == c.text.end
    } else {
        c.column == 0 && at == c.text.start
    };
    stuck.then_some(None)
}

/// `selectedText`: of a selection of cells, their text one after another.
pub(crate) fn selected_text(s: &State) -> String {
    match (selected(s), s.rich.as_ref()) {
        (Some((a, c)), Some(r)) => r
            .doc
            .cell_rectangle(&a, &c)
            .into_iter()
            .map(|cell| r.doc.plain_text(cell))
            .collect(),
        _ => super::rich::plain_text(s, s.model.selection_bytes()),
    }
}

/// The characters a copy puts on the clipboard: of a selection of cells a
/// line break before each cell and one after the last; of other rich text
/// a line break between cells. A line break and the start of a paragraph
/// of a cell are a line break too, as in Qt 6.11
/// (features/text-input/rich-text-cell-paragraphs.feature).
pub(crate) fn copied_text(s: &mut State) -> String {
    super::rich::sync_doc(s);
    match (selected(s), s.rich.as_ref()) {
        (Some((a, c)), Some(r)) => {
            let mut out = String::new();
            for cell in r.doc.cell_rectangle(&a, &c) {
                out.push('\n');
                out.push_str(&r.doc.text()[cell].replace([CELL_PARAGRAPH, LINE_BREAK], "\n"));
            }
            out.push('\n');
            out
        }
        (None, Some(_)) => s
            .model
            .selected_text()
            .replace([CELL, CELL_PARAGRAPH, LINE_BREAK], "\n"),
        _ => s.model.selected_text().to_owned(),
    }
}

/// Empties every selected cell, as typing, Backspace or Delete over them
/// does, and leaves the cursor at the start of the cursor's cell. `None`
/// when no cells are selected; else whether any text went.
pub(crate) fn empty(s: &mut State) -> Option<bool> {
    super::rich::sync_doc(s);
    let (a, c) = selected(s)?;
    let doc = &s.rich.as_ref()?.doc;
    let cells: Vec<Range<usize>> = doc.cell_rectangle(&a, &c);
    let removed = cells.iter().any(|r| !r.is_empty());
    if removed {
        s.model.break_undo();
        super::rich::remove_each(s, &cells);
    }
    let doc = &s.rich.as_ref()?.doc;
    let at = doc.cells_of_row(c.row)[c.column].start;
    let at = byte_to_utf16(s.model.text(), at);
    s.model.select(at, at);
    s.cell_anchor = None;
    Some(removed)
}
