//! What a control takes from the items it is inside: its `font`, its
//! `palette` and its `locale`.
//!
//! A control, a `Label` and a window each have a font and a palette; a
//! plain `Text` or `Rectangle` has no font and lets it through, and hands
//! down the colours of its palette that were set on it. Only a control,
//! a `Label` or a window finds out which colours it has where nobody set
//! one: a plain item reads the style's then. An attribute of the font
//! (or a colour of the palette) that the author set on an object stays;
//! every other one is the nearest such ancestor's, and follows it. In a
//! style with a font of its own ([`crate::style_font`]) a control starts
//! from the style's font for its type instead, and takes from above only
//! what an author set there. A
//! palette colour nobody set is the style's, and the style has a second
//! set for a control that is not enabled, and both again for a dark colour
//! scheme (`Qt.styleHints.colorScheme`).
//!
//! The `locale` of a control or an `ApplicationWindow` is the nearest
//! such ancestor's as a whole until it is given one, and the
//! application's (`Qt.locale()`) when no ancestor has one.
//!
//! Most objects never set any of them. Until the first one does, nothing
//! walks the tree: one graph node per kind (an epoch, as `LayoutMirroring`
//! has) tells every object when that changes.
//!
//! Above every one of them is the application font
//! ([`set_application_font`]), which the host can change at any time: the
//! objects that inherit it follow, each once, and its attributes that differ
//! from the default count as set above, so a style with a font of its own
//! takes them too.
//!
//! Assigning `undefined` hands one back ([`FONT_RESET`], [`register`]'s
//! reset of a colour, [`LOCALE_RESET`]): the object has what is above it again and follows
//! it.
//!
//! Behaviour is pinned by `features/controls/control-font.feature`,
//! `control-palette.feature`, `control-look-reset.feature`,
//! `style-font.feature` and `application-font.feature`.
//!
//! Limits: an attribute set to the value it would have had anyway is not
//! seen as set; a control that has the application's locale keeps the one
//! it was given when the application's changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use ljos_a11y::ColorScheme;
use ljos_engine::{
    Color, Engine, EngineError, Font, NodeId, ObjectId, PropIndex, Provenance, QmlType, Reset,
    Value,
};
use ljos_quick::{Item, Text, TextEditor};

use crate::control::Control;
use crate::label::Label;
use crate::style_font::{self, Look, Scope};
use crate::text_area::TextArea;
use crate::text_field::TextField;
use crate::util::{check, warn};
use crate::window::{ApplicationWindow, Window};
use ljos_quick::palette::{ROLE_COUNT, ROLES, role_props};

/// Which of the types that have a font or a palette an object is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Control,
    Label,
    /// An editor with a background: `TextField`, `TextArea`.
    TextField,
    TextArea,
    /// A palette and no font.
    Window,
    ApplicationWindow,
    /// Any other item: a palette that hands down what was set on it and
    /// nothing else (checked on qml6: a `Row` with `palette.button` set
    /// gives it to the buttons inside it).
    Item,
}

/// What an object remembers about its own font and palette. Lives in the
/// object's native part.
#[derive(Debug, Default)]
pub(crate) struct InheritState {
    /// The attributes of the font the author set here ([`attr`]).
    font_mask: Cell<u32>,
    /// The font this module last wrote, so that the author's writes can be
    /// told from its own.
    font_resolved: RefCell<Option<Arc<Font>>>,
    /// What the object's type says of its font.
    scope: Cell<Scope>,
    /// The style whose font it starts from: its own, or that of the
    /// control above.
    look: Cell<Option<Look>>,
    /// The attributes of the font an author set on the objects above.
    font_above: Cell<u32>,
    /// Whether the palette ever left the defaults of an enabled control.
    palette_touched: Cell<bool>,
}

mod attr {
    pub const FAMILY: u32 = 1 << 0;
    pub const POINT_SIZE: u32 = 1 << 1;
    pub const PIXEL_SIZE: u32 = 1 << 2;
    pub const WEIGHT: u32 = 1 << 3;
    pub const ITALIC: u32 = 1 << 4;
    pub const UNDERLINE: u32 = 1 << 5;
    pub const STRIKEOUT: u32 = 1 << 6;
    pub const LETTER_SPACING: u32 = 1 << 7;
    pub const WORD_SPACING: u32 = 1 << 8;
    pub const SCALE_WITH_SYSTEM: u32 = 1 << 9;
    pub const CAPITALIZATION: u32 = 1 << 10;
    pub const HINTING_PREFERENCE: u32 = 1 << 11;
    pub const STYLE_NAME: u32 = 1 << 12;
    pub const OVERLINE: u32 = 1 << 13;
    pub const KERNING: u32 = 1 << 14;
    pub const PREFER_SHAPING: u32 = 1 << 15;
    pub const CONTEXT_FONT_MERGING: u32 = 1 << 16;
    pub const PREFER_TYPO_LINE_METRICS: u32 = 1 << 17;
    pub const FEATURES: u32 = 1 << 18;
    pub const VARIABLE_AXES: u32 = 1 << 19;
}

