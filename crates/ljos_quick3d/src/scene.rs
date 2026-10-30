//! A view's objects, walked into an `ljos_3d` scene.

use std::collections::HashMap;

use ljos::item::{self, ljos_3d};
use ljos::prelude::*;
use ljos_3d::{
    AlphaMode, Background, Channel, CullMode, Environment, Fade, LightProbe, Lighting, MeshSource,
    NodeId, NodeKind, Primitive, PrincipledMaps, Scene, Tonemap, Transform,
};

use crate::node::{self, alpha, antialiasing, background, channel, cull, lighting, tonemap};
use crate::texture;
use crate::{
    Camera, DefaultMaterial, DirectionalLight, Geometry, Light, Material, Model, Node, Object3D,
    OrthographicCamera, PerspectiveCamera, PointLight, PrincipledMaterial, SceneEnvironment,
    SpotLight, View3D,
};

/// The scene of the view `view`, read now. Tracked.
pub(crate) fn build(e: &Engine, view: ObjectId) -> Scene {
    build_with_ids(e, view).0
}

/// [`build`], with the scene node each object became: what a pick needs
/// to say which object it hit.
pub(crate) fn build_with_ids(e: &Engine, view: ObjectId) -> (Scene, HashMap<ObjectId, NodeId>) {
    let mut walk = Walk {
        e,
        view,
        scene: Scene::new(),
        ids: HashMap::new(),
    };
    for o in item::resources(e, view) {
        walk.node(None, o);
    }
    if let Some(imported) = item::prop(e, view, View3D::IMPORT_SCENE) {
        walk.node(None, imported);
    }
    let env = item::prop(e, view, View3D::ENVIRONMENT)
        .map_or_else(Environment::default, |env| environment(e, view, env));
    walk.scene.set_environment(env);
    // A camera that is not in the scene leaves the choice to the scene:
    // its first camera.
    let camera = item::prop(e, view, View3D::CAMERA).and_then(|c| walk.ids.get(&c).copied());
    walk.scene.set_camera(camera);
    (walk.scene, walk.ids)
}

/// The camera `obj` is, as the renderer sees it; `None` when it is not one.
pub(crate) fn camera(e: &Engine, obj: ObjectId) -> Option<ljos_3d::Camera> {
    if e.is::<PerspectiveCamera>(obj) {
        return Some(ljos_3d::Camera::Perspective {
            fov_degrees: item::prop(e, obj, PerspectiveCamera::FIELD_OF_VIEW) as f32,
            fov_horizontal: item::prop(e, obj, PerspectiveCamera::FIELD_OF_VIEW_ORIENTATION) == 1,
            clip_near: item::prop(e, obj, PerspectiveCamera::CLIP_NEAR) as f32,
            clip_far: item::prop(e, obj, PerspectiveCamera::CLIP_FAR) as f32,
            frustum_culling: item::prop(e, obj, Camera::FRUSTUM_CULLING_ENABLED),
        });
    }
    if e.is::<OrthographicCamera>(obj) {
        return Some(ljos_3d::Camera::Orthographic {
            clip_near: item::prop(e, obj, OrthographicCamera::CLIP_NEAR) as f32,
            clip_far: item::prop(e, obj, OrthographicCamera::CLIP_FAR) as f32,
            horizontal_magnification: item::prop(
                e,
                obj,
                OrthographicCamera::HORIZONTAL_MAGNIFICATION,
            ) as f32,
            vertical_magnification: item::prop(e, obj, OrthographicCamera::VERTICAL_MAGNIFICATION)
                as f32,
            frustum_culling: item::prop(e, obj, Camera::FRUSTUM_CULLING_ENABLED),
        });
    }
    None
}

struct Walk<'e> {
    e: &'e Engine,
    /// Painted again when a texture it asked for is ready.
    view: ObjectId,
    scene: Scene,
    ids: HashMap<ObjectId, NodeId>,
}

