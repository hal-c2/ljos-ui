//! The `palette` of a control: the colours a style draws with.
//!
//! The numbers are the Basic style's light palette as `qml6` prints it
//! for an enabled and for a disabled control (Qt 6.11.2); see
//! `features/controls/control-palette.feature`.
//!
//! TODO(palette): the `active`, `inactive` and `disabled` sub-groups, a
//! whole `Palette {}` assigned at once, and the dark scheme are not built.

use ljos_engine::{Color, MetaObject, PropIndex, QmlObject};

/// The `palette` group of a control, a label and a window.
#[derive(Clone, Debug, QmlObject)]
#[qml(name = "Palette", uncreatable)]
pub struct Palette {
    #[qml(notify)]
    pub accent: Color,
    #[qml(notify)]
    pub alternate_base: Color,
    #[qml(notify)]
    pub base: Color,
    #[qml(notify)]
    pub bright_text: Color,
    #[qml(notify)]
    pub button: Color,
    #[qml(notify)]
    pub button_text: Color,
    #[qml(notify)]
    pub dark: Color,
    #[qml(notify)]
    pub highlight: Color,
    #[qml(notify)]
    pub highlighted_text: Color,
    #[qml(notify)]
    pub light: Color,
    #[qml(notify)]
    pub link: Color,
    #[qml(notify)]
    pub link_visited: Color,
    #[qml(notify)]
    pub mid: Color,
    #[qml(notify)]
    pub midlight: Color,
    #[qml(notify)]
    pub placeholder_text: Color,
    #[qml(notify)]
    pub shadow: Color,
    #[qml(notify)]
    pub text: Color,
    #[qml(notify)]
    pub tool_tip_base: Color,
    #[qml(notify)]
    pub tool_tip_text: Color,
    #[qml(notify)]
    pub window: Color,
    #[qml(notify)]
    pub window_text: Color,
}

/// How many colour roles a palette has.
pub(crate) const ROLE_COUNT: usize = 21;

/// One colour role: its QML name and what it is when nobody sets it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Role {
    pub name: &'static str,
    pub active: Color,
    /// For a control that is not enabled.
    pub disabled: Color,
}

const fn rgb(v: u32) -> Color {
    Color::from_rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

const fn argb(v: u32) -> Color {
    Color::from_rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8)
}

const fn same(name: &'static str, c: Color) -> Role {
    Role {
        name,
        active: c,
        disabled: c,
    }
}

/// In the order of [`Palette`]'s fields.
pub(crate) const ROLES: [Role; ROLE_COUNT] = [
    Role {
        name: "accent",
        active: rgb(0x308cc6),
        disabled: rgb(0x919191),
    },
    same("alternateBase", rgb(0xf7f7f7)),
    Role {
        name: "base",
        active: rgb(0xffffff),
        disabled: rgb(0xd6d6d6),
    },
    Role {
        name: "brightText",
        active: rgb(0xffffff),
        disabled: argb(0x4dffffff),
    },
    same("button", rgb(0xe0e0e0)),
    Role {
        name: "buttonText",
        active: rgb(0x26282a),
        disabled: argb(0x4d26282a),
    },
    same("dark", rgb(0x353637)),
    Role {
        name: "highlight",
        active: rgb(0x0066ff),
        disabled: rgb(0xf0f6ff),
    },
    same("highlightedText", rgb(0x090909)),
    same("light", rgb(0xf6f6f6)),
    same("link", rgb(0x45a7d7)),
    same("linkVisited", rgb(0xff00ff)),
    same("mid", rgb(0xbdbdbd)),
    same("midlight", rgb(0xe4e4e4)),
    same("placeholderText", argb(0x88353637)),
    same("shadow", rgb(0x28282a)),
    Role {
        name: "text",
        active: rgb(0x353637),
        disabled: argb(0x7f353637),
    },
    same("toolTipBase", rgb(0xffffff)),
    same("toolTipText", rgb(0x000000)),
    same("window", rgb(0xffffff)),
    Role {
        name: "windowText",
        active: rgb(0x26282a),
        disabled: rgb(0xbdbebf),
    },
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
pub(crate) fn role_props(meta: &MetaObject) -> [PropIndex; ROLE_COUNT] {
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
