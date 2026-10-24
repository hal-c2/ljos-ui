//! `Text`, laid out and shaped by `ljos_text` (ARCHITECTURE §7.5).
//!
//! A `Text` keeps numbers only: its implicit size, content size, line count
//! and baseline. The layout that produced them lives in the text engine's
//! cache, and the glyph runs that are drawn live in the item's display list,
//! which the scene drops when the item leaves the screen. So text that is not
//! shown costs a few numbers, however much of it there is.
//!
//! Text that is more than plain (`textFormat`) is a [`ljos_text::rich::RichDoc`]
//! the item keeps ([`rich`]): the document is what makes a change cost only
//! the blocks it touches. Its links are [`links`], and selecting across
//! several `Text` items is [`scope`].

mod links;
mod rich;
mod scope;

pub use rich::RichHandle;
pub(crate) use rich::{editor_css_color, editor_image, table_frame};
pub use scope::SelectionScope;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, Font, ObjectId, PropIndex, QmlObject, Reset, Url, Value,
    qml_impl,
};
use ljos_render::{Decorations, DisplayListBuilder, kurbo};
use ljos_text::rich::{DocMetrics, TextFormat, might_be_rich};
use ljos_text::{
    ElideMode, LayoutParams, Line, LineHeight, Ranged, TextEngine, TextLayout, TextStyle, WrapMode,
    reads_right_to_left,
};

use crate::enums::{align, elide, line_height_mode, wrap_mode};
use crate::item::{Item, is_unset, num, size_follows_implicit, write_computed};
use crate::rectangle::to_rgba;
use crate::text_input::text_format;

/// A text engine shared by everything on the UI thread.
pub type SharedTextEngine = Rc<RefCell<TextEngine>>;

thread_local! {
    // One per UI thread, which is one per application: scenes, `FontMetrics`
    // and `TextMetrics` have to agree on fonts and share the caches.
    static ENGINE: RefCell<Option<SharedTextEngine>> = const { RefCell::new(None) };
    // One being made on another thread, which the first use waits for.
    static COMING: RefCell<Option<JoinHandle<TextEngine>>> = const { RefCell::new(None) };
    static MEASURES: Cell<u64> = const { Cell::new(0) };
}

/// How many times this thread's Text items were laid out to say what they
/// measure: a test of how often that is redone counts them.
pub fn texts_measured() -> u64 {
    MEASURES.get()
}

/// The UI thread's text engine. Until [`set_text_engine`] installs another it
/// is one with the bundled font only, which lays text out the same on every
/// machine.
///
/// Do not keep it borrowed while reading or writing QML properties: a binding
/// that runs then may measure text itself.
pub fn text_engine() -> SharedTextEngine {
    if let Some(coming) = COMING.with(|c| c.borrow_mut().take()) {
        // A thread that panicked made none; the same engine is made here.
        let engine = coming
            .join()
            .unwrap_or_else(|_| TextEngine::with_system_fonts());
        return set_text_engine(engine);
    }
    ENGINE.with(|slot| {
        Rc::clone(
            slot.borrow_mut()
                .get_or_insert_with(|| Rc::new(RefCell::new(TextEngine::new()))),
        )
    })
}

/// The UI thread's text engine if one is in use, without making one or
/// waiting for one being made: for tools that look, which must not load
/// fonts by looking.
pub fn text_engine_in_use() -> Option<SharedTextEngine> {
    ENGINE.with(|slot| slot.borrow().clone())
}

/// Makes `engine` the UI thread's text engine, for example one made by
/// [`TextEngine::with_system_fonts`]. A scene picks it up on its next frame
/// and lays its text out again; install it before the first frame, or call
/// [`crate::QuickScene::refresh_text`] to ask for that frame.
pub fn set_text_engine(engine: TextEngine) -> SharedTextEngine {
    COMING.with(|c| c.borrow_mut().take());
    let shared = Rc::new(RefCell::new(engine));
    ENGINE.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&shared)));
    shared
}

