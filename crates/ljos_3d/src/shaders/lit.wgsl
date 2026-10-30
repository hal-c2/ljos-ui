// Every material the scene has, lit by up to MAX_LIGHTS lights and by the
// environment's light probe. Light is summed in linear space and tone
// mapped (tone.wgsl, which is put before this file) into sRGB,
// premultiplied, which is how the 2D scene that shows the picture blends.
//
// The metal-roughness model is the textbook one: a GGX distribution of
// microfacets, Smith's shadowing in Schlick's form and Schlick's Fresnel,
// over a Lambert diffuse that only dielectrics have. Both halves are
// scaled by pi so that, as with the plain diffuse material, a white light
// of brightness 1 shining straight at a surface shows the surface's own
// colour. The probe lights through maps made from it once: radiance
// prefiltered for each roughness, and the irradiance a normal gathers.

const MAX_LIGHTS: u32 = 16u;
const MAX_SHADOW_LAYERS: u32 = 24u;
const PI: f32 = 3.14159265;

struct Light {
    // xyz where it is; w its kind: 0 directional, 1 point, 2 spot.
    position: vec4<f32>,
    // xyz the way it shines; w the cosine of half the spot's cone.
    direction: vec4<f32>,
    // Linear colour times brightness; w the cosine of half the inner cone.
    color: vec4<f32>,
    // The fade's constant, linear and quadratic terms, already scaled.
    fade: vec4<f32>,
    // x its first shadow map, below 0 for none; y how many it has; z how
    // much light a shadow keeps away; w the bias, in scene units.
    shadow: vec4<f32>,
    // x the radius samples are spread over; y how many; z the share of a
    // cascade blended into the next.
    shadow2: vec4<f32>,
};

struct ShadowLayer {
    view_proj: mat4x4<f32>,
    // x the camera's view depth a cascade ends at, y where it starts.
    params: vec4<f32>,
};

struct Frame {
    view_proj: mat4x4<f32>,
    // xyz where the camera is; w 1 for an orthographic camera.
    camera: vec4<f32>,
    // xyz the way the camera looks.
    camera_dir: vec4<f32>,
    // rgb every light's ambient colour, summed; w unused.
    ambient: vec4<f32>,
    // x how many of `lights` are used.
    counts: vec4<u32>,
    // The inverse of the camera's projection and turn, for the sky box.
    sky: mat4x4<f32>,
    // Turns a direction in the scene into one in the probe.
    probe_rot: mat4x4<f32>,
    // x the probe's exposure, y its horizon, z the prefiltered map's last
    // level, w 1 where there is a probe.
    env: vec4<f32>,
    // See tone.wgsl; w the sky box's blur.
    tone: vec4<f32>,
    lights: array<Light, MAX_LIGHTS>,
    shadow_layers: array<ShadowLayer, MAX_SHADOW_LAYERS>,
};