/// The attributes in which two fonts differ.
fn differing(a: &Font, b: &Font) -> u32 {
    let mut m = 0;
    let mut bit = |differs: bool, flag: u32| {
        if differs {
            m |= flag;
        }
    };
    bit(a.family != b.family, attr::FAMILY);
    bit(a.point_size != b.point_size, attr::POINT_SIZE);
    bit(a.pixel_size != b.pixel_size, attr::PIXEL_SIZE);
    bit(a.weight != b.weight, attr::WEIGHT);
    bit(a.italic != b.italic, attr::ITALIC);
    bit(a.underline != b.underline, attr::UNDERLINE);
    bit(a.strikeout != b.strikeout, attr::STRIKEOUT);
    bit(a.letter_spacing != b.letter_spacing, attr::LETTER_SPACING);
    bit(a.word_spacing != b.word_spacing, attr::WORD_SPACING);
    bit(
        a.scale_with_system != b.scale_with_system,
        attr::SCALE_WITH_SYSTEM,
    );
    bit(a.capitalization != b.capitalization, attr::CAPITALIZATION);
    bit(
        a.hinting_preference != b.hinting_preference,
        attr::HINTING_PREFERENCE,
    );
    bit(a.style_name != b.style_name, attr::STYLE_NAME);
    bit(a.overline != b.overline, attr::OVERLINE);
    bit(a.kerning != b.kerning, attr::KERNING);
    bit(a.prefer_shaping != b.prefer_shaping, attr::PREFER_SHAPING);
    bit(
        a.context_font_merging != b.context_font_merging,
        attr::CONTEXT_FONT_MERGING,
    );
    bit(
        a.prefer_typo_line_metrics != b.prefer_typo_line_metrics,
        attr::PREFER_TYPO_LINE_METRICS,
    );
    bit(a.features != b.features, attr::FEATURES);
    bit(a.variable_axes != b.variable_axes, attr::VARIABLE_AXES);
    m
}

/// `base` with the attributes in `mask` taken from `own`.
fn merged(base: &Font, own: &Font, mask: u32) -> Font {
    let mut f = base.clone();
    let has = |flag: u32| mask & flag != 0;
    if has(attr::FAMILY) {
        f.family = own.family.clone();
    }
    // A font has one size. Whichever unit was set here wins over the
    // other unit from above.
    match (has(attr::PIXEL_SIZE), has(attr::POINT_SIZE)) {
        (true, true) => {
            f.pixel_size = own.pixel_size;
            f.point_size = own.point_size;
        }
        (true, false) => {
            f.pixel_size = own.pixel_size;
            f.point_size = -1.0;
        }
        (false, true) => {
            f.point_size = own.point_size;
            f.pixel_size = -1;
        }
        (false, false) => {}
    }
    if has(attr::WEIGHT) {
        f.weight = own.weight;
    }
    if has(attr::ITALIC) {
        f.italic = own.italic;
    }
    if has(attr::UNDERLINE) {
        f.underline = own.underline;
    }
    if has(attr::STRIKEOUT) {
        f.strikeout = own.strikeout;
    }
    if has(attr::LETTER_SPACING) {
        f.letter_spacing = own.letter_spacing;
    }
    if has(attr::WORD_SPACING) {
        f.word_spacing = own.word_spacing;
    }
    if has(attr::SCALE_WITH_SYSTEM) {
        f.scale_with_system = own.scale_with_system;
    }
    if has(attr::CAPITALIZATION) {
        f.capitalization = own.capitalization;
    }
    if has(attr::HINTING_PREFERENCE) {
        f.hinting_preference = own.hinting_preference;
    }
    if has(attr::STYLE_NAME) {
        f.style_name = own.style_name.clone();
    }
    if has(attr::OVERLINE) {
        f.overline = own.overline;
    }
    if has(attr::KERNING) {
        f.kerning = own.kerning;
    }
    if has(attr::PREFER_SHAPING) {
        f.prefer_shaping = own.prefer_shaping;
    }
    if has(attr::CONTEXT_FONT_MERGING) {
        f.context_font_merging = own.context_font_merging;
    }
    if has(attr::PREFER_TYPO_LINE_METRICS) {
        f.prefer_typo_line_metrics = own.prefer_typo_line_metrics;
    }
    if has(attr::FEATURES) {
        f.features = own.features.clone();
    }
    if has(attr::VARIABLE_AXES) {
        f.variable_axes = own.variable_axes.clone();
    }
    f
}

