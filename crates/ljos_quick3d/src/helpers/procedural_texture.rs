//! `ProceduralTextureData` and `ProceduralSkyTextureData`: texture data
//! written in QML, as bytes or as the colours of a sky.

use ljos::item::ljos_3d::sky::Sky;
use ljos::item::ljos_3d::texture_data::format;
use ljos::item::{self};
use ljos::prelude::*;

use crate::texture_data::{TextureData, commit};

/// `ProceduralTextureData` (QtQuick3D.Helpers): `width` by `height`
/// pixels of `format`, the bytes of the ArrayBuffer `textureData`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "ProceduralTextureData", extends = TextureData)]
pub struct ProceduralTextureData {
    #[qml(base)]
    pub texture_data_base: TextureData,
    #[qml(notify)]
    pub width: i32,
    #[qml(notify)]
    pub height: i32,
    #[qml(notify)]
    pub depth: i32,
    #[qml(notify, enumeration = "TextureData.Format")]
    pub format: i32,
    #[qml(notify)]
    pub has_transparency: bool,
    #[qml(notify, rename = "textureData")]
    pub texture_data: Value,
}

impl Default for ProceduralTextureData {
    fn default() -> Self {
        ProceduralTextureData {
            texture_data_base: TextureData::default(),
            width: 0,
            height: 0,
            depth: 0,
            format: format::RGBA8,
            has_transparency: false,
            texture_data: Value::Undefined,
        }
    }
}

/// Reads the bytes of an ArrayBuffer (or of a typed array's view of one)
/// as a string of characters below 256, the one form of bytes a JS value
/// becomes here; in pieces, so a large buffer is not one call's
/// arguments.
const BYTES: &str = r"(function (b) {
  if (b === null || b === undefined) return '';
  var u = ArrayBuffer.isView(b) ? new Uint8Array(b.buffer, b.byteOffset, b.byteLength)
        : b instanceof ArrayBuffer ? new Uint8Array(b) : null;
  if (u === null) return '';
  var s = '';
  for (var i = 0; i < u.length; i += 8192)
    s += String.fromCharCode.apply(null, u.subarray(i, i + 8192));
  return s;
})";

fn bytes(e: &Engine, v: &Value) -> Vec<u8> {
    if !matches!(v, Value::Js(_)) {
        return Vec::new();
    }
    let Ok(read) = e.eval_js(BYTES) else {
        return Vec::new();
    };
    match e.call_js(&read, std::slice::from_ref(v)) {
        Some(Value::String(s)) => s.chars().map(|c| c as u32 as u8).collect(),
        _ => Vec::new(),
    }
}

#[qml_impl]
impl ProceduralTextureData {
    /// Committed again whenever a property changes.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine.effect_in(me, move |e| {
            let width: i32 = item::prop(e, me, ProceduralTextureData::WIDTH);
            let height: i32 = item::prop(e, me, ProceduralTextureData::HEIGHT);
            let depth: i32 = item::prop(e, me, ProceduralTextureData::DEPTH);
            let format: i32 = item::prop(e, me, ProceduralTextureData::FORMAT);
            let transparent: bool = item::prop(e, me, ProceduralTextureData::HAS_TRANSPARENCY);
            let data = e.read(me, ProceduralTextureData::TEXTURE_DATA.index);
            e.untrack(|| {
                let bytes = bytes(e, &data);
                let _ = e.with_mut(me, |t: &mut TextureData| {
                    t.set_size(width.max(0) as u32, height.max(0) as u32);
                    t.set_depth(depth.max(0) as u32);
                    t.set_format(format);
                    t.set_has_transparency(transparent);
                    t.set_texture_data(bytes);
                });
                TextureData::update(e, me);
            });
        })?;
        Ok(())
    }
}

fn sky_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "SkyTextureQuality",
        vec![
            ("SkyTextureQualityLow", 512),
            ("SkyTextureQualityMedium", 1024),
            ("SkyTextureQualityHigh", 2048),
            ("SkyTextureQualityVeryHigh", 4096),
        ],
    )]
}