impl Walk<'_> {
    fn node(&mut self, parent: Option<NodeId>, obj: ObjectId) {
        let e = self.e;
        // Materials, environments and anything else declared among the
        // nodes are not drawn by themselves.
        if !e.is::<Node>(obj) || self.ids.contains_key(&obj) {
            return;
        }
        let transform = Transform {
            position: node::vec3(item::prop(e, obj, Node::POSITION)),
            rotation: node::quat(e.get(obj, Node::ROTATION).unwrap_or(Quaternion {
                scalar: 1.0,
                x: 0.0,
                y: 0.0,
                z: 0.0,
            })),
            scale: node::vec3(e.get(obj, Node::SCALE).unwrap_or(Vector3d {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            })),
        };
        let mut n = ljos_3d::Node::new(kind(e, self.view, obj)).with_transform(transform);
        n.visible = item::prop(e, obj, Node::VISIBLE);
        if e.is::<Model>(obj) {
            n.casts_shadows = item::prop(e, obj, Model::CASTS_SHADOWS);
            n.receives_shadows = item::prop(e, obj, Model::RECEIVES_SHADOWS);
        }
        n.shadow = shadow(e, obj);
        let id = self.scene.add(parent, n);
        self.ids.insert(obj, id);
        for child in list(e, obj, Object3D::DATA.index) {
            self.node(Some(id), child);
        }
    }
}

/// What the light `obj` casts shadows with, if it is a light that casts
/// them.
fn shadow(e: &Engine, obj: ObjectId) -> Option<ljos_3d::Shadow> {
    if !e.is::<Light>(obj) || !item::prop(e, obj, Light::CASTS_SHADOW) {
        return None;
    }
    let quality = item::prop(e, obj, Light::SHADOW_MAP_QUALITY)
        .clamp(node::shadow_map::LOW, node::shadow_map::ULTRA);
    // PCF32 and PCF64 take 16 samples: past that a pixel costs more than
    // the softer edge is worth on the slow targets.
    let samples = match item::prop(e, obj, Light::SOFT_SHADOW_QUALITY)
        .clamp(node::soft_shadow::HARD, node::soft_shadow::PCF64)
    {
        node::soft_shadow::HARD => 1,
        q => (4u32 << (q - 1)).min(16),
    };
    let cascades = if e.is::<DirectionalLight>(obj) {
        ljos_3d::Cascades {
            splits: item::prop(e, obj, DirectionalLight::CSM_NUM_SPLITS).clamp(0, 3) as u32,
            at: [
                DirectionalLight::CSM_SPLIT1,
                DirectionalLight::CSM_SPLIT2,
                DirectionalLight::CSM_SPLIT3,
            ]
            .map(|p| item::prop(e, obj, p) as f32),
            blend: item::prop(e, obj, DirectionalLight::CSM_BLEND_RATIO) as f32,
            lock_texels: item::prop(e, obj, DirectionalLight::LOCK_SHADOWMAP_TEXELS),
        }
    } else {
        ljos_3d::Cascades::default()
    };
    Some(ljos_3d::Shadow {
        map_size: 256 << quality,
        bias: item::prop(e, obj, Light::SHADOW_BIAS) as f32,
        factor: (item::prop(e, obj, Light::SHADOW_FACTOR) as f32 / 100.0).clamp(0.0, 1.0),
        far: item::prop(e, obj, Light::SHADOW_MAP_FAR) as f32,
        samples,
        pcf_radius: item::prop(e, obj, Light::PCF_FACTOR) as f32,
        cascades,
    })
}

fn kind(e: &Engine, view: ObjectId, obj: ObjectId) -> NodeKind {
    if e.is::<Model>(obj) {
        return model(e, view, obj).map_or(NodeKind::Group, NodeKind::Model);
    }
    if let Some(camera) = camera(e, obj) {
        return NodeKind::Camera(camera);
    }
    let color_of = |p| color(item::prop(e, obj, p));
    let (light_color, ambient_color) = (color_of(Light::COLOR), color_of(Light::AMBIENT_COLOR));
    let brightness = item::prop(e, obj, Light::BRIGHTNESS) as f32;
    if e.is::<DirectionalLight>(obj) {
        return NodeKind::Light(ljos_3d::Light::Directional {
            color: light_color,
            ambient_color,
            brightness,
        });
    }
    if e.is::<PointLight>(obj) {
        return NodeKind::Light(ljos_3d::Light::Point {
            color: light_color,
            ambient_color,
            brightness,
            fade: Fade {
                constant: item::prop(e, obj, PointLight::CONSTANT_FADE) as f32,
                linear: item::prop(e, obj, PointLight::LINEAR_FADE) as f32,
                quadratic: item::prop(e, obj, PointLight::QUADRATIC_FADE) as f32,
            },
        });
    }
    if e.is::<SpotLight>(obj) {
        return NodeKind::Light(ljos_3d::Light::Spot {
            color: light_color,
            ambient_color,
            brightness,
            fade: Fade {
                constant: item::prop(e, obj, SpotLight::CONSTANT_FADE) as f32,
                linear: item::prop(e, obj, SpotLight::LINEAR_FADE) as f32,
                quadratic: item::prop(e, obj, SpotLight::QUADRATIC_FADE) as f32,
            },
            cone_angle: item::prop(e, obj, SpotLight::CONE_ANGLE) as f32,
            inner_cone_angle: item::prop(e, obj, SpotLight::INNER_CONE_ANGLE) as f32,
        });
    }
    NodeKind::Group
}

