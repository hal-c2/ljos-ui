//! The value types of `QtQuick.Pdf`: `pdfLink`, where something in a
//! document is or goes, and `pdfSelection`, some text of a page.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use hayro::kurbo;
use ljos_engine::{Atom, CustomMember, CustomType, CustomValue, Engine, Point, Rect, Value};
use ljos_platform::{ClipboardMode, Platform};

use super::structure::{Place, Target};

thread_local! {
    /// The platform whose clipboard a value's `copyToClipboard` writes:
    /// a value's methods are not given the engine, and every value of
    /// these types is made on the engine's thread.
    static PLATFORM: RefCell<Weak<Platform>> = const { RefCell::new(Weak::new()) };
}

/// Remembers the platform of `e`, for the values made after.
pub(crate) fn remember_platform(e: &Engine) {
    let platform = Platform::of(e);
    PLATFORM.with(|p| *p.borrow_mut() = Rc::downgrade(&platform));
}

fn copy(text: &str) -> Result<Value, String> {
    let platform = PLATFORM.with(|p| p.borrow().upgrade());
    if let Some(platform) = platform {
        platform
            .clipboard()
            .set_text(ClipboardMode::Clipboard, text)
            .map_err(|e| e.to_string())?;
    }
    Ok(Value::Undefined)
}

pub(crate) fn point(p: kurbo::Point) -> Value {
    Value::Point(Arc::new(Point { x: p.x, y: p.y }))
}

pub(crate) fn rect(r: kurbo::Rect) -> Value {
    Value::Rect(Arc::new(Rect {
        x: r.x0,
        y: r.y0,
        width: r.width(),
        height: r.height(),
    }))
}

/// A rectangle as a polygon: its corners from the top left, clockwise, and
/// the first again to close it, as Qt gives one.
pub(crate) fn polygon(r: kurbo::Rect) -> Value {
    Value::list(
        [
            (r.x0, r.y0),
            (r.x1, r.y0),
            (r.x1, r.y1),
            (r.x0, r.y1),
            (r.x0, r.y0),
        ]
        .into_iter()
        .map(|(x, y)| point(kurbo::Point::new(x, y)))
        .collect(),
    )
}

pub(crate) fn text(s: &str) -> Value {
    Value::String(Atom::from(s))
}

// ------------------------------------------------------------------ pdfLink

#[derive(Debug)]
pub(crate) struct LinkType;
pub(crate) static LINK: LinkType = LinkType;

mod link {
    pub const VALID: usize = 0;
    pub const PAGE: usize = 1;
    pub const LOCATION: usize = 2;
    pub const ZOOM: usize = 3;
    pub const URL: usize = 4;
    pub const FOUND: usize = 8;
}

/// A link of a page, as a link model's row has it: a web address is no
/// place in the document, and a place that leaves the zoom as it is is a
/// zoom of 1.
pub(crate) fn link_value(to: &Target, rectangle: kurbo::Rect) -> Value {
    let (valid, page, location, zoom, url) = match to {
        Target::Url(url) => (false, -1, kurbo::Point::ZERO, 1.0, url.as_str()),
        Target::Place(p) => (
            true,
            p.page,
            p.location,
            if p.zoom > 0.0 { p.zoom } else { 1.0 },
            "",
        ),
    };
    CustomValue::new(
        &LINK,
        vec![
            Value::Bool(valid),
            Value::Int(page),
            point(location),
            Value::Real(zoom),
            Value::Url(Atom::from(url)),
            text(""),
            text(""),
            Value::list(vec![rect(rectangle)]),
            Value::Bool(false),
        ],
    )
    .into()
}

/// A place in the document, its zoom as it is.
pub(crate) fn place_value(place: Place) -> Value {
    CustomValue::new(
        &LINK,
        vec![
            Value::Bool(true),
            Value::Int(place.page),
            point(place.location),
            Value::Real(place.zoom),
        ],
    )
    .into()
}

/// Where a search found something: its place, the text around it and a
/// rectangle around each of its lines.
pub(crate) fn found_value(
    place: Place,
    before: &str,
    after: &str,
    rectangles: Vec<kurbo::Rect>,
) -> Value {
    CustomValue::new(
        &LINK,
        vec![
            Value::Bool(true),
            Value::Int(place.page),
            point(place.location),
            Value::Real(place.zoom),
            Value::Url(Atom::from("")),
            text(before),
            text(after),
            Value::list(rectangles.into_iter().map(rect).collect()),
            Value::Bool(true),
        ],
    )
    .into()
}

/// The place a link value goes: `None` for one that goes nowhere in the
/// document.
pub(crate) fn link_place(v: &Value) -> Option<(i32, Point, f64)> {
    let Value::Custom(c) = v else { return None };
    if !c.is(&LINK) || !c.field(link::VALID).to_boolean() {
        return None;
    }
    let location = match c.field(link::LOCATION) {
        Value::Point(p) => **p,
        _ => Point { x: 0.0, y: 0.0 },
    };
    Some((
        c.field(link::PAGE).to_number() as i32,
        location,
        c.field(link::ZOOM).to_number(),
    ))
}

