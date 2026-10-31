//! The walk from a View3D's objects to a [`Scene`].

use std::collections::HashMap;
use std::sync::Arc;

use super::enums::{
    alpha, antialiasing, background, channel, cull, filter, fov_orientation, lighting, shadow_map,
    soft_shadow, tiling, tonemap,
};
use super::{GridLines, Prop3d as P, SceneSource, Type3d as T, defaults as d};
use crate::custom::{
    Adjustments, AmbientOcclusion, DepthOfField, Glow, GlowBlend, Lut, PostProcess, Vignette,
};
use crate::geometry::CustomGeometry;
use crate::instancing::{MAX_JOINTS, Skin};
use crate::math::{Quat, Vec3};
use crate::scene::{
    AlphaMode, Background, Camera, Cascades, Color, CullMode, DefaultMaterial, Environment, Fade,
    Light, LightProbe, Lighting, Material, MeshSource, Model, Node, NodeId, NodeKind, Primitive,
    PrincipledMaps, PrincipledMaterial, Scene, Shadow, Tonemap, Transform,
};
use crate::texture::{Channel, Filter, Sampling, Texture, Tiling, UvTransform};

/// A view's scene, and which object each of its nodes is.
#[derive(Debug)]
pub struct Built<O> {
    pub scene: Scene,
    /// What a pick needs to say which object it hit.
    pub ids: HashMap<O, NodeId>,
    /// The models a `RuntimeLoader` made of its file's meshes, a scene
    /// node per mesh primitive: what a pick names when it hits one.
    pub loaded: Vec<(NodeId, O)>,
}

/// The scene of the View3D `view`, read now from `src`.
pub fn build<S: SceneSource>(src: &S, view: S::Object) -> Built<S::Object> {
    let mut walk = Walk {
        s: src,
        view,
        scene: Scene::new(),
        ids: HashMap::new(),
        loaded: Vec::new(),
        later: Vec::new(),
    };
    for o in src.objects(view, P::Data) {
        walk.node(None, o);
    }
    if let Some(imported) = src.object(view, P::ImportScene) {
        walk.node(None, imported);
    }
    walk.finish();
    let env_object = src.object(view, P::Environment);
    if let Some(env) = env_object.filter(|&env| src.is(env, T::SceneEnvironment)) {
        // Declared among the environment's data, drawn in the scene.
        for grid in src.objects(env, P::Data) {
            if let Some(lines) = src.infinite_grid(grid) {
                walk.infinite_grid(&lines);
            }
        }
    }
    let env = env_object.map_or_else(Environment::default, |env| environment(src, view, env));
    walk.scene.set_environment(env);
    // A camera that is not in the scene leaves the choice to the scene:
    // its first camera.
    let camera = src
        .object(view, P::Camera)
        .and_then(|c| walk.ids.get(&c).copied());
    walk.scene.set_camera(camera);
    Built {
        scene: walk.scene,
        ids: walk.ids,
        loaded: walk.loaded,
    }
}

fn num<S: SceneSource>(s: &S, o: S::Object, p: P, default: f64) -> f32 {
    s.number(o, p).unwrap_or(default) as f32
}

fn int<S: SceneSource>(s: &S, o: S::Object, p: P, default: i32) -> i32 {
    s.int(o, p).unwrap_or(default)
}

fn on<S: SceneSource>(s: &S, o: S::Object, p: P, default: bool) -> bool {
    s.boolean(o, p).unwrap_or(default)
}

fn color<S: SceneSource>(s: &S, o: S::Object, p: P, default: [u8; 4]) -> Color {
    let [r, g, b, a] = s.color(o, p).unwrap_or_else(|| srgb8(default));
    Color { r, g, b, a }
}

/// A colour's bytes as the channels [`SceneSource::color`] gives, divided
/// in `f64` as the QML engine's colours are: a default and the same colour
/// written out come to the same bits.
pub fn srgb8(bytes: [u8; 4]) -> [f32; 4] {
    bytes.map(|c| (f64::from(c) / 255.0) as f32)
}

