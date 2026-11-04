//! `QtQuick.Pdf`: a document, the images of its pages, and a history of
//! where in it the reader has been. The viewers built on these are QML, in
//! `ljos_controls`, whose controls they scroll with.
//!
//! A document is opened on the spot, as Qt does, and what can be asked of it
//! is kept; the pages are read again by whoever draws them.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use hayro::hayro_syntax::object::DateTime;
use hayro::hayro_syntax::{DecryptionError, LoadPdfError, Pdf};
use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, Point, PropIndex, QmlObject, QtObject, Size,
    Url, Value, qml_impl,
};
use ljos_platform::Platform;

use crate::enums::{EnumTable, keys};
use crate::image::{Image, ImageBase};
use crate::item::write_computed;

mod models;
mod selection;
mod structure;
mod text;
mod values;

pub use models::{PdfBookmarkModel, PdfLinkModel, PdfSearchModel};
pub use selection::PdfSelection;

use structure::{Entry, LabelRange, Link};
use text::PageText;

const CATEGORY: &str = "qml";

/// `PdfDocument.Status`.
mod status {
    pub const NULL: i32 = 0;
    pub const LOADING: i32 = 1;
    pub const UNLOADING: i32 = 3;
    pub const READY: i32 = 2;
    pub const ERROR: i32 = 4;
}

fn document_enums() -> EnumTable {
    vec![(
        "Status",
        keys(&[
            ("Null", status::NULL),
            ("Loading", status::LOADING),
            ("Ready", status::READY),
            ("Unloading", status::UNLOADING),
            ("Error", status::ERROR),
        ]),
    )]
}

/// Qt's words for what happened to the last opening.
const NO_ERROR: &str = "no error";

/// What is open, for every object that reads it: the document's own text,
/// links and outline are read from it the first time they are asked for,
/// on whichever thread asks.
pub(crate) struct DocData {
    pdf: Pdf,
    labels: Vec<LabelRange>,
    texts: Vec<OnceLock<Arc<PageText>>>,
    links: Vec<OnceLock<Arc<Vec<Link>>>>,
    outline: OnceLock<Arc<Vec<Entry>>>,
}

impl std::fmt::Debug for DocData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocData")
            .field("pages", &self.texts.len())
            .finish_non_exhaustive()
    }
}

// A search reads the text of every page off the engine's thread.
const _: fn() = || {
    fn shared<T: Send + Sync>() {}
    shared::<DocData>();
};

impl DocData {
    fn new(pdf: Pdf) -> Self {
        let pages = pdf.pages().len();
        Self {
            labels: structure::labels(&pdf),
            texts: (0..pages).map(|_| OnceLock::new()).collect(),
            links: (0..pages).map(|_| OnceLock::new()).collect(),
            outline: OnceLock::new(),
            pdf,
        }
    }

    pub(crate) fn page_count(&self) -> usize {
        self.texts.len()
    }

    /// The text of page `page`; `None` for a page there is not.
    pub(crate) fn text(&self, page: i32) -> Option<Arc<PageText>> {
        let i = usize::try_from(page).ok()?;
        let slot = self.texts.get(i)?;
        Some(Arc::clone(slot.get_or_init(|| {
            Arc::new(PageText::of(&self.pdf.pages()[i]))
        })))
    }

    pub(crate) fn links(&self, page: i32) -> Arc<Vec<Link>> {
        let Some((i, slot)) = usize::try_from(page)
            .ok()
            .and_then(|i| Some((i, self.links.get(i)?)))
        else {
            return Arc::default();
        };
        Arc::clone(slot.get_or_init(|| Arc::new(structure::links(&self.pdf, i))))
    }

    pub(crate) fn outline(&self) -> Arc<Vec<Entry>> {
        Arc::clone(
            self.outline
                .get_or_init(|| Arc::new(structure::outline(&self.pdf))),
        )
    }

