//! `Text`, laid out and shaped by `ljos_text` (ARCHITECTURE §7.5).
//!
//! A `Text` keeps numbers only: its implicit size, content size, line count
//! and baseline. The layout that produced them lives in the text engine's
//! cache, and the glyph runs that are drawn live in the item's display list,
//! which the scene drops when the item leaves the screen. So text that is not
//! shown costs a few numbers, however much of it there is.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, Font, ObjectId, PropIndex, QmlObject, Reset, Url, Value,
    qml_impl,
};
use ljos_render::{DisplayListBuilder, kurbo};
use ljos_text::{
    ElideMode, LayoutParams, Line, LineHeight, TextEngine, TextLayout, TextStyle, WrapMode,
    reads_right_to_left,
};

use crate::enums::{align, elide, line_height_mode, wrap_mode};
use crate::item::{Item, is_unset, num, size_follows_implicit, write_computed};
use crate::rectangle::to_rgba;

/// A text engine shared by everything on the UI thread.
pub type SharedTextEngine = Rc<RefCell<TextEngine>>;

thread_local! {
    // One per UI thread, which is one per application: scenes, `FontMetrics`
    // and `TextMetrics` have to agree on fonts and share the caches.
    static ENGINE: RefCell<Option<SharedTextEngine>> = const { RefCell::new(None) };
}

/// The UI thread's text engine. Until [`set_text_engine`] installs another it
/// is one with the bundled font only, which lays text out the same on every
/// machine.
///
/// Do not keep it borrowed while reading or writing QML properties: a binding
/// that runs then may measure text itself.
pub fn text_engine() -> SharedTextEngine {
    ENGINE.with(|slot| {
        Rc::clone(
            slot.borrow_mut()
                .get_or_insert_with(|| Rc::new(RefCell::new(TextEngine::new()))),
        )
    })
}

/// Makes `engine` the UI thread's text engine, for example one made by
/// [`TextEngine::with_system_fonts`]. A scene picks it up on its next frame
/// and lays its text out again; install it before the first frame, or call
/// [`crate::QuickScene::refresh_text`] to ask for that frame.
pub fn set_text_engine(engine: TextEngine) -> SharedTextEngine {
    let shared = Rc::new(RefCell::new(engine));
    ENGINE.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&shared)));
    shared
}

/// `Text` (QtQuick): plain text.
///
/// Not implemented: `textFormat` (rich text and Markdown), `style`, links,
/// `fontSizeMode`, padding and `renderType`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Text",
    extends = Item,
    enums = crate::enums::text_enums
)]
pub struct Text {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub font: Font,
    /// [`crate::enums::align`] `LEFT`, `RIGHT`, `H_CENTER` or `JUSTIFY`.
    /// Until it is assigned it follows the direction of the text.
    #[qml(
        notify, enumeration = "HAlignment",
        reset = ljos_engine::Reset::Follow(natural_alignment)
    )]
    pub horizontal_alignment: i32,
    /// `horizontalAlignment` as it is applied.
    #[qml(notify, readonly, enumeration = "HAlignment")]
    pub effective_horizontal_alignment: i32,
    /// [`crate::enums::align`] `TOP`, `BOTTOM` or `V_CENTER`.
    #[qml(notify, enumeration = "VAlignment")]
    pub vertical_alignment: i32,
    /// [`crate::enums::wrap_mode`].
    #[qml(notify, enumeration = "WrapMode")]
    pub wrap_mode: i32,
    /// [`crate::enums::elide`].
    #[qml(notify, enumeration = "TextElideMode")]
    pub elide: i32,
    /// At most this many lines are shown.
    #[qml(notify, reset)]
    pub maximum_line_count: i32,
    #[qml(notify)]
    pub line_height: f64,
    /// [`crate::enums::line_height_mode`].
    #[qml(notify, enumeration = "LineHeightMode")]
    pub line_height_mode: i32,
    #[qml(notify, readonly)]
    pub content_width: f64,
    #[qml(notify, readonly)]
    pub content_height: f64,
    #[qml(notify, readonly)]
    pub line_count: i32,
    #[qml(notify, readonly)]
    pub truncated: bool,
    /// Kept; nothing is loaded relative to it.
    #[qml(notify, reset = BASE_URL_RESET)]
    pub base_url: Url,
    /// [`init_text`] ran.
    #[qml(skip)]
    initialised: bool,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            // Text is antialiased unless told otherwise, as in Qt.
            item: Item {
                antialiasing: true,
                ..Item::default()
            },
            text: Atom::EMPTY,
            color: Color::BLACK,
            font: Font::default(),
            horizontal_alignment: align::LEFT,
            effective_horizontal_alignment: align::LEFT,
            vertical_alignment: align::TOP,
            wrap_mode: wrap_mode::NO_WRAP,
            elide: elide::NONE,
            // Qt's "no limit".
            maximum_line_count: i32::MAX,
            line_height: 1.0,
            line_height_mode: line_height_mode::PROPORTIONAL,
            content_width: 0.0,
            content_height: 0.0,
            // Empty text is still one line.
            line_count: 1,
            truncated: false,
            base_url: Url::default(),
            initialised: false,
        }
    }
}

