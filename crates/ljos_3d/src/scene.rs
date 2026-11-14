//! The retained scene: what a `View3D` shows, as plain data.
//!
//! The QML types write it and a renderer reads it; neither knows the
//! other. Each change bumps [`Scene::revision`], so a renderer that drew a
//! revision at a size draws nothing again until one of the two changes:
//! an idle 3D view costs the blit of the picture it already has.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use slotmap::{SecondaryMap, SlotMap, new_key_type};

use crate::custom::{AmbientOcclusion, CustomMaterial, DepthDraw, Effect, PostProcess};
use crate::geometry::CustomGeometry;
use crate::instancing::{Instancing, Skin};
use crate::math::{Mat4, Quat, Vec3};
use crate::texture::{Channel, Texture};

new_key_type! {
    /// A node of one [`Scene`].
    pub struct NodeId;
}

/// A colour as QML gives it: sRGB, not premultiplied, each channel 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Color { r, g, b, a: 1.0 }
    }

    /// The channels in linear light, where the lighting is summed.
    pub fn to_linear(self) -> [f32; 4] {
        // The fit to sRGB Qt's shaders use, the inverse of what tone.wgsl
        // writes: a colour said in QML comes back out as it was.
        fn lin(c: f32) -> f32 {
            c * (c * (c * 0.305_306 + 0.682_171_1) + 0.012_522_878)
        }
        [lin(self.r), lin(self.g), lin(self.b), self.a]
    }
}

/// A node's own placement: `position`, `rotation` and `scale`, relative to
/// its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    /// A node placed at `position` that scales and turns about `pivot`,
    /// a point in its own space: the pivot is what lands on `position`,
    /// so the node's origin moves away from it by the scaled, turned pivot.
    pub fn about_pivot(position: Vec3, rotation: Quat, scale: Vec3, pivot: Vec3) -> Transform {
        let origin = rotation.rotate(Vec3::new(
            -pivot.x * scale.x,
            -pivot.y * scale.y,
            -pivot.z * scale.z,
        ));
        Transform {
            position: Vec3::new(
                position.x + origin.x,
                position.y + origin.y,
                position.z + origin.z,
            ),
            rotation,
            scale,
        }
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_trs(self.position, self.rotation, self.scale)
    }
}

/// Qt Quick 3D's built-in meshes (`source: "#Cube"`), each 100 scene
/// units across: the cube a side, the sphere its diameter, the cylinder
/// and the cone their height and the diameter of their base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Primitive {
    /// Centred on its origin.
    Cube,
    /// In the XY plane, facing +Z, centred on its origin.
    Rectangle,
    /// Centred on its origin.
    Sphere,
    /// Standing along Y, centred on its origin.
    Cylinder,
    /// Standing on its base at the origin, pointing up Y.
    Cone,
}

impl Primitive {
    /// From a `Model.source` such as `#Cube`; `None` for a name this
    /// renderer does not build yet.
    pub fn from_source(source: &str) -> Option<Self> {
        match source {
            "#Cube" => Some(Primitive::Cube),
            "#Rectangle" => Some(Primitive::Rectangle),
            "#Sphere" => Some(Primitive::Sphere),
            "#Cylinder" => Some(Primitive::Cylinder),
            "#Cone" => Some(Primitive::Cone),
            _ => None,
        }
    }

    /// A sphere that holds the mesh, centre and radius in model space:
    /// what frustum culling tests.
    pub fn bounds(self) -> (Vec3, f32) {
        let half_diagonal = 50.0 * std::f32::consts::SQRT_2;
        match self {
            Primitive::Cube => (Vec3::ZERO, 50.0 * 3f32.sqrt()),
            Primitive::Rectangle | Primitive::Cylinder => (Vec3::ZERO, half_diagonal),
            Primitive::Sphere => (Vec3::ZERO, 50.0),
            Primitive::Cone => (Vec3::new(0.0, 50.0, 0.0), half_diagonal),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MeshSource {
    Primitive(Primitive),
    /// An application's own geometry, shared with the renderer's cache.
    Custom(Arc<CustomGeometry>),
}

impl MeshSource {
    /// A sphere that holds the mesh: what frustum culling tests.
    pub fn bounds(&self) -> (Vec3, f32) {
        match self {
            MeshSource::Primitive(p) => p.bounds(),
            MeshSource::Custom(g) => g.bounding_sphere(),
        }
    }

    /// The box that holds the mesh in model space, least corner then most:
    /// what tells two models that may meet from two that cannot.
    pub fn bounding_box(&self) -> (Vec3, Vec3) {
        match self {
            MeshSource::Primitive(p) => crate::mesh::primitive_box(*p),
            MeshSource::Custom(g) => g.bounding_box(),
        }
    }
}

/// `DefaultMaterial.lighting`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lighting {
    /// The diffuse colour as it is, whatever the lights.
    None,
    /// Qt's default: lit by the scene's lights.
    #[default]
    Fragment,
}

/// `Material.cullMode`: which faces are not drawn. A face is a front face
/// where its triangle winds counter-clockwise on the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CullMode {
    /// Qt's default.
    #[default]
    Back,
    Front,
    None,
}

/// `blendMode` of the built-in materials: how a surface is laid over what
/// is behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    SourceOver,
    Screen,
    Multiply,
}

/// `DefaultMaterial`, as far as it is built: a diffuse colour lit by the
/// scene's lights, with a highlight of `specular_amount` and `specular_tint`.
#[derive(Clone, Debug, PartialEq)]
pub struct DefaultMaterial {
    pub diffuse_color: Color,
    /// Multiplies `diffuse_color`; only once its image is loaded.
    pub diffuse_map: Option<Texture>,
    pub lighting: Lighting,
    /// 0 to 1; 0, Qt's default, has no highlight.
    pub specular_amount: f32,
    /// 0 to 1: how far the highlight spreads.
    pub specular_roughness: f32,
    /// How far its normal map turns it; 0, Qt's default, not at all.
    pub bump_amount: f32,
    pub cull_mode: CullMode,
    pub depth_draw: DepthDraw,
    /// Multiplies `diffuse_color`'s alpha.
    pub opacity: f32,
    pub opacity_channel: Channel,
    /// The map says how clear the surface is, not how opaque.
    pub invert_opacity_map_value: bool,
    /// Linear light the surface gives by itself.
    pub emissive: [f32; 3],
    /// Boxed for the reason [`PrincipledMaterial::maps`] is.
    pub maps: Box<DefaultMaps>,
    pub specular_tint: Color,
    /// 0 has the highlight the same seen from any side; above it, the
    /// highlight grows towards the edges.
    pub fresnel_power: f32,
    pub index_of_refraction: f32,
    pub blend_mode: BlendMode,
    pub vertex_colors: bool,
}

