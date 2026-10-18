//! The document of a `Text` whose text is more than plain.
//!
//! The item keeps a [`RichDoc`]: the blocks of the text, each with its own
//! layout. Setting the text again reads only what changed (for Markdown
//! that grew at its end, only its last block) and lays out only blocks that
//! look different, so an answer that arrives a few characters at a time
//! costs a paragraph a piece, not the document.
//!
//! A document is untrusted input. Nothing in it runs. The only thing it can
//! make the program read is an image, and only one that is already here: a
//! regular file of a sane size, a `data:` address, or a resource the host
//! registered. An address with any other scheme is not asked for at all.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Atom, Color, Engine, ObjectId, Value};
use ljos_reactive::NodeId;
use ljos_render::{Decorations, DisplayListBuilder, ImageHandle, Rgba, kurbo};
use ljos_text::TextEngine;
use ljos_text::rich::{
    Block, DocMetrics, DocParams, HAlign, InlineImage, ParseOptions, RichDoc, TextFormat, View,
    default_image_size,
};

use super::{Text, TextInputs, vertical_offset};
use crate::enums::align;
use crate::image::{ImageKey, ImageStore, Request};
use crate::item::{Item, num};
use crate::rectangle::to_rgba;

/// The room between two blocks of Markdown. Measured on qml6: 7 pixels at
/// every font size for the text a `Text` is created with (Qt works it out
/// from the application's font, and from the item's own font only when the
/// text is set again; here it does not depend on when the text was set).
const OPTIONS: ParseOptions = ParseOptions {
    paragraph_spacing: 7.0,
};

/// A file larger than this is not read as an inline image: the document
/// chose the file, and it is decoded on the UI thread.
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// A document asks for at most this many different images.
const MAX_IMAGES: usize = 256;

pub(crate) type Shared = Rc<RefCell<RichState>>;

/// What a `Text` keeps besides its numbers once its text is a document.
#[derive(Default)]
pub(crate) struct RichState {
    pub(crate) doc: RichDoc,
    /// Kept beside the document: the pointer asks on every move.
    pub(crate) has_links: bool,
    /// The images of the document by their address as written: `None` for
    /// one that cannot be shown.
    images: HashMap<Arc<str>, Option<ImageHandle>>,
    /// What the addresses in `images` were resolved against.
    base: Atom,
    /// Bumped when the picture changes without a property changing.
    epoch: Option<NodeId>,
    /// The link a press landed on.
    pub(crate) pressed: Option<Arc<str>>,
    /// The link the keyboard is on, counted in reading order.
    pub(crate) key_link: Option<usize>,
    /// Whether the keyboard is on the item, so that its coming is seen
    /// once.
    pub(crate) keyboard: bool,
    /// What a `SelectionScope` has selected of the document.
    pub(crate) selection: Option<std::ops::Range<usize>>,
}

impl std::fmt::Debug for RichState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RichState")
            .field("blocks", &self.doc.blocks().len())
            .field("images", &self.images.len())
            .field("key_link", &self.key_link)
            .finish_non_exhaustive()
    }
}

/// A `Text`'s document, as its layout inputs carry it. Two are equal when
/// they are the same document: what it holds is told by the text and the
/// format beside it.
#[derive(Clone)]
pub struct RichHandle(pub(crate) Shared);

impl PartialEq for RichHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for RichHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RichHandle")
    }
}

impl RichHandle {
    pub(crate) fn has_links(&self) -> bool {
        self.0.borrow().has_links
    }

    /// What a `SelectionScope` has selected of the document.
    pub(crate) fn selection(&self) -> Option<std::ops::Range<usize>> {
        self.0.borrow().selection.clone()
    }

    /// The keyboard left the item: it is on no link.
    pub(crate) fn leave_keyboard(&self) {
        let mut s = self.0.borrow_mut();
        s.key_link = None;
        s.keyboard = false;
    }

    /// Lays out what `t` changed and says what the document measures.
    pub(crate) fn layout(&self, te: &mut TextEngine, t: &TextInputs) -> DocMetrics {
        self.0.borrow_mut().layout(te, t)
    }
}

