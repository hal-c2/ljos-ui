//! `MonthGrid`, `DayOfWeekRow`, `WeekNumberColumn` (QtQuick.Templates),
//! `CalendarModel` and the `Calendar` months (QtQuick.Controls).
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-controls-monthgrid.html and
//! its siblings, and what `qml6` (Qt 6.11.2) does, pinned by
//! `features/controls/calendar.feature`.
//!
//! The cells are made here, from `delegate`, into the content item, as a
//! page indicator makes its dots: each cell has a context object of its
//! own that holds the roles of its row (`date`, `day`, `weekNumber`, …), so
//! a delegate reads them as plain names, through `model`, or as required
//! properties. The cells are spread over the room the content item has,
//! `spacing` apart; the names and numbers come from the `locale`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, ItemModel, MetaRef, ModelChange, ModelListeners, ObjectId,
    PropIndex, PropertyFlags, QmlObject, QmlType, Value, qml_impl,
};
use ljos_i18n::LocaleInfo;
use ljos_i18n::calendar::{NameWidth, civil_from_days, days_from_civil, iso_week_number};
use ljos_quick::{
    Item, PointerEvent, PointerItem, flickable, register_pointer_item, set_parent_item,
};
use ljos_render::kurbo;

use crate::control::{Control, hover, is_left, takes_input};
use crate::ticker;
use crate::util::{check, num, object, warn};

const COLUMNS: usize = 7;
const MONTH_ROWS: usize = 6;

/// The months, as `Calendar.January` to `Calendar.December`.
fn calendar_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Month",
        [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ]
        .into_iter()
        .zip(0_i64..)
        .collect(),
    )]
}

/// `Calendar`: the months, by name.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Controls",
    name = "Calendar",
    enums = calendar_enums,
    uncreatable
)]
pub struct Calendar {}

// ------------------------------------------------------------- dates in JS

/// The JavaScript functions dates are made and read with, made once.
#[derive(Default)]
struct DateJs {
    make: RefCell<Option<Value>>,
    parts: RefCell<Option<Value>>,
    today: RefCell<Option<Value>>,
}

/// A date as a number a script can hand back: year, month and day packed.
fn unpack(n: f64) -> Option<(i32, u32, u32)> {
    if !n.is_finite() {
        return None;
    }
    let n = n as i64;
    Some((
        (n / 10_000 - 300_000) as i32,
        ((n / 100) % 100) as u32,
        (n % 100) as u32,
    ))
}

impl DateJs {
    fn function(e: &Engine, slot: &RefCell<Option<Value>>, source: &str) -> Option<Value> {
        if let Some(f) = slot.borrow().clone() {
            return Some(f);
        }
        let f = e.eval_js(source).ok()?;
        *slot.borrow_mut() = Some(f.clone());
        Some(f)
    }

    /// The local midnight that starts the day, as a script's `Date`.
    fn make(&self, e: &Engine, year: i32, month0: u32, day: u32) -> Value {
        let f = Self::function(
            e,
            &self.make,
            "(function(y,m,d){var x=new Date(2000,0,1);x.setFullYear(y,m,d);\
             x.setHours(0,0,0,0);return x})",
        );
        f.and_then(|f| {
            e.call_js(
                &f,
                &[
                    Value::Int(year),
                    Value::Int(month0 as i32),
                    Value::Int(day as i32),
                ],
            )
        })
        .unwrap_or_default()
    }

    /// Year, month (0 to 11) and day of a `Date`, in local time.
    fn parts(&self, e: &Engine, date: &Value) -> Option<(i32, u32, u32)> {
        let f = Self::function(
            e,
            &self.parts,
            "(function(d){if(!(d instanceof Date)||isNaN(d.getTime()))return NaN;\
             return ((d.getFullYear()+300000)*100+d.getMonth())*100+d.getDate()})",
        )?;
        unpack(e.call_js(&f, std::slice::from_ref(date))?.to_number())
    }

