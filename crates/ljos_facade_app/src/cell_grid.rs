//! A terminal-like item, written against `ljos::item` alone: a grid of
//! character cells sized from its font, that a pointer selects in and
//! hovers over, that an input method types into and places its window by,
//! and that keeps its state outside its properties and says when to paint
//! it again.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use ljos::item::{self, DisplayListBuilder, Font, ItemPaint, LayoutParams, PointerEvent};
use ljos::item::{FocusReason, MouseButtons, PointerItem, Rgba, Surrounding, TextEngine};
use ljos::item::{TextInputClient, TextStyle, kurbo};
use ljos::prelude::*;

/// A grid of cells as large as its font's widest letter, as many as fit.
/// What it shows is typed text, one character to a cell, wrapping at the
/// last column; it is not a property, so the grid asks to be painted again
/// itself when it changes.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "CellGrid", extends = Item)]
pub struct CellGrid {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub font: Font,
    /// How many cells fit across, from the font and the width.
    #[qml(notify, readonly)]
    pub columns: i32,
    /// How many cells fit down, from the font and the height.
    #[qml(notify, readonly)]
    pub rows: i32,
    #[qml(skip)]
    pub cells: Rc<RefCell<Cells>>,
}

/// A grid's state outside its properties.
#[derive(Debug, Default)]
pub struct Cells {
    pub text: Vec<char>,
    /// One cell, in logical pixels.
    pub cell: kurbo::Size,
    /// The cell a press held by the grid started on.
    pub anchor: Option<usize>,
    /// The selected cells.
    pub selection: Option<Range<usize>>,
    /// The cell under a pointer with no button held.
    pub hovered: Option<usize>,
    /// What drops the selection when the grid loses the focus.
    pub focus_lost: Option<ConnectionId>,
}

#[qml_impl]
impl CellGrid {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let cells = cells_of(e, me);
        let measured = Rc::clone(&cells);
        e.effect_in(me, move |e| measure(e, me, &measured))?;
        let id = e.connect_by_name(me, "activeFocusChanged", move |e, _| {
            if !item::prop(e, me, Item::ACTIVE_FOCUS) {
                cells_of(e, me).borrow_mut().selection = None;
                item::repaint(e, me);
            }
        })?;
        cells.borrow_mut().focus_lost = Some(id);
        Ok(())
    }
}

fn cells_of(engine: &Engine, grid: ObjectId) -> Rc<RefCell<Cells>> {
    engine
        .with::<CellGrid, _>(grid, |g| Rc::clone(&g.cells))
        .unwrap_or_default()
}

/// Tracked: the cell follows the font and the system's text size, the
/// counts follow the cell and the size. Measured outside paint, with the
/// text engine the scene paints with.
fn measure(engine: &Engine, grid: ObjectId, cells: &RefCell<Cells>) {
    let style = item::text_style(engine, &item::prop(engine, grid, CellGrid::FONT));
    let Some(cell) = item::with_text_engine(|te| {
        let m = te.layout("M", &style, &LayoutParams::default());
        kurbo::Size::new(m.advance, m.content_height)
    }) else {
        return;
    };
    cells.borrow_mut().cell = cell;
    let fit = |length: f64, by: f64| {
        if by > 0.0 {
            (length / by).floor() as i32
        } else {
            0
        }
    };
    let width = item::prop(engine, grid, Item::WIDTH);
    let height = item::prop(engine, grid, Item::HEIGHT);
    let _ = engine.set_internal(grid, CellGrid::COLUMNS, fit(width, cell.width));
    let _ = engine.set_internal(grid, CellGrid::ROWS, fit(height, cell.height));
    item::repaint(engine, grid);
}

fn columns(engine: &Engine, grid: ObjectId) -> usize {
    item::prop(engine, grid, CellGrid::COLUMNS).max(1) as usize
}

