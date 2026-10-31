//! Qt's documented default of each member the mapping reads: what a
//! member left out says.
//!
//! The QML types' `Default` impls and [`super::build`] both read these,
//! so a default is written once. Colours are `[r, g, b, a]` bytes, sRGB.

use super::enums;

pub mod node {
    pub const POSITION: [f32; 3] = [0.0, 0.0, 0.0];
    pub const SCALE: [f32; 3] = [1.0, 1.0, 1.0];
    pub const OPACITY: f64 = 1.0;
    pub const VISIBLE: bool = true;
}

pub mod model {
    pub const CASTS_SHADOWS: bool = true;
    pub const RECEIVES_SHADOWS: bool = true;
}

pub mod camera {
    pub const FRUSTUM_CULLING_ENABLED: bool = false;
}

pub mod perspective_camera {
    use super::enums::fov_orientation;
    pub const FIELD_OF_VIEW: f64 = 60.0;
    pub const FIELD_OF_VIEW_ORIENTATION: i32 = fov_orientation::VERTICAL;
    pub const CLIP_NEAR: f64 = 10.0;
    pub const CLIP_FAR: f64 = 10000.0;
}

pub mod orthographic_camera {
    pub const CLIP_NEAR: f64 = 10.0;
    pub const CLIP_FAR: f64 = 10000.0;
    pub const HORIZONTAL_MAGNIFICATION: f64 = 1.0;
    pub const VERTICAL_MAGNIFICATION: f64 = 1.0;
}

pub mod light {
    use super::enums::{shadow_map, soft_shadow};
    pub const COLOR: [u8; 4] = [255, 255, 255, 255];
    pub const AMBIENT_COLOR: [u8; 4] = [0, 0, 0, 255];
    pub const BRIGHTNESS: f64 = 1.0;
    pub const CASTS_SHADOW: bool = false;
    pub const SHADOW_FACTOR: f64 = 75.0;
    pub const SHADOW_BIAS: f64 = 10.0;
    pub const SHADOW_MAP_QUALITY: i32 = shadow_map::LOW;
    pub const SHADOW_MAP_FAR: f64 = 5000.0;
    pub const SOFT_SHADOW_QUALITY: i32 = soft_shadow::PCF4;
    pub const PCF_FACTOR: f64 = 2.0;
    pub const SHADOW_FILTER: f64 = 5.0;
}

pub mod directional_light {
    pub const CSM_NUM_SPLITS: i32 = 0;
    pub const CSM_SPLIT1: f64 = 0.1;
    pub const CSM_SPLIT2: f64 = 0.25;
    pub const CSM_SPLIT3: f64 = 0.5;
    pub const CSM_BLEND_RATIO: f64 = 0.05;
    pub const LOCK_SHADOWMAP_TEXELS: bool = false;
}

/// `PointLight`'s and `SpotLight`'s.
pub mod fade {
    pub const CONSTANT: f64 = 1.0;
    pub const LINEAR: f64 = 0.0;
    pub const QUADRATIC: f64 = 1.0;
}

pub mod spot_light {
    pub const CONE_ANGLE: f64 = 40.0;
    pub const INNER_CONE_ANGLE: f64 = 30.0;
}

pub mod material {
    use super::enums::cull;
    pub const CULL_MODE: i32 = cull::BACK;
}

pub mod default_material {
    use super::enums::lighting;
    pub const DIFFUSE_COLOR: [u8; 4] = [255, 255, 255, 255];
    pub const LIGHTING: i32 = lighting::FRAGMENT_LIGHTING;
    pub const SPECULAR_AMOUNT: f64 = 0.0;
    pub const SPECULAR_ROUGHNESS: f64 = 0.0;
}

pub mod principled_material {
    use super::enums::{alpha, channel, lighting};
    pub const BASE_COLOR: [u8; 4] = [255, 255, 255, 255];
    pub const METALNESS: f64 = 0.0;
    pub const ROUGHNESS: f64 = 0.0;
    pub const EMISSIVE_FACTOR: [f32; 3] = [0.0, 0.0, 0.0];
    pub const OPACITY: f64 = 1.0;
    pub const ALPHA_MODE: i32 = alpha::DEFAULT;
    pub const ALPHA_CUTOFF: f64 = 0.5;
    pub const LIGHTING: i32 = lighting::FRAGMENT_LIGHTING;
    pub const METALNESS_CHANNEL: i32 = channel::B;
    pub const ROUGHNESS_CHANNEL: i32 = channel::G;
    pub const NORMAL_STRENGTH: f64 = 1.0;
    pub const OCCLUSION_CHANNEL: i32 = channel::R;
    pub const OCCLUSION_AMOUNT: f64 = 1.0;
    pub const OPACITY_CHANNEL: i32 = channel::A;
}

