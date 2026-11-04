//! The models of `QtQuick.Pdf`: what a search found, the links of a page and
//! the outline of a document.
//!
//! Measured on qml6 (features/pdf/pdf-search.feature, pdf-links.feature):
//! - A search runs after the change that asked for it, never during it, and
//!   its rows replace the last search's all at once.
//! - `currentResult` counts every result of the document; setting it past
//!   the last goes to the first, and below the first to the last, and moves
//!   `currentPage` to its page. Neither changes when the search does.
//! - The current result is drawn around only while it is on `currentPage`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use hayro::kurbo;
use ljos_engine::{
    Atom, Cx, Engine, EngineError, ItemTree, ModelSignals, ObjectId, PropIndex, QmlListModel,
    QmlObject, QtObject, TreeChange, TreeListener, TreeListeners, Value, qml_impl,
};
use ljos_platform::Platform;

use super::structure::{Entry, Link, Place, Target};
use super::text::whole;
use super::{DocData, PdfDocument, data_of, document_of, set, values};

/// Watches the status of the document `p` of `me` names, through every
/// document it names, and calls `changed` each time either changes.
fn follow_document(
    e: &Engine,
    me: ObjectId,
    p: PropIndex,
    changed: impl Fn(&Engine, ObjectId) + 'static,
) -> Result<(), EngineError> {
    let changed = Rc::new(changed);
    let watched: Rc<Cell<Option<ObjectId>>> = Rc::default();
    let follow = move |e: &Engine| {
        let doc = document_of(e, me, p);
        if doc.is_some() && doc != watched.get() {
            watched.set(doc);
            if let Some(doc) = doc {
                let changed = Rc::clone(&changed);
                let _ = e.watch(doc, PdfDocument::STATUS.index, move |e, _| {
                    // A document watched before is still, when it is no
                    // longer the one named; its changes say nothing.
                    if e.is_alive(me) && document_of(e, me, p) == Some(doc) {
                        changed(e, me);
                    }
                });
            }
        }
        changed(e, me);
    };
    follow(e);
    e.watch(me, p, move |e, _| follow(e))?;
    Ok(())
}

fn data(e: &Engine, me: ObjectId, p: PropIndex) -> Option<Arc<DocData>> {
    let doc = document_of(e, me, p)?;
    e.untrack(|| data_of(e, doc))
}

fn int(e: &Engine, me: ObjectId, p: PropIndex) -> i32 {
    e.untrack(|| e.read(me, p)).to_number() as i32
}

fn reset(rows: &RefCell<impl Signalled>, e: &Engine) {
    let signals = rows.borrow().signals();
    if let Some(s) = signals {
        s.model_reset(e);
    }
}

trait Signalled {
    fn signals(&self) -> Option<ModelSignals>;
}

// ----------------------------------------------------------- PdfSearchModel

/// One place a search found.
#[derive(Clone, Debug)]
struct Hit {
    page: i32,
    index_on_page: i32,
    /// A rectangle around each of its lines, each side on a whole point.
    lines: Vec<kurbo::Rect>,
    before: String,
    after: String,
}

impl Hit {
    fn bounds(&self) -> kurbo::Rect {
        self.lines
            .iter()
            .copied()
            .reduce(|a, b| a.union(b))
            .unwrap_or(kurbo::Rect::ZERO)
    }

    fn place(&self) -> Place {
        let b = self.bounds();
        Place {
            page: self.page,
            location: kurbo::Point::new(b.x0, b.y0),
            zoom: 0.0,
        }
    }

    fn link(&self) -> Value {
        values::found_value(self.place(), &self.before, &self.after, self.lines.clone())
    }
}

#[derive(Debug, Default)]
struct Found {
    rows: Vec<Hit>,
    signals: Option<ModelSignals>,
}

impl Signalled for Found {
    fn signals(&self) -> Option<ModelSignals> {
        self.signals.clone()
    }
}

struct FoundRows(Rc<RefCell<Found>>);

impl QmlListModel for FoundRows {
    fn row_count(&self) -> usize {
        self.0.borrow().rows.len()
    }
    fn role_names(&self) -> Vec<Atom> {
        [
            "page",
            "indexOnPage",
            "location",
            "contextBefore",
            "contextAfter",
        ]
        .into_iter()
        .map(Atom::from)
        .collect()
    }
    fn data(&self, row: usize, role: &str) -> Value {
        let found = self.0.borrow();
        let Some(hit) = found.rows.get(row) else {
            return Value::Undefined;
        };
        match role {
            "page" => Value::Int(hit.page),
            "indexOnPage" => Value::Int(hit.index_on_page),
            "location" => values::point(hit.place().location),
            "contextBefore" => values::text(&hit.before),
            "contextAfter" => values::text(&hit.after),
            _ => Value::Undefined,
        }
    }
}