    /// Today, by the clock scripts see.
    fn today(&self, e: &Engine) -> (i32, u32, u32) {
        let f = Self::function(
            e,
            &self.today,
            "(function(){var d=new Date();\
             return ((d.getFullYear()+300000)*100+d.getMonth())*100+d.getDate()})",
        );
        f.and_then(|f| e.call_js(&f, &[]))
            .and_then(|v| unpack(v.to_number()))
            .unwrap_or((1970, 0, 1))
    }
}

fn date_js(e: &Engine) -> Rc<DateJs> {
    e.extension::<DateJs>()
}

// ------------------------------------------------------------------ cells

/// What a cell's delegate finds in its context: the roles of its row. A
/// role the kind of grid does not have is `undefined`. Not a QML type of
/// its own.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "CalendarCell", uncreatable)]
pub struct CalendarCell {
    #[qml(notify)]
    pub index: i32,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify)]
    pub date: Value,
    #[qml(notify)]
    pub day: Value,
    #[qml(notify)]
    pub month: Value,
    #[qml(notify)]
    pub year: Value,
    #[qml(notify)]
    pub week_number: Value,
    #[qml(notify)]
    pub today: Value,
    #[qml(notify)]
    pub long_name: Value,
    #[qml(notify)]
    pub short_name: Value,
    #[qml(notify)]
    pub narrow_name: Value,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kind {
    #[default]
    Month,
    Days,
    Weeks,
}

impl Kind {
    fn count(self) -> usize {
        match self {
            Kind::Month => COLUMNS * MONTH_ROWS,
            Kind::Days => COLUMNS,
            Kind::Weeks => MONTH_ROWS,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Month => "MonthGrid",
            Kind::Days => "DayOfWeekRow",
            Kind::Weeks => "WeekNumberColumn",
        }
    }

    fn delegate(self) -> PropIndex {
        match self {
            Kind::Month => MonthGrid::DELEGATE.index,
            Kind::Days => DayOfWeekRow::DELEGATE.index,
            Kind::Weeks => WeekNumberColumn::DELEGATE.index,
        }
    }

    /// (columns, rows) the cells are laid out in.
    fn shape(self) -> (usize, usize) {
        match self {
            Kind::Month => (COLUMNS, MONTH_ROWS),
            Kind::Days => (COLUMNS, 1),
            Kind::Weeks => (1, MONTH_ROWS),
        }
    }
}

/// What a calendar control keeps that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct Cells {
    kind: Cell<Kind>,
    /// Each cell's item, and the object that is its context.
    made: RefCell<Vec<(ObjectId, ObjectId)>>,
    /// The content item the cells are in, and the one that is watched.
    host: Cell<Option<ObjectId>>,
    watched: Cell<Option<ObjectId>>,
    /// A press is held, on this cell.
    held: Cell<bool>,
    current: Cell<Option<usize>>,
    timer: Cell<Option<ObjectId>>,
}

fn cells_of(e: &Engine, obj: ObjectId) -> Option<Rc<Cells>> {
    e.with::<MonthGrid, _>(obj, |g| g.cells.clone())
        .or_else(|_| e.with::<DayOfWeekRow, _>(obj, |g| g.cells.clone()))
        .or_else(|_| e.with::<WeekNumberColumn, _>(obj, |g| g.cells.clone()))
        .ok()
}

/// The locale a control writes its names with.
fn locale_of(e: &Engine, obj: ObjectId) -> LocaleInfo {
    let v = e.untrack(|| e.read(obj, Control::LOCALE.index));
    let name = e
        .js_member(&v, "name")
        .filter(|n| !n.is_nullish())
        .map(|n| n.to_js_string().to_string())
        .unwrap_or_else(|| e.locale_name());
    LocaleInfo::new(&name)
}

/// The first day shown of a month's grid: the day of the week the locale
/// starts on, on or before the 1st; a whole week earlier when the 1st is
/// that day.
fn grid_start(year: i32, month0: u32, first_day: u8) -> i64 {
    let first = days_from_civil(year, month0 + 1, 1);
    // 0 is Monday: 1970-01-01 was a Thursday.
    let weekday = (first + 3).rem_euclid(7);
    let offset = (weekday - i64::from(first_day - 1)).rem_euclid(7);
    first - if offset == 0 { 7 } else { offset }
}

