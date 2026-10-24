//! The rich text of a `TextEdit` whose `textFormat` is `RichText` or
//! `MarkdownText` (or `AutoText` given markup).
//!
//! The model ([`ljos_text::TextModel`]) still holds the characters, the
//! cursor and the undo history, as for plain text; beside it a
//! [`RichText`] holds the same characters with their formats. Every edit
//! reaches the model first and is then told to the rich text, which gives
//! the new characters their format. Undo takes back formats too: the rich
//! text keeps itself as it was before each step of the model's history.
//!
//! `text` reads back as the document written out again (Qt's HTML, or its
//! Markdown). Writing it out costs as much as the document is long, so it
//! is done when something reads `text` after a change, not on every key
//! ([`written_out`], made on read by `base::publish`); only while
//! something observes `text` is it written out at each change.
//!
//! Paragraph margins, indents, list markers and tables are laid out as a
//! rich `Text` lays them out ([`edit_blocks`]); a table's rows stay its
//! lines, so a row with a cell that wraps is left as plain lines. Tables
//! are not written back as tables but as a paragraph per row. The
//! background colours of a part of the text and of a paragraph are
//! painted as a rich `Text` paints them, and a paragraph's is written back.

use std::collections::HashMap;
use std::sync::Arc;

use ljos_engine::{Color, Engine, Font, ObjectId, QmlType as _, Value};
use ljos_render::{ImageHandle, Rgba, kurbo};
use ljos_text::rich::{
    BlockFormat, CharFormat, DocFont, HAlign, ImageFormat, OBJECT, ParseOptions, RichText,
    TextFormat, might_be_rich,
};
use ljos_text::{BUNDLED_FAMILY, EditBlock, EditTable, byte_to_utf16, utf16_to_byte};

use super::base::{SharedState, State, TextEditor};
use super::edit::TextEdit;
use super::text_format;
use crate::rectangle::to_rgba;
use crate::text::{editor_css_color, editor_image, text_style};

/// A document asks for at most this many different images.
const MAX_IMAGES: usize = 256;

/// What a `TextEdit` keeps of its rich text.
pub(crate) struct RichEdit {
    pub format: TextFormat,
    /// Always holds the model's characters once [`sync_doc`] ran.
    pub doc: RichText,
    /// `text` as last written out.
    pub published: String,
    /// The document changed since it was written out.
    pub dirty: bool,
    options: ParseOptions,
    /// The document before each step of the model's undo history, and
    /// after each step that was undone.
    undo: Vec<RichText>,
    redo: Vec<RichText>,
    /// The next change of the text starts a step of the model's history.
    pub fresh_step: bool,
    /// Where the edit being made starts (a byte of the text before it):
    /// where the characters are told apart when a run of equal ones makes
    /// that ambiguous.
    hint: Option<usize>,
    /// The fragment a program inserts, with its formats.
    pub pending: Option<RichText>,
    /// The format the user gave the cursor without a selection: what is
    /// typed next at that byte.
    pub typing: Option<(usize, Arc<CharFormat>)>,
    /// The pictures of the images by their source as written; `None` for
    /// one that cannot be shown.
    pub images: HashMap<Arc<str>, Option<ImageHandle>>,
    /// The link a press landed on.
    pub pressed: Option<Arc<str>>,
    /// The colour of each run, by the run's index.
    colors: Option<Arc<Vec<Option<Rgba>>>>,
    /// The colour behind each run, by the run's index.
    backgrounds: Option<Arc<Vec<Option<Rgba>>>>,
    /// The colour behind each paragraph, by the paragraph's index.
    block_backgrounds: Option<Arc<Vec<Option<Rgba>>>>,
}

impl std::fmt::Debug for RichEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RichEdit")
            .field("format", &self.format)
            .field("len", &self.doc.text().len())
            .field("dirty", &self.dirty)
            .field("undo", &self.undo.len())
            .field("redo", &self.redo.len())
            .finish()
    }
}

