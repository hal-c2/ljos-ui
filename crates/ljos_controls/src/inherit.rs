//! What a control takes from the items it is inside: its `font` and its
//! `palette`.
//!
//! A control, a `Label` and a window each have both; a plain `Text` or
//! `Rectangle` has neither and lets them through. An attribute of the font
//! (or a colour of the palette) that the author set on an object stays;
//! every other one is the nearest such ancestor's, and follows it. A
//! palette colour nobody set is the style's, and the style has a second
//! set for a control that is not enabled.
//!
//! Most objects never set either. Until the first one does, nothing walks
//! the tree: one graph node per kind (an epoch, as `LayoutMirroring` has)
//! tells every object when that changes.
//!
//! Behaviour is pinned by `features/controls/control-font.feature` and
//! `control-palette.feature`.
//!
//! Limits: an attribute set to the value it would have had anyway is not
//! seen as set; `font = undefined` does not hand the font back.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use ljos_engine::{
    Color, Engine, EngineError, Font, NodeId, ObjectId, PropIndex, Provenance, QmlType, Value,
};
use ljos_quick::{Item, Text};

use crate::control::Control;
use crate::label::Label;
use crate::palette::{ROLE_COUNT, ROLES, role_props};
use crate::util::{check, warn};
use crate::window::{ApplicationWindow, Window};

/// Which of the types that have a font or a palette an object is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Control,
    Label,
    /// A palette and no font.
    Window,
    ApplicationWindow,
}

/// What an object remembers about its own font and palette. Lives in the
/// object's native part.
#[derive(Debug, Default)]
pub(crate) struct InheritState {
    /// The attributes of the font the author set here ([`attr`]).
    font_mask: Cell<u16>,
    /// The font this module last wrote, so that the author's writes can be
    /// told from its own.
    font_resolved: RefCell<Option<Arc<Font>>>,
    /// Whether the palette ever left the defaults of an enabled control.
    palette_touched: Cell<bool>,
}

mod attr {
    pub const FAMILY: u16 = 1 << 0;
    pub const POINT_SIZE: u16 = 1 << 1;
    pub const PIXEL_SIZE: u16 = 1 << 2;
    pub const WEIGHT: u16 = 1 << 3;
    pub const ITALIC: u16 = 1 << 4;
    pub const UNDERLINE: u16 = 1 << 5;
    pub const STRIKEOUT: u16 = 1 << 6;
    pub const LETTER_SPACING: u16 = 1 << 7;
    pub const WORD_SPACING: u16 = 1 << 8;
}

/// The attributes in which two fonts differ.
fn differing(a: &Font, b: &Font) -> u16 {
    let mut m = 0;
    let mut bit = |differs: bool, flag: u16| {
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
    m
}

/// `base` with the attributes in `mask` taken from `own`.
fn merged(base: &Font, own: &Font, mask: u16) -> Font {
    let mut f = base.clone();
    let has = |flag: u16| mask & flag != 0;
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
    f
}

/// The two epochs of an engine: how often a font attribute, and a palette
/// colour, was set by an author.
#[derive(Default)]
struct Epochs {
    font: Cell<Option<NodeId>>,
    palette: Cell<Option<NodeId>>,
}

#[derive(Clone, Copy)]
enum Which {
    Font,
    Palette,
}

fn epoch(engine: &Engine, which: Which) -> Option<NodeId> {
    let all = engine.extension::<Epochs>();
    let slot = match which {
        Which::Font => &all.font,
        Which::Palette => &all.palette,
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
        Kind::ApplicationWindow => Some(ApplicationWindow::FONT.index),
        Kind::Window => None,
    }
}

fn roles_of(kind: Kind) -> &'static [PropIndex; ROLE_COUNT] {
    static CONTROL: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static LABEL: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    static WINDOW: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    match kind {
        Kind::Control => CONTROL.get_or_init(|| role_props(Control::static_meta())),
        Kind::Label => LABEL.get_or_init(|| role_props(Label::static_meta())),
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
        let st = state.clone();
        engine.effect_in(obj, move |e| {
            if in_use(e, Which::Font) {
                check(e, "font", resolve_font(e, obj, fp, &st));
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

/// The nearest ancestor with a font, and that font (both tracked).
fn inherited_font(engine: &Engine, obj: ObjectId) -> Arc<Font> {
    let mut cur = engine.visual_parent(obj);
    // A tree deeper than this is a cycle someone built by hand.
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        if let Some(fp) = kind_of(engine, p).and_then(font_prop) {
            return font_value(&engine.read(p, fp));
        }
        cur = engine.visual_parent(p);
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
        Kind::Control | Kind::Label => engine.read(obj, Item::ENABLED.index).to_boolean(),
    };
    if !in_use && enabled && !state.palette_touched.get() {
        return Ok(());
    }
    let own = roles_of(kind);
    let mut inherited: [Option<Color>; ROLE_COUNT] = [None; ROLE_COUNT];
    let mine: Vec<bool> = own.iter().map(|p| author_set(engine, obj, *p)).collect();
    if in_use {
        let mut open = mine.iter().filter(|m| !**m).count();
        let mut cur = engine.visual_parent(obj);
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
            cur = engine.visual_parent(p);
        }
        if !ended {
            warn(engine, "palette: the item tree does not end");
        }
    }
    for i in 0..ROLE_COUNT {
        if mine[i] {
            continue;
        }
        let c = inherited[i].unwrap_or(if enabled {
            ROLES[i].active
        } else {
            ROLES[i].disabled
        });
        let cur = engine.untrack(|| engine.read(obj, own[i]));
        if color_of(&cur) != Some(c) {
            state.palette_touched.set(true);
            engine.write_derived(obj, own[i], Value::Color(c))?;
        }
    }
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