fn int(n: impl TryInto<i32>) -> Value {
    Value::Int(n.try_into().unwrap_or_default())
}

/// The rows of a control, for its month, year and locale.
fn rows(e: &Engine, obj: ObjectId, kind: Kind) -> Option<(Vec<CalendarCell>, String)> {
    let locale = locale_of(e, obj);
    let js = date_js(e);
    let mut title = String::new();
    let mut out = Vec::with_capacity(kind.count());
    match kind {
        Kind::Days => {
            let first = locale.first_day_of_week();
            for i in 0..COLUMNS {
                let day = (usize::from(first) - 1 + i) % 7 + 1;
                let day = day as u8;
                out.push(CalendarCell {
                    index: i as i32,
                    day: int(day % 7),
                    long_name: Value::string(locale.day_name(day, NameWidth::Long)),
                    short_name: Value::string(locale.day_name(day, NameWidth::Short)),
                    narrow_name: Value::string(locale.day_name(day, NameWidth::Narrow)),
                    ..CalendarCell::default()
                });
            }
        }
        Kind::Month | Kind::Weeks => {
            let (month_p, year_p) = match kind {
                Kind::Month => (MonthGrid::MONTH.index, MonthGrid::YEAR.index),
                _ => (WeekNumberColumn::MONTH.index, WeekNumberColumn::YEAR.index),
            };
            let (month, year) = e.untrack(|| (num(e, obj, month_p), num(e, obj, year_p)));
            let (month, year) = (month as i32, year as i32);
            if !(0..=11).contains(&month) || !(-271_820..=275_759).contains(&year) {
                return None;
            }
            let start = grid_start(year, month as u32, locale.first_day_of_week());
            if kind == Kind::Month {
                title = format!(
                    "{} {year}",
                    locale.standalone_month_name(month as u8, NameWidth::Long)
                );
                let today = js.today(e);
                for i in 0..kind.count() {
                    let (y, m, d) = civil_from_days(start + i as i64);
                    out.push(CalendarCell {
                        index: i as i32,
                        date: js.make(e, y, m - 1, d),
                        day: int(d),
                        month: int(m - 1),
                        year: int(y),
                        week_number: int(iso_week_number(y, m, d)),
                        today: Value::Bool((y, m - 1, d) == today),
                        ..CalendarCell::default()
                    });
                }
            } else {
                for i in 0..kind.count() {
                    let (y, m, d) = civil_from_days(start + 7 * i as i64);
                    out.push(CalendarCell {
                        index: i as i32,
                        week_number: int(iso_week_number(y, m, d)),
                        ..CalendarCell::default()
                    });
                }
            }
        }
    }
    Some((out, title))
}

/// The roles of a cell, by name, for a delegate that has them required.
fn role_values(cell: &CalendarCell) -> Vec<(&'static str, Value)> {
    vec![
        ("index", Value::Int(cell.index)),
        ("date", cell.date.clone()),
        ("day", cell.day.clone()),
        ("month", cell.month.clone()),
        ("year", cell.year.clone()),
        ("weekNumber", cell.week_number.clone()),
        ("today", cell.today.clone()),
        ("longName", cell.long_name.clone()),
        ("shortName", cell.short_name.clone()),
        ("narrowName", cell.narrow_name.clone()),
    ]
}

/// Puts a row's roles in a cell's context object, where they changed.
fn apply(e: &Engine, ctx: ObjectId, cell: &CalendarCell) {
    let sets: [(_, &Value); 9] = [
        (CalendarCell::DATE, &cell.date),
        (CalendarCell::DAY, &cell.day),
        (CalendarCell::MONTH, &cell.month),
        (CalendarCell::YEAR, &cell.year),
        (CalendarCell::WEEK_NUMBER, &cell.week_number),
        (CalendarCell::TODAY, &cell.today),
        (CalendarCell::LONG_NAME, &cell.long_name),
        (CalendarCell::SHORT_NAME, &cell.short_name),
        (CalendarCell::NARROW_NAME, &cell.narrow_name),
    ];
    for (p, v) in sets {
        // A date is a new object each time: only a different day is a
        // change.
        if p.index == CalendarCell::DATE.index {
            let old = e.untrack(|| e.read(ctx, p.index));
            if date_js(e).parts(e, &old) == date_js(e).parts(e, v) && !old.is_nullish() {
                continue;
            }
        }
        check(e, "cell", e.set(ctx, p, v.clone()));
    }
}

