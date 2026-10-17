//! The document of a `TextEdit` and the colours a program gives stretches
//! of an editor's text (ARCHITECTURE §7.5).
//!
//! A program that highlights syntax knows which stretch is a keyword; the
//! editor knows where every character is. So the program hands over spans
//! ([`set_highlights`]), or a function that makes them from the text
//! ([`set_highlighter`]), and the editor paints its own layout in those
//! colours. A span changes how text looks and nothing else: not the text,
//! not what can be undone. Colours, a background and an underline are
//! painted over the layout the editor has. A weight, italic, strike-out or
//! another family change the glyphs, so the editor lays its text out again
//! with them, as Qt does for a `QTextCharFormat`; spans with colours only
//! never cost a layout.
//!
//! In Qt a `QSyntaxHighlighter` on the `QTextDocument` behind
//! `TextEdit.textDocument` does this from C++. Here the document is the
//! same handle; `highlight` and `clearHighlights` on it are ours.
//!
//! Nothing runs unless something changed: the highlighter is asked once
//! for each text the editor comes to show, and what it answered is kept
//! for as long as the layout it was painted on.

use std::collections::BTreeSet;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, ObjectId, QmlObject, Url, Value, qml_impl,
};
use ljos_render::{Decorations, GlyphRun, Rgba};
use ljos_text::{EditLayout, EditLine, StyleSpan, TextStyle, utf16_to_byte};

use super::base::{SharedState, bump, check, state_of};
use super::edit::TextEdit;
use super::ops;
use crate::rectangle::to_rgba;

/// One stretch of an editor's text and how it is painted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HighlightSpan {
    /// Bytes of the text (UTF-8). What reaches past the text is ignored.
    pub range: Range<usize>,
    /// The colour of the text; `None`: the editor's.
    pub color: Option<Color>,
    /// Painted behind the text and under the selection.
    pub background: Option<Color>,
    /// A line under the text, in its colour.
    pub underline: bool,
    /// A line through the text, in its colour.
    pub strikeout: bool,
    /// The weight of the font, 1 to 1000 as `font.weight` (`Font.Bold` is
    /// 700); `None`: the editor's.
    pub weight: Option<u16>,
    /// The text is set in italics.
    pub italic: bool,
    /// Another family; `None`: the editor's.
    pub family: Option<Arc<str>>,
}

impl HighlightSpan {
    fn paints(&self) -> bool {
        self.range.start < self.range.end
            && (self.color.is_some()
                || self.background.is_some()
                || self.underline
                || self.shapes())
    }

    /// The span changes the glyphs, so the text is laid out with it.
    pub(crate) fn shapes(&self) -> bool {
        self.strikeout || self.italic || self.weight.is_some() || self.family.is_some()
    }

    /// Everything but the range is the same.
    fn looks_like(&self, other: &Self) -> bool {
        self.color == other.color
            && self.background == other.background
            && self.underline == other.underline
            && self.strikeout == other.strikeout
            && self.weight == other.weight
            && self.italic == other.italic
            && self.family == other.family
    }
}

/// Makes the spans of a text. It runs on the UI thread each time the text
/// changes, so it must be quick: work that takes long belongs on another
/// thread, with the result handed to [`set_highlights`].
pub type Highlighter = Box<dyn FnMut(&str) -> Vec<HighlightSpan>>;

/// What an editor keeps of its highlighting.
#[derive(Default)]
pub(crate) struct Highlights {
    /// As they were given: a later span is painted over an earlier one.
    raw: Vec<HighlightSpan>,
    /// `raw` in order, with none overlapping; `None` until it is asked
    /// for.
    flat: Option<Rc<[HighlightSpan]>>,
    highlighter: Option<Highlighter>,
    /// Counts the highlighters that were set, so one that is replaced
    /// while it runs is not put back.
    generation: u64,
    /// The text changed since the highlighter read it.
    pub stale: bool,
    /// What was last painted, kept while it still holds.
    painted: Option<Painted>,
}

impl std::fmt::Debug for Highlights {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlights")
            .field("spans", &self.raw.len())
            .field("highlighter", &self.highlighter.is_some())
            .field("stale", &self.stale)
            .finish()
    }
}

