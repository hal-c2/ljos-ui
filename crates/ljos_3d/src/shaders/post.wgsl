// The built-in passes over a scene drawn in linear light: ambient
// occlusion, depth of field, glow, and the last pass, which tonemaps and
// grades the picture into the target. Every pass draws one triangle over
// its output; `a` is what it reads, `b` a second picture and `depth` the
// scene's depth where a pass needs it. Colours are premultiplied.

struct Post {
    // The inverse of the camera's projection, and the projection.
    inverse: mat4x4<f32>,
    projection: mat4x4<f32>,
    // xy a texel of `a`, zw the blur's direction in texels.
    texel: vec4<f32>,
    // The tonemapping mode, the exposure, the white point, and 1 to dither.
    tone: vec4<f32>,
    // Strength (0 to 1), distance, softness in pixels, samples.
    ao: vec4<f32>,
    // Bias, 1 to dither.
    ao2: vec4<f32>,
    // Focus distance, focus range, blur in pixels.
    dof: vec4<f32>,
    // Strength, intensity, bloom, blend mode.
    glow: vec4<f32>,
    // HDR minimum, maximum and scale, 1 for the better downsample.
    glow2: vec4<f32>,
    // Brightness, contrast, saturation, 1 where enabled.
    adjust: vec4<f32>,
    // The vignette's colour in linear light, 1 where enabled.
    vignette: vec4<f32>,
    // Radius, strength.
    vignette2: vec4<f32>,
    // The table's size, its alpha, 1 where there is a table; sharpness.
    lut: vec4<f32>,
};

@group(0) @binding(0) var<uniform> post: Post;
@group(0) @binding(1) var a: texture_2d<f32>;
@group(0) @binding(2) var b: texture_2d<f32>;
@group(0) @binding(3) var depth: texture_2d<f32>;
@group(0) @binding(4) var smooth_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VsOut;
    out.clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

// Where a pixel of the depth is, seen from the camera.
fn view_at(px: vec2<i32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(depth));
    let p = clamp(px, vec2<i32>(0), size - vec2<i32>(1));
    let d = textureLoad(depth, p, 0).r;
    let uv = (vec2<f32>(p) + vec2<f32>(0.5)) / vec2<f32>(size);
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let v = post.inverse * ndc;
    return v.xyz / v.w;
}

fn drawn_at(px: vec2<i32>) -> bool {
    let size = vec2<i32>(textureDimensions(depth));
    return textureLoad(depth, clamp(px, vec2<i32>(0), size - vec2<i32>(1)), 0).r < 1.0;
}

// How much of the light around reaches a pixel: 1 for all of it. Points
// on a disc around the pixel, as wide as the distance, are looked up in
// the depth; those in front of the surface shade it, less the farther
// they are.
@fragment
fn fs_ao(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    if !drawn_at(px) {
        return vec4<f32>(1.0);
    }
    let p = view_at(px);
    // The surface's normal from its neighbours, each from the nearer side
    // so that an edge does not bend it.
    let r = view_at(px + vec2<i32>(1, 0));
    let l = view_at(px - vec2<i32>(1, 0));
    let d = view_at(px + vec2<i32>(0, 1));
    let u = view_at(px - vec2<i32>(0, 1));
    // At the picture's edge the neighbour outside is the pixel itself.
    let last = vec2<i32>(textureDimensions(depth)) - vec2<i32>(1);
    let dx = select(p - l, r - p, px.x == 0 || (px.x < last.x && abs(r.z - p.z) < abs(p.z - l.z)));
    let dy = select(p - u, d - p, px.y == 0 || (px.y < last.y && abs(d.z - p.z) < abs(p.z - u.z)));
    var n = normalize(cross(dx, dy));
    if n.z < 0.0 {
        n = -n;
    }
    let distance = post.ao.y;
    let size = vec2<f32>(textureDimensions(depth));
    let centre = post.projection * vec4<f32>(p, 1.0);
    let side = post.projection * vec4<f32>(p + vec3<f32>(distance, 0.0, 0.0), 1.0);
    let radius = abs(side.x / side.w - centre.x / centre.w) * 0.5 * size.x;
    let count = max(u32(post.ao.w), 1u);
    let turn = select(0.0, hash(in.clip.xy) * 6.2831853, post.ao2.y > 0.5);
    var occlusion = 0.0;
    for (var i = 0u; i < count; i++) {
        let t = (f32(i) + 0.5) / f32(count);
        let angle = f32(i) * 2.3999632 + turn;
        let offset = vec2<f32>(cos(angle), sin(angle)) * radius * sqrt(t);
        let q = px + vec2<i32>(round(offset));
        if !drawn_at(q) {
            continue;
        }
        let v = view_at(q) - p;
        let apart = max(length(v), 1e-4);
        let facing = dot(n, v) / apart - post.ao2.x - 0.02;
        occlusion += max(facing, 0.0) * clamp(distance / apart, 0.0, 1.0);
    }
    return vec4<f32>(vec3<f32>(1.0 - clamp(post.ao.x * occlusion / f32(count), 0.0, 1.0)), 1.0);
}