/// Makes the cells again: one per row, in the content item.
fn rebuild(e: &Engine, obj: ObjectId) {
    let Some(cells) = cells_of(e, obj) else {
        return;
    };
    let kind = cells.kind.get();
    for (item, ctx) in cells.made.take() {
        if e.is_alive(item) {
            e.destroy(item);
        }
        if e.is_alive(ctx) {
            e.destroy(ctx);
        }
    }
    let (content, delegate) = e.untrack(|| {
        (
            object(e, obj, Control::CONTENT_ITEM.index),
            object(e, obj, kind.delegate()),
        )
    });
    cells.host.set(content);
    let (Some(content), Some(delegate)) = (content, delegate) else {
        return;
    };
    if cells.watched.get() != Some(content) {
        cells.watched.set(Some(content));
        for p in [Item::WIDTH.index, Item::HEIGHT.index] {
            check(
                e,
                "content",
                e.watch(content, p, move |e, _| layout(e, obj)).map(|_| ()),
            );
        }
    }
    let Some((data, _)) = rows(e, obj, kind) else {
        return;
    };
    let mut made = Vec::new();
    for cell in data {
        let mut values = role_values(&cell);
        let ctx = match e.create_native_incomplete(cell, Some(obj)) {
            Ok(c) => c,
            Err(err) => {
                check(e, "delegate", Err(err));
                return;
            }
        };
        check(
            e,
            "model",
            e.set(ctx, CalendarCell::MODEL, Value::Object(ctx)),
        );
        values.push(("model", Value::Object(ctx)));
        let item = e.create_in_context(delegate, Some(obj), &values, |e, root| {
            if let Some(scope) = e.context_of(root) {
                e.set_context_object(scope, Some(ctx));
            }
            follow_required(e, root, ctx);
            check(e, "delegate", set_parent_item(e, root, Some(content)));
        });
        match item {
            Ok(item) => made.push((item, ctx)),
            Err(err) => check(e, "delegate", Err(err)),
        }
    }
    *cells.made.borrow_mut() = made;
    layout(e, obj);
}

/// A delegate that declares a role required follows its cell's.
fn follow_required(e: &Engine, root: ObjectId, ctx: ObjectId) {
    let (Some(meta), Some(from)) = (e.meta_of(root), e.meta_of(ctx)) else {
        return;
    };
    for (i, p) in meta.properties.iter().enumerate() {
        if !p.flags.contains(PropertyFlags::REQUIRED) {
            continue;
        }
        let Some(source) = from
            .properties
            .iter()
            .position(|q| q.name.as_str() == p.name.as_str())
        else {
            continue;
        };
        let (to, source) = (PropIndex(i as u32), PropIndex(source as u32));
        check(
            e,
            p.name.as_str(),
            e.bind(root, to, move |e| e.read(ctx, source)),
        );
    }
}

/// Puts the data of the month, year and locale in the cells that are
/// there, and the title.
fn refresh(e: &Engine, obj: ObjectId) {
    let Some(cells) = cells_of(e, obj) else {
        return;
    };
    let kind = cells.kind.get();
    let Some((data, title)) = rows(e, obj, kind) else {
        return;
    };
    for (cell, (_, ctx)) in data.iter().zip(cells.made.borrow().iter()) {
        if e.is_alive(*ctx) {
            apply(e, *ctx, cell);
        }
    }
    if kind == Kind::Month {
        check(e, "title", e.set(obj, MonthGrid::TITLE, title));
    }
}