/// How a text in `text_format` is read; `None`: as plain text.
pub(crate) fn format_of(text_format: i32, text: &str) -> Option<TextFormat> {
    match text_format {
        text_format::RICH_TEXT => Some(TextFormat::Rich),
        text_format::MARKDOWN_TEXT => Some(TextFormat::Markdown),
        text_format::AUTO_TEXT if might_be_rich(text) => Some(TextFormat::Rich),
        _ => None,
    }
}

/// A colour as the text keeps it: `#rrggbb`, with its alpha when it has
/// one.
pub(crate) fn color_name(c: Color) -> Arc<str> {
    let [r, g, b, a] = c.to_rgba8();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}").into()
    } else {
        let alpha = f64::from(a) / 255.0;
        format!("rgba({r},{g},{b},{alpha})").into()
    }
}

fn resolve_color(written: &str) -> Option<Arc<str>> {
    editor_css_color(written).map(color_name)
}

/// The colour a run's format names.
pub(crate) fn color_of(f: &CharFormat) -> Option<Color> {
    f.inline.color.as_deref().and_then(editor_css_color)
}

/// Measured on Qt 6.11 (Linux): two thirds of the point size between
/// paragraphs of Markdown, and nothing for a font sized in pixels. Taken
/// as half the point size in pixels, which is the same at 96 dpi and
/// follows a platform's own DPI.
fn options(font: &Font) -> ParseOptions {
    let paragraph_spacing = if font.pixel_size > 0 {
        0.0
    } else if font.point_size > 0.0 {
        (ljos_runtime::points_to_pixels(font.point_size) / 2.0) as f32
    } else {
        7.0
    };
    ParseOptions { paragraph_spacing }
}

/// The item's font (untracked).
pub(crate) fn font_of(engine: &Engine, obj: ObjectId) -> Arc<Font> {
    match engine.untrack(|| engine.read(obj, TextEditor::FONT.index)) {
        Value::Font(f) => f,
        _ => Arc::default(),
    }
}

/// What assigning `text` needs besides the text (untracked): how it is
/// read and the font. A `TextInput` is always plain.
pub(crate) fn inputs(
    engine: &Engine,
    obj: ObjectId,
    multi: bool,
    text: &str,
) -> (Option<TextFormat>, Arc<Font>) {
    if !multi {
        return (None, Arc::default());
    }
    let f = engine
        .untrack(|| engine.read(obj, TextEdit::TEXT_FORMAT.index))
        .to_number() as i32;
    (format_of(f, text), font_of(engine, obj))
}

/// Whether `text`, as a program wrote it, is other than what the editor
/// holds.
pub(crate) fn differs(s: &State, text: &str) -> bool {
    match &s.rich {
        Some(r) => r.published != text,
        None => s.model.text() != text,
    }
}

/// Makes `source` the editor's text, read as `format`. The history goes,
/// as for plain text.
pub(crate) fn assign(s: &mut State, format: Option<TextFormat>, source: &str, font: &Font) {
    let Some(format) = format else {
        s.rich = None;
        s.model.set_text(source);
        return;
    };
    let options = options(font);
    let doc = RichText::parse(format, source, &options, &resolve_color);
    if !s.model.set_text(doc.text()) {
        // The characters are the same and the formats may not be: it is a
        // new text all the same.
        s.model.clear_undo();
        s.model.set_modified(false);
    }
    let images = s.rich.take().map(|r| r.images).unwrap_or_default();
    s.rich = Some(RichEdit {
        format,
        doc,
        published: source.to_owned(),
        dirty: true,
        options,
        undo: Vec::new(),
        redo: Vec::new(),
        fresh_step: true,
        hint: None,
        pending: None,
        typing: None,
        images,
        pressed: None,
        colors: None,
        backgrounds: None,
        block_backgrounds: None,
    });
    s.layout = None;
}