/// The maps of a [`DefaultMaterial`] past its diffuse one; each only once
/// its image is loaded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DefaultMaps {
    pub opacity: Option<Texture>,
    pub emissive: Option<Texture>,
    pub normal: Option<Texture>,
    pub roughness: Option<Texture>,
    /// Multiplies the highlight.
    pub specular: Option<Texture>,
}

impl Default for DefaultMaterial {
    fn default() -> Self {
        DefaultMaterial {
            diffuse_color: Color::WHITE,
            diffuse_map: None,
            lighting: Lighting::Fragment,
            specular_amount: 0.0,
            specular_roughness: 0.0,
            bump_amount: 0.0,
            cull_mode: CullMode::Back,
            depth_draw: DepthDraw::OpaqueOnly,
            opacity: 1.0,
            opacity_channel: Channel::A,
            invert_opacity_map_value: false,
            emissive: [0.0; 3],
            maps: Box::default(),
            specular_tint: Color::WHITE,
            fresnel_power: 0.0,
            index_of_refraction: 1.45,
            blend_mode: BlendMode::SourceOver,
            vertex_colors: false,
        }
    }
}

/// `PrincipledMaterial.alphaMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    /// Blended where the opacity is less than 1, opaque where it is 1.
    #[default]
    Default,
    /// Always blended.
    Blend,
    /// The opacity is ignored.
    Opaque,
    /// Not drawn below `alpha_cutoff`, opaque at or above it.
    Mask,
}

/// `PrincipledMaterial`, the metal-roughness model.
#[derive(Clone, Debug, PartialEq)]
pub struct PrincipledMaterial {
    /// Its alpha is part of the opacity.
    pub base_color: Color,
    /// 0 a dielectric, 1 a metal.
    pub metalness: f32,
    pub roughness: f32,
    /// Linear light the surface gives by itself, whatever lights it.
    pub emissive: [f32; 3],
    pub opacity: f32,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub cull_mode: CullMode,
    pub lighting: Lighting,
    /// Boxed: seven maps would make every material, and every node, as
    /// large as one that has them all.
    pub maps: Box<PrincipledMaps>,
}

/// The maps of a [`PrincipledMaterial`]; each only once its image is
/// loaded. The colour maps multiply their value, the single-value maps
/// multiply it by one channel.
#[derive(Clone, Debug, PartialEq)]
pub struct PrincipledMaps {
    pub base_color: Option<Texture>,
    pub metalness: Option<Texture>,
    pub metalness_channel: Channel,
    pub roughness: Option<Texture>,
    pub roughness_channel: Channel,
    /// Tangent-space normals, as RGB from 0 to 1.
    pub normal: Option<Texture>,
    /// 0 leaves the normals as the mesh has them, 1 takes the map's.
    pub normal_strength: f32,
    pub emissive: Option<Texture>,
    /// Shades the light the environment gives, not the lights'.
    pub occlusion: Option<Texture>,
    pub occlusion_channel: Channel,
    pub occlusion_amount: f32,
    pub opacity: Option<Texture>,
    pub opacity_channel: Channel,
    /// The map says how clear the surface is, not how opaque.
    pub invert_opacity_map_value: bool,
    /// How much of the background shows through the diffuse light.
    pub transmission: Option<Texture>,
    pub transmission_channel: Channel,
    /// Moves what the other maps read, so that the surface looks carved.
    pub height: Option<Texture>,
    pub height_channel: Channel,
    /// Multiply the clearcoat's amount and its roughness.
    pub clearcoat: Option<Texture>,
    pub clearcoat_channel: Channel,
    pub clearcoat_roughness: Option<Texture>,
    pub clearcoat_roughness_channel: Channel,
    /// Bends the clearcoat alone, as much as its strength says.
    pub clearcoat_normal: Option<Texture>,
    pub clearcoat_normal_strength: f32,
    pub surface: Surface,
}

/// What a [`PrincipledMaterial`] has beyond the metal-roughness model.
/// In its box as most materials leave all of it as it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub depth_draw: DepthDraw,
    /// How much of a dielectric's reflection there is; metals keep theirs.
    pub specular_amount: f32,
    /// 0 reflects white, 1 the base colour; metals always the base colour.
    pub specular_tint: f32,
    /// What a dielectric reflects head on comes from it.
    pub index_of_refraction: f32,
    pub fresnel: Fresnel,
    /// A second, dielectric layer over the surface: how much there is.
    pub clearcoat_amount: f32,
    pub clearcoat_roughness: f32,
    pub clearcoat_fresnel: Fresnel,
    pub transmission_factor: f32,
    pub height_amount: f32,
    /// The fewest and the most samples a pixel's height is looked for with.
    pub height_samples: [u32; 2],
    pub blend_mode: BlendMode,
    pub vertex_colors: bool,
    /// Lights this material in place of the environment's light probe.
    pub light_probe: Option<Texture>,
}

/// How a reflection grows towards the edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fresnel {
    pub power: f32,
    pub scale: f32,
    pub bias: f32,
}

impl Default for Fresnel {
    fn default() -> Self {
        Fresnel {
            power: 5.0,
            scale: 1.0,
            bias: 0.0,
        }
    }
}