/// Spreads the cells over the content item, `spacing` apart, and tells the
/// content item what size it would like from the cells'.
fn layout(e: &Engine, obj: ObjectId) {
    let Some(cells) = cells_of(e, obj) else {
        return;
    };
    let Some(content) = cells.host.get().filter(|c| e.is_alive(*c)) else {
        return;
    };
    let kind = cells.kind.get();
    let (cols, rows) = kind.shape();
    let spacing = e.untrack(|| num(e, obj, Control::SPACING.index));
    let (w, h) = e.untrack(|| {
        (
            num(e, content, Item::WIDTH.index),
            num(e, content, Item::HEIGHT.index),
        )
    });
    let gaps_w = spacing * (cols - 1) as f64;
    let gaps_h = spacing * (rows - 1) as f64;
    let cell_w = ((w - gaps_w) / cols as f64).max(0.0);
    let cell_h = ((h - gaps_h) / rows as f64).max(0.0);
    let (mut max_w, mut max_h) = (0.0_f64, 0.0_f64);
    for (i, (item, _)) in cells.made.borrow().iter().enumerate() {
        if !e.is_alive(*item) {
            continue;
        }
        let (col, row) = (i % cols, i / cols);
        e.untrack(|| {
            max_w = max_w.max(num(e, *item, Item::IMPLICIT_WIDTH.index));
            max_h = max_h.max(num(e, *item, Item::IMPLICIT_HEIGHT.index));
        });
        for (p, v) in [
            (Item::X, col as f64 * (cell_w + spacing)),
            (Item::Y, row as f64 * (cell_h + spacing)),
            (Item::WIDTH, cell_w),
            (Item::HEIGHT, cell_h),
        ] {
            check(e, "cell", e.set(*item, p, v));
        }
    }
    for (p, v) in [
        (Item::IMPLICIT_WIDTH, cols as f64 * max_w + gaps_w),
        (Item::IMPLICIT_HEIGHT, rows as f64 * max_h + gaps_h),
    ] {
        check(e, "content", e.set(content, p, v));
    }
}

/// Watches what the cells depend on.
fn start(e: &Engine, obj: ObjectId, kind: Kind) -> Result<(), EngineError> {
    let Some(cells) = cells_of(e, obj) else {
        return Ok(());
    };
    cells.kind.set(kind);
    rebuild(e, obj);
    refresh(e, obj);
    for p in [kind.delegate(), Control::CONTENT_ITEM.index] {
        e.watch(obj, p, move |e, _| {
            rebuild(e, obj);
            refresh(e, obj);
        })?;
    }
    e.watch(obj, Control::SPACING.index, move |e, _| layout(e, obj))?;
    e.watch(obj, Control::LOCALE.index, move |e, _| refresh(e, obj))?;
    let range = match kind {
        Kind::Month => Some((MonthGrid::MONTH.index, MonthGrid::YEAR.index)),
        Kind::Weeks => Some((WeekNumberColumn::MONTH.index, WeekNumberColumn::YEAR.index)),
        Kind::Days => None,
    };
    if let Some((month, year)) = range {
        e.watch(obj, month, move |e, v| {
            let m = v.to_number();
            if !(0.0..=11.0).contains(&m) {
                warn(
                    e,
                    format!(
                        "{}: month {} is out of range [0...11]",
                        kind.name(),
                        m as i64
                    ),
                );
                return;
            }
            refresh(e, obj);
        })?;
        e.watch(obj, year, move |e, v| {
            let y = v.to_number();
            if !(-271_820.0..=275_759.0).contains(&y) {
                warn(
                    e,
                    format!(
                        "{}: year {} is out of range [-271820...275759]",
                        kind.name(),
                        y as i64
                    ),
                );
                return;
            }
            refresh(e, obj);
        })?;
    }
    Ok(())
}

/// Today's month and year, for a control that was not given any.
fn default_month_year(e: &Engine, obj: ObjectId, month: PropIndex, year: PropIndex) {
    let (y, m, _) = date_js(e).today(e);
    for (p, v) in [(month, m as i32), (year, y)] {
        if e.untrack(|| num(e, obj, p)) < -1.5e9 {
            check(e, "default", e.write_derived(obj, p, Value::Int(v)));
        }
    }
}

// -------------------------------------------------------------- MonthGrid

/// `MonthGrid`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "AbstractMonthGrid", extends = Control)]
pub struct MonthGrid {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub month: i32,
    #[qml(notify)]
    pub year: i32,
    #[qml(notify)]
    pub source: Value,
    #[qml(notify)]
    pub title: String,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) cells: Rc<Cells>,
}

