// A shadow map's depth pass: each caster drawn from the light, keeping
// only its depth. The model's uniforms are the lit shader's, of which
// only the first matrix is read.

struct Layer {
    view_proj: mat4x4<f32>,
    params: vec4<f32>,
};

struct Model {
    world: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> layer: Layer;
@group(1) @binding(0) var<uniform> model: Model;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return layer.view_proj * model.world * vec4<f32>(position, 1.0);
}

// GL links no program without a fragment stage.
@fragment
fn fs_main() {}