#[qml_impl]
impl Text {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_text(cx.engine, cx.this)
    }
}

/// The alignment of a text nobody aligned: right-to-left text is aligned
/// right, as Qt documents.
fn natural_alignment(engine: &Engine, obj: ObjectId, _p: ljos_engine::PropIndex) -> Value {
    let text = engine.read(obj, Text::TEXT.index).to_js_string();
    Value::Int(if reads_right_to_left(text.as_str()) {
        align::RIGHT
    } else {
        align::LEFT
    })
}

/// `baseUrl` of `Text` and `TextEdit`: until it is given one it is the
/// address of the file the item is declared in, and an empty address counts
/// as none given (Qt 6.11.2, by running qml6; the documentation only says
/// that the default is the URL of the QML file).
pub(crate) const BASE_URL_RESET: Reset = Reset::Absent {
    absent: no_address,
    value: document_address,
};

fn no_address(v: &Value) -> bool {
    matches!(v, Value::Url(s) | Value::String(s) if s.is_empty())
}

fn document_address(engine: &Engine, obj: ObjectId, _base_url: PropIndex) -> Value {
    Value::Url(engine.document_url(obj).unwrap_or(Atom::EMPTY))
}

/// What completing a `Text` does, for one built natively without completion
/// (the scene calls it before the first layout). Idempotent.
pub(crate) fn init_text(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let done = engine.untrack(|| engine.with(obj, |t: &Text| t.initialised))?;
    if done {
        return Ok(());
    }
    engine.with_mut(obj, |t: &mut Text| t.initialised = true)?;
    // Right-to-left text is aligned right until an alignment is assigned;
    // assigning one replaces this binding, as Qt documents.
    if is_unset(engine, obj, Text::HORIZONTAL_ALIGNMENT.index) {
        let p = Text::HORIZONTAL_ALIGNMENT.index;
        engine.bind(obj, p, move |e| natural_alignment(e, obj, p))?;
    }
    // Without layout mirroring the effective alignment is the alignment.
    engine.bind_initial(obj, Text::EFFECTIVE_HORIZONTAL_ALIGNMENT.index, move |e| {
        e.read(obj, Text::HORIZONTAL_ALIGNMENT.index)
    })
}

/// Font size used when neither `pixelSize` nor `pointSize` is set.
pub const DEFAULT_PIXEL_SIZE: f64 = 14.0;

/// A QML `font` as the text engine wants it. `pixelSize` wins over
/// `pointSize` (1 pt = 4/3 px at 96 dpi).
///
/// Qt sets a point size at the nearest whole pixel size (11 pt is 15 px,
/// 10 pt is 13 px), and so does this.
pub fn text_style(f: &Font) -> TextStyle {
    let pixel_size = if f.pixel_size > 0 {
        f64::from(f.pixel_size)
    } else if f.point_size > 0.0 {
        (f.point_size * 4.0 / 3.0).round().max(1.0)
    } else {
        DEFAULT_PIXEL_SIZE
    };
    TextStyle {
        family: Arc::from(f.family.as_str()),
        pixel_size: pixel_size as f32,
        weight: f.weight.clamp(1, 1000) as f32,
        italic: f.italic,
        underline: f.underline,
        strikeout: f.strikeout,
        letter_spacing: f.letter_spacing as f32,
        word_spacing: f.word_spacing as f32,
    }
}