    /// What page `page` is called: its number past the last page.
    pub(crate) fn label(&self, page: usize) -> String {
        if page < self.page_count() {
            structure::label(&self.labels, page)
        } else {
            (page + 1).to_string()
        }
    }
}

/// What the document `doc` has open: `None` while it has nothing. A
/// binding asking this asks again when the document opens something else.
pub(crate) fn data_of(e: &Engine, doc: ObjectId) -> Option<Arc<DocData>> {
    let _ = e.read(doc, PdfDocument::STATUS.index);
    e.with(doc, |d: &PdfDocument| d.data.clone()).ok().flatten()
}

/// The document an object's `document` property names.
pub(crate) fn document_of(e: &Engine, me: ObjectId, p: PropIndex) -> Option<ObjectId> {
    match e.untrack(|| e.read(me, p)) {
        Value::Object(d) if e.with(d, |_: &PdfDocument| ()).is_ok() => Some(d),
        _ => None,
    }
}

/// `PdfDocument`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Pdf",
    name = "PdfDocument",
    extends = QtObject,
    enums = document_enums
)]
pub struct PdfDocument {
    #[qml(base)]
    pub base: QtObject,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify)]
    pub password: String,
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    #[qml(notify, readonly)]
    pub error: String,
    #[qml(notify, readonly)]
    pub page_count: i32,
    #[qml(notify, readonly)]
    pub max_page_width: f64,
    #[qml(notify, readonly)]
    pub max_page_height: f64,
    #[qml(notify, readonly)]
    pub title: String,
    #[qml(notify, readonly)]
    pub subject: String,
    #[qml(notify, readonly)]
    pub author: String,
    #[qml(notify, readonly)]
    pub keywords: String,
    #[qml(notify, readonly)]
    pub producer: String,
    #[qml(notify, readonly)]
    pub creator: String,
    /// A script's `Date`, an invalid one when the document gives none.
    #[qml(notify, readonly)]
    pub creation_date: Value,
    #[qml(notify, readonly)]
    pub modification_date: Value,
    /// A model of a row for each page, of its label and size.
    #[qml(notify, readonly)]
    pub page_model: Option<ObjectId>,
    /// Width and height of each page in points.
    #[qml(skip)]
    pages: Vec<(f64, f64)>,
    #[qml(skip)]
    data: Option<Arc<DocData>>,
    #[qml(skip)]
    page_rows: Rc<RefCell<PageRows>>,
}

/// The rows of `pageModel`.
#[derive(Debug, Default)]
struct PageRows {
    rows: Vec<(String, (f64, f64))>,
    signals: Option<ljos_engine::ModelSignals>,
}

struct PageModel(Rc<RefCell<PageRows>>);

impl ljos_engine::QmlListModel for PageModel {
    fn row_count(&self) -> usize {
        self.0.borrow().rows.len()
    }
    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::from("label"), Atom::from("pointSize")]
    }
    fn data(&self, row: usize, role: &str) -> Value {
        let rows = self.0.borrow();
        let Some((label, (width, height))) = rows.rows.get(row) else {
            return Value::Undefined;
        };
        match role {
            "label" => text(label),
            "pointSize" => Value::Size(Arc::new(Size {
                width: *width,
                height: *height,
            })),
            _ => Value::Undefined,
        }
    }
}

impl Default for PdfDocument {
    fn default() -> Self {
        Self {
            base: QtObject::default(),
            source: Url::default(),
            password: String::new(),
            status: status::NULL,
            error: NO_ERROR.to_owned(),
            page_count: 0,
            max_page_width: 0.0,
            max_page_height: 0.0,
            title: String::new(),
            subject: String::new(),
            author: String::new(),
            keywords: String::new(),
            producer: String::new(),
            creator: String::new(),
            creation_date: Value::Undefined,
            modification_date: Value::Undefined,
            page_model: None,
            pages: Vec::new(),
            data: None,
            page_rows: Rc::default(),
        }
    }
}

