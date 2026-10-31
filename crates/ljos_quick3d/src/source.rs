//! The QML engine's objects as the shared mapping reads them.
//!
//! Each [`Prop3d`] is the typed property of the type that declares it; a
//! name several types declare (`clipNear`, `lighting`) is tried on each,
//! and the engine's owner check picks the one the object is. A read the
//! object's type does not have is `None` and untracked, as `Engine::get`
//! is.

use std::sync::Arc;

use ljos::item::ljos_3d::math::{Mat4, Quat, Vec3};
use ljos::item::ljos_3d::qml::{GridLines, Prop3d, SceneSource, Type3d};
use ljos::item::{self, ljos_3d};
use ljos::ljos_engine::Provenance;
use ljos::prelude::*;

use crate::helpers::ExtendedSceneEnvironment as X;
use crate::{
    Camera, CustomMaterial, DefaultMaterial, DirectionalLight, Effect, Geometry, Instancing, Joint,
    Light, Material, Model, Node, Object3D, OrthographicCamera, PerspectiveCamera, PointLight,
    PrincipledMaterial, RuntimeLoader, SceneEnvironment, Skeleton, Skin, SpotLight, Texture,
    View3D,
};

/// The engine `e`'s objects, read by the mapping. Reads are tracked: a
/// paint that builds its scene through this is painted again when one of
/// them changes.
#[derive(Clone, Copy)]
pub struct EngineSource<'e> {
    pub e: &'e Engine,
}

impl std::fmt::Debug for EngineSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineSource").finish_non_exhaustive()
    }
}

impl<'e> EngineSource<'e> {
    pub fn new(e: &'e Engine) -> Self {
        EngineSource { e }
    }

    fn first<T: QmlValue>(&self, o: ObjectId, props: &[Prop<T>]) -> Option<T> {
        props.iter().find_map(|&p| self.e.get(o, p).ok())
    }

    /// The index of `p` on `o`'s type, for a read by index.
    fn index(&self, o: ObjectId, p: Prop3d) -> Option<PropIndex> {
        fn on<T: QmlValue>(e: &Engine, o: ObjectId, props: &[Prop<T>]) -> Option<PropIndex> {
            props
                .iter()
                .find(|&&q| e.get(o, q).is_ok())
                .map(|q| q.index)
        }
        let e = self.e;
        on(e, o, numbers(p))
            .or_else(|| on(e, o, ints(p)))
            .or_else(|| on(e, o, bools(p)))
            .or_else(|| on(e, o, colors(p)))
            .or_else(|| on(e, o, vectors(p)))
            .or_else(|| on(e, o, objects(p)))
    }

