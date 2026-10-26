//! What a control takes from the items it is inside: its `font`, its
//! `palette` and its `locale`.
//!
//! A control, a `Label` and a window each have a font and a palette; a
//! plain `Text` or `Rectangle` has no font and lets it through, and hands
//! down the colours of its palette that were set on it. Only a control,
//! a `Label` or a window finds out which colours it has where nobody set
//! one: a plain item reads the style's where no item above it set one. An attribute of the font
//! (or a colour of the palette) that the author set on an object stays;
//! every other one is the nearest such ancestor's, and follows it. In a
//! style with a font of its own ([`crate::style_font`]) a control starts
//! from the style's font for its type instead, and takes from above only
//! what an author set there. A
//! palette colour nobody set is the style's, and the style has a second
//! set for a control that is not enabled, and both again for a dark colour
//! scheme (`Qt.styleHints.colorScheme`).
//!
//! The same holds for each colour of each of the palette's groups
//! (`palette.disabled.button`): one set in a group of an object, or else on
//! its palette, is that group's colour there and below. What a control
//! reads now is its disabled group's when it is not enabled and its active
//! group's otherwise. A group's colours are written only where they are
//! not what the group starts out as, so that most controls never make one;
//! a script that sets a group's colour on an object makes it listen to all
//! of them (`palette-groups.feature`).
//!
//! The `locale` of a control or an `ApplicationWindow` is the nearest
//! such ancestor's as a whole until it is given one, and the
//! application's (`Qt.locale()`) when no ancestor has one.
//!
//! A plain item's own colours are the nearest ancestor's too, as on qml6.
//! They are handed down when an author sets one, not looked up when they
//! are read: each plain item below is given the colour to make on its
//! next read ([`Engine::make_on_read`]), or at once when something
//! observes it, and one made later looks up when it completes. Reading a
//! colour costs what it did; setting one costs a walk of the items below.
//!
//! Most objects never set any of them. Until the first one does, nothing
//! walks the tree: one graph node per kind (an epoch, as `LayoutMirroring`
//! has) tells every object when that changes.
//!
//! Above every one of them is the application font
//! ([`set_application_font`]), which the host can change at any time: the
//! objects that inherit it follow, each once, and its attributes that differ
//! from the default count as set above, so a style with a font of its own
//! takes them too. So does a plain text item, though it hands nothing down.
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
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use ljos_a11y::ColorScheme;
use ljos_engine::{
    Color, Engine, EngineError, Font, NodeId, ObjectId, PropIndex, Provenance, QmlType, Reset,
    Value, WriteKind,
};
use ljos_quick::{Item, Text, TextEditor};

use crate::control::Control;
use crate::label::Label;
use crate::style_font::{self, Look, Scope};
use crate::text_area::TextArea;
use crate::text_field::TextField;
use crate::util::{check, warn};
use crate::window::{ApplicationWindow, Window};
use ljos_quick::palette::{
    FUSION_ROLES, ROLE_COUNT, ROLES, Role, group_props, has_groups, role_props,
};

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
    /// Whether the object is made to hold other controls: a colour of its
    /// palette that a binding gives it again is news to them.
    holds: Cell<bool>,
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

/// The plain items whose colours the items inside them follow, and the
/// objects whose colours an author changed, to hand down to the plain
/// items below them once the creation running now is complete.
#[derive(Default)]
struct PlainItems {
    watched: RefCell<HashSet<ObjectId>>,
    from: RefCell<Vec<ObjectId>>,
    /// This module is writing a plain item's colours: the write is not an
    /// author's.
    handing: Cell<bool>,
    /// The objects that listen to the colours of their groups.
    groups_watched: RefCell<HashSet<ObjectId>>,
    /// This module is writing a colour of a group: the write is not an
    /// author's.
    grouping: Cell<bool>,
    /// Colours made the author's by a write of the value they had, which
    /// their watches did not hear ([`made_own`]).
    made_own: RefCell<HashSet<(ObjectId, PropIndex)>>,
    /// How many times a plain item looked for the colours above it.
    /// What is handed down once [`handed_together`] returns: each object
    /// with the roles of it that changed.
    gathered: RefCell<Option<Vec<(ObjectId, u32)>>>,
    #[cfg(test)]
    looks: Cell<u64>,
}