impl Default for Surface {
    fn default() -> Self {
        Surface {
            depth_draw: DepthDraw::OpaqueOnly,
            specular_amount: 1.0,
            specular_tint: 0.0,
            index_of_refraction: 1.5,
            fresnel: Fresnel::default(),
            clearcoat_amount: 0.0,
            clearcoat_roughness: 0.0,
            clearcoat_fresnel: Fresnel::default(),
            transmission_factor: 0.0,
            height_amount: 0.0,
            height_samples: [8, 32],
            blend_mode: BlendMode::SourceOver,
            vertex_colors: true,
            light_probe: None,
        }
    }
}

impl Default for PrincipledMaps {
    fn default() -> Self {
        PrincipledMaps {
            base_color: None,
            metalness: None,
            metalness_channel: Channel::B,
            roughness: None,
            roughness_channel: Channel::G,
            normal: None,
            normal_strength: 1.0,
            emissive: None,
            occlusion: None,
            occlusion_channel: Channel::R,
            occlusion_amount: 1.0,
            opacity: None,
            opacity_channel: Channel::A,
            invert_opacity_map_value: false,
            transmission: None,
            transmission_channel: Channel::R,
            height: None,
            height_channel: Channel::R,
            clearcoat: None,
            clearcoat_channel: Channel::R,
            clearcoat_roughness: None,
            clearcoat_roughness_channel: Channel::G,
            clearcoat_normal: None,
            clearcoat_normal_strength: 1.0,
            surface: Surface::default(),
        }
    }
}

impl Default for PrincipledMaterial {
    fn default() -> Self {
        PrincipledMaterial {
            base_color: Color::WHITE,
            metalness: 0.0,
            roughness: 0.0,
            emissive: [0.0; 3],
            opacity: 1.0,
            alpha_mode: AlphaMode::Default,
            alpha_cutoff: 0.5,
            cull_mode: CullMode::Back,
            lighting: Lighting::Fragment,
            maps: Box::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Material {
    Default(DefaultMaterial),
    Principled(PrincipledMaterial),
    /// An author's shader snippets (`CustomMaterial`).
    Custom(CustomMaterial),
}

impl Material {
    /// Whether its own opacity leaves nothing to draw. Whatever its alpha
    /// mode: a model used only for its place has a material like this.
    pub fn is_clear(&self) -> bool {
        match self {
            Material::Default(m) => m.opacity <= 0.0,
            Material::Principled(m) => m.opacity <= 0.0,
            Material::Custom(_) => false,
        }
    }

    pub fn cull_mode(&self) -> CullMode {
        match self {
            Material::Default(m) => m.cull_mode,
            Material::Principled(m) => m.cull_mode,
            Material::Custom(m) => m.cull_mode,
        }
    }

    /// The opacity a pixel of the material has before its alpha mode.
    /// What it says of hiding what is drawn behind it afterwards.
    pub fn depth_draw(&self) -> DepthDraw {
        match self {
            Material::Default(m) => m.depth_draw,
            Material::Principled(m) => m.maps.surface.depth_draw,
            Material::Custom(m) => m.depth_draw,
        }
    }

    pub fn blend_mode(&self) -> BlendMode {
        match self {
            Material::Default(m) => m.blend_mode,
            Material::Principled(m) => m.maps.surface.blend_mode,
            // Its own source and destination factors say.
            Material::Custom(_) => BlendMode::SourceOver,
        }
    }

    pub fn alpha(&self) -> f32 {
        match self {
            Material::Default(m) => m.diffuse_color.a * m.opacity,
            Material::Principled(m) => m.base_color.a * m.opacity,
            // What the snippet writes is not known here.
            Material::Custom(_) => 1.0,
        }
    }

    /// Whether it is laid over what is behind it, and so drawn after the
    /// opaque models, back to front.
    pub fn blends(&self) -> bool {
        let see_through = |map: &Option<Texture>| map.as_ref().is_some_and(|t| !t.image.opaque());
        match self {
            Material::Default(m) => {
                self.alpha() < 1.0
                    || m.maps.opacity.is_some()
                    || m.blend_mode != BlendMode::SourceOver
                    || see_through(&m.diffuse_map)
            }
            Material::Principled(m)
                if m.maps.surface.blend_mode != BlendMode::SourceOver
                    || m.maps.surface.transmission_factor > 0.0 =>
            {
                true
            }
            Material::Principled(m) => match m.alpha_mode {
                AlphaMode::Default => {
                    self.alpha() < 1.0
                        || m.maps.opacity.is_some()
                        || see_through(&m.maps.base_color)
                }
                AlphaMode::Blend => true,
                AlphaMode::Opaque | AlphaMode::Mask => false,
            },
            Material::Custom(m) => m.blends(),
        }
    }
}

impl Default for Material {
    fn default() -> Self {
        Material::Default(DefaultMaterial::default())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub mesh: MeshSource,
    pub material: Material,
}

/// A camera looks down its local -Z axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Camera {
    /// `PerspectiveCamera`; Qt's defaults are a 60° vertical field of view
    /// and clip planes at 10 and 10000.
    Perspective {
        /// Vertical, or horizontal where `fov_horizontal` says so.
        fov_degrees: f32,
        fov_horizontal: bool,
        clip_near: f32,
        clip_far: f32,
        /// `frustumCullingEnabled`.
        frustum_culling: bool,
    },
    /// `OrthographicCamera`: one scene unit is one pixel of the view at a
    /// magnification of 1.
    Orthographic {
        clip_near: f32,
        clip_far: f32,
        horizontal_magnification: f32,
        vertical_magnification: f32,
        frustum_culling: bool,
    },
}

impl Camera {
    pub const PERSPECTIVE: Camera = Camera::Perspective {
        fov_degrees: 60.0,
        fov_horizontal: false,
        clip_near: 10.0,
        clip_far: 10000.0,
        frustum_culling: false,
    };

    /// Whether what it cannot see is left out before drawing.
    pub fn frustum_culling(&self) -> bool {
        match *self {
            Camera::Perspective {
                frustum_culling, ..
            }
            | Camera::Orthographic {
                frustum_culling, ..
            } => frustum_culling,
        }
    }

    /// View space to clip space for a view `width` by `height` pixels.
    pub fn projection(&self, width: f32, height: f32) -> Mat4 {
        let aspect = if height > 0.0 { width / height } else { 1.0 };
        match *self {
            Camera::Perspective {
                fov_degrees,
                fov_horizontal,
                clip_near,
                clip_far,
                ..
            } => {
                let fov = fov_degrees.to_radians();
                // A horizontal field of view keeps the width and lets the
                // height follow the aspect ratio.
                let fov_y = if fov_horizontal {
                    2.0 * ((fov * 0.5).tan() / aspect).atan()
                } else {
                    fov
                };
                Mat4::perspective(fov_y, aspect, clip_near, clip_far)
            }
            Camera::Orthographic {
                clip_near,
                clip_far,
                horizontal_magnification,
                vertical_magnification,
                ..
            } => Mat4::orthographic(
                width / horizontal_magnification.max(f32::EPSILON),
                height / vertical_magnification.max(f32::EPSILON),
                clip_near,
                clip_far,
            ),
        }
    }
}

/// How a point or spot light fades with the distance `d` from it: its
/// light is divided by `constant + linear × 0.01 × d + quadratic × 0.0001
/// × d²`, the factors as QML gives them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub constant: f32,
    pub linear: f32,
    pub quadratic: f32,
}

impl Default for Fade {
    /// Qt's: the inverse square of the distance, a hundred units away
    /// halving the light.
    fn default() -> Self {
        Fade {
            constant: 1.0,
            linear: 0.0,
            quadratic: 1.0,
        }
    }
}

impl Fade {
    /// What the light is multiplied by `d` units away.
    pub fn at(&self, d: f32) -> f32 {
        let divisor = self.constant + self.linear * 0.01 * d + self.quadratic * 0.0001 * d * d;
        if divisor > 0.0 { 1.0 / divisor } else { 0.0 }
    }
}

/// A light. Directional and spot lights shine along their local -Z axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Light {
    Directional {
        color: Color,
        ambient_color: Color,
        brightness: f32,
    },
    /// Shines from where it is in every direction.
    Point {
        color: Color,
        ambient_color: Color,
        brightness: f32,
        fade: Fade,
    },
    /// A point light that shines only inside a cone.
    Spot {
        color: Color,
        ambient_color: Color,
        brightness: f32,
        fade: Fade,
        /// The cone, edge to edge, in degrees.
        cone_angle: f32,
        /// Where the light starts to fall off towards the cone's edge;
        /// one wider than the cone is the cone.
        inner_cone_angle: f32,
    },
}

impl Light {
    pub const DIRECTIONAL: Light = Light::Directional {
        color: Color::WHITE,
        ambient_color: Color::BLACK,
        brightness: 1.0,
    };

