//! `ExtendedSceneEnvironment`, a SceneEnvironment with the common
//! post-processing built in.

use ljos::prelude::*;

use crate::SceneEnvironment;

use ljos::item::ljos_3d::qml::defaults::extended_scene_environment as x;
pub(crate) use ljos::item::ljos_3d::qml::enums::glow_blend;

fn extended_environment_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "GlowBlendMode",
            vec![
                ("Additive", i64::from(glow_blend::ADDITIVE)),
                ("Screen", i64::from(glow_blend::SCREEN)),
                ("SoftLight", i64::from(glow_blend::SOFT_LIGHT)),
                ("Replace", i64::from(glow_blend::REPLACE)),
            ],
        ),
        (
            "GlowLevel",
            ["One", "Two", "Three", "Four", "Five", "Six", "Seven"]
                .into_iter()
                .zip((0..7).map(|i| 1i64 << i))
                .collect(),
        ),
    ]
}

/// `ExtendedSceneEnvironment`: exposure and white point for the
/// tonemapper, glow and bloom, depth of field, colour adjustments, a
/// vignette, a look-up table, sharpening, FXAA and dithering, each on by
/// its own property. The scene is drawn in high dynamic range and
/// finished by passes over it whatever is on.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "ExtendedSceneEnvironment",
    extends = SceneEnvironment,
    enums = extended_environment_enums
)]
pub struct ExtendedSceneEnvironment {
    #[qml(base)]
    pub environment: SceneEnvironment,
    #[qml(notify)]
    pub exposure: f64,
    #[qml(notify)]
    pub white_point: f64,
    #[qml(notify)]
    pub sharpness_amount: f64,
    #[qml(notify)]
    pub fxaa_enabled: bool,
    #[qml(notify)]
    pub dithering_enabled: bool,
    #[qml(notify)]
    pub color_adjustments_enabled: bool,
    #[qml(notify)]
    pub adjustment_brightness: f64,
    #[qml(notify)]
    pub adjustment_contrast: f64,
    #[qml(notify)]
    pub adjustment_saturation: f64,
    #[qml(notify)]
    pub depth_of_field_enabled: bool,
    #[qml(notify)]
    pub depth_of_field_focus_distance: f64,
    #[qml(notify)]
    pub depth_of_field_focus_range: f64,
    /// The blur's radius in pixels, at the most out of focus.
    #[qml(notify)]
    pub depth_of_field_blur_amount: f64,
    #[qml(notify)]
    pub glow_enabled: bool,
    #[qml(notify)]
    pub glow_quality_high: bool,
    /// Kept: each level is scaled up with a linear filter.
    #[qml(notify)]
    pub glow_use_bicubic_upscale: bool,
    #[qml(notify)]
    pub glow_strength: f64,
    #[qml(notify)]
    pub glow_intensity: f64,
    #[qml(notify)]
    pub glow_bloom: f64,
    #[qml(notify, rename = "glowHDRMaximumValue")]
    pub glow_hdr_maximum_value: f64,
    #[qml(notify, rename = "glowHDRMinimumValue")]
    pub glow_hdr_minimum_value: f64,
    #[qml(notify, rename = "glowHDRScale")]
    pub glow_hdr_scale: f64,
    /// `GlowLevel` flags.
    #[qml(notify)]
    pub glow_level: i32,
    #[qml(notify)]
    pub glow_blend_mode: i32,
    #[qml(notify)]
    pub vignette_enabled: bool,
    #[qml(notify)]
    pub vignette_strength: f64,
    #[qml(notify)]
    pub vignette_color: Color,
    #[qml(notify)]
    pub vignette_radius: f64,
    #[qml(notify)]
    pub lut_enabled: bool,
    #[qml(notify)]
    pub lut_size: f64,
    #[qml(notify)]
    pub lut_filter_alpha: f64,
    #[qml(notify, object = "Texture")]
    pub lut_texture: Option<ObjectId>,
    /// Kept, as are the other lens flare properties: the flare is not
    /// built.
    #[qml(notify)]
    pub lens_flare_enabled: bool,
    #[qml(notify)]
    pub lens_flare_bloom_scale: f64,
    #[qml(notify)]
    pub lens_flare_bloom_bias: f64,
    #[qml(notify)]
    pub lens_flare_ghost_dispersal: f64,
    #[qml(notify)]
    pub lens_flare_ghost_count: i32,
    #[qml(notify)]
    pub lens_flare_halo_width: f64,
    #[qml(notify)]
    pub lens_flare_stretch_to_aspect: f64,
    #[qml(notify)]
    pub lens_flare_distortion: f64,
    #[qml(notify)]
    pub lens_flare_blur_amount: f64,
    #[qml(notify)]
    pub lens_flare_apply_dirt_texture: bool,
    #[qml(notify)]
    pub lens_flare_apply_starburst_texture: bool,
    #[qml(notify)]
    pub lens_flare_camera_direction: Vector3d,
    #[qml(notify, object = "Texture")]
    pub lens_flare_lens_color_texture: Option<ObjectId>,
    #[qml(notify, object = "Texture")]
    pub lens_flare_lens_dirt_texture: Option<ObjectId>,
    #[qml(notify, object = "Texture")]
    pub lens_flare_lens_starburst_texture: Option<ObjectId>,
}