/// Reads `source` as the editor's format, for `insert` and `append`.
pub(crate) fn fragment(st: &SharedState, source: &str) -> Option<RichText> {
    let s = st.borrow();
    let r = s.rich.as_ref()?;
    Some(RichText::parse(
        r.format,
        source,
        &r.options,
        &resolve_color,
    ))
}

/// What a copy puts on the clipboard beside the characters: the selection
/// as HTML. A plain `TextEdit` copies HTML too, of text without formats,
/// so that pasted into rich text it does not take the format where it
/// goes; a `TextInput` copies only the characters
/// (features/text-input/rich-text-clipboard.feature).
pub(crate) fn copied_html(s: &mut State) -> Option<String> {
    if !s.multi {
        return None;
    }
    sync_doc(s);
    let range = s.model.selection_bytes();
    if range.is_empty() {
        return None;
    }
    // DECISION: of a selection of cells the HTML is of the characters
    // from one end to the other, which takes in the cells; not measured.
    Some(match &s.rich {
        Some(r) => r.doc.html_fragment(range),
        None => {
            let part = RichText::plain(&s.model.text()[range]);
            part.html_fragment(0..part.text().len())
        }
    })
}

/// What a rich editor pastes of the clipboard's `html`, with its formats;
/// `None` when the editor is plain or the HTML has no characters.
pub(crate) fn pasted(st: &SharedState, html: &str) -> Option<RichText> {
    let s = st.borrow();
    let r = s.rich.as_ref()?;
    // Whatever the editor writes, what is pasted is read as HTML: a
    // Markdown editor takes the formats as well.
    let doc = RichText::parse(TextFormat::Rich, html, &r.options, &resolve_color);
    (!doc.text().is_empty()).then_some(doc)
}

/// An edit is about to be made, starting at `start` (UTF-16; `None`: at
/// the selection). `breaks`: it starts a step of the history.
pub(crate) fn before_edit(s: &mut State, start: Option<usize>, breaks: bool) {
    let Some(r) = s.rich.as_mut() else { return };
    if breaks {
        r.fresh_step = true;
    }
    let text = s.model.text();
    r.hint = Some(match start {
        Some(p) => utf16_to_byte(text, p),
        None => s.model.selection_bytes().start,
    });
}

/// Removes each of `ranges` (bytes, in order, none overlapping) as one
/// step of the history, the last first, so that the rich text is told of
/// each on its own.
pub(crate) fn remove_each(s: &mut State, ranges: &[std::ops::Range<usize>]) {
    if let Some(r) = s.rich.as_mut() {
        r.fresh_step = true;
    }
    for range in ranges.iter().rev().filter(|r| !r.is_empty()) {
        let text = s.model.text();
        let (a, b) = (
            byte_to_utf16(text, range.start),
            byte_to_utf16(text, range.end),
        );
        if let Some(r) = s.rich.as_mut() {
            r.hint = Some(range.start);
        }
        s.model.remove(a, b);
        sync_doc(s);
    }
}

/// The edit was made: tells the rich text.
pub(crate) fn after_edit(s: &mut State) {
    sync_doc(s);
    if let Some(r) = s.rich.as_mut() {
        r.pending = None;
        r.hint = None;
    }
}

/// Takes the model's characters into the rich text: what differs between
/// the two is what the last edit did.
pub(crate) fn sync_doc(s: &mut State) {
    let Some(r) = s.rich.as_mut() else { return };
    let new = s.model.text();
    let old = r.doc.text();
    if old == new {
        return;
    }
    let mut start = same_start(old.as_bytes(), new.as_bytes());
    if let Some(hint) = r.hint.take() {
        start = start.min(hint);
    }
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let room = (old.len() - start).min(new.len() - start);
    let mut tail = same_end(
        &old.as_bytes()[old.len() - room..],
        &new.as_bytes()[new.len() - room..],
    );
    while !old.is_char_boundary(old.len() - tail) || !new.is_char_boundary(new.len() - tail) {
        tail -= 1;
    }
    let removed = start..old.len() - tail;
    let inserted = new[start..new.len() - tail].to_owned();
    if r.fresh_step {
        r.undo.push(r.doc.clone());
        r.fresh_step = false;
    }
    r.redo.clear();
    match r.pending.take() {
        Some(frag) if frag.text() == inserted => r.doc.splice_rich(removed, &frag),
        _ => {
            let typed = r
                .typing
                .take()
                .filter(|(at, _)| *at == removed.start && removed.is_empty())
                .map(|(_, f)| f);
            r.doc.splice(removed, &inserted, typed);
        }
    }
    r.typing = None;
    changed(s);
}

