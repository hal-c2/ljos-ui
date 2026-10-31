//! The QtQuick3D types and members the mapping reads, by their QML names.

/// A QtQuick3D type the mapping asks about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type3d {
    View3D,
    Object3D,
    Node,
    Model,
    Camera,
    PerspectiveCamera,
    OrthographicCamera,
    Light,
    DirectionalLight,
    PointLight,
    SpotLight,
    Material,
    DefaultMaterial,
    PrincipledMaterial,
    CustomMaterial,
    Texture,
    Geometry,
    SceneEnvironment,
    ExtendedSceneEnvironment,
    Effect,
    Instancing,
    Skin,
    Skeleton,
    Joint,
    RuntimeLoader,
    InfiniteGrid,
}

impl Type3d {
    pub fn qml_name(self) -> &'static str {
        match self {
            Type3d::View3D => "View3D",
            Type3d::Object3D => "Object3D",
            Type3d::Node => "Node",
            Type3d::Model => "Model",
            Type3d::Camera => "Camera",
            Type3d::PerspectiveCamera => "PerspectiveCamera",
            Type3d::OrthographicCamera => "OrthographicCamera",
            Type3d::Light => "Light",
            Type3d::DirectionalLight => "DirectionalLight",
            Type3d::PointLight => "PointLight",
            Type3d::SpotLight => "SpotLight",
            Type3d::Material => "Material",
            Type3d::DefaultMaterial => "DefaultMaterial",
            Type3d::PrincipledMaterial => "PrincipledMaterial",
            Type3d::CustomMaterial => "CustomMaterial",
            Type3d::Texture => "Texture",
            Type3d::Geometry => "Geometry",
            Type3d::SceneEnvironment => "SceneEnvironment",
            Type3d::ExtendedSceneEnvironment => "ExtendedSceneEnvironment",
            Type3d::Effect => "Effect",
            Type3d::Instancing => "Instancing",
            Type3d::Skin => "Skin",
            Type3d::Skeleton => "Skeleton",
            Type3d::Joint => "Joint",
            Type3d::RuntimeLoader => "RuntimeLoader",
            Type3d::InfiniteGrid => "InfiniteGrid",
        }
    }

    /// The type this one derives from; `None` for `Object3D` and for
    /// `View3D`, which is a 2D item.
    pub fn base(self) -> Option<Type3d> {
        use Type3d as T;
        match self {
            T::View3D | T::Object3D => None,
            T::Node
            | T::Material
            | T::Texture
            | T::Geometry
            | T::SceneEnvironment
            | T::Effect
            | T::Instancing
            | T::Skin
            | T::InfiniteGrid => Some(T::Object3D),
            T::Model | T::Camera | T::Light | T::Skeleton | T::Joint | T::RuntimeLoader => {
                Some(T::Node)
            }
            T::PerspectiveCamera | T::OrthographicCamera => Some(T::Camera),
            T::DirectionalLight | T::PointLight | T::SpotLight => Some(T::Light),
            T::DefaultMaterial | T::PrincipledMaterial | T::CustomMaterial => Some(T::Material),
            T::ExtendedSceneEnvironment => Some(T::SceneEnvironment),
        }
    }

    /// Whether an object of this type is a `ty`: this type or one it
    /// derives from. What a graph that knows only each object's own type
    /// answers [`super::SceneSource::is`] with.
    pub fn is_a(self, ty: Type3d) -> bool {
        let mut t = Some(self);
        while let Some(at) = t {
            if at == ty {
                return true;
            }
            t = at.base();
        }
        false
    }
}