    /// The live objects of the list `p` on `o`, keeping a gone one's place
    /// as `None` when `holes`.
    fn list(&self, o: ObjectId, p: Prop3d, holes: bool) -> Vec<Option<ObjectId>> {
        let e = self.e;
        if p == Prop3d::Data && e.is::<View3D>(o) {
            return item::resources(e, o).into_iter().map(Some).collect();
        }
        let index = match p {
            Prop3d::Data if e.is::<Object3D>(o) => Object3D::DATA.index,
            Prop3d::Materials if e.is::<Model>(o) => Model::MATERIALS.index,
            Prop3d::Effects if e.is::<SceneEnvironment>(o) => SceneEnvironment::EFFECTS.index,
            Prop3d::Joints if e.is::<Skin>(o) => Skin::JOINTS.index,
            _ => return Vec::new(),
        };
        match e.read(o, index) {
            Value::List(l) => l
                .iter()
                .map(|v| v.as_object().filter(|x| e.is_alive(*x)))
                .filter(|x| holes || x.is_some())
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl SceneSource for EngineSource<'_> {
    type Object = ObjectId;

    fn is(&self, o: ObjectId, ty: Type3d) -> bool {
        let e = self.e;
        match ty {
            Type3d::View3D => e.is::<View3D>(o),
            Type3d::Object3D => e.is::<Object3D>(o),
            Type3d::Node => e.is::<Node>(o),
            Type3d::Model => e.is::<Model>(o),
            Type3d::Camera => e.is::<Camera>(o),
            Type3d::PerspectiveCamera => e.is::<PerspectiveCamera>(o),
            Type3d::OrthographicCamera => e.is::<OrthographicCamera>(o),
            Type3d::Light => e.is::<Light>(o),
            Type3d::DirectionalLight => e.is::<DirectionalLight>(o),
            Type3d::PointLight => e.is::<PointLight>(o),
            Type3d::SpotLight => e.is::<SpotLight>(o),
            Type3d::Material => e.is::<Material>(o),
            Type3d::DefaultMaterial => e.is::<DefaultMaterial>(o),
            Type3d::PrincipledMaterial => e.is::<PrincipledMaterial>(o),
            Type3d::CustomMaterial => e.is::<CustomMaterial>(o),
            Type3d::Texture => e.is::<Texture>(o),
            Type3d::Geometry => e.is::<Geometry>(o),
            Type3d::SceneEnvironment => e.is::<SceneEnvironment>(o),
            Type3d::ExtendedSceneEnvironment => e.is::<X>(o),
            Type3d::Effect => e.is::<Effect>(o),
            Type3d::Instancing => e.is::<Instancing>(o),
            Type3d::Skin => e.is::<Skin>(o),
            Type3d::Skeleton => e.is::<Skeleton>(o),
            Type3d::Joint => e.is::<Joint>(o),
            Type3d::RuntimeLoader => e.is::<RuntimeLoader>(o),
            Type3d::InfiniteGrid => e.is::<crate::helpers::InfiniteGrid>(o),
        }
    }

    fn number(&self, o: ObjectId, p: Prop3d) -> Option<f64> {
        self.first(o, numbers(p))
    }

    fn int(&self, o: ObjectId, p: Prop3d) -> Option<i32> {
        self.first(o, ints(p))
    }

    fn boolean(&self, o: ObjectId, p: Prop3d) -> Option<bool> {
        self.first(o, bools(p))
    }

    fn color(&self, o: ObjectId, p: Prop3d) -> Option<[f32; 4]> {
        self.first(o, colors(p))
            .map(|c| c.to_rgba_f().map(|x| x as f32))
    }

    fn vector3d(&self, o: ObjectId, p: Prop3d) -> Option<Vec3> {
        self.first(o, vectors(p)).map(crate::node::vec3)
    }

    fn quaternion(&self, o: ObjectId, p: Prop3d) -> Option<Quat> {
        match p {
            Prop3d::Rotation => self.e.get(o, Node::ROTATION).ok().map(|q| Quat {
                x: q.x,
                y: q.y,
                z: q.z,
                w: q.scalar,
            }),
            _ => None,
        }
    }

    fn url(&self, o: ObjectId, p: Prop3d) -> Option<String> {
        match p {
            Prop3d::Source => self
                .e
                .get(o, Model::SOURCE)
                .ok()
                .map(|u| u.0.as_str().to_owned()),
            _ => None,
        }
    }

    fn object(&self, o: ObjectId, p: Prop3d) -> Option<ObjectId> {
        self.first(o, objects(p)).flatten()
    }

    fn objects(&self, o: ObjectId, p: Prop3d) -> Vec<ObjectId> {
        self.list(o, p, false).into_iter().flatten().collect()
    }

    fn object_slots(&self, o: ObjectId, p: Prop3d) -> Vec<Option<ObjectId>> {
        self.list(o, p, true)
    }

    fn authored(&self, o: ObjectId, p: Prop3d) -> bool {
        self.index(o, p).is_some_and(|index| {
            matches!(
                self.e.provenance(o, index),
                Some(Provenance::Assigned | Provenance::Bound)
            )
        })
    }

    fn matrices(&self, o: ObjectId, p: Prop3d) -> Vec<Mat4> {
        let e = self.e;
        let index = match p {
            Prop3d::InverseBindPoses if e.is::<Skin>(o) => Skin::INVERSE_BIND_POSES.index,
            Prop3d::InverseBindPoses if e.is::<Model>(o) => Model::INVERSE_BIND_POSES.index,
            _ => return Vec::new(),
        };
        match &e.read(o, index) {
            Value::List(l) => l.iter().map(|m| crate::skin::matrix(e, m)).collect(),
            _ => Vec::new(),
        }
    }

    fn texture_image(&self, view: ObjectId, texture: ObjectId) -> Option<ljos_3d::TextureImage> {
        crate::texture::image_of(self.e, view, texture)
    }

    fn geometry(&self, g: ObjectId) -> Option<Arc<ljos_3d::CustomGeometry>> {
        // The revision is what a commit notifies.
        item::prop(self.e, g, Geometry::REVISION);
        self.e.with(g, Geometry::committed).ok().flatten()
    }

    fn custom_material(&self, view: ObjectId, m: ObjectId) -> Option<ljos_3d::CustomMaterial> {
        crate::custom::material(self.e, view, m)
    }

    fn effect(&self, view: ObjectId, fx: ObjectId) -> Option<ljos_3d::Effect> {
        crate::custom::effect(self.e, view, fx)
    }

    fn instancing(&self, table: ObjectId) -> Option<ljos_3d::Instancing> {
        crate::instancing::instancing(self.e, table)
    }

    #[cfg(feature = "gltf")]
    fn loaded(&self, loader: ObjectId) -> Option<ljos_3d::qml::Loaded<ObjectId>> {
        let asset = crate::runtime_loader::asset(self.e, loader)?;
        Some(ljos_3d::qml::Loaded {
            asset,
            models: crate::runtime_loader::models(self.e, loader),
        })
    }

    fn items_quad(&self, view: ObjectId, node: ObjectId) -> Option<(ljos_3d::Node, ObjectId)> {
        let holder = crate::items::holder_of(self.e, node)?;
        Some((crate::items::quad(self.e, view, holder)?, holder))
    }

    fn infinite_grid(&self, grid: ObjectId) -> Option<GridLines> {
        let (made, axes) = crate::helpers::infinite_grid(self.e, grid)?;
        Some(GridLines {
            grid: made.grid.clone(),
            x_axis: made.x_axis.clone(),
            z_axis: made.z_axis.clone(),
            axes,
        })
    }
}

fn numbers(p: Prop3d) -> &'static [Prop<f64>] {
    use Prop3d as P;
    match p {
        P::X => &[Node::X],
        P::Y => &[Node::Y],
        P::Z => &[Node::Z],
        P::Opacity => &[Node::OPACITY, PrincipledMaterial::OPACITY],
        P::FieldOfView => &[PerspectiveCamera::FIELD_OF_VIEW],
        P::ClipNear => &[PerspectiveCamera::CLIP_NEAR, OrthographicCamera::CLIP_NEAR],
        P::ClipFar => &[PerspectiveCamera::CLIP_FAR, OrthographicCamera::CLIP_FAR],
        P::HorizontalMagnification => &[OrthographicCamera::HORIZONTAL_MAGNIFICATION],
        P::VerticalMagnification => &[OrthographicCamera::VERTICAL_MAGNIFICATION],
        P::Brightness => &[Light::BRIGHTNESS],
        P::ShadowBias => &[Light::SHADOW_BIAS],
        P::ShadowFactor => &[Light::SHADOW_FACTOR],
        P::ShadowMapFar => &[Light::SHADOW_MAP_FAR],
        P::PcfFactor => &[Light::PCF_FACTOR],
        P::CsmSplit1 => &[DirectionalLight::CSM_SPLIT1],
        P::CsmSplit2 => &[DirectionalLight::CSM_SPLIT2],
        P::CsmSplit3 => &[DirectionalLight::CSM_SPLIT3],
        P::CsmBlendRatio => &[DirectionalLight::CSM_BLEND_RATIO],
        P::ConstantFade => &[PointLight::CONSTANT_FADE, SpotLight::CONSTANT_FADE],
        P::LinearFade => &[PointLight::LINEAR_FADE, SpotLight::LINEAR_FADE],
        P::QuadraticFade => &[PointLight::QUADRATIC_FADE, SpotLight::QUADRATIC_FADE],
        P::ConeAngle => &[SpotLight::CONE_ANGLE],
        P::InnerConeAngle => &[SpotLight::INNER_CONE_ANGLE],
        P::SpecularAmount => &[DefaultMaterial::SPECULAR_AMOUNT],
        P::SpecularRoughness => &[DefaultMaterial::SPECULAR_ROUGHNESS],
        P::Metalness => &[PrincipledMaterial::METALNESS],
        P::Roughness => &[PrincipledMaterial::ROUGHNESS],
        P::AlphaCutoff => &[PrincipledMaterial::ALPHA_CUTOFF],
        P::NormalStrength => &[PrincipledMaterial::NORMAL_STRENGTH],
        P::OcclusionAmount => &[PrincipledMaterial::OCCLUSION_AMOUNT],
        P::ScaleU => &[Texture::SCALE_U],
        P::ScaleV => &[Texture::SCALE_V],
        P::PositionU => &[Texture::POSITION_U],
        P::PositionV => &[Texture::POSITION_V],
        P::RotationUV => &[Texture::ROTATION_UV],
        P::PivotU => &[Texture::PIVOT_U],
        P::PivotV => &[Texture::PIVOT_V],
        P::ProbeExposure => &[SceneEnvironment::PROBE_EXPOSURE],
        P::ProbeHorizon => &[SceneEnvironment::PROBE_HORIZON],
        P::SkyboxBlurAmount => &[SceneEnvironment::SKYBOX_BLUR_AMOUNT],
        P::AoStrength => &[SceneEnvironment::AO_STRENGTH],
        P::AoDistance => &[SceneEnvironment::AO_DISTANCE],
        P::AoSoftness => &[SceneEnvironment::AO_SOFTNESS],
        P::AoBias => &[SceneEnvironment::AO_BIAS],
        P::Exposure => &[X::EXPOSURE],
        P::WhitePoint => &[X::WHITE_POINT],
        P::SharpnessAmount => &[X::SHARPNESS_AMOUNT],
        P::AdjustmentBrightness => &[X::ADJUSTMENT_BRIGHTNESS],
        P::AdjustmentContrast => &[X::ADJUSTMENT_CONTRAST],
        P::AdjustmentSaturation => &[X::ADJUSTMENT_SATURATION],
        P::DepthOfFieldFocusDistance => &[X::DEPTH_OF_FIELD_FOCUS_DISTANCE],
        P::DepthOfFieldFocusRange => &[X::DEPTH_OF_FIELD_FOCUS_RANGE],
        P::DepthOfFieldBlurAmount => &[X::DEPTH_OF_FIELD_BLUR_AMOUNT],
        P::GlowStrength => &[X::GLOW_STRENGTH],
        P::GlowIntensity => &[X::GLOW_INTENSITY],
        P::GlowBloom => &[X::GLOW_BLOOM],
        P::GlowHdrMaximumValue => &[X::GLOW_HDR_MAXIMUM_VALUE],
        P::GlowHdrMinimumValue => &[X::GLOW_HDR_MINIMUM_VALUE],
        P::GlowHdrScale => &[X::GLOW_HDR_SCALE],
        P::VignetteStrength => &[X::VIGNETTE_STRENGTH],
        P::VignetteRadius => &[X::VIGNETTE_RADIUS],
        P::LutSize => &[X::LUT_SIZE],
        P::LutFilterAlpha => &[X::LUT_FILTER_ALPHA],
        P::LensFlareBloomScale => &[X::LENS_FLARE_BLOOM_SCALE],
        P::LensFlareBloomBias => &[X::LENS_FLARE_BLOOM_BIAS],
        P::LensFlareGhostDispersal => &[X::LENS_FLARE_GHOST_DISPERSAL],
        P::LensFlareHaloWidth => &[X::LENS_FLARE_HALO_WIDTH],
        P::LensFlareStretchToAspect => &[X::LENS_FLARE_STRETCH_TO_ASPECT],
        P::LensFlareDistortion => &[X::LENS_FLARE_DISTORTION],
        P::LensFlareBlurAmount => &[X::LENS_FLARE_BLUR_AMOUNT],
        P::SsrThickness => &[X::SSR_THICKNESS],
        P::SsgiBufferSizeFactor => &[X::SSGI_BUFFER_SIZE_FACTOR],
        P::SsgiHitThickness => &[X::SSGI_HIT_THICKNESS],
        P::SsgiIndirectLightBoost => &[X::SSGI_INDIRECT_LIGHT_BOOST],
        P::SsgiSampleRadius => &[X::SSGI_SAMPLE_RADIUS],
        P::SsgiSimulatedBounceFactor => &[X::SSGI_SIMULATED_BOUNCE_FACTOR],
        _ => &[],
    }
}

fn ints(p: Prop3d) -> &'static [Prop<i32>] {
    use Prop3d as P;
    match p {
        P::FieldOfViewOrientation => &[PerspectiveCamera::FIELD_OF_VIEW_ORIENTATION],
        P::ShadowMapQuality => &[Light::SHADOW_MAP_QUALITY],
        P::SoftShadowQuality => &[Light::SOFT_SHADOW_QUALITY],
        P::CsmNumSplits => &[DirectionalLight::CSM_NUM_SPLITS],
        P::CullMode => &[Material::CULL_MODE],
        P::Lighting => &[DefaultMaterial::LIGHTING, PrincipledMaterial::LIGHTING],
        P::MetalnessChannel => &[PrincipledMaterial::METALNESS_CHANNEL],
        P::RoughnessChannel => &[PrincipledMaterial::ROUGHNESS_CHANNEL],
        P::OcclusionChannel => &[PrincipledMaterial::OCCLUSION_CHANNEL],
        P::OpacityChannel => &[PrincipledMaterial::OPACITY_CHANNEL],
        P::AlphaMode => &[PrincipledMaterial::ALPHA_MODE],
        P::IndexUV => &[Texture::INDEX_UV],
        P::TilingModeHorizontal => &[Texture::TILING_MODE_HORIZONTAL],
        P::TilingModeVertical => &[Texture::TILING_MODE_VERTICAL],
        P::MinFilter => &[Texture::MIN_FILTER],
        P::MagFilter => &[Texture::MAG_FILTER],
        P::MipFilter => &[Texture::MIP_FILTER],
        P::BackgroundMode => &[SceneEnvironment::BACKGROUND_MODE],
        P::AntialiasingMode => &[SceneEnvironment::ANTIALIASING_MODE],
        P::AntialiasingQuality => &[SceneEnvironment::ANTIALIASING_QUALITY],
        P::TonemapMode => &[SceneEnvironment::TONEMAP_MODE],
        P::AoSampleRate => &[SceneEnvironment::AO_SAMPLE_RATE],
        P::GlowLevel => &[X::GLOW_LEVEL],
        P::GlowBlendMode => &[X::GLOW_BLEND_MODE],
        P::LensFlareGhostCount => &[X::LENS_FLARE_GHOST_COUNT],
        P::SsrMaxSteps => &[X::SSR_MAX_STEPS],
        P::SsgiSampleCount => &[X::SSGI_SAMPLE_COUNT],
        P::SsgiSliceCount => &[X::SSGI_SLICE_COUNT],
        P::Index => &[Joint::INDEX],
        _ => &[],
    }
}