/// Every role of a palette, as a set of [`hand_down`].
const ALL_ROLES: u32 = (1 << ROLE_COUNT) - 1;

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
/// made from now on starts from it ([`Engine::set_default_font`]), and a
/// plain `Text`, `TextInput` or `TextEdit` follows it where it names nothing
/// of its own ([`ljos_quick::application_font_changed`]).
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
    let old = font_value(&g.get_untracked(node));
    engine.batch(|| {
        // A label or a text field resolves its font as a control does.
        let resolves = |o| kind_of(engine, o).and_then(font_prop).is_some();
        ljos_quick::application_font_changed(engine, &old, &font, resolves);
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
        engine.after_creation(move_pending);
    }
}

/// Moves the epochs [`bump`] put off until the creation is done.
fn move_pending(engine: &Engine) {
    let pending = engine.extension::<Epochs>().pending.take();
    for which in [Which::Font, Which::Palette, Which::Locale] {
        if pending & 1 << which as u8 != 0 {
            move_epoch(engine, which);
        }
    }
    if pending & 1 << Which::Palette as u8 != 0 {
        hand_down_to_plain_items(engine);
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

/// The groups of a palette, as in [`ljos_quick::palette::GROUPS`].
const ACTIVE: usize = 0;
const DISABLED: usize = 2;

/// What is read now, then each group: role `i` of slot `s` is at
/// `s * ROLE_COUNT + i`.
const SLOTS: usize = 4 * ROLE_COUNT;

/// The properties of each group's roles, as [`roles_of`].
fn groups_of(kind: Kind) -> &'static [[PropIndex; ROLE_COUNT]; 3] {
    static CONTROL: OnceLock<[[PropIndex; ROLE_COUNT]; 3]> = OnceLock::new();
    static LABEL: OnceLock<[[PropIndex; ROLE_COUNT]; 3]> = OnceLock::new();
    static WINDOW: OnceLock<[[PropIndex; ROLE_COUNT]; 3]> = OnceLock::new();
    static TEXT_FIELD: OnceLock<[[PropIndex; ROLE_COUNT]; 3]> = OnceLock::new();
    static TEXT_AREA: OnceLock<[[PropIndex; ROLE_COUNT]; 3]> = OnceLock::new();
    static ITEM: OnceLock<[[PropIndex; ROLE_COUNT]; 3]> = OnceLock::new();
    match kind {
        Kind::Control => CONTROL.get_or_init(|| group_props(Control::static_meta())),
        Kind::Label => LABEL.get_or_init(|| group_props(Label::static_meta())),
        Kind::TextField => TEXT_FIELD.get_or_init(|| group_props(TextField::static_meta())),
        Kind::TextArea => TEXT_AREA.get_or_init(|| group_props(TextArea::static_meta())),
        Kind::Item => ITEM.get_or_init(|| group_props(Item::static_meta())),
        Kind::Window | Kind::ApplicationWindow => {
            WINDOW.get_or_init(|| group_props(Window::static_meta()))
        }
    }
}

/// The group whose colours `obj` reads now (tracked): the disabled one
/// when it is not enabled. DECISION: never the inactive one, as Ljós does
/// not tell its windows apart by focus.
fn group_now(engine: &Engine, obj: ObjectId, kind: Kind) -> usize {
    let enabled = match kind {
        Kind::Window | Kind::ApplicationWindow | Kind::Item => true,
        // In a scene this is false inside a disabled item too.
        Kind::Control | Kind::Label | Kind::TextField | Kind::TextArea => {
            engine.read(obj, Item::ENABLED.index).to_boolean()
        }
    };
    if enabled { ACTIVE } else { DISABLED }
}

/// The colour an author gave role `i` of group `g` on `obj` (tracked): in
/// the group, or on the palette, which sets it in every group. `groups`
/// says whether `obj` has groups of its own to look in. DECISION: one set
/// in the group wins over one set on the palette, whichever was set last.
fn author_colour(
    engine: &Engine,
    obj: ObjectId,
    kind: Kind,
    groups: bool,
    g: usize,
    i: usize,
) -> Option<Color> {
    if groups {
        let p = groups_of(kind)[g][i];
        if author_set(engine, obj, p) {
            return color_of(&engine.read(obj, p));
        }
    }
    let p = roles_of(kind)[i];
    if author_set(engine, obj, p) {
        color_of(&engine.read(obj, p))
    } else {
        None
    }
}

/// Each slot's property on an object of kind `kind`: role `i` of slot
/// `s` is at `s * ROLE_COUNT + i`.
fn slots_of(kind: Kind) -> &'static [PropIndex; SLOTS] {
    static SLOTS_OF: [OnceLock<[PropIndex; SLOTS]>; 6] = [const { OnceLock::new() }; 6];
    let at = match kind {
        Kind::Control => 0,
        Kind::Label => 1,
        Kind::TextField => 2,
        Kind::TextArea => 3,
        Kind::Item => 4,
        Kind::Window | Kind::ApplicationWindow => 5,
    };
    SLOTS_OF[at].get_or_init(|| {
        std::array::from_fn(|at| match at / ROLE_COUNT {
            0 => roles_of(kind)[at],
            g => groups_of(kind)[g - 1][at % ROLE_COUNT],
        })
    })
}