/// The epochs of an engine: how often a font attribute, a palette colour
/// and a locale was set by an author.
#[derive(Default)]
struct Epochs {
    font: Cell<Option<NodeId>>,
    palette: Cell<Option<NodeId>>,
    locale: Cell<Option<NodeId>>,
    /// The epochs to move once the creation running now is complete, as
    /// bits of [`Which`]. Moved for each control made, every control made
    /// before it looked again: a creation of many was many times many.
    pending: Cell<u8>,
    /// Whether a style with a font of its own is in use. Until one is, a
    /// font is the one above as a whole, and which attributes an author
    /// set does not matter to the objects below.
    looks: Cell<bool>,
}

/// The engine's application font: a graph node, so that what reads it runs
/// again when it changes, and its attributes that differ from the default.
#[derive(Default)]
struct ApplicationFont {
    node: Cell<Option<NodeId>>,
    set: Cell<u32>,
}

fn application_font_node(engine: &Engine) -> Option<NodeId> {
    let app = engine.extension::<ApplicationFont>();
    if app.node.get().is_none() {
        let g = engine.graph();
        let font = Value::Font(Arc::new(Font::process_default()));
        app.node.set(g.property(g.root(), font).ok());
    }
    app.node.get()
}

/// The application font (tracked): what is above every object that has a
/// font. [`Font::default`] until the host sets another.
pub fn application_font(engine: &Engine) -> Arc<Font> {
    match application_font_node(engine) {
        Some(node) => font_value(&engine.graph().get(node)),
        None => Arc::new(Font::default()),
    }
}

/// Makes `font` the application font of this engine, or the process's
/// ([`Font::process_default`]) again for `None`, while the application
/// runs: every control, label and window that inherits its font takes it,
/// keeping what it or an object above it set, and `Qt.application.font`
/// reads it. Each of them is resolved once for a change. A `Qt.font({...})`
/// and a plain `Text` made from now on start from it ([`Engine::set_default_font`]);
/// a plain `Text` made before keeps the font it was made with.
///
/// [`Font::set_application_font`] is the font of the whole process, read
/// when an object is made; this one is the engine's and is followed.
pub fn set_application_font(engine: &Engine, font: Option<&Font>) {
    let Some(node) = application_font_node(engine) else {
        return;
    };
    let g = engine.graph();
    let process = Font::process_default();
    let font = font.cloned().unwrap_or_else(|| process.clone());
    if *font_value(&g.get_untracked(node)) == font {
        return;
    }
    let set = differing(&font, &process);
    engine.set_default_font((set != 0).then_some(&font));
    let font = Arc::new(font);
    engine.batch(|| {
        engine.extension::<ApplicationFont>().set.set(set);
        g.set(node, Value::Font(font.clone()));
        if set != 0 && !engine.untrack(|| in_use(engine, Which::Font)) {
            // Until now nothing walked the tree for a font.
            bump(engine, Which::Font);
        }
        crate::window::application_font_changed(engine, font);
    });
    engine.request_wake();
}

#[derive(Clone, Copy)]
enum Which {
    Font,
    Palette,
    Locale,
}

fn epoch(engine: &Engine, which: Which) -> Option<NodeId> {
    let all = engine.extension::<Epochs>();
    let slot = match which {
        Which::Font => &all.font,
        Which::Palette => &all.palette,
        Which::Locale => &all.locale,
    };
    if slot.get().is_none() {
        let g = engine.graph();
        slot.set(g.property(g.root(), Value::Int(0)).ok());
    }
    slot.get()
}

/// Whether anything of the kind was ever set (tracked).
fn in_use(engine: &Engine, which: Which) -> bool {
    epoch(engine, which).is_some_and(|n| engine.graph().get(n).to_number() != 0.0)
}