#[qml_impl]
impl PdfDocument {
    /// Emitted each time the document will not open without another
    /// password; setting `password` tries again.
    #[qml(signal)]
    fn password_required();

    #[qml(invokable, name = "pagePointSize")]
    fn page_point_size(cx: &Cx, page: i32) -> Value {
        // A binding asking this asks again once the document is open, which
        // happens when the document's creation is complete: after it.
        let _ = cx.engine.read(cx.this, PdfDocument::STATUS.index);
        let size = cx
            .engine
            .with_mut(cx.this, |d: &mut PdfDocument| {
                usize::try_from(page)
                    .ok()
                    .and_then(|i| d.pages.get(i).copied())
            })
            .ok()
            .flatten()
            .unwrap_or((-1.0, -1.0));
        Value::Size(Arc::new(Size {
            width: size.0,
            height: size.1,
        }))
    }

    /// What the document calls page `page`: its label, or its number.
    #[qml(invokable, name = "pageLabel")]
    fn page_label(cx: &Cx, page: i32) -> String {
        let Some(data) = data_of(cx.engine, cx.this) else {
            return String::new();
        };
        usize::try_from(page).map_or_else(|_| String::new(), |p| data.label(p))
    }

    /// The page the document calls `label`; -1 when it calls none so.
    #[qml(invokable, name = "pageIndexForLabel")]
    fn page_index_for_label(cx: &Cx, label: String) -> i32 {
        data_of(cx.engine, cx.this)
            .and_then(|d| (0..d.page_count()).position(|p| d.label(p) == label))
            .and_then(|p| i32::try_from(p).ok())
            .unwrap_or(-1)
    }

    /// The text of `page` between two points in points from its top left.
    #[qml(invokable, name = "getSelection")]
    fn get_selection(cx: &Cx, page: i32, start: Point, end: Point) -> Value {
        values::remember_platform(cx.engine);
        let Some(t) = data_of(cx.engine, cx.this).and_then(|d| d.text(page)) else {
            return values::selection_value(None, "", &[]);
        };
        let p = |p: Point| hayro::kurbo::Point::new(p.x, p.y);
        selected(&t, t.between(p(start), p(end)))
    }

    /// Up to `max_length` characters of `page` from the `start`th.
    #[qml(invokable, name = "getSelectionAtIndex")]
    fn get_selection_at_index(cx: &Cx, page: i32, start: i32, max_length: i32) -> Value {
        values::remember_platform(cx.engine);
        let Some(t) = data_of(cx.engine, cx.this).and_then(|d| d.text(page)) else {
            return values::selection_value(None, "", &[]);
        };
        let start = usize::try_from(start).unwrap_or(0).min(t.len());
        let end = start
            .saturating_add(usize::try_from(max_length).unwrap_or(0))
            .min(t.len());
        selected(&t, (end > start).then_some((start, end)))
    }

    /// All the text of `page`.
    #[qml(invokable, name = "getAllText")]
    fn get_all_text(cx: &Cx, page: i32) -> Value {
        values::remember_platform(cx.engine);
        let Some(t) = data_of(cx.engine, cx.this).and_then(|d| d.text(page)) else {
            return values::selection_value(None, "", &[]);
        };
        selected(&t, Some((0, t.len())))
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let holder = e.create_native(QtObject::default(), Some(me))?;
        let rows = e.with(me, |d: &PdfDocument| Rc::clone(&d.page_rows))?;
        let signals = e.set_list_model(holder, Rc::new(PageModel(Rc::clone(&rows))))?;
        rows.borrow_mut().signals = Some(signals);
        set(e, me, PdfDocument::PAGE_MODEL.index, Value::Object(holder));
        let none = date(e, f64::NAN);
        set(e, me, PdfDocument::CREATION_DATE.index, none.clone());
        set(e, me, PdfDocument::MODIFICATION_DATE.index, none);
        // Not before: the password declared after `source` is part of the
        // first opening.
        let opened = |e: &Engine, me: ObjectId| {
            if !e
                .untrack(|| e.read(me, PdfDocument::SOURCE.index).to_js_string())
                .is_empty()
            {
                open(e, me);
            }
        };
        opened(e, me);
        e.watch(me, PdfDocument::SOURCE.index, move |e, _| {
            let source = e.untrack(|| e.read(me, PdfDocument::SOURCE.index).to_js_string());
            if source.is_empty() {
                // What was open stays open.
                warn(e, me, "Cannot open: ");
            } else {
                open(e, me);
            }
        })?;
        e.watch(me, PdfDocument::PASSWORD.index, move |e, _| opened(e, me))?;
        Ok(())
    }
}