fn vec3<S: SceneSource>(s: &S, o: S::Object, p: P, default: [f32; 3]) -> Vec3 {
    s.vector3d(o, p)
        .unwrap_or(Vec3::new(default[0], default[1], default[2]))
}

/// A quaternion of unit length; one of no length is no turn.
fn unit(q: Quat) -> Quat {
    let len = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
    if len > f32::EPSILON {
        Quat {
            x: q.x / len,
            y: q.y / len,
            z: q.z / len,
            w: q.w / len,
        }
    } else {
        Quat::IDENTITY
    }
}

/// The camera `obj` is, as the renderer sees it; `None` when it is not one.
pub fn camera<S: SceneSource>(s: &S, obj: S::Object) -> Option<Camera> {
    let frustum_culling = || {
        on(
            s,
            obj,
            P::FrustumCullingEnabled,
            d::camera::FRUSTUM_CULLING_ENABLED,
        )
    };
    if s.is(obj, T::PerspectiveCamera) {
        return Some(Camera::Perspective {
            fov_degrees: num(s, obj, P::FieldOfView, d::perspective_camera::FIELD_OF_VIEW),
            fov_horizontal: int(
                s,
                obj,
                P::FieldOfViewOrientation,
                d::perspective_camera::FIELD_OF_VIEW_ORIENTATION,
            ) == fov_orientation::HORIZONTAL,
            clip_near: num(s, obj, P::ClipNear, d::perspective_camera::CLIP_NEAR),
            clip_far: num(s, obj, P::ClipFar, d::perspective_camera::CLIP_FAR),
            frustum_culling: frustum_culling(),
        });
    }
    if s.is(obj, T::OrthographicCamera) {
        use d::orthographic_camera as o;
        return Some(Camera::Orthographic {
            clip_near: num(s, obj, P::ClipNear, o::CLIP_NEAR),
            clip_far: num(s, obj, P::ClipFar, o::CLIP_FAR),
            horizontal_magnification: num(
                s,
                obj,
                P::HorizontalMagnification,
                o::HORIZONTAL_MAGNIFICATION,
            ),
            vertical_magnification: num(
                s,
                obj,
                P::VerticalMagnification,
                o::VERTICAL_MAGNIFICATION,
            ),
            frustum_culling: frustum_culling(),
        });
    }
    None
}

struct Walk<'s, S: SceneSource> {
    s: &'s S,
    /// Painted again when a texture it asked for is ready.
    view: S::Object,
    scene: Scene,
    ids: HashMap<S::Object, NodeId>,
    loaded: Vec<(NodeId, S::Object)>,
    /// Models whose instance root or joints are other nodes, which may
    /// come later in the walk: given once every node has its place.
    later: Vec<(S::Object, NodeId)>,
}