    /// The colour, brightness and ambient colour every light has.
    pub fn common(&self) -> (Color, f32, Color) {
        match *self {
            Light::Directional {
                color,
                ambient_color,
                brightness,
            }
            | Light::Point {
                color,
                ambient_color,
                brightness,
                ..
            }
            | Light::Spot {
                color,
                ambient_color,
                brightness,
                ..
            } => (color, brightness, ambient_color),
        }
    }
}

/// `Light.castsShadow` and what shapes the shadows: a light with one casts
/// them. The sizes and distances are in scene units, as QML gives them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// `shadowMapQuality`: the side of the map, 256 to 4096.
    pub map_size: u32,
    /// `shadowBias`: how far towards the light a surface is taken before
    /// it is compared with the map, so that it does not shadow itself.
    pub bias: f32,
    /// `shadowFactor` over 100: how much of the light a shadow keeps away.
    pub factor: f32,
    /// `shadowMapFar`: the farthest a point or spot light's map reaches,
    /// and how far from the camera a directional light's cascades do.
    pub far: f32,
    /// `softShadowQuality`: samples per pixel, 1 for hard shadows.
    pub samples: u32,
    /// `pcfFactor`: the radius the samples are spread over.
    pub pcf_radius: f32,
    /// A directional light's cascades.
    pub cascades: Cascades,
}

impl Default for Shadow {
    /// The documentation's: a low quality map (256), a bias of 10, a
    /// factor of 75, a far of 5000 and four samples 2 units apart.
    fn default() -> Self {
        Shadow {
            map_size: 256,
            bias: 10.0,
            factor: 0.75,
            far: 5000.0,
            samples: 4,
            pcf_radius: 2.0,
            cascades: Cascades::default(),
        }
    }
}

/// `DirectionalLight`'s cascaded shadow maps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cascades {
    /// `csmNumSplits`, 0 to 3. With none, one map covers every model that
    /// casts or receives shadows, wherever the camera is.
    pub splits: u32,
    /// `csmSplit1` to `csmSplit3`: where each split is, as a share of the
    /// distance from the camera's near plane to `Shadow::far`.
    pub at: [f32; 3],
    /// `csmBlendRatio`: the share of a cascade, at its far end, blended
    /// into the next.
    pub blend: f32,
    /// `lockShadowmapTexels`: each cascade keeps its size and moves by
    /// whole texels, so its edges do not shimmer as the camera moves.
    pub lock_texels: bool,
}