fn bools(p: Prop3d) -> &'static [Prop<bool>] {
    use Prop3d as P;
    match p {
        P::Visible => &[Node::VISIBLE],
        P::CastsShadows => &[Model::CASTS_SHADOWS],
        P::ReceivesShadows => &[Model::RECEIVES_SHADOWS],
        P::FrustumCullingEnabled => &[Camera::FRUSTUM_CULLING_ENABLED],
        P::CastsShadow => &[Light::CASTS_SHADOW],
        P::LockShadowmapTexels => &[DirectionalLight::LOCK_SHADOWMAP_TEXELS],
        P::FlipU => &[Texture::FLIP_U],
        P::FlipV => &[Texture::FLIP_V],
        P::GenerateMipmaps => &[Texture::GENERATE_MIPMAPS],
        P::AoEnabled => &[SceneEnvironment::AO_ENABLED],
        P::AoDither => &[SceneEnvironment::AO_DITHER],
        P::FxaaEnabled => &[X::FXAA_ENABLED],
        P::DitheringEnabled => &[X::DITHERING_ENABLED],
        P::ColorAdjustmentsEnabled => &[X::COLOR_ADJUSTMENTS_ENABLED],
        P::DepthOfFieldEnabled => &[X::DEPTH_OF_FIELD_ENABLED],
        P::GlowEnabled => &[X::GLOW_ENABLED],
        P::GlowQualityHigh => &[X::GLOW_QUALITY_HIGH],
        P::VignetteEnabled => &[X::VIGNETTE_ENABLED],
        P::LutEnabled => &[X::LUT_ENABLED],
        P::LensFlareEnabled => &[X::LENS_FLARE_ENABLED],
        P::LensFlareApplyDirtTexture => &[X::LENS_FLARE_APPLY_DIRT_TEXTURE],
        P::LensFlareApplyStarburstTexture => &[X::LENS_FLARE_APPLY_STARBURST_TEXTURE],
        P::SsrEnabled => &[X::SSR_ENABLED],
        P::SsgiEnabled => &[X::SSGI_ENABLED],
        P::SsgiIndirectLightEnabled => &[X::SSGI_INDIRECT_LIGHT_ENABLED],
        P::SsgiSimulatedBounceEnabled => &[X::SSGI_SIMULATED_BOUNCE_ENABLED],
        _ => &[],
    }
}