/// As [`set_text_engine`], with the engine `making` makes on another
/// thread: the first use of the text engine on this thread waits for it,
/// so what comes before that use runs while the system's fonts are read.
/// One that panicked is made again with [`TextEngine::with_system_fonts`].
pub fn set_text_engine_later(making: JoinHandle<TextEngine>) {
    ENGINE.with(|slot| *slot.borrow_mut() = None);
    COMING.with(|c| *c.borrow_mut() = Some(making));
}

/// Every font family the UI thread's text engine can lay text out with,
/// sorted, once each: what `Qt.fontFamilies()` answers. With system fonts
/// (the desktop shell's default) that is the bundled families, the ones
/// `FontLoader` added and the system's; without, the bundled ones only.
pub fn font_families() -> Vec<String> {
    text_engine().borrow_mut().family_names()
}

/// `Text` (QtQuick): plain text, the tags of `StyledText`, the HTML subset
/// of `RichText`, or Markdown, as `textFormat` says.
///
/// A document is untrusted: nothing in it is run or fetched. A link only
/// reports its address (`linkActivated`); an image is shown when it is a
/// file, a `data:` address or a registered resource, never from a network.
///
/// Beyond Qt: with the keyboard on a `Text`, Tab goes from link to link
/// and Return activates the one it is on; the links are children of the
/// `Text` in the accessibility tree.
///
/// Not implemented: `fontSizeMode`. `renderType` is kept and
/// not applied; there is one way of drawing text.
/// `font.capitalization` applies to plain text only; small capitals are a
/// small letter's capital at seven tenths of the size, as Qt draws them.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Text",
    extends = Item,
    enums = text_enums
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
    /// `Text.Normal` (0), `Outline`, `Raised` or `Sunken`: a second copy of
    /// the text in `styleColor`, one pixel off, drawn behind it.
    #[qml(notify, enumeration = "TextStyle")]
    pub style: i32,
    #[qml(notify)]
    pub style_color: Color,
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
    /// Kept; there is one way of drawing text.
    #[qml(notify, enumeration = "RenderType")]
    pub render_type: i32,
    #[qml(notify, readonly)]
    pub content_width: f64,
    #[qml(notify, readonly)]
    pub content_height: f64,
    #[qml(notify, readonly)]
    pub line_count: i32,
    #[qml(notify, readonly)]
    pub truncated: bool,
    /// What the address of an image in the text is relative to.
    #[qml(notify, reset = BASE_URL_RESET)]
    pub base_url: Url,
    /// [`text_format`]: how `text` is read. `AUTO_TEXT` takes text that
    /// starts like markup for `STYLED_TEXT`.
    #[qml(notify, enumeration = "TextFormat")]
    pub text_format: i32,
    /// The colour of links.
    #[qml(notify)]
    pub link_color: Color,
    /// The address of the link under the pointer, or empty.
    #[qml(notify, readonly)]
    pub hovered_link: Atom,
    #[qml(notify, reset)]
    pub padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub left_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub top_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub right_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub bottom_padding: f64,
    /// [`init_text`] ran.
    #[qml(skip)]
    initialised: bool,
    /// What the completion measured the text with, until the scene's first
    /// look at it: a polish of the same inputs would only measure it again.
    #[qml(skip)]
    measured: Option<Box<TextInputs>>,
    /// The document, once the text was more than plain.
    #[qml(skip)]
    pub(crate) rich: Option<rich::Shared>,
}