impl Default for Cascades {
    fn default() -> Self {
        Cascades {
            splits: 0,
            at: [0.1, 0.25, 0.5],
            blend: 0.05,
            lock_texels: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    /// A plain `Node`: a transform for its children.
    Group,
    Model(Model),
    Camera(Camera),
    Light(Light),
    ReflectionProbe(ReflectionProbe),
}

/// `ReflectionProbe`: a place the scene is looked at from all round, for the
/// models near it that take reflections to mirror in place of the light
/// probe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReflectionProbe {
    /// How far across each side of its cube is.
    pub across: u32,
    /// What nothing is seen of shows, as it is: not a colour of the screen's.
    pub clear: [f32; 4],
    /// Looked through once, and again when `turn` changes, rather than for
    /// every picture.
    pub once: bool,
    pub turn: u64,
    /// `parallaxCorrection`: what is read is put right for where the surface
    /// is in the box.
    pub parallax: bool,
    /// `boxSize`, about where the probe is, moved by `boxOffset`.
    pub box_size: Vec3,
    pub box_offset: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    pub transform: Transform,
    pub visible: bool,
    /// This node's own opacity; its children's are multiplied by it
    /// ([`Scene::faded`]).
    pub opacity: f32,
    /// A model's `castsShadows`: whether it is drawn into shadow maps.
    pub casts_shadows: bool,
    /// A model's `receivesShadows`: whether shadows darken it.
    pub receives_shadows: bool,
    /// A model's `castsReflections`: whether a reflection probe sees it.
    pub casts_reflections: bool,
    /// A model's `receivesReflections`: whether it mirrors a reflection
    /// probe whose box it is in.
    pub receives_reflections: bool,
    /// A light's shadow, where its `castsShadow` is true.
    ///
    /// These three are the node's rather than in [`Model`] and [`Light`]:
    /// a light's uniforms and every model built elsewhere stay as they
    /// were, and a node that is neither ignores them.
    pub shadow: Option<Shadow>,
    /// A model's copies, each drawn where its table's entry puts it.
    pub instancing: Option<Instancing>,
    /// A model's joints, which bend its mesh in place of its transform.
    pub skin: Option<Skin>,
    /// A model's `depthBias`: added to its distance from the camera, which
    /// orders what is blended.
    pub depth_bias: f32,
    /// A light's `scope`: it lights only this node and what is inside it.
    pub scope: Option<NodeId>,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

impl Node {
    pub fn new(kind: NodeKind) -> Self {
        Node {
            kind,
            transform: Transform::default(),
            visible: true,
            opacity: 1.0,
            casts_shadows: true,
            receives_shadows: true,
            casts_reflections: true,
            receives_reflections: false,
            shadow: None,
            instancing: None,
            skin: None,
            depth_bias: 0.0,
            scope: None,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
}

/// `SceneEnvironment.backgroundMode`, as far as it is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Background {
    /// Qt's default: the 2D scene shows through where nothing is drawn.
    Transparent,
    Color(Color),
    /// The light probe, around the scene; transparent without one.
    SkyBox,
}

/// `SceneEnvironment.tonemapMode`: how the linear light a pixel gathers
/// becomes a colour on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tonemap {
    /// The light as it is, neither exposed nor gamma corrected.
    None,
    /// Qt's default: exposed, clamped and encoded as sRGB.
    #[default]
    Linear,
    /// Narkowicz's fit of the ACES filmic curve.
    Aces,
    /// Hejl and Burgess-Dawson's curve, which has the gamma in it.
    HejlDawson,
    /// Hable's filmic curve.
    Filmic,
}

impl Tonemap {
    /// A colour in linear light as this mode shows it, by the sums of
    /// `tone.wgsl`, for what is not drawn by a shader: the clear colour.
    #[cfg(feature = "gpu")]
    pub(crate) fn shown(self, c: [f32; 3]) -> [f32; 3] {
        let screen = |c: f32| {
            let s1 = c.max(0.0).sqrt();
            let s2 = s1.sqrt();
            0.585_122_4 * s1 + 0.783_140_36 * s2 - 0.368_262_74 * s2.sqrt()
        };
        let hable =
            |x: f32| (x * (0.15 * x + 0.05) + 0.004) / (x * (0.15 * x + 0.5) + 0.06) - 0.02 / 0.3;
        c.map(|c| match self {
            Tonemap::None => c,
            Tonemap::Linear => screen(c),
            Tonemap::Aces => screen((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14)),
            Tonemap::HejlDawson => {
                let h = (c - 0.004).max(0.0);
                (h * (6.2 * h + 0.5)) / (h * (6.2 * h + 1.7) + 0.06)
            }
            Tonemap::Filmic => screen(hable(2.0 * c) / hable(11.2)),
        })
    }
}

/// `SceneEnvironment.lightProbe` and how it is laid around the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct LightProbe {
    /// An equirectangular image: longitude across, the zenith at its top.
    pub texture: Texture,
    /// `probeExposure`: multiplies its light.
    pub exposure: f32,
    /// `probeHorizon`, 0 to 1: how much darker the light from below the
    /// horizon is on the materials it lights; the sky box is not darkened.
    pub horizon: f32,
    /// `probeOrientation`: Euler angles in degrees, as a node's rotation.
    pub orientation: Vec3,
    /// `skyboxBlurAmount`, 0 to 1.
    pub skybox_blur: f32,
}

impl LightProbe {
    /// `texture` laid around the scene as the documentation's defaults
    /// lay it: exposure 1, no horizon, not turned, not blurred.
    pub fn new(texture: Texture) -> Self {
        LightProbe {
            texture,
            exposure: 1.0,
            horizon: 0.0,
            orientation: Vec3::ZERO,
            skybox_blur: 0.0,
        }
    }
}

/// `SceneEnvironment.fog`, when enabled: what lit surfaces fade into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    pub color: Color,
    pub density: f32,
    /// By the distance from the camera: where it starts, where it is
    /// whole (0 at or below is the camera's far clip) and the curve.
    pub depth: Option<[f32; 3]>,
    /// By height: the y of the least, the y of the most, and the curve.
    pub height: Option<[f32; 3]>,
    /// The curve of light from behind the fog brightening it.
    pub transmit: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    pub background: Background,
    /// Samples a pixel for multisample antialiasing (`antialiasingMode`
    /// MSAA): 1 for none, else 2, 4 or 8, which a renderer may bring to a
    /// count its device has.
    pub msaa_samples: u32,
    /// Drawn for the cube of a reflection probe: the light as it is, over
    /// this rather than the background's colour.
    #[doc(hidden)]
    pub capture: Option<[f32; 4]>,
    /// Drawn for what a material reads as the picture behind it: only what
    /// is opaque and does not read it, into a capture, before the tone map.
    #[doc(hidden)]
    pub behind: bool,
    pub light_probe: Option<LightProbe>,
    pub tonemap: Tonemap,
    /// Multiplies the light before it is tone mapped; ignored by
    /// [`Tonemap::None`].
    pub exposure: f32,
    /// The light that maps to white under the filmic curves (ACES,
    /// Hejl-Dawson, filmic).
    pub white_point: f32,
    /// `effects`, run over the picture in their order.
    pub effects: Vec<Effect>,
    /// ExtendedSceneEnvironment's built-in post-processing.
    pub post: PostProcess,
    /// `None` while its strength or distance is 0.
    pub ambient_occlusion: Option<AmbientOcclusion>,
    pub fog: Option<Fog>,
    /// `specularAAEnabled`: widens highlights where normals change fast
    /// across a pixel.
    pub specular_aa: bool,
    /// `depthTestEnabled`: off, a surface hides nothing and nothing hides it.
    pub depth_test: bool,
}