/// Compared a block at a time: the texts are as long as the document, and a
/// key changes one place in them.
const BLOCK: usize = 32;

/// How many bytes `a` and `b` start with in common.
fn same_start(a: &[u8], b: &[u8]) -> usize {
    let n = a.len().min(b.len());
    let mut i = 0;
    while i + BLOCK <= n && a[i..i + BLOCK] == b[i..i + BLOCK] {
        i += BLOCK;
    }
    while i < n && a[i] == b[i] {
        i += 1;
    }
    i
}

/// How many bytes `a` and `b` end with in common.
fn same_end(a: &[u8], b: &[u8]) -> usize {
    let (la, lb) = (a.len(), b.len());
    let n = la.min(lb);
    let mut i = 0;
    while i + BLOCK <= n && a[la - i - BLOCK..la - i] == b[lb - i - BLOCK..lb - i] {
        i += BLOCK;
    }
    while i < n && a[la - i - 1] == b[lb - i - 1] {
        i += 1;
    }
    i
}

/// The document changed: written out again, painted and laid out again.
fn changed(s: &mut State) {
    if let Some(r) = s.rich.as_mut() {
        r.dirty = true;
        r.colors = None;
        r.backgrounds = None;
        r.block_backgrounds = None;
    }
    s.layout = None;
}

/// The model undid (`undo`) or redid a step: the formats go with it.
pub(crate) fn history_swap(s: &mut State, undo: bool) {
    let Some(r) = s.rich.as_mut() else { return };
    let popped = if undo { r.undo.pop() } else { r.redo.pop() };
    if let Some(doc) = popped {
        let now = std::mem::replace(&mut r.doc, doc);
        if undo {
            r.redo.push(now);
        } else {
            r.undo.push(now);
        }
    }
    r.fresh_step = true;
    r.typing = None;
    if r.doc.text() != s.model.text() {
        // The two histories went apart (an edit the rich text was not told
        // of): the characters are the model's, the formats what fits.
        let keep = std::mem::take(&mut r.fresh_step);
        sync_doc(s);
        if let Some(r) = s.rich.as_mut() {
            r.fresh_step = keep;
        }
    }
    changed(s);
}

/// The history says the text as it is now was saved: what is typed next
/// starts a step.
pub(crate) fn saved(s: &mut State) {
    if let Some(r) = s.rich.as_mut() {
        r.fresh_step = true;
    }
}

/// Backspace at the start of a list item, or of a paragraph that was one
/// and keeps its indent: the paragraph's format steps out as one step of
/// the history, and the text stays. Whether it did.
pub(crate) fn outdent(s: &mut State) -> bool {
    sync_doc(s);
    let Some(r) = s.rich.as_ref() else {
        return false;
    };
    let at = s.model.cursor_byte();
    if s.model.has_selection() || r.doc.text() != s.model.text() {
        return false;
    }
    let i = r.doc.block_index(at);
    if r.doc.block_range(i).start != at || r.doc.blocks()[i].outdented().is_none() {
        return false;
    }
    format_step(s, |doc, range| {
        doc.set_block_format(range, |b| b.outdented().unwrap_or_else(|| b.clone()));
    });
    true
}