/// The enums of `Text`: those every text item has, and its own
/// `TextFormat`, which has `StyledText` where an editor's has not.
fn text_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    let mut table = crate::enums::text_enums();
    table.push((
        "RenderType",
        [
            ("QtRendering", 0),
            ("NativeRendering", 1),
            ("CurveRendering", 2),
        ]
        .into_iter()
        .map(|(k, v)| (k, i64::from(v)))
        .collect(),
    ));
    table.push((
        "TextStyle",
        [("Normal", 0), ("Outline", 1), ("Raised", 2), ("Sunken", 3)]
            .into_iter()
            .map(|(k, v)| (k, i64::from(v)))
            .collect(),
    ));
    table.push((
        "TextFormat",
        [
            ("PlainText", text_format::PLAIN_TEXT),
            ("RichText", text_format::RICH_TEXT),
            ("AutoText", text_format::AUTO_TEXT),
            ("MarkdownText", text_format::MARKDOWN_TEXT),
            ("StyledText", text_format::STYLED_TEXT),
        ]
        .into_iter()
        .map(|(k, v)| (k, i64::from(v)))
        .collect(),
    ));
    table
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
            style: 0,
            style_color: Color::BLACK,
            horizontal_alignment: align::LEFT,
            effective_horizontal_alignment: align::LEFT,
            vertical_alignment: align::TOP,
            wrap_mode: wrap_mode::NO_WRAP,
            elide: elide::NONE,
            // Qt's "no limit".
            maximum_line_count: i32::MAX,
            line_height: 1.0,
            line_height_mode: line_height_mode::PROPORTIONAL,
            render_type: 0,
            content_width: 0.0,
            content_height: 0.0,
            // Empty text is still one line.
            line_count: 1,
            truncated: false,
            base_url: Url::default(),
            text_format: text_format::AUTO_TEXT,
            // Qt's default, by running qml6.
            link_color: Color::from_rgba8(0, 0, 255, 255),
            hovered_link: Atom::EMPTY,
            padding: 0.0,
            left_padding: 0.0,
            top_padding: 0.0,
            right_padding: 0.0,
            bottom_padding: 0.0,
            initialised: false,
            measured: None,
            rich: None,
        }
    }
}

#[qml_impl]
impl Text {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_text(cx.engine, cx.this)?;
        // Qt's text knows its size by the time `Component.onCompleted`
        // runs. The layout stays in the engine's cache, so the first
        // frame's polish finds it there. Text that wraps or elides to a
        // width waits for that polish: the width it has now may still
        // change before the first frame, and that would be a second layout.
        // So does an empty one, which is most often one built in Rust and
        // given its text after it was completed.
        let t = cx.engine.untrack(|| TextInputs::read(cx.engine, cx.this));
        if t.params.width.is_some() || t.text.is_empty() {
            return Ok(());
        }
        write_measured(cx.engine, cx.this, &t, &text_engine())?;
        if t.rich.is_none() {
            cx.engine
                .with_mut(cx.this, |x: &mut Text| x.measured = Some(Box::new(t)))?;
        }
        Ok(())
    }

    /// A link was pressed and released. Nothing is opened: what an address
    /// means is the program's to decide.
    #[qml(signal, name = "linkActivated")]
    fn link_activated(link: String);

    /// The pointer came over a link, or left one (an empty address).
    #[qml(signal, name = "linkHovered")]
    fn link_hovered(link: String);

    /// `linkAt(x, y)`: the address of the link at a point of the item, or
    /// an empty string.
    #[qml(invokable, name = "linkAt")]
    fn link_at(cx: &Cx, x: f64, y: f64) -> String {
        links::link_at(cx.engine, cx.this, kurbo::Point::new(x, y))
            .map(|href| href.to_string())
            .unwrap_or_default()
    }
}

