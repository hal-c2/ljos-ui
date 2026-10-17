//! The built-in styles (`Basic`, `Material`, `Fusion`, `Universal`): QML files
//! of this crate (`qml/`), compiled to IR by the build script and embedded
//! here, plus the native style types `Material`, `Fusion` and `Universal`.
//! Nothing parses QML at run time.
//!
//! Note: `Material`, `Fusion` and `Universal` currently draw with Basic-derived
//! QML components under their own documented metrics, palettes and attached
//! style properties rather than full custom design-system shaders.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ljos_engine::{
    Color, ComponentSource, Engine, EngineError, MetaRef, NodeId, ObjectId, PropIndex, Provenance,
    QmlObject, QmlType, QmlValue, Reset, Value, Version, qml_singleton,
};
use ljos_ir::CompiledFile;
use ljos_quick::Item;

/// What `build.rs` packed: a count, then per file its key and its IR, each
/// behind a little-endian `u32` length.
static PACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/style.bin"));

/// Where the style's files say they are, in messages and to
/// [`is_style_url`].
const URL_PREFIX: &str = "ljos-controls:/";

/// The directory of the Basic style in the pack.
pub(crate) const BASIC: &str = "QtQuick/Controls/Basic";
/// The directory of the Material style in the pack.
pub(crate) const MATERIAL: &str = "QtQuick/Controls/Material";
/// The directory of the Fusion style in the pack.
pub(crate) const FUSION: &str = "QtQuick/Controls/Fusion";
/// The directory of the Universal style in the pack.
pub(crate) const UNIVERSAL: &str = "QtQuick/Controls/Universal";

/// Whether an object declared at `location` (as
/// [`ljos_engine::Engine::declared_at`] gives it) was declared by a style
/// file, not by an application.
pub(crate) fn is_style_url(location: &str) -> bool {
    location.starts_with(URL_PREFIX)
}

/// The embedded files. A file's IR is decoded when a type made of it is
/// first registered, and shared from then on.
#[derive(Debug)]
pub(crate) struct StyleLibrary {
    directory: String,
    files: HashMap<String, Arc<CompiledFile>>,
}

impl StyleLibrary {
    /// Reads the pack. A file that does not decode (the IR version moved
    /// on without a rebuild) is left out and named in the result.
    pub(crate) fn embedded() -> (BTreeMap<String, Self>, Vec<String>) {
        let mut libraries: BTreeMap<String, Self> = BTreeMap::new();
        let mut bad = Vec::new();
        for (key, bytes) in entries(PACK) {
            let Some((directory, _)) = key.rsplit_once('/') else {
                bad.push(format!("invalid style key {key}"));
                continue;
            };
            match CompiledFile::from_bytes(bytes) {
                Ok(f) => {
                    let library = libraries
                        .entry(directory.to_owned())
                        .or_insert_with(|| Self {
                            directory: directory.to_owned(),
                            files: HashMap::new(),
                        });
                    library.files.insert(key.to_owned(), Arc::new(f));
                }
                Err(e) => bad.push(format!("{key}: {e}")),
            }
        }
        (libraries, bad)
    }

    /// The type names this style has files for, sorted.
    pub(crate) fn type_names(&self) -> Vec<String> {
        let prefix = format!("{}/", self.directory);
        let mut names: Vec<String> = self
            .files
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|n| !n.contains('/'))
            .map(str::to_owned)
            .collect();
        names.sort();
        names
    }
}

impl ComponentSource for StyleLibrary {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        self.files.get(key).cloned()
    }

    fn url(&self, key: &str) -> String {
        format!("{URL_PREFIX}{key}.qml")
    }

    fn key(&self, url: &str) -> String {
        let s = url.strip_prefix(URL_PREFIX).unwrap_or(url);
        s.strip_suffix(".qml").unwrap_or(s).to_owned()
    }
}

/// The `(key, IR)` pairs of a pack; stops at the first entry that does not
/// fit, which only a truncated file has.
fn entries(pack: &[u8]) -> Vec<(&str, &[u8])> {
    fn take<'a>(rest: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        if rest.len() < n {
            return None;
        }
        let (head, tail) = rest.split_at(n);
        *rest = tail;
        Some(head)
    }
    fn len(rest: &mut &[u8]) -> Option<usize> {
        let b = take(rest, 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }
    let mut rest = pack;
    let mut out = Vec::new();
    let Some(count) = len(&mut rest) else {
        return out;
    };
    for _ in 0..count {
        let entry = (|| {
            let n = len(&mut rest)?;
            let key = std::str::from_utf8(take(&mut rest, n)?).ok()?;
            let n = len(&mut rest)?;
            Some((key, take(&mut rest, n)?))
        })();
        match entry {
            Some(e) => out.push(e),
            None => break,
        }
    }
    out
}

// ============================================================================
// Shared attached-style inheritance helpers
// ============================================================================

#[derive(Default)]
struct StyleEpoch(Cell<Option<NodeId>>);

fn style_epoch(engine: &Engine) -> Option<NodeId> {
    let slot = engine.extension::<StyleEpoch>();
    if slot.0.get().is_none() {
        let g = engine.graph();
        slot.0.set(g.property(g.root(), Value::Int(0)).ok());
    }
    slot.0.get()
}

fn bump_style_epoch(engine: &Engine) {
    if let Some(node) = style_epoch(engine) {
        let g = engine.graph();
        let n = g.get_untracked(node).to_number() as i32;
        g.set(node, Value::Int(n.wrapping_add(1)));
    }
}

fn author_set(engine: &Engine, obj: ObjectId, prop: PropIndex) -> bool {
    match engine.provenance_tracked(obj, prop) {
        Some(Provenance::Assigned | Provenance::Bound) => true,
        Some(Provenance::Animated) => {
            // Resolving an enum to a color uses a derived write. Keep the
            // author's raw choice until a reset explicitly clears it.
            engine
                .with::<MaterialAttached, _>(obj, |m| {
                    let raw = if prop == MaterialAttached::PRIMARY.index {
                        m.raw_primary.get()
                    } else if prop == MaterialAttached::ACCENT.index {
                        m.raw_accent.get()
                    } else if prop == MaterialAttached::FOREGROUND.index {
                        m.raw_foreground.get()
                    } else if prop == MaterialAttached::BACKGROUND.index {
                        m.raw_background.get()
                    } else {
                        RawColor::Unset
                    };
                    raw != RawColor::Unset
                })
                .unwrap_or(false)
                || engine
                    .with::<UniversalAttached, _>(obj, |u| {
                        let raw = if prop == UniversalAttached::ACCENT.index {
                            u.raw_accent.get()
                        } else if prop == UniversalAttached::FOREGROUND.index {
                            u.raw_foreground.get()
                        } else if prop == UniversalAttached::BACKGROUND.index {
                            u.raw_background.get()
                        } else {
                            RawColor::Unset
                        };
                        raw != RawColor::Unset
                    })
                    .unwrap_or(false)
        }
        _ => false,
    }
}

fn above(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    if is_a::<Item>(engine, obj) {
        let _ = engine.read(obj, Item::PARENT.index);
    }
    if let Some(popup) = crate::popup::popup_of(engine, obj)
        && let Value::Object(p) = engine.read(popup, crate::popup::Popup::PARENT.index)
        && engine.is_alive(p)
    {
        return Some(p);
    }
    engine
        .visual_parent(obj)
        .or_else(|| engine.owner(obj))
        .or_else(|| ljos_quick::layer_host(engine, obj))
}