struct Model {
    world: mat4x4<f32>,
    normal: mat4x4<f32>,
    // Linear and not premultiplied; a its opacity.
    base: vec4<f32>,
    // rgb linear emitted light.
    emissive: vec4<f32>,
    // x 1 where lit, y 1 for the metal-roughness model, z metalness,
    // w roughness.
    params: vec4<f32>,
    // x the mask's cutoff, below 0 where nothing is masked; y the plain
    // material's specular amount, z its specular roughness; w 1 where
    // shadows fall on it.
    params2: vec4<f32>,
    // Per map (base, metalness, roughness, normal, emissive, occlusion,
    // opacity), two rows taking the mesh's UV to the image's.
    uv: array<vec4<f32>, 14>,
    // x a bit per map that is there; y the channels of metalness (bits
    // 0-1), roughness (2-3), occlusion (4-5) and opacity (6-7); z a bit per
    // map read through the second UV set; w 1 where the joints of
    // `skin_joints` bend the mesh.
    maps: vec4<u32>,
    // x the normal map's strength, y how much the occlusion map shades.
    map_params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
// Every shadow map of the scene, a layer each.
@group(0) @binding(1) var shadow_maps: texture_depth_2d_array;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(1) @binding(0) var<uniform> model: Model;
// A skinned model's joints: each one's place times its inverse bind pose.
// 256 fill WebGL2's smallest uniform block.
@group(1) @binding(1) var<uniform> skin_joints: array<mat4x4<f32>, 256>;
// Colour maps are sRGB textures, read as linear light; the others as they
// are stored. A missing map is white.
@group(2) @binding(0) var base_map: texture_2d<f32>;
@group(2) @binding(1) var metalness_map: texture_2d<f32>;
@group(2) @binding(2) var roughness_map: texture_2d<f32>;
@group(2) @binding(3) var normal_map: texture_2d<f32>;
@group(2) @binding(4) var emissive_map: texture_2d<f32>;
@group(2) @binding(5) var occlusion_map: texture_2d<f32>;
@group(2) @binding(6) var opacity_map: texture_2d<f32>;
@group(2) @binding(7) var base_sampler: sampler;
@group(2) @binding(8) var metalness_sampler: sampler;
@group(2) @binding(9) var roughness_sampler: sampler;
@group(2) @binding(10) var normal_sampler: sampler;
@group(2) @binding(11) var emissive_sampler: sampler;
@group(2) @binding(12) var occlusion_sampler: sampler;
@group(2) @binding(13) var opacity_sampler: sampler;
@group(3) @binding(0) var specular_cube: texture_cube<f32>;
@group(3) @binding(1) var irradiance_cube: texture_cube<f32>;
@group(3) @binding(2) var cube_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // xyz along the image's u in the scene; w which way its v runs.
    @location(3) tangent: vec4<f32>,
    @location(4) uv1: vec2<f32>,
    // Linear, not premultiplied: the copy's colour, white for a model
    // drawn once.
    @location(5) color: vec4<f32>,
};

// One copy of a model: where it is, as the top three rows of a matrix put
// in front of the model's world transform, and its colour. A model drawn
// once is drawn as the one copy of an identity entry.
struct Instance {
    @location(4) row0: vec4<f32>,
    @location(5) row1: vec4<f32>,
    @location(6) row2: vec4<f32>,
    @location(7) color: vec4<f32>,
};

fn instance_matrix(i: Instance) -> mat4x4<f32> {
    return transpose(mat4x4<f32>(i.row0, i.row1, i.row2, vec4<f32>(0.0, 0.0, 0.0, 1.0)));
}

// What carries a normal through `m`: its inverse transpose up to a
// positive scale, without dividing by a determinant that may be 0.
fn cofactor(m: mat4x4<f32>) -> mat3x3<f32> {
    let a = m[0].xyz;
    let b = m[1].xyz;
    let c = m[2].xyz;
    let n = mat3x3<f32>(cross(b, c), cross(c, a), cross(a, b));
    return n * select(1.0, -1.0, dot(a, n[0]) < 0.0);
}

fn shade_vertex(
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    uv1: vec2<f32>,
    tangent: vec4<f32>,
    copy: Instance,
    skin: mat4x4<f32>,
) -> VsOut {
    var out: VsOut;
    let place = instance_matrix(copy);
    let m = place * model.world * skin;
    let world = m * vec4<f32>(position, 1.0);
    out.clip = frame.view_proj * world;
    let n = (model.normal * vec4<f32>(cofactor(skin) * normal, 0.0)).xyz;
    out.normal = cofactor(place) * n;
    out.world = world.xyz;
    out.uv = uv;
    out.uv1 = uv1;
    out.tangent = vec4<f32>((m * vec4<f32>(tangent.xyz, 0.0)).xyz, tangent.w);
    out.color = copy.color;
    return out;
}