/// Reads a `font` property as a style (tracked).
pub(crate) fn read_style(
    engine: &Engine,
    obj: ObjectId,
    font: ljos_engine::PropIndex,
) -> TextStyle {
    let f = match engine.read(obj, font) {
        Value::Font(f) => f,
        _ => Default::default(),
    };
    let mut style = text_style(&f);
    if f.scale_with_system {
        // The system's text size: applied here, where text is laid out,
        // so `font.pixelSize` reads as it was written and nothing but text
        // grows (ARCHITECTURE §7.5, §14). Read tracked: the layout follows
        // the setting.
        let scale = crate::system_prefs::font_scale(engine) as f32;
        if scale != 1.0 {
            style.pixel_size *= scale;
            style.letter_spacing *= scale;
            style.word_spacing *= scale;
        }
    }
    style
}

/// The text of `obj` in the letters its font's `capitalization` asks for
/// (tracked): what is laid out, while the property keeps what was written.
pub(crate) fn drawn_text(engine: &Engine, obj: ObjectId, font: PropIndex, text: PropIndex) -> Atom {
    let written = engine.read(obj, text).to_js_string();
    let Value::Font(f) = engine.read(obj, font) else {
        return written;
    };
    match f.drawn(written.as_str()) {
        std::borrow::Cow::Borrowed(_) => written,
        std::borrow::Cow::Owned(drawn) => Atom::from(drawn.as_str()),
    }
}

pub(crate) fn elide_mode(v: i32) -> ElideMode {
    match v {
        elide::LEFT => ElideMode::Left,
        elide::RIGHT => ElideMode::Right,
        elide::MIDDLE => ElideMode::Middle,
        _ => ElideMode::None,
    }
}

fn wrap(v: i32) -> WrapMode {
    match v {
        wrap_mode::WORD_WRAP => WrapMode::WordWrap,
        wrap_mode::WRAP_ANYWHERE => WrapMode::WrapAnywhere,
        wrap_mode::WRAP => WrapMode::Wrap,
        _ => WrapMode::NoWrap,
    }
}

/// Everything a Text's layout and paint depend on.
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputs {
    pub text: Atom,
    pub color: Color,
    pub style: TextStyle,
    /// [`crate::enums::align`] flags.
    pub h_align: i32,
    pub v_align: i32,
    /// The constraints, with the item's own width and height where it has
    /// them and they matter.
    pub params: LayoutParams,
    /// The item's height when the text is not aligned to the top (it moves
    /// the baseline), otherwise 0 so a height change costs nothing.
    pub align_height: f64,
}

impl TextInputs {
    /// Reads the text properties (tracked). The item's size is only read
    /// where it changes the result, so resizing a plain label does not lay
    /// it out again.
    pub fn read(engine: &Engine, obj: ObjectId) -> Self {
        let int = |p: ljos_engine::PropIndex| engine.read(obj, p).to_number() as i32;
        let h_align = int(Text::HORIZONTAL_ALIGNMENT.index);
        let v_align = int(Text::VERTICAL_ALIGNMENT.index);
        let wrap = wrap(int(Text::WRAP_MODE.index));
        let elide = elide_mode(int(Text::ELIDE.index));
        let line_height = engine.read(obj, Text::LINE_HEIGHT.index).to_number();
        let max_lines = int(Text::MAXIMUM_LINE_COUNT.index);
        // Qt wraps and elides to a width the item was given, never to the
        // one it took from the text itself.
        let own = |horizontal: bool, p| {
            (!size_follows_implicit(engine, obj, horizontal)).then(|| num(engine, obj, p))
        };
        let constrained = wrap != WrapMode::NoWrap || elide != ElideMode::None;
        let width = if constrained {
            own(true, Item::WIDTH.index)
        } else {
            None
        };
        let height = if elide == ElideMode::Right && width.is_some() {
            own(false, Item::HEIGHT.index)
        } else {
            None
        };
        let top = v_align & (align::BOTTOM | align::V_CENTER) == 0;
        TextInputs {
            text: drawn_text(engine, obj, Text::FONT.index, Text::TEXT.index),
            color: match engine.read(obj, Text::COLOR.index) {
                Value::Color(c) => c,
                _ => Color::BLACK,
            },
            style: read_style(engine, obj, Text::FONT.index),
            h_align,
            v_align,
            params: LayoutParams {
                width,
                height,
                wrap,
                elide,
                // 0 is the engine's "all of them", and what Qt does with a
                // limit of zero or less.
                max_lines: if max_lines == i32::MAX {
                    0
                } else {
                    max_lines.max(0) as u32
                },
                line_height: if int(Text::LINE_HEIGHT_MODE.index) == line_height_mode::FIXED {
                    LineHeight::Fixed(line_height)
                } else {
                    LineHeight::Proportional(line_height)
                },
                justify: h_align & align::JUSTIFY != 0,
            },
            align_height: if top {
                0.0
            } else {
                num(engine, obj, Item::HEIGHT.index)
            },
        }
    }