macro_rules! props {
    ($($(#[$m:meta])* $v:ident = $name:literal,)*) => {
        /// A member the mapping reads, named as QML names it. A name
        /// several types declare (`clipNear`, `lighting`, `opacity`) is
        /// one member: the object says which type's it is.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Prop3d {
            $($(#[$m])* $v,)*
        }

        impl Prop3d {
            pub const ALL: &'static [Prop3d] = &[$(Prop3d::$v,)*];

            pub fn qml_name(self) -> &'static str {
                match self {
                    $(Prop3d::$v => $name,)*
                }
            }

            pub fn from_qml_name(name: &str) -> Option<Prop3d> {
                match name {
                    $($name => Some(Prop3d::$v),)*
                    _ => None,
                }
            }
        }
    };
}

props! {
    // View3D
    Environment = "environment",
    Camera = "camera",
    ImportScene = "importScene",
    /// A node's children; a View3D's 3D nodes; the environment's helpers.
    Data = "data",
    // Node
    X = "x",
    Y = "y",
    Z = "z",
    Position = "position",
    Rotation = "rotation",
    EulerRotation = "eulerRotation",
    Scale = "scale",
    Opacity = "opacity",
    Visible = "visible",
    // Model
    Source = "source",
    Materials = "materials",
    Geometry = "geometry",
    CastsShadows = "castsShadows",
    ReceivesShadows = "receivesShadows",
    /// `Model.instancing` and `RuntimeLoader.instancing`.
    Instancing = "instancing",
    InstanceRoot = "instanceRoot",
    Skin = "skin",
    Skeleton = "skeleton",
    /// `Model.inverseBindPoses` and `Skin.inverseBindPoses`.
    InverseBindPoses = "inverseBindPoses",
    // Skin, Joint
    Joints = "joints",
    Index = "index",
    SkeletonRoot = "skeletonRoot",
    // Camera
    FrustumCullingEnabled = "frustumCullingEnabled",
    FieldOfView = "fieldOfView",
    FieldOfViewOrientation = "fieldOfViewOrientation",
    ClipNear = "clipNear",
    ClipFar = "clipFar",
    HorizontalMagnification = "horizontalMagnification",
    VerticalMagnification = "verticalMagnification",
    // Light
    Color = "color",
    AmbientColor = "ambientColor",
    Brightness = "brightness",
    CastsShadow = "castsShadow",
    ShadowMapQuality = "shadowMapQuality",
    SoftShadowQuality = "softShadowQuality",
    ShadowBias = "shadowBias",
    ShadowFactor = "shadowFactor",
    ShadowMapFar = "shadowMapFar",
    PcfFactor = "pcfFactor",
    CsmNumSplits = "csmNumSplits",
    CsmSplit1 = "csmSplit1",
    CsmSplit2 = "csmSplit2",
    CsmSplit3 = "csmSplit3",
    CsmBlendRatio = "csmBlendRatio",
    LockShadowmapTexels = "lockShadowmapTexels",
    ConstantFade = "constantFade",
    LinearFade = "linearFade",
    QuadraticFade = "quadraticFade",
    ConeAngle = "coneAngle",
    InnerConeAngle = "innerConeAngle",
    // Materials
    CullMode = "cullMode",
    Lighting = "lighting",
    DiffuseColor = "diffuseColor",
    DiffuseMap = "diffuseMap",
    SpecularAmount = "specularAmount",
    SpecularRoughness = "specularRoughness",
    BaseColor = "baseColor",
    BaseColorMap = "baseColorMap",
    Metalness = "metalness",
    MetalnessMap = "metalnessMap",
    MetalnessChannel = "metalnessChannel",
    Roughness = "roughness",
    RoughnessMap = "roughnessMap",
    RoughnessChannel = "roughnessChannel",
    EmissiveFactor = "emissiveFactor",
    EmissiveMap = "emissiveMap",
    NormalMap = "normalMap",
    NormalStrength = "normalStrength",
    OcclusionMap = "occlusionMap",
    OcclusionChannel = "occlusionChannel",
    OcclusionAmount = "occlusionAmount",
    OpacityMap = "opacityMap",
    OpacityChannel = "opacityChannel",
    AlphaMode = "alphaMode",
    AlphaCutoff = "alphaCutoff",
    // Texture
    ScaleU = "scaleU",
    ScaleV = "scaleV",
    PositionU = "positionU",
    PositionV = "positionV",
    RotationUV = "rotationUV",
    PivotU = "pivotU",
    PivotV = "pivotV",
    FlipU = "flipU",
    FlipV = "flipV",
    IndexUV = "indexUV",
    TilingModeHorizontal = "tilingModeHorizontal",
    TilingModeVertical = "tilingModeVertical",
    MinFilter = "minFilter",
    MagFilter = "magFilter",
    MipFilter = "mipFilter",
    GenerateMipmaps = "generateMipmaps",
    // SceneEnvironment
    BackgroundMode = "backgroundMode",
    ClearColor = "clearColor",
    AntialiasingMode = "antialiasingMode",
    AntialiasingQuality = "antialiasingQuality",
    LightProbe = "lightProbe",
    ProbeExposure = "probeExposure",
    ProbeHorizon = "probeHorizon",
    ProbeOrientation = "probeOrientation",
    SkyboxBlurAmount = "skyboxBlurAmount",
    TonemapMode = "tonemapMode",
    Effects = "effects",
    AoEnabled = "aoEnabled",
    AoStrength = "aoStrength",
    AoDistance = "aoDistance",
    AoSoftness = "aoSoftness",
    AoSampleRate = "aoSampleRate",
    AoBias = "aoBias",
    AoDither = "aoDither",
    // ExtendedSceneEnvironment
    Exposure = "exposure",
    WhitePoint = "whitePoint",
    SharpnessAmount = "sharpnessAmount",
    FxaaEnabled = "fxaaEnabled",
    DitheringEnabled = "ditheringEnabled",
    ColorAdjustmentsEnabled = "colorAdjustmentsEnabled",
    AdjustmentBrightness = "adjustmentBrightness",
    AdjustmentContrast = "adjustmentContrast",
    AdjustmentSaturation = "adjustmentSaturation",
    DepthOfFieldEnabled = "depthOfFieldEnabled",
    DepthOfFieldFocusDistance = "depthOfFieldFocusDistance",
    DepthOfFieldFocusRange = "depthOfFieldFocusRange",
    DepthOfFieldBlurAmount = "depthOfFieldBlurAmount",
    GlowEnabled = "glowEnabled",
    GlowQualityHigh = "glowQualityHigh",
    GlowStrength = "glowStrength",
    GlowIntensity = "glowIntensity",
    GlowBloom = "glowBloom",
    GlowHdrMaximumValue = "glowHDRMaximumValue",
    GlowHdrMinimumValue = "glowHDRMinimumValue",
    GlowHdrScale = "glowHDRScale",
    GlowLevel = "glowLevel",
    GlowBlendMode = "glowBlendMode",
    VignetteEnabled = "vignetteEnabled",
    VignetteStrength = "vignetteStrength",
    VignetteColor = "vignetteColor",
    VignetteRadius = "vignetteRadius",
    LutEnabled = "lutEnabled",
    LutSize = "lutSize",
    LutFilterAlpha = "lutFilterAlpha",
    LutTexture = "lutTexture",
}