impl RichState {
    fn layout(&mut self, te: &mut TextEngine, t: &TextInputs) -> DocMetrics {
        let params = DocParams {
            style: t.style.clone(),
            layout: t.params,
        };
        let images = &self.images;
        self.doc
            .layout(te, &params, &mut |image| image_size(images, image))
    }
}

/// How large an image is shown: as the document says, in the picture's own
/// shape when it gives one side only, else as large as the picture is, and
/// a small placeholder for a picture that cannot be shown.
fn image_size(images: &HashMap<Arc<str>, Option<ImageHandle>>, image: &InlineImage) -> (f32, f32) {
    let natural = images
        .get(&image.source)
        .and_then(Option::as_ref)
        .map(|h| (h.width() as f32, h.height() as f32))
        .filter(|(w, h)| *w > 0.0 && *h > 0.0);
    match (image.width, image.height, natural) {
        (Some(w), Some(h), _) => (w, h),
        // Measured on Qt 6.11.2: StyledText takes the size a document gives
        // only when it gives both sides.
        (_, _, Some(natural)) if image.in_spaces => natural,
        (Some(w), None, Some((nw, nh))) => (w, w * nh / nw),
        (None, Some(h), Some((nw, nh))) => (h * nw / nh, h),
        (None, None, Some(natural)) => natural,
        _ => default_image_size(image),
    }
}

/// Makes the document of `obj` show `text` and gives it with the count of
/// what else changes its picture (read tracked, so a change runs the
/// caller again).
pub(crate) fn prepare(
    engine: &Engine,
    obj: ObjectId,
    text: &str,
    format: TextFormat,
) -> Option<(RichHandle, i32)> {
    // Completing the item gives it its scope in the graph, where the epoch
    // lives so that it dies with the item.
    super::init_text(engine, obj).ok()?;
    let shared = engine
        .untrack(|| {
            engine.with_mut(obj, |t: &mut Text| {
                Rc::clone(t.rich.get_or_insert_with(Shared::default))
            })
        })
        .ok()?;
    let (changed, has_images) = {
        let mut s = shared.borrow_mut();
        let changed = s.doc.update(text, format, &OPTIONS);
        if changed {
            s.has_links = s.doc.has_links();
            s.pressed = None;
        }
        (changed, s.doc.has_images())
    };
    if has_images {
        let base = engine.read(obj, Text::BASE_URL.index).to_js_string();
        let document = engine.document_url(obj).unwrap_or(Atom::EMPTY);
        load_images(engine, &shared, &document, &base, changed);
    } else if changed {
        shared.borrow_mut().images.clear();
    }
    let node = {
        let mut s = shared.borrow_mut();
        if s.epoch.is_none()
            && let Some(scope) = engine.object_scope(obj)
        {
            s.epoch = engine.graph().property(scope, Value::Int(0)).ok();
        }
        s.epoch
    };
    let epoch = node
        .and_then(|n| engine.graph().try_get(n))
        .map_or(0, |v| v.to_number() as i32);
    Some((RichHandle(shared), epoch))
}

/// Drops the document of an item whose text is plain again.
pub(crate) fn forget(engine: &Engine, obj: ObjectId) {
    let has = engine
        .untrack(|| engine.with(obj, |t: &Text| t.rich.is_some()))
        .unwrap_or(false);
    if has {
        // The state goes, the epoch node stays with the item's scope: an
        // effect that read it is the one running now.
        let _ = engine.untrack(|| {
            engine.with_mut(obj, |t: &mut Text| {
                if let Some(s) = &t.rich {
                    let mut s = s.borrow_mut();
                    let epoch = s.epoch;
                    *s = RichState {
                        epoch,
                        ..RichState::default()
                    };
                }
            })
        });
    }
}

/// The document of `obj`, if it has one.
pub(crate) fn state(engine: &Engine, obj: ObjectId) -> Option<Shared> {
    engine
        .untrack(|| engine.with(obj, |t: &Text| t.rich.clone()))
        .ok()
        .flatten()
}