/// The slots of `wanted` on `obj` an author gave a colour, bit `at` for
/// slot `at`: the groups' only where `groups` says it has groups of its
/// own. Asked once of each object a palette looks in, not once for each
/// colour.
fn authored(engine: &Engine, obj: ObjectId, kind: Kind, groups: bool, wanted: u128) -> u128 {
    let slots = slots_of(kind);
    engine.authored(
        obj,
        if groups { slots } else { &slots[..ROLE_COUNT] },
        wanted,
    )
}

/// Every slot.
const ALL_SLOTS: u128 = (1 << SLOTS) - 1;

/// The slots of `open`, and the role of each: what [`given_colour`]
/// looks at for them.
fn with_roles(open: u128) -> u128 {
    let role = (1u128 << ROLE_COUNT) - 1;
    (0..4).fold(open, |w, s| w | open >> (s * ROLE_COUNT) & role)
}

/// Every slot of each role `open` has a slot of: a palette's own slot
/// reads the colour of the group in force, which may be one the object
/// gave itself and so is not open.
fn whole_roles(open: u128) -> u128 {
    let role = (1u128 << ROLE_COUNT) - 1;
    let roles = (0..4).fold(0, |w, s| w | open >> (s * ROLE_COUNT) & role);
    (0..4).fold(0, |w, s| w | roles << (s * ROLE_COUNT))
}

/// [`author_colour`], with what [`authored`] said of `obj`.
fn given_colour(
    engine: &Engine,
    obj: ObjectId,
    kind: Kind,
    given: u128,
    groups: bool,
    g: usize,
    i: usize,
) -> Option<Color> {
    let p = if groups && given >> ((g + 1) * ROLE_COUNT + i) & 1 != 0 {
        groups_of(kind)[g][i]
    } else if given >> i & 1 != 0 {
        roles_of(kind)[i]
    } else {
        return None;
    };
    color_of(&engine.read(obj, p))
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
/// made to hold other controls: a colour of its palette that a binding
/// gives it again is then news to the controls inside ([`group_written`]).
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
    let grouped = has_groups(engine, obj)
        && groups_of(kind)
            .iter()
            .flatten()
            .any(|p| author_set(engine, obj, *p));
    if grouped || roles.iter().any(|p| author_set(engine, obj, *p)) {
        colours_made(engine, obj);
    }
    // A colour set later is heard through its setter, which listens from
    // then on ([`group_written`]): most containers never have one set, and
    // a graph node per colour per container is most of what one costs.
    state.holds.set(has_children);
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
        let parent = match engine.read(popup, crate::popup::Popup::PARENT.index) {
            Value::Object(p) => Some(p),
            _ => None,
        };
        // A drawer hangs from an overlay, which an item that hosts the
        // window may own: then the window it was written in.
        return window_above(engine, parent)
            .or_else(|| window_above(engine, crate::popup::default_parent(engine, popup)));
    }
    engine
        .visual_parent(obj)
        .or_else(|| ljos_quick::layer_host(engine, obj))
}