/// `spans` in the order of the text, with none overlapping: where several
/// cover a stretch, each of colour, background, weight and family is that
/// of the last span that has it, and it is underlined, struck out or in
/// italics when one of them is.
fn flatten(spans: &[HighlightSpan]) -> Vec<HighlightSpan> {
    let spans: Vec<&HighlightSpan> = spans.iter().filter(|s| s.paints()).collect();
    if spans.windows(2).all(|w| w[0].range.end <= w[1].range.start) {
        // What a highlighter usually gives: nothing to work out.
        return spans.into_iter().cloned().collect();
    }
    let mut cuts: Vec<usize> = spans
        .iter()
        .flat_map(|s| [s.range.start, s.range.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let by = |key: fn(&HighlightSpan) -> usize| {
        let mut order: Vec<usize> = (0..spans.len()).collect();
        order.sort_by_key(|i| key(spans[*i]));
        order.into_iter().peekable()
    };
    let (mut starts, mut ends) = (by(|s| s.range.start), by(|s| s.range.end));
    // The spans that cover the stretch at hand, in the order they were
    // given.
    let mut covering: BTreeSet<usize> = BTreeSet::new();
    let mut out: Vec<HighlightSpan> = Vec::new();
    for w in cuts.windows(2) {
        while let Some(i) = ends.next_if(|i| spans[*i].range.end <= w[0]) {
            covering.remove(&i);
        }
        while let Some(i) = starts.next_if(|i| spans[*i].range.start <= w[0]) {
            covering.insert(i);
        }
        let on = || covering.iter().map(|i| spans[*i]);
        let piece = HighlightSpan {
            range: w[0]..w[1],
            color: on().filter_map(|s| s.color).next_back(),
            background: on().filter_map(|s| s.background).next_back(),
            underline: on().any(|s| s.underline),
            strikeout: on().any(|s| s.strikeout),
            weight: on().filter_map(|s| s.weight).next_back(),
            italic: on().any(|s| s.italic),
            family: on().filter_map(|s| s.family.clone()).next_back(),
        };
        if !piece.paints() {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.range.end == piece.range.start && last.looks_like(&piece) => {
                last.range.end = piece.range.end;
            }
            _ => out.push(piece),
        }
    }
    out
}

/// Where an input method's composition stands in what is shown (a byte)
/// and how long it is: spans count the bytes of the text, which the
/// composition is not part of.
pub(crate) type Shift = (usize, usize);

/// A span's bytes in what is shown.
fn shown(range: &Range<usize>, (at, by): Shift) -> Range<usize> {
    // A span that ends where the composition stands does not take it in;
    // one that reaches over it does.
    let start = if range.start >= at {
        range.start + by
    } else {
        range.start
    };
    let end = if range.end > at {
        range.end + by
    } else {
        range.end
    };
    start..end
}

/// What the spans that change the glyphs ask the layout of `base` for, in
/// what is shown. Empty when no span does: the text is laid out as it is.
pub(crate) fn style_spans(
    spans: &[HighlightSpan],
    base: &TextStyle,
    shift: Shift,
) -> Vec<StyleSpan> {
    spans
        .iter()
        .filter(|s| s.shapes())
        .zip(1..)
        .map(|(s, tag)| StyleSpan {
            weight: s.weight.map_or(base.weight, f32::from),
            italic: base.italic || s.italic,
            strikeout: base.strikeout || s.strikeout,
            family: s.family.clone(),
            ..StyleSpan::of(base, shown(&s.range, shift), tag)
        })
        .collect()
}

/// Glyphs of one colour: a run of the layout, or the part of one that a
/// span colours.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub glyphs: Arc<GlyphRun>,
    /// For every glyph, the byte its character starts at.
    pub bytes: Vec<usize>,
    /// `None`: the editor's colour.
    pub color: Option<Rgba>,
    pub decorations: Decorations,
}

/// How one line with spans on it is painted. Every x is from the line's
/// left edge.
#[derive(Clone, Debug, Default)]
pub(crate) struct LinePaint {
    pub backgrounds: Vec<(f64, f64, Rgba)>,
    /// `None`: in the editor's colour.
    pub underlines: Vec<(f64, f64, Option<Rgba>)>,
    pub pieces: Vec<Piece>,
}

/// By line; `None` for a line no span is on, which is painted as the
/// layout has it.
pub(crate) type Lines = Rc<Vec<Option<LinePaint>>>;

/// [`Lines`] and what they were made from.
struct Painted {
    layout: Rc<EditLayout>,
    spans: Rc<[HighlightSpan]>,
    shift: Shift,
    lines: Lines,
}

fn line_paint(line: &EditLine, spans: &[HighlightSpan], shift: Shift) -> Option<LinePaint> {
    // The spans on this line: they are in order and none overlaps.
    let first = spans.partition_point(|s| shown(&s.range, shift).end <= line.range.start);
    let count = spans[first..].partition_point(|s| shown(&s.range, shift).start < line.range.end);
    let on: Vec<(Range<usize>, &HighlightSpan)> = spans[first..first + count]
        .iter()
        .map(|s| (shown(&s.range, shift), s))
        .collect();
    if on.is_empty() {
        return None;
    }
    let mut out = LinePaint::default();
    for (range, span) in &on {
        if span.background.is_none() && !span.underline {
            continue;
        }
        for (x0, x1) in line.range_extents(range) {
            if let Some(c) = span.background {
                out.backgrounds.push((x0, x1, to_rgba(c)));
            }
            if span.underline {
                out.underlines.push((x0, x1, span.color.map(to_rgba)));
            }
        }
    }
    let color_at = |byte: usize| -> Option<Rgba> {
        let i = on.partition_point(|(range, _)| range.end <= byte);
        on.get(i)
            .filter(|(range, _)| range.start <= byte)
            .and_then(|(_, span)| span.color)
            .map(to_rgba)
    };
    for run in &line.runs {
        let colors: Vec<Option<Rgba>> = run.glyph_bytes.iter().map(|b| color_at(*b)).collect();
        let mut distinct: Vec<Option<Rgba>> = Vec::new();
        for c in &colors {
            if !distinct.contains(c) {
                distinct.push(*c);
            }
        }
        if distinct.len() <= 1 {
            out.pieces.push(Piece {
                glyphs: Arc::clone(&run.run.glyphs),
                bytes: run.glyph_bytes.clone(),
                color: distinct.first().copied().flatten(),
                decorations: run.run.decorations,
            });
            continue;
        }
        // An underline or strike-through of the font runs under the whole
        // run and is drawn once: with the text the editor colours, or with
        // the first part when a span colours all of it.
        let decorated = distinct.iter().position(Option::is_none).unwrap_or(0);
        for (n, color) in distinct.iter().enumerate() {
            let mut part = GlyphRun::clone(&run.run.glyphs);
            let mut i = 0;
            part.glyphs.retain(|_| {
                let keep = colors.get(i) == Some(color);
                i += 1;
                keep
            });
            let bytes = run
                .glyph_bytes
                .iter()
                .zip(&colors)
                .filter(|(_, c)| *c == color)
                .map(|(b, _)| *b)
                .collect();
            out.pieces.push(Piece {
                glyphs: Arc::new(part),
                bytes,
                color: *color,
                decorations: if n == decorated {
                    run.run.decorations
                } else {
                    Decorations::default()
                },
            });
        }
    }
    Some(out)
}

/// The spans of an editor as its picture is made: the highlighter is
/// asked first when the text changed since it last was.
pub(crate) fn current(st: &SharedState) -> Rc<[HighlightSpan]> {
    // The highlighter is a program's code. It runs with the state free, so
    // whatever it reaches finds the editor as it is.
    let job = {
        let mut s = st.borrow_mut();
        let s = &mut *s;
        let h = &mut s.highlights;
        if std::mem::take(&mut h.stale) {
            h.highlighter
                .take()
                .map(|f| (f, s.model.text().to_owned(), h.generation))
        } else {
            None
        }
    };
    if let Some((mut f, text, generation)) = job {
        let spans = f(&text);
        let mut s = st.borrow_mut();
        let h = &mut s.highlights;
        if h.generation == generation {
            h.highlighter = Some(f);
            h.raw = spans;
            h.flat = None;
        }
    }
    let mut s = st.borrow_mut();
    let h = &mut s.highlights;
    if h.flat.is_none() {
        h.flat = Some(flatten(&h.raw).into());
    }
    Rc::clone(h.flat.as_ref().expect("set above"))
}

/// How the lines with spans on them are painted, for the layout the editor
/// has now. `None` when no span is on the text, which is then painted as
/// the layout has it.
pub(crate) fn lines(st: &SharedState) -> Option<Lines> {
    let spans = current(st);
    let mut s = st.borrow_mut();
    let s = &mut *s;
    if spans.is_empty() || !s.shown.is_plain() {
        // What is masked is not the text the spans count in.
        s.highlights.painted = None;
        return None;
    }
    let layout = s.layout.as_ref()?;
    let pre = s.shown.preedit_bytes();
    let shift = (pre.start, pre.len());
    if let Some(p) = &s.highlights.painted
        && Rc::ptr_eq(&p.layout, layout)
        && Rc::ptr_eq(&p.spans, &spans)
        && p.shift == shift
    {
        // A blinking cursor paints the editor twice a second: the same
        // text in the same colours is not worked out again.
        return Some(Rc::clone(&p.lines));
    }
    let lines: Lines = Rc::new(
        layout
            .lines
            .iter()
            .map(|line| line_paint(line, &spans, shift))
            .collect(),
    );
    s.highlights.painted = Some(Painted {
        layout: Rc::clone(layout),
        spans,
        shift,
        lines: Rc::clone(&lines),
    });
    Some(lines)
}

/// The editor `obj` is, or is the document of.
fn editor_of(engine: &Engine, obj: ObjectId) -> Option<(ObjectId, SharedState)> {
    if let Some(st) = state_of(engine, obj) {
        return Some((obj, st));
    }
    let edit = engine
        .untrack(|| engine.with(obj, |d: &TextDocument| d.edit))
        .ok()??;
    state_of(engine, edit).map(|st| (edit, st))
}

/// Paints the editor again, and lays its text out again first when spans
/// that change the glyphs came or went (`shaped`: whether the spans it had
/// did).
fn repaint(engine: &Engine, st: &SharedState, shaped: bool) {
    let epochs = {
        let s = st.borrow();
        let shapes = shaped || s.highlights.raw.iter().any(HighlightSpan::shapes);
        [s.paint_epoch, s.shape_epoch.filter(|_| shapes)]
    };
    for n in epochs.into_iter().flatten() {
        bump(engine, n);
    }
}

fn shaped(st: &SharedState) -> bool {
    st.borrow().highlights.raw.iter().any(HighlightSpan::shapes)
}

/// Gives an editor the spans its text is painted with, in place of those
/// it had. `item` is a `TextInput`, a `TextEdit`, a type derived from one,
/// or the `textDocument` of a `TextEdit`. The spans are counted in the text
/// as it is now and stay where they are when it changes: a program that
/// highlights gives new ones then (on `textChanged`), or sets a
/// [`set_highlighter`]. Returns whether `item` is an editor.
pub fn set_highlights(engine: &Engine, item: ObjectId, spans: Vec<HighlightSpan>) -> bool {
    let Some((_, st)) = editor_of(engine, item) else {
        return false;
    };
    let was = shaped(&st);
    {
        let mut s = st.borrow_mut();
        s.highlights.raw = spans;
        s.highlights.flat = None;
    }
    repaint(engine, &st, was);
    true
}

/// Gives an editor the function that makes the spans of its text, or takes
/// it away with `None` (the spans it made go with it). The function is
/// asked once for every text the editor comes to show, and not again for a
/// selection, a blinking cursor or a new size. Returns whether `item` is
/// an editor.
pub fn set_highlighter(engine: &Engine, item: ObjectId, highlighter: Option<Highlighter>) -> bool {
    let Some((_, st)) = editor_of(engine, item) else {
        return false;
    };
    let was = shaped(&st);
    let highlighter_set = highlighter.is_some();
    {
        let mut s = st.borrow_mut();
        let h = &mut s.highlights;
        h.generation += 1;
        h.stale = highlighter.is_some();
        h.highlighter = highlighter;
        h.raw.clear();
        h.flat = None;
    }
    // What the new highlighter makes is not known until it runs.
    repaint(engine, &st, was || highlighter_set);
    true
}

/// The spans an editor's text is painted with, in the order of the text and
/// with none overlapping. Empty when `item` is not an editor.
pub fn highlights(engine: &Engine, item: ObjectId) -> Vec<HighlightSpan> {
    editor_of(engine, item).map_or_else(Vec::new, |(_, st)| current(&st).to_vec())
}

/// `TextEdit.textDocument` (doc.qt.io, `qml-qtquick-textdocument.html`):
/// the text of a `TextEdit` as something a program holds on to: whether it
/// is modified, and the file it is read from and saved to
/// ([`super::files`]). `highlight` and `clearHighlights` are ours.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "TextDocument",
    uncreatable,
    enums = super::files::document_enums
)]
pub struct TextDocument {
    /// Whether the text differs from what it was when it was assigned or
    /// when a program last wrote `false` here. Undoing back to that text
    /// makes it `false` again.
    #[qml(notify)]
    pub modified: bool,
    /// The file the text is read from; writing it reads the file.
    #[qml(notify)]
    pub source: Url,
    /// [`super::files::status`]: whether the file is being read or saved,
    /// was, or could not be.
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    /// Why the last read or save failed; empty when it did not.
    #[qml(notify, readonly)]
    pub error_string: Atom,
    #[qml(skip)]
    pub(crate) edit: Option<ObjectId>,
    /// Counts the reads and saves, so that the answer to one that was
    /// overtaken is dropped.
    #[qml(skip)]
    pub(crate) generation: u64,
}