fn link_text(v: &CustomValue) -> String {
    let url = v.field(link::URL).to_js_string().to_string();
    if !url.is_empty() {
        return url;
    }
    // A search result is a place, though not one a reader is told of.
    if !v.field(link::VALID).to_boolean() || v.field(link::FOUND).to_boolean() {
        return String::new();
    }
    let (x, y) = match v.field(link::LOCATION) {
        Value::Point(p) => (p.x, p.y),
        _ => (0.0, 0.0),
    };
    format!(
        "Page {} location {x:.1}, {y:.1} zoom {}",
        v.field(link::PAGE).to_js_string(),
        v.field(link::ZOOM).to_js_string()
    )
}

impl CustomType for LinkType {
    fn name(&self) -> &'static str {
        "pdfLink"
    }
    fn module(&self) -> &'static str {
        "QtQuick.Pdf"
    }
    fn members(&self) -> &'static [CustomMember] {
        const M: &[CustomMember] = &[
            CustomMember::stored("valid", "bool"),
            CustomMember::stored("page", "int"),
            CustomMember::stored("location", "point"),
            CustomMember::stored("zoom", "real"),
            CustomMember::stored("url", "url"),
            CustomMember::stored("contextBefore", "string"),
            CustomMember::stored("contextAfter", "string"),
            CustomMember::stored("rectangles", "list<rect>"),
            CustomMember::stored("found", "bool").hidden(),
        ];
        M
    }
    fn methods(&self) -> &'static [&'static str] {
        &["toString", "copyToClipboard"]
    }
    fn default_fields(&self) -> Vec<Value> {
        vec![
            Value::Bool(false),
            Value::Int(-1),
            point(kurbo::Point::ZERO),
            Value::Real(0.0),
            Value::Url(Atom::from("")),
            text(""),
            text(""),
            Value::list(Vec::new()),
            Value::Bool(false),
        ]
    }
    fn text(&self, v: &CustomValue) -> String {
        link_text(v)
    }
    fn with(&self, _: &CustomValue, _: &str, _: &Value) -> Option<CustomValue> {
        // Read-only, as Qt's is.
        None
    }
    fn call(&self, v: &CustomValue, method: &str, _: &[Value]) -> Result<Value, String> {
        match method {
            "toString" => Ok(text(&link_text(v))),
            "copyToClipboard" => copy(&link_text(v)),
            _ => Err(format!("{method} is not a function")),
        }
    }
}

// ------------------------------------------------------------- pdfSelection

#[derive(Debug)]
pub(crate) struct SelectionType;
pub(crate) static SELECTION: SelectionType = SelectionType;

/// Some of the text of a page: `None` for none.
pub(crate) fn selection_value(
    found: Option<(usize, usize)>,
    text_of: &str,
    lines: &[kurbo::Rect],
) -> Value {
    let Some((start, end)) = found else {
        return CustomValue::default_of(&SELECTION).into();
    };
    let bounding = lines
        .iter()
        .copied()
        .reduce(|a, b| a.union(b))
        .unwrap_or(kurbo::Rect::ZERO);
    let index = |i: usize| Value::Int(i32::try_from(i).unwrap_or(i32::MAX));
    CustomValue::new(
        &SELECTION,
        vec![
            Value::Bool(true),
            Value::list(lines.iter().copied().map(polygon).collect()),
            rect(bounding),
            text(text_of),
            index(start),
            index(end),
        ],
    )
    .into()
}

impl CustomType for SelectionType {
    fn name(&self) -> &'static str {
        "pdfSelection"
    }
    fn module(&self) -> &'static str {
        "QtQuick.Pdf"
    }
    fn members(&self) -> &'static [CustomMember] {
        const M: &[CustomMember] = &[
            CustomMember::stored("valid", "bool"),
            CustomMember::stored("bounds", "list<polygon>"),
            CustomMember::stored("boundingRectangle", "rect"),
            CustomMember::stored("text", "string"),
            CustomMember::stored("startIndex", "int"),
            CustomMember::stored("endIndex", "int"),
        ];
        M
    }
    fn methods(&self) -> &'static [&'static str] {
        &["copyToClipboard"]
    }
    fn default_fields(&self) -> Vec<Value> {
        vec![
            Value::Bool(false),
            Value::list(Vec::new()),
            rect(kurbo::Rect::ZERO),
            text(""),
            Value::Int(-1),
            Value::Int(-1),
        ]
    }
    fn text(&self, v: &CustomValue) -> String {
        v.field(3).to_js_string().to_string()
    }
    fn with(&self, _: &CustomValue, _: &str, _: &Value) -> Option<CustomValue> {
        None
    }
    fn call(&self, v: &CustomValue, method: &str, _: &[Value]) -> Result<Value, String> {
        match method {
            "copyToClipboard" => copy(&v.field(3).to_js_string()),
            _ => Err(format!("{method} is not a function")),
        }
    }
}