/// The nearest window at or above `cur` (tracked).
fn window_above(engine: &Engine, mut cur: Option<ObjectId>) -> Option<ObjectId> {
    for _ in 0..4096 {
        let p = cur.filter(|p| engine.is_alive(*p))?;
        if matches!(
            kind_of(engine, p),
            Some(Kind::Window | Kind::ApplicationWindow)
        ) {
            return Some(p);
        }
        // A drawer hangs from the window's overlay, a layer.
        cur = engine
            .visual_parent(p)
            .or_else(|| ljos_quick::layer_host(engine, p));
    }
    None
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

/// Gives `obj` the colours nobody set on it: what it reads now, and, where
/// it is not what its group starts out as, each of its groups' colours.
fn resolve_palette(
    engine: &Engine,
    obj: ObjectId,
    kind: Kind,
    state: &InheritState,
) -> Result<(), EngineError> {
    let in_use = in_use(engine, Which::Palette);
    let now = group_now(engine, obj, kind);
    // Tracked: every palette is resolved again when the scheme changes.
    let dark = ljos_quick::color_scheme(engine) == ColorScheme::Dark;
    let roles = style_roles(engine);
    if !in_use && now == ACTIVE && !dark && roles.is_none() && !state.palette_touched.get() {
        return Ok(());
    }
    let roles = roles.unwrap_or(&ROLES);
    let own = roles_of(kind);
    let own_groups = groups_of(kind);
    let prop = |at: usize| match at / ROLE_COUNT {
        0 => own[at],
        s => own_groups[s - 1][at % ROLE_COUNT],
    };
    let group = |at: usize| match at / ROLE_COUNT {
        0 => now,
        s => s - 1,
    };
    let mine_groups = has_groups(engine, obj);
    let mut found: [Option<Color>; SLOTS] = [None; SLOTS];
    let given = authored(engine, obj, kind, mine_groups, ALL_SLOTS);
    let mine: [bool; SLOTS] = std::array::from_fn(|at| given >> at & 1 != 0);
    let mut open = SLOTS - mine.iter().filter(|m| **m).count();
    // What `p` gives the slots still open.
    let mut look_in = |p: ObjectId, k: Kind, open: &mut usize| {
        let groups = has_groups(engine, p);
        let open_slots = (0..SLOTS)
            .filter(|at| !mine[*at] && found[*at].is_none())
            .fold(0u128, |w, at| w | 1 << at);
        let given = authored(engine, p, k, groups, whole_roles(open_slots));
        if given == 0 {
            return;
        }
        for i in 0..ROLE_COUNT {
            let wanting = |at: &usize| !mine[*at] && found[*at].is_none();
            if !(0..4).map(|s| s * ROLE_COUNT + i).any(|at| wanting(&at)) {
                continue;
            }
            // Without groups, one colour for every slot of the role.
            let all = (!groups)
                .then(|| given_colour(engine, p, k, given, false, ACTIVE, i))
                .flatten();
            for at in (0..4).map(|s| s * ROLE_COUNT + i) {
                if mine[at] || found[at].is_some() {
                    continue;
                }
                found[at] = if groups {
                    given_colour(engine, p, k, given, true, group(at), i)
                } else {
                    all
                };
                *open -= usize::from(found[at].is_some());
            }
        }
    };
    look_in(obj, kind, &mut open);
    if in_use {
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
                look_in(p, k, &mut open);
            }
            cur = above(engine, p);
        }
        if !ended {
            warn(engine, "palette: the item tree does not end");
        }
    }
    let plain = engine.extension::<PlainItems>();
    for at in (0..SLOTS).filter(|at| !mine[*at]) {
        let g = group(at);
        let c = found[at].unwrap_or(roles[at % ROLE_COUNT].colour(dark, g != DISABLED));
        let p = prop(at);
        let cur = engine.untrack(|| engine.read(obj, p));
        if color_of(&cur) != Some(c) {
            state.palette_touched.set(true);
            plain.grouping.set(true);
            let done = engine.write_derived(obj, p, Value::Color(c));
            plain.grouping.set(false);
            done?;
        }
    }
    Ok(())
}

/// Which role `p` of an object of kind `kind` is, on its palette or in
/// one of its groups.
fn role_index(kind: Kind, p: PropIndex) -> Option<usize> {
    std::iter::once(roles_of(kind))
        .chain(groups_of(kind))
        .find_map(|props| props.iter().position(|q| *q == p))
}

/// Whether `obj`, of kind `kind`, may have colours to hand down: a plain
/// item that never set one has no palette to look in.
fn hands_down(engine: &Engine, obj: ObjectId, kind: Kind) -> bool {
    kind != Kind::Item || ljos_quick::palette::has_own(engine, obj)
}

/// `palette.button = undefined`: the colour is the one from above again,
/// or the style's. A plain item that completes with a colour set, or is
/// given its first one later, tells the items inside it.
pub(crate) fn register(engine: &Engine) {
    ljos_quick::palette::set_role_reset(engine, take_role_back, role_from_above);
    ljos_quick::palette::set_on_item_palette(engine, item_palette_set);
    ljos_quick::palette::set_on_group_written(engine, group_written);
}