fn warn(e: &Engine, me: ObjectId, message: &str) {
    let at = e.declared_at(me).unwrap_or_default();
    e.log(
        LogLevel::Warning,
        CATEGORY,
        format!("{at} QML PdfDocument: {message}"),
    );
}

fn set(e: &Engine, me: ObjectId, p: PropIndex, v: Value) {
    if e.is_alive(me)
        && let Err(err) = write_computed(e, me, p, v)
    {
        e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
    }
}

fn text(s: &str) -> Value {
    Value::String(Atom::from(s))
}

/// A script's `Date` at `ms` after the epoch.
fn date(e: &Engine, ms: f64) -> Value {
    #[derive(Default)]
    struct Make(std::cell::OnceCell<Option<Value>>);
    let slot = e.extension::<Make>();
    let f = slot
        .0
        .get_or_init(|| e.eval_js("(function(t){return new Date(t)})").ok());
    f.as_ref()
        .and_then(|f| e.call_js(f, &[Value::Real(ms)]))
        .unwrap_or_default()
}

/// Milliseconds after the epoch of a date in a document, which says its
/// offset from UTC.
fn date_ms(d: &DateTime) -> f64 {
    // Days from the civil calendar (Howard Hinnant's algorithm).
    let (y, m, day) = (
        i64::from(d.year),
        i64::from(d.month.max(1)),
        i64::from(d.day.max(1)),
    );
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let offset_hour = i64::from(d.utc_offset_hour);
    let offset_minute = i64::from(d.utc_offset_minute) * if offset_hour < 0 { -1 } else { 1 };
    let seconds =
        days * 86_400 + i64::from(d.hour) * 3600 + i64::from(d.minute) * 60 + i64::from(d.second)
            - offset_hour * 3600
            - offset_minute * 60;
    (seconds * 1000) as f64
}

/// A string of the document's information: UTF-16 or UTF-8 behind its byte
/// order mark, otherwise bytes that are their own code points for everything
/// a title has in practice.
fn info_text(bytes: Option<&Vec<u8>>) -> String {
    let Some(bytes) = bytes else {
        return String::new();
    };
    if let Some(utf16) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let units = utf16
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c));
        return char::decode_utf16(units)
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();
    }
    if let Some(utf8) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(utf8).into_owned();
    }
    bytes.iter().map(|b| char::from(*b)).collect()
}

/// The bytes of a document: only local files and resources, as Qt's document
/// says of itself.
fn read(e: &Engine, url: &str) -> Option<Vec<u8>> {
    let platform = Platform::of(e);
    if let Some(bytes) = platform.resource(url) {
        return Some(bytes.to_vec());
    }
    // A sandboxed run keeps every file under its root.
    std::fs::read(platform.file_of(url)?).ok()
}