impl<S: SceneSource> Walk<'_, S> {
    /// The lines of an `InfiniteGrid` in the environment: the grid in
    /// grey and, with `gridAxes`, the X axis in red and the Z axis in blue.
    fn infinite_grid(&mut self, lines: &GridLines) {
        let line = |mesh: &Arc<CustomGeometry>, rgb: [f32; 3]| {
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Custom(mesh.clone()),
                material: Material::Default(DefaultMaterial {
                    diffuse_color: Color::rgb(rgb[0], rgb[1], rgb[2]),
                    lighting: Lighting::None,
                    cull_mode: CullMode::None,
                    ..DefaultMaterial::default()
                }),
            }))
        };
        self.scene.add(None, line(&lines.grid, [0.5, 0.5, 0.5]));
        let (x, z) = if lines.axes {
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
        } else {
            ([0.5; 3], [0.5; 3])
        };
        self.scene.add(None, line(&lines.x_axis, x));
        self.scene.add(None, line(&lines.z_axis, z));
    }

    fn node(&mut self, parent: Option<NodeId>, obj: S::Object) {
        let s = self.s;
        // Materials, environments and anything else declared among the
        // nodes are not drawn by themselves.
        if !s.is(obj, T::Node) || self.ids.contains_key(&obj) {
            return;
        }
        let mut n = Node::new(kind(s, self.view, obj)).with_transform(transform(s, obj));
        n.visible = on(s, obj, P::Visible, d::node::VISIBLE);
        n.opacity = num(s, obj, P::Opacity, d::node::OPACITY);
        let is_model = s.is(obj, T::Model);
        if is_model {
            n.casts_shadows = on(s, obj, P::CastsShadows, d::model::CASTS_SHADOWS);
            n.receives_shadows = on(s, obj, P::ReceivesShadows, d::model::RECEIVES_SHADOWS);
            n.instancing = s.object(obj, P::Instancing).and_then(|t| s.instancing(t));
        }
        n.shadow = shadow(s, obj);
        let id = self.scene.add(parent, n);
        self.ids.insert(obj, id);
        if is_model {
            self.later.push((obj, id));
        }
        // A loaded model's nodes are the loader's first children. Each
        // mesh is named by the Model the loader made for it.
        #[cfg(feature = "gltf")]
        if let Some(loaded) = s.loaded(obj) {
            self.loaded(obj, id, &loaded);
        }
        for child in s.objects(obj, P::Data) {
            self.node(Some(id), child);
        }
        // Its items, as one picture; the pick finds them by their holder.
        if let Some((quad, holder)) = s.items_quad(self.view, obj) {
            let q = self.scene.add(Some(id), quad);
            self.ids.insert(holder, q);
        }
    }

    /// Gives the models their instance roots and skins.
    fn finish(&mut self) {
        let s = self.s;
        for (obj, id) in std::mem::take(&mut self.later) {
            let root = s
                .object(obj, P::InstanceRoot)
                .and_then(|r| self.ids.get(&r).copied());
            let skin = skin(s, obj, &self.ids);
            if let Some(node) = self.scene.node_mut(id) {
                if let Some(instancing) = node.instancing.as_mut() {
                    instancing.root = root;
                }
                node.skin = skin;
            }
        }
    }

    /// Adds the loader `obj`'s file under its node `id`. The loader's
    /// table, if it has one, copies the whole file: each mesh is copied
    /// in the loader's space.
    #[cfg(feature = "gltf")]
    fn loaded(&mut self, obj: S::Object, id: NodeId, loaded: &super::Loaded<S::Object>) {
        let s = self.s;
        let placed = loaded.asset.add_to(&mut self.scene, id);
        let instancing = s
            .object(obj, P::Instancing)
            .and_then(|t| s.instancing(t))
            .map(|i| crate::instancing::Instancing {
                root: Some(id),
                ..i
            });
        for (index, model) in placed.models {
            if let Some(i) = &instancing
                && let Some(node) = self.scene.node_mut(model)
            {
                node.instancing = Some(i.clone());
            }
            if let Some((_, m)) = loaded.models.iter().find(|(n, _)| *n == index) {
                self.loaded.push((model, *m));
            }
        }
    }
}

/// A node's place. `position` and `rotation` are what Qt keeps; a graph
/// that holds only `x`, `y`, `z` or `eulerRotation` is read through those.
fn transform<S: SceneSource>(s: &S, obj: S::Object) -> Transform {
    let position = s.vector3d(obj, P::Position).unwrap_or_else(|| {
        let [x, y, z] = d::node::POSITION;
        Vec3::new(
            s.number(obj, P::X).map_or(x, |v| v as f32),
            s.number(obj, P::Y).map_or(y, |v| v as f32),
            s.number(obj, P::Z).map_or(z, |v| v as f32),
        )
    });
    let rotation = s
        .quaternion(obj, P::Rotation)
        .or_else(|| {
            s.vector3d(obj, P::EulerRotation)
                .map(Quat::from_euler_degrees)
        })
        .map_or(Quat::IDENTITY, unit);
    Transform {
        position,
        rotation,
        scale: vec3(s, obj, P::Scale, d::node::SCALE),
    }
}