/// Registers what `Text` does with the pointer and the keyboard, and the
/// types that go with it.
pub(crate) fn register(engine: &Engine) {
    links::register(engine);
    scope::register(engine);
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

fn all_padding(engine: &Engine, obj: ObjectId, _side: PropIndex) -> Value {
    engine.read(obj, Text::PADDING.index)
}

/// What completing a `Text` does, for one built natively without completion
/// (the scene calls it before the first layout). Idempotent.
pub(crate) fn init_text(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let done = engine.untrack(|| engine.with(obj, |t: &Text| t.initialised))?;
    if done {
        return Ok(());
    }
    engine.with_mut(obj, |t: &mut Text| t.initialised = true)?;
    // Each of these is bound when something first reads or watches it.
    // The layout works the same out for itself (`TextInputs::read`), so a
    // text nobody asks for its sides holds a value and no binding.
    for side in SIDES {
        if is_unset(engine, obj, side) {
            engine.bind_on_read(obj, side, |e, obj, side| {
                e.bind(obj, side, move |e| e.read(obj, Text::PADDING.index))
            })?;
        }
    }
    // Right-to-left text is aligned right until an alignment is assigned;
    // assigning one replaces this binding, as Qt documents.
    if is_unset(engine, obj, Text::HORIZONTAL_ALIGNMENT.index) {
        engine.bind_on_read(obj, Text::HORIZONTAL_ALIGNMENT.index, |e, obj, p| {
            e.bind(obj, p, move |e| natural_alignment(e, obj, p))
        })?;
    }
    // Without layout mirroring the effective alignment is the alignment.
    engine.bind_on_read(
        obj,
        Text::EFFECTIVE_HORIZONTAL_ALIGNMENT.index,
        |e, obj, p| {
            e.bind_initial(obj, p, move |e| {
                e.read(obj, Text::HORIZONTAL_ALIGNMENT.index)
            })
        },
    )
}

/// A `Text`'s side paddings, in the order `TextInputs` keeps them.
const SIDES: [ljos_engine::PropIndex; 4] = [
    Text::LEFT_PADDING.index,
    Text::TOP_PADDING.index,
    Text::RIGHT_PADDING.index,
    Text::BOTTOM_PADDING.index,
];

/// Font size used when neither `pixelSize` nor `pointSize` is set: the
/// application font's ([`Font::DEFAULT_POINT_SIZE`]) in pixels.
pub const DEFAULT_PIXEL_SIZE: f64 = Font::DEFAULT_POINT_SIZE * 4.0 / 3.0;

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
    shown_text_style(engine, &f)
}

/// A QML `font` as a `Text` lays it out: [`text_style`], at the system's
/// text size when the font follows it (tracked).
pub fn shown_text_style(engine: &Engine, f: &Font) -> TextStyle {
    let mut style = text_style(f);
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
    /// `Text.style` and `Text.styleColor`.
    pub text_style: i32,
    pub style_color: Color,
    /// `font.overline`, drawn as a line above each run.
    pub overline: bool,
    pub style: TextStyle,
    /// [`crate::enums::align`] flags.
    pub h_align: i32,
    pub v_align: i32,
    /// `[left, top, right, bottom]` padding.
    pub pad: [f64; 4],
    /// The constraints, with the item's own width and height where it has
    /// them and they matter.
    pub params: LayoutParams,
    /// The item's height when the text is not aligned to the top (it moves
    /// the baseline), otherwise 0 so a height change costs nothing.
    pub align_height: f64,
    /// How the text is read, with `AutoText` resolved.
    pub format: TextFormat,
    pub link_color: Color,
    /// The document, for text that is not plain or that a
    /// [`SelectionScope`] selects in.
    pub rich: Option<RichHandle>,
    /// Counts what changes the picture and is not a property: the link the
    /// keyboard is on.
    pub epoch: i32,
    /// The keyboard is on the item, so the link it is on shows.
    pub focused: bool,
    /// The text is drawn in small capitals (`font.capitalization`). Plain
    /// text only, and not text a [`SelectionScope`] selects in, which is
    /// drawn as written.
    pub small_caps: bool,
    /// What a [`SelectionScope`] has selected of the item, in the offsets
    /// of its document, and the colour behind it.
    pub selection: Option<(std::ops::Range<usize>, Color)>,
}

/// How `text` is read under a `textFormat` (measured on qml6: `AutoText`
/// never takes text for Markdown or for the HTML subset).
fn resolve_format(format: i32, text: &str) -> TextFormat {
    match format {
        text_format::PLAIN_TEXT => TextFormat::Plain,
        text_format::RICH_TEXT => TextFormat::Rich,
        text_format::MARKDOWN_TEXT => TextFormat::Markdown,
        text_format::STYLED_TEXT => TextFormat::Styled,
        _ if might_be_rich(text) => TextFormat::Styled,
        _ => TextFormat::Plain,
    }
}