/// Makes the item lay out and paint again though no property changed.
pub(crate) fn bump(engine: &Engine, obj: ObjectId, shared: &Shared) {
    let node = shared.borrow().epoch;
    if let Some(n) = node
        && engine.is_alive(obj)
    {
        let g = engine.graph();
        if let Some(v) = g.untrack(|| g.try_get(n)) {
            g.set(n, Value::Int((v.to_number() as i32).wrapping_add(1)));
        }
    }
}

/// The document of `obj` laid out for what its properties say now, the
/// view it is shown in and how far down it starts. Asking is cheap when
/// nothing changed, and never shows a stale document to a caller that set
/// the text a moment ago.
pub(crate) fn current(engine: &Engine, obj: ObjectId) -> Option<(Shared, View, f64)> {
    let t = engine.untrack(|| TextInputs::read(engine, obj));
    let handle = t.rich.clone()?;
    let te = super::text_engine();
    let metrics = handle.layout(&mut te.borrow_mut(), &t);
    let (width, height) = engine.untrack(|| {
        (
            num(engine, obj, Item::WIDTH.index),
            num(engine, obj, Item::HEIGHT.index),
        )
    });
    let view = View {
        width,
        align: h_align(t.h_align),
    };
    let top = vertical_offset(t.v_align, height, metrics.content_height);
    Some((handle.0, view, top))
}

pub(crate) fn h_align(flags: i32) -> HAlign {
    if flags & align::RIGHT != 0 {
        HAlign::Right
    } else if flags & align::H_CENTER != 0 {
        HAlign::Center
    } else if flags & align::JUSTIFY != 0 {
        HAlign::Justify
    } else {
        HAlign::Left
    }
}

// ---------------------------------------------------------------- images

/// The scheme of an address: letters, then a colon. One letter is a drive
/// (`C:\pictures`), not a scheme.
fn scheme(url: &str) -> Option<String> {
    let end = url.find(':')?;
    let head = &url[..end];
    let mut chars = head.chars();
    let ok = head.len() > 1
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    ok.then(|| head.to_ascii_lowercase())
}

/// `source` as an address of its own: one without a scheme is relative to
/// `base` (the item's `baseUrl`).
fn resolve(base: &str, source: &str) -> String {
    if base.is_empty() || scheme(source).is_some() {
        return source.to_owned();
    }
    if source.starts_with('/') {
        // The same place, another path.
        return match base.find("://") {
            Some(i) => {
                let rest = &base[i + 3..];
                let host = rest.find('/').map_or(base.len(), |j| i + 3 + j);
                format!("{}{source}", &base[..host])
            }
            None if scheme(base).is_some() => {
                format!("{}{source}", &base[..=base.find(':').unwrap_or(0)])
            }
            None => source.to_owned(),
        };
    }
    match base.rfind('/') {
        Some(i) => format!("{}{source}", &base[..=i]),
        None => source.to_owned(),
    }
}

/// `url` without the `.` and `..` steps of its path, so that one picture
/// has one address however a document walks to it. A step above the top
/// stays at the top.
fn without_dot_steps(url: &str) -> String {
    if !url.contains("/.") {
        return url.to_owned();
    }
    // The path starts after the place (`file://host`), or after the scheme.
    let start = match url.find("://") {
        Some(i) => url[i + 3..].find('/').map_or(url.len(), |j| i + 3 + j),
        None => scheme(url).map_or(0, |s| s.len() + 1),
    };
    let (head, path) = url.split_at(start);
    let end = path.find(['?', '#']).unwrap_or(path.len());
    let (path, tail) = path.split_at(end);
    let rooted = path.starts_with('/');
    let mut steps: Vec<&str> = Vec::new();
    for step in path.split('/').skip(usize::from(rooted)) {
        match step {
            "." => {}
            ".." => {
                steps.pop();
            }
            step => steps.push(step),
        }
    }
    // A path that ended in a step (`a/..`) names a directory.
    let mut out = String::with_capacity(url.len());
    out.push_str(head);
    if rooted {
        out.push('/');
    }
    out.push_str(&steps.join("/"));
    if matches!(path.rsplit('/').next(), Some("." | "..")) && !out.ends_with('/') {
        out.push('/');
    }
    out.push_str(tail);
    out
}