/// What the light `obj` casts shadows with, if it is a light that casts
/// them.
fn shadow<S: SceneSource>(s: &S, obj: S::Object) -> Option<Shadow> {
    use d::light as l;
    if !s.is(obj, T::Light) || !on(s, obj, P::CastsShadow, l::CASTS_SHADOW) {
        return None;
    }
    let quality = int(s, obj, P::ShadowMapQuality, l::SHADOW_MAP_QUALITY)
        .clamp(shadow_map::LOW, shadow_map::ULTRA);
    // PCF32 and PCF64 take 16 samples: past that a pixel costs more than
    // the softer edge is worth on the slow targets.
    let samples = match int(s, obj, P::SoftShadowQuality, l::SOFT_SHADOW_QUALITY)
        .clamp(soft_shadow::HARD, soft_shadow::PCF64)
    {
        soft_shadow::HARD => 1,
        q => (4u32 << (q - 1)).min(16),
    };
    let cascades = if s.is(obj, T::DirectionalLight) {
        use d::directional_light as dl;
        Cascades {
            splits: int(s, obj, P::CsmNumSplits, dl::CSM_NUM_SPLITS).clamp(0, 3) as u32,
            at: [
                num(s, obj, P::CsmSplit1, dl::CSM_SPLIT1),
                num(s, obj, P::CsmSplit2, dl::CSM_SPLIT2),
                num(s, obj, P::CsmSplit3, dl::CSM_SPLIT3),
            ],
            blend: num(s, obj, P::CsmBlendRatio, dl::CSM_BLEND_RATIO),
            lock_texels: on(s, obj, P::LockShadowmapTexels, dl::LOCK_SHADOWMAP_TEXELS),
        }
    } else {
        Cascades::default()
    };
    Some(Shadow {
        map_size: 256 << quality,
        bias: num(s, obj, P::ShadowBias, l::SHADOW_BIAS),
        factor: (num(s, obj, P::ShadowFactor, l::SHADOW_FACTOR) / 100.0).clamp(0.0, 1.0),
        far: num(s, obj, P::ShadowMapFar, l::SHADOW_MAP_FAR),
        samples,
        pcf_radius: num(s, obj, P::PcfFactor, l::PCF_FACTOR),
        cascades,
    })
}

fn kind<S: SceneSource>(s: &S, view: S::Object, obj: S::Object) -> NodeKind {
    if s.is(obj, T::Model) {
        return model(s, view, obj).map_or(NodeKind::Group, NodeKind::Model);
    }
    if let Some(camera) = camera(s, obj) {
        return NodeKind::Camera(camera);
    }
    if !s.is(obj, T::Light) {
        return NodeKind::Group;
    }
    let light_color = color(s, obj, P::Color, d::light::COLOR);
    let ambient_color = color(s, obj, P::AmbientColor, d::light::AMBIENT_COLOR);
    let brightness = num(s, obj, P::Brightness, d::light::BRIGHTNESS);
    let fade = || Fade {
        constant: num(s, obj, P::ConstantFade, d::fade::CONSTANT),
        linear: num(s, obj, P::LinearFade, d::fade::LINEAR),
        quadratic: num(s, obj, P::QuadraticFade, d::fade::QUADRATIC),
    };
    if s.is(obj, T::DirectionalLight) {
        return NodeKind::Light(Light::Directional {
            color: light_color,
            ambient_color,
            brightness,
        });
    }
    if s.is(obj, T::PointLight) {
        return NodeKind::Light(Light::Point {
            color: light_color,
            ambient_color,
            brightness,
            fade: fade(),
        });
    }
    if s.is(obj, T::SpotLight) {
        return NodeKind::Light(Light::Spot {
            color: light_color,
            ambient_color,
            brightness,
            fade: fade(),
            cone_angle: num(s, obj, P::ConeAngle, d::spot_light::CONE_ANGLE),
            inner_cone_angle: num(s, obj, P::InnerConeAngle, d::spot_light::INNER_CONE_ANGLE),
        });
    }
    NodeKind::Group
}