fn bump(engine: &Engine, which: Which) {
    let all = engine.extension::<Epochs>();
    let before = all.pending.get();
    all.pending.set(before | 1 << which as u8);
    if before == 0 {
        engine.after_creation(|e| {
            let pending = e.extension::<Epochs>().pending.take();
            for which in [Which::Font, Which::Palette, Which::Locale] {
                if pending & 1 << which as u8 != 0 {
                    move_epoch(e, which);
                }
            }
        });
    }
}

fn move_epoch(engine: &Engine, which: Which) {
    if let Some(node) = epoch(engine, which) {
        let g = engine.graph();
        let n = g.get_untracked(node).to_number() as i32;
        g.set(node, Value::Int(n.wrapping_add(1).max(1)));
    }
}

/// What kind of font-and-palette object `obj` is, if any.
pub(crate) fn kind_of(engine: &Engine, obj: ObjectId) -> Option<Kind> {
    let meta = engine.meta_of(obj)?;
    if meta.inherits(Control::static_meta()) {
        Some(Kind::Control)
    } else if meta.inherits(Label::static_meta()) {
        Some(Kind::Label)
    } else if meta.inherits(TextField::static_meta()) {
        Some(Kind::TextField)
    } else if meta.inherits(TextArea::static_meta()) {
        Some(Kind::TextArea)
    } else if meta.inherits(ApplicationWindow::static_meta()) {
        Some(Kind::ApplicationWindow)
    } else if meta.inherits(Window::static_meta()) {
        Some(Kind::Window)
    } else if meta.inherits(Item::static_meta()) {
        Some(Kind::Item)
    } else {
        None
    }
}

fn font_prop(kind: Kind) -> Option<PropIndex> {
    match kind {
        Kind::Control => Some(Control::FONT.index),
        Kind::Label => Some(Text::FONT.index),
        Kind::TextField | Kind::TextArea => Some(TextEditor::FONT.index),
        Kind::ApplicationWindow => Some(ApplicationWindow::FONT.index),
        Kind::Window | Kind::Item => None,
    }
}

fn locale_prop(kind: Kind) -> Option<PropIndex> {
    match kind {
        Kind::Control => Some(Control::LOCALE.index),
        Kind::ApplicationWindow => Some(ApplicationWindow::LOCALE.index),
        Kind::Label | Kind::TextField | Kind::TextArea | Kind::Window | Kind::Item => None,
    }
}

fn roles_of(kind: Kind) -> &'static [PropIndex; ROLE_COUNT] {
    static CONTROL: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static LABEL: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static WINDOW: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static TEXT_FIELD: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static TEXT_AREA: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static ITEM: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    match kind {
        Kind::Control => CONTROL.get_or_init(|| role_props(Control::static_meta())),
        Kind::Label => LABEL.get_or_init(|| role_props(Label::static_meta())),
        Kind::TextField => TEXT_FIELD.get_or_init(|| role_props(TextField::static_meta())),
        Kind::TextArea => TEXT_AREA.get_or_init(|| role_props(TextArea::static_meta())),
        Kind::Item => ITEM.get_or_init(|| role_props(Item::static_meta())),
        // An ApplicationWindow's palette is its Window part's, which is its Item part's.
        Kind::Window | Kind::ApplicationWindow => {
            WINDOW.get_or_init(|| role_props(Window::static_meta()))
        }
    }
}

fn state_of(engine: &Engine, obj: ObjectId, kind: Kind) -> Option<Rc<InheritState>> {
    match kind {
        Kind::Control => engine.with::<Control, _>(obj, |c| c.inherit.clone()),
        Kind::Label => engine.with::<Label, _>(obj, |c| c.inherit.clone()),
        Kind::TextField => engine.with::<TextField, _>(obj, |c| c.inherit.clone()),
        Kind::TextArea => engine.with::<TextArea, _>(obj, |c| c.inherit.clone()),
        Kind::Window | Kind::ApplicationWindow => {
            engine.with::<Window, _>(obj, |c| c.inherit.clone())
        }
        Kind::Item => return None,
    }
    .ok()
}

/// Whether the author gave the property a value (assigned or bound). What
/// this module writes does not count: it writes with
/// [`Engine::write_derived`].
fn author_set(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    matches!(
        engine.provenance(obj, p),
        Some(Provenance::Assigned | Provenance::Bound)
    )
}

fn font_value(v: &Value) -> Arc<Font> {
    match v {
        Value::Font(f) => f.clone(),
        _ => Arc::new(Font::default()),
    }
}

