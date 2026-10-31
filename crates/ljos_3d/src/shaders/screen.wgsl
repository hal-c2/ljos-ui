// SCREEN_TEXTURE: the scene drawn so far, copied for the materials that
// read it, and its mip levels, each from the one above. A picture drawn
// for post-processing holds linear light, which is encoded on the way, so
// that a material reads the screen as it reads a picture drawn without:
// as the sRGB its own FRAGCOLOR is written in.

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VsOut;
    out.clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// Level 0 at the source's size samples texel centres, so it copies; a
// smaller level averages the four texels of the one above.
@fragment
fn fs_copy(in: VsOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(source, source_sampler, in.uv, 0.0);
}

// Premultiplied, as the picture is: the colour is encoded, not its
// product with the coverage.
@fragment
fn fs_encode(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(source, source_sampler, in.uv, 0.0);
    if c.a <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(srgb3(c.rgb / c.a) * c.a, c.a);
}