/// Changes the format of the selection, or of the paragraph the cursor is
/// in, as one step of the history. `f` gets the text and the bytes.
pub(crate) fn format_step(s: &mut State, f: impl FnOnce(&mut RichText, std::ops::Range<usize>)) {
    if s.rich.is_none() {
        return;
    }
    sync_doc(s);
    let range = s.model.selection_bytes();
    let (anchor, cursor) = (s.model.anchor(), s.model.cursor());
    let units = s.model.selection();
    let Some(r) = s.rich.as_mut() else { return };
    let before = r.doc.clone();
    f(&mut r.doc, range);
    if r.doc == before {
        return;
    }
    r.undo.push(before);
    r.redo.clear();
    r.fresh_step = true;
    r.typing = None;
    // The model has a step too, one that leaves the characters as they
    // are, so that undo takes the two back together.
    let span = if units.is_empty() {
        let p = s.model.paragraph_start(units.start);
        p..s.model.paragraph_end(units.start)
    } else {
        units
    };
    let same = s.model.slice(span.start, span.end).to_owned();
    s.model.break_undo();
    if span.is_empty() {
        // An empty paragraph: a step that inserts and takes away nothing
        // still has to be one.
        s.model.insert(span.start, "\u{200b}");
        s.model.remove(span.start, span.start + 1);
    } else {
        s.model.replace(span, &same);
    }
    s.model.break_undo();
    s.model.select(anchor, cursor);
    s.run = None;
    changed(s);
}

/// What the document is written out as.
fn export(r: &RichEdit, font: &Font) -> String {
    write_out(&r.doc, r.format, font)
}

/// `doc` written out as `format`; HTML names `font` as the document's.
pub(crate) fn write_out(doc: &RichText, format: TextFormat, font: &Font) -> String {
    match format {
        TextFormat::Markdown => doc.markdown(),
        _ => {
            let style = text_style(font);
            let family = if font.family.is_empty() {
                BUNDLED_FAMILY.to_owned()
            } else {
                font.family.as_str().to_owned()
            };
            doc.html(&DocFont {
                family,
                pixel_size: style.pixel_size,
                weight: style.weight,
                italic: style.italic,
            })
        }
    }
}

/// Brings the rich text up to date before the properties are written:
/// takes in an edit it was not told of and finds the pictures of new
/// images. The text is written out by [`written_out`].
pub(crate) fn refresh(engine: &Engine, obj: ObjectId, st: &SharedState) {
    {
        let mut s = st.borrow_mut();
        if s.rich.is_none() {
            return;
        }
        sync_doc(&mut s);
    }
    load_images(engine, obj, st);
}

/// The text of a byte range as `getText` and `selectedText` give it: of
/// rich text with paragraphs, cells and tables marked as Qt marks them.
pub(crate) fn plain_text(s: &State, bytes: std::ops::Range<usize>) -> String {
    let text = s.model.text();
    match &s.rich {
        Some(r) if r.doc.text() == text => r.doc.plain_text(bytes),
        Some(_) => RichText::plain_of(&text[bytes]),
        None => text[bytes].to_owned(),
    }
}

/// `text` as the document is now: written out again when it changed since
/// the last time.
pub(crate) fn written_out(engine: &Engine, obj: ObjectId, st: &SharedState) -> String {
    let dirty = st.borrow().rich.as_ref().is_some_and(|r| r.dirty);
    if dirty {
        let font = font_of(engine, obj);
        let mut s = st.borrow_mut();
        sync_doc(&mut s);
        if let Some(r) = s.rich.as_mut() {
            r.published = export(r, &font);
            r.dirty = false;
        }
    }
    st.borrow()
        .rich
        .as_ref()
        .map(|r| r.published.clone())
        .unwrap_or_default()
}

/// The font changed: the HTML names it.
pub(crate) fn font_changed(st: &SharedState) {
    let mut s = st.borrow_mut();
    if let Some(r) = s.rich.as_mut()
        && r.format == TextFormat::Rich
    {
        r.dirty = true;
    }
}