/// Starts keeping `obj`'s font and palette in step with its ancestors'.
/// Called when the object completes. `has_children` says the object is
/// made to hold other controls: it then also notices a colour set on it
/// from a script later, at the price of a graph node per colour.
pub(crate) fn attach(
    engine: &Engine,
    obj: ObjectId,
    kind: Kind,
    has_children: bool,
) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, obj, kind) else {
        return Ok(());
    };

    if let Some(fp) = font_prop(kind) {
        let scope = style_font::scope_of(engine, obj);
        state.scope.set(scope);
        state.look.set(scope.look);
        let epochs = engine.extension::<Epochs>();
        let looks = &epochs.looks;
        if scope.look.is_some() && !looks.replace(true) {
            // The style's font differs from the default: every object
            // finds out what it starts from, and what was set above it.
            bump(engine, Which::Font);
        }
        if author_set(engine, obj, fp) {
            let own = font_value(&engine.untrack(|| engine.read(obj, fp)));
            let mask = differing(&own, &Font::default());
            state.font_mask.set(mask);
            // The objects below read this font as it is written; only a
            // style's font laid under it needs what was set here.
            if mask != 0 && (looks.get() || !engine.untrack(|| in_use(engine, Which::Font))) {
                bump(engine, Which::Font);
            }
        }
        let st = state.clone();
        engine.watch(obj, fp, move |e, v| font_written(e, obj, fp, &st, v))?;
        // One effect for both: every object that has a locale has a font.
        let lp = locale_prop(kind);
        if let Some(lp) = lp {
            if author_set(engine, obj, lp) {
                bump(engine, Which::Locale);
            } else if !engine.untrack(|| in_use(engine, Which::Locale)) {
                // Nothing in the scene has a locale of its own, so nothing
                // above this one has one to hand down.
                engine.write_derived(obj, lp, application_locale(engine))?;
            }
            if has_children {
                engine.watch(obj, lp, move |e, _| {
                    // The first locale given from a script: the controls
                    // below start to look.
                    if author_set(e, obj, lp) && !e.untrack(|| in_use(e, Which::Locale)) {
                        bump(e, Which::Locale);
                    }
                })?;
            }
        }
        let st = state.clone();
        engine.effect_in(obj, move |e| {
            if in_use(e, Which::Font) {
                check(e, "font", resolve_font(e, obj, fp, &st));
            }
            if let Some(lp) = lp
                && in_use(e, Which::Locale)
            {
                check(e, "locale", resolve_locale(e, obj, lp));
            }
        })?;
    }

    let roles = roles_of(kind);
    // Only the first colour anybody sets has to tell everything to look.
    // After that, the objects made with this one read its colours as they
    // resolve (a creation assigns before anything completes), and one
    // bump per object would resolve every palette again for each.
    if !engine.untrack(|| in_use(engine, Which::Palette))
        && roles.iter().any(|p| author_set(engine, obj, *p))
    {
        bump(engine, Which::Palette);
    }
    if has_children {
        for p in roles.iter().copied() {
            engine.watch(obj, p, move |e, _| {
                // The colours this module hands down change too; only the
                // author's are news to the items below.
                if author_set(e, obj, p) {
                    bump(e, Which::Palette);
                }
            })?;
        }
    }
    let st = state;
    engine.effect_in(obj, move |e| {
        check(e, "palette", resolve_palette(e, obj, kind, &st));
    })?;
    Ok(())
}

/// Where the look of `obj` comes from next (tracked): the item it is in,
/// and for a popup's item the window the popup belongs to, whatever the
/// popup was written in (checked on qml6: a popup in a `Pane` has none of
/// the pane's colours).
fn above(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    if let Some(popup) = crate::popup::popup_of(engine, obj) {
        let mut cur = match engine.read(popup, crate::popup::Popup::PARENT.index) {
            Value::Object(p) => Some(p),
            _ => None,
        };
        for _ in 0..4096 {
            let p = cur.filter(|p| engine.is_alive(*p))?;
            if matches!(
                kind_of(engine, p),
                Some(Kind::Window | Kind::ApplicationWindow)
            ) {
                return Some(p);
            }
            cur = engine.visual_parent(p);
        }
        return None;
    }
    engine
        .visual_parent(obj)
        .or_else(|| ljos_quick::layer_host(engine, obj))
}

/// What the nearest ancestor with a font hands down.
struct FromAbove {
    font: Arc<Font>,
    /// The attributes an author set on it or above it.
    set: u32,
    look: Option<Look>,
}

