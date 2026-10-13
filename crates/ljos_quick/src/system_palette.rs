//! `SystemPalette` (QtQuick): the colours of the system's theme.
//!
//! Ljós reads one thing of the theme, its colour scheme
//! ([`crate::color_scheme`]), so the palette is one of two: the colours Qt
//! gave on a GNOME desktop with Adwaita light and dark
//! (features/platform/system-palette.feature). A palette follows the
//! scheme while the application runs.

use ljos_a11y::ColorScheme;
use ljos_engine::{
    Color, Cx, Engine, EngineError, LogLevel, ObjectId, PropIndex, QmlObject, QtObject, Value,
    qml_impl,
};

use crate::enums::EnumTable;
use crate::item::write_computed;

/// `SystemPalette.ColorGroup`.
pub mod color_group {
    pub const ACTIVE: i32 = 0;
    pub const DISABLED: i32 = 1;
    pub const INACTIVE: i32 = 2;
}

pub(crate) fn system_palette_enums() -> EnumTable {
    vec![(
        "ColorGroup",
        vec![
            ("Active", i64::from(color_group::ACTIVE)),
            ("Disabled", i64::from(color_group::DISABLED)),
            ("Inactive", i64::from(color_group::INACTIVE)),
        ],
    )]
}

/// `SystemPalette` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "SystemPalette",
    extends = QtObject,
    enums = system_palette_enums
)]
pub struct SystemPalette {
    #[qml(base)]
    pub object: QtObject,
    /// [`color_group`].
    #[qml(notify, enumeration = "ColorGroup")]
    pub color_group: i32,
    #[qml(notify, readonly)]
    pub window: Color,
    #[qml(notify, readonly)]
    pub window_text: Color,
    #[qml(notify, readonly)]
    pub base: Color,
    #[qml(notify, readonly)]
    pub text: Color,
    #[qml(notify, readonly)]
    pub alternate_base: Color,
    #[qml(notify, readonly)]
    pub button: Color,
    #[qml(notify, readonly)]
    pub button_text: Color,
    #[qml(notify, readonly)]
    pub light: Color,
    #[qml(notify, readonly)]
    pub midlight: Color,
    #[qml(notify, readonly)]
    pub dark: Color,
    #[qml(notify, readonly)]
    pub mid: Color,
    #[qml(notify, readonly)]
    pub shadow: Color,
    #[qml(notify, readonly)]
    pub highlight: Color,
    #[qml(notify, readonly)]
    pub highlighted_text: Color,
    #[qml(notify, readonly)]
    pub placeholder_text: Color,
    #[qml(notify, readonly)]
    pub accent: Color,
}

impl Default for SystemPalette {
    fn default() -> Self {
        let c = colours(ColorScheme::Light, color_group::ACTIVE);
        SystemPalette {
            object: QtObject::default(),
            color_group: color_group::ACTIVE,
            window: c[0],
            window_text: c[1],
            base: c[2],
            text: c[3],
            alternate_base: c[4],
            button: c[5],
            button_text: c[6],
            light: c[7],
            midlight: c[8],
            dark: c[9],
            mid: c[10],
            shadow: c[11],
            highlight: c[12],
            highlighted_text: c[13],
            placeholder_text: c[14],
            accent: c[15],
        }
    }
}

/// The roles in the order of [`colours`].
fn roles() -> [PropIndex; 16] {
    [
        SystemPalette::WINDOW.index,
        SystemPalette::WINDOW_TEXT.index,
        SystemPalette::BASE.index,
        SystemPalette::TEXT.index,
        SystemPalette::ALTERNATE_BASE.index,
        SystemPalette::BUTTON.index,
        SystemPalette::BUTTON_TEXT.index,
        SystemPalette::LIGHT.index,
        SystemPalette::MIDLIGHT.index,
        SystemPalette::DARK.index,
        SystemPalette::MID.index,
        SystemPalette::SHADOW.index,
        SystemPalette::HIGHLIGHT.index,
        SystemPalette::HIGHLIGHTED_TEXT.index,
        SystemPalette::PLACEHOLDER_TEXT.index,
        SystemPalette::ACCENT.index,
    ]
}

const fn rgb(v: u32) -> Color {
    Color::from_rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, 0xff)
}

/// The active group of each scheme, in the order of [`roles`].
const LIGHT: [u32; 16] = [
    0xfaf9f8, 0x000000, 0xfaf9f8, 0x000000, 0xeae9e8, 0xfaf9f8, 0x000000, 0xffffff, 0xffffff,
    0xdbdada, 0xffffff, 0x0c0c0c, 0x3584e4, 0xffffff, 0x646464, 0x000000,
];
const DARK: [u32; 16] = [
    0x323232, 0xffffff, 0x323232, 0xffffff, 0x2f2f2f, 0x323232, 0xffffff, 0x3e3e3e, 0x383838,
    0x2c2c2c, 0x383838, 0x020202, 0x15539e, 0xffffff, 0x9b9b9b, 0x000000,
];

/// One group of one scheme. The inactive and disabled groups differ from
/// the active one only in their text colours.
fn colours(scheme: ColorScheme, group: i32) -> [Color; 16] {
    let dark = scheme == ColorScheme::Dark;
    let mut table = if dark { DARK } else { LIGHT };
    match group {
        color_group::INACTIVE => table[1] = if dark { 0x919190 } else { 0x929595 },
        color_group::DISABLED => {
            let grey = if dark { 0x989898 } else { 0x7d7c7c };
            table[1] = grey;
            table[3] = grey;
            table[6] = grey;
            if !dark {
                table[13] = 0x000000;
            }
        }
        _ => {}
    }
    table.map(rgb)
}

#[qml_impl]
impl SystemPalette {
    #[qml(signal, name = "paletteChanged")]
    fn palette_changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        // Tracked: the scheme and the group, so a change of either runs it
        // again.
        cx.engine.effect_in(me, move |e| show(e, me))?;
        Ok(())
    }
}

fn show(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) {
        return;
    }
    let scheme = crate::color_scheme(e);
    let group = e.read(me, SystemPalette::COLOR_GROUP.index).to_number() as i32;
    let colours = colours(scheme, group);
    e.untrack(|| {
        let before = e.read(me, SystemPalette::WINDOW_TEXT.index);
        let was = e.read(me, SystemPalette::WINDOW.index);
        e.batch(|| {
            for (role, colour) in roles().into_iter().zip(colours) {
                if let Err(err) = write_computed(e, me, role, Value::Color(colour)) {
                    e.log(LogLevel::Warning, "qml", format!("{err}"));
                }
            }
        });
        let changed = !was.change_equals(&e.read(me, SystemPalette::WINDOW.index))
            || !before.change_equals(&e.read(me, SystemPalette::WINDOW_TEXT.index));
        if changed {
            let _ = e.emit_by_name(me, "paletteChanged", &[]);
        }
    });
}
