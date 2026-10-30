// A shadow map's depth pass: each caster drawn from the light, keeping
// only its depth. The model's uniforms, its copies and its joints are the
// lit shader's; only where the mesh ends up is read.

struct Layer {
    view_proj: mat4x4<f32>,
    params: vec4<f32>,
};

// lit.wgsl's, up to the field read here.
struct Model {
    world: mat4x4<f32>,
    normal: mat4x4<f32>,
    base: vec4<f32>,
    emissive: vec4<f32>,
    params: vec4<f32>,
    params2: vec4<f32>,
    uv: array<vec4<f32>, 14>,
    maps: vec4<u32>,
};

@group(0) @binding(0) var<uniform> layer: Layer;
@group(1) @binding(0) var<uniform> model: Model;
@group(1) @binding(1) var<uniform> skin_joints: array<mat4x4<f32>, 256>;

fn place(row0: vec4<f32>, row1: vec4<f32>, row2: vec4<f32>) -> mat4x4<f32> {
    return transpose(mat4x4<f32>(row0, row1, row2, vec4<f32>(0.0, 0.0, 0.0, 1.0)));
}

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(4) row0: vec4<f32>,
    @location(5) row1: vec4<f32>,
    @location(6) row2: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    return layer.view_proj * place(row0, row1, row2) * model.world * vec4<f32>(position, 1.0);
}

@vertex
fn vs_extra(
    @location(0) position: vec3<f32>,
    @location(4) row0: vec4<f32>,
    @location(5) row1: vec4<f32>,
    @location(6) row2: vec4<f32>,
    @location(9) joints: vec4<u32>,
    @location(10) weights: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    var p = vec4<f32>(position, 1.0);
    if model.maps.w != 0u {
        let j = min(joints, vec4<u32>(255u));
        p = (skin_joints[j.x] * weights.x + skin_joints[j.y] * weights.y
            + skin_joints[j.z] * weights.z + skin_joints[j.w] * weights.w) * p;
    }
    return layer.view_proj * place(row0, row1, row2) * model.world * p;
}

// GL links no program without a fragment stage.
@fragment
fn fs_main() {}