/// The nearest ancestor with a font, and that font (both tracked).
fn inherited_font(engine: &Engine, obj: ObjectId) -> FromAbove {
    let mut cur = above(engine, obj);
    // A tree deeper than this is a cycle someone built by hand.
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        let kind = kind_of(engine, p);
        if let Some(fp) = kind.and_then(font_prop) {
            let font = font_value(&engine.read(p, fp));
            let st = kind.and_then(|k| state_of(engine, p, k));
            return FromAbove {
                font,
                set: st
                    .as_ref()
                    .map_or(0, |s| s.font_above.get() | s.font_mask.get()),
                look: st.and_then(|s| s.look.get()),
            };
        }
        cur = above(engine, p);
    }
    FromAbove {
        font: application_font(engine),
        set: engine.extension::<ApplicationFont>().set.get(),
        look: None,
    }
}

/// The font `obj` has where it sets nothing (tracked): the one above, or
/// for a control of a style with a font of its own that font, with what
/// an author set above laid over it.
fn base_font(engine: &Engine, obj: ObjectId, state: &InheritState) -> Arc<Font> {
    let from = inherited_font(engine, obj);
    let scope = state.scope.get();
    let look = scope.look.or(from.look);
    state.look.set(look);
    state.font_above.set(from.set);
    match look {
        None => from.font,
        Some(look) => Arc::new(merged(
            &style_font::font(look, scope.name),
            &from.font,
            from.set,
        )),
    }
}

fn resolve_font(
    engine: &Engine,
    obj: ObjectId,
    fp: PropIndex,
    state: &InheritState,
) -> Result<(), EngineError> {
    let base = base_font(engine, obj, state);
    let own = font_value(&engine.untrack(|| engine.read(obj, fp)));
    let font = merged(&base, &own, state.font_mask.get());
    if font == *own {
        *state.font_resolved.borrow_mut() = Some(own);
        return Ok(());
    }
    let font = Arc::new(font);
    // Before the write: the watch on the font runs inside it.
    *state.font_resolved.borrow_mut() = Some(font.clone());
    engine.write_derived(obj, fp, Value::Font(font))
}

/// The font changed. A value this module did not write is the author's:
/// what differs in it was set here.
fn font_written(engine: &Engine, obj: ObjectId, fp: PropIndex, state: &InheritState, v: &Value) {
    let now = font_value(v);
    let last = state.font_resolved.borrow().clone();
    if last.as_ref().is_some_and(|l| **l == *now) {
        return;
    }
    let mask = if engine.is_bound(obj, fp) {
        // A binding gives the whole font again each time it runs, built
        // on the default font.
        differing(&now, &Font::default())
    } else {
        let before = last.unwrap_or_default();
        state.font_mask.get() | differing(&now, &before)
    };
    state.font_mask.set(mask);
    if mask != 0 && !engine.untrack(|| in_use(engine, Which::Font)) {
        // The effects of every object run, this one's too.
        bump(engine, Which::Font);
        return;
    }
    check(engine, "font", resolve_font(engine, obj, fp, state));
}

/// `font = undefined`: the font is the one from above again, whole.
pub(crate) const FONT_RESET: Reset = Reset::Custom {
    apply: take_font_back,
    value: font_from_above,
};

fn font_from_above(engine: &Engine, obj: ObjectId, _font: PropIndex) -> Value {
    match kind_of(engine, obj).and_then(|k| state_of(engine, obj, k)) {
        Some(state) => Value::Font(base_font(engine, obj, &state)),
        None => Value::Font(inherited_font(engine, obj).font),
    }
}

fn take_font_back(engine: &Engine, obj: ObjectId, fp: PropIndex) -> Result<(), EngineError> {
    let Some(state) = kind_of(engine, obj).and_then(|k| state_of(engine, obj, k)) else {
        return engine.reset_to_default(obj, fp);
    };
    let font = engine.untrack(|| base_font(engine, obj, &state));
    state.font_mask.set(0);
    // Before the write, so that the watch on the font takes it for this
    // module's and not for the author's.
    *state.font_resolved.borrow_mut() = Some(font.clone());
    engine.reset_to(obj, fp, Value::Font(font))
}

/// The locale of the nearest ancestor that has one (tracked), or nothing.
fn inherited_locale(engine: &Engine, obj: ObjectId) -> Value {
    let mut cur = above(engine, obj);
    // A tree deeper than this is a cycle someone built by hand.
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        if let Some(lp) = kind_of(engine, p).and_then(locale_prop) {
            return engine.read(p, lp);
        }
        cur = above(engine, p);
    }
    application_locale(engine)
}