/// Month and year start as the sentinel `default_month_year` replaces
/// with today's, once the control is complete.
impl Default for MonthGrid {
    fn default() -> Self {
        Self {
            control: Control::default(),
            month: i32::MIN,
            year: i32::MIN,
            source: Value::Undefined,
            title: String::new(),
            delegate: None,
            cells: Rc::default(),
        }
    }
}

#[qml_impl]
impl MonthGrid {
    #[qml(signal)]
    fn clicked(date: Value);

    #[qml(signal)]
    fn pressed(date: Value);

    #[qml(signal)]
    fn released(date: Value);

    #[qml(signal, name = "pressAndHold")]
    fn press_and_hold(date: Value);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        default_month_year(e, obj, MonthGrid::MONTH.index, MonthGrid::YEAR.index);
        start(e, obj, Kind::Month)?;
        for p in [Item::ENABLED.index, Item::VISIBLE.index] {
            e.watch(obj, p, move |e, v| {
                if !v.to_boolean() {
                    cancel(e, obj);
                }
            })?;
        }
        Ok(())
    }
}

// ----------------------------------------------------------- DayOfWeekRow

/// `DayOfWeekRow`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "AbstractDayOfWeekRow", extends = Control)]
pub struct DayOfWeekRow {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub source: Value,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) cells: Rc<Cells>,
}

#[qml_impl]
impl DayOfWeekRow {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        start(e, obj, Kind::Days)
    }
}

// ------------------------------------------------------- WeekNumberColumn

/// `WeekNumberColumn`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "AbstractWeekNumberColumn", extends = Control)]
pub struct WeekNumberColumn {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub month: i32,
    #[qml(notify)]
    pub year: i32,
    #[qml(notify)]
    pub source: Value,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) cells: Rc<Cells>,
}

/// Month and year start as the sentinel `default_month_year` replaces
/// with today's, once the control is complete.
impl Default for WeekNumberColumn {
    fn default() -> Self {
        Self {
            control: Control::default(),
            month: i32::MIN,
            year: i32::MIN,
            source: Value::Undefined,
            delegate: None,
            cells: Rc::default(),
        }
    }
}

#[qml_impl]
impl WeekNumberColumn {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        default_month_year(
            e,
            obj,
            WeekNumberColumn::MONTH.index,
            WeekNumberColumn::YEAR.index,
        );
        start(e, obj, Kind::Weeks)
    }
}

// ---------------------------------------------------------------- pointer

/// The cell under `at`, in the month grid's coordinates; none in the
/// space between cells.
fn hit(e: &Engine, obj: ObjectId, at: kurbo::Point) -> Option<usize> {
    let cells = cells_of(e, obj)?;
    let content = cells.host.get().filter(|c| e.is_alive(*c))?;
    let (cols, rows) = cells.kind.get().shape();
    e.untrack(|| {
        let spacing = num(e, obj, Control::SPACING.index);
        let (x, y) = (
            at.x - num(e, content, Item::X.index),
            at.y - num(e, content, Item::Y.index),
        );
        let (w, h) = (
            num(e, content, Item::WIDTH.index),
            num(e, content, Item::HEIGHT.index),
        );
        let cell_w = (w - spacing * (cols - 1) as f64) / cols as f64;
        let cell_h = (h - spacing * (rows - 1) as f64) / rows as f64;
        if x < 0.0 || y < 0.0 || cell_w <= 0.0 || cell_h <= 0.0 {
            return None;
        }
        let (col, row) = (
            (x / (cell_w + spacing)).floor() as usize,
            (y / (cell_h + spacing)).floor() as usize,
        );
        let inside = x - col as f64 * (cell_w + spacing) <= cell_w
            && y - row as f64 * (cell_h + spacing) <= cell_h;
        (col < cols && row < rows && inside).then_some(row * cols + col)
    })
}

fn emit(e: &Engine, obj: ObjectId, cells: &Cells, signal: &str, cell: usize) {
    let date = cells
        .made
        .borrow()
        .get(cell)
        .map(|(_, ctx)| e.untrack(|| e.read(*ctx, CalendarCell::DATE.index)))
        .unwrap_or_default();
    if e.is_alive(obj) {
        check(e, signal, e.emit_by_name(obj, signal, &[date]));
    }
}