/// A colour is about to be set in a group of `obj` from a script, or on
/// the palette of a control or a window that completed: from now on, a
/// change of any colour of its groups, and of its palette's, is news to it
/// and to the objects inside it. Few objects ever have a colour set after
/// they completed. A plain item hears its palette's colours already
/// ([`item_palette_set`]).
fn group_written(engine: &Engine, obj: ObjectId) {
    let plain = engine.extension::<PlainItems>();
    if plain.grouping.get() || plain.handing.get() || plain.groups_watched.borrow().contains(&obj) {
        return;
    }
    let Some(kind) = kind_of(engine, obj) else {
        return;
    };
    plain.groups_watched.borrow_mut().insert(obj);
    let holds = state_of(engine, obj, kind).is_some_and(|s| s.holds.get());
    let palette = (kind != Kind::Item).then(|| roles_of(kind));
    for p in groups_of(kind)
        .iter()
        .flatten()
        .chain(palette.into_iter().flatten())
        .copied()
    {
        // It hears the write about to be stored. A colour of the palette
        // that is bound is written over as it resolves, which is no news.
        let role = roles_of(kind).contains(&p);
        let was = Cell::new(author_set(engine, obj, p));
        let watched = engine.watch(obj, p, move |e, v| {
            let given = match e.provenance(obj, p) {
                Some(Provenance::Assigned) => true,
                Some(Provenance::Bound) if !role => true,
                Some(Provenance::Bound) => {
                    // A colour already the author's was read by the
                    // controls that took it as they resolved; only the
                    // plain items below are handed it.
                    if holds && was.get() {
                        let i = role_index(kind, p);
                        e.after_creation(move |e| {
                            hand_down(e, vec![obj], i.map_or(ALL_ROLES, |i| 1 << i))
                        });
                    } else if holds {
                        colours_changed(e, obj);
                    }
                    false
                }
                _ => false,
            };
            if given {
                over_groups(e, obj, kind, p, v);
                colour_written(e, obj, role_index(kind, p), was_own(e, obj, p, &was));
            } else if author_set(e, obj, p) {
                was.set(true);
            } else {
                not_own(e, obj, p, &was);
            }
        });
        check(engine, "palette", watched.map(|_| ()));
    }
}

/// A colour set on the palette after it completed is set in each of its
/// groups, over one set there before, as on qml6, where the last colour
/// written wins. `p` that is not a palette colour (a group's) is left.
fn over_groups(engine: &Engine, obj: ObjectId, kind: Kind, p: PropIndex, v: &Value) {
    let Some(i) = roles_of(kind).iter().position(|r| *r == p) else {
        return;
    };
    for g in groups_of(kind) {
        let q = g[i];
        if author_set(engine, obj, q) && engine.untrack(|| engine.read(obj, q)) != *v {
            check(
                engine,
                "palette",
                engine.untrack(|| engine.write(obj, q, v.clone(), WriteKind::Imperative)),
            );
        }
    }
}

/// An item completed, or was given a colour after it completed. A plain
/// item with colours of its own makes the items inside it look, now and
/// whenever a colour of it changes; one made inside an item with colours
/// takes them.
fn item_palette_set(engine: &Engine, item: ObjectId, written: bool) {
    // Most items: no palette, and none set anywhere.
    let own = ljos_quick::palette::has_own(engine, item);
    if !written && !own && !engine.untrack(|| in_use(engine, Which::Palette)) {
        return;
    }
    let plain = engine.extension::<PlainItems>();
    if plain.handing.get() || plain.watched.borrow().contains(&item) {
        return;
    }
    match kind_of(engine, item) {
        Some(Kind::Item) => {}
        // Its groups were resolved with the colours it had then.
        Some(_) if written => return group_written(engine, item),
        _ => return,
    }
    let roles = roles_of(Kind::Item);
    // A colour being written is not yet stored nor marked as the author's.
    if written || own && roles.iter().any(|p| author_set(engine, item, *p)) {
        plain.watched.borrow_mut().insert(item);
        for (i, p) in roles.iter().copied().enumerate() {
            let was = Cell::new(author_set(engine, item, p));
            let watched = engine.watch(item, p, move |e, v| {
                if author_set(e, item, p) {
                    over_groups(e, item, Kind::Item, p, v);
                    colour_written(e, item, Some(i), was_own(e, item, p, &was));
                } else {
                    not_own(e, item, p, &was);
                }
            });
            check(engine, "palette", watched.map(|_| ()));
        }
        // As for a control: the colours were set before anything
        // completed, so once something looks, what is made with this item
        // reads them; the plain items inside are handed them.
        colours_changed(engine, item);
    } else if !own {
        // Made inside items that hand colours down: they did so before it was.
        engine.untrack(|| take_from_above(engine, item, ALL_ROLES));
    }
}

