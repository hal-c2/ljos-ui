// The sky box: the light probe behind everything, drawn first over the
// whole picture. Each pixel looks along the ray the camera sees through
// it, turned into the probe, and reads the prefiltered map, at a rougher
// level the more the sky is blurred. tone.wgsl is put before this file.

// The front of lit.wgsl's frame, up to what the sky reads.
struct Frame {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    camera_dir: vec4<f32>,
    ambient: vec4<f32>,
    counts: vec4<u32>,
    sky: mat4x4<f32>,
    probe_rot: mat4x4<f32>,
    env: vec4<f32>,
    tone: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(3) @binding(0) var specular_cube: texture_cube<f32>;
@group(3) @binding(2) var cube_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VsOut;
    out.ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    out.clip = vec4<f32>(out.ndc, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // From the near plane to the far one: the ray for a perspective camera
    // and an orthographic one alike.
    let near = frame.sky * vec4<f32>(in.ndc, 0.0, 1.0);
    let far = frame.sky * vec4<f32>(in.ndc, 1.0, 1.0);
    let d = far.xyz / far.w - near.xyz / near.w;
    let p = (frame.probe_rot * vec4<f32>(d, 0.0)).xyz;
    let lod = frame.tone.w * frame.env.z;
    let light = textureSampleLevel(specular_cube, cube_sampler, p, lod).rgb * frame.env.x;
    return vec4<f32>(tonemap(light, frame.tone), 1.0);
}
