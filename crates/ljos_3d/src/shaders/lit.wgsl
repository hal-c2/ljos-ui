// Every material the scene has, lit by up to MAX_LIGHTS lights. Light is
// summed in linear space and written as sRGB, premultiplied, which is how
// the 2D scene that shows the picture blends.
//
// The metal-roughness model is the textbook one: a GGX distribution of
// microfacets, Smith's shadowing in Schlick's form and Schlick's Fresnel,
// over a Lambert diffuse that only dielectrics have. Both halves are
// scaled by pi so that, as with the plain diffuse material, a white light
// of brightness 1 shining straight at a surface shows the surface's own
// colour.

const MAX_LIGHTS: u32 = 16u;
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
    lights: array<Light, MAX_LIGHTS>,
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
    // material's specular amount, z its specular roughness.
    params2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> model: Model;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    let world = model.world * vec4<f32>(position, 1.0);
    out.clip = frame.view_proj * world;
    out.normal = (model.normal * vec4<f32>(normal, 0.0)).xyz;
    out.world = world.xyz;
    return out;
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
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

@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var a = model.base.a;
    let cutoff = model.params2.x;
    if cutoff >= 0.0 {
        if a < cutoff {
            discard;
        }
        a = 1.0;
    }
    var rgb = model.base.rgb;
    if model.params.x > 0.5 {
        // A face seen from behind (with culling off) is lit as its front.
        var n = normalize(in.normal);
        if !front {
            n = -n;
        }
        var v = -frame.camera_dir.xyz;
        if frame.camera.w < 0.5 {
            v = normalize(frame.camera.xyz - in.world);
        }
        let nv = max(dot(n, v), 1e-4);
        let principled = model.params.y > 0.5;
        let metal = model.params.z;
        let roughness = model.params.w;
        let f0 = mix(vec3<f32>(0.04), model.base.rgb, metal);
        var sum = model.base.rgb * frame.ambient.rgb;
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
            let h = normalize(l + v);
            let nh = max(dot(n, h), 0.0);
            if principled {
                let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - max(dot(v, h), 0.0), 5.0);
                let spec = ggx(nh, squared(roughness)) * smith(nl, nv, roughness) / (4.0 * nl * nv);
                let kd = (vec3<f32>(1.0) - fresnel) * (1.0 - metal);
                sum += (kd * model.base.rgb + PI * spec * fresnel) * radiance * nl;
            } else {
                let rough = model.params2.z;
                let spec = model.params2.y * ggx(nh, squared(rough)) * smith(nl, nv, rough) / (4.0 * nl * nv);
                sum += (model.base.rgb + vec3<f32>(PI * spec)) * radiance * nl;
            }
        }
        rgb = sum + model.emissive.rgb;
    }
    let c = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(vec3<f32>(to_srgb(c.r), to_srgb(c.g), to_srgb(c.b)) * a, a);
}