/// The application's locale, the object `Qt.locale()` gives: what a
/// control has when nothing above it gave one. Made once per locale name,
/// so every such control has the same object.
pub(crate) fn application_locale(engine: &Engine) -> Value {
    #[derive(Default)]
    struct Cached(RefCell<Option<(String, Value)>>);
    let cached = engine.extension::<Cached>();
    let name = engine.locale_name();
    if let Some((made_for, locale)) = &*cached.0.borrow()
        && *made_for == name
    {
        return locale.clone();
    }
    // Without a script host (a scene built from Rust alone) there is no
    // locale object to give, and the control has none, as before.
    let locale = engine
        .untrack(|| engine.eval_js("Qt.locale()"))
        .unwrap_or(Value::Undefined);
    *cached.0.borrow_mut() = Some((name, locale.clone()));
    locale
}

fn resolve_locale(engine: &Engine, obj: ObjectId, lp: PropIndex) -> Result<(), EngineError> {
    // Read even where it is not used: the control that hands its locale
    // back follows this one from then on.
    let above = inherited_locale(engine, obj);
    if author_set(engine, obj, lp) {
        return Ok(());
    }
    if engine
        .untrack(|| engine.read(obj, lp))
        .change_equals(&above)
    {
        return Ok(());
    }
    engine.write_derived(obj, lp, above)
}

/// `locale = undefined`: the locale is the one from above again.
pub(crate) const LOCALE_RESET: Reset = Reset::CustomFromCreation {
    apply: take_locale_back,
    value: locale_from_above,
};

fn locale_from_above(engine: &Engine, obj: ObjectId, _locale: PropIndex) -> Value {
    inherited_locale(engine, obj)
}

fn take_locale_back(engine: &Engine, obj: ObjectId, lp: PropIndex) -> Result<(), EngineError> {
    let above = engine.untrack(|| inherited_locale(engine, obj));
    engine.reset_to(obj, lp, above)
}

fn color_of(v: &Value) -> Option<Color> {
    match v {
        Value::Color(c) => Some(*c),
        _ => None,
    }
}

fn resolve_palette(
    engine: &Engine,
    obj: ObjectId,
    kind: Kind,
    state: &InheritState,
) -> Result<(), EngineError> {
    let in_use = in_use(engine, Which::Palette);
    let enabled = match kind {
        Kind::Window | Kind::ApplicationWindow | Kind::Item => true,
        // In a scene this is false inside a disabled item too.
        Kind::Control | Kind::Label | Kind::TextField | Kind::TextArea => {
            engine.read(obj, Item::ENABLED.index).to_boolean()
        }
    };
    // Tracked: every palette is resolved again when the scheme changes.
    let dark = ljos_quick::color_scheme(engine) == ColorScheme::Dark;
    if !in_use && enabled && !dark && !state.palette_touched.get() {
        return Ok(());
    }
    let own = roles_of(kind);
    let mut inherited: [Option<Color>; ROLE_COUNT] = [None; ROLE_COUNT];
    let mine: Vec<bool> = own.iter().map(|p| author_set(engine, obj, *p)).collect();
    if in_use {
        let mut open = mine.iter().filter(|m| !**m).count();
        let mut cur = above(engine, obj);
        let mut ended = false;
        // A tree deeper than this is a cycle someone built by hand.
        for _ in 0..4096 {
            let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
                ended = true;
                break;
            };
            if open == 0 {
                ended = true;
                break;
            }
            if let Some(k) = kind_of(engine, p).filter(|k| hands_down(engine, p, *k)) {
                let theirs = roles_of(k);
                for i in 0..ROLE_COUNT {
                    if !mine[i] && inherited[i].is_none() && author_set(engine, p, theirs[i]) {
                        inherited[i] = color_of(&engine.read(p, theirs[i]));
                        open -= usize::from(inherited[i].is_some());
                    }
                }
            }
            cur = above(engine, p);
        }
        if !ended {
            warn(engine, "palette: the item tree does not end");
        }
    }
    for i in 0..ROLE_COUNT {
        if mine[i] {
            continue;
        }
        let c = inherited[i].unwrap_or(ROLES[i].colour(dark, enabled));
        let cur = engine.untrack(|| engine.read(obj, own[i]));
        if color_of(&cur) != Some(c) {
            state.palette_touched.set(true);
            engine.write_derived(obj, own[i], Value::Color(c))?;
        }
    }
    Ok(())
}