    fn layout(&self, te: &mut TextEngine) -> Arc<TextLayout> {
        te.layout(self.text.as_str(), &self.style, &self.params)
    }
}

/// How far down the text starts in an item of `height`.
fn vertical_offset(v_align: i32, height: f64, content_height: f64) -> f64 {
    if v_align & align::BOTTOM != 0 {
        height - content_height
    } else if v_align & align::V_CENTER != 0 {
        (height - content_height) / 2.0
    } else {
        0.0
    }
}

/// Where a line starts in an item of `width`.
fn horizontal_offset(h_align: i32, rtl: bool, width: f64, line: &Line) -> f64 {
    let free = width - line.width;
    if line.justified {
        0.0
    } else if h_align & align::RIGHT != 0 {
        free
    } else if h_align & align::H_CENTER != 0 {
        free / 2.0
    } else if h_align & align::JUSTIFY != 0 && rtl {
        // The last line of a justified paragraph starts where the text does.
        free
    } else {
        0.0
    }
}

/// Lays a Text out and writes what the layout says about it: implicit size,
/// content size, line count, `truncated` and the baseline. The layout itself
/// stays in the engine's cache.
pub(crate) fn polish(
    engine: &Engine,
    obj: ObjectId,
    te: &SharedTextEngine,
) -> Result<(), EngineError> {
    init_text(engine, obj)?;
    let t = engine.untrack(|| TextInputs::read(engine, obj));
    // Borrowed for the layout only: the writes below run bindings.
    let l = t.layout(&mut te.borrow_mut());
    let shift = vertical_offset(t.v_align, t.align_height, l.content_height);
    engine.batch(|| {
        write_computed(
            engine,
            obj,
            Item::IMPLICIT_WIDTH.index,
            Value::Real(l.natural_width),
        )?;
        write_computed(
            engine,
            obj,
            Item::IMPLICIT_HEIGHT.index,
            Value::Real(l.implicit_height),
        )?;
        write_computed(
            engine,
            obj,
            Text::CONTENT_WIDTH.index,
            Value::Real(l.content_width),
        )?;
        write_computed(
            engine,
            obj,
            Text::CONTENT_HEIGHT.index,
            Value::Real(l.content_height),
        )?;
        write_computed(
            engine,
            obj,
            Text::LINE_COUNT.index,
            Value::Int(l.line_count() as i32),
        )?;
        write_computed(engine, obj, Text::TRUNCATED.index, Value::Bool(l.truncated))?;
        write_computed(
            engine,
            obj,
            Item::BASELINE_OFFSET.index,
            Value::Real(l.baseline + shift),
        )
    })
}

/// Paints the text into an item of `size`: one glyph run per font and
/// direction on each line.
pub(crate) fn paint(
    te: &mut TextEngine,
    t: &TextInputs,
    size: kurbo::Size,
    out: &mut DisplayListBuilder,
) {
    if t.text.is_empty() || t.color.a == 0 {
        return;
    }
    // The request polish made, so this is a cache hit unless the item was
    // off screen long enough for the layout to be dropped.
    let l = t.layout(te);
    let top = vertical_offset(t.v_align, size.height, l.content_height);
    let color = to_rgba(t.color);
    for line in &l.lines {
        let left = horizontal_offset(t.h_align, l.rtl, size.width, line);
        for run in &line.runs {
            out.glyphs(
                Arc::clone(&run.glyphs),
                kurbo::Vec2::new(left, top),
                color,
                run.decorations,
            );
        }
    }
}