/// `None` for a mesh this renderer does not build yet: the node still
/// places its children.
fn model(e: &Engine, view: ObjectId, obj: ObjectId) -> Option<ljos_3d::Model> {
    let mesh = match item::prop(e, obj, Model::GEOMETRY) {
        // The geometry wins over a source, as it is the application's
        // own mesh. One never updated has nothing to draw yet.
        Some(g) if e.is::<Geometry>(g) => {
            // The revision is what a commit notifies.
            item::prop(e, g, Geometry::REVISION);
            MeshSource::Custom(e.with(g, Geometry::committed).ok().flatten()?)
        }
        _ => {
            let source = item::prop(e, obj, Model::SOURCE);
            // A relative `#Cube` comes resolved against the file's URL;
            // the built-in meshes are named by the fragment.
            let name = source
                .0
                .as_str()
                .rfind('#')
                .map(|i| &source.0.as_str()[i..])?;
            MeshSource::Primitive(Primitive::from_source(name)?)
        }
    };
    // A built-in mesh has one part, drawn with the first material.
    let material = list(e, obj, Model::MATERIALS.index)
        .into_iter()
        .next()
        .and_then(|m| material(e, view, m))
        .unwrap_or_default();
    Some(ljos_3d::Model { mesh, material })
}

fn material(e: &Engine, view: ObjectId, obj: ObjectId) -> Option<ljos_3d::Material> {
    let map =
        |p: Prop<Option<ObjectId>>| item::prop(e, obj, p).and_then(|t| texture::read(e, view, t));
    let channel = |p: Prop<i32>| match item::prop(e, obj, p) {
        channel::G => Channel::G,
        channel::B => Channel::B,
        channel::A => Channel::A,
        _ => Channel::R,
    };
    let lighting = |l| {
        if l == lighting::NO_LIGHTING {
            Lighting::None
        } else {
            Lighting::Fragment
        }
    };
    let cull_mode = || match item::prop(e, obj, Material::CULL_MODE) {
        cull::FRONT => CullMode::Front,
        cull::NONE => CullMode::None,
        _ => CullMode::Back,
    };
    if e.is::<DefaultMaterial>(obj) {
        return Some(ljos_3d::Material::Default(ljos_3d::DefaultMaterial {
            diffuse_color: color(item::prop(e, obj, DefaultMaterial::DIFFUSE_COLOR)),
            diffuse_map: map(DefaultMaterial::DIFFUSE_MAP),
            lighting: lighting(item::prop(e, obj, DefaultMaterial::LIGHTING)),
            specular_amount: item::prop(e, obj, DefaultMaterial::SPECULAR_AMOUNT) as f32,
            specular_roughness: item::prop(e, obj, DefaultMaterial::SPECULAR_ROUGHNESS) as f32,
            cull_mode: cull_mode(),
        }));
    }
    if e.is::<PrincipledMaterial>(obj) {
        let emissive = item::prop(e, obj, PrincipledMaterial::EMISSIVE_FACTOR);
        return Some(ljos_3d::Material::Principled(ljos_3d::PrincipledMaterial {
            base_color: color(item::prop(e, obj, PrincipledMaterial::BASE_COLOR)),
            metalness: item::prop(e, obj, PrincipledMaterial::METALNESS) as f32,
            roughness: item::prop(e, obj, PrincipledMaterial::ROUGHNESS) as f32,
            emissive: [emissive.x, emissive.y, emissive.z],
            opacity: item::prop(e, obj, PrincipledMaterial::OPACITY) as f32,
            alpha_mode: match item::prop(e, obj, PrincipledMaterial::ALPHA_MODE) {
                alpha::BLEND => AlphaMode::Blend,
                alpha::OPAQUE => AlphaMode::Opaque,
                alpha::MASK => AlphaMode::Mask,
                _ => AlphaMode::Default,
            },
            alpha_cutoff: item::prop(e, obj, PrincipledMaterial::ALPHA_CUTOFF) as f32,
            cull_mode: cull_mode(),
            lighting: lighting(item::prop(e, obj, PrincipledMaterial::LIGHTING)),
            maps: Box::new(PrincipledMaps {
                base_color: map(PrincipledMaterial::BASE_COLOR_MAP),
                metalness: map(PrincipledMaterial::METALNESS_MAP),
                metalness_channel: channel(PrincipledMaterial::METALNESS_CHANNEL),
                roughness: map(PrincipledMaterial::ROUGHNESS_MAP),
                roughness_channel: channel(PrincipledMaterial::ROUGHNESS_CHANNEL),
                normal: map(PrincipledMaterial::NORMAL_MAP),
                normal_strength: item::prop(e, obj, PrincipledMaterial::NORMAL_STRENGTH) as f32,
                emissive: map(PrincipledMaterial::EMISSIVE_MAP),
                occlusion: map(PrincipledMaterial::OCCLUSION_MAP),
                occlusion_channel: channel(PrincipledMaterial::OCCLUSION_CHANNEL),
                occlusion_amount: item::prop(e, obj, PrincipledMaterial::OCCLUSION_AMOUNT) as f32,
                opacity: map(PrincipledMaterial::OPACITY_MAP),
                opacity_channel: channel(PrincipledMaterial::OPACITY_CHANNEL),
            }),
        }));
    }
    None
}