const IDENTITY: mat4x4<f32> = mat4x4<f32>(
    vec4<f32>(1.0, 0.0, 0.0, 0.0),
    vec4<f32>(0.0, 1.0, 0.0, 0.0),
    vec4<f32>(0.0, 0.0, 1.0, 0.0),
    vec4<f32>(0.0, 0.0, 0.0, 1.0),
);

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    copy: Instance,
) -> VsOut {
    return shade_vertex(position, normal, uv, uv, tangent, copy, IDENTITY);
}

fn skin_matrix(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    if model.maps.w == 0u {
        return IDENTITY;
    }
    let j = min(joints, vec4<u32>(255u));
    return skin_joints[j.x] * weights.x + skin_joints[j.y] * weights.y
        + skin_joints[j.z] * weights.z + skin_joints[j.w] * weights.w;
}

// A mesh with a second UV set or joints: those come from a second buffer.
@vertex
fn vs_extra(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    copy: Instance,
    @location(8) uv1: vec2<f32>,
    @location(9) joints: vec4<u32>,
    @location(10) weights: vec4<f32>,
) -> VsOut {
    let skin = skin_matrix(joints, weights);
    return shade_vertex(position, normal, uv, uv1, tangent, copy, skin);
}

fn map_uv(i: u32, in: VsOut) -> vec2<f32> {
    let p = vec3<f32>(select(in.uv, in.uv1, (model.maps.z & (1u << i)) != 0u), 1.0);
    return vec2<f32>(dot(model.uv[i * 2u].xyz, p), dot(model.uv[i * 2u + 1u].xyz, p));
}

fn has_map(i: u32) -> bool {
    return (model.maps.x & (1u << i)) != 0u;
}

// The channel of `t` that a map of one value is read from, its two bits at
// `shift` in maps.y.
fn channel(t: vec4<f32>, shift: u32) -> f32 {
    let c = (model.maps.y >> shift) & 3u;
    if c == 0u {
        return t.r;
    }
    if c == 1u {
        return t.g;
    }
    if c == 2u {
        return t.b;
    }
    return t.a;
}

// Karis's fit of the split-sum's scale and bias for the Fresnel term,
// which spares a lookup table.
fn env_brdf(f0: vec3<f32>, roughness: f32, nv: f32) -> vec3<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + vec3<f32>(ab.y);
}

fn probe_dir(d: vec3<f32>) -> vec3<f32> {
    return (frame.probe_rot * vec4<f32>(d, 0.0)).xyz;
}

// GGX's share of microfacets turned to `nh`, for a squared roughness `a`.
fn ggx(nh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = nh * nh * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// Smith's shadowing and masking, each side in Schlick's form.
fn smith(nl: f32, nv: f32, roughness: f32) -> f32 {
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    return nl / (nl * (1.0 - k) + k) * (nv / (nv * (1.0 - k) + k));
}

// Perceptual roughness 0 would make a highlight of no width; Qt's smooth
// surfaces still show one.
fn squared(roughness: f32) -> f32 {
    let r = max(roughness, 0.045);
    return r * r;
}

// How much of a map's light reaches `p`: 1 outside the map.
fn shadow_tap(layer: u32, p: vec3<f32>) -> f32 {
    let clip = frame.shadow_layers[layer].view_proj * vec4<f32>(p, 1.0);
    if clip.w <= 0.0 {
        return 1.0;
    }
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z > 1.0 {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_maps, shadow_sampler, uv, layer, ndc.z);
}

// The cube face of a point light that `d`, from the light, falls in.
fn cube_face(d: vec3<f32>) -> u32 {
    let a = abs(d);
    if a.x >= a.y && a.x >= a.z {
        return select(1u, 0u, d.x > 0.0);
    }
    if a.y >= a.z {
        return select(3u, 2u, d.y > 0.0);
    }
    return select(5u, 4u, d.z > 0.0);
}

// The samples' spread over a unit disk, the first at its centre, so that
// fewer samples take the first of them and one is a hard shadow.
const TAPS = array<vec2<f32>, 16>(
    vec2<f32>(0.0, 0.0), vec2<f32>(0.7, 0.0), vec2<f32>(-0.7, 0.0), vec2<f32>(0.0, 0.7),
    vec2<f32>(0.0, -0.7), vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5),
    vec2<f32>(-0.5, 0.5), vec2<f32>(1.0, 0.0), vec2<f32>(-1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(0.0, -1.0), vec2<f32>(0.3, 0.9), vec2<f32>(-0.9, 0.3), vec2<f32>(0.9, -0.3),
);

// How much of one map's light reaches `p`, its samples spread across the
// light around it.
fn soft_tap(light: Light, layer: u32, p: vec3<f32>, l: vec3<f32>) -> f32 {
    let side = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(l.y) > 0.9);
    let t = normalize(cross(l, side));
    let b = cross(l, t);
    let n = clamp(u32(light.shadow2.y), 1u, 16u);
    let point = light.position.w > 0.5 && light.position.w < 1.5;
    var taps = TAPS;
    var sum = 0.0;
    for (var k = 0u; k < n; k++) {
        let o = taps[k] * light.shadow2.x;
        let q = p + t * o.x + b * o.y;
        var face = layer;
        if point {
            face = layer + cube_face(q - light.position.xyz);
        }
        sum += shadow_tap(face, q);
    }
    return sum / f32(n);
}