fn press_cell(e: &Engine, obj: ObjectId, cells: &Cells, cell: Option<usize>) {
    cells.current.set(cell);
    ticker::stop(e, &cells.timer);
    if let Some(c) = cell {
        emit(e, obj, cells, "pressed", c);
        ticker::start(
            e,
            obj,
            &cells.timer,
            ljos_quick::press_and_hold_interval(e),
            held,
        );
    }
}

fn held(e: &Engine, obj: ObjectId) {
    let Some(cells) = cells_of(e, obj) else {
        return;
    };
    if let (true, Some(c)) = (cells.held.get(), cells.current.get()) {
        emit(e, obj, &cells, "pressAndHold", c);
    }
}

fn press(e: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if !takes_input(e, obj) || !is_left(ev) {
        return false;
    }
    let Some(cells) = cells_of(e, obj) else {
        return false;
    };
    let Some(cell) = hit(e, obj, at) else {
        return false;
    };
    cells.held.set(true);
    flickable::keep_pointer_grab(e, obj, true);
    press_cell(e, obj, &cells, Some(cell));
    true
}

fn moved(e: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(cells) = cells_of(e, obj) else {
        return;
    };
    if !cells.held.get() {
        return;
    }
    let now = hit(e, obj, at);
    if now == cells.current.get() {
        return;
    }
    if let Some(old) = cells.current.get() {
        emit(e, obj, &cells, "released", old);
    }
    press_cell(e, obj, &cells, now);
}

fn release(e: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(cells) = cells_of(e, obj) else {
        return;
    };
    if !cells.held.replace(false) {
        return;
    }
    flickable::keep_pointer_grab(e, obj, false);
    ticker::stop(e, &cells.timer);
    let _ = at;
    if let Some(c) = cells.current.take() {
        emit(e, obj, &cells, "released", c);
        emit(e, obj, &cells, "clicked", c);
    }
}

fn cancel(e: &Engine, obj: ObjectId) {
    if let Some(cells) = cells_of(e, obj)
        && cells.held.replace(false)
    {
        flickable::keep_pointer_grab(e, obj, false);
        ticker::stop(e, &cells.timer);
        cells.current.set(None);
    }
}

// ---------------------------------------------------------- CalendarModel

/// The months of a `CalendarModel`: from the one `from` is in to the one
/// `to` is in. Shared with the model the views read.
#[derive(Default)]
struct Months {
    /// Year times twelve plus month, of `from` and of `to`.
    from: Cell<i64>,
    to: Cell<i64>,
    listeners: ModelListeners,
}

impl Months {
    fn count(&self) -> usize {
        usize::try_from(self.to.get() - self.from.get() + 1).unwrap_or(0)
    }
}

impl ItemModel for Months {
    fn row_count(&self) -> usize {
        self.count()
    }

    fn role_names(&self) -> Vec<Atom> {
        ["month", "year", "index"]
            .into_iter()
            .map(Atom::from)
            .collect()
    }

    fn data(&self, _engine: &Engine, row: usize, role: &str) -> Value {
        if row >= self.count() {
            return Value::Undefined;
        }
        let n = self.from.get() + row as i64;
        match role {
            "month" => Value::Int(n.rem_euclid(12) as i32),
            "year" => Value::Int(n.div_euclid(12) as i32),
            "index" => Value::Int(row as i32),
            _ => Value::Undefined,
        }
    }

    fn subscribe(&self, listener: ljos_engine::ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }
}

/// `CalendarModel`: a row for each month from `from` to `to`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Controls", name = "CalendarModel")]
pub struct CalendarModel {
    #[qml(notify)]
    pub from: Value,
    #[qml(notify)]
    pub to: Value,
    #[qml(readonly, notify)]
    pub count: i32,
    #[qml(skip)]
    months: Rc<MonthsCell>,
}

/// [`Months`] with the `Debug` the object needs.
#[derive(Default)]
struct MonthsCell(Rc<Months>);

