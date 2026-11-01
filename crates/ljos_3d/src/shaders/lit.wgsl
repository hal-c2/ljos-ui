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
    // rgb the fog's linear colour; w its density.
    fog: vec4<f32>,
    // Where the distance's fog starts and is whole, its curve, 1 when on.
    fog_depth: vec4<f32>,
    // The y of the least and of the most, the curve, 1 when on.
    fog_height: vec4<f32>,
    // x the transmission's curve, y 1 when on; z 1 where highlights are
    // widened against aliasing.
    fog_transmit: vec4<f32>,
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
    // opacity, transmission, height), two rows taking the mesh's UV to the
    // image's.
    uv: array<vec4<f32>, 18>,
    // x a bit per map that is there; y the channels of metalness (bits
    // 0-1), roughness (2-3), occlusion (4-5) and opacity (6-7); z a bit per
    // map read through the second UV set; w 1 where the joints of
    // `skin_joints` bend the mesh.
    maps: vec4<u32>,
    // x the normal map's strength, y how much the occlusion map shades, z the
    // node's opacity with its parents'.
    map_params: vec4<f32>,
    // x how much of a dielectric's reflection there is, y how far towards
    // the base colour it is tinted, z what it is head on, w the Fresnel
    // power; a plain material has the power alone.
    shine: vec4<f32>,
    // x the Fresnel scale, y its bias, z the clearcoat's amount, w its
    // roughness.
    edge: vec4<f32>,
    // x the clearcoat's Fresnel power, y its scale, z its bias; w how much of
    // the diffuse light gives way to what is behind.
    coat_edge: vec4<f32>,
    // rgb a plain material's linear specular tint; w the height amount.
    tint: vec4<f32>,
    // x a bit per light that lights it; y 1 to use the vertices' colours; z
    // the fewest and most height samples, low and high half; w the channels
    // of transmission (bits 0-1) and height (2-3).
    flags: vec4<u32>,
    // rgb the ambient of the lights scoped to it.
    ambient: vec4<f32>,
    // x its own probe's last level, y 1 where it has one, z its exposure.
    // ... y 2 where that is a reflection probe's, and x is its levels.
    probe: vec4<f32>,
    // Where a reflection probe looked from, w 1 where what is read of it is
    // put right; and the corners of its box.
    mirror_at: vec4<f32>,
    mirror_min: vec4<f32>,
    mirror_max: vec4<f32>,
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
@group(2) @binding(7) var transmission_map: texture_2d<f32>;
@group(2) @binding(8) var height_map: texture_2d<f32>;
@group(2) @binding(9) var base_sampler: sampler;
@group(2) @binding(10) var metalness_sampler: sampler;
@group(2) @binding(11) var roughness_sampler: sampler;
@group(2) @binding(12) var normal_sampler: sampler;
@group(2) @binding(13) var emissive_sampler: sampler;
@group(2) @binding(14) var occlusion_sampler: sampler;
@group(2) @binding(15) var opacity_sampler: sampler;
@group(2) @binding(16) var transmission_sampler: sampler;
@group(2) @binding(17) var height_sampler: sampler;
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