fn colors(p: Prop3d) -> &'static [Prop<Color>] {
    use Prop3d as P;
    match p {
        P::Color => &[Light::COLOR],
        P::AmbientColor => &[Light::AMBIENT_COLOR],
        P::DiffuseColor => &[DefaultMaterial::DIFFUSE_COLOR],
        P::BaseColor => &[PrincipledMaterial::BASE_COLOR],
        P::ClearColor => &[SceneEnvironment::CLEAR_COLOR],
        P::VignetteColor => &[X::VIGNETTE_COLOR],
        _ => &[],
    }
}

fn vectors(p: Prop3d) -> &'static [Prop<Vector3d>] {
    use Prop3d as P;
    match p {
        P::Position => &[Node::POSITION],
        P::EulerRotation => &[Node::EULER_ROTATION],
        P::Scale => &[Node::SCALE],
        P::EmissiveFactor => &[PrincipledMaterial::EMISSIVE_FACTOR],
        P::ProbeOrientation => &[SceneEnvironment::PROBE_ORIENTATION],
        P::LensFlareCameraDirection => &[X::LENS_FLARE_CAMERA_DIRECTION],
        _ => &[],
    }
}

fn objects(p: Prop3d) -> &'static [Prop<Option<ObjectId>>] {
    use Prop3d as P;
    match p {
        P::Environment => &[View3D::ENVIRONMENT],
        P::Camera => &[View3D::CAMERA],
        P::ImportScene => &[View3D::IMPORT_SCENE],
        P::Geometry => &[Model::GEOMETRY],
        P::Instancing => &[Model::INSTANCING, RuntimeLoader::INSTANCING],
        P::InstanceRoot => &[Model::INSTANCE_ROOT],
        P::Skin => &[Model::SKIN],
        P::Skeleton => &[Model::SKELETON],
        P::SkeletonRoot => &[Joint::SKELETON_ROOT],
        P::DiffuseMap => &[DefaultMaterial::DIFFUSE_MAP],
        P::BaseColorMap => &[PrincipledMaterial::BASE_COLOR_MAP],
        P::MetalnessMap => &[PrincipledMaterial::METALNESS_MAP],
        P::RoughnessMap => &[PrincipledMaterial::ROUGHNESS_MAP],
        P::NormalMap => &[PrincipledMaterial::NORMAL_MAP],
        P::EmissiveMap => &[PrincipledMaterial::EMISSIVE_MAP],
        P::OcclusionMap => &[PrincipledMaterial::OCCLUSION_MAP],
        P::OpacityMap => &[PrincipledMaterial::OPACITY_MAP],
        P::LightProbe => &[SceneEnvironment::LIGHT_PROBE],
        P::LutTexture => &[X::LUT_TEXTURE],
        P::LensFlareLensColorTexture => &[X::LENS_FLARE_LENS_COLOR_TEXTURE],
        P::LensFlareLensDirtTexture => &[X::LENS_FLARE_LENS_DIRT_TEXTURE],
        P::LensFlareLensStarburstTexture => &[X::LENS_FLARE_LENS_STARBURST_TEXTURE],
        _ => &[],
    }
}