fn cell_rect(cell: kurbo::Size, columns: usize, index: usize) -> kurbo::Rect {
    let (row, column) = (index / columns, index % columns);
    kurbo::Rect::from_origin_size((column as f64 * cell.width, row as f64 * cell.height), cell)
}

/// The cell under `p`, in the grid's coordinates.
fn cell_at(engine: &Engine, grid: ObjectId, p: kurbo::Point) -> Option<usize> {
    let cell = cells_of(engine, grid).borrow().cell;
    let (columns, rows) = (
        columns(engine, grid),
        item::prop(engine, grid, CellGrid::ROWS).max(0) as usize,
    );
    if cell.is_zero_area() || p.x < 0.0 || p.y < 0.0 {
        return None;
    }
    let (column, row) = ((p.x / cell.width) as usize, (p.y / cell.height) as usize);
    (column < columns && row < rows).then_some(row * columns + column)
}

/// What a grid shows.
#[derive(Debug)]
pub struct Screenful {
    pub lines: Vec<String>,
    pub style: TextStyle,
    pub cell: kurbo::Size,
    pub columns: usize,
    pub selection: Option<Range<usize>>,
    pub hovered: Option<usize>,
    /// The cell the next character goes in, when the grid has the focus.
    pub cursor: Option<usize>,
    pub dark: bool,
    /// One device pixel, in logical pixels: as wide as the cursor is.
    pub pixel: f64,
}

/// The colour a selected cell is filled with.
pub fn selection_colour() -> Rgba {
    Rgba::rgb(0x33, 0x66, 0xcc)
}

/// The colour of the line under a hovered cell.
pub fn hover_colour() -> Rgba {
    Rgba::rgb(0x99, 0x99, 0x99)
}

impl ItemPaint for Screenful {
    fn paint(&self, size: kurbo::Size, te: &mut TextEngine, out: &mut DisplayListBuilder) {
        let (background, ink) = if self.dark {
            (Rgba::BLACK, Rgba::WHITE)
        } else {
            (Rgba::WHITE, Rgba::BLACK)
        };
        out.fill_rect(size.to_rect(), background);
        for i in self.selection.clone().into_iter().flatten() {
            out.fill_rect(cell_rect(self.cell, self.columns, i), selection_colour());
        }
        if let Some(i) = self.hovered {
            let r = cell_rect(self.cell, self.columns, i);
            let line = kurbo::Rect::new(r.x0, r.y1 - self.pixel, r.x1, r.y1);
            out.fill_rect(line, hover_colour());
        }
        for (row, text) in self.lines.iter().enumerate() {
            let layout = te.layout(text, &self.style, &LayoutParams::default());
            let top = row as f64 * self.cell.height;
            for run in layout.lines.iter().flat_map(|l| &l.runs) {
                out.glyphs(
                    Arc::clone(&run.glyphs),
                    kurbo::Vec2::new(0.0, top),
                    ink,
                    run.decorations,
                );
            }
        }
        if let Some(i) = self.cursor {
            let r = cell_rect(self.cell, self.columns, i);
            out.fill_rect(kurbo::Rect::new(r.x0, r.y0, r.x0 + self.pixel, r.y1), ink);
        }
    }
}

/// Tracked: the grid repaints when its font, size, focus, colour scheme
/// or screen changes, and when [`item::repaint`] says its cells did.
pub fn read_cell_grid(engine: &Engine, grid: ObjectId) -> Rc<dyn ItemPaint> {
    let columns = columns(engine, grid);
    let cells = cells_of(engine, grid);
    let cells = cells.borrow();
    let dark = ljos::style_hints(engine).is_some_and(|h| {
        item::prop(engine, h, StyleHints::COLOR_SCHEME) == ColorScheme::Dark.qt_value()
    });
    let focused = item::prop(engine, grid, Item::ACTIVE_FOCUS);
    Rc::new(Screenful {
        lines: cells.text.chunks(columns).map(String::from_iter).collect(),
        style: item::text_style(engine, &item::prop(engine, grid, CellGrid::FONT)),
        cell: cells.cell,
        columns,
        selection: cells.selection.clone(),
        hovered: cells.hovered,
        cursor: focused.then_some(cells.text.len()),
        dark,
        pixel: 1.0 / item::device_pixel_ratio(engine, grid),
    })
}