pub mod texture {
    use super::enums::{filter, tiling};
    pub const SCALE_U: f64 = 1.0;
    pub const SCALE_V: f64 = 1.0;
    pub const POSITION_U: f64 = 0.0;
    pub const POSITION_V: f64 = 0.0;
    pub const ROTATION_UV: f64 = 0.0;
    pub const PIVOT_U: f64 = 0.0;
    pub const PIVOT_V: f64 = 0.0;
    pub const FLIP_U: bool = false;
    pub const FLIP_V: bool = false;
    pub const INDEX_UV: i32 = 0;
    pub const TILING_MODE: i32 = tiling::REPEAT;
    pub const MIN_FILTER: i32 = filter::LINEAR;
    pub const MAG_FILTER: i32 = filter::LINEAR;
    pub const MIP_FILTER: i32 = filter::NONE;
    pub const GENERATE_MIPMAPS: bool = false;
    pub const AUTO_ORIENTATION: bool = true;
}

pub mod scene_environment {
    use super::enums::{antialiasing, background, tonemap};
    pub const CLEAR_COLOR: [u8; 4] = [0, 0, 0, 255];
    pub const BACKGROUND_MODE: i32 = background::TRANSPARENT;
    pub const TONEMAP_MODE: i32 = tonemap::LINEAR;
    pub const PROBE_EXPOSURE: f64 = 1.0;
    pub const PROBE_HORIZON: f64 = 0.0;
    pub const PROBE_ORIENTATION: [f32; 3] = [0.0, 0.0, 0.0];
    pub const SKYBOX_BLUR_AMOUNT: f64 = 0.0;
    pub const ANTIALIASING_MODE: i32 = antialiasing::NO_AA;
    pub const ANTIALIASING_QUALITY: i32 = antialiasing::HIGH;
    pub const AO_ENABLED: bool = false;
    pub const AO_STRENGTH: f64 = 0.0;
    pub const AO_DISTANCE: f64 = 5.0;
    pub const AO_SOFTNESS: f64 = 50.0;
    pub const AO_SAMPLE_RATE: i32 = 2;
    pub const AO_BIAS: f64 = 0.0;
    pub const AO_DITHER: bool = false;
    /// What `aoEnabled` turned on writes to a strength still at 0: Qt
    /// 6.5's switch shows the effect at once. The host does this write
    /// itself, as the member's change handler; the mapping only reads.
    pub const AO_STRENGTH_ENABLED: f64 = 100.0;
}

pub mod extended_scene_environment {
    use super::enums::glow_blend;
    pub const EXPOSURE: f64 = 1.0;
    pub const WHITE_POINT: f64 = 1.0;
    pub const SHARPNESS_AMOUNT: f64 = 0.0;
    pub const FXAA_ENABLED: bool = false;
    pub const DITHERING_ENABLED: bool = false;
    pub const COLOR_ADJUSTMENTS_ENABLED: bool = false;
    pub const ADJUSTMENT_BRIGHTNESS: f64 = 1.0;
    pub const ADJUSTMENT_CONTRAST: f64 = 1.0;
    pub const ADJUSTMENT_SATURATION: f64 = 1.0;
    pub const DEPTH_OF_FIELD_ENABLED: bool = false;
    pub const DEPTH_OF_FIELD_FOCUS_DISTANCE: f64 = 600.0;
    pub const DEPTH_OF_FIELD_FOCUS_RANGE: f64 = 100.0;
    pub const DEPTH_OF_FIELD_BLUR_AMOUNT: f64 = 4.0;
    pub const GLOW_ENABLED: bool = false;
    pub const GLOW_QUALITY_HIGH: bool = false;
    pub const GLOW_STRENGTH: f64 = 1.0;
    pub const GLOW_INTENSITY: f64 = 1.0;
    pub const GLOW_BLOOM: f64 = 0.0;
    pub const GLOW_HDR_MAXIMUM_VALUE: f64 = 12.0;
    pub const GLOW_HDR_MINIMUM_VALUE: f64 = 1.0;
    pub const GLOW_HDR_SCALE: f64 = 2.0;
    pub const GLOW_LEVEL: i32 = 1;
    pub const GLOW_BLEND_MODE: i32 = glow_blend::SOFT_LIGHT;
    pub const VIGNETTE_ENABLED: bool = false;
    pub const VIGNETTE_STRENGTH: f64 = 15.0;
    pub const VIGNETTE_COLOR: [u8; 4] = [128, 128, 128, 255];
    pub const VIGNETTE_RADIUS: f64 = 0.35;
    pub const LUT_ENABLED: bool = false;
    pub const LUT_SIZE: f64 = 16.0;
    pub const LUT_FILTER_ALPHA: f64 = 1.0;
}