/// `obj`'s colour `p` was the author's before the write being heard, by
/// what this watch last heard of it or by [`made_own`]. It is from now.
fn was_own(engine: &Engine, obj: ObjectId, p: PropIndex, was: &Cell<bool>) -> bool {
    let told = || {
        engine
            .extension::<PlainItems>()
            .made_own
            .borrow()
            .contains(&(obj, p))
    };
    was.replace(true) || told()
}

/// `obj`'s colour `p` is not the author's after the write being heard.
fn not_own(engine: &Engine, obj: ObjectId, p: PropIndex, was: &Cell<bool>) {
    was.set(false);
    engine
        .extension::<PlainItems>()
        .made_own
        .borrow_mut()
        .remove(&(obj, p));
}

/// `obj`'s colour `p` was made the author's by a write of the value it
/// had already. Its watch did not hear it, so the next change would be
/// news to every object in the scene, as a colour that was not the
/// author's: a theme bound on many menus moved the epoch once for each
/// on its first switch. The controls below look again now instead.
pub(crate) fn made_own(engine: &Engine, obj: ObjectId, p: PropIndex) {
    engine
        .extension::<PlainItems>()
        .made_own
        .borrow_mut()
        .insert((obj, p));
    colours_changed(engine, obj);
}

/// An author changed `obj`'s colours: the controls below look again, and
/// the plain items below are handed them.
fn colours_changed(engine: &Engine, obj: ObjectId) {
    engine.extension::<PlainItems>().from.borrow_mut().push(obj);
    bump(engine, Which::Palette);
}

/// `obj` completed with colours of its own. The first anywhere tells
/// every object to look. After that, the objects made with it read them
/// as they resolved, so only the plain items inside it are handed them:
/// moving the epoch again had every control in the scene look again, and
/// a page of controls that each set one cost many times many.
fn colours_made(engine: &Engine, obj: ObjectId) {
    if !engine.untrack(|| in_use(engine, Which::Palette)) {
        return colours_changed(engine, obj);
    }
    let plain = engine.extension::<PlainItems>();
    let first = plain.from.borrow().is_empty();
    plain.from.borrow_mut().push(obj);
    // Otherwise a hand-down is pending already.
    if first {
        engine.after_creation(hand_down_to_plain_items);
    }
}

/// As [`colours_changed`], for a colour written on an object that
/// completed: what reads it next sees it, as on qml6, also from an
/// `onCompleted` handler of the creation it was made in. `was` says the
/// colour was the author's before: then each control that took it read it
/// as it resolved and runs again on its own, and only the plain items
/// below are handed it. Moving the epoch for it had every control in the
/// scene look again for each colour of a theme bound on a menu bar.
fn colour_written(engine: &Engine, obj: ObjectId, role: Option<usize>, was: bool) {
    if was {
        hand_down(engine, vec![obj], role.map_or(ALL_ROLES, |i| 1 << i));
    } else {
        colours_changed(engine, obj);
        move_pending(engine);
    }
}

/// Runs `f`, and hands the colours it changes down to the plain items
/// once it returns, each object's once with all of its roles that changed.
/// One by one, a theme bound on a menu had each plain item in it look for
/// its colours twice for each colour: once for the watch on the popup's
/// item, once for the watch on its groups.
pub(crate) fn handed_together<R>(engine: &Engine, f: impl FnOnce() -> R) -> R {
    let plain = engine.extension::<PlainItems>();
    if plain.gathered.borrow().is_some() {
        return f();
    }
    *plain.gathered.borrow_mut() = Some(Vec::new());
    let r = f();
    let gathered = plain.gathered.borrow_mut().take().unwrap_or_default();
    for (obj, roles) in gathered {
        hand_down(engine, vec![obj], roles);
    }
    r
}

/// Hands the colours of the objects an author changed down to the plain
/// items below them.
fn hand_down_to_plain_items(engine: &Engine) {
    let from = std::mem::take(&mut *engine.extension::<PlainItems>().from.borrow_mut());
    hand_down(engine, from, ALL_ROLES);
}