/// `None` for a mesh this renderer does not build yet: the node still
/// places its children.
fn model<S: SceneSource>(s: &S, view: S::Object, obj: S::Object) -> Option<Model> {
    let mesh = match s.object(obj, P::Geometry) {
        // The geometry wins over a source, as it is the application's
        // own mesh. One never updated has nothing to draw yet.
        Some(g) if s.is(g, T::Geometry) => MeshSource::Custom(s.geometry(g)?),
        _ => {
            let source = s.url(obj, P::Source)?;
            // A relative `#Cube` comes resolved against the file's URL;
            // the built-in meshes are named by the fragment.
            let name = source.rfind('#').map(|i| &source[i..])?;
            MeshSource::Primitive(Primitive::from_source(name)?)
        }
    };
    // A built-in mesh has one part, drawn with the first material.
    let first = s.objects(obj, P::Materials).into_iter().next();
    let material = match first {
        // Snippets that make no program leave the model out, as Qt does.
        Some(m) if s.is(m, T::CustomMaterial) => Material::Custom(s.custom_material(view, m)?),
        _ => first.and_then(|m| material(s, view, m)).unwrap_or_default(),
    };
    Some(Model { mesh, material })
}

fn material<S: SceneSource>(s: &S, view: S::Object, obj: S::Object) -> Option<Material> {
    let map = |p| s.object(obj, p).and_then(|t| texture(s, view, t));
    let channel = |p, default| match int(s, obj, p, default) {
        channel::G => Channel::G,
        channel::B => Channel::B,
        channel::A => Channel::A,
        _ => Channel::R,
    };
    let lighting = |default| {
        if int(s, obj, P::Lighting, default) == lighting::NO_LIGHTING {
            Lighting::None
        } else {
            Lighting::Fragment
        }
    };
    let cull_mode = || match int(s, obj, P::CullMode, d::material::CULL_MODE) {
        cull::FRONT => CullMode::Front,
        cull::NONE => CullMode::None,
        _ => CullMode::Back,
    };
    if s.is(obj, T::DefaultMaterial) {
        use d::default_material as dm;
        return Some(Material::Default(DefaultMaterial {
            diffuse_color: color(s, obj, P::DiffuseColor, dm::DIFFUSE_COLOR),
            diffuse_map: map(P::DiffuseMap),
            lighting: lighting(dm::LIGHTING),
            specular_amount: num(s, obj, P::SpecularAmount, dm::SPECULAR_AMOUNT),
            specular_roughness: num(s, obj, P::SpecularRoughness, dm::SPECULAR_ROUGHNESS),
            cull_mode: cull_mode(),
        }));
    }
    if s.is(obj, T::PrincipledMaterial) {
        use d::principled_material as pm;
        let emissive = vec3(s, obj, P::EmissiveFactor, pm::EMISSIVE_FACTOR);
        return Some(Material::Principled(PrincipledMaterial {
            base_color: color(s, obj, P::BaseColor, pm::BASE_COLOR),
            metalness: num(s, obj, P::Metalness, pm::METALNESS),
            roughness: num(s, obj, P::Roughness, pm::ROUGHNESS),
            emissive: [emissive.x, emissive.y, emissive.z],
            opacity: num(s, obj, P::Opacity, pm::OPACITY),
            alpha_mode: match int(s, obj, P::AlphaMode, pm::ALPHA_MODE) {
                alpha::BLEND => AlphaMode::Blend,
                alpha::OPAQUE => AlphaMode::Opaque,
                alpha::MASK => AlphaMode::Mask,
                _ => AlphaMode::Default,
            },
            alpha_cutoff: num(s, obj, P::AlphaCutoff, pm::ALPHA_CUTOFF),
            cull_mode: cull_mode(),
            lighting: lighting(pm::LIGHTING),
            maps: Box::new(PrincipledMaps {
                base_color: map(P::BaseColorMap),
                metalness: map(P::MetalnessMap),
                metalness_channel: channel(P::MetalnessChannel, pm::METALNESS_CHANNEL),
                roughness: map(P::RoughnessMap),
                roughness_channel: channel(P::RoughnessChannel, pm::ROUGHNESS_CHANNEL),
                normal: map(P::NormalMap),
                normal_strength: num(s, obj, P::NormalStrength, pm::NORMAL_STRENGTH),
                emissive: map(P::EmissiveMap),
                occlusion: map(P::OcclusionMap),
                occlusion_channel: channel(P::OcclusionChannel, pm::OCCLUSION_CHANNEL),
                occlusion_amount: num(s, obj, P::OcclusionAmount, pm::OCCLUSION_AMOUNT),
                opacity: map(P::OpacityMap),
                opacity_channel: channel(P::OpacityChannel, pm::OPACITY_CHANNEL),
            }),
        }));
    }
    None
}