/// Finds the pictures of images the text has and the editor has not asked
/// for. Nothing is borrowed while they are found.
fn load_images(engine: &Engine, obj: ObjectId, st: &SharedState) {
    let wanted: Vec<Arc<str>> = {
        let s = st.borrow();
        let Some(r) = s.rich.as_ref() else { return };
        if !r.doc.text().contains(OBJECT) {
            return;
        }
        let mut wanted = Vec::new();
        for (_, f) in r.doc.runs() {
            if let Some(image) = &f.image
                && !r.images.contains_key(&image.source)
                && !wanted.contains(&image.source)
                && r.images.len() + wanted.len() < MAX_IMAGES
            {
                wanted.push(Arc::clone(&image.source));
            }
        }
        wanted
    };
    if wanted.is_empty() {
        return;
    }
    let base = engine
        .untrack(|| engine.read(obj, TextEdit::BASE_URL.index))
        .to_js_string();
    let found: Vec<_> = wanted
        .into_iter()
        .map(|source| {
            let image = editor_image(engine, obj, base.as_str(), &source);
            (source, image)
        })
        .collect();
    let mut s = st.borrow_mut();
    if let Some(r) = s.rich.as_mut() {
        r.images.extend(found);
    }
    s.layout = None;
}

/// How large an image is shown: as the document says, in the picture's
/// own shape when it gives one side only, else as large as the picture,
/// and a placeholder for a picture that cannot be shown.
pub(crate) fn image_size(
    images: &HashMap<Arc<str>, Option<ImageHandle>>,
    image: &ImageFormat,
) -> (f32, f32) {
    let natural = images
        .get(&image.source)
        .and_then(Option::as_ref)
        .map(|h| (h.width() as f32, h.height() as f32))
        .filter(|(w, h)| *w > 0.0 && *h > 0.0);
    match (image.width, image.height, natural) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some((nw, nh))) => (w, w * nh / nw),
        (None, Some(h), Some((nw, nh))) => (h * nw / nh, h),
        (None, None, Some(natural)) => natural,
        _ => image.placeholder_size(),
    }
}

/// The colour of every run, by its index (a layout run's tag less one);
/// `None` where the text has the editor's colour.
pub(crate) fn run_colors(s: &mut State) -> Option<Arc<Vec<Option<Rgba>>>> {
    let r = s.rich.as_mut()?;
    if r.colors.is_none() {
        let link = to_rgba(Color::from_rgba8(0, 0, 0xff, 0xff));
        let colors = r
            .doc
            .runs()
            .map(|(_, f)| match color_of(f) {
                Some(c) => Some(to_rgba(c)),
                None if f.inline.link.is_some() => Some(link),
                None => None,
            })
            .collect();
        r.colors = Some(Arc::new(colors));
    }
    r.colors.clone()
}

/// The colour behind each run of the document, by the run's index; none
/// for a run with no background or a transparent one.
pub(crate) fn run_backgrounds(s: &mut State) -> Option<Arc<Vec<Option<Rgba>>>> {
    let r = s.rich.as_mut()?;
    if r.backgrounds.is_none() {
        let backgrounds = r
            .doc
            .runs()
            .map(|(_, f)| {
                f.inline
                    .background
                    .as_deref()
                    .and_then(editor_css_color)
                    .filter(|c| c.a != 0)
                    .map(to_rgba)
            })
            .collect();
        r.backgrounds = Some(Arc::new(backgrounds));
    }
    r.backgrounds.clone()
}

/// The colour behind each paragraph of rich text, when one has one.
pub(crate) fn block_backgrounds(s: &mut State) -> Option<Arc<Vec<Option<Rgba>>>> {
    let r = s.rich.as_mut()?;
    if r.block_backgrounds.is_none() {
        let backgrounds: Vec<_> = r.doc.blocks().iter().map(|b| block_background(b)).collect();
        r.block_backgrounds = Some(Arc::new(backgrounds));
    }
    r.block_backgrounds
        .clone()
        .filter(|b| b.iter().any(Option::is_some))
}