impl Environment {
    /// Whether the picture is drawn in high dynamic range and finished by
    /// passes over it, rather than tone mapped as it is drawn.
    pub fn post_processed(&self) -> bool {
        !self.effects.is_empty() || self.post.on || self.ambient_occlusion.is_some()
    }
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            background: Background::Transparent,
            msaa_samples: 1,
            light_probe: None,
            capture: None,
            behind: false,
            tonemap: Tonemap::Linear,
            exposure: 1.0,
            white_point: 1.0,
            effects: Vec::new(),
            post: PostProcess::default(),
            ambient_occlusion: None,
            fog: None,
            specular_aa: false,
            depth_test: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    // Shared, so a scene that is patched while a renderer still holds the
    // last one copies a pointer per node and not the nodes.
    nodes: SlotMap<NodeId, Arc<Node>>,
    roots: Vec<NodeId>,
    environment: Environment,
    camera: Option<NodeId>,
    /// In place of any camera: where a reflection probe looks through a
    /// side of its cube.
    looking: Option<(Camera, Mat4)>,
    revision: u64,
}

impl Scene {
    pub fn new() -> Self {
        Scene::default()
    }

    /// Each node's opacity with its parents' multiplied in, 0 to 1. `None`
    /// when every node is whole, so a scene that fades nothing pays one look
    /// over its nodes.
    pub fn faded(&self) -> Option<SecondaryMap<NodeId, f32>> {
        if self.nodes.values().all(|n| n.opacity >= 1.0) {
            return None;
        }
        let mut out = SecondaryMap::with_capacity(self.nodes.len());
        let mut stack: Vec<(NodeId, f32)> = self.roots.iter().map(|r| (*r, 1.0)).collect();
        while let Some((id, above)) = stack.pop() {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            // A broken value hides rather than poison what is under it.
            let own = if node.opacity.is_nan() {
                0.0
            } else {
                above * node.opacity.clamp(0.0, 1.0)
            };
            out.insert(id, own);
            stack.extend(node.children.iter().map(|c| (*c, own)));
        }
        Some(out)
    }

    /// Changes whenever anything a renderer reads changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    // One counter for every scene: a view handed a scene built anew must
    // not take it for the one it drew because both counted to the same
    // number.
    fn touch(&mut self) {
        static REVISIONS: AtomicU64 = AtomicU64::new(1);
        self.revision = REVISIONS.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether `other` draws the same: the same nodes under the same ids,
    /// environment and camera, whatever the revisions. A host that builds
    /// its scene again from its items keeps the one it had when nothing
    /// changed, so the picture is not drawn again.
    pub fn same_as(&self, other: &Scene) -> bool {
        self.roots == other.roots
            && self.environment == other.environment
            && self.camera == other.camera
            && self.nodes.len() == other.nodes.len()
            && self.nodes.iter().all(|(id, n)| {
                other
                    .nodes
                    .get(id)
                    .is_some_and(|o| Arc::ptr_eq(o, n) || o == n)
            })
    }

    /// Which node of `other` each of this scene's is, when the two are the
    /// same tree of the same nodes whatever ids each gave them: the same
    /// roots and children in the same order, each node naming the other's
    /// node where this one names its own, the same environment and camera.
    /// Otherwise what is found first to differ, said: what a check that a
    /// scene kept and patched is the one built anew reports.
    pub fn matching(&self, other: &Scene) -> Result<HashMap<NodeId, NodeId>, String> {
        if self.roots.len() != other.roots.len() {
            return Err(format!(
                "{} roots and {}",
                self.roots.len(),
                other.roots.len()
            ));
        }
        let mut map = HashMap::with_capacity(self.nodes.len());
        let mut stack: Vec<(NodeId, NodeId)> = self
            .roots
            .iter()
            .copied()
            .zip(other.roots.iter().copied())
            .collect();
        while let Some((a, b)) = stack.pop() {
            let (Some(na), Some(nb)) = (self.nodes.get(a), other.nodes.get(b)) else {
                return Err(format!("{a:?} and {b:?}, one of them not in its scene"));
            };
            if na.children.len() != nb.children.len() {
                return Err(format!(
                    "{a:?} {na:#?} with {} children and {b:?} {nb:#?} with {}",
                    na.children.len(),
                    nb.children.len()
                ));
            }
            map.insert(a, b);
            stack.extend(na.children.iter().copied().zip(nb.children.iter().copied()));
        }
        if map.len() != self.nodes.len() || other.nodes.len() != self.nodes.len() {
            return Err(format!(
                "{} nodes and {}, {} of them in the tree",
                self.nodes.len(),
                other.nodes.len(),
                map.len()
            ));
        }
        let at = |id: NodeId| map.get(&id).copied().unwrap_or_default();
        for (&a, &b) in &map {
            let mut n = Node::clone(&self.nodes[a]);
            n.parent = n.parent.map(at);
            for c in &mut n.children {
                *c = at(*c);
            }
            n.scope = n.scope.map(at);
            if let Some(t) = n.instancing.as_mut() {
                t.root = t.root.map(at);
            }
            if let Some(skin) = n.skin.as_mut() {
                for j in &mut skin.joints {
                    *j = at(*j);
                }
            }
            let o = &*other.nodes[b];
            if n != *o {
                return Err(format!("{a:?} {n:#?} and {b:?} {o:#?}"));
            }
        }
        if self.environment != other.environment {
            return Err(format!(
                "environment {:#?} and {:#?}",
                self.environment, other.environment
            ));
        }
        if self.camera.map(at) != other.camera {
            return Err(format!("camera {:?} and {:?}", self.camera, other.camera));
        }
        Ok(map)
    }

    /// What [`matching`](Self::matching) finds first to differ, `None`
    /// when the two are the same tree whatever their ids.
    pub fn difference(&self, other: &Scene) -> Option<String> {
        self.matching(other).err()
    }

    /// Adds `node` as the last child of `parent`, or as a root. A parent
    /// that is not in the scene makes it a root.
    pub fn add(&mut self, parent: Option<NodeId>, mut node: Node) -> NodeId {
        let parent = parent.filter(|p| self.nodes.contains_key(*p));
        node.parent = parent;
        node.children.clear();
        let id = self.nodes.insert(Arc::new(node));
        match parent {
            Some(p) => Arc::make_mut(&mut self.nodes[p]).children.push(id),
            None => self.roots.push(id),
        }
        self.touch();
        id
    }

    /// Removes `id` and everything under it.
    pub fn remove(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        match node.parent {
            Some(p) => Arc::make_mut(&mut self.nodes[p])
                .children
                .retain(|c| *c != id),
            None => self.roots.retain(|c| *c != id),
        }
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if let Some(node) = self.nodes.remove(n) {
                stack.extend_from_slice(&node.children);
            }
        }
        if self.camera.is_some_and(|c| !self.nodes.contains_key(c)) {
            self.camera = None;
        }
        self.touch();
    }