/// The `Texture` `obj` as a material or the environment samples it;
/// `None` while its pixels are not ready, or when it is not a texture.
pub fn texture<S: SceneSource>(s: &S, view: S::Object, obj: S::Object) -> Option<Texture> {
    use d::texture as dt;
    if !s.is(obj, T::Texture) {
        return None;
    }
    let image = s.texture_image(view, obj)?;
    let filter = |p, default| match int(s, obj, p, default) {
        filter::NONE => Filter::None,
        filter::NEAREST => Filter::Nearest,
        _ => Filter::Linear,
    };
    let sampling = Sampling {
        min: filter(P::MinFilter, dt::MIN_FILTER),
        mag: filter(P::MagFilter, dt::MAG_FILTER),
        mip: filter(P::MipFilter, dt::MIP_FILTER),
        tiling_u: tiling_mode(int(s, obj, P::TilingModeHorizontal, dt::TILING_MODE)),
        tiling_v: tiling_mode(int(s, obj, P::TilingModeVertical, dt::TILING_MODE)),
        generate_mipmaps: on(s, obj, P::GenerateMipmaps, dt::GENERATE_MIPMAPS),
    };
    Some(Texture {
        image,
        sampling,
        uv: uv_transform(s, obj),
        // QtQuick3D has two UV sets: a larger index reads the second.
        uv_set: int(s, obj, P::IndexUV, dt::INDEX_UV).clamp(0, 1) as u32,
    })
}

/// `Texture.tilingModeHorizontal`'s number as the sampler's tiling.
pub fn tiling_mode(t: i32) -> Tiling {
    match t {
        tiling::CLAMP_TO_EDGE => Tiling::ClampToEdge,
        tiling::MIRRORED_REPEAT => Tiling::MirroredRepeat,
        _ => Tiling::Repeat,
    }
}

/// The `Texture` `obj`'s placement on a mesh's UVs.
pub fn uv_transform<S: SceneSource>(s: &S, obj: S::Object) -> UvTransform {
    use d::texture as dt;
    UvTransform {
        scale: [
            num(s, obj, P::ScaleU, dt::SCALE_U),
            num(s, obj, P::ScaleV, dt::SCALE_V),
        ],
        position: [
            num(s, obj, P::PositionU, dt::POSITION_U),
            num(s, obj, P::PositionV, dt::POSITION_V),
        ],
        rotation: num(s, obj, P::RotationUV, dt::ROTATION_UV),
        pivot: [
            num(s, obj, P::PivotU, dt::PIVOT_U),
            num(s, obj, P::PivotV, dt::PIVOT_V),
        ],
        flip_u: on(s, obj, P::FlipU, dt::FLIP_U),
        flip_v: on(s, obj, P::FlipV, dt::FLIP_V),
    }
}

/// The joints a model's vertices follow: its `skin`'s, else its
/// `skeleton`'s. `None` while a joint is not in the scene.
fn skin<S: SceneSource>(s: &S, obj: S::Object, ids: &HashMap<S::Object, NodeId>) -> Option<Skin> {
    let (joints, inverse_bind) = if let Some(sk) = s.object(obj, P::Skin)
        && s.is(sk, T::Skin)
    {
        (
            s.object_slots(sk, P::Joints),
            s.matrices(sk, P::InverseBindPoses),
        )
    } else if let Some(sk) = s.object(obj, P::Skeleton)
        && s.is(sk, T::Skeleton)
    {
        (skeleton_joints(s, sk), s.matrices(obj, P::InverseBindPoses))
    } else {
        return None;
    };
    let joints = joints
        .into_iter()
        .map(|j| ids.get(&j?).copied())
        .collect::<Option<Vec<_>>>()?;
    Some(Skin {
        joints,
        inverse_bind,
    })
}