fn open(e: &Engine, me: ObjectId) {
    let source = e.untrack(|| e.read(me, PdfDocument::SOURCE.index).to_js_string());
    let password = e.untrack(|| e.read(me, PdfDocument::PASSWORD.index).to_js_string());
    let url = e.resolve_url(me, &source);
    // What is open goes first, and whoever watches the status sees it go.
    let was_open = e.untrack(|| e.read(me, PdfDocument::STATUS.index)) == Value::Int(status::READY);
    if was_open {
        set(
            e,
            me,
            PdfDocument::STATUS.index,
            Value::Int(status::UNLOADING),
        );
        show(e, me, None, NO_ERROR, status::NULL);
    }
    set(
        e,
        me,
        PdfDocument::STATUS.index,
        Value::Int(status::LOADING),
    );
    let opened = match read(e, &url) {
        None => Err("file not found"),
        Some(bytes) => Pdf::new_with_password(bytes, &password).map_err(|err| match err {
            LoadPdfError::Decryption(DecryptionError::PasswordProtected) => "incorrect password",
            LoadPdfError::Decryption(_) => "unsupported security scheme",
            LoadPdfError::Invalid => "invalid file format",
        }),
    };
    match opened {
        Ok(pdf) => {
            crate::image::remember_pdf_password(&url, &password);
            show(e, me, Some(pdf), NO_ERROR, status::READY);
        }
        Err(why) => {
            show(e, me, None, why, status::ERROR);
            if why == "incorrect password" {
                let _ = e.emit_by_name(me, "passwordRequired", &[]);
            }
        }
    }
}

/// A selection of `t`'s text.
fn selected(t: &PageText, found: Option<(usize, usize)>) -> Value {
    let (start, end) = found.unwrap_or((0, 0));
    values::selection_value(
        found,
        &t.text(start, end),
        &t.lines(start, end)
            .into_iter()
            .map(text::whole)
            .collect::<Vec<_>>(),
    )
}

/// Everything that can be asked of what is open, at once, for whoever is
/// told of the change.
fn show(e: &Engine, me: ObjectId, pdf: Option<Pdf>, error: &str, now: i32) {
    let pages: Vec<(f64, f64)> = pdf
        .as_ref()
        .map(|pdf| {
            pdf.pages()
                .iter()
                .map(|page| {
                    let (w, h) = page.render_dimensions();
                    (f64::from(w), f64::from(h))
                })
                .collect()
        })
        .unwrap_or_default();
    let widest = pages.iter().fold(0.0_f64, |m, p| m.max(p.0));
    let tallest = pages.iter().fold(0.0_f64, |m, p| m.max(p.1));
    let count = i32::try_from(pages.len()).unwrap_or(i32::MAX);
    let info = pdf.as_ref().map(Pdf::metadata).cloned();
    let info = info.as_ref();
    let data = pdf.map(|pdf| Arc::new(DocData::new(pdf)));
    let rows: Vec<(String, (f64, f64))> = pages
        .iter()
        .enumerate()
        .map(|(i, size)| {
            (
                data.as_ref().map_or_else(String::new, |d| d.label(i)),
                *size,
            )
        })
        .collect();
    let Ok(page_rows) = e.with_mut(me, |d: &mut PdfDocument| {
        d.pages = pages;
        d.data = data;
        Rc::clone(&d.page_rows)
    }) else {
        return;
    };
    let signals = {
        let mut r = page_rows.borrow_mut();
        r.rows = rows;
        r.signals.clone()
    };
    if let Some(signals) = signals {
        signals.model_reset(e);
    }
    let word = |pick: fn(&hayro::hayro_syntax::metadata::Metadata) -> Option<&Vec<u8>>| {
        text(&info_text(info.and_then(pick)))
    };
    let when = |d: Option<&DateTime>| date(e, d.map_or(f64::NAN, date_ms));
    use PdfDocument as D;
    e.batch(|| {
        set(e, me, D::ERROR.index, text(error));
        set(e, me, D::PAGE_COUNT.index, Value::Int(count));
        set(e, me, D::MAX_PAGE_WIDTH.index, Value::Real(widest));
        set(e, me, D::MAX_PAGE_HEIGHT.index, Value::Real(tallest));
        set(e, me, D::TITLE.index, word(|m| m.title.as_ref()));
        set(e, me, D::SUBJECT.index, word(|m| m.subject.as_ref()));
        set(e, me, D::AUTHOR.index, word(|m| m.author.as_ref()));
        set(e, me, D::KEYWORDS.index, word(|m| m.keywords.as_ref()));
        set(e, me, D::PRODUCER.index, word(|m| m.producer.as_ref()));
        set(e, me, D::CREATOR.index, word(|m| m.creator.as_ref()));
        let created = when(info.and_then(|m| m.creation_date.as_ref()));
        let modified = when(info.and_then(|m| m.modification_date.as_ref()));
        set(e, me, D::CREATION_DATE.index, created);
        set(e, me, D::MODIFICATION_DATE.index, modified);
        set(e, me, D::STATUS.index, Value::Int(now));
    });
}