/// `ProceduralSkyTextureData` (QtQuick3D.Helpers): a light probe painted
/// from the colours of a sky, its ground and its sun.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "ProceduralSkyTextureData",
    extends = TextureData,
    enums = sky_enums
)]
pub struct ProceduralSkyTextureData {
    #[qml(base)]
    pub texture_data_base: TextureData,
    #[qml(notify)]
    pub sky_top_color: Color,
    #[qml(notify)]
    pub sky_horizon_color: Color,
    #[qml(notify)]
    pub sky_curve: f64,
    #[qml(notify)]
    pub sky_energy: f64,
    #[qml(notify)]
    pub ground_bottom_color: Color,
    #[qml(notify)]
    pub ground_horizon_color: Color,
    #[qml(notify)]
    pub ground_curve: f64,
    #[qml(notify)]
    pub ground_energy: f64,
    #[qml(notify)]
    pub sun_color: Color,
    #[qml(notify)]
    pub sun_latitude: f64,
    #[qml(notify)]
    pub sun_longitude: f64,
    #[qml(notify)]
    pub sun_angle_min: f64,
    #[qml(notify)]
    pub sun_angle_max: f64,
    #[qml(notify)]
    pub sun_curve: f64,
    #[qml(notify)]
    pub sun_energy: f64,
    /// The width of the image painted, which is half as tall.
    #[qml(notify, enumeration = "SkyTextureQuality")]
    pub texture_quality: i32,
}

fn color([r, g, b]: [f32; 3]) -> Color {
    let byte = |c: f32| (c * 255.0).round() as u8;
    Color::from_rgba8(byte(r), byte(g), byte(b), 255)
}

fn rgb(c: Color) -> [f32; 3] {
    let [r, g, b, _] = c.to_rgba_f();
    [r as f32, g as f32, b as f32]
}

impl Default for ProceduralSkyTextureData {
    fn default() -> Self {
        let s = Sky::default();
        ProceduralSkyTextureData {
            texture_data_base: TextureData::default(),
            sky_top_color: color(s.sky_top),
            sky_horizon_color: color(s.sky_horizon),
            sky_curve: f64::from(s.sky_curve),
            sky_energy: f64::from(s.sky_energy),
            ground_bottom_color: color(s.ground_bottom),
            ground_horizon_color: color(s.ground_horizon),
            ground_curve: f64::from(s.ground_curve),
            ground_energy: f64::from(s.ground_energy),
            sun_color: color(s.sun),
            sun_latitude: f64::from(s.sun_latitude),
            sun_longitude: f64::from(s.sun_longitude),
            sun_angle_min: f64::from(s.sun_angle_min),
            sun_angle_max: f64::from(s.sun_angle_max),
            sun_curve: f64::from(s.sun_curve),
            sun_energy: f64::from(s.sun_energy),
            texture_quality: s.width as i32,
        }
    }
}

#[qml_impl]
impl ProceduralSkyTextureData {
    /// Painted again whenever a property changes.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine.effect_in(me, move |e| {
            type P = ProceduralSkyTextureData;
            let n = |p: Prop<f64>| item::prop(e, me, p) as f32;
            let c = |p: Prop<Color>| rgb(item::prop(e, me, p));
            let quality: i32 = item::prop(e, me, P::TEXTURE_QUALITY);
            let sky = Sky {
                sky_top: c(P::SKY_TOP_COLOR),
                sky_horizon: c(P::SKY_HORIZON_COLOR),
                sky_curve: n(P::SKY_CURVE),
                sky_energy: n(P::SKY_ENERGY),
                ground_bottom: c(P::GROUND_BOTTOM_COLOR),
                ground_horizon: c(P::GROUND_HORIZON_COLOR),
                ground_curve: n(P::GROUND_CURVE),
                ground_energy: n(P::GROUND_ENERGY),
                sun: c(P::SUN_COLOR),
                sun_latitude: n(P::SUN_LATITUDE),
                sun_longitude: n(P::SUN_LONGITUDE),
                sun_angle_min: n(P::SUN_ANGLE_MIN),
                sun_angle_max: n(P::SUN_ANGLE_MAX),
                sun_curve: n(P::SUN_CURVE),
                sun_energy: n(P::SUN_ENERGY),
                width: quality.clamp(2, 4096) as u32,
            };
            e.untrack(|| commit(e, me, Some(sky.image())));
        })?;
        Ok(())
    }
}