    /// Moves `id`, with what is under it, to be the last child of
    /// `parent`, or a root; a parent not in the scene makes it a root.
    pub(crate) fn reparent(&mut self, id: NodeId, parent: Option<NodeId>) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        let was = node.parent;
        let parent = parent.filter(|p| self.nodes.contains_key(*p));
        match was {
            Some(p) => Arc::make_mut(&mut self.nodes[p])
                .children
                .retain(|c| *c != id),
            None => self.roots.retain(|c| *c != id),
        }
        Arc::make_mut(&mut self.nodes[id]).parent = parent;
        match parent {
            Some(p) => Arc::make_mut(&mut self.nodes[p]).children.push(id),
            None => self.roots.push(id),
        }
        self.touch();
    }

    /// Puts the children of `parent`, or the roots, in `order`. Returns
    /// false, changing nothing, when `order` is not the same ids.
    pub(crate) fn reorder(&mut self, parent: Option<NodeId>, order: Vec<NodeId>) -> bool {
        let held = match parent {
            None => &self.roots,
            Some(p) => match self.nodes.get(p) {
                Some(n) => &n.children,
                None => return false,
            },
        };
        if *held == order {
            return true;
        }
        let mut a = held.clone();
        let mut b = order.clone();
        a.sort_unstable();
        b.sort_unstable();
        if a != b {
            return false;
        }
        match parent {
            None => self.roots = order,
            Some(p) => Arc::make_mut(&mut self.nodes[p]).children = order,
        }
        self.touch();
        true
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id).map(|n| &**n)
    }

    /// The node to change; the scene counts it as changed.
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        if self.nodes.contains_key(id) {
            self.touch();
        }
        self.nodes.get_mut(id).map(Arc::make_mut)
    }

    /// Puts `node` in the place of `id`, under the same parent and over the
    /// same children whatever `node` says of those. The scene counts as
    /// changed only when the node does; returns whether it did.
    pub fn set_node(&mut self, id: NodeId, mut node: Node) -> bool {
        let Some(held) = self.nodes.get_mut(id) else {
            return false;
        };
        node.parent = held.parent;
        node.children.clone_from(&held.children);
        if **held == node {
            return false;
        }
        *held = Arc::new(node);
        self.touch();
        true
    }

    /// Whether putting `node` in the place of `id` ([`Scene::set_node`])
    /// would change the scene; `node` takes `id`'s links on the way.
    pub(crate) fn would_change(&self, id: NodeId, node: &mut Node) -> bool {
        let Some(held) = self.nodes.get(id) else {
            return false;
        };
        node.parent = held.parent;
        node.children.clone_from(&held.children);
        **held != *node
    }

    /// `View3D.camera`, if it is set and in the scene.
    pub fn camera(&self) -> Option<NodeId> {
        self.camera
    }

    /// The nodes in the order they were added, for a scene nothing was
    /// removed from.
    #[cfg(any(feature = "gltf", test))]
    pub(crate) fn added(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().map(|(id, n)| (id, &**n))
    }

    /// `id`'s world transform, whether or not it is visible: what a skin's
    /// joints and an instancing's root are placed by.
    pub fn world(&self, id: NodeId) -> Option<Mat4> {
        let mut node = self.nodes.get(id)?;
        let mut world = node.transform.matrix();
        while let Some(p) = node.parent {
            node = &self.nodes[p];
            world = node.transform.matrix() * world;
        }
        Some(world)
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn set_environment(&mut self, environment: Environment) {
        if self.environment != environment {
            self.environment = environment;
            self.touch();
        }
    }

    /// `View3D.camera`. Without one the first camera in the tree, depth
    /// first in declaration order, is used, as Qt does.
    pub fn set_camera(&mut self, camera: Option<NodeId>) {
        if self.camera != camera {
            self.camera = camera;
            self.touch();
        }
    }

    /// Every visible node with its world transform, parents before
    /// children, in declaration order. An invisible node hides its subtree.
    pub fn walk(&self) -> Vec<(NodeId, &Node, Mat4)> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<(NodeId, Mat4)> = self
            .roots
            .iter()
            .rev()
            .map(|r| (*r, Mat4::IDENTITY))
            .collect();
        while let Some((id, parent)) = stack.pop() {
            let node = &*self.nodes[id];
            if !node.visible {
                continue;
            }
            let world = parent * node.transform.matrix();
            stack.extend(node.children.iter().rev().map(|c| (*c, world)));
            out.push((id, node, world));
        }
        out
    }

    /// The camera the view is drawn through, with its world transform.
    pub fn active_camera(&self) -> Option<(Camera, Mat4)> {
        if self.looking.is_some() {
            return self.looking;
        }
        let walked = self.walk();
        let pick = |want: Option<NodeId>| {
            walked
                .iter()
                .find_map(|(id, node, world)| match &node.kind {
                    NodeKind::Camera(c) if want.is_none_or(|w| w == *id) => Some((*c, *world)),
                    _ => None,
                })
        };
        match self.camera {
            Some(c) => pick(Some(c)),
            None => pick(None),
        }
    }
}