// The scene darkened by its occlusion, softened over the softness.
@fragment
fn fs_ao_apply(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(a, smooth_sampler, in.uv, 0.0);
    let texel = vec2<f32>(1.0) / vec2<f32>(textureDimensions(b));
    let spread = texel * post.ao.z;
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            sum += textureSampleLevel(b, smooth_sampler, in.uv + vec2<f32>(f32(x), f32(y)) * spread, 0.0).r;
        }
    }
    return vec4<f32>(c.rgb * (sum / 9.0), c.a);
}

fn distance_at(px: vec2<i32>) -> f32 {
    return -view_at(px).z;
}

fn focus_blur(z: f32) -> f32 {
    let range = max(post.dof.y, 1e-4);
    return clamp((abs(z - post.dof.x) - range) / range, 0.0, 1.0);
}

// Depth of field: each pixel is the average of a disc as wide as its own
// distance from the focus asks.
@fragment
fn fs_dof(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let radius = focus_blur(distance_at(px)) * post.dof.z;
    let c = textureSampleLevel(a, smooth_sampler, in.uv, 0.0);
    if radius < 0.5 {
        return c;
    }
    let texel = post.texel.xy;
    var sum = c;
    let count = 32u;
    for (var i = 0u; i < count; i++) {
        let t = (f32(i) + 0.5) / f32(count);
        let angle = f32(i) * 2.3999632;
        let offset = vec2<f32>(cos(angle), sin(angle)) * radius * sqrt(t);
        sum += textureSampleLevel(a, smooth_sampler, in.uv + offset * texel, 0.0);
    }
    return sum / f32(count + 1u);
}

// What glows: the light above the minimum, scaled and capped, and the
// bloom's share of all of it, at half the size.
@fragment
fn fs_bright(in: VsOut) -> @location(0) vec4<f32> {
    let t = post.texel.xy;
    var c = textureSampleLevel(a, smooth_sampler, in.uv, 0.0).rgb;
    if post.glow2.w > 0.5 {
        c = 0.5 * c + 0.125 * (
            textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(-t.x, -t.y), 0.0).rgb
            + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, -t.y), 0.0).rgb
            + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(-t.x, t.y), 0.0).rgb
            + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, t.y), 0.0).rgb);
    }
    let above = min(max(c - vec3<f32>(post.glow2.x), vec3<f32>(0.0)) * post.glow2.z, vec3<f32>(post.glow2.y));
    return vec4<f32>(above + c * post.glow.z, 1.0);
}

// Half the size: four texels of the larger level in one sample each.
@fragment
fn fs_down(in: VsOut) -> @location(0) vec4<f32> {
    let t = post.texel.xy * 0.5;
    return 0.25 * (
        textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(-t.x, -t.y), 0.0)
        + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, -t.y), 0.0)
        + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(-t.x, t.y), 0.0)
        + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, t.y), 0.0));
}

// A Gaussian along one direction; two of them blur a level.
@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let step = post.texel.xy * post.texel.zw;
    let weights = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    var sum = textureSampleLevel(a, smooth_sampler, in.uv, 0.0) * weights[0];
    for (var i = 1; i < 5; i++) {
        let o = step * f32(i);
        sum += textureSampleLevel(a, smooth_sampler, in.uv + o, 0.0) * weights[i];
        sum += textureSampleLevel(a, smooth_sampler, in.uv - o, 0.0) * weights[i];
    }
    return sum;
}

// A level added into the glow, by the strength.
@fragment
fn fs_add(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(a, smooth_sampler, in.uv, 0.0).rgb * post.glow.x, 0.0);
}

// The glow over the scene, by its blend mode.
@fragment
fn fs_glow(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(a, smooth_sampler, in.uv, 0.0);
    let g = textureSampleLevel(b, smooth_sampler, in.uv, 0.0).rgb * post.glow.y;
    let mode = u32(post.glow.w + 0.5);
    var rgb = c.rgb + g;
    if mode == 1u {
        let s = clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
        let k = clamp(g, vec3<f32>(0.0), vec3<f32>(1.0));
        rgb = max(c.rgb, s + k - s * k);
    } else if mode == 2u {
        let s = clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
        let k = clamp(g, vec3<f32>(0.0), vec3<f32>(1.0));
        let soft = select(
            s + (2.0 * k - vec3<f32>(1.0)) * (sqrt(s) - s),
            s - (vec3<f32>(1.0) - 2.0 * k) * s * (vec3<f32>(1.0) - s),
            k <= vec3<f32>(0.5));
        rgb = c.rgb - s + soft;
    } else if mode == 3u {
        rgb = g;
    }
    // Glow lights what was not drawn too.
    let alpha = max(c.a, clamp(max(g.r, max(g.g, g.b)), 0.0, 1.0));
    return vec4<f32>(rgb, alpha);
}