// What a light's shadow leaves of its light at `p`, lit along `l`.
fn shadowed(light: Light, p: vec3<f32>, l: vec3<f32>) -> f32 {
    if light.shadow.x < 0.0 || model.params2.w < 0.5 {
        return 1.0;
    }
    // Taken towards the light by the bias, a surface clears its own depth
    // in the map.
    let q = p + l * light.shadow.w;
    var layer = u32(light.shadow.x);
    var visible = 1.0;
    if light.position.w < 0.5 {
        // A directional light's cascades: the first that reaches as far
        // from the camera, blended into the next over its far end.
        let depth = dot(p - frame.camera.xyz, frame.camera_dir.xyz);
        let count = u32(light.shadow.y);
        var found = count;
        for (var c = 0u; c < count; c++) {
            if depth <= frame.shadow_layers[layer + c].params.x {
                found = c;
                break;
            }
        }
        if found == count {
            return 1.0;
        }
        layer += found;
        visible = soft_tap(light, layer, q, l);
        let range = frame.shadow_layers[layer].params;
        let blend = (range.x - range.y) * light.shadow2.z;
        if found + 1u < count && blend > 0.0 && depth > range.x - blend {
            let next = soft_tap(light, layer + 1u, q, l);
            visible = mix(visible, next, (depth - (range.x - blend)) / blend);
        }
    } else {
        visible = soft_tap(light, layer, q, l);
    }
    return 1.0 - light.shadow.z * (1.0 - visible);
}