/// The `Joint`s under `skeleton`, at their indices; a joint whose
/// `skeletonRoot` is another skeleton is that one's.
fn skeleton_joints<S: SceneSource>(s: &S, skeleton: S::Object) -> Vec<Option<S::Object>> {
    let mut found = Vec::new();
    let mut stack = vec![skeleton];
    while let Some(n) = stack.pop() {
        for child in s.object_slots(n, P::Data).into_iter().flatten() {
            if s.is(child, T::Joint) {
                let root = s.object(child, P::SkeletonRoot);
                if root.is_none_or(|r| r == skeleton)
                    && let Ok(i) = usize::try_from(int(s, child, P::Index, 0))
                {
                    found.push((i, child));
                }
            }
            if s.is(child, T::Node) {
                stack.push(child);
            }
        }
    }
    let len = found.iter().map(|(i, _)| i + 1).max().unwrap_or(0);
    let mut joints = vec![None; len.min(MAX_JOINTS)];
    for (i, j) in found {
        if let Some(slot) = joints.get_mut(i) {
            *slot = Some(j);
        }
    }
    joints
}

fn environment<S: SceneSource>(s: &S, view: S::Object, env: S::Object) -> Environment {
    use d::scene_environment as se;
    if !s.is(env, T::SceneEnvironment) {
        return Environment::default();
    }
    let background = match int(s, env, P::BackgroundMode, se::BACKGROUND_MODE) {
        background::COLOR => Background::Color(color(s, env, P::ClearColor, se::CLEAR_COLOR)),
        background::SKY_BOX => Background::SkyBox,
        // A cube map and a sky material are not built: the 2D scene
        // shows through.
        _ => Background::Transparent,
    };
    // SSAA and progressive antialiasing are not built: they draw as none.
    let msaa_samples =
        if int(s, env, P::AntialiasingMode, se::ANTIALIASING_MODE) == antialiasing::MSAA {
            match int(s, env, P::AntialiasingQuality, se::ANTIALIASING_QUALITY) {
                antialiasing::MEDIUM => 2,
                antialiasing::VERY_HIGH => 8,
                _ => 4,
            }
        } else {
            1
        };
    let light_probe = s
        .object(env, P::LightProbe)
        .and_then(|t| texture(s, view, t))
        .map(|texture| LightProbe {
            texture,
            exposure: num(s, env, P::ProbeExposure, se::PROBE_EXPOSURE),
            horizon: num(s, env, P::ProbeHorizon, se::PROBE_HORIZON),
            orientation: vec3(s, env, P::ProbeOrientation, se::PROBE_ORIENTATION),
            skybox_blur: num(s, env, P::SkyboxBlurAmount, se::SKYBOX_BLUR_AMOUNT),
        });
    let tonemap = match int(s, env, P::TonemapMode, se::TONEMAP_MODE) {
        tonemap::NONE => Tonemap::None,
        tonemap::ACES => Tonemap::Aces,
        tonemap::HEJL_DAWSON => Tonemap::HejlDawson,
        tonemap::FILMIC => Tonemap::Filmic,
        // TonemapModeCustom is not built.
        _ => Tonemap::Linear,
    };
    // An effect whose snippets make no program is left out; the others
    // still run.
    let effects = s
        .objects(env, P::Effects)
        .into_iter()
        .filter_map(|fx| s.effect(view, fx))
        .collect();
    // Code written before `aoEnabled` sets only the strength; once given,
    // `aoEnabled` decides.
    let ao_on = on(s, env, P::AoEnabled, se::AO_ENABLED) || !s.authored(env, P::AoEnabled);
    let ambient_occlusion = Some(AmbientOcclusion {
        strength: num(s, env, P::AoStrength, se::AO_STRENGTH).clamp(0.0, 100.0),
        distance: num(s, env, P::AoDistance, se::AO_DISTANCE),
        softness: num(s, env, P::AoSoftness, se::AO_SOFTNESS).clamp(0.0, 50.0),
        sample_rate: int(s, env, P::AoSampleRate, se::AO_SAMPLE_RATE).clamp(2, 4) as u32,
        bias: num(s, env, P::AoBias, se::AO_BIAS),
        dither: on(s, env, P::AoDither, se::AO_DITHER),
    })
    .filter(|ao| ao_on && ao.shows());
    let mut environment = Environment {
        background,
        msaa_samples,
        light_probe,
        tonemap,
        effects,
        ambient_occlusion,
        ..Environment::default()
    };
    if s.is(env, T::ExtendedSceneEnvironment) {
        extended(s, view, env, &mut environment);
    }
    environment
}