impl TextInputs {
    /// Reads the text properties (tracked). The item's size is only read
    /// where it changes the result, so resizing a plain label does not lay
    /// it out again.
    pub fn read(engine: &Engine, obj: ObjectId) -> Self {
        // Read together: a text's inputs are read as it is made, by its
        // layout and by its paint, and one at a time each read looked the
        // object and its reader up again.
        let [
            h_align,
            v_align,
            wrap_v,
            elide_v,
            line_height,
            max_lines,
            left,
            top_pad,
            right,
            bottom,
            written,
            format_v,
            font,
            color_v,
            style_v,
            style_color_v,
            link_color_v,
            line_height_mode,
            padding,
        ] = engine.read_values_unless_bound_on_read(
            obj,
            [
                Text::HORIZONTAL_ALIGNMENT.index,
                Text::VERTICAL_ALIGNMENT.index,
                Text::WRAP_MODE.index,
                Text::ELIDE.index,
                Text::LINE_HEIGHT.index,
                Text::MAXIMUM_LINE_COUNT.index,
                Text::LEFT_PADDING.index,
                Text::TOP_PADDING.index,
                Text::RIGHT_PADDING.index,
                Text::BOTTOM_PADDING.index,
                Text::TEXT.index,
                Text::TEXT_FORMAT.index,
                Text::FONT.index,
                Text::COLOR.index,
                Text::STYLE.index,
                Text::STYLE_COLOR.index,
                Text::LINK_COLOR.index,
                Text::LINE_HEIGHT_MODE.index,
                Text::PADDING.index,
            ],
        );
        // What `init_text` binds on a read, worked out here instead: a
        // side follows the padding, an alignment nobody gave follows the
        // direction of the text.
        let padding = padding.unwrap_or_default().to_number();
        let side = |v: Option<Value>| v.map_or(padding, |v| v.to_number());
        let [left, top_pad, right, bottom] = [left, top_pad, right, bottom].map(side);
        let written = written.unwrap_or_default().to_js_string();
        let [
            v_align,
            wrap_v,
            elide_v,
            line_height,
            max_lines,
            format_v,
            font,
            color_v,
            style_v,
            style_color_v,
            link_color_v,
            line_height_mode,
        ] = [
            v_align,
            wrap_v,
            elide_v,
            line_height,
            max_lines,
            format_v,
            font,
            color_v,
            style_v,
            style_color_v,
            link_color_v,
            line_height_mode,
        ]
        .map(Option::unwrap_or_default);
        let int = |v: &Value| v.to_number() as i32;
        let h_align = match h_align {
            Some(v) => int(&v),
            None if reads_right_to_left(written.as_str()) => align::RIGHT,
            None => align::LEFT,
        };
        let v_align = int(&v_align);
        let wrap = wrap(int(&wrap_v));
        let elide = elide_mode(int(&elide_v));
        let line_height = line_height.to_number();
        let max_lines = int(&max_lines);
        let pad = [left, top_pad, right, bottom];
        // Qt wraps and elides to a width the item was given, never to the
        // one it took from the text itself.
        // The size is read either way: a layout that sizes the item stops
        // it following the implicit size without changing anything else
        // this reads, and the new size is what says so.
        let own = |horizontal: bool, p| {
            let size = num(engine, obj, p);
            (!size_follows_implicit(engine, obj, horizontal)).then_some(size)
        };
        let constrained = wrap != WrapMode::NoWrap || elide != ElideMode::None;
        let width = if constrained {
            own(true, Item::WIDTH.index).map(|w| (w - pad[0] - pad[2]).max(0.0))
        } else {
            None
        };
        let height = if elide == ElideMode::Right && width.is_some() {
            own(false, Item::HEIGHT.index).map(|h| (h - pad[1] - pad[3]).max(0.0))
        } else {
            None
        };
        let top = v_align & (align::BOTTOM | align::V_CENTER) == 0;
        let color = |v: Value, or: Color| match v {
            Value::Color(c) => c,
            _ => or,
        };
        let format = resolve_format(int(&format_v), written.as_str());
        let font = match font {
            Value::Font(f) => Some(f),
            _ => None,
        };
        // Capitals are for plain text: in markup they would change tags,
        // character references and addresses with the words.
        let text = match &font {
            Some(f) if format == TextFormat::Plain => match f.drawn(written.as_str()) {
                std::borrow::Cow::Borrowed(_) => written,
                std::borrow::Cow::Owned(drawn) => Atom::from(drawn.as_str()),
            },
            _ => written,
        };
        let member = scope::member(engine, obj);
        let document = if format != TextFormat::Plain || member.is_some() {
            rich::prepare(engine, obj, text.as_str(), format)
        } else {
            // Plain text that became plain again keeps no document.
            rich::forget(engine, obj);
            None
        };
        let has_links = document.as_ref().is_some_and(|(d, _)| d.has_links());
        let focused = has_links && engine.read(obj, Item::ACTIVE_FOCUS.index).to_boolean();
        if let Some((d, _)) = &document {
            if focused {
                links::arrive(engine, obj, d);
            } else {
                d.leave_keyboard();
            }
        }
        let selection = match (&document, member) {
            (Some((d, _)), Some(m)) => d.selection().map(|range| (range, m.color)),
            _ => None,
        };
        let small_caps = document.is_none()
            && font
                .as_ref()
                .is_some_and(|f| f.capitalization == Font::SMALL_CAPS);
        TextInputs {
            text,
            small_caps,
            color: color(color_v, Color::BLACK),
            overline: font.as_ref().is_some_and(|f| f.overline),
            text_style: int(&style_v),
            style_color: color(style_color_v, Color::BLACK),
            link_color: color(link_color_v, Color::BLACK),
            format,
            epoch: document.as_ref().map_or(0, |(_, epoch)| *epoch),
            rich: document.map(|(d, _)| d),
            focused,
            selection,
            style: shown_text_style(engine, &font.unwrap_or_default()),
            h_align,
            v_align,
            pad,
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
                line_height: if int(&line_height_mode) == line_height_mode::FIXED {
                    LineHeight::Fixed(line_height)
                } else {
                    LineHeight::Proportional(line_height)
                },
                justify: h_align & align::JUSTIFY != 0,
            },
            align_height: if top {
                0.0
            } else {
                (num(engine, obj, Item::HEIGHT.index) - pad[1] - pad[3]).max(0.0)
            },
        }
    }

    fn layout(&self, te: &mut TextEngine) -> Arc<TextLayout> {
        if self.small_caps {
            let (drawn, spans) = Ranged::small_caps(self.text.as_str(), &self.style);
            let ranged = Ranged {
                spans: &spans,
                strut: true,
                ..Ranged::NONE
            };
            return te.layout_ranged(&drawn, &self.style, &ranged, &self.params);
        }
        te.layout(self.text.as_str(), &self.style, &self.params)
    }

    /// Lays the text out and says what it measures.
    fn measure(&self, te: &mut TextEngine) -> DocMetrics {
        if let Some(d) = &self.rich {
            return d.layout(te, self);
        }
        let l = self.layout(te);
        DocMetrics {
            natural_width: l.natural_width,
            content_width: l.content_width,
            content_height: l.content_height,
            implicit_height: l.implicit_height,
            line_count: l.line_count(),
            truncated: l.truncated,
            baseline: l.baseline,
            rtl: l.rtl,
        }
    }
}