fn color_of(v: &Value) -> Option<Color> {
    match v {
        Value::Color(c) => Some(*c),
        Value::String(_) => Color::parse(v.to_js_string().as_str()),
        _ => None,
    }
}

#[qml_impl]
impl TextDocument {
    /// `highlight(start, end, color, background)`: paints the text from
    /// position `start` to `end` in `color` on `background`, over what was
    /// highlighted before. Either colour may be left out or `undefined`:
    /// the text keeps its own, and has no background. Positions are cut to
    /// the text.
    #[qml(invokable)]
    fn highlight(cx: &Cx, start: i32, end: i32, color: Value, background: Value) {
        let Some((edit, _)) = editor_of(cx.engine, cx.this) else {
            return;
        };
        // What a program wrote to `text` just now is the text the
        // positions are in.
        let Some(st) = ops::ready(cx.engine, edit) else {
            return;
        };
        {
            let mut s = st.borrow_mut();
            let s = &mut *s;
            let text = s.model.text();
            let at = |p: i32| utf16_to_byte(text, usize::try_from(p).unwrap_or(0));
            let (a, b) = (at(start.min(end)), at(start.max(end)));
            s.highlights.raw.push(HighlightSpan {
                range: a..b,
                color: color_of(&color),
                background: color_of(&background),
                ..HighlightSpan::default()
            });
            s.highlights.flat = None;
        }
        repaint(cx.engine, &st, false);
    }