impl Scene {
    /// This scene as a reflection probe sees it: what is for mirroring,
    /// with no tone mapping or passes after, over `clear` or over the sky
    /// box where that is what is behind the scene.
    #[cfg(feature = "gpu")]
    pub(crate) fn mirrored(&self, clear: [f32; 4]) -> Scene {
        let mut scene = self.clone();
        for node in scene.nodes.values_mut() {
            if !node.casts_reflections && matches!(node.kind, NodeKind::Model(_)) {
                Arc::make_mut(node).kind = NodeKind::Group;
            }
        }
        let environment = &mut scene.environment;
        environment.capture = Some(clear);
        environment.tonemap = Tonemap::None;
        environment.exposure = 1.0;
        environment.msaa_samples = 1;
        environment.effects.clear();
        environment.post = PostProcess::default();
        environment.ambient_occlusion = None;
        scene
    }

    /// This scene as what reads the picture behind it sees it: the view's
    /// colour as it is written, which Qt reads so; the sky box's light.
    #[cfg(feature = "gpu")]
    pub(crate) fn behind(&self) -> Scene {
        let mut scene = self.clone();
        let environment = &mut scene.environment;
        environment.capture = Some(match environment.background {
            Background::Color(c) => [c.r, c.g, c.b, c.a],
            _ => [0.0; 4],
        });
        environment.behind = true;
        environment.msaa_samples = 1;
        environment.effects.clear();
        environment.post = PostProcess::default();
        environment.ambient_occlusion = None;
        scene
    }

    /// Looks through `camera` placed at `world`, whatever the scene's is.
    #[cfg(feature = "gpu")]
    pub(crate) fn look(&mut self, camera: Camera, world: Mat4) {
        self.looking = Some((camera, world));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_moves_with_its_parent() {
        let mut s = Scene::new();
        let parent = s.add(
            None,
            Node::new(NodeKind::Group).with_transform(Transform {
                position: Vec3::new(100.0, 0.0, 0.0),
                ..Transform::default()
            }),
        );
        let child = s.add(
            Some(parent),
            Node::new(NodeKind::Group).with_transform(Transform {
                position: Vec3::new(0.0, 50.0, 0.0),
                ..Transform::default()
            }),
        );
        let world = s
            .walk()
            .into_iter()
            .find(|(id, ..)| *id == child)
            .unwrap()
            .2;
        assert_eq!(world.translation(), Vec3::new(100.0, 50.0, 0.0));
    }

    #[test]
    fn scenes_built_apart_never_share_a_revision() {
        let mut a = Scene::new();
        let mut b = Scene::new();
        a.add(None, Node::new(NodeKind::Group));
        b.add(None, Node::new(NodeKind::Light(Light::DIRECTIONAL)));
        assert_ne!(a.revision(), b.revision());
        assert!(!a.same_as(&b));
        let mut c = Scene::new();
        c.add(None, Node::new(NodeKind::Group));
        assert!(a.same_as(&c), "the same nodes draw the same");
    }

    #[test]
    fn every_change_is_a_new_revision() {
        let mut s = Scene::new();
        let a = s.add(None, Node::new(NodeKind::Group));
        let r = s.revision();
        s.node_mut(a).unwrap().visible = false;
        assert_ne!(s.revision(), r);
        let r = s.revision();
        s.set_environment(Environment::default());
        assert_eq!(
            s.revision(),
            r,
            "setting what is already there changes nothing"
        );
    }

    #[test]
    fn without_a_camera_set_the_first_one_is_used() {
        let mut s = Scene::new();
        s.add(None, Node::new(NodeKind::Group));
        s.add(None, Node::new(NodeKind::Camera(Camera::PERSPECTIVE)));
        let second = s.add(
            None,
            Node::new(NodeKind::Camera(Camera::Perspective {
                fov_degrees: 30.0,
                clip_near: 1.0,
                fov_horizontal: false,
                clip_far: 100.0,
                frustum_culling: false,
            })),
        );
        assert_eq!(s.active_camera().unwrap().0, Camera::PERSPECTIVE);
        s.set_camera(Some(second));
        assert_ne!(s.active_camera().unwrap().0, Camera::PERSPECTIVE);
        s.remove(second);
        assert_eq!(s.active_camera().unwrap().0, Camera::PERSPECTIVE);
    }

    #[test]
    fn opacity_multiplies_down_the_tree_and_whole_scenes_pay_nothing() {
        let group = || Node::new(NodeKind::Group);
        let mut scene = Scene::new();
        let top = scene.add(None, group());
        let child = scene.add(Some(top), group());
        assert!(scene.faded().is_none());
        scene.node_mut(top).unwrap().opacity = 0.5;
        scene.node_mut(child).unwrap().opacity = 0.5;
        let faded = scene.faded().expect("something fades");
        assert_eq!((faded[top], faded[child]), (0.5, 0.25));
        scene.node_mut(child).unwrap().opacity = f32::NAN;
        assert_eq!(scene.faded().unwrap()[child], 0.0);
        scene.node_mut(child).unwrap().opacity = 7.0;
        assert_eq!(scene.faded().unwrap()[child], 0.5);
    }
}