/// Where the copies of the text in `styleColor` go for a `Text.style`:
/// `Outline` is one pixel in each direction, `Raised` one pixel down and
/// `Sunken` one pixel up. Painted before the text, so it covers them.
pub(crate) fn style_offsets(text_style: i32) -> &'static [kurbo::Vec2] {
    const OUTLINE: [kurbo::Vec2; 4] = [
        kurbo::Vec2::new(0.0, -1.0),
        kurbo::Vec2::new(0.0, 1.0),
        kurbo::Vec2::new(-1.0, 0.0),
        kurbo::Vec2::new(1.0, 0.0),
    ];
    const RAISED: [kurbo::Vec2; 1] = [kurbo::Vec2::new(0.0, 1.0)];
    const SUNKEN: [kurbo::Vec2; 1] = [kurbo::Vec2::new(0.0, -1.0)];
    match text_style {
        1 => &OUTLINE,
        2 => &RAISED,
        3 => &SUNKEN,
        _ => &[],
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
    read: Option<TextInputs>,
) -> Result<(), EngineError> {
    let fresh = !engine.untrack(|| engine.with(obj, |t: &Text| t.initialised))?;
    init_text(engine, obj)?;
    // What the scene's polish effect read is what the text is now: it runs
    // again on every change. A text first set up just now may read
    // otherwise.
    let t = match read {
        Some(t) if !fresh => t,
        _ => engine.untrack(|| TextInputs::read(engine, obj)),
    };
    // Measured again, so what the completion measured no longer stands.
    engine.with_mut(obj, |x: &mut Text| x.measured = None)?;
    write_measured(engine, obj, &t, te)
}

