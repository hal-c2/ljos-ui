//! What a control takes from the items it is inside: its `font`, its
//! `palette` and its `locale`.
//!
//! A control, a `Label` and a window each have a font and a palette; a
//! plain `Text` or `Rectangle` has neither and lets them through. An attribute of the font
//! (or a colour of the palette) that the author set on an object stays;
//! every other one is the nearest such ancestor's, and follows it. A
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
//! Assigning `undefined` hands one back ([`FONT_RESET`], [`ROLE_RESET`],
//! [`LOCALE_RESET`]): the object has what is above it again and follows
//! it.
//!
//! Behaviour is pinned by `features/controls/control-font.feature`,
//! `control-palette.feature` and `control-look-reset.feature`.
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
use crate::palette::{ROLE_COUNT, ROLES, role_props};
use crate::text_area::TextArea;
use crate::text_field::TextField;
use crate::util::{check, warn};
use crate::window::{ApplicationWindow, Window};

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
        Kind::Window => None,
    }
}

fn locale_prop(kind: Kind) -> Option<PropIndex> {
    match kind {
        Kind::Control => Some(Control::LOCALE.index),
        Kind::ApplicationWindow => Some(ApplicationWindow::LOCALE.index),
        Kind::Label | Kind::TextField | Kind::TextArea | Kind::Window => None,
    }
}

fn roles_of(kind: Kind) -> &'static [PropIndex; ROLE_COUNT] {
    static CONTROL: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static LABEL: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static WINDOW: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static TEXT_FIELD: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static TEXT_AREA: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    match kind {
        Kind::Control => CONTROL.get_or_init(|| role_props(Control::static_meta())),
        Kind::Label => LABEL.get_or_init(|| role_props(Label::static_meta())),
        Kind::TextField => TEXT_FIELD.get_or_init(|| role_props(TextField::static_meta())),
        Kind::TextArea => TEXT_AREA.get_or_init(|| role_props(TextArea::static_meta())),
        // An ApplicationWindow's palette is its Window part's.
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
        if author_set(engine, obj, fp) {
            let own = font_value(&engine.untrack(|| engine.read(obj, fp)));
            let mask = differing(&own, &Font::default());
            state.font_mask.set(mask);
            if mask != 0 {
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
    if roles.iter().any(|p| author_set(engine, obj, *p)) {
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

/// The nearest ancestor with a font, and that font (both tracked).
fn inherited_font(engine: &Engine, obj: ObjectId) -> Arc<Font> {
    let mut cur = above(engine, obj);
    // A tree deeper than this is a cycle someone built by hand.
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        if let Some(fp) = kind_of(engine, p).and_then(font_prop) {
            return font_value(&engine.read(p, fp));
        }
        cur = above(engine, p);
    }
    Arc::new(Font::default())
}

fn resolve_font(
    engine: &Engine,
    obj: ObjectId,
    fp: PropIndex,
    state: &InheritState,
) -> Result<(), EngineError> {
    let base = inherited_font(engine, obj);
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
    Value::Font(inherited_font(engine, obj))
}

fn take_font_back(engine: &Engine, obj: ObjectId, fp: PropIndex) -> Result<(), EngineError> {
    let Some(state) = kind_of(engine, obj).and_then(|k| state_of(engine, obj, k)) else {
        return engine.reset_to_default(obj, fp);
    };
    let font = engine.untrack(|| inherited_font(engine, obj));
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
fn application_locale(engine: &Engine) -> Value {
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
pub(crate) const LOCALE_RESET: Reset = Reset::Custom {
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
        Kind::Window | Kind::ApplicationWindow => true,
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
            if let Some(k) = kind_of(engine, p) {
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

/// `palette.button = undefined`: the colour is the one from above again,
/// or the style's.
pub(crate) const ROLE_RESET: Reset = Reset::Custom {
    apply: take_role_back,
    value: role_from_above,
};

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
            if let Some(k) = kind_of(engine, p) {
                let theirs = roles_of(k)[i];
                if author_set(engine, p, theirs) {
                    return engine.read(p, theirs);
                }
            }
            cur = above(engine, p);
        }
    }
    let enabled = match kind {
        Kind::Window | Kind::ApplicationWindow => true,
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