// A mesh with a second UV set, joints or colours: those come from a second
// buffer.
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
    @location(12) color: vec4<f32>,
) -> VsOut {
    let skin = skin_matrix(joints, weights);
    var out = shade_vertex(position, normal, uv, uv1, tangent, copy, skin);
    if model.flags.y != 0u {
        out.color = out.color * color;
    }
    return out;
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

// One of four channels, numbered as a `TextureChannelMapping`.
fn channel_of(t: vec4<f32>, c: u32) -> f32 {
    return t[c & 3u];
}

// How much a surface reflects at the cosine `c`, from `f0` head on.
fn schlick(f0: vec3<f32>, c: f32, power: f32, scale: f32, bias: f32) -> vec3<f32> {
    let edge = pow(max(1.0 - c, 1e-4), power);
    return f0 + (vec3<f32>(1.0) - f0) * clamp(bias + scale * edge, 0.0, 1.0);
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

// The fog over a lit surface's light `lit` at `p`, by distance and by height.
fn eased(from_: f32, to: f32, at: f32) -> f32 {
    let t = clamp((at - from_) / max(abs(to - from_), 1e-6) * sign(to - from_), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn fogged(lit: vec3<f32>, p: vec3<f32>) -> vec3<f32> {
    if frame.fog_depth.w < 0.5 && frame.fog_height.w < 0.5 {
        return lit;
    }
    var color = frame.fog.rgb;
    var amount = 0.0;
    if frame.fog_depth.w > 0.5 {
        let far = eased(frame.fog_depth.x, frame.fog_depth.y, length(frame.camera.xyz - p));
        amount = pow(far, frame.fog_depth.z) * frame.fog.w;
        if frame.fog_transmit.y > 0.5 {
            color = mix(max(lit, color), color, pow(far, frame.fog_transmit.x));
        }
    }
    if frame.fog_height.w > 0.5 {
        // By its height a fog is as thick as it is, whatever its density
        // and its curve say.
        amount = max(amount, eased(frame.fog_height.x, frame.fog_height.y, p.y));
    }
    return mix(lit, color, clamp(amount, 0.0, 1.0));
}

// Where a height map has the eye see, walking along the view inside the
// surface until it is below the map's height.
fn carved(in: VsOut, front: bool) -> VsOut {
    var at = in;
    if !has_map(8u) || model.tint.w == 0.0 {
        return at;
    }
    var n = normalize(in.normal);
    if !front {
        n = -n;
    }
    let t = normalize(in.tangent.xyz - n * dot(n, in.tangent.xyz));
    let b = in.tangent.w * cross(n, t);
    var v = -frame.camera_dir.xyz;
    if frame.camera.w < 0.5 {
        v = normalize(frame.camera.xyz - in.world);
    }
    let seen = vec3<f32>(dot(v, t), dot(v, b), max(dot(v, n), 0.05));
    let fewest = f32(model.flags.z & 0xffffu);
    let most = f32(model.flags.z >> 16u);
    let layers = max(mix(most, fewest, seen.z), 1.0);
    let step_ = -seen.xy / seen.z * model.tint.w / layers;
    let shift = (model.flags.w >> 2u) & 3u;
    let uv = map_uv(8u, in);
    var off = vec2<f32>(0.0);
    var depth = 0.0;
    for (var k = 0u; k < u32(layers); k++) {
        let h = channel_of(textureSampleLevel(height_map, height_sampler, uv + off, 0.0), shift);
        if depth >= 1.0 - h {
            break;
        }
        depth += 1.0 / layers;
        off += step_;
    }
    at.uv = at.uv + off;
    at.uv1 = at.uv1 + off;
    return at;
}

fn probe_dir(d: vec3<f32>) -> vec3<f32> {
    return (frame.probe_rot * vec4<f32>(d, 0.0)).xyz;
}

// The way a reflection probe's cube is read for what lies a way from a
// surface: that way, or, put right, the way from where the probe looked to
// where a line that way from the surface leaves the probe's box.
fn righted(way: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if model.mirror_at.w < 0.5 {
        return way;
    }
    let furthest = max((model.mirror_max.xyz - world) / way, (model.mirror_min.xyz - world) / way);
    return world + way * min(min(furthest.x, furthest.y), furthest.z) - model.mirror_at.xyz;
}

fn fifth(value: f32) -> f32 {
    let n = 1.0 - value;
    let n2 = n * n;
    return n2 * n2 * n;
}

fn half_seen(cosine: f32, alpha: f32) -> f32 {
    let k = 0.5 * alpha;
    return 0.5 / (cosine * (1.0 - k) + k);
}

// How much of a light a rough surface gives back all round: Burley's, which
// is what Qt's PrincipledMaterial has.
fn burley(n: vec3<f32>, l: vec3<f32>, v: vec3<f32>, roughness: f32) -> f32 {
    let h = normalize(v + l);
    let lh = max(0.0, dot(l, h));
    let nv = max(0.0, dot(n, v));
    let nl = max(0.0, dot(n, l));
    let at90 = 0.5 + 2.0 * lh * lh * roughness;
    return mix(1.0, at90, fifth(nl)) * mix(1.0, at90, fifth(nv)) * nl;
}

// And how much it gives back as the light's own shine: GGX, not scaled by pi
// the way the diffuse half isn't divided by it, as Qt sums the two.
fn microfacets(n: vec3<f32>, l: vec3<f32>, v: vec3<f32>, f0: vec3<f32>, roughness: f32) -> vec3<f32> {
    let nl = max(0.0, dot(n, l));
    let nv = max(0.0, dot(n, v));
    let h = normalize(v + l);
    let lh = max(0.0, dot(l, h));
    let nh = max(0.001, dot(n, h));
    let alpha = clamp(roughness * roughness, 0.001, 1.0);
    let alpha2 = alpha * alpha;
    let d = 1.0 + (alpha2 - 1.0) * nh * nh;
    let spread = alpha2 / (PI * d * d);
    let seen = half_seen(nl, alpha) * half_seen(nv, alpha);
    return nl * spread * mix(vec3<f32>(fifth(lh)), vec3<f32>(1.0), f0) * seen;
}

// Qt's exposure saturates what a probe gives, ahead of the tone map; without
// a tone map the light is as it is.
fn probe_seen(read: vec3<f32>, own: bool, exposure: f32) -> vec3<f32> {
    if own || frame.tone.x < 0.5 {
        return read;
    }
    return vec3<f32>(1.0) - exp(-read * exposure);
}

// How much of a probe is left along a way, turned as the probe is, by how far
// its horizon reaches: all of it at 0; at 1 nothing below it and a soft edge.
fn under_horizon(way: vec3<f32>, horizon: f32) -> f32 {
    if horizon <= 0.0 {
        return 1.0;
    }
    let low = min(horizon - 1.0, -0.001);
    let middle = 0.5 + 0.5 * low;
    return mix(1.0, smoothstep(middle * 0.25, middle + 0.25, way.y), low + 1.0);
}

// What the probe gives along a way at a level.
fn probe_light(way: vec3<f32>, level: f32, own: bool, horizon: f32) -> vec3<f32> {
    let turned = select(probe_dir(way), way, own);
    return textureSampleLevel(specular_cube, cube_sampler, turned, level).rgb
        * under_horizon(turned, horizon);
}

// The same, as it is seen.
fn probe_read(way: vec3<f32>, level: f32, own: bool, horizon: f32, exposure: f32) -> vec3<f32> {
    return probe_seen(probe_light(way, level, own, horizon), own, exposure);
}

// What of a surface's light comes back from all round at an edge a material
// draws its own way: a CustomMaterial's FRESNEL_POWER and the like.
fn env_edge(f0: vec3<f32>, roughness: f32, nv: f32, power: f32, scale: f32, bias: f32) -> vec3<f32> {
    if power == 5.0 && scale == 1.0 && bias == 0.0 {
        return env_brdf(f0, roughness, nv);
    }
    let top = max(vec3<f32>(1.0 - roughness), f0);
    let edge = pow(max(1.0 - nv, 1e-4), power);
    return clamp(vec3<f32>(bias) + scale * (f0 + (top - f0) * edge), vec3<f32>(0.0), vec3<f32>(1.0));
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
fn fs_main(raw: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let in = carved(raw, front);
    // Every map is read here, before anything branches, as sampling with
    // derivatives must be.
    let base_t = textureSample(base_map, base_sampler, map_uv(0u, in));
    let metal_t = textureSample(metalness_map, metalness_sampler, map_uv(1u, in));
    let rough_t = textureSample(roughness_map, roughness_sampler, map_uv(2u, in));
    let normal_t = textureSample(normal_map, normal_sampler, map_uv(3u, in));
    let emissive_t = textureSample(emissive_map, emissive_sampler, map_uv(4u, in));
    let occlusion_t = textureSample(occlusion_map, occlusion_sampler, map_uv(5u, in));
    let opacity_t = textureSample(opacity_map, opacity_sampler, map_uv(6u, in));
    let transmission_t = textureSample(transmission_map, transmission_sampler, map_uv(7u, in));
    // Normalized first: a scaled node's normals come here longer or shorter.
    let unit_normal = normalize(in.normal);
    let normal_dx = dpdx(unit_normal);
    let normal_dy = dpdy(unit_normal);

    let base = model.base * base_t * in.color;
    var a = base.a * channel(opacity_t, 6u);
    let cutoff = model.params2.x;
    if cutoff >= 0.0 {
        if a < cutoff {
            discard;
        }
        a = 1.0;
    }
    // After the mask: a node's fade is not a material's cut-out.
    a *= model.map_params.z;
    let through = model.coat_edge.w * channel_of(transmission_t, model.flags.w);
    var rgb = base.rgb * (1.0 - through);
    if model.params.x > 0.5 {
        // A face seen from behind (with culling off) is lit as its front.
        var n = normalize(in.normal);
        if !front {
            n = -n;
        }
        // The clearcoat lies over the mesh's own shape, under no normal map.
        let coat_n = n;
        if has_map(3u) {
            let t = normalize(in.tangent.xyz - n * dot(n, in.tangent.xyz));
            let b = in.tangent.w * cross(n, t);
            var m = normal_t.xyz * 2.0 - vec3<f32>(1.0);
            m = vec3<f32>(m.xy * model.map_params.x, m.z);
            n = normalize(t * m.x + b * m.y + n * m.z);
        }
        // From where the camera is, even for a flat projection: Qt's sums do
        // not know a camera's kind, and a highlight runs across a plane.
        let v = normalize(frame.camera.xyz - in.world);
        let nv = max(dot(n, v), 1e-4);
        let principled = model.params.y > 0.5;
        let metal = model.params.z * channel(metal_t, 0u);
        var roughness = model.params.w * channel(rough_t, 2u);
        var plain_rough = model.params2.z * channel(rough_t, 2u);
        if frame.fog_transmit.z > 0.5 {
            // Tokuyoshi and Kaplanyan: the variance of the normal across a
            // pixel widens the distribution, so a highlight smaller than a
            // pixel is dimmed rather than flickering.
            let kernel = min(0.5 * (dot(normal_dx, normal_dx) + dot(normal_dy, normal_dy)), 0.18);
            roughness = pow(clamp(pow(squared(roughness), 2.0) + kernel, 0.0, 1.0), 0.25);
            plain_rough = pow(clamp(pow(squared(plain_rough), 2.0) + kernel, 0.0, 1.0), 0.25);
        }
        let ao = mix(1.0, channel(occlusion_t, 4u), model.map_params.y);
        let shine = model.shine;
        let bent = vec3<f32>(shine.z);
        let f0 = mix(bent, base.rgb, metal);
        // What a surface gives back of its surroundings as it turns from
        // the eye, before each material's kind says whose colour that is.
        let edge = select(pow(max(1.0 - nv, 1e-4), shine.w), 1.0, shine.w == 0.0);
        let rough_here = select(plain_rough, roughness, principled);
        let turning = f0 + (max(vec3<f32>(1.0 - rough_here), f0) - f0) * edge;
        // What is not metal gives a light back as it is, or as far towards
        // its own colour as the tint says; a metal in its colour alone, by
        // its f0. Of the amount the lights heed only whether there is any.
        let tint = mix(vec3<f32>(1.0), mix(vec3<f32>(1.0), base.rgb, shine.y), 1.0 - metal);
        let shiny = shine.x > 0.01 || metal > 0.01;
        var amount = base.rgb * model.params2.y * metal_t.rgb * turning;
        if principled {
            amount = vec3<f32>(metal + shine.x * (1.0 - metal))
                * clamp(vec3<f32>(model.edge.y) + model.edge.x * turning, vec3<f32>(0.0), vec3<f32>(1.0));
        }
        let plain_shine = 2.56 / (plain_rough + 0.01);
        let coat = model.edge.z;
        let coat_rough = model.edge.w;
        let coats = principled && coat > 0.0;
        var coating = vec3<f32>(0.0);
        var diffuse = base.rgb * (frame.ambient.rgb + model.ambient.rgb);
        if principled {
            diffuse *= 1.0 - metal;
        }
        var glossy = vec3<f32>(0.0);
        for (var i = 0u; i < min(frame.counts.x, MAX_LIGHTS); i++) {
            if (model.flags.x & (1u << i)) == 0u {
                continue;
            }
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
            if principled {
                diffuse += base.rgb * radiance * (1.0 - metal) * burley(n, l, v, roughness);
                if shiny {
                    glossy += radiance * tint * microfacets(n, l, v, f0, roughness);
                }
                if coats {
                    coating += radiance * microfacets(coat_n, l, v, bent, coat_rough);
                }
            } else {
                diffuse += base.rgb * radiance * nl;
                if model.params2.y > 0.0 {
                    glossy += radiance * amount * pow(max(dot(n, h), 0.0), plain_shine);
                }
            }
        }
        // What a light gives all round is less where less of it reaches.
        diffuse *= ao;
        let own_probe = model.probe.y > 0.5;
        if model.probe.y > 1.5 {
            // What is round a reflection probe is in place of the
            // surroundings: neither dimmed below a horizon nor by what less
            // light reaches.
            let round = textureSampleLevel(irradiance_cube, cube_sampler, righted(n, in.world), 0.0).rgb;
            let r = normalize(reflect(-v, n));
            let last = model.probe.x - 1.0;
            if principled {
                diffuse += base.rgb * (vec3<f32>(1.0) - amount) * round;
                glossy += tint * env_brdf(amount, roughness, nv)
                    * textureSampleLevel(specular_cube, cube_sampler, righted(r, in.world), clamp(roughness * 5.0, 0.0, last)).rgb;
                if coats {
                    coating += env_brdf(bent, coat_rough, max(dot(coat_n, v), 1e-4))
                        * textureSampleLevel(specular_cube, cube_sampler, righted(normalize(reflect(-v, coat_n)), in.world), clamp(coat_rough * 5.0, 0.0, last)).rgb;
                }
            } else {
                let sigma = smoothstep(0.0, 1.0, clamp(plain_rough, 1e-4, 1.0));
                let along = clamp(dot(r, n), 0.0, 0.999995);
                let k = sigma * 0.31830988618;
                let seen = clamp((along / (along * (1.0 - k) + k) + (1.0 - k * k)) * 0.5, 0.0, 1.0);
                diffuse += base.rgb * round;
                glossy += amount * model.tint.rgb * seen
                    * textureSampleLevel(specular_cube, cube_sampler, righted(r, in.world), sigma * last).rgb;
            }
        } else if frame.env.w > 0.5 || own_probe {
            // A material's own probe is as it is: not turned, and without
            // the environment's horizon.
            let exposure = select(frame.env.x, model.probe.z, own_probe);
            let last = select(frame.env.z, model.probe.x, own_probe);
            let horizon = select(frame.env.y, 0.0, own_probe);
            let round = probe_seen(
                textureSampleLevel(irradiance_cube, cube_sampler, select(probe_dir(n), n, own_probe), 0.0).rgb
                    * under_horizon(probe_dir(n), horizon),
                own_probe,
                exposure,
            );
            let r = normalize(reflect(-v, n));
            if principled {
                // Qt exposes the light that leaves the surface, not what falls on it.
                let read = probe_light(r, clamp(roughness * 5.0, 0.0, last), own_probe, horizon);
                diffuse += base.rgb * (vec3<f32>(1.0) - amount) * round * ao;
                glossy += tint * probe_seen(read * env_brdf(amount, roughness, nv), own_probe, exposure) * ao;
                if coats {
                    let coat_r = normalize(reflect(-v, coat_n));
                    coating += probe_seen(
                        probe_light(coat_r, clamp(coat_rough * 5.0, 0.0, last), own_probe, horizon)
                            * env_brdf(bent, coat_rough, max(dot(coat_n, v), 1e-4)),
                        own_probe,
                        exposure,
                    ) * ao;
                }
            } else {
                let sigma = smoothstep(0.0, 1.0, clamp(plain_rough, 1e-4, 1.0));
                let along = clamp(dot(r, n), 0.0, 0.999995);
                let k = sigma * 0.31830988618;
                let seen = clamp((along / (along * (1.0 - k) + k) + (1.0 - k * k)) * 0.5, 0.0, 1.0);
                diffuse += base.rgb * round * ao;
                glossy += amount * model.tint.rgb * seen
                    * probe_read(r, sigma * last, own_probe, horizon, exposure) * ao;
            }
        }
        if principled {
            // Qt takes what is metal out of the light above, and out of the
            // sum of it once more here.
            diffuse *= 1.0 - metal;
        }
        var emissive = model.emissive.rgb;
        if has_map(4u) {
            emissive = emissive * emissive_t.rgb;
        }
        var lit = fogged(diffuse * (1.0 - through) + glossy + emissive, in.world);
        if coats {
            // What is under a clear coat shows less the more the coat gives
            // back, which is more the further it is turned from the eye.
            let turn = clamp(pow(clamp(dot(coat_n, v), 0.0, 1.0), model.coat_edge.x), 0.0, 1.0);
            let back = clamp(
                vec3<f32>(model.coat_edge.z) + model.coat_edge.y * (bent + (vec3<f32>(1.0) - bent) * (1.0 - turn)),
                vec3<f32>(0.0),
                vec3<f32>(1.0),
            );
            lit = lit * (vec3<f32>(1.0) - coat * back) + coating * coat;
        }
        rgb = lit;
    }
    return vec4<f32>(tonemap(rgb, frame.tone) * a, a * (1.0 - through));
}

struct SurfaceOut {
    // The normal in the scene's space; w the roughness, 2 for a surface
    // that reflects nothing and 3 for one that is not lit.
    @location(0) normal: vec4<f32>,
    // The base colour in linear light; w the metalness.
    @location(1) material: vec4<f32>,
};

// What the screen-space reflections and global illumination read of a
// surface, drawn where the picture has it.
@fragment
fn fs_surface(in: VsOut, @builtin(front_facing) front: bool) -> SurfaceOut {
    let base_t = textureSample(base_map, base_sampler, map_uv(0u, in));
    let metal_t = textureSample(metalness_map, metalness_sampler, map_uv(1u, in));
    let rough_t = textureSample(roughness_map, roughness_sampler, map_uv(2u, in));
    let normal_t = textureSample(normal_map, normal_sampler, map_uv(3u, in));
    let opacity_t = textureSample(opacity_map, opacity_sampler, map_uv(6u, in));
    let base = model.base * base_t * in.color;
    if model.params2.x >= 0.0 && base.a * channel(opacity_t, 6u) < model.params2.x {
        discard;
    }
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
    var out: SurfaceOut;
    if model.params.x < 0.5 {
        out.normal = vec4<f32>(n, 3.0);
        out.material = vec4<f32>(0.0);
        return out;
    }
    if model.params.y > 0.5 {
        let roughness = clamp(model.params.w * channel(rough_t, 2u), 0.0, 1.0);
        out.normal = vec4<f32>(n, roughness);
        out.material = vec4<f32>(base.rgb, clamp(model.params.z * channel(metal_t, 0u), 0.0, 1.0));
    } else {
        out.normal = vec4<f32>(n, 2.0);
        out.material = vec4<f32>(base.rgb, 0.0);
    }
    return out;
}

// A custom material's surface: its shading is its own, so it is taken as
// not lit, and neither effect changes it.
@fragment
fn fs_surface_unlit(in: VsOut) -> SurfaceOut {
    var out: SurfaceOut;
    out.normal = vec4<f32>(normalize(in.normal), 3.0);
    out.material = vec4<f32>(0.0);
    return out;
}