    /// `clearHighlights()`: the text is painted as the editor has it.
    #[qml(invokable, name = "clearHighlights")]
    fn clear_highlights(cx: &Cx) {
        set_highlights(cx.engine, cx.this, Vec::new());
    }

    /// `save()`: writes the text to `source`, in the background.
    #[qml(invokable)]
    fn save(cx: &Cx) {
        super::files::save(cx.engine, cx.this, None);
    }

    /// `saveAs(url)`: writes the text to `url`, which becomes `source`
    /// once it is written.
    #[qml(invokable, name = "saveAs")]
    fn save_as(cx: &Cx, url: Value) {
        let url = url.to_js_string();
        super::files::save(cx.engine, cx.this, Some(url.as_str()));
    }
}

/// Makes the document of a `TextEdit` that completes.
pub(crate) fn init_document(
    engine: &Engine,
    edit: ObjectId,
    st: &SharedState,
) -> Result<(), EngineError> {
    let doc = engine.create_native_incomplete(
        TextDocument {
            edit: Some(edit),
            ..TextDocument::default()
        },
        Some(edit),
    )?;
    st.borrow_mut().document = Some(doc);
    engine.set_internal(edit, TextEdit::TEXT_DOCUMENT, Some(doc))?;
    // A program that saved the text writes `false`.
    engine.watch(doc, TextDocument::MODIFIED.index, move |e, v| {
        let Some(st) = state_of(e, edit) else { return };
        let wanted = v.to_boolean();
        let changed = {
            let mut s = st.borrow_mut();
            let changed = s.model.is_modified() != wanted;
            if changed {
                s.model.set_modified(wanted);
                // What is typed next does not join what led to the text
                // that was saved.
                s.run = None;
                if !wanted {
                    super::rich::saved(&mut s);
                }
            }
            changed
        };
        if changed {
            sync_modified(e, &st);
        }
    })?;
    // A program names a file: it is read into the editor.
    engine.watch(doc, TextDocument::SOURCE.index, move |e, v| {
        let Some(st) = state_of(e, edit) else { return };
        let url = v.to_js_string();
        if st.borrow().doc_source == url.as_str() {
            // The editor said it itself, after `saveAs`.
            return;
        }
        super::files::load(e, doc, edit, &st, url.as_str());
    })?;
    Ok(())
}