/// Hands the colours of `from` down to the plain items below them: the
/// roles in the set `roles`.
fn hand_down(engine: &Engine, from: Vec<ObjectId>, roles: u32) {
    if let Some(gathered) = &mut *engine.extension::<PlainItems>().gathered.borrow_mut() {
        for obj in from {
            match gathered.iter_mut().find(|(o, _)| *o == obj) {
                Some((_, r)) => *r |= roles,
                None => gathered.push((obj, roles)),
            }
        }
        return;
    }
    let mut seen = HashSet::new();
    let mut stack: Vec<ObjectId> = Vec::new();
    for obj in from {
        if !engine.is_alive(obj) {
            continue;
        }
        stack.extend(engine.visual_children(obj));
        while let Some(item) = stack.pop() {
            // Under two of the objects: handed down once.
            if !seen.insert(item) {
                continue;
            }
            stack.extend(engine.visual_children(item));
            if kind_of(engine, item) == Some(Kind::Item) {
                engine.untrack(|| take_from_above(engine, item, roles));
            }
        }
    }
}

/// Gives a plain item the colours of the items it is in, for each role
/// and each group's it did not set itself: on its next read, or now if
/// something observes it. A colour nobody above set is the style's
/// again, where it had another. `roles` are the roles to look for, in
/// each group, where the others are as they were.
fn take_from_above(engine: &Engine, item: ObjectId, roles: u32) {
    #[cfg(test)]
    {
        let looks = &engine.extension::<PlainItems>().looks;
        looks.set(looks.get() + 1);
    }
    // Each role, then each group's: a colour set on a palette is set in
    // each of its groups, and one set in a group there alone, so the
    // groups too are the nearest ancestor's (checked on qml6).
    let own_roles = roles_of(Kind::Item);
    let own_groups = groups_of(Kind::Item);
    let prop = |at: usize| match at / ROLE_COUNT {
        0 => own_roles[at],
        g => own_groups[g - 1][at % ROLE_COUNT],
    };
    let mut inherited: [Option<Color>; SLOTS] = [None; SLOTS];
    // A group's colour the item gave on its palette is its own too.
    // The slots of the roles looked for.
    let looked = (0..4).fold(0u128, |w, s| w | u128::from(roles) << (s * ROLE_COUNT));
    let given = authored(engine, item, Kind::Item, has_groups(engine, item), looked);
    // A role not looked for is as good as the item's own.
    let mine: [bool; SLOTS] = std::array::from_fn(|at| {
        let i = at % ROLE_COUNT;
        roles & 1 << i == 0 || given >> at & 1 != 0 || given >> i & 1 != 0
    });
    let mut open = mine.iter().filter(|m| !**m).count();
    let mut cur = above(engine, item);
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        if open == 0 {
            break;
        }
        if let Some(k) = kind_of(engine, p).filter(|k| hands_down(engine, p, *k)) {
            let groups = has_groups(engine, p);
            let open_slots = (0..SLOTS)
                .filter(|at| !mine[*at] && inherited[*at].is_none())
                .fold(0u128, |w, at| w | 1 << at);
            let given = authored(engine, p, k, groups, with_roles(open_slots));
            for at in (0..SLOTS).filter(|_| given != 0) {
                if mine[at] || inherited[at].is_some() {
                    continue;
                }
                let i = at % ROLE_COUNT;
                let c = match at / ROLE_COUNT {
                    0 => given_colour(engine, p, k, given, false, ACTIVE, i),
                    g => given_colour(engine, p, k, given, groups, g - 1, i),
                };
                if c.is_some() {
                    inherited[at] = c;
                    open -= 1;
                }
            }
        }
        cur = above(engine, p);
    }
    let has_palette = ljos_quick::palette::has_own(engine, item);
    let plain = engine.extension::<PlainItems>();
    let style = style_roles(engine).unwrap_or(&ROLES);
    for at in (0..SLOTS).filter(|at| !mine[*at]) {
        let p = prop(at);
        // Nothing above, and the item still has the style's colour.
        if inherited[at].is_none() && !has_palette && !engine.is_made_on_read(item, p) {
            continue;
        }
        let role = &style[at % ROLE_COUNT];
        let fallback = if at / ROLE_COUNT == DISABLED + 1 {
            role.disabled
        } else {
            role.active
        };
        let c = Value::Color(inherited[at].unwrap_or(fallback));
        let done = if !engine.is_observed(item, p) {
            engine.make_on_read(item, p, move |_| c)
        } else {
            plain.handing.set(true);
            let done = engine.write_derived(item, p, c);
            plain.handing.set(false);
            done
        };
        check(engine, "palette", done);
    }
}