// The cells are borrowed only between reads: a repaint or a focus change
// can run what reads them again at once.
fn press(engine: &Engine, grid: ObjectId, ev: &PointerEvent, p: kurbo::Point) -> bool {
    if ev.button != Some(MouseButtons::LEFT) {
        return false;
    }
    item::force_active_focus(engine, grid, FocusReason::Mouse);
    let anchor = cell_at(engine, grid, p);
    {
        let cells = cells_of(engine, grid);
        let mut cells = cells.borrow_mut();
        cells.anchor = anchor;
        cells.selection = None;
    }
    item::repaint(engine, grid);
    true
}

fn drag(engine: &Engine, grid: ObjectId, _: &PointerEvent, p: kurbo::Point) {
    let cells = cells_of(engine, grid);
    let anchor = cells.borrow().anchor;
    if let (Some(a), Some(c)) = (anchor, cell_at(engine, grid, p)) {
        cells.borrow_mut().selection = Some(a.min(c)..a.max(c) + 1);
        item::repaint(engine, grid);
    }
}

fn hover_move(engine: &Engine, grid: ObjectId, p: kurbo::Point) {
    let hovered = cell_at(engine, grid, p);
    let cells = cells_of(engine, grid);
    let was = std::mem::replace(&mut cells.borrow_mut().hovered, hovered);
    if was != hovered {
        item::repaint(engine, grid);
    }
}

const POINTER: PointerItem = PointerItem {
    press,
    moved: drag,
    release: |e, grid, _, _| cells_of(e, grid).borrow_mut().anchor = None,
    cancel: |e, grid| {
        let cells = cells_of(e, grid);
        let had = cells.borrow_mut().selection.take();
        cells.borrow_mut().anchor = None;
        if had.is_some() {
            item::repaint(e, grid);
        }
    },
    hover: |e, grid, over| {
        if !over {
            cells_of(e, grid).borrow_mut().hovered = None;
            item::repaint(e, grid);
        }
    },
};

/// A terminal types at its end, whatever the input method thinks is
/// selected: what is typed goes to a program, not into an edited text.
struct Typing;

impl TextInputClient for Typing {
    fn surrounding(&self, engine: &Engine, grid: ObjectId) -> Surrounding {
        let text = String::from_iter(&cells_of(engine, grid).borrow().text);
        let end = text.encode_utf16().count();
        Surrounding {
            text,
            anchor: end,
            cursor: end,
        }
    }

    fn replace(&self, engine: &Engine, grid: ObjectId, _: Range<usize>, text: &str) {
        cells_of(engine, grid)
            .borrow_mut()
            .text
            .extend(text.chars());
        item::repaint(engine, grid);
    }

    fn set_selection(&self, _: &Engine, _: ObjectId, _: usize, _: usize) {}

    fn cursor_rect(&self, engine: &Engine, grid: ObjectId) -> kurbo::Rect {
        let columns = columns(engine, grid);
        let cells = cells_of(engine, grid);
        let cells = cells.borrow();
        cell_rect(cells.cell, columns, cells.text.len())
    }

    fn hints(&self, _: &Engine, _: ObjectId) -> i32 {
        0
    }
}

pub(crate) fn register(engine: &Engine) {
    item::register_paint_item::<CellGrid>(engine, read_cell_grid);
    item::register_pointer_item::<CellGrid>(engine, POINTER);
    item::register_hover_move::<CellGrid>(engine, hover_move);
    item::register_text_input_client::<CellGrid>(engine, Rc::new(Typing));
}