fn block_background(b: &BlockFormat) -> Option<Rgba> {
    b.background
        .as_deref()
        .and_then(editor_css_color)
        .filter(|c| c.a != 0)
        .map(to_rgba)
}

/// Where the pictures of the images are drawn, in the item.
pub(crate) fn placed_images(s: &State) -> Vec<(ImageHandle, kurbo::Rect)> {
    let (Some(r), Some(layout)) = (s.rich.as_ref(), s.layout.as_ref()) else {
        return Vec::new();
    };
    let text = r.doc.text();
    if !text.contains(OBJECT) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (range, f) in r.doc.runs() {
        let Some(image) = &f.image else { continue };
        let Some(Some(picture)) = r.images.get(&image.source) else {
            continue;
        };
        let (w, h) = image_size(&r.images, image);
        for (i, c) in text[range.clone()].char_indices() {
            if c != OBJECT {
                continue;
            }
            let pos = ljos_text::byte_to_utf16(text, range.start + i);
            let caret = layout.caret(s.shown.byte_of(pos));
            let Some(line) = layout.lines.get(caret.line) else {
                continue;
            };
            let x = s.geo.line_x.get(caret.line).copied().unwrap_or(0.0) + caret.x;
            let bottom = s.geo.top + line.baseline;
            out.push((
                picture.clone(),
                kurbo::Rect::new(x, bottom - f64::from(h), x + f64::from(w), bottom),
            ));
        }
    }
    out
}

/// How each paragraph stands apart and in, when one does: its margins,
/// indents and list marker are laid out as a rich `Text` lays them out.
pub(crate) fn edit_blocks(r: &RichEdit) -> Option<Vec<EditBlock>> {
    let blocks = r.doc.blocks();
    let plain = |b: &BlockFormat| {
        b.margin_top == 0.0
            && b.margin_bottom == 0.0
            && b.indent == 0.0
            && b.indent_right == 0.0
            && b.marker.is_none()
            && block_background(b).is_none()
            && b.table.is_none()
    };
    if blocks.iter().all(|b| plain(b)) {
        return None;
    }
    let mut row = 0;
    Some(
        blocks
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let same_table = i > 0
                    && blocks[i - 1]
                        .table
                        .as_ref()
                        .zip(b.table.as_ref())
                        .is_some_and(|(a, b)| a.same_table(b));
                row = if same_table { row + 1 } else { 0 };
                EditBlock {
                    margin_top: f64::from(b.margin_top),
                    margin_bottom: f64::from(b.margin_bottom),
                    indent: f64::from(b.indent),
                    indent_right: f64::from(b.indent_right),
                    marker: b.marker.clone(),
                    table: b.table.as_ref().map(|t| EditTable {
                        row,
                        border: f64::from(t.table.border),
                        spacing: f64::from(t.table.spacing),
                        padding: f64::from(t.table.padding),
                        background: t.table.background.clone(),
                        cells: t.cells.iter().map(|c| c.background.clone()).collect(),
                    }),
                }
            })
            .collect(),
    )
}

/// The alignment of each paragraph, for the lines of a `TextEdit`.
pub(crate) fn block_aligns(s: &State) -> Option<Vec<Option<HAlign>>> {
    let r = s.rich.as_ref()?;
    Some(r.doc.blocks().iter().map(|b| b.align).collect())
}

/// The link under a point of the item.
pub(crate) fn link_at(st: &SharedState, at: kurbo::Point) -> Option<Arc<str>> {
    let s = st.borrow();
    let r = s.rich.as_ref()?;
    let layout = s.layout.as_ref()?;
    let y = at.y - s.geo.top;
    if y < 0.0 || y >= layout.content_height {
        return None;
    }
    let index = layout.line_at(y);
    let line = layout.lines.get(index)?;
    let x = at.x - s.geo.line_x.get(index).copied().unwrap_or(0.0);
    if x < 0.0 || x > line.width {
        return None;
    }
    let pos = s.shown.pos_of(layout.hit_line(index, x, true));
    r.doc.link_at(utf16_to_byte(s.model.text(), pos))
}

