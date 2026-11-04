//! What a document says of itself besides its pages' drawings: what its
//! pages are called, where its links go and its outline.
//!
//! Places are in points from the top left of the page they are on, as the
//! page is shown (`Page::initial_transform`).

use hayro::hayro_interpret::util::TransformExt as _;
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::object::{Array, Dict, MaybeRef, Name, Number, Object, ObjectIdentifier};
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, Point, Rect};

/// How deep a tree of labels, names or outline entries is followed: a
/// document whose tree loops back on itself still opens.
const DEEPEST: usize = 64;

/// Outline entries read at most, for the same reason.
const MOST_ENTRIES: usize = 100_000;

/// Where in the document something goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Place {
    pub page: i32,
    pub location: Point,
    /// What the document says; 0 when it leaves the zoom as it is.
    pub zoom: f64,
}

/// A link on a page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Link {
    pub rect: Rect,
    pub to: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    Url(String),
    Place(Place),
}

/// An entry of the outline and the entries under it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Entry {
    pub title: String,
    pub place: Option<Place>,
    pub children: Vec<Entry>,
}

/// One range of page labels: from `first`, numbered in `style` from
/// `start`, after `prefix`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LabelRange {
    pub first: usize,
    pub style: Option<u8>,
    pub prefix: String,
    pub start: i64,
}

fn catalog(pdf: &Pdf) -> Option<Dict<'_>> {
    pdf.xref().get::<Dict<'_>>(pdf.xref().root_id())
}

fn name_is(n: &Option<Name<'_>>, what: &[u8]) -> bool {
    n.as_ref().is_some_and(|n| &**n == what)
}

/// A text string of the document as text.
fn string(o: Option<hayro::hayro_syntax::object::String<'_>>) -> String {
    super::info_text(o.map(|s| s.as_bytes().to_vec()).as_ref())
}

/// The labels of the document's pages, as its `/PageLabels` number tree
/// lists them.
pub(crate) fn labels(pdf: &Pdf) -> Vec<LabelRange> {
    let mut ranges = Vec::new();
    if let Some(tree) = catalog(pdf).and_then(|c| c.get::<Dict<'_>>(b"PageLabels")) {
        number_tree(&tree, 0, &mut |key, value| {
            let Some(d) = value.into_dict() else { return };
            let Ok(first) = usize::try_from(key) else {
                return;
            };
            ranges.push(LabelRange {
                first,
                style: d.get::<Name<'_>>(b"S").and_then(|s| s.first().copied()),
                prefix: string(d.get(b"P")),
                start: d.get::<Number>(b"St").map_or(1, |n| n.as_i64()),
            });
        });
    }
    ranges.sort_by_key(|r| r.first);
    ranges
}

fn number_tree<'a>(node: &Dict<'a>, depth: usize, each: &mut dyn FnMut(i64, Object<'a>)) {
    if depth > DEEPEST {
        return;
    }
    if let Some(nums) = node.get::<Array<'a>>(b"Nums") {
        let mut it = nums.iter::<Object<'a>>();
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            if let Some(k) = k.into_number() {
                each(k.as_i64(), v);
            }
        }
    }
    if let Some(kids) = node.get::<Array<'a>>(b"Kids") {
        for kid in kids.iter::<Dict<'a>>() {
            number_tree(&kid, depth + 1, each);
        }
    }
}

/// What page `page` is called: its label, or its number when the document
/// labels none.
pub(crate) fn label(ranges: &[LabelRange], page: usize) -> String {
    let Some(r) = ranges.iter().rev().find(|r| r.first <= page) else {
        return (page + 1).to_string();
    };
    // A document's /St is any number it likes: the page's own must not
    // overflow past it.
    let offset = i64::try_from(page - r.first).unwrap_or(i64::MAX);
    let n = r.start.saturating_add(offset);
    let number = match r.style {
        Some(b'D') => n.to_string(),
        Some(b'R') => roman(n),
        Some(b'r') => roman(n).to_lowercase(),
        Some(b'A') => letters(n),
        Some(b'a') => letters(n).to_lowercase(),
        _ => String::new(),
    };
    format!("{}{number}", r.prefix)
}