fn is_a<T: QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine.with::<T, _>(obj, |_| ()).is_ok()
}

fn write_if_changed(engine: &Engine, obj: ObjectId, prop: PropIndex, val: Value) {
    let cur = engine.untrack(|| engine.read(obj, prop));
    if !cur.change_equals(&val) {
        let _ = engine.write_derived(obj, prop, val);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RawColor {
    Unset,
    Enum(i32),
    Direct(Color),
}

// ============================================================================
// Material style (`QtQuick.Controls.Material`)
// ============================================================================

fn material_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        ("Theme", vec![("Light", 0), ("Dark", 1), ("System", 2)]),
        ("Variant", vec![("Normal", 0), ("Dense", 1)]),
        (
            "Color",
            vec![
                ("Red", 0),
                ("Pink", 1),
                ("Purple", 2),
                ("DeepPurple", 3),
                ("Indigo", 4),
                ("Blue", 5),
                ("LightBlue", 6),
                ("Cyan", 7),
                ("Teal", 8),
                ("Green", 9),
                ("LightGreen", 10),
                ("Lime", 11),
                ("Yellow", 12),
                ("Amber", 13),
                ("Orange", 14),
                ("DeepOrange", 15),
                ("Brown", 16),
                ("Grey", 17),
                ("BlueGrey", 18),
            ],
        ),
        (
            "Shade",
            vec![
                ("Shade50", 0),
                ("Shade100", 1),
                ("Shade200", 2),
                ("Shade300", 3),
                ("Shade400", 4),
                ("Shade500", 5),
                ("Shade600", 6),
                ("Shade700", 7),
                ("Shade800", 8),
                ("Shade900", 9),
                ("ShadeA100", 10),
                ("ShadeA200", 11),
                ("ShadeA400", 12),
                ("ShadeA700", 13),
            ],
        ),
        (
            "RoundedScale",
            vec![
                ("NotRounded", 0),
                ("ExtraSmallScale", 4),
                ("SmallScale", 8),
                ("MediumScale", 12),
                ("LargeScale", 16),
                ("ExtraLargeScale", 28),
                ("FullScale", 255),
            ],
        ),
        ("ContainerStyle", vec![("Filled", 0), ("Outlined", 1)]),
    ]
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgba8(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
        0xff,
    )
}

const fn argb(hex: u32) -> Color {
    Color::from_rgba8(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
        ((hex >> 24) & 0xff) as u8,
    )
}

const MATERIAL_SHADES: [[u32; 14]; 19] = [
    // 0: Red
    [
        0xffebee, 0xffcdd2, 0xef9a9a, 0xe57373, 0xef5350, 0xf44336, 0xe53935, 0xd32f2f, 0xc62828,
        0xb71c1c, 0xff8a80, 0xff5252, 0xff1744, 0xd50000,
    ],
    // 1: Pink
    [
        0xfce4ec, 0xf8bbd0, 0xf48fb1, 0xf06292, 0xec407a, 0xe91e63, 0xd81b60, 0xc2185b, 0xad1457,
        0x880e4f, 0xff80ab, 0xff4081, 0xf50057, 0xc51162,
    ],
    // 2: Purple
    [
        0xf3e5f5, 0xe1bee7, 0xce93d8, 0xba68c8, 0xab47bc, 0x9c27b0, 0x8e24aa, 0x7b1fa2, 0x6a1b9a,
        0x4a148c, 0xea80fc, 0xe040fb, 0xd500f9, 0xaa00ff,
    ],
    // 3: DeepPurple
    [
        0xede7f6, 0xd1c4e9, 0xb39ddb, 0x9575cd, 0x7e57c2, 0x673ab7, 0x5e35b1, 0x512da8, 0x4527a0,
        0x311b92, 0xb388ff, 0x7c4dff, 0x651fff, 0x6200ea,
    ],
    // 4: Indigo
    [
        0xe8eaf6, 0xc5cae9, 0x9fa8da, 0x7986cb, 0x5c6bc0, 0x3f51b5, 0x3949ab, 0x303f9f, 0x283593,
        0x1a237e, 0x8c9eff, 0x536dfe, 0x3d5afe, 0x304ffe,
    ],
    // 5: Blue
    [
        0xe3f2fd, 0xbbdefb, 0x90caf9, 0x64b5f6, 0x42a5f5, 0x2196f3, 0x1e88e5, 0x1976d2, 0x1565c0,
        0x0d47a1, 0x82b1ff, 0x448aff, 0x2979ff, 0x2962ff,
    ],
    // 6: LightBlue
    [
        0xe1f5fe, 0xb3e5fc, 0x81d4fa, 0x4fc3f7, 0x29b6f6, 0x03a9f4, 0x039be5, 0x0288d1, 0x0277bd,
        0x01579b, 0x80d8ff, 0x40c4ff, 0x00b0ff, 0x0091ea,
    ],
    // 7: Cyan
    [
        0xe0f7fa, 0xb2ebf2, 0x80deea, 0x4dd0e1, 0x26c6da, 0x00bcd4, 0x00acc1, 0x0097a7, 0x00838f,
        0x006064, 0x84ffff, 0x18ffff, 0x00e5ff, 0x00b8d4,
    ],
    // 8: Teal
    [
        0xe0f2f1, 0xb2dfdb, 0x80cbc4, 0x4db6ac, 0x26a69a, 0x009688, 0x00897b, 0x00796b, 0x00695c,
        0x004d40, 0xa7ffeb, 0x64ffda, 0x1de9b6, 0x00bfa5,
    ],
    // 9: Green
    [
        0xe8f5e9, 0xc8e6c9, 0xa5d6a7, 0x81c784, 0x66bb6a, 0x4caf50, 0x43a047, 0x388e3c, 0x2e7d32,
        0x1b5e20, 0xb9f6ca, 0x69f0ae, 0x00e676, 0x00c853,
    ],
    // 10: LightGreen
    [
        0xf1f8e9, 0xdcedc8, 0xc5e1a5, 0xaed581, 0x9ccc65, 0x8bc34a, 0x7cb342, 0x689f38, 0x558b2f,
        0x33691e, 0xccff90, 0xb2ff59, 0x76ff03, 0x64dd17,
    ],
    // 11: Lime
    [
        0xf9fbe7, 0xf0f4c3, 0xe6ee9c, 0xdce775, 0xd4e157, 0xcddc39, 0xc0ca33, 0xafb42b, 0x9e9d24,
        0x827717, 0xf4ff81, 0xeeff41, 0xc6ff00, 0xaeea00,
    ],
    // 12: Yellow
    [
        0xfffde7, 0xfff9c4, 0xfff59d, 0xfff176, 0xffee58, 0xffeb3b, 0xfdd835, 0xfbc02d, 0xf9a825,
        0xf57f17, 0xffff8d, 0xffff00, 0xffea00, 0xffd600,
    ],
    // 13: Amber
    [
        0xfff8e1, 0xffecb3, 0xffe082, 0xffd54f, 0xffca28, 0xffc107, 0xffb300, 0xffa000, 0xff8f00,
        0xff6f00, 0xffe57f, 0xffd740, 0xffc400, 0xffab00,
    ],
    // 14: Orange
    [
        0xfff3e0, 0xffe0b2, 0xffcc80, 0xffb74d, 0xffa726, 0xff9800, 0xfb8c00, 0xf57c00, 0xef6c00,
        0xe65100, 0xffd180, 0xffab40, 0xff9100, 0xff6d00,
    ],
    // 15: DeepOrange
    [
        0xfbe9e7, 0xffccbc, 0xffab91, 0xff8a65, 0xff7043, 0xff5722, 0xf4511e, 0xe64a19, 0xd84315,
        0xbf360c, 0xff9e80, 0xff6e40, 0xff3d00, 0xdd2c00,
    ],
    // 16: Brown
    [
        0xefebe9, 0xd7ccc8, 0xbcaaa4, 0xa1887f, 0x8d6e63, 0x795548, 0x6d4c41, 0x5d4037, 0x4e342e,
        0x3e2723, 0xd7ccc8, 0xbcaaa4, 0x8d6e63, 0x5d4037,
    ],
    // 17: Grey
    [
        0xfafafa, 0xf5f5f5, 0xeeeeee, 0xe0e0e0, 0xbdbdbd, 0x9e9e9e, 0x757575, 0x616161, 0x424242,
        0x212121, 0xf5f5f5, 0xeeeeee, 0xbdbdbd, 0x616161,
    ],
    // 18: BlueGrey
    [
        0xeceff1, 0xcfd8dc, 0xb0bec5, 0x90a4ae, 0x78909c, 0x607d8b, 0x546e7a, 0x455a64, 0x37474f,
        0x263238, 0xcfd8dc, 0xb0bec5, 0x78909c, 0x455a64,
    ],
];

