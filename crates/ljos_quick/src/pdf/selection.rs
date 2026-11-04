//! `PdfSelection`: the text of a page between two points a reader drags
//! between.
//!
//! Measured on qml6 (features/pdf/pdf-text.feature): the points are pixels
//! of the page drawn at `renderScale`; the text is worked out again each
//! time one of them moves, unless `hold` is set, and is not when `hold` is
//! let go of; `clear` puts both points back at the top left.

use std::cell::Cell;
use std::sync::Arc;

use ljos_engine::{Cx, Engine, EngineError, ObjectId, Point, QmlObject, Value, qml_impl};
use ljos_platform::{ClipboardMode, Platform};

use super::{PdfDocument, data_of, document_of, set, text, values};
use crate::item::Item;

#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Pdf", name = "PdfSelection", extends = Item)]
pub struct PdfSelection {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub document: Option<ObjectId>,
    #[qml(notify)]
    pub page: i32,
    #[qml(notify)]
    pub render_scale: f64,
    #[qml(notify)]
    pub from: Point,
    #[qml(notify)]
    pub to: Point,
    #[qml(notify)]
    pub hold: bool,
    #[qml(notify, readonly)]
    pub text: String,
    /// A closed polygon around each line of the text, in pixels as the
    /// points are.
    #[qml(notify, readonly)]
    pub geometry: Value,
}

impl Default for PdfSelection {
    fn default() -> Self {
        Self {
            item: Item::default(),
            document: None,
            page: 0,
            render_scale: 1.0,
            from: Point { x: 0.0, y: 0.0 },
            to: Point { x: 0.0, y: 0.0 },
            hold: false,
            text: String::new(),
            geometry: Value::list(Vec::new()),
        }
    }
}

#[qml_impl]
impl PdfSelection {
    /// Selects nothing: both points go back to the top left.
    #[qml(invokable, name = "clear")]
    fn clear(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        let zero = Value::Point(Arc::new(Point { x: 0.0, y: 0.0 }));
        e.batch(|| {
            set(e, me, PdfSelection::FROM.index, zero.clone());
            set(e, me, PdfSelection::TO.index, zero);
        });
        show(e, me, None);
    }

    /// Selects all the text of the page.
    #[qml(invokable, name = "selectAll")]
    fn select_all(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        let all = page_text(e, me)
            .map(|t| (0, t.len()))
            .filter(|(_, n)| *n > 0);
        show(e, me, all);
    }

    #[qml(invokable, name = "copyToClipboard")]
    fn copy_to_clipboard(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        let selected = e.untrack(|| e.read(me, PdfSelection::TEXT.index).to_js_string());
        if let Err(err) = Platform::of(e)
            .clipboard()
            .set_text(ClipboardMode::Clipboard, &selected)
        {
            e.log(
                ljos_engine::LogLevel::Warning,
                super::CATEGORY,
                format!("PdfSelection: cannot copy: {err}"),
            );
        }
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        // What the document has open is watched through the document the
        // selection has now: one given later is watched once it is.
        let watched: std::rc::Rc<Cell<Option<ObjectId>>> = std::rc::Rc::default();
        let follow = move |e: &Engine| {
            let doc = document_of(e, me, PdfSelection::DOCUMENT.index);
            if doc.is_some() && doc != watched.get() {
                watched.set(doc);
                if let Some(doc) = doc {
                    let _ = e.watch(doc, PdfDocument::STATUS.index, move |e, _| {
                        if e.is_alive(me) {
                            reselect(e, me);
                        }
                    });
                }
            }
            reselect(e, me);
        };
        follow(e);
        e.watch(me, PdfSelection::DOCUMENT.index, move |e, _| follow(e))?;
        for p in [
            PdfSelection::PAGE.index,
            PdfSelection::RENDER_SCALE.index,
            PdfSelection::FROM.index,
            PdfSelection::TO.index,
        ] {
            e.watch(me, p, move |e, _| {
                let held = e
                    .untrack(|| e.read(me, PdfSelection::HOLD.index))
                    .to_boolean();
                if !held {
                    reselect(e, me);
                }
            })?;
        }
        Ok(())
    }
}

fn page_text(e: &Engine, me: ObjectId) -> Option<Arc<super::text::PageText>> {
    let doc = document_of(e, me, PdfSelection::DOCUMENT.index)?;
    let page = e
        .untrack(|| e.read(me, PdfSelection::PAGE.index))
        .to_number() as i32;
    e.untrack(|| data_of(e, doc))?.text(page)
}

fn scale(e: &Engine, me: ObjectId) -> f64 {
    let s = e
        .untrack(|| e.read(me, PdfSelection::RENDER_SCALE.index))
        .to_number();
    if s.is_finite() && s > 0.0 { s } else { 1.0 }
}

/// Selects what is between the two points.
fn reselect(e: &Engine, me: ObjectId) {
    let scale = scale(e, me);
    let at = |p| match e.untrack(|| e.read(me, p)) {
        Value::Point(p) => hayro::kurbo::Point::new(p.x / scale, p.y / scale),
        _ => hayro::kurbo::Point::ZERO,
    };
    let (from, to) = (at(PdfSelection::FROM.index), at(PdfSelection::TO.index));
    let found = page_text(e, me).and_then(|t| t.between(from, to));
    show(e, me, found);
}

fn show(e: &Engine, me: ObjectId, found: Option<(usize, usize)>) {
    let t = page_text(e, me);
    let scale = scale(e, me);
    let (words, lines) = match (&t, found) {
        (Some(t), Some((start, end))) => (t.text(start, end), t.lines(start, end)),
        _ => (String::new(), Vec::new()),
    };
    let geometry = Value::list(
        lines
            .into_iter()
            .map(|r| values::polygon(super::text::whole(r).scale_from_origin(scale)))
            .collect(),
    );
    e.batch(|| {
        set(e, me, PdfSelection::TEXT.index, text(&words));
        set(e, me, PdfSelection::GEOMETRY.index, geometry);
    });
}