/// `PdfSearchModel`: every place a string is in a document.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Pdf", name = "PdfSearchModel", extends = QtObject)]
pub struct PdfSearchModel {
    #[qml(base)]
    pub base: QtObject,
    #[qml(notify)]
    pub document: Option<ObjectId>,
    #[qml(notify)]
    pub search_string: String,
    #[qml(notify)]
    pub current_page: i32,
    #[qml(notify)]
    pub current_result: i32,
    /// A `pdfLink` to the current result.
    #[qml(notify, readonly)]
    pub current_result_link: Value,
    /// A `rect` around the current result while it is on `currentPage`.
    #[qml(notify, readonly)]
    pub current_result_bounding_rect: Value,
    #[qml(notify, readonly)]
    pub current_result_bounding_polygons: Value,
    #[qml(notify, readonly)]
    pub current_page_bounding_polygons: Value,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(skip)]
    found: Rc<RefCell<Found>>,
    /// Which search the rows are of: one that ends after a later one
    /// started is forgotten.
    #[qml(skip)]
    searches: Rc<Cell<u64>>,
}

impl Default for PdfSearchModel {
    fn default() -> Self {
        Self {
            base: QtObject::default(),
            document: None,
            search_string: String::new(),
            current_page: 0,
            current_result: 0,
            current_result_link: Value::Undefined,
            current_result_bounding_rect: values::rect(kurbo::Rect::ZERO),
            current_result_bounding_polygons: Value::list(Vec::new()),
            current_page_bounding_polygons: Value::list(Vec::new()),
            count: 0,
            found: Rc::default(),
            searches: Rc::default(),
        }
    }
}

#[qml_impl]
impl PdfSearchModel {
    /// A polygon around each result on `page`.
    #[qml(invokable, name = "boundingPolygonsOnPage")]
    fn bounding_polygons_on_page(cx: &Cx, page: i32) -> Value {
        let found = cx
            .engine
            .with(cx.this, |m: &PdfSearchModel| Rc::clone(&m.found));
        let _ = cx.engine.read(cx.this, PdfSearchModel::COUNT.index);
        found.map_or_else(
            |_| Value::list(Vec::new()),
            |f| polygons_on(&f.borrow(), page),
        )
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        use PdfSearchModel as M;
        let (e, me) = (cx.engine, cx.this);
        values::remember_platform(e);
        let found = e.with(me, |m: &M| Rc::clone(&m.found))?;
        let signals = e.set_list_model(me, Rc::new(FoundRows(Rc::clone(&found))))?;
        found.borrow_mut().signals = Some(signals);
        follow_document(e, me, M::DOCUMENT.index, search)?;
        e.watch(me, M::SEARCH_STRING.index, move |e, _| search(e, me))?;
        e.watch(me, M::CURRENT_RESULT.index, move |e, _| choose(e, me))?;
        e.watch(me, M::CURRENT_PAGE.index, move |e, _| show_current(e, me))?;
        show_current(e, me);
        Ok(())
    }
}

fn polygons_on(found: &Found, page: i32) -> Value {
    Value::list(
        found
            .rows
            .iter()
            .filter(|h| h.page == page)
            .flat_map(|h| h.lines.iter().copied().map(values::polygon))
            .collect(),
    )
}

/// Starts searching again, off the engine's thread; the rows change when
/// it is done.
fn search(e: &Engine, me: ObjectId) {
    use PdfSearchModel as M;
    let Ok(searches) = e.with(me, |m: &M| Rc::clone(&m.searches)) else {
        return;
    };
    let this = searches.get() + 1;
    searches.set(this);
    let needle = e.untrack(|| e.read(me, M::SEARCH_STRING.index).to_js_string());
    let doc = data(e, me, M::DOCUMENT.index);
    let reply = Platform::of(e).expect_for("PdfSearchModel search", Some(me));
    // The rows go to the engine's thread through a slot only it empties:
    // what a search found is plain data, the model is not.
    let slot: Arc<Mutex<Vec<Hit>>> = Arc::default();
    let filled = Arc::clone(&slot);
    crate::off_thread::off_thread(move || {
        let hits = doc.map(|d| find(&d, &needle)).unwrap_or_default();
        *filled.lock().unwrap_or_else(PoisonError::into_inner) = hits;
        reply.send(move |e: &Engine| {
            if !e.is_alive(me) {
                return;
            }
            let Ok((searches, found)) =
                e.with(me, |m: &M| (Rc::clone(&m.searches), Rc::clone(&m.found)))
            else {
                return;
            };
            if searches.get() != this {
                return;
            }
            let hits = std::mem::take(&mut *slot.lock().unwrap_or_else(PoisonError::into_inner));
            let count = i32::try_from(hits.len()).unwrap_or(i32::MAX);
            found.borrow_mut().rows = hits;
            reset(&found, e);
            set(e, me, M::COUNT.index, Value::Int(count));
            show_current(e, me);
        });
    });
}