/// Whether `t` is what the completion measured the text with and nothing
/// measured it since. Asked once: the answer is forgotten either way.
pub(crate) fn completion_measured(engine: &Engine, obj: ObjectId, t: &TextInputs) -> bool {
    engine
        .with_mut(obj, |x: &mut Text| x.measured.take())
        .ok()
        .flatten()
        .is_some_and(|m| *m == *t)
}

/// Lays `t` out and writes what it measures on `obj`.
fn write_measured(
    engine: &Engine,
    obj: ObjectId,
    t: &TextInputs,
    te: &SharedTextEngine,
) -> Result<(), EngineError> {
    MEASURES.set(MEASURES.get() + 1);
    // Borrowed for the layout only: the writes below run bindings.
    let m = t.measure(&mut te.borrow_mut());
    let [left, top, right, bottom] = t.pad;
    let shift = vertical_offset(t.v_align, t.align_height, m.content_height);
    engine.batch(|| {
        let put = |p: PropIndex, v: Value| write_computed(engine, obj, p, v);
        put(
            Item::IMPLICIT_WIDTH.index,
            Value::Real(m.natural_width + left + right),
        )?;
        put(
            Item::IMPLICIT_HEIGHT.index,
            Value::Real(m.implicit_height + top + bottom),
        )?;
        put(Text::CONTENT_WIDTH.index, Value::Real(m.content_width))?;
        put(Text::CONTENT_HEIGHT.index, Value::Real(m.content_height))?;
        put(Text::LINE_COUNT.index, Value::Int(m.line_count as i32))?;
        put(Text::TRUNCATED.index, Value::Bool(m.truncated))?;
        put(
            Item::BASELINE_OFFSET.index,
            Value::Real(top + m.baseline + shift),
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
    if let Some(d) = &t.rich {
        return rich::paint(te, t, d, size, out);
    }
    if t.text.is_empty() || t.color.a == 0 {
        return;
    }
    // The request polish made, so this is a cache hit unless the item was
    // off screen long enough for the layout to be dropped.
    let l = t.layout(te);
    let [pad_l, pad_t, pad_r, pad_b] = t.pad;
    let avail_w = (size.width - pad_l - pad_r).max(0.0);
    let avail_h = (size.height - pad_t - pad_b).max(0.0);
    let top = pad_t + vertical_offset(t.v_align, avail_h, l.content_height);
    let color = to_rgba(t.color);
    let shadow = to_rgba(t.style_color);
    for line in &l.lines {
        let left = pad_l + horizontal_offset(t.h_align, l.rtl, avail_w, line);
        for run in &line.runs {
            for by in style_offsets(t.text_style) {
                out.glyphs(
                    Arc::clone(&run.glyphs),
                    kurbo::Vec2::new(left, top) + *by,
                    shadow,
                    Decorations::default(),
                );
            }
            out.glyphs(
                Arc::clone(&run.glyphs),
                kurbo::Vec2::new(left, top),
                color,
                run.decorations,
            );
            if t.overline {
                out.fill_rect(run.overline + kurbo::Vec2::new(left, top), color);
            }
        }
    }
}
