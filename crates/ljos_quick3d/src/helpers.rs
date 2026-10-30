//! `QtQuick3D.Helpers`: `ExtendedSceneEnvironment`, a SceneEnvironment
//! with the common post-processing built in.

use ljos::prelude::*;

use crate::SceneEnvironment;
use crate::random_instancing::{InstanceRange, RandomInstancing};

pub(crate) mod glow_blend {
    pub const ADDITIVE: i32 = 0;
    pub const SCREEN: i32 = 1;
    pub const SOFT_LIGHT: i32 = 2;
    pub const REPLACE: i32 = 3;
}

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
            exposure: 1.0,
            white_point: 1.0,
            sharpness_amount: 0.0,
            fxaa_enabled: false,
            dithering_enabled: false,
            color_adjustments_enabled: false,
            adjustment_brightness: 1.0,
            adjustment_contrast: 1.0,
            adjustment_saturation: 1.0,
            depth_of_field_enabled: false,
            depth_of_field_focus_distance: 600.0,
            depth_of_field_focus_range: 100.0,
            depth_of_field_blur_amount: 4.0,
            glow_enabled: false,
            glow_quality_high: false,
            glow_use_bicubic_upscale: false,
            glow_strength: 1.0,
            glow_intensity: 1.0,
            glow_bloom: 0.0,
            glow_hdr_maximum_value: 12.0,
            glow_hdr_minimum_value: 1.0,
            glow_hdr_scale: 2.0,
            glow_level: 1,
            glow_blend_mode: glow_blend::SOFT_LIGHT,
            vignette_enabled: false,
            vignette_strength: 15.0,
            vignette_color: Color::from_rgba8(128, 128, 128, 255),
            vignette_radius: 0.35,
            lut_enabled: false,
            lut_size: 16.0,
            lut_filter_alpha: 1.0,
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

/// Registers `QtQuick3D.Helpers`.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos::module!("QtQuick3D.Helpers" 6.0 => [
        ExtendedSceneEnvironment,
        InstanceRange,
        RandomInstancing,
    ]));
}