/// Every place `needle` is in the document, page by page.
fn find(doc: &DocData, needle: &str) -> Vec<Hit> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for page in 0..doc.page_count() {
        let Ok(page) = i32::try_from(page) else { break };
        let Some(t) = doc.text(page) else { continue };
        for (i, (start, end)) in t.find(needle).into_iter().enumerate() {
            let (before, after) = t.context(start, end);
            hits.push(Hit {
                page,
                index_on_page: i32::try_from(i).unwrap_or(i32::MAX),
                lines: t.lines(start, end).into_iter().map(whole).collect(),
                before,
                after,
            });
        }
    }
    hits
}

/// The current result was set: it wraps past either end, and its page
/// becomes the current one.
fn choose(e: &Engine, me: ObjectId) {
    use PdfSearchModel as M;
    let Ok(found) = e.with(me, |m: &M| Rc::clone(&m.found)) else {
        return;
    };
    let n = found.borrow().rows.len();
    let asked = int(e, me, M::CURRENT_RESULT.index);
    if n > 0 {
        let i = if asked < 0 {
            n - 1
        } else if usize::try_from(asked).is_ok_and(|a| a >= n) {
            0
        } else {
            usize::try_from(asked).unwrap_or(0)
        };
        let page = found.borrow().rows[i].page;
        e.batch(|| {
            set(
                e,
                me,
                M::CURRENT_RESULT.index,
                Value::Int(i32::try_from(i).unwrap_or(0)),
            );
            set(e, me, M::CURRENT_PAGE.index, Value::Int(page));
        });
    }
    show_current(e, me);
}

/// What is said of the current result and the current page.
fn show_current(e: &Engine, me: ObjectId) {
    use PdfSearchModel as M;
    let Ok(found) = e.with(me, |m: &M| Rc::clone(&m.found)) else {
        return;
    };
    let found = found.borrow();
    let page = int(e, me, M::CURRENT_PAGE.index);
    let current = usize::try_from(int(e, me, M::CURRENT_RESULT.index))
        .ok()
        .and_then(|i| found.rows.get(i));
    let link = current.map_or_else(
        || {
            Value::Custom(Arc::new(ljos_engine::CustomValue::default_of(
                &values::LINK,
            )))
        },
        Hit::link,
    );
    let shown = current.filter(|h| h.page == page);
    let rect = shown.map_or(kurbo::Rect::ZERO, Hit::bounds);
    let polygons = Value::list(
        shown
            .map(|h| h.lines.iter().copied().map(values::polygon).collect())
            .unwrap_or_default(),
    );
    let on_page = polygons_on(&found, page);
    drop(found);
    e.batch(|| {
        set(e, me, M::CURRENT_RESULT_LINK.index, link);
        set(
            e,
            me,
            M::CURRENT_RESULT_BOUNDING_RECT.index,
            values::rect(rect),
        );
        set(e, me, M::CURRENT_RESULT_BOUNDING_POLYGONS.index, polygons);
        set(e, me, M::CURRENT_PAGE_BOUNDING_POLYGONS.index, on_page);
    });
}

// ------------------------------------------------------------- PdfLinkModel

#[derive(Debug, Default)]
struct Links {
    rows: Arc<Vec<Link>>,
    signals: Option<ModelSignals>,
}

impl Signalled for Links {
    fn signals(&self) -> Option<ModelSignals> {
        self.signals.clone()
    }
}

struct LinkRows(Rc<RefCell<Links>>);

impl QmlListModel for LinkRows {
    fn row_count(&self) -> usize {
        self.0.borrow().rows.len()
    }
    fn role_names(&self) -> Vec<Atom> {
        ["link", "rectangle", "url", "page", "location", "zoom"]
            .into_iter()
            .map(Atom::from)
            .collect()
    }
    fn data(&self, row: usize, role: &str) -> Value {
        let links = self.0.borrow();
        let Some(link) = links.rows.get(row) else {
            return Value::Undefined;
        };
        let place = match &link.to {
            Target::Place(p) => Some(*p),
            Target::Url(_) => None,
        };
        match role {
            "link" => values::link_value(&link.to, link.rect),
            "rectangle" => values::rect(link.rect),
            "url" => Value::Url(Atom::from(match &link.to {
                Target::Url(u) => u.as_str(),
                Target::Place(_) => "",
            })),
            "page" => Value::Int(place.map_or(-1, |p| p.page)),
            "location" => values::point(place.map_or(kurbo::Point::ZERO, |p| p.location)),
            "zoom" => Value::Real(place.map_or(1.0, |p| if p.zoom > 0.0 { p.zoom } else { 1.0 })),
            _ => Value::Undefined,
        }
    }
}