/// `PdfPageImage`: an `Image` of a page of a document; `currentFrame` is the
/// page.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Pdf", name = "PdfPageImage", extends = Image)]
pub struct PdfPageImage {
    #[qml(base)]
    pub image: Image,
    #[qml(notify)]
    pub document: Option<ObjectId>,
}

#[qml_impl]
impl PdfPageImage {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        follow(e, me);
        e.watch(me, PdfPageImage::DOCUMENT.index, move |e, _| follow(e, me))?;
        Ok(())
    }
}

/// Shows what the image's document has open, each time it opens something:
/// the source alone would reach the image before the password does.
fn follow(e: &Engine, me: ObjectId) {
    let Value::Object(document) = e.untrack(|| e.read(me, PdfPageImage::DOCUMENT.index)) else {
        return;
    };
    let show = move |e: &Engine| {
        if !e.is_alive(me) {
            return;
        }
        let ready = e.untrack(|| e.read(document, PdfDocument::STATUS.index));
        if matches!(ready, Value::Int(status::READY)) {
            let source = e.untrack(|| e.read(document, PdfDocument::SOURCE.index));
            let url = e.resolve_url(document, &source.to_js_string());
            set(
                e,
                me,
                ImageBase::SOURCE.index,
                Value::Url(Atom::from(url.as_str())),
            );
        }
    };
    show(e);
    if let Err(err) = e.watch(document, PdfDocument::STATUS.index, move |e, _| show(e)) {
        e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
    }
}

/// Where a reader was.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Destination {
    page: i32,
    location: Point,
    zoom: f64,
}

/// `PdfPageNavigator`: back and forward through where the reader jumped to.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Pdf", name = "PdfPageNavigator", extends = QtObject)]
pub struct PdfPageNavigator {
    #[qml(base)]
    pub base: QtObject,
    #[qml(notify, readonly)]
    pub current_page: i32,
    #[qml(notify, readonly)]
    pub current_location: Point,
    #[qml(notify, readonly)]
    pub current_zoom: f64,
    #[qml(notify, readonly)]
    pub back_available: bool,
    #[qml(notify, readonly)]
    pub forward_available: bool,
    #[qml(skip)]
    history: Vec<Destination>,
    #[qml(skip)]
    at: usize,
}

const START: Destination = Destination {
    page: 0,
    location: Point { x: 0.0, y: 0.0 },
    zoom: 1.0,
};

impl Default for PdfPageNavigator {
    fn default() -> Self {
        Self {
            base: QtObject::default(),
            current_page: START.page,
            current_location: START.location,
            current_zoom: START.zoom,
            back_available: false,
            forward_available: false,
            history: vec![START],
            at: 0,
        }
    }
}

#[qml_impl]
impl PdfPageNavigator {
    /// An abrupt move, from `jump`, `back` or `forward`: `current` is a
    /// `pdfLink` to where the reader is now.
    #[qml(signal)]
    fn jumped(current: Value);