@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Every map is read here, before anything branches, as sampling with
    // derivatives must be.
    let base_t = textureSample(base_map, base_sampler, map_uv(0u, in));
    let metal_t = textureSample(metalness_map, metalness_sampler, map_uv(1u, in));
    let rough_t = textureSample(roughness_map, roughness_sampler, map_uv(2u, in));
    let normal_t = textureSample(normal_map, normal_sampler, map_uv(3u, in));
    let emissive_t = textureSample(emissive_map, emissive_sampler, map_uv(4u, in));
    let occlusion_t = textureSample(occlusion_map, occlusion_sampler, map_uv(5u, in));
    let opacity_t = textureSample(opacity_map, opacity_sampler, map_uv(6u, in));

    let base = model.base * base_t * in.color;
    var a = base.a * channel(opacity_t, 6u);
    let cutoff = model.params2.x;
    if cutoff >= 0.0 {
        if a < cutoff {
            discard;
        }
        a = 1.0;
    }
    var rgb = base.rgb;
    if model.params.x > 0.5 {
        // A face seen from behind (with culling off) is lit as its front.
        var n = normalize(in.normal);
        if !front {
            n = -n;
        }
        if has_map(3u) {
            let t = normalize(in.tangent.xyz - n * dot(n, in.tangent.xyz));
            let b = in.tangent.w * cross(n, t);
            var m = normal_t.xyz * 2.0 - vec3<f32>(1.0);
            m = vec3<f32>(m.xy * model.map_params.x, m.z);
            n = normalize(t * m.x + b * m.y + n * m.z);
        }
        var v = -frame.camera_dir.xyz;
        if frame.camera.w < 0.5 {
            v = normalize(frame.camera.xyz - in.world);
        }
        let nv = max(dot(n, v), 1e-4);
        let principled = model.params.y > 0.5;
        let metal = model.params.z * channel(metal_t, 0u);
        let roughness = model.params.w * channel(rough_t, 2u);
        let ao = mix(1.0, channel(occlusion_t, 4u), model.map_params.y);
        let f0 = mix(vec3<f32>(0.04), base.rgb, metal);
        var sum = base.rgb * frame.ambient.rgb * ao;
        for (var i = 0u; i < min(frame.counts.x, MAX_LIGHTS); i++) {
            let light = frame.lights[i];
            var l = -light.direction.xyz;
            var radiance = light.color.rgb;
            if light.position.w > 0.5 {
                let to_light = light.position.xyz - in.world;
                let d = length(to_light);
                l = to_light / max(d, 1e-6);
                let fade = light.fade.x + light.fade.y * d + light.fade.z * d * d;
                radiance = radiance / max(fade, 1e-6);
                if light.position.w > 1.5 {
                    let c = dot(-l, light.direction.xyz);
                    let outer = light.direction.w;
                    let inner = light.color.w;
                    if inner > outer {
                        radiance = radiance * smoothstep(outer, inner, c);
                    } else if c < outer {
                        radiance = vec3<f32>(0.0);
                    }
                }
            }
            let nl = dot(n, l);
            if nl <= 0.0 {
                continue;
            }
            radiance = radiance * shadowed(light, in.world, l);
            let h = normalize(l + v);
            let nh = max(dot(n, h), 0.0);
            if principled {
                let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - max(dot(v, h), 0.0), 5.0);
                let spec = ggx(nh, squared(roughness)) * smith(nl, nv, roughness) / (4.0 * nl * nv);
                let kd = (vec3<f32>(1.0) - fresnel) * (1.0 - metal);
                sum += (kd * base.rgb + PI * spec * fresnel) * radiance * nl;
            } else {
                let rough = model.params2.z;
                let spec = model.params2.y * ggx(nh, squared(rough)) * smith(nl, nv, rough) / (4.0 * nl * nv);
                sum += (base.rgb + vec3<f32>(PI * spec)) * radiance * nl;
            }
        }
        if frame.env.w > 0.5 {
            // probeHorizon darkens what comes from below, in the scene's
            // up, whichever way the probe is turned: all of it straight
            // down, none of it level.
            let horizon = frame.env.y;
            let irradiance = textureSampleLevel(irradiance_cube, cube_sampler, probe_dir(n), 0.0).rgb
                * (1.0 - horizon * (1.0 - n.y) * 0.5);
            var indirect = vec3<f32>(0.0);
            if principled {
                let r = reflect(-v, n);
                let lod = roughness * frame.env.z;
                let below = mix(select(0.0, 1.0, r.y < 0.0), (1.0 - r.y) * 0.5, roughness);
                let prefiltered = textureSampleLevel(specular_cube, cube_sampler, probe_dir(r), lod).rgb
                    * (1.0 - horizon * below);
                let specular = env_brdf(f0, roughness, nv);
                let kd = (vec3<f32>(1.0) - specular) * (1.0 - metal);
                indirect = kd * base.rgb * irradiance + prefiltered * specular;
            } else {
                indirect = base.rgb * irradiance;
            }
            sum += indirect * ao * frame.env.x;
        }
        var emissive = model.emissive.rgb;
        if has_map(4u) {
            emissive = emissive * emissive_t.rgb;
        }
        rgb = sum + emissive;
    }
    return vec4<f32>(tonemap(rgb, frame.tone) * a, a);
}