fn set_hovered(engine: &Engine, obj: ObjectId, href: &str) {
    let p = TextEdit::HOVERED_LINK.index;
    let old = engine.untrack(|| engine.read(obj, p)).to_js_string();
    if old.as_str() == href {
        return;
    }
    let href = ljos_engine::Atom::from(href);
    if crate::item::write_computed(engine, obj, p, Value::String(href.clone())).is_ok() {
        let _ = engine.emit_by_name(obj, "linkHovered", &[Value::String(href)]);
    }
}

/// The pointer moved over a `TextEdit`.
pub(crate) fn hover_moved(engine: &Engine, obj: ObjectId, at: kurbo::Point) {
    let Some(st) = super::base::state_of(engine, obj) else {
        return;
    };
    if st.borrow().rich.is_none() {
        return;
    }
    let link = link_at(&st, at);
    set_hovered(engine, obj, link.as_deref().unwrap_or(""));
}

/// The pointer left an editor: it is over no link.
pub(crate) fn hover(engine: &Engine, obj: ObjectId, over: bool) {
    if over
        || !engine
            .meta_of(obj)
            .is_some_and(|m| m.inherits(&ljos_engine::MetaRef::Static(TextEdit::static_meta())))
    {
        return;
    }
    set_hovered(engine, obj, "");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ljos_text::TextModel;

    fn state(source: &str) -> State {
        let mut s = State {
            multi: true,
            model: TextModel::new(),
            ..State::default()
        };
        assign(&mut s, Some(TextFormat::Rich), source, &Font::default());
        s
    }

    fn bold_at(s: &State, byte: usize) -> bool {
        s.rich.as_ref().unwrap().doc.format_at(byte).inline.bold
    }

    #[test]
    fn typing_keeps_the_format_of_the_character_before() {
        let mut s = state("a<b>bold</b>c");
        assert_eq!(s.model.text(), "aboldc");
        s.model.move_to(3, false);
        before_edit(&mut s, None, true);
        s.model.replace_selection("x");
        after_edit(&mut s);
        assert_eq!(s.rich.as_ref().unwrap().doc.text(), "aboxldc");
        assert!(bold_at(&s, 3));
    }

    #[test]
    fn a_run_of_equal_characters_is_told_apart_by_where_the_edit_was() {
        let mut s = state("<b>a</b>a");
        s.model.move_to(1, false);
        before_edit(&mut s, None, true);
        s.model.replace_selection("a");
        after_edit(&mut s);
        // Typed after the bold "a", so bold, whatever the diff would say.
        assert!(bold_at(&s, 1));
        assert!(!bold_at(&s, 2));
    }

    #[test]
    fn undo_takes_the_formats_back() {
        let mut s = state("ab");
        s.model.select(0, 1);
        format_step(&mut s, |doc, range| {
            doc.set_char_format(range, |f| {
                let mut f = f.clone();
                f.inline.bold = true;
                f
            });
        });
        assert!(bold_at(&s, 0));
        assert!(s.model.undo());
        history_swap(&mut s, true);
        assert!(!bold_at(&s, 0));
        assert!(s.model.redo());
        history_swap(&mut s, false);
        assert!(bold_at(&s, 0));
    }

    #[test]
    fn an_edit_the_rich_text_was_not_told_of_is_taken_in() {
        let mut s = state("<i>ab</i>");
        s.model.move_to(2, false);
        s.model.replace_selection("c");
        sync_doc(&mut s);
        assert_eq!(s.rich.as_ref().unwrap().doc.text(), "abc");
        assert!(s.rich.as_ref().unwrap().doc.format_at(2).inline.italic);
    }

    #[test]
    fn colours_are_kept_as_written_by_qt() {
        assert_eq!(&*color_name(Color::from_rgba8(255, 0, 0, 255)), "#ff0000");
        assert_eq!(resolve_color("red").as_deref(), Some("#ff0000"));
    }
}