fn grade(rgb_in: vec3<f32>, uv: vec2<f32>, px: vec2<f32>) -> vec3<f32> {
    var rgb = rgb_in;
    if post.adjust.w > 0.5 {
        rgb = rgb * post.adjust.x;
        rgb = (rgb - vec3<f32>(0.5)) * post.adjust.y + vec3<f32>(0.5);
        let grey = dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
        rgb = mix(vec3<f32>(grey), rgb, post.adjust.z);
        rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if post.lut.z > 0.5 {
        // `size` cells a side, each blue a cell of the row.
        let n = post.lut.x;
        let k = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
        let cell = k.b * (n - 1.0);
        let first = floor(cell);
        let second = min(first + 1.0, n - 1.0);
        let x = (k.r * (n - 1.0) + 0.5) / (n * n);
        let y = (k.g * (n - 1.0) + 0.5) / n;
        let one = textureSampleLevel(b, smooth_sampler, vec2<f32>(x + first / n, y), 0.0).rgb;
        let two = textureSampleLevel(b, smooth_sampler, vec2<f32>(x + second / n, y), 0.0).rgb;
        rgb = mix(rgb, mix(one, two, cell - first), clamp(post.lut.y, 0.0, 1.0));
    }
    if post.vignette.w > 0.5 {
        // 1 in the middle, 0 in the corners; the strength brings it in
        // and the radius bends it.
        let e = uv * (vec2<f32>(1.0) - uv);
        let v = clamp(16.0 * e.x * e.y * post.vignette2.y / 15.0, 0.0, 1.0);
        let keep = pow(v, max(post.vignette2.x, 1e-3));
        rgb = mix(srgb3(post.vignette.rgb), rgb, keep);
    }
    if post.tone.w > 0.5 {
        rgb = rgb + vec3<f32>((hash(px) - 0.5) / 255.0);
    }
    return rgb;
}

// The last pass: sharpened, exposed and tonemapped, then graded, into a
// picture of the target's format.
@fragment
fn fs_final(in: VsOut) -> @location(0) vec4<f32> {
    var c = textureSampleLevel(a, smooth_sampler, in.uv, 0.0);
    let sharpness = post.lut.w;
    if sharpness > 0.0 {
        let t = post.texel.xy;
        let around = 0.25 * (
            textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, 0.0), 0.0)
            + textureSampleLevel(a, smooth_sampler, in.uv - vec2<f32>(t.x, 0.0), 0.0)
            + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(0.0, t.y), 0.0)
            + textureSampleLevel(a, smooth_sampler, in.uv - vec2<f32>(0.0, t.y), 0.0));
        c = max(c + (c - around) * sharpness * 2.0, vec4<f32>(0.0));
    }
    let alpha = clamp(c.a, 0.0, 1.0);
    if alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    let rgb = tonemap(c.rgb / alpha, post.tone);
    return vec4<f32>(clamp(grade(rgb, in.uv, in.clip.xy), vec3<f32>(0.0), vec3<f32>(1.0)) * alpha, alpha);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// FXAA over the finished picture: along an edge the pixel is the average
// of the colours on it, and a flat area keeps its own.
@fragment
fn fs_fxaa(in: VsOut) -> @location(0) vec4<f32> {
    let t = post.texel.xy;
    let nw = luma(textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(-t.x, -t.y), 0.0).rgb);
    let ne = luma(textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, -t.y), 0.0).rgb);
    let sw = luma(textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(-t.x, t.y), 0.0).rgb);
    let se = luma(textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, t.y), 0.0).rgb);
    let m = textureSampleLevel(a, smooth_sampler, in.uv, 0.0);
    let lm = luma(m.rgb);
    let lo = min(lm, min(min(nw, ne), min(sw, se)));
    let hi = max(lm, max(max(nw, ne), max(sw, se)));
    var dir = vec2<f32>(-((nw + ne) - (sw + se)), (nw + sw) - (ne + se));
    let reduce = max((nw + ne + sw + se) * 0.03125, 1.0 / 128.0);
    let scale = 1.0 / (min(abs(dir.x), abs(dir.y)) + reduce);
    dir = clamp(dir * scale, vec2<f32>(-8.0), vec2<f32>(8.0)) * t;
    let two = 0.5 * (
        textureSampleLevel(a, smooth_sampler, in.uv + dir * (1.0 / 3.0 - 0.5), 0.0)
        + textureSampleLevel(a, smooth_sampler, in.uv + dir * (2.0 / 3.0 - 0.5), 0.0));
    let four = two * 0.5 + 0.25 * (
        textureSampleLevel(a, smooth_sampler, in.uv - dir * 0.5, 0.0)
        + textureSampleLevel(a, smooth_sampler, in.uv + dir * 0.5, 0.0));
    let l4 = luma(four.rgb);
    if l4 < lo || l4 > hi {
        return two;
    }
    return four;
}
