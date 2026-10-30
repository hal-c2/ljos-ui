// DefaultMaterial lit by one directional light. Light is summed in linear
// space and written as sRGB, premultiplied, which is how the 2D scene that
// shows the picture blends.

struct Frame {
    view_proj: mat4x4<f32>,
    // The way the light shines, in world space; w unused.
    light_dir: vec4<f32>,
    // Linear colour times brightness; w is 1 when there is a light.
    light_color: vec4<f32>,
    ambient: vec4<f32>,
};

struct Model {
    world: mat4x4<f32>,
    normal: mat4x4<f32>,
    // Linear, not premultiplied.
    diffuse: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> model: Model;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = frame.view_proj * model.world * vec4<f32>(position, 1.0);
    out.normal = (model.normal * vec4<f32>(normal, 0.0)).xyz;
    return out;
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let lambert = max(dot(n, -frame.light_dir.xyz), 0.0) * frame.light_color.w;
    let lit = model.diffuse.rgb * (frame.light_color.rgb * lambert + frame.ambient.rgb);
    let a = model.diffuse.a;
    let rgb = vec3<f32>(to_srgb(clamp(lit.r, 0.0, 1.0)), to_srgb(clamp(lit.g, 0.0, 1.0)), to_srgb(clamp(lit.b, 0.0, 1.0)));
    return vec4<f32>(rgb * a, a);
}