impl Default for ExtendedSceneEnvironment {
    fn default() -> Self {
        ExtendedSceneEnvironment {
            environment: SceneEnvironment::default(),
            exposure: x::EXPOSURE,
            white_point: x::WHITE_POINT,
            sharpness_amount: x::SHARPNESS_AMOUNT,
            fxaa_enabled: x::FXAA_ENABLED,
            dithering_enabled: x::DITHERING_ENABLED,
            color_adjustments_enabled: x::COLOR_ADJUSTMENTS_ENABLED,
            adjustment_brightness: x::ADJUSTMENT_BRIGHTNESS,
            adjustment_contrast: x::ADJUSTMENT_CONTRAST,
            adjustment_saturation: x::ADJUSTMENT_SATURATION,
            depth_of_field_enabled: x::DEPTH_OF_FIELD_ENABLED,
            depth_of_field_focus_distance: x::DEPTH_OF_FIELD_FOCUS_DISTANCE,
            depth_of_field_focus_range: x::DEPTH_OF_FIELD_FOCUS_RANGE,
            depth_of_field_blur_amount: x::DEPTH_OF_FIELD_BLUR_AMOUNT,
            glow_enabled: x::GLOW_ENABLED,
            glow_quality_high: x::GLOW_QUALITY_HIGH,
            glow_use_bicubic_upscale: false,
            glow_strength: x::GLOW_STRENGTH,
            glow_intensity: x::GLOW_INTENSITY,
            glow_bloom: x::GLOW_BLOOM,
            glow_hdr_maximum_value: x::GLOW_HDR_MAXIMUM_VALUE,
            glow_hdr_minimum_value: x::GLOW_HDR_MINIMUM_VALUE,
            glow_hdr_scale: x::GLOW_HDR_SCALE,
            glow_level: x::GLOW_LEVEL,
            glow_blend_mode: x::GLOW_BLEND_MODE,
            vignette_enabled: x::VIGNETTE_ENABLED,
            vignette_strength: x::VIGNETTE_STRENGTH,
            vignette_color: crate::node::rgba(x::VIGNETTE_COLOR),
            vignette_radius: x::VIGNETTE_RADIUS,
            lut_enabled: x::LUT_ENABLED,
            lut_size: x::LUT_SIZE,
            lut_filter_alpha: x::LUT_FILTER_ALPHA,
            lut_texture: None,
            lens_flare_enabled: false,
            lens_flare_bloom_scale: 10.0,
            lens_flare_bloom_bias: 0.95,
            lens_flare_ghost_dispersal: 0.5,
            lens_flare_ghost_count: 4,
            lens_flare_halo_width: 0.25,
            lens_flare_stretch_to_aspect: 0.5,
            lens_flare_distortion: 5.0,
            lens_flare_blur_amount: 3.0,
            lens_flare_apply_dirt_texture: false,
            lens_flare_apply_starburst_texture: false,
            lens_flare_camera_direction: Vector3d {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
            lens_flare_lens_color_texture: None,
            lens_flare_lens_dirt_texture: None,
            lens_flare_lens_starburst_texture: None,
        }
    }
}
