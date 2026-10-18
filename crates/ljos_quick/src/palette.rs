//! The `palette` of an item, a window and a popup: the colours a style
//! draws with.
//!
//! The numbers are the Basic style's palettes as `qml6` prints them for
//! an enabled and for a disabled control, in a light and in a dark colour
//! scheme (Qt 6.11.2); see `features/controls/control-palette.feature` and
//! `control-palette-dark.feature`. `accent` and `alternateBase` are the
//! platform's on Qt and not the style's: the values here are the ones the
//! offscreen platform gave.
//!
//! Every item has the group, and an item that sets none of its colours
//! has no palette of its own, only a pointer ([`crate::Item::palette`]).
//! Which colour an item has where nobody set one is the controls' business
//! (`ljos_controls::inherit`): it says so through [`set_role_reset`] and
//! [`set_on_item_palette`].
//!
//! TODO(palette): the `active`, `inactive` and `disabled` sub-groups and
//! a whole `Palette {}` assigned at once are not built.

use std::cell::Cell;

use ljos_engine::{
    Color, Engine, EngineError, MetaObject, ObjectId, PropIndex, QmlObject, Reset, ResetApply,
    ResetValue, Value,
};

/// `palette.button = undefined`: as the controls say ([`set_role_reset`]),
/// or the colour a new palette has.
pub const ROLE_RESET: Reset = Reset::Custom {
    apply: take_role_back,
    value: role_from_above,
};

/// Told of an item that completes with a colour set on its palette.
pub type OnItemPalette = fn(&Engine, ObjectId);

#[derive(Default)]
struct Hooks {
    reset: Cell<Option<(ResetApply, ResetValue)>>,
    on_item: Cell<Option<OnItemPalette>>,
}

/// How a colour of a palette is taken back, and what it is then.
pub fn set_role_reset(engine: &Engine, apply: ResetApply, value: ResetValue) {
    engine.extension::<Hooks>().reset.set(Some((apply, value)));
}

/// What is told of an item that completes with a colour of its palette
/// set, so that the controls inside it can take it.
pub fn set_on_item_palette(engine: &Engine, hook: OnItemPalette) {
    engine.extension::<Hooks>().on_item.set(Some(hook));
}

/// Whether a colour of the item's palette was ever set: it has a palette
/// of its own.
pub fn has_own(engine: &Engine, item: ObjectId) -> bool {
    engine
        .with(item, |i: &crate::Item| i.palette.is_some())
        .unwrap_or(false)
}

pub(crate) fn item_palette_set(engine: &Engine, item: ObjectId) {
    if let Some(hook) = engine.extension::<Hooks>().on_item.get() {
        hook(engine, item);
    }
}

fn take_role_back(engine: &Engine, obj: ObjectId, prop: PropIndex) -> Result<(), EngineError> {
    match engine.extension::<Hooks>().reset.get() {
        Some((apply, _)) => apply(engine, obj, prop),
        None => engine.reset_to_default(obj, prop),
    }
}

fn role_from_above(engine: &Engine, obj: ObjectId, prop: PropIndex) -> Value {
    match engine.extension::<Hooks>().reset.get() {
        Some((_, value)) => value(engine, obj, prop),
        None => engine.default_value(obj, prop),
    }
}

/// The `palette` group.
#[derive(Clone, Debug, QmlObject)]
#[qml(name = "Palette", uncreatable)]
pub struct Palette {
    #[qml(notify, reset = ROLE_RESET)]
    pub accent: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub alternate_base: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub base: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub bright_text: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub button: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub button_text: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub dark: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub highlight: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub highlighted_text: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub light: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub link: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub link_visited: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub mid: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub midlight: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub placeholder_text: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub shadow: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub text: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub tool_tip_base: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub tool_tip_text: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub window: Color,
    #[qml(notify, reset = ROLE_RESET)]
    pub window_text: Color,
}

/// How many colour roles a palette has.
pub const ROLE_COUNT: usize = 21;

/// One colour role: its QML name and what it is when nobody sets it.
#[derive(Clone, Copy, Debug)]
pub struct Role {
    pub name: &'static str,
    /// In a light colour scheme.
    pub active: Color,
    /// For a control that is not enabled.
    pub disabled: Color,
    /// In a dark colour scheme.
    pub dark_active: Color,
    pub dark_disabled: Color,
}

impl Role {
    /// The colour of the role where nobody set one.
    pub const fn colour(&self, dark: bool, enabled: bool) -> Color {
        match (dark, enabled) {
            (false, true) => self.active,
            (false, false) => self.disabled,
            (true, true) => self.dark_active,
            (true, false) => self.dark_disabled,
        }
    }
}