/// The address of `source` in a document of the item `obj`: from `base`,
/// which is itself from the file the item is declared in when it is not a
/// whole address (Qt 6.11.2 finds `../a.png` beside the QML file for a
/// `baseUrl` of `"elsewhere/"`).
fn address(document: &str, base: &str, source: &str) -> String {
    without_dot_steps(&resolve(&resolve(document, base), source))
}

fn percent_decoded(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(a), Some(b)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((a * 16 + b) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// Whether a document may make the program read `url`.
fn may_load(store: &ImageStore, url: &str) -> bool {
    // What the host registered is in memory, whatever its address says.
    if store.has_resource(url) {
        return true;
    }
    let path: PathBuf = match scheme(url).as_deref() {
        None => url.into(),
        Some("file") => {
            let rest = &url["file:".len()..];
            let path = match rest.strip_prefix("//") {
                Some(r) => r.find('/').map_or("", |i| &r[i..]),
                None => rest,
            };
            String::from_utf8_lossy(&percent_decoded(path))
                .into_owned()
                .into()
        }
        // In memory already, or compiled into the program or nothing at all.
        Some("data" | "qrc") => return true,
        // Anything that could reach out of the machine.
        Some(_) => return false,
    };
    // A regular file: not a device that never ends, not a pipe that waits.
    std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_IMAGE_BYTES)
}

/// Finds the pictures of the document's images. They are decoded here, on
/// the UI thread: the layout needs their sizes now, and a document's images
/// are small. One the cache already has costs a lookup.
fn load_images(engine: &Engine, shared: &Shared, document: &str, base: &str, changed: bool) {
    let wanted: Vec<Arc<str>> = {
        let mut s = shared.borrow_mut();
        if s.base.as_str() != base {
            s.base = Atom::from(base);
            s.images.clear();
        } else if !changed {
            return;
        }
        let mut wanted: Vec<Arc<str>> = Vec::new();
        for block in s.doc.blocks() {
            for image in &block.images {
                if !wanted.contains(&image.source) {
                    wanted.push(image.source.clone());
                }
            }
        }
        s.images.retain(|source, _| wanted.contains(source));
        wanted.retain(|source| !s.images.contains_key(source));
        let room = MAX_IMAGES.saturating_sub(s.images.len());
        wanted.truncate(room);
        wanted
    };
    if wanted.is_empty() {
        return;
    }
    let store = ImageStore::of(engine);
    for source in wanted {
        let key = ImageKey {
            url: address(document, base, &source),
            request: Request::default(),
        };
        let image = match store.cached(&key) {
            Some(image) => Some(image),
            None if may_load(&store, &key.url) => store.load_now(&key, true).ok(),
            None => None,
        };
        shared.borrow_mut().images.insert(source, image);
    }
}

// ----------------------------------------------------------------- paint

/// How one run of glyphs of a document is painted.
struct RunLook {
    color: Rgba,
    /// The colour of the lines under, over and through it.
    lines: Rgba,
    overline: bool,
    background: Option<Rgba>,
}

/// The look of the run tagged `tag` in `block`: the document's colours where
/// it names ones that can be read, the item's otherwise.
fn run_look(t: &TextInputs, block: &Block, tag: u32, marker: bool) -> RunLook {
    let base = to_rgba(t.color);
    let plain = RunLook {
        color: base,
        lines: base,
        overline: false,
        background: None,
    };
    if marker {
        return plain;
    }
    let Some(span) = block.span_of_tag(tag) else {
        return plain;
    };
    let style = &span.style;
    let read = |c: &Option<Arc<str>>| c.as_deref().and_then(css_color);
    let color = match read(&style.color) {
        Some(color) => to_rgba(color),
        None if style.link.is_some() => to_rgba(link_color(t)),
        None => base,
    };
    RunLook {
        color,
        lines: read(&style.decoration_color).map_or(color, to_rgba),
        overline: style.overline,
        background: read(&style.background).filter(|c| c.a != 0).map(to_rgba),
    }
}

/// The colour of a link no style gives one. Measured on Qt 6.11, and its
/// documentation of `linkColor` says so: that property is for StyledText,
/// and a link of RichText or of Markdown is blue.
fn link_color(t: &TextInputs) -> Color {
    match t.format {
        TextFormat::Styled => t.link_color,
        _ => Color::from_rgba8(0, 0, 255, 255),
    }
}

/// A line of the text on whole pixels, as the painters put a glyph run's own
/// bars: left on a fraction of a pixel it is a blur, and no two lines of a
/// text would look alike.
fn whole_pixels(bar: kurbo::Rect) -> kurbo::Rect {
    let top = bar.y0.round();
    let height = bar.height().round().max(1.0);
    kurbo::Rect::new(bar.x0.round(), top, bar.x1.round(), top + height)
}

/// A colour as rich text writes it: what QML reads as one, and `rgb()`.
fn css_color(text: &str) -> Option<Color> {
    let text = text.trim();
    // Measured on Qt 6.11: `#f00` and `#80ff0000` are colours in a style,
    // four digits are not.
    if text.strip_prefix('#').is_some_and(|hex| hex.len() == 4) {
        return None;
    }
    let Some(open) = text.find('(') else {
        return Color::parse(text);
    };
    let name = text[..open].trim_end();
    if !name.eq_ignore_ascii_case("rgb") && !name.eq_ignore_ascii_case("rgba") {
        return None;
    }
    let inside = text[open + 1..].strip_suffix(')')?;
    let mut parts = inside.split(',').map(str::trim);
    let mut channel = || -> Option<u8> {
        let part = parts.next()?;
        let value = match part.strip_suffix('%') {
            Some(percent) => percent.trim_end().parse::<f64>().ok()? * 2.55,
            None => part.parse::<f64>().ok()?,
        };
        value
            .is_finite()
            .then(|| value.round().clamp(0.0, 255.0) as u8)
    };
    let (r, g, b) = (channel()?, channel()?, channel()?);
    // Measured: an alpha up to one is a share, a larger one counts to 255,
    // and half is 127.
    let alpha = match parts.next() {
        None => 255,
        Some(part) => {
            let value = part.parse::<f64>().ok().filter(|v| v.is_finite())?;
            let share = if value <= 1.0 { value } else { value / 255.0 };
            (share.clamp(0.0, 1.0) * 255.0) as u8
        }
    };
    parts
        .next()
        .is_none()
        .then(|| Color::from_rgba8(r, g, b, alpha))
}

/// Paints the document into an item of `size`.
pub(crate) fn paint(
    te: &mut TextEngine,
    t: &TextInputs,
    handle: &RichHandle,
    size: kurbo::Size,
    out: &mut DisplayListBuilder,
) {
    let mut s = handle.0.borrow_mut();
    // What polish laid out, so nothing is laid out here unless the item
    // was painted without a polish in between.
    let metrics = s.layout(te, t);
    let down = kurbo::Vec2::new(
        0.0,
        vertical_offset(t.v_align, size.height, metrics.content_height),
    );
    let view = View {
        width: size.width,
        align: h_align(t.h_align),
    };
    let lines = s.doc.lines(&view);
    // Measured on Qt 6.11: the colour behind a paragraph fills it from its
    // first line's top to its last line's bottom, across the whole width
    // of the document and not its margins.
    let mut block: Option<(&Block, Rgba, f64, f64)> = None;
    let mut fill_block = |(_, color, top, bottom): (&Block, Rgba, f64, f64)| {
        out.fill_rect(kurbo::Rect::new(0.0, top, view.width, bottom), color);
    };
    for placed in lines.iter().filter(|p| !p.marker) {
        let top = placed.y + placed.line.top + down.y;
        let bottom = top + placed.line.height;
        match &mut block {
            Some((b, _, _, end)) if std::ptr::eq(*b, placed.block) => *end = bottom,
            _ => {
                block.take().map(&mut fill_block);
                let color = placed.block.background.as_deref().and_then(css_color);
                block = color
                    .filter(|c| c.a != 0)
                    .map(|c| (placed.block, to_rgba(c), top, bottom));
            }
        }
    }
    block.take().map(&mut fill_block);
    // Measured on Qt 6.11: the colour behind a part of the text fills that
    // part of its line from the line's top to its bottom, whatever size the
    // part itself is set in.
    for placed in &lines {
        let top = placed.y + placed.line.top + down.y;
        let mut fill = |(color, x0, x1): (Rgba, f64, f64)| {
            let (x0, x1) = (placed.x + x0, placed.x + x1);
            out.fill_rect(
                kurbo::Rect::new(x0, top, x1, top + placed.line.height),
                color,
            );
        };
        // Runs that meet under one colour are one rectangle: two that met
        // inside a pixel would leave a lighter seam.
        let mut open: Option<(Rgba, f64, f64)> = None;
        for run in &placed.line.runs {
            let background = run_look(t, placed.block, run.tag, placed.marker).background;
            let (x0, x1) = (run.overline.x0, run.overline.x1);
            match (&mut open, background) {
                (Some((color, _, end)), Some(next))
                    if *color == next && (x0 - *end).abs() < 0.01 =>
                {
                    *end = x1;
                }
                _ => {
                    open.take().map(&mut fill);
                    open = background.map(|color| (color, x0, x1));
                }
            }
        }
        open.take().map(&mut fill);
    }
    if let Some((range, color)) = &t.selection {
        for r in s.doc.selection_rects(range.clone(), &view) {
            out.fill_rect(r + down, to_rgba(*color));
        }
    }
    let base = to_rgba(t.color);
    for rule in s.doc.rules(&view) {
        out.fill_rect(rule + down, base);
    }
    for placed in &lines {
        let origin = kurbo::Vec2::new(placed.x, placed.y) + down;
        for run in &placed.line.runs {
            let look = run_look(t, placed.block, run.tag, placed.marker);
            // A glyph run's bars are painted in its own colour, so lines of
            // another colour are rectangles of their own.
            let own = look.lines == look.color;
            for by in crate::text::style_offsets(t.text_style) {
                out.glyphs(
                    Arc::clone(&run.glyphs),
                    origin + *by,
                    to_rgba(t.style_color),
                    Decorations::default(),
                );
            }
            out.glyphs(
                Arc::clone(&run.glyphs),
                origin,
                look.color,
                if own {
                    run.decorations
                } else {
                    Decorations::default()
                },
            );
            // A line in a colour that lets everything through is no line.
            let seen = look.lines.a > 0.0;
            if !own && seen {
                for bar in run.decorations.bars() {
                    out.fill_rect(whole_pixels(bar + origin), look.lines);
                }
            }
            if look.overline && seen {
                out.fill_rect(whole_pixels(run.overline + origin), look.lines);
            }
        }
    }
    for placed in s.doc.images(&view) {
        if let Some(Some(image)) = s.images.get(&placed.image.source) {
            out.image(image.clone(), placed.rect + down);
        }
    }
    let links = match s.key_link {
        Some(_) if t.focused => s.doc.links(&view),
        _ => Vec::new(),
    };
    // Past the end is the last link, for a keyboard that came backwards.
    let on = s.key_link.map(|i| i.min(links.len().saturating_sub(1)));
    if let Some(link) = on.and_then(|i| links.get(i)) {
        // A frame of one pixel around the link the keyboard is on.
        let color = to_rgba(link_color(t));
        for r in &link.rects {
            let r = (*r + down).inflate(1.0, 1.0);
            let edges = [
                kurbo::Rect::new(r.x0, r.y0, r.x1, r.y0 + 1.0),
                kurbo::Rect::new(r.x0, r.y1 - 1.0, r.x1, r.y1),
                kurbo::Rect::new(r.x0, r.y0, r.x0 + 1.0, r.y1),
                kurbo::Rect::new(r.x1 - 1.0, r.y0, r.x1, r.y1),
            ];
            for edge in edges {
                out.fill_rect(edge, color);
            }
        }
    }
}

// ------------------------------------------------- for an editor's text

/// The picture of an image `source` names in the rich text of `obj`, found
/// the way a `Text` finds its own: relative to `base` and the document the
/// item was declared in, and only where a document may reach.
pub(crate) fn editor_image(
    engine: &Engine,
    obj: ObjectId,
    base: &str,
    source: &str,
) -> Option<ImageHandle> {
    let document = engine.document_url(obj).unwrap_or(Atom::EMPTY);
    let store = ImageStore::of(engine);
    let key = ImageKey {
        url: address(document.as_str(), base, source),
        request: Request::default(),
    };
    match store.cached(&key) {
        Some(image) => Some(image),
        None if may_load(&store, &key.url) => store.load_now(&key, true).ok(),
        None => None,
    }
}

/// A colour as rich text writes it, for an editor's rich text.
pub(crate) fn editor_css_color(text: &str) -> Option<Color> {
    css_color(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_writes_a_colour_as_qml_does_or_as_rgb() {
        let rgba = |r, g, b, a| Some(Color::from_rgba8(r, g, b, a));
        assert_eq!(css_color("red"), rgba(255, 0, 0, 255));
        assert_eq!(css_color(" LightGrey "), rgba(0xd3, 0xd3, 0xd3, 255));
        assert_eq!(css_color("#0f0"), rgba(0, 255, 0, 255));
        assert_eq!(css_color("#80ff0000"), rgba(255, 0, 0, 0x80));
        assert_eq!(css_color("#ff00"), None, "four digits are no colour");
        assert_eq!(css_color("transparent").map(|c| c.a), Some(0));
        assert_eq!(css_color("rgb(0, 255, 0)"), rgba(0, 255, 0, 255));
        assert_eq!(css_color("RGB(100%,0%,0%)"), rgba(255, 0, 0, 255));
        assert_eq!(css_color("rgb(300, -5, 0)"), rgba(255, 0, 0, 255));
        assert_eq!(css_color("rgba(0,255,0,255)"), rgba(0, 255, 0, 255));
        assert_eq!(css_color("rgba(0,255,0,0.5)"), rgba(0, 255, 0, 127));
        for no in [
            "",
            "rgb(",
            "rgb()",
            "rgb(1,2)",
            "rgb(1,2,3,4,5)",
            "rgb(a,b,c)",
            "rgb(NaN,0,0)",
            "rgb(inf,0,0)",
            "rgb(1,2,3) x",
            "hsl(0, 100%, 50%)",
            "url(https://example.com/a.png)",
            "expression(alert(1))",
            "nocolour",
        ] {
            assert_eq!(css_color(no), None, "{no}");
        }
    }

    /// A style is written by whoever wrote the document: whatever stands
    /// where a colour should, reading it ends, and with a colour or none.
    #[test]
    fn no_text_breaks_the_reading_of_a_colour() {
        const PARTS: [&str; 24] = [
            "rgb",
            "rgba",
            "RGB",
            "(",
            ")",
            ",",
            "%",
            "#",
            "1",
            "255",
            "-",
            ".",
            "e",
            "999",
            "0.5",
            "NaN",
            "inf",
            " ",
            "red",
            "ff",
            "\u{e9}",
            "\u{202e}",
            "\0",
            "\u{1F600}",
        ];
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let mut text = String::new();
            for _ in 0..next() % 12 {
                text.push_str(PARTS[(next() % PARTS.len() as u64) as usize]);
            }
            let _ = css_color(&text);
        }
        let long = "rgb(".to_owned() + &"1,".repeat(100_000) + ")";
        assert_eq!(css_color(&long), None);
    }

    #[test]
    fn a_bar_is_on_whole_pixels_and_never_thinner_than_one() {
        let bar = whole_pixels(kurbo::Rect::new(0.4, 10.6, 20.6, 10.9));
        assert_eq!(bar, kurbo::Rect::new(0.0, 11.0, 21.0, 12.0));
        let bar = whole_pixels(kurbo::Rect::new(3.0, 2.0, 9.0, 4.4));
        assert_eq!(bar, kurbo::Rect::new(3.0, 2.0, 9.0, 4.0));
    }

    #[test]
    fn an_address_without_a_scheme_is_relative_to_the_base() {
        assert_eq!(resolve("file:///a/b/c.qml", "d.png"), "file:///a/b/d.png");
        assert_eq!(resolve("file:///a/b/c.qml", "/d.png"), "file:///d.png");
        assert_eq!(resolve("file:///a/b/", "x/d.png"), "file:///a/b/x/d.png");
        assert_eq!(resolve("qrc:/a/c.qml", "/d.png"), "qrc:/d.png");
        assert_eq!(resolve("/a/b/c.qml", "d.png"), "/a/b/d.png");
        assert_eq!(resolve("", "d.png"), "d.png");
        assert_eq!(
            resolve("file:///a/b/c.qml", "data:image/png;base64,AAAA"),
            "data:image/png;base64,AAAA"
        );
        assert_eq!(
            resolve("file:///a/", "https://example.com/x.png"),
            "https://example.com/x.png"
        );
    }

    #[test]
    fn steps_up_and_in_place_are_walked_before_the_address_is_used() {
        assert_eq!(without_dot_steps("file:///a/b/../c.png"), "file:///a/c.png");
        assert_eq!(
            without_dot_steps("file:///a/./b/./c.png"),
            "file:///a/b/c.png"
        );
        assert_eq!(without_dot_steps("file:///../../c.png"), "file:///c.png");
        assert_eq!(without_dot_steps("qrc:/a/b/../c.png"), "qrc:/a/c.png");
        assert_eq!(without_dot_steps("/a/b/../../c.png"), "/c.png");
        assert_eq!(without_dot_steps("a/../c.png"), "c.png");
        assert_eq!(without_dot_steps("file:///a/b/.."), "file:///a/");
        assert_eq!(
            without_dot_steps("file:///a/b/../c.png?x=/../y"),
            "file:///a/c.png?x=/../y"
        );
        assert_eq!(
            without_dot_steps("file:///a/.hidden/c.png"),
            "file:///a/.hidden/c.png"
        );
        assert_eq!(
            without_dot_steps("http://host/../c.png"),
            "http://host/c.png"
        );
    }

    #[test]
    fn a_base_that_is_no_whole_address_is_beside_the_document() {
        assert_eq!(
            address("file:///app/Main.qml", "elsewhere/", "../a.png"),
            "file:///app/a.png"
        );
        assert_eq!(
            address("file:///app/Main.qml", "file:///app/Main.qml", "a.png"),
            "file:///app/a.png"
        );
        assert_eq!(
            address("file:///app/Main.qml", "qrc:/pictures/", "a.png"),
            "qrc:/pictures/a.png"
        );
        assert_eq!(address("", "", "a.png"), "a.png");
    }

    #[test]
    fn what_the_host_registered_is_read_whatever_its_address() {
        let engine = Engine::new();
        let store = ImageStore::of(&engine);
        assert!(!may_load(&store, "file:///no/such/file.png"));
        store.register_resource("file:///no/such/file.png", vec![1u8, 2, 3]);
        assert!(may_load(&store, "file:///no/such/file.png"));
        assert!(!may_load(&store, "file:///no/such/other.png"));
    }

    #[test]
    fn a_document_reads_nothing_that_is_not_a_plain_file() {
        let engine = Engine::new();
        let store = &ImageStore::of(&engine);
        // Nothing that reaches out of the machine, whatever it names.
        for url in [
            "http://example.com/a.png",
            "https://example.com/a.png",
            "ftp://example.com/a.png",
            "image://provider/a",
            "javascript:alert(1)",
            "smb://host/share/a.png",
        ] {
            assert!(!may_load(store, url), "{url}");
        }
        // Not a device, a directory or a file that is not there.
        for url in [
            "/dev/zero",
            "file:///dev/urandom",
            "/",
            "file:///no/such/file.png",
        ] {
            assert!(!may_load(store, url), "{url}");
        }
        let file = std::env::current_exe().expect("the test binary");
        assert!(
            file.metadata().expect("its size").len() <= MAX_IMAGE_BYTES
                || !may_load(store, &file.to_string_lossy())
        );
        assert!(may_load(store, "data:image/png;base64,AAAA"));
    }

    #[test]
    fn percent_signs_are_decoded_only_when_they_are_escapes() {
        assert_eq!(percent_decoded("a%20b"), b"a b");
        assert_eq!(percent_decoded("100%"), b"100%");
        assert_eq!(percent_decoded("%zz%4"), b"%zz%4");
    }
}