/// Which role of `obj`'s palette `prop` is, and in which group, if it is
/// one of a group's.
fn role_at(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
) -> Option<(Kind, Option<usize>, usize)> {
    let kind = kind_of(engine, obj)?;
    if let Some(i) = roles_of(kind).iter().position(|p| *p == prop) {
        return Some((kind, None, i));
    }
    groups_of(kind).iter().enumerate().find_map(|(g, props)| {
        let i = props.iter().position(|p| *p == prop)?;
        Some((kind, Some(g), i))
    })
}

/// What the role is where the object has no colour of its own (tracked):
/// the nearest ancestor's that was given one, or the style's. Where the
/// role is a group's, a colour set on the object's palette is one.
fn role_from_above(engine: &Engine, obj: ObjectId, prop: PropIndex) -> Value {
    let Some((kind, group, i)) = role_at(engine, obj, prop) else {
        // A popup's colours are its item's.
        return crate::popup::look_from_above(engine, obj, prop)
            .unwrap_or_else(|| engine.default_value(obj, prop));
    };
    let g = match group {
        Some(g) => {
            let p = roles_of(kind)[i];
            if author_set(engine, obj, p) {
                return engine.read(obj, p);
            }
            g
        }
        None => {
            let g = group_now(engine, obj, kind);
            let p = groups_of(kind)[g][i];
            if has_groups(engine, obj) && author_set(engine, obj, p) {
                return engine.read(obj, p);
            }
            g
        }
    };
    if in_use(engine, Which::Palette) {
        let mut cur = above(engine, obj);
        for _ in 0..4096 {
            let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
                break;
            };
            if let Some(k) = kind_of(engine, p).filter(|k| hands_down(engine, p, *k))
                && let Some(c) = author_colour(engine, p, k, has_groups(engine, p), g, i)
            {
                return Value::Color(c);
            }
            cur = above(engine, p);
        }
    }
    let dark = ljos_quick::color_scheme(engine) == ColorScheme::Dark;
    Value::Color(style_roles(engine).unwrap_or(&ROLES)[i].colour(dark, g != DISABLED))
}

/// The colours of a style other than Basic whose module the application
/// uses: as in Qt, the style an application imports gives every control
/// its palette, the controls it has no file of too. DECISION: a bare
/// `Control` or `Container` of `import QtQuick.Controls` is the template's
/// type, not the style's, so an application that makes nothing else keeps
/// Basic's colours.
fn style_roles(engine: &Engine) -> Option<&'static [Role; ROLE_COUNT]> {
    engine
        .uses_module("QtQuick.Controls.Fusion")
        .then_some(&FUSION_ROLES)
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
    // The items below had the colour from here.
    colours_changed(engine, obj);
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

    /// How many times the plain items in a menu looked for their colours
    /// when `bound` of the colours bound on the menu changed.
    fn looks_on_switch(bound: &[&str]) -> u64 {
        let colours: String = bound
            .iter()
            .map(|r| format!("palette.{r}: dark ? \"black\" : \"white\"\n"))
            .collect();
        let source = format!(
            r#"
import QtQuick
import QtQuick.Controls.Basic
Item {{
    property bool dark: false
    Menu {{
        {colours}
        Repeater {{ model: 4; MenuItem {{ }} }}
    }}
}}
"#
        );
        let mut project = ljos_compiler::Project::new(qml_front::QtTypeOracle);
        project.add("Main", &source);
        let c = ljos_compiler::compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
        assert!(!c.has_errors(), "{:?}", c.diagnostics);
        let mut files = ljos_engine::MapSource::new();
        files.insert("Main", c.file);
        let e = Engine::new();
        ljos_quick::register(&e);
        crate::register(&e);
        e.set_component_source(files);
        let root = e.create_from_url("Main", None, Vec::new()).unwrap();
        let looks = &e.extension::<PlainItems>().looks;
        let before = looks.get();
        e.write_by_name(root, "dark", Value::Bool(true)).unwrap();
        looks.get() - before
    }

    /// Each colour bound on a menu was handed to the plain items in it on
    /// its own, and twice: by the watch on the popup's item and by the
    /// watch on its groups. They are handed down together, once.
    #[test]
    fn colours_bound_together_are_handed_down_once() {
        let one = looks_on_switch(&["base"]);
        let three = looks_on_switch(&["base", "text", "highlight"]);
        assert!(one > 0);
        assert_eq!(three, one, "3 colours: {three} looks, 1 colour: {one}");
    }
}