impl std::fmt::Debug for MonthsCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Months").finish_non_exhaustive()
    }
}

fn months_of(e: &Engine, obj: ObjectId) -> Option<Rc<Months>> {
    e.with::<CalendarModel, _>(obj, |m| m.months.0.clone()).ok()
}

/// Reads `from` and `to` again, and tells the views when the rows
/// changed.
fn months_changed(e: &Engine, obj: ObjectId) {
    let Some(months) = months_of(e, obj) else {
        return;
    };
    let js = date_js(e);
    let month_index = |p: PropIndex| {
        let v = e.untrack(|| e.read(obj, p));
        js.parts(e, &v)
            .map(|(y, m, _)| i64::from(y) * 12 + i64::from(m))
    };
    let (Some(from), Some(to)) = (
        month_index(CalendarModel::FROM.index),
        month_index(CalendarModel::TO.index),
    ) else {
        return;
    };
    if (from, to) == (months.from.get(), months.to.get()) {
        return;
    }
    months.from.set(from);
    months.to.set(to);
    months.listeners.emit(e, &ModelChange::ModelReset);
    let count = months.count().min(i32::MAX as usize) as i32;
    check(e, "count", e.set_internal(obj, CalendarModel::COUNT, count));
}

#[qml_impl]
impl CalendarModel {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        let js = date_js(e);
        for (p, default) in [
            (CalendarModel::FROM, (1, 0, 1)),
            (CalendarModel::TO, (275_759, 8, 25)),
        ] {
            if e.untrack(|| e.read(obj, p.index)).is_nullish() {
                let v = js.make(e, default.0, default.1, default.2);
                check(e, "range", e.write_derived(obj, p.index, v));
            }
        }
        if let Some(months) = months_of(e, obj) {
            let rows: Rc<dyn ItemModel> = months;
            e.set_object_model(obj, Some(rows))?;
        }
        months_changed(e, obj);
        for p in [CalendarModel::FROM.index, CalendarModel::TO.index] {
            e.watch(obj, p, move |e, _| months_changed(e, obj))?;
        }
        Ok(())
    }

    /// The month (0 to 11) of a row.
    #[qml(invokable, name = "monthAt")]
    fn month_at(cx: &Cx, row: i32) -> i32 {
        months_of(cx.engine, cx.this)
            .map_or(0, |m| (m.from.get() + i64::from(row)).rem_euclid(12) as i32)
    }

    /// The year of a row.
    #[qml(invokable, name = "yearAt")]
    fn year_at(cx: &Cx, row: i32) -> i32 {
        months_of(cx.engine, cx.this)
            .map_or(0, |m| (m.from.get() + i64::from(row)).div_euclid(12) as i32)
    }

    /// The row of a month: given a `Date`, or a year and a month. -1 for
    /// one before the first row.
    #[qml(invokable, name = "indexOf")]
    fn index_of(cx: &Cx, first: Value, month: Value) -> i32 {
        let e = cx.engine;
        let Some(months) = months_of(e, cx.this) else {
            return -1;
        };
        let n = if month.is_nullish() {
            date_js(e)
                .parts(e, &first)
                .map(|(y, m, _)| i64::from(y) * 12 + i64::from(m))
        } else {
            let (y, m) = (first.to_number(), month.to_number());
            (y.is_finite() && m.is_finite()).then(|| y as i64 * 12 + m as i64)
        };
        match n {
            Some(n) if n >= months.from.get() => {
                (n - months.from.get()).min(i64::from(i32::MAX)) as i32
            }
            _ => -1,
        }
    }
}

pub(crate) fn register_all(engine: &Engine) {
    for meta in [
        MonthGrid::static_meta(),
        DayOfWeekRow::static_meta(),
        WeekNumberColumn::static_meta(),
        CalendarModel::static_meta(),
    ] {
        engine.register_type(
            "QtQuick.Templates",
            ljos_engine::Version::new(2, 15),
            MetaRef::Static(meta),
        );
    }
    register_pointer_item(
        engine,
        MetaRef::Static(MonthGrid::static_meta()),
        PointerItem {
            press,
            moved,
            release,
            cancel,
            hover,
        },
    );
}