fn material_color(color: i32, shade: i32) -> Color {
    let Ok(c) = usize::try_from(color) else {
        return Color::TRANSPARENT;
    };
    let Ok(s) = usize::try_from(shade) else {
        return Color::TRANSPARENT;
    };
    match MATERIAL_SHADES.get(c).and_then(|row| row.get(s)) {
        Some(&hex) => rgb(hex),
        None => Color::TRANSPARENT,
    }
}

/// `Material` (`QtQuick.Controls.Material`): attaches [`MaterialAttached`] and
/// exposes the Material theme, color, shade, scale and container enums.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Controls.Material",
    name = "Material",
    attached = MaterialAttached,
    enums = material_enums,
    uncreatable
)]
pub struct Material {}

/// What `Material.` attaches to an object.
#[derive(Debug, QmlObject)]
#[qml(name = "MaterialAttached", enums = material_enums)]
pub struct MaterialAttached {
    #[qml(notify, enumeration = "Theme", reset = MATERIAL_THEME_RESET)]
    pub theme: i32,
    #[qml(notify, reset = MATERIAL_PRIMARY_RESET)]
    pub primary: Value,
    #[qml(notify, reset = MATERIAL_ACCENT_RESET)]
    pub accent: Value,
    #[qml(notify, reset = MATERIAL_FOREGROUND_RESET)]
    pub foreground: Value,
    #[qml(notify, reset = MATERIAL_BACKGROUND_RESET)]
    pub background: Value,
    #[qml(notify, reset)]
    pub elevation: i32,
    #[qml(notify, enumeration = "RoundedScale", reset)]
    pub rounded_scale: i32,
    #[qml(notify, enumeration = "ContainerStyle", reset)]
    pub container_style: i32,

    #[qml(readonly, notify)]
    pub primary_color: Color,
    #[qml(readonly, notify)]
    pub accent_color: Color,
    #[qml(readonly, notify)]
    pub background_color: Color,
    #[qml(readonly, notify)]
    pub primary_text_color: Color,
    #[qml(readonly, notify)]
    pub secondary_text_color: Color,
    #[qml(readonly, notify)]
    pub hint_text_color: Color,
    #[qml(readonly, notify)]
    pub divider_color: Color,
    #[qml(readonly, notify)]
    pub frame_color: Color,
    #[qml(readonly, notify)]
    pub dialog_color: Color,
    #[qml(readonly, notify)]
    pub tool_bar_color: Color,

    #[qml(readonly, constant)]
    pub touch_target: i32,
    #[qml(readonly, constant)]
    pub button_height: i32,
    #[qml(readonly, constant)]
    pub delegate_height: i32,
    #[qml(readonly, constant)]
    pub text_field_height: i32,

    #[qml(skip)]
    raw_primary: Cell<RawColor>,
    #[qml(skip)]
    last_primary: Cell<Option<Color>>,
    #[qml(skip)]
    raw_accent: Cell<RawColor>,
    #[qml(skip)]
    last_accent: Cell<Option<Color>>,
    #[qml(skip)]
    raw_foreground: Cell<RawColor>,
    #[qml(skip)]
    last_foreground: Cell<Option<Color>>,
    #[qml(skip)]
    raw_background: Cell<RawColor>,
    #[qml(skip)]
    last_background: Cell<Option<Color>>,
}

impl Default for MaterialAttached {
    fn default() -> Self {
        let primary = rgb(0x3f51b5);
        let accent = rgb(0xe91e63);
        let fg = argb(0xdd000000);
        let bg = rgb(0xfffbfe);
        Self {
            theme: 0,
            primary: Value::Color(primary),
            accent: Value::Color(accent),
            foreground: Value::Color(fg),
            background: Value::Color(bg),
            elevation: 0,
            rounded_scale: 0,
            container_style: 0,
            primary_color: primary,
            accent_color: accent,
            background_color: bg,
            primary_text_color: fg,
            secondary_text_color: argb(0x89000000),
            hint_text_color: argb(0x60000000),
            divider_color: argb(0x1e000000),
            frame_color: argb(0x60000000),
            dialog_color: rgb(0xffffff),
            tool_bar_color: primary,
            touch_target: 48,
            button_height: 40,
            delegate_height: 48,
            text_field_height: 56,
            raw_primary: Cell::new(RawColor::Unset),
            last_primary: Cell::new(None),
            raw_accent: Cell::new(RawColor::Unset),
            last_accent: Cell::new(None),
            raw_foreground: Cell::new(RawColor::Unset),
            last_foreground: Cell::new(None),
            raw_background: Cell::new(RawColor::Unset),
            last_background: Cell::new(None),
        }
    }
}

#[ljos_engine::qml_impl]
impl MaterialAttached {
    #[qml(invokable)]
    pub fn color(&self, color: i32, shade: Value) -> Color {
        let s = i32::from_value(&shade).unwrap_or(5);
        material_color(color, s)
    }

    #[allow(clippy::too_many_arguments)]
    #[qml(invokable)]
    pub fn button_color(
        &self,
        theme: i32,
        background: Value,
        accent: Value,
        _enabled: bool,
        flat: bool,
        highlighted: bool,
        checked: bool,
    ) -> Color {
        let bg = Color::from_value(&background).unwrap_or(Color::TRANSPARENT);
        let acc = Color::from_value(&accent).unwrap_or(self.accent_color);
        let default_bg = if theme == 1 {
            rgb(0x1c1b1f)
        } else {
            rgb(0xfffbfe)
        };
        if self.raw_background.get() != RawColor::Unset || bg != default_bg {
            return bg;
        }
        if flat && !highlighted && !checked {
            return Color::TRANSPARENT;
        }
        if highlighted {
            return acc;
        }
        if theme == 1 {
            argb(0x3fcccccc)
        } else {
            rgb(0xd6d7d7)
        }
    }
}

fn material_attacher() -> MetaRef {
    MetaRef::Static(Material::static_meta())
}