const fn rgb(v: u32) -> Color {
    Color::from_rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

const fn argb(v: u32) -> Color {
    Color::from_rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8)
}

/// A role that is the same whether the control is enabled or not.
const fn same(name: &'static str, light: Color, dark: Color) -> Role {
    Role {
        name,
        active: light,
        disabled: light,
        dark_active: dark,
        dark_disabled: dark,
    }
}

/// A role with a second colour for a control that is not enabled: light,
/// then dark, each as (enabled, disabled).
const fn paler(name: &'static str, light: (Color, Color), dark: (Color, Color)) -> Role {
    Role {
        name,
        active: light.0,
        disabled: light.1,
        dark_active: dark.0,
        dark_disabled: dark.1,
    }
}

/// In the order of [`Palette`]'s fields.
pub const ROLES: [Role; ROLE_COUNT] = [
    paler(
        "accent",
        (rgb(0x308cc6), rgb(0x919191)),
        (rgb(0x308cc6), rgb(0x919191)),
    ),
    same("alternateBase", rgb(0xf7f7f7), rgb(0x2b2b2b)),
    paler(
        "base",
        (rgb(0xffffff), rgb(0xd6d6d6)),
        (rgb(0x000000), rgb(0x292929)),
    ),
    paler(
        "brightText",
        (rgb(0xffffff), argb(0x4dffffff)),
        (rgb(0x000000), argb(0x4d000000)),
    ),
    same("button", rgb(0xe0e0e0), rgb(0x2f2f2f)),
    paler(
        "buttonText",
        (rgb(0x26282a), argb(0x4d26282a)),
        (rgb(0xd4d6d8), argb(0x4dd4d6d8)),
    ),
    same("dark", rgb(0x353637), rgb(0xc8c9cb)),
    paler(
        "highlight",
        (rgb(0x0066ff), rgb(0xf0f6ff)),
        (rgb(0x0d69f2), rgb(0x01060f)),
    ),
    same("highlightedText", rgb(0x090909), rgb(0xfdfdfd)),
    same("light", rgb(0xf6f6f6), rgb(0x1a1a1a)),
    same("link", rgb(0x45a7d7), rgb(0x2f86b1)),
    same("linkVisited", rgb(0xff00ff), rgb(0xff00ff)),
    same("mid", rgb(0xbdbdbd), rgb(0x626262)),
    same("midlight", rgb(0xe4e4e4), rgb(0x2c2c2c)),
    same("placeholderText", argb(0x88353637), argb(0x88c8c9cb)),
    same("shadow", rgb(0x28282a), rgb(0x28282a)),
    paler(
        "text",
        (rgb(0x353637), argb(0x7f353637)),
        (rgb(0xeff0f2), argb(0x7fc8c9cb)),
    ),
    same("toolTipBase", rgb(0xffffff), rgb(0x000000)),
    same("toolTipText", rgb(0x000000), rgb(0xffffff)),
    same("window", rgb(0xffffff), rgb(0x000000)),
    paler(
        "windowText",
        (rgb(0x26282a), rgb(0xbdbebf)),
        (rgb(0xd4d6d8), rgb(0x3f4040)),
    ),
];

impl Default for Palette {
    fn default() -> Self {
        let c = |i: usize| ROLES[i].active;
        Palette {
            accent: c(0),
            alternate_base: c(1),
            base: c(2),
            bright_text: c(3),
            button: c(4),
            button_text: c(5),
            dark: c(6),
            highlight: c(7),
            highlighted_text: c(8),
            light: c(9),
            link: c(10),
            link_visited: c(11),
            mid: c(12),
            midlight: c(13),
            placeholder_text: c(14),
            shadow: c(15),
            text: c(16),
            tool_tip_base: c(17),
            tool_tip_text: c(18),
            window: c(19),
            window_text: c(20),
        }
    }
}

/// The properties of the roles on a type that has a `palette` group, in
/// [`ROLES`]' order.
pub fn role_props(meta: &MetaObject) -> [PropIndex; ROLE_COUNT] {
    std::array::from_fn(|i| {
        meta.property_index(&format!("palette.{}", ROLES[i].name))
            .expect("the type has a palette group")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ljos_engine::QmlType;

    #[test]
    fn the_role_table_names_the_fields_in_order() {
        let meta = Palette::static_meta();
        for (i, role) in ROLES.iter().enumerate() {
            let p = meta.property_index(role.name);
            assert_eq!(p.map(|p| p.0 as usize), Some(i), "{}", role.name);
        }
    }
}