/// Whether `obj`, of kind `kind`, may have colours to hand down: a plain
/// item that never set one has no palette to look in.
fn hands_down(engine: &Engine, obj: ObjectId, kind: Kind) -> bool {
    kind != Kind::Item || ljos_quick::palette::has_own(engine, obj)
}

/// `palette.button = undefined`: the colour is the one from above again,
/// or the style's. A plain item that completes with a colour set, or is
/// given its first one later, tells the controls inside it.
pub(crate) fn register(engine: &Engine) {
    ljos_quick::palette::set_role_reset(engine, take_role_back, role_from_above);
    ljos_quick::palette::set_on_item_palette(engine, item_palette_set);
}

/// A plain item has colours of its own, from when it completed or from
/// its first one set since: the controls inside it look, now and whenever
/// a colour of it changes.
fn item_palette_set(engine: &Engine, item: ObjectId) {
    if kind_of(engine, item) != Some(Kind::Item) {
        return;
    }
    // As for a control: the colours were set before anything completed,
    // so once something looks, what is made with this item reads them.
    if !engine.untrack(|| in_use(engine, Which::Palette)) {
        bump(engine, Which::Palette);
    }
    for p in roles_of(Kind::Item).iter().copied() {
        let watched = engine.watch(item, p, move |e, _| {
            if author_set(e, item, p) {
                bump(e, Which::Palette);
            }
        });
        check(engine, "palette", watched.map(|_| ()));
    }
}

/// Which role of `obj`'s palette `prop` is.
fn role_at(engine: &Engine, obj: ObjectId, prop: PropIndex) -> Option<(Kind, usize)> {
    let kind = kind_of(engine, obj)?;
    let i = roles_of(kind).iter().position(|p| *p == prop)?;
    Some((kind, i))
}

/// What the role is where the object has no colour of its own (tracked):
/// the nearest ancestor's that was given one, or the style's.
fn role_from_above(engine: &Engine, obj: ObjectId, prop: PropIndex) -> Value {
    let Some((kind, i)) = role_at(engine, obj, prop) else {
        // A popup's colours are its item's.
        return crate::popup::look_from_above(engine, obj, prop)
            .unwrap_or_else(|| engine.default_value(obj, prop));
    };
    if in_use(engine, Which::Palette) {
        let mut cur = above(engine, obj);
        for _ in 0..4096 {
            let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
                break;
            };
            if let Some(k) = kind_of(engine, p).filter(|k| hands_down(engine, p, *k)) {
                let theirs = roles_of(k)[i];
                if author_set(engine, p, theirs) {
                    return engine.read(p, theirs);
                }
            }
            cur = above(engine, p);
        }
    }
    let enabled = match kind {
        Kind::Window | Kind::ApplicationWindow | Kind::Item => true,
        Kind::Control | Kind::Label | Kind::TextField | Kind::TextArea => {
            engine.read(obj, Item::ENABLED.index).to_boolean()
        }
    };
    let dark = ljos_quick::color_scheme(engine) == ColorScheme::Dark;
    Value::Color(ROLES[i].colour(dark, enabled))
}

fn take_role_back(engine: &Engine, obj: ObjectId, prop: PropIndex) -> Result<(), EngineError> {
    if role_at(engine, obj, prop).is_none() {
        return match crate::popup::take_look_back(engine, obj, prop) {
            Some(done) => done,
            None => engine.reset_to_default(obj, prop),
        };
    }
    let above = engine.untrack(|| role_from_above(engine, obj, prop));
    engine.reset_to(obj, prop, above)?;
    // The controls below had the colour from here.
    bump(engine, Which::Palette);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_set_here_replaces_the_other_unit_from_above() {
        let base = Font {
            point_size: 12.0,
            weight: 700,
            ..Font::default()
        };
        let own = Font {
            pixel_size: 20,
            ..Font::default()
        };
        let mask = differing(&own, &Font::default());
        assert_eq!(mask, attr::PIXEL_SIZE);
        let f = merged(&base, &own, mask);
        assert_eq!((f.pixel_size, f.point_size, f.weight), (20, -1.0, 700));
    }

    #[test]
    fn nothing_set_here_gives_the_font_from_above() {
        let base = Font {
            italic: true,
            ..Font::default()
        };
        assert_eq!(merged(&base, &Font::default(), 0), base);
    }
}