    /// Forgets where the reader has been and goes back to the start.
    #[qml(invokable, name = "clear")]
    fn clear(cx: &Cx) {
        moved(cx, false, |n| {
            n.history = vec![START];
            n.at = 0;
        });
    }

    /// Adds a destination, a `pdfLink` or a page, a location and a zoom;
    /// what could be gone forward to is forgotten.
    #[qml(invokable, name = "jump")]
    fn jump(cx: &Cx, to: Value, location: Value, zoom: Value) {
        let there = match values::link_place(&to) {
            Some((page, location, zoom)) => Destination {
                page,
                location,
                zoom,
            },
            None if matches!(to, Value::Custom(_)) => return,
            None => Destination {
                page: to.to_number() as i32,
                location: match location {
                    Value::Point(p) => *p,
                    _ => Point { x: 0.0, y: 0.0 },
                },
                zoom: zoom.to_number(),
            },
        };
        // Going where the reader already is goes nowhere: nothing to go back
        // to, nothing said.
        let same = cx
            .engine
            .with_mut(cx.this, |n: &mut PdfPageNavigator| n.history[n.at] == there);
        if same.unwrap_or(true) {
            return;
        }
        moved(cx, true, |n| {
            n.history.truncate(n.at + 1);
            n.history.push(there);
            n.at += 1;
        });
    }

    /// Changes where the reader is now, as scrolling does, adding nothing to
    /// go back to.
    #[qml(invokable, name = "update")]
    fn update(cx: &Cx, page: i32, location: Point, zoom: f64) {
        moved(cx, false, |n| {
            n.history[n.at] = Destination {
                page,
                location,
                zoom,
            };
        });
    }

    #[qml(invokable, name = "back")]
    fn back(cx: &Cx) {
        let can = cx
            .engine
            .with_mut(cx.this, |n: &mut PdfPageNavigator| n.at > 0);
        if can.unwrap_or(false) {
            moved(cx, true, |n| n.at -= 1);
        }
    }

    #[qml(invokable, name = "forward")]
    fn forward(cx: &Cx) {
        let can = cx.engine.with_mut(cx.this, |n: &mut PdfPageNavigator| {
            n.at + 1 < n.history.len()
        });
        if can.unwrap_or(false) {
            moved(cx, true, |n| n.at += 1);
        }
    }
}

fn moved(cx: &Cx, jumped: bool, change: impl FnOnce(&mut PdfPageNavigator)) {
    use PdfPageNavigator as N;
    let (e, me) = (cx.engine, cx.this);
    let Ok((now, back, forward)) = e.with_mut(me, |n: &mut N| {
        change(n);
        (n.history[n.at], n.at > 0, n.at + 1 < n.history.len())
    }) else {
        return;
    };
    e.batch(|| {
        set(e, me, N::CURRENT_PAGE.index, Value::Int(now.page));
        set(
            e,
            me,
            N::CURRENT_LOCATION.index,
            Value::Point(Arc::new(now.location)),
        );
        set(e, me, N::CURRENT_ZOOM.index, Value::Real(now.zoom));
        set(e, me, N::BACK_AVAILABLE.index, Value::Bool(back));
        set(e, me, N::FORWARD_AVAILABLE.index, Value::Bool(forward));
    });
    if jumped {
        values::remember_platform(e);
        let current = values::place_value(structure::Place {
            page: now.page,
            location: hayro::kurbo::Point::new(now.location.x, now.location.y),
            zoom: now.zoom,
        });
        let _ = e.emit_by_name(me, "jumped", &[current]);
    }
}

/// Registers the types of `QtQuick.Pdf` written in Rust.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick.Pdf" 5.0 => [
        PdfDocument, PdfPageImage, PdfPageNavigator, PdfSelection, PdfSearchModel,
        PdfLinkModel, PdfBookmarkModel,
    ]));
    engine.register_value_type(&values::LINK);
    engine.register_value_type(&values::SELECTION);
}