/// Writes whether the text is modified to the document, after anything
/// that may have changed it.
pub(crate) fn sync_modified(engine: &Engine, st: &SharedState) {
    let (doc, modified) = {
        let s = st.borrow();
        (s.document, s.model.is_modified())
    };
    let Some(doc) = doc.filter(|d| engine.is_alive(*d)) else {
        return;
    };
    let p = TextDocument::MODIFIED.index;
    if engine.untrack(|| engine.read(doc, p)).to_boolean() != modified {
        check(
            engine,
            "modified",
            engine.write_derived(doc, p, Value::Bool(modified)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(range: Range<usize>, color: Option<u8>, background: Option<u8>) -> HighlightSpan {
        let c = |v: u8| Color::from_rgba8(v, v, v, 0xff);
        HighlightSpan {
            range,
            color: color.map(c),
            background: background.map(c),
            ..HighlightSpan::default()
        }
    }

    #[test]
    fn spans_in_order_are_kept_as_they_are() {
        let given = vec![span(0..2, Some(1), None), span(2..5, Some(2), None)];
        assert_eq!(flatten(&given), given);
    }

    #[test]
    fn a_span_that_paints_nothing_is_dropped() {
        let given = vec![
            span(3..3, Some(1), None),
            // A stretch that ends before it starts, as a program may give.
            span(Range { start: 5, end: 2 }, Some(1), None),
            span(0..4, None, None),
        ];
        assert_eq!(flatten(&given), Vec::new());
    }

    #[test]
    fn a_later_span_is_painted_over_an_earlier_one() {
        let given = vec![span(0..10, Some(1), Some(7)), span(4..6, Some(2), None)];
        assert_eq!(
            flatten(&given),
            vec![
                span(0..4, Some(1), Some(7)),
                // The later span has no background of its own, so the
                // earlier one's shows.
                span(4..6, Some(2), Some(7)),
                span(6..10, Some(1), Some(7)),
            ]
        );
    }

    #[test]
    fn spans_given_out_of_order_come_out_in_order() {
        let given = vec![span(6..8, Some(2), None), span(0..2, Some(1), None)];
        assert_eq!(
            flatten(&given),
            vec![span(0..2, Some(1), None), span(6..8, Some(2), None)]
        );
    }

    #[test]
    fn neighbours_painted_alike_are_one_span() {
        let given = vec![span(0..5, Some(1), None), span(3..8, Some(1), None)];
        assert_eq!(flatten(&given), vec![span(0..8, Some(1), None)]);
    }

    #[test]
    fn a_composition_moves_what_follows_it() {
        // Two bytes are being composed at byte 4.
        let shift = (4, 2);
        assert_eq!(shown(&(0..4), shift), 0..4);
        assert_eq!(shown(&(4..6), shift), 6..8);
        assert_eq!(shown(&(2..6), shift), 2..8);
        assert_eq!(shown(&(0..3), shift), 0..3);
    }
}