/// Roman numerals for 1 to 3999, the ones written without a bar over
/// them; any other number in figures, so that a document's huge /St makes
/// a short label rather than millions of Ms.
fn roman(mut n: i64) -> String {
    if !(1..=3999).contains(&n) {
        return n.to_string();
    }
    const DIGITS: [(i64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (v, s) in DIGITS {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

/// A, B … Z, then AA, BB … ZZ, then AAA: the PDF's way of lettering.
fn letters(n: i64) -> String {
    if n < 1 {
        return String::new();
    }
    let letter = char::from(b'A' + ((n - 1) % 26) as u8);
    let times = ((n - 1) / 26 + 1) as usize;
    std::iter::repeat_n(letter, times.min(1000)).collect()
}

/// Every page of the document, by the object it is: where a destination's
/// page is found.
fn page_index(pdf: &Pdf, id: ObjectIdentifier) -> Option<usize> {
    pdf.pages()
        .iter()
        .position(|p| p.raw().obj_id() == Some(id))
}

/// A place a destination array names: `[page /XYZ left top zoom]` and the
/// fits, whose place is the top of the page.
fn place_of(pdf: &Pdf, dest: &Array<'_>) -> Option<Place> {
    let mut raw = dest.raw_iter();
    let page = match raw.next()? {
        MaybeRef::Ref(r) => page_index(pdf, r.into())?,
        // A destination in another document gives its page's number.
        MaybeRef::NotRef(o) => usize::try_from(o.into_number()?.as_i64()).ok()?,
    };
    let target = pdf.pages().get(page)?;
    let mut rest = dest.iter::<Object<'_>>().skip(1);
    let kind = rest.next().and_then(Object::into_name);
    let mut number = || {
        rest.next()
            .and_then(Object::into_number)
            .map(|n| n.as_f64())
    };
    let (left, top, zoom) = if name_is(&kind, b"XYZ") {
        (number(), number(), number().unwrap_or(0.0))
    } else {
        (None, None, 0.0)
    };
    // A side the destination leaves out stays where it is: here, the left
    // or the top of the page.
    let p = shown(target) * Point::new(left.unwrap_or(0.0), top.unwrap_or(0.0));
    Some(Place {
        page: i32::try_from(page).ok()?,
        location: Point::new(
            if left.is_some() { p.x } else { 0.0 },
            if top.is_some() { p.y } else { 0.0 },
        ),
        zoom,
    })
}

/// From PDF space to points from the top left of the page as shown.
fn shown(page: &Page<'_>) -> Affine {
    page.initial_transform(true).to_kurbo()
}

/// A destination: an array, a named one, or a dictionary holding one.
fn destination(pdf: &Pdf, o: Object<'_>, depth: usize) -> Option<Place> {
    if depth > 4 {
        return None;
    }
    match o {
        Object::Array(a) => place_of(pdf, &a),
        Object::Dict(d) => destination(pdf, d.get::<Object<'_>>(b"D")?, depth + 1),
        Object::Name(n) => {
            let dests = catalog(pdf)?.get::<Dict<'_>>(b"Dests")?;
            destination(pdf, dests.get::<Object<'_>>(&*n)?, depth + 1)
        }
        Object::String(s) => {
            let tree = catalog(pdf)?
                .get::<Dict<'_>>(b"Names")?
                .get::<Dict<'_>>(b"Dests")?;
            let found = name_tree(&tree, s.as_bytes(), 0)?;
            destination(pdf, found, depth + 1)
        }
        _ => None,
    }
}

fn name_tree<'a>(node: &Dict<'a>, key: &[u8], depth: usize) -> Option<Object<'a>> {
    if depth > DEEPEST {
        return None;
    }
    if let Some(names) = node.get::<Array<'a>>(b"Names") {
        let mut it = names.iter::<Object<'a>>();
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            if k.into_string().is_some_and(|k| k.as_bytes() == key) {
                return Some(v);
            }
        }
    }
    node.get::<Array<'a>>(b"Kids")?
        .iter::<Dict<'a>>()
        .find_map(|kid| name_tree(&kid, key, depth + 1))
}

/// Where an action or a destination goes.
fn target(pdf: &Pdf, d: &Dict<'_>) -> Option<Target> {
    if let Some(action) = d.get::<Dict<'_>>(b"A") {
        let kind = action.get::<Name<'_>>(b"S");
        if name_is(&kind, b"URI") {
            let url = action.get::<hayro::hayro_syntax::object::String<'_>>(b"URI")?;
            return Some(Target::Url(
                String::from_utf8_lossy(url.as_bytes()).into_owned(),
            ));
        }
        if name_is(&kind, b"GoTo") {
            return destination(pdf, action.get::<Object<'_>>(b"D")?, 0).map(Target::Place);
        }
        return None;
    }
    destination(pdf, d.get::<Object<'_>>(b"Dest")?, 0).map(Target::Place)
}

/// The links on page `page`, in the order the page lists them.
pub(crate) fn links(pdf: &Pdf, page: usize) -> Vec<Link> {
    let Some(p) = pdf.pages().get(page) else {
        return Vec::new();
    };
    let Some(annots) = p.raw().get::<Array<'_>>(b"Annots") else {
        return Vec::new();
    };
    let shown = shown(p);
    annots
        .iter::<Dict<'_>>()
        .filter(|a| name_is(&a.get::<Name<'_>>(b"Subtype"), b"Link"))
        .filter_map(|a| {
            let r = a.get::<hayro::hayro_syntax::object::Rect>(b"Rect")?;
            let corners = [
                Point::new(r.x0, r.y0),
                Point::new(r.x1, r.y1),
                Point::new(r.x0, r.y1),
                Point::new(r.x1, r.y0),
            ]
            .map(|c| shown * c);
            let rect = corners
                .iter()
                .skip(1)
                .fold(Rect::from_points(corners[0], corners[0]), |r, c| {
                    r.union_pt(*c)
                });
            Some(Link {
                rect,
                to: target(pdf, &a)?,
            })
        })
        .collect()
}

/// The document's outline.
pub(crate) fn outline(pdf: &Pdf) -> Vec<Entry> {
    let Some(first) = catalog(pdf)
        .and_then(|c| c.get::<Dict<'_>>(b"Outlines"))
        .and_then(|o| o.get::<Dict<'_>>(b"First"))
    else {
        return Vec::new();
    };
    let mut budget = MOST_ENTRIES;
    entries(pdf, first, 0, &mut budget)
}

fn entries(pdf: &Pdf, first: Dict<'_>, depth: usize, budget: &mut usize) -> Vec<Entry> {
    let mut out = Vec::new();
    if depth > DEEPEST {
        return out;
    }
    let mut next = Some(first);
    let mut seen = Vec::new();
    while let Some(d) = next.take() {
        // An entry met again is a loop: the rest of this level is lost.
        if let Some(id) = d.obj_id() {
            if seen.contains(&id) {
                break;
            }
            seen.push(id);
        }
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        out.push(Entry {
            title: string(d.get(b"Title")),
            place: match target(pdf, &d) {
                Some(Target::Place(p)) => Some(p),
                _ => None,
            },
            children: d
                .get::<Dict<'_>>(b"First")
                .map(|f| entries(pdf, f, depth + 1, budget))
                .unwrap_or_default(),
        });
        next = d.get::<Dict<'_>>(b"Next");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(first: usize, style: u8, prefix: &str, start: i64) -> LabelRange {
        LabelRange {
            first,
            style: Some(style),
            prefix: prefix.to_owned(),
            start,
        }
    }

    #[test]
    fn labels_are_numbered_in_their_style_from_their_start() {
        let ranges = [
            range(0, b'r', "", 1),
            range(4, b'D', "A-", 1),
            range(6, b'A', "", 26),
        ];
        let all: Vec<String> = (0..8).map(|p| label(&ranges, p)).collect();
        assert_eq!(all, ["i", "ii", "iii", "iv", "A-1", "A-2", "Z", "AA"]);
        assert_eq!(label(&[], 4), "5");
        assert_eq!(roman(1994), "MCMXCIV");
        assert_eq!(letters(53), "AAA");
    }

    #[test]
    fn a_huge_start_makes_a_short_label() {
        assert_eq!(
            label(&[range(0, b'R', "", 1_000_000_000_000)], 0),
            "1000000000000"
        );
        assert_eq!(label(&[range(0, b'r', "", 4000)], 0), "4000");
        assert_eq!(label(&[range(0, b'R', "", 3999)], 0), "MMMCMXCIX");
        assert_eq!(
            label(&[range(0, b'D', "", i64::MAX)], 3),
            i64::MAX.to_string()
        );
        assert_eq!(
            label(&[range(0, b'R', "", i64::MAX - 1)], 5),
            i64::MAX.to_string()
        );
        assert_eq!(label(&[range(0, b'A', "", i64::MAX)], 2).len(), 1000);
    }
}