/// `PdfLinkModel`: the links of one page.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Pdf", name = "PdfLinkModel", extends = QtObject)]
pub struct PdfLinkModel {
    #[qml(base)]
    pub base: QtObject,
    #[qml(notify)]
    pub document: Option<ObjectId>,
    #[qml(notify)]
    pub page: i32,
    #[qml(skip)]
    links: Rc<RefCell<Links>>,
}

#[qml_impl]
impl PdfLinkModel {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        values::remember_platform(e);
        let links = e.with(me, |m: &PdfLinkModel| Rc::clone(&m.links))?;
        let signals = e.set_list_model(me, Rc::new(LinkRows(Rc::clone(&links))))?;
        links.borrow_mut().signals = Some(signals);
        follow_document(e, me, PdfLinkModel::DOCUMENT.index, relink)?;
        e.watch(me, PdfLinkModel::PAGE.index, move |e, _| relink(e, me))?;
        Ok(())
    }
}

fn relink(e: &Engine, me: ObjectId) {
    let Ok(links) = e.with(me, |m: &PdfLinkModel| Rc::clone(&m.links)) else {
        return;
    };
    let page = int(e, me, PdfLinkModel::PAGE.index);
    let rows =
        data(e, me, PdfLinkModel::DOCUMENT.index).map_or_else(Arc::default, |d| d.links(page));
    links.borrow_mut().rows = rows;
    reset(&links, e);
}

// --------------------------------------------------------- PdfBookmarkModel

#[derive(Debug, Default)]
struct Outline {
    entries: RefCell<Arc<Vec<Entry>>>,
    listeners: TreeListeners,
}

fn entry<'a>(entries: &'a [Entry], path: &[usize]) -> Option<&'a Entry> {
    let (first, rest) = path.split_first()?;
    let mut at = entries.get(*first)?;
    for i in rest {
        at = at.children.get(*i)?;
    }
    Some(at)
}

struct OutlineTree(Rc<Outline>);

impl ItemTree for OutlineTree {
    fn row_count(&self, parent: &[usize]) -> usize {
        let entries = self.0.entries.borrow();
        if parent.is_empty() {
            return entries.len();
        }
        entry(&entries, parent).map_or(0, |e| e.children.len())
    }
    fn role_names(&self) -> Vec<Atom> {
        ["title", "level", "page", "location", "zoom"]
            .into_iter()
            .map(Atom::from)
            .collect()
    }
    fn data(&self, _: &Engine, path: &[usize], column: usize, role: &str) -> Value {
        let entries = self.0.entries.borrow();
        let Some(entry) = entry(&entries, path).filter(|_| column == 0) else {
            return Value::Undefined;
        };
        let place = entry.place;
        match role {
            "title" => values::text(&entry.title),
            "level" => Value::Int(i32::try_from(path.len() - 1).unwrap_or(0)),
            "page" => Value::Int(place.map_or(-1, |p| p.page)),
            "location" => values::point(place.map_or(kurbo::Point::ZERO, |p| p.location)),
            "zoom" => Value::Real(place.map_or(0.0, |p| p.zoom)),
            _ => Value::Undefined,
        }
    }
    fn subscribe(&self, listener: TreeListener) -> u64 {
        self.0.listeners.add(listener)
    }
    fn unsubscribe(&self, id: u64) {
        self.0.listeners.remove(id);
    }
}

/// `PdfBookmarkModel`: the outline of a document, as a tree.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Pdf", name = "PdfBookmarkModel", extends = QtObject)]
pub struct PdfBookmarkModel {
    #[qml(base)]
    pub base: QtObject,
    #[qml(notify)]
    pub document: Option<ObjectId>,
    #[qml(skip)]
    outline: Rc<Outline>,
}

#[qml_impl]
impl PdfBookmarkModel {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let outline = e.with(me, |m: &PdfBookmarkModel| Rc::clone(&m.outline))?;
        e.set_object_tree(me, Rc::new(OutlineTree(outline)))?;
        follow_document(e, me, PdfBookmarkModel::DOCUMENT.index, reread)?;
        Ok(())
    }
}

fn reread(e: &Engine, me: ObjectId) {
    let Ok(outline) = e.with(me, |m: &PdfBookmarkModel| Rc::clone(&m.outline)) else {
        return;
    };
    let entries =
        data(e, me, PdfBookmarkModel::DOCUMENT.index).map_or_else(Arc::default, |d| d.outline());
    *outline.entries.borrow_mut() = entries;
    outline.listeners.emit(e, &TreeChange::ModelReset);
}