/// ExtendedSceneEnvironment's exposure, white point and built-in effects.
fn extended<S: SceneSource>(s: &S, view: S::Object, env: S::Object, out: &mut Environment) {
    use d::extended_scene_environment as x;
    let n = |p, default| num(s, env, p, default);
    let b = |p, default| on(s, env, p, default);
    out.exposure = n(P::Exposure, x::EXPOSURE);
    out.white_point = n(P::WhitePoint, x::WHITE_POINT);
    let vignette_color = color(s, env, P::VignetteColor, x::VIGNETTE_COLOR).to_linear();
    out.post = PostProcess {
        on: true,
        glow: b(P::GlowEnabled, x::GLOW_ENABLED).then(|| Glow {
            strength: n(P::GlowStrength, x::GLOW_STRENGTH),
            intensity: n(P::GlowIntensity, x::GLOW_INTENSITY),
            bloom: n(P::GlowBloom, x::GLOW_BLOOM),
            hdr_minimum: n(P::GlowHdrMinimumValue, x::GLOW_HDR_MINIMUM_VALUE),
            hdr_maximum: n(P::GlowHdrMaximumValue, x::GLOW_HDR_MAXIMUM_VALUE),
            hdr_scale: n(P::GlowHdrScale, x::GLOW_HDR_SCALE),
            levels: int(s, env, P::GlowLevel, x::GLOW_LEVEL).clamp(0, 127) as u32,
            blend: GlowBlend::from_index(i64::from(int(
                s,
                env,
                P::GlowBlendMode,
                x::GLOW_BLEND_MODE,
            ))),
            quality_high: b(P::GlowQualityHigh, x::GLOW_QUALITY_HIGH),
        }),
        depth_of_field: b(P::DepthOfFieldEnabled, x::DEPTH_OF_FIELD_ENABLED).then(|| {
            DepthOfField {
                focus_distance: n(
                    P::DepthOfFieldFocusDistance,
                    x::DEPTH_OF_FIELD_FOCUS_DISTANCE,
                ),
                focus_range: n(P::DepthOfFieldFocusRange, x::DEPTH_OF_FIELD_FOCUS_RANGE),
                blur_amount: n(P::DepthOfFieldBlurAmount, x::DEPTH_OF_FIELD_BLUR_AMOUNT),
            }
        }),
        adjustments: b(P::ColorAdjustmentsEnabled, x::COLOR_ADJUSTMENTS_ENABLED).then(|| {
            Adjustments {
                brightness: n(P::AdjustmentBrightness, x::ADJUSTMENT_BRIGHTNESS),
                contrast: n(P::AdjustmentContrast, x::ADJUSTMENT_CONTRAST),
                saturation: n(P::AdjustmentSaturation, x::ADJUSTMENT_SATURATION),
            }
        }),
        vignette: b(P::VignetteEnabled, x::VIGNETTE_ENABLED).then(|| Vignette {
            color: [vignette_color[0], vignette_color[1], vignette_color[2]],
            radius: n(P::VignetteRadius, x::VIGNETTE_RADIUS),
            strength: n(P::VignetteStrength, x::VIGNETTE_STRENGTH),
        }),
        lut: b(P::LutEnabled, x::LUT_ENABLED).then(|| Lut {
            texture: s
                .object(env, P::LutTexture)
                .and_then(|t| texture(s, view, t)),
            size: n(P::LutSize, x::LUT_SIZE),
            alpha: n(P::LutFilterAlpha, x::LUT_FILTER_ALPHA),
        }),
        fxaa: b(P::FxaaEnabled, x::FXAA_ENABLED),
        dithering: b(P::DitheringEnabled, x::DITHERING_ENABLED),
        sharpness: n(P::SharpnessAmount, x::SHARPNESS_AMOUNT).clamp(0.0, 1.0),
    };
}