const MATERIAL_THEME_RESET: Reset = Reset::Custom {
    apply: reset_material_theme,
    value: material_theme_value,
};
const MATERIAL_PRIMARY_RESET: Reset = Reset::Custom {
    apply: reset_material_primary,
    value: material_primary_value,
};
const MATERIAL_ACCENT_RESET: Reset = Reset::Custom {
    apply: reset_material_accent,
    value: material_accent_value,
};
const MATERIAL_FOREGROUND_RESET: Reset = Reset::Custom {
    apply: reset_material_foreground,
    value: material_foreground_value,
};
const MATERIAL_BACKGROUND_RESET: Reset = Reset::Custom {
    apply: reset_material_background,
    value: material_background_value,
};

fn material_theme_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    Value::Int(inherited_material_theme(engine, att))
}

fn reset_material_theme(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let v = engine.untrack(|| material_theme_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_material(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn material_primary_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    let theme = effective_material_theme(engine, att);
    Value::Color(inherited_material_color(
        engine,
        att,
        MaterialAttached::PRIMARY.index,
        theme,
        |m| m.raw_primary.get(),
        |_| rgb(0x3f51b5),
        false,
    ))
}

fn reset_material_primary(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let _ = engine.with::<MaterialAttached, _>(att, |m| {
        m.raw_primary.set(RawColor::Unset);
        m.last_primary.set(None);
    });
    let v = engine.untrack(|| material_primary_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_material(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn material_accent_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    let theme = effective_material_theme(engine, att);
    Value::Color(inherited_material_color(
        engine,
        att,
        MaterialAttached::ACCENT.index,
        theme,
        |m| m.raw_accent.get(),
        |t| {
            if t == 1 {
                material_color(1, 2)
            } else {
                rgb(0xe91e63)
            }
        },
        true,
    ))
}

fn reset_material_accent(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let _ = engine.with::<MaterialAttached, _>(att, |m| {
        m.raw_accent.set(RawColor::Unset);
        m.last_accent.set(None);
    });
    let v = engine.untrack(|| material_accent_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_material(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn material_foreground_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    let theme = effective_material_theme(engine, att);
    Value::Color(inherited_material_color(
        engine,
        att,
        MaterialAttached::FOREGROUND.index,
        theme,
        |m| m.raw_foreground.get(),
        |t| {
            if t == 1 {
                rgb(0xffffff)
            } else {
                argb(0xdd000000)
            }
        },
        false,
    ))
}

fn reset_material_foreground(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let _ = engine.with::<MaterialAttached, _>(att, |m| {
        m.raw_foreground.set(RawColor::Unset);
        m.last_foreground.set(None);
    });
    let v = engine.untrack(|| material_foreground_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_material(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn material_background_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    let theme = effective_material_theme(engine, att);
    Value::Color(inherited_material_color(
        engine,
        att,
        MaterialAttached::BACKGROUND.index,
        theme,
        |m| m.raw_background.get(),
        |t| {
            if t == 1 { rgb(0x1c1b1f) } else { rgb(0xfffbfe) }
        },
        true,
    ))
}

fn reset_material_background(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let _ = engine.with::<MaterialAttached, _>(att, |m| {
        m.raw_background.set(RawColor::Unset);
        m.last_background.set(None);
    });
    let v = engine.untrack(|| material_background_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_material(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn inherited_material_theme(engine: &Engine, att: ObjectId) -> i32 {
    let meta = material_attacher();
    let mut cur = engine.owner(att).and_then(|o| above(engine, o));
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        if let Some(a) = engine.existing_attached_object(p, &meta) {
            let v = engine.read(a, MaterialAttached::THEME.index).to_number() as i32;
            if author_set(engine, a, MaterialAttached::THEME.index) {
                return if v == 1 { 1 } else { 0 };
            }
        }
        cur = above(engine, p);
    }
    0
}

fn effective_material_theme(engine: &Engine, att: ObjectId) -> i32 {
    let own = engine.read(att, MaterialAttached::THEME.index).to_number() as i32;
    if author_set(engine, att, MaterialAttached::THEME.index) {
        if own == 1 { 1 } else { 0 }
    } else {
        inherited_material_theme(engine, att)
    }
}

fn resolve_raw_material_color(raw: RawColor, theme: i32, dark_shade200: bool) -> Option<Color> {
    match raw {
        RawColor::Unset => None,
        RawColor::Enum(c) => {
            let shade = if dark_shade200 && theme == 1 { 2 } else { 5 };
            Some(material_color(c, shade))
        }
        RawColor::Direct(c) => Some(c),
    }
}

fn inherited_material_color(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
    theme: i32,
    get_raw: impl Fn(&MaterialAttached) -> RawColor,
    default_for_theme: impl Fn(i32) -> Color,
    dark_shade200: bool,
) -> Color {
    let meta = material_attacher();
    let mut cur = engine.owner(att).and_then(|o| above(engine, o));
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        if let Some(a) = engine.existing_attached_object(p, &meta) {
            let value = engine.read(a, prop);
            if author_set(engine, a, prop) {
                // An ancestor's effect may not have resolved this assignment yet.
                let raw = match value {
                    Value::Int(n) => RawColor::Enum(n),
                    Value::Real(n) => RawColor::Enum(n as i32),
                    _ => engine
                        .with::<MaterialAttached, _>(a, &get_raw)
                        .unwrap_or(RawColor::Unset),
                };
                if let Some(c) = resolve_raw_material_color(raw, theme, dark_shade200) {
                    return c;
                }
            }
        }
        cur = above(engine, p);
    }
    default_for_theme(theme)
}

fn parse_assigned_material_color(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
    theme: i32,
    dark_shade200: bool,
    get_state: impl Fn(&MaterialAttached) -> (RawColor, Option<Color>),
    set_state: impl Fn(&MaterialAttached, RawColor, Option<Color>),
) -> Option<Color> {
    let v = engine.read(att, prop);
    if !author_set(engine, att, prop) {
        let _ = engine.with::<MaterialAttached, _>(att, |m| {
            set_state(m, RawColor::Unset, None);
        });
        return None;
    }
    let (prev_raw, prev_derived) = engine
        .with::<MaterialAttached, _>(att, &get_state)
        .unwrap_or((RawColor::Unset, None));
    let raw = match &v {
        Value::Int(n) => RawColor::Enum(*n),
        Value::Real(f) => RawColor::Enum(*f as i32),
        Value::Color(c) => {
            if prev_derived == Some(*c) && matches!(prev_raw, RawColor::Enum(_)) {
                prev_raw
            } else {
                RawColor::Direct(*c)
            }
        }
        other => {
            let c = Color::from_value(other).unwrap_or(Color::TRANSPARENT);
            RawColor::Direct(c)
        }
    };
    let resolved = resolve_raw_material_color(raw, theme, dark_shade200)?;
    let _ = engine.with::<MaterialAttached, _>(att, |m| {
        set_state(m, raw, Some(resolved));
    });
    Some(resolved)
}

fn sync_material(engine: &Engine, att: ObjectId) {
    if let Some(node) = style_epoch(engine) {
        engine.graph().get(node);
    }
    let theme = effective_material_theme(engine, att);
    if !author_set(engine, att, MaterialAttached::THEME.index) {
        write_if_changed(
            engine,
            att,
            MaterialAttached::THEME.index,
            Value::Int(theme),
        );
    }

    let primary = parse_assigned_material_color(
        engine,
        att,
        MaterialAttached::PRIMARY.index,
        theme,
        false,
        |m| (m.raw_primary.get(), m.last_primary.get()),
        |m, r, d| {
            m.raw_primary.set(r);
            m.last_primary.set(d);
        },
    )
    .unwrap_or_else(|| {
        inherited_material_color(
            engine,
            att,
            MaterialAttached::PRIMARY.index,
            theme,
            |m| m.raw_primary.get(),
            |_| rgb(0x3f51b5),
            false,
        )
    });
    write_if_changed(
        engine,
        att,
        MaterialAttached::PRIMARY.index,
        Value::Color(primary),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::PRIMARY_COLOR.index,
        Value::Color(primary),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::TOOL_BAR_COLOR.index,
        Value::Color(primary),
    );

    let accent = parse_assigned_material_color(
        engine,
        att,
        MaterialAttached::ACCENT.index,
        theme,
        true,
        |m| (m.raw_accent.get(), m.last_accent.get()),
        |m, r, d| {
            m.raw_accent.set(r);
            m.last_accent.set(d);
        },
    )
    .unwrap_or_else(|| {
        inherited_material_color(
            engine,
            att,
            MaterialAttached::ACCENT.index,
            theme,
            |m| m.raw_accent.get(),
            |t| {
                if t == 1 {
                    material_color(1, 2)
                } else {
                    rgb(0xe91e63)
                }
            },
            true,
        )
    });
    write_if_changed(
        engine,
        att,
        MaterialAttached::ACCENT.index,
        Value::Color(accent),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::ACCENT_COLOR.index,
        Value::Color(accent),
    );

    let fg = parse_assigned_material_color(
        engine,
        att,
        MaterialAttached::FOREGROUND.index,
        theme,
        false,
        |m| (m.raw_foreground.get(), m.last_foreground.get()),
        |m, r, d| {
            m.raw_foreground.set(r);
            m.last_foreground.set(d);
        },
    )
    .unwrap_or_else(|| {
        inherited_material_color(
            engine,
            att,
            MaterialAttached::FOREGROUND.index,
            theme,
            |m| m.raw_foreground.get(),
            |t| {
                if t == 1 {
                    rgb(0xffffff)
                } else {
                    argb(0xdd000000)
                }
            },
            false,
        )
    });
    write_if_changed(
        engine,
        att,
        MaterialAttached::FOREGROUND.index,
        Value::Color(fg),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::PRIMARY_TEXT_COLOR.index,
        Value::Color(fg),
    );

    let bg = parse_assigned_material_color(
        engine,
        att,
        MaterialAttached::BACKGROUND.index,
        theme,
        true,
        |m| (m.raw_background.get(), m.last_background.get()),
        |m, r, d| {
            m.raw_background.set(r);
            m.last_background.set(d);
        },
    )
    .unwrap_or_else(|| {
        inherited_material_color(
            engine,
            att,
            MaterialAttached::BACKGROUND.index,
            theme,
            |m| m.raw_background.get(),
            |t| {
                if t == 1 { rgb(0x1c1b1f) } else { rgb(0xfffbfe) }
            },
            true,
        )
    });
    write_if_changed(
        engine,
        att,
        MaterialAttached::BACKGROUND.index,
        Value::Color(bg),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::BACKGROUND_COLOR.index,
        Value::Color(bg),
    );

    let (sec_text, hint_text, divider, dialog) = if theme == 1 {
        (
            argb(0xb2ffffff),
            argb(0x4cffffff),
            argb(0x1effffff),
            rgb(0x424242),
        )
    } else {
        (
            argb(0x89000000),
            argb(0x60000000),
            argb(0x1e000000),
            rgb(0xffffff),
        )
    };
    write_if_changed(
        engine,
        att,
        MaterialAttached::SECONDARY_TEXT_COLOR.index,
        Value::Color(sec_text),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::HINT_TEXT_COLOR.index,
        Value::Color(hint_text),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::DIVIDER_COLOR.index,
        Value::Color(divider),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::FRAME_COLOR.index,
        Value::Color(hint_text),
    );
    write_if_changed(
        engine,
        att,
        MaterialAttached::DIALOG_COLOR.index,
        Value::Color(dialog),
    );
}

// ============================================================================
// Fusion style (`QtQuick.Controls.Fusion`)
// ============================================================================

/// `Fusion` (`QtQuick.Controls.Fusion`): singleton style helper.
#[derive(Debug, QmlObject)]
#[qml_singleton]
#[qml(module = "QtQuick.Controls.Fusion", name = "Fusion")]
pub struct Fusion {
    #[qml(readonly, constant)]
    pub high_contrast: bool,
    #[qml(readonly, constant)]
    pub light_shade: Color,
    #[qml(readonly, constant)]
    pub dark_shade: Color,
    #[qml(readonly, constant)]
    pub top_shadow: Color,
    #[qml(readonly, constant)]
    pub inner_contrast_line: Color,
}

impl Default for Fusion {
    fn default() -> Self {
        Self {
            high_contrast: false,
            light_shade: argb(0x5affffff),
            dark_shade: argb(0x3c000000),
            top_shadow: argb(0x12000000),
            inner_contrast_line: argb(0x1effffff),
        }
    }
}

// ============================================================================
// Universal style (`QtQuick.Controls.Universal`)
// ============================================================================

fn universal_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        ("Theme", vec![("Light", 0), ("Dark", 1), ("System", 2)]),
        (
            "Color",
            vec![
                ("Lime", 0),
                ("Green", 1),
                ("Emerald", 2),
                ("Teal", 3),
                ("Cyan", 4),
                ("Cobalt", 5),
                ("Indigo", 6),
                ("Violet", 7),
                ("Pink", 8),
                ("Magenta", 9),
                ("Crimson", 10),
                ("Red", 11),
                ("Orange", 12),
                ("Amber", 13),
                ("Yellow", 14),
                ("Brown", 15),
                ("Olive", 16),
                ("Steel", 17),
                ("Mauve", 18),
                ("Taupe", 19),
            ],
        ),
    ]
}

const UNIVERSAL_COLORS: [u32; 20] = [
    0xa4c400, // 0: Lime
    0x60a917, // 1: Green
    0x008a00, // 2: Emerald
    0x00aba9, // 3: Teal
    0x1ba1e2, // 4: Cyan
    0x3e65ff, // 5: Cobalt
    0x6a00ff, // 6: Indigo
    0xaa00ff, // 7: Violet
    0xf472d0, // 8: Pink
    0xd80073, // 9: Magenta
    0xa20025, // 10: Crimson
    0xe51400, // 11: Red
    0xfa6800, // 12: Orange
    0xf0a30a, // 13: Amber
    0xe3c800, // 14: Yellow
    0x825a2c, // 15: Brown
    0x6d8764, // 16: Olive
    0x647687, // 17: Steel
    0x76608a, // 18: Mauve
    0x87794e, // 19: Taupe
];

fn universal_color(color: i32) -> Color {
    let Ok(idx) = usize::try_from(color) else {
        return Color::TRANSPARENT;
    };
    match UNIVERSAL_COLORS.get(idx) {
        Some(&hex) => rgb(hex),
        None => Color::TRANSPARENT,
    }
}

/// `Universal` (`QtQuick.Controls.Universal`): attaches [`UniversalAttached`]
/// and exposes the Universal `Theme` and `Color` enums.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Controls.Universal",
    name = "Universal",
    attached = UniversalAttached,
    enums = universal_enums,
    uncreatable
)]
pub struct Universal {}

/// What `Universal.` attaches to an object.
#[derive(Debug, QmlObject)]
#[qml(name = "UniversalAttached", enums = universal_enums)]
pub struct UniversalAttached {
    #[qml(notify, enumeration = "Theme", reset = UNIVERSAL_THEME_RESET)]
    pub theme: i32,
    #[qml(notify, reset = UNIVERSAL_ACCENT_RESET)]
    pub accent: Value,
    #[qml(notify, reset = UNIVERSAL_FOREGROUND_RESET)]
    pub foreground: Value,
    #[qml(notify, reset = UNIVERSAL_BACKGROUND_RESET)]
    pub background: Value,

    #[qml(readonly, notify)]
    pub alt_high_color: Color,
    #[qml(readonly, notify)]
    pub alt_low_color: Color,
    #[qml(readonly, notify)]
    pub alt_medium_color: Color,
    #[qml(readonly, notify)]
    pub alt_medium_high_color: Color,
    #[qml(readonly, notify)]
    pub alt_medium_low_color: Color,
    #[qml(readonly, notify)]
    pub base_high_color: Color,
    #[qml(readonly, notify)]
    pub base_low_color: Color,
    #[qml(readonly, notify)]
    pub base_medium_color: Color,
    #[qml(readonly, notify)]
    pub base_medium_high_color: Color,
    #[qml(readonly, notify)]
    pub base_medium_low_color: Color,
    #[qml(readonly, notify)]
    pub chrome_alt_low_color: Color,
    #[qml(readonly, notify)]
    pub chrome_black_high_color: Color,
    #[qml(readonly, notify)]
    pub chrome_black_low_color: Color,
    #[qml(readonly, notify)]
    pub chrome_black_medium_low_color: Color,
    #[qml(readonly, notify)]
    pub chrome_black_medium_color: Color,
    #[qml(readonly, notify)]
    pub chrome_disabled_high_color: Color,
    #[qml(readonly, notify)]
    pub chrome_disabled_low_color: Color,
    #[qml(readonly, notify)]
    pub chrome_high_color: Color,
    #[qml(readonly, notify)]
    pub chrome_low_color: Color,
    #[qml(readonly, notify)]
    pub chrome_medium_color: Color,
    #[qml(readonly, notify)]
    pub chrome_medium_low_color: Color,
    #[qml(readonly, notify)]
    pub chrome_white_color: Color,
    #[qml(readonly, notify)]
    pub list_low_color: Color,
    #[qml(readonly, notify)]
    pub list_medium_color: Color,

    #[qml(skip)]
    raw_accent: Cell<RawColor>,
    #[qml(skip)]
    last_accent: Cell<Option<Color>>,
    #[qml(skip)]
    raw_foreground: Cell<RawColor>,
    #[qml(skip)]
    last_foreground: Cell<Option<Color>>,
    #[qml(skip)]
    raw_background: Cell<RawColor>,
    #[qml(skip)]
    last_background: Cell<Option<Color>>,
}

impl Default for UniversalAttached {
    fn default() -> Self {
        let accent = rgb(0x3e65ff);
        let fg = rgb(0x000000);
        let bg = rgb(0xffffff);
        Self {
            theme: 0,
            accent: Value::Color(accent),
            foreground: Value::Color(fg),
            background: Value::Color(bg),
            alt_high_color: rgb(0xffffff),
            alt_low_color: argb(0x33ffffff),
            alt_medium_color: argb(0x99ffffff),
            alt_medium_high_color: argb(0xccffffff),
            alt_medium_low_color: argb(0x66ffffff),
            base_high_color: rgb(0x000000),
            base_low_color: argb(0x33000000),
            base_medium_color: argb(0x99000000),
            base_medium_high_color: argb(0xcc000000),
            base_medium_low_color: argb(0x66000000),
            chrome_alt_low_color: rgb(0x171717),
            chrome_black_high_color: rgb(0x000000),
            chrome_black_low_color: argb(0x33000000),
            chrome_black_medium_low_color: argb(0x66000000),
            chrome_black_medium_color: argb(0xcc000000),
            chrome_disabled_high_color: rgb(0xcccccc),
            chrome_disabled_low_color: rgb(0x7a7a7a),
            chrome_high_color: rgb(0xcccccc),
            chrome_low_color: rgb(0xf2f2f2),
            chrome_medium_color: rgb(0xe6e6e6),
            chrome_medium_low_color: rgb(0xf2f2f2),
            chrome_white_color: rgb(0xffffff),
            list_low_color: argb(0x19000000),
            list_medium_color: argb(0x33000000),
            raw_accent: Cell::new(RawColor::Unset),
            last_accent: Cell::new(None),
            raw_foreground: Cell::new(RawColor::Unset),
            last_foreground: Cell::new(None),
            raw_background: Cell::new(RawColor::Unset),
            last_background: Cell::new(None),
        }
    }
}

#[ljos_engine::qml_impl]
impl UniversalAttached {
    #[qml(invokable)]
    pub fn color(&self, color: i32) -> Color {
        universal_color(color)
    }
}

fn universal_attacher() -> MetaRef {
    MetaRef::Static(Universal::static_meta())
}

const UNIVERSAL_THEME_RESET: Reset = Reset::Custom {
    apply: reset_universal_theme,
    value: universal_theme_value,
};
const UNIVERSAL_ACCENT_RESET: Reset = Reset::Custom {
    apply: reset_universal_accent,
    value: universal_accent_value,
};
const UNIVERSAL_FOREGROUND_RESET: Reset = Reset::Custom {
    apply: reset_universal_foreground,
    value: universal_foreground_value,
};
const UNIVERSAL_BACKGROUND_RESET: Reset = Reset::Custom {
    apply: reset_universal_background,
    value: universal_background_value,
};

fn universal_theme_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    Value::Int(inherited_universal_theme(engine, att))
}

fn reset_universal_theme(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let v = engine.untrack(|| universal_theme_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_universal(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn universal_accent_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    Value::Color(inherited_universal_color(
        engine,
        att,
        UniversalAttached::ACCENT.index,
        |u| u.raw_accent.get(),
        rgb(0x3e65ff),
    ))
}

fn reset_universal_accent(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let _ = engine.with::<UniversalAttached, _>(att, |u| {
        u.raw_accent.set(RawColor::Unset);
        u.last_accent.set(None);
    });
    let v = engine.untrack(|| universal_accent_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_universal(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn universal_foreground_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    let theme = effective_universal_theme(engine, att);
    let default = if theme == 1 {
        rgb(0xffffff)
    } else {
        rgb(0x000000)
    };
    Value::Color(inherited_universal_color(
        engine,
        att,
        UniversalAttached::FOREGROUND.index,
        |u| u.raw_foreground.get(),
        default,
    ))
}

fn reset_universal_foreground(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let _ = engine.with::<UniversalAttached, _>(att, |u| {
        u.raw_foreground.set(RawColor::Unset);
        u.last_foreground.set(None);
    });
    let v = engine.untrack(|| universal_foreground_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_universal(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn universal_background_value(engine: &Engine, att: ObjectId, _prop: PropIndex) -> Value {
    let theme = effective_universal_theme(engine, att);
    let default = if theme == 1 {
        rgb(0x000000)
    } else {
        rgb(0xffffff)
    };
    Value::Color(inherited_universal_color(
        engine,
        att,
        UniversalAttached::BACKGROUND.index,
        |u| u.raw_background.get(),
        default,
    ))
}

fn reset_universal_background(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
) -> Result<(), EngineError> {
    let _ = engine.with::<UniversalAttached, _>(att, |u| {
        u.raw_background.set(RawColor::Unset);
        u.last_background.set(None);
    });
    let v = engine.untrack(|| universal_background_value(engine, att, prop));
    engine.reset_to(att, prop, v)?;
    sync_universal(engine, att);
    bump_style_epoch(engine);
    Ok(())
}

fn inherited_universal_theme(engine: &Engine, att: ObjectId) -> i32 {
    let meta = universal_attacher();
    let mut cur = engine.owner(att).and_then(|o| above(engine, o));
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        if let Some(a) = engine.existing_attached_object(p, &meta) {
            let v = engine.read(a, UniversalAttached::THEME.index).to_number() as i32;
            if author_set(engine, a, UniversalAttached::THEME.index) {
                return if v == 1 { 1 } else { 0 };
            }
        }
        cur = above(engine, p);
    }
    0
}

fn effective_universal_theme(engine: &Engine, att: ObjectId) -> i32 {
    let own = engine.read(att, UniversalAttached::THEME.index).to_number() as i32;
    if author_set(engine, att, UniversalAttached::THEME.index) {
        if own == 1 { 1 } else { 0 }
    } else {
        inherited_universal_theme(engine, att)
    }
}

fn resolve_raw_universal_color(raw: RawColor) -> Option<Color> {
    match raw {
        RawColor::Unset => None,
        RawColor::Enum(c) => Some(universal_color(c)),
        RawColor::Direct(c) => Some(c),
    }
}

fn inherited_universal_color(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
    get_raw: impl Fn(&UniversalAttached) -> RawColor,
    default: Color,
) -> Color {
    let meta = universal_attacher();
    let mut cur = engine.owner(att).and_then(|o| above(engine, o));
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| engine.is_alive(*p)) else {
            break;
        };
        if let Some(a) = engine.existing_attached_object(p, &meta) {
            let value = engine.read(a, prop);
            if author_set(engine, a, prop) {
                let raw = match value {
                    Value::Int(n) => RawColor::Enum(n),
                    Value::Real(n) => RawColor::Enum(n as i32),
                    _ => engine
                        .with::<UniversalAttached, _>(a, &get_raw)
                        .unwrap_or(RawColor::Unset),
                };
                if let Some(c) = resolve_raw_universal_color(raw) {
                    return c;
                }
            }
        }
        cur = above(engine, p);
    }
    default
}

fn parse_assigned_universal_color(
    engine: &Engine,
    att: ObjectId,
    prop: PropIndex,
    get_state: impl Fn(&UniversalAttached) -> (RawColor, Option<Color>),
    set_state: impl Fn(&UniversalAttached, RawColor, Option<Color>),
) -> Option<Color> {
    let v = engine.read(att, prop);
    if !author_set(engine, att, prop) {
        let _ = engine.with::<UniversalAttached, _>(att, |u| {
            set_state(u, RawColor::Unset, None);
        });
        return None;
    }
    let (prev_raw, prev_derived) = engine
        .with::<UniversalAttached, _>(att, &get_state)
        .unwrap_or((RawColor::Unset, None));
    let raw = match &v {
        Value::Int(n) => RawColor::Enum(*n),
        Value::Real(f) => RawColor::Enum(*f as i32),
        Value::Color(c) => {
            if prev_derived == Some(*c) && matches!(prev_raw, RawColor::Enum(_)) {
                prev_raw
            } else {
                RawColor::Direct(*c)
            }
        }
        other => {
            let c = Color::from_value(other).unwrap_or(Color::TRANSPARENT);
            RawColor::Direct(c)
        }
    };
    let resolved = resolve_raw_universal_color(raw)?;
    let _ = engine.with::<UniversalAttached, _>(att, |u| {
        set_state(u, raw, Some(resolved));
    });
    Some(resolved)
}

fn sync_universal(engine: &Engine, att: ObjectId) {
    if let Some(node) = style_epoch(engine) {
        engine.graph().get(node);
    }
    let theme = effective_universal_theme(engine, att);
    if !author_set(engine, att, UniversalAttached::THEME.index) {
        write_if_changed(
            engine,
            att,
            UniversalAttached::THEME.index,
            Value::Int(theme),
        );
    }

    let accent = parse_assigned_universal_color(
        engine,
        att,
        UniversalAttached::ACCENT.index,
        |u| (u.raw_accent.get(), u.last_accent.get()),
        |u, r, d| {
            u.raw_accent.set(r);
            u.last_accent.set(d);
        },
    )
    .unwrap_or_else(|| {
        inherited_universal_color(
            engine,
            att,
            UniversalAttached::ACCENT.index,
            |u| u.raw_accent.get(),
            rgb(0x3e65ff),
        )
    });
    write_if_changed(
        engine,
        att,
        UniversalAttached::ACCENT.index,
        Value::Color(accent),
    );

    let default_fg = if theme == 1 {
        rgb(0xffffff)
    } else {
        rgb(0x000000)
    };
    let fg = parse_assigned_universal_color(
        engine,
        att,
        UniversalAttached::FOREGROUND.index,
        |u| (u.raw_foreground.get(), u.last_foreground.get()),
        |u, r, d| {
            u.raw_foreground.set(r);
            u.last_foreground.set(d);
        },
    )
    .unwrap_or_else(|| {
        inherited_universal_color(
            engine,
            att,
            UniversalAttached::FOREGROUND.index,
            |u| u.raw_foreground.get(),
            default_fg,
        )
    });
    write_if_changed(
        engine,
        att,
        UniversalAttached::FOREGROUND.index,
        Value::Color(fg),
    );

    let default_bg = if theme == 1 {
        rgb(0x000000)
    } else {
        rgb(0xffffff)
    };
    let bg = parse_assigned_universal_color(
        engine,
        att,
        UniversalAttached::BACKGROUND.index,
        |u| (u.raw_background.get(), u.last_background.get()),
        |u, r, d| {
            u.raw_background.set(r);
            u.last_background.set(d);
        },
    )
    .unwrap_or_else(|| {
        inherited_universal_color(
            engine,
            att,
            UniversalAttached::BACKGROUND.index,
            |u| u.raw_background.get(),
            default_bg,
        )
    });
    write_if_changed(
        engine,
        att,
        UniversalAttached::BACKGROUND.index,
        Value::Color(bg),
    );

    for (prop, light, dark) in UNIVERSAL_THEME_COLORS {
        let hex = if theme == 1 { dark } else { light };
        write_if_changed(engine, att, prop, Value::Color(argb(hex)));
    }
}

/// The theme colours of `Universal` as Qt 6.11 reports them (`#aarrggbb`),
/// light then dark.
const UNIVERSAL_THEME_COLORS: [(PropIndex, u32, u32); 24] = [
    (
        UniversalAttached::ALT_HIGH_COLOR.index,
        0xffffffff,
        0xff000000,
    ),
    (
        UniversalAttached::ALT_LOW_COLOR.index,
        0x33ffffff,
        0x33000000,
    ),
    (
        UniversalAttached::ALT_MEDIUM_COLOR.index,
        0x99ffffff,
        0x99000000,
    ),
    (
        UniversalAttached::ALT_MEDIUM_HIGH_COLOR.index,
        0xccffffff,
        0xcc000000,
    ),
    (
        UniversalAttached::ALT_MEDIUM_LOW_COLOR.index,
        0x66ffffff,
        0x66000000,
    ),
    (
        UniversalAttached::BASE_HIGH_COLOR.index,
        0xff000000,
        0xffffffff,
    ),
    (
        UniversalAttached::BASE_LOW_COLOR.index,
        0x33000000,
        0x33ffffff,
    ),
    (
        UniversalAttached::BASE_MEDIUM_COLOR.index,
        0x99000000,
        0x99ffffff,
    ),
    (
        UniversalAttached::BASE_MEDIUM_HIGH_COLOR.index,
        0xcc000000,
        0xccffffff,
    ),
    (
        UniversalAttached::BASE_MEDIUM_LOW_COLOR.index,
        0x66000000,
        0x66ffffff,
    ),
    (
        UniversalAttached::CHROME_ALT_LOW_COLOR.index,
        0xff171717,
        0xfff2f2f2,
    ),
    (
        UniversalAttached::CHROME_BLACK_HIGH_COLOR.index,
        0xff000000,
        0xff000000,
    ),
    (
        UniversalAttached::CHROME_BLACK_LOW_COLOR.index,
        0x33000000,
        0x33000000,
    ),
    (
        UniversalAttached::CHROME_BLACK_MEDIUM_LOW_COLOR.index,
        0x66000000,
        0x66000000,
    ),
    (
        UniversalAttached::CHROME_BLACK_MEDIUM_COLOR.index,
        0xcc000000,
        0xcc000000,
    ),
    (
        UniversalAttached::CHROME_DISABLED_HIGH_COLOR.index,
        0xffcccccc,
        0xff333333,
    ),
    (
        UniversalAttached::CHROME_DISABLED_LOW_COLOR.index,
        0xff7a7a7a,
        0xff858585,
    ),
    (
        UniversalAttached::CHROME_HIGH_COLOR.index,
        0xffcccccc,
        0xff767676,
    ),
    (
        UniversalAttached::CHROME_LOW_COLOR.index,
        0xfff2f2f2,
        0xff171717,
    ),
    (
        UniversalAttached::CHROME_MEDIUM_COLOR.index,
        0xffe6e6e6,
        0xff1f1f1f,
    ),
    (
        UniversalAttached::CHROME_MEDIUM_LOW_COLOR.index,
        0xfff2f2f2,
        0xff2b2b2b,
    ),
    (
        UniversalAttached::CHROME_WHITE_COLOR.index,
        0xffffffff,
        0xffffffff,
    ),
    (
        UniversalAttached::LIST_LOW_COLOR.index,
        0x19000000,
        0x19ffffff,
    ),
    (
        UniversalAttached::LIST_MEDIUM_COLOR.index,
        0x33000000,
        0x33ffffff,
    ),
];

// ============================================================================
// Registration
// ============================================================================

pub(crate) fn register(engine: &Engine) {
    engine.register_type(
        "QtQuick.Controls.Material",
        Version::new(2, 15),
        MetaRef::Static(Material::static_meta()),
    );
    engine.register_singleton::<Fusion>("QtQuick.Controls.Fusion", Version::new(2, 15));
    engine.register_type(
        "QtQuick.Controls.Universal",
        Version::new(2, 15),
        MetaRef::Static(Universal::static_meta()),
    );

    engine.on_attached_created(|e, _owner, att| {
        if is_a::<MaterialAttached>(e, att) {
            bump_style_epoch(e);
            for prop in [
                MaterialAttached::THEME.index,
                MaterialAttached::PRIMARY.index,
                MaterialAttached::ACCENT.index,
                MaterialAttached::FOREGROUND.index,
                MaterialAttached::BACKGROUND.index,
            ] {
                let _ = e.watch(att, prop, move |e, _| {
                    if matches!(
                        e.provenance(att, prop),
                        Some(Provenance::Assigned | Provenance::Bound)
                    ) {
                        bump_style_epoch(e);
                    }
                });
            }
            let _ = e.effect_in(att, move |e| {
                sync_material(e, att);
            });
        } else if is_a::<UniversalAttached>(e, att) {
            bump_style_epoch(e);
            for prop in [
                UniversalAttached::THEME.index,
                UniversalAttached::ACCENT.index,
                UniversalAttached::FOREGROUND.index,
                UniversalAttached::BACKGROUND.index,
            ] {
                let _ = e.watch(att, prop, move |e, _| {
                    if matches!(
                        e.provenance(att, prop),
                        Some(Provenance::Assigned | Provenance::Bound)
                    ) {
                        bump_style_epoch(e);
                    }
                });
            }
            let _ = e.effect_in(att, move |e| {
                sync_universal(e, att);
            });
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_truncated_pack_gives_what_fits() {
        let mut pack = Vec::new();
        pack.extend_from_slice(&2u32.to_le_bytes());
        pack.extend_from_slice(&1u32.to_le_bytes());
        pack.push(b'a');
        pack.extend_from_slice(&2u32.to_le_bytes());
        pack.extend_from_slice(&[7, 8]);
        pack.extend_from_slice(&9u32.to_le_bytes());
        assert_eq!(entries(&pack), vec![("a", &[7u8, 8][..])]);
        assert!(entries(&[]).is_empty());
    }

    #[test]
    fn every_style_only_overrides_basic_controls() {
        let (libraries, bad) = StyleLibrary::embedded();
        assert!(bad.is_empty(), "{bad:?}");
        let basic = libraries[BASIC].type_names();
        assert_eq!(basic.len(), 59);
        for directory in [MATERIAL, FUSION, UNIVERSAL] {
            for name in libraries[directory].type_names() {
                assert!(
                    basic.contains(&name),
                    "{directory}/{name} is not a Basic control"
                );
            }
        }
    }

    #[test]
    fn every_style_control_can_be_created() {
        let (libraries, bad) = StyleLibrary::embedded();
        assert!(bad.is_empty(), "{bad:?}");
        for directory in [MATERIAL, FUSION, UNIVERSAL] {
            for name in libraries[directory].type_names() {
                let engine = Engine::new();
                ljos_quick::register(&engine);
                crate::register(&engine);
                let key = format!("{directory}/{name}");
                let root = engine
                    .create_from_url(&key, None, Vec::new())
                    .unwrap_or_else(|error| panic!("{key}: {error}"));
                assert!(engine.is_alive(root), "{key}");
                if ["Button", "CheckBox", "Switch", "Slider", "TextField"].contains(&name.as_str())
                {
                    let mut scene =
                        ljos_quick::QuickScene::new(ljos_render::kurbo::Size::new(400.0, 300.0));
                    scene.set_root(&engine, root).unwrap();
                    for _ in 0..16 {
                        let frame = scene.frame(&engine, ljos_runtime::Time::ZERO);
                        assert_eq!(frame.next_wake, None, "{key}");
                        if !scene.needs_frame() {
                            break;
                        }
                    }
                    assert!(!scene.needs_frame(), "{key} did not settle");
                    assert_eq!(engine.next_timer_deadline(), None, "{key}");
                }
            }
        }
    }

    #[test]
    fn style_urls_are_told_from_application_urls() {
        let (libraries, bad) = StyleLibrary::embedded();
        assert!(bad.is_empty(), "{bad:?}");
        let lib = &libraries[BASIC];
        let url = lib.url("QtQuick/Controls/Basic/Button");
        assert!(is_style_url(&format!("{url}:3:5: ")));
        assert!(!is_style_url("file:///Main.qml:3:5: "));
        assert_eq!(lib.key(&url), "QtQuick/Controls/Basic/Button");
    }
}