fn environment(e: &Engine, view: ObjectId, env: ObjectId) -> Environment {
    if !e.is::<SceneEnvironment>(env) {
        return Environment::default();
    }
    let background = match item::prop(e, env, SceneEnvironment::BACKGROUND_MODE) {
        background::COLOR => {
            Background::Color(color(item::prop(e, env, SceneEnvironment::CLEAR_COLOR)))
        }
        background::SKY_BOX => Background::SkyBox,
        // A cube map and a sky material are not built: the 2D scene
        // shows through.
        _ => Background::Transparent,
    };
    // SSAA and progressive antialiasing are not built: they draw as none.
    let msaa_samples =
        if item::prop(e, env, SceneEnvironment::ANTIALIASING_MODE) == antialiasing::MSAA {
            match item::prop(e, env, SceneEnvironment::ANTIALIASING_QUALITY) {
                antialiasing::MEDIUM => 2,
                antialiasing::VERY_HIGH => 8,
                _ => 4,
            }
        } else {
            1
        };
    let light_probe = item::prop(e, env, SceneEnvironment::LIGHT_PROBE)
        .and_then(|t| texture::read(e, view, t))
        .map(|texture| {
            let orientation = item::prop(e, env, SceneEnvironment::PROBE_ORIENTATION);
            LightProbe {
                texture,
                exposure: item::prop(e, env, SceneEnvironment::PROBE_EXPOSURE) as f32,
                horizon: item::prop(e, env, SceneEnvironment::PROBE_HORIZON) as f32,
                orientation: node::vec3(orientation),
                skybox_blur: item::prop(e, env, SceneEnvironment::SKYBOX_BLUR_AMOUNT) as f32,
            }
        });
    let tonemap = match item::prop(e, env, SceneEnvironment::TONEMAP_MODE) {
        tonemap::NONE => Tonemap::None,
        tonemap::ACES => Tonemap::Aces,
        tonemap::HEJL_DAWSON => Tonemap::HejlDawson,
        tonemap::FILMIC => Tonemap::Filmic,
        // TonemapModeCustom is not built.
        _ => Tonemap::Linear,
    };
    // Exposure and the white point are ExtendedSceneEnvironment's, which
    // is not built: the documentation's 1 and 1.
    Environment {
        background,
        msaa_samples,
        light_probe,
        tonemap,
        ..Environment::default()
    }
}

fn color(c: Color) -> ljos_3d::Color {
    let [r, g, b, a] = c.to_rgba_f();
    ljos_3d::Color {
        r: r as f32,
        g: g as f32,
        b: b as f32,
        a: a as f32,
    }
}

/// An object list property's live objects; tracked.
fn list(e: &Engine, obj: ObjectId, p: PropIndex) -> Vec<ObjectId> {
    match e.read(obj, p) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| e.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}
