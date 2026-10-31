// The built-in passes over a scene drawn in linear light: ambient
// occlusion, global illumination, reflections, depth of field, glow, the
// lens flare, and the last pass, which tonemaps and grades the picture
// into the target. Every pass draws one triangle over its output; `a` is
// what it reads, `b` a second picture, `c_map` and `d_map` a third and a
// fourth (the surfaces' normals and materials where a pass reads them),
// and `depth` the scene's depth where a pass needs it. Colours are
// premultiplied.

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
    // The camera's view, which turns the surfaces' normals into its space.
    view: mat4x4<f32>,
    // The lens flare's bloom scale, bloom bias, ghost dispersal and ghost
    // count.
    flare: vec4<f32>,
    // Its halo's width, the halo's stretch to the aspect, the distortion
    // in texels, and the blur.
    flare2: vec4<f32>,
    // 1 where there is a lens colour texture; the dirt's and the
    // starburst's source (0 none, 1 built in, 2 a texture); the
    // starburst's turn in radians.
    flare3: vec4<f32>,
    // The reflections' most steps and thickness; the camera's near and
    // far planes.
    ssr: vec4<f32>,
    // The global illumination's hit thickness, indirect light boost, 1
    // with indirect light, and samples per slice side.
    gi: vec4<f32>,
    // Its sample radius as a share of the view's height, the bounce
    // factor (0 without the bounce), and slices.
    gi2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> post: Post;
@group(0) @binding(1) var a: texture_2d<f32>;
@group(0) @binding(2) var b: texture_2d<f32>;
@group(0) @binding(3) var depth: texture_2d<f32>;
@group(0) @binding(4) var smooth_sampler: sampler;
@group(0) @binding(5) var c_map: texture_2d<f32>;
@group(0) @binding(6) var d_map: texture_2d<f32>;

const PI: f32 = 3.14159265;

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

fn orthographic() -> bool {
    return post.projection[3][3] > 0.5;
}

// The way the camera looks through a point it sees.
fn view_ray(p: vec3<f32>) -> vec3<f32> {
    if orthographic() {
        return vec3<f32>(0.0, 0.0, -1.0);
    }
    return normalize(p);
}

// A surface's normal in the camera's space, and its roughness or what
// stands for it (see fs_surface).
fn surface_at(px: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(c_map));
    let s = textureLoad(c_map, clamp(px, vec2<i32>(0), size - vec2<i32>(1)), 0);
    let n = (post.view * vec4<f32>(s.xyz, 0.0)).xyz;
    return vec4<f32>(n / max(length(n), 1e-6), s.w);
}

fn pixel_of(clip: vec4<f32>, size: vec2<f32>) -> vec2<f32> {
    let ndc = clip.xy / clip.w;
    return vec2<f32>((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5) * size;
}

// Reflections: the reflected ray is followed across the picture, a pixel
// a step up to the most steps, and the first surface it passes behind by
// less than the thickness is what the pixel reflects. rgb is that light
// by how sure the hit is, a how sure.
@fragment
fn fs_ssr(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let s = surface_at(px);
    if !drawn_at(px) || s.w > 1.5 {
        return vec4<f32>(0.0);
    }
    // A rough surface's reflection is the probe's to give: it fades out.
    let glossy = (1.0 - s.w) * (1.0 - s.w);
    if glossy <= 0.0 {
        return vec4<f32>(0.0);
    }
    let p = view_at(px);
    let r = reflect(view_ray(p), s.xyz);
    let near = post.ssr.z;
    var reach = post.ssr.w - near;
    if r.z > 1e-4 {
        reach = min(reach, (-near - p.z) / r.z * 0.99);
    }
    if reach <= 0.0 {
        return vec4<f32>(0.0);
    }
    let e = p + r * reach;
    let h0 = post.projection * vec4<f32>(p, 1.0);
    let h1 = post.projection * vec4<f32>(e, 1.0);
    let size = vec2<f32>(textureDimensions(depth));
    let s0 = pixel_of(h0, size);
    let s1 = pixel_of(h1, size);
    let delta = s1 - s0;
    // Cut where the ray leaves the picture.
    var end = 1.0;
    if s1.x < 0.0 {
        end = min(end, -s0.x / delta.x);
    }
    if s1.x > size.x {
        end = min(end, (size.x - s0.x) / delta.x);
    }
    if s1.y < 0.0 {
        end = min(end, -s0.y / delta.y);
    }
    if s1.y > size.y {
        end = min(end, (size.y - s0.y) / delta.y);
    }
    let steps = u32(clamp(length(delta) * end, 1.0, max(post.ssr.x, 1.0)));
    // Depth along the ray is interpolated as the projection has it.
    let k0 = 1.0 / h0.w;
    let k1 = 1.0 / h1.w;
    let jitter = hash(in.clip.xy);
    var before = 0.0;
    var hit = -1.0;
    for (var i = 1u; i <= steps; i++) {
        let t = end * (f32(i) - 0.5 + 0.5 * jitter) / f32(steps);
        let q = vec2<i32>(mix(s0, s1, t));
        if all(abs(q - px) <= vec2<i32>(1)) || !drawn_at(q) {
            before = t;
            continue;
        }
        let ray_z = mix(p.z * k0, e.z * k1, t) / mix(k0, k1, t);
        let behind = view_at(q).z - ray_z;
        if behind > 0.0 && behind < post.ssr.y {
            var lo = before;
            var hi = t;
            for (var k = 0; k < 5; k++) {
                let m = 0.5 * (lo + hi);
                let mq = vec2<i32>(mix(s0, s1, m));
                let mz = mix(p.z * k0, e.z * k1, m) / mix(k0, k1, m);
                let mb = view_at(mq).z - mz;
                if drawn_at(mq) && mb > 0.0 && mb < post.ssr.y {
                    hi = m;
                } else {
                    lo = m;
                }
            }
            hit = hi;
            break;
        }
        before = t;
    }
    if hit < 0.0 {
        return vec4<f32>(0.0);
    }
    let at = mix(s0, s1, hit);
    // A surface seen from behind is not what the ray would meet.
    if dot(surface_at(vec2<i32>(at)).xyz, r) > 0.0 {
        return vec4<f32>(0.0);
    }
    let uv = at / size;
    let edge = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let sure = glossy * smoothstep(0.0, 0.08, edge) * (1.0 - smoothstep(0.75, 1.0, hit / end));
    let light = textureSampleLevel(a, smooth_sampler, uv, 0.0).rgb;
    return vec4<f32>(light * sure, sure);
}

// The reflections over the scene, blurred as wide as the surface is
// rough and by the Fresnel term of its specular colour.
@fragment
fn fs_ssr_apply(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let col = textureSampleLevel(a, smooth_sampler, in.uv, 0.0);
    let s = surface_at(px);
    if !drawn_at(px) || s.w > 1.5 {
        return col;
    }
    let texel = vec2<f32>(1.0) / vec2<f32>(textureDimensions(b));
    let radius = s.w * 12.0;
    var reflected = textureSampleLevel(b, smooth_sampler, in.uv, 0.0).rgb;
    if radius >= 1.0 {
        let count = 12u;
        for (var i = 0u; i < count; i++) {
            let t = (f32(i) + 0.5) / f32(count);
            let angle = f32(i) * 2.3999632;
            let offset = vec2<f32>(cos(angle), sin(angle)) * radius * sqrt(t);
            reflected += textureSampleLevel(b, smooth_sampler, in.uv + offset * texel, 0.0).rgb;
        }
        reflected = reflected / f32(count + 1u);
    }
    let m = textureLoad(d_map, px, 0);
    let v = -view_ray(view_at(px));
    let nv = clamp(dot(s.xyz, v), 0.0, 1.0);
    let f0 = mix(vec3<f32>(0.04), m.rgb, m.a);
    let fresnel = f0 + (max(vec3<f32>(1.0 - s.w), f0) - f0) * pow(1.0 - nv, 5.0);
    return vec4<f32>(col.rgb + reflected * fresnel, col.a);
}

// Global illumination at the buffer's size: in slices turned about the
// view, the surfaces around each pixel are walked out from it on both
// sides. A surface higher above the pixel's horizon than any nearer one
// hides the light from all around (the occlusion, in a) and gives its own
// light (rgb), as much as it rises, the less the farther past the radius
// it is by the hit thickness. With the bounce, what the last frame
// gathered lights it again.
@fragment
fn fs_gi(in: VsOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(depth));
    let px = vec2<i32>(in.uv * size);
    let s = surface_at(px);
    if !drawn_at(px) || s.w > 2.5 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let p = view_at(px);
    let n = s.xyz;
    let radius = max(post.gi2.x * size.y, 1.0);
    // A pixel's size in the scene where the surface is.
    let unit = select(-p.z, 1.0, orthographic()) * 2.0 / (post.projection[1][1] * size.y);
    let reach = radius * unit;
    let thickness = max(reach * post.gi.x, 1e-4);
    let slices = max(u32(post.gi2.z), 1u);
    let count = max(u32(post.gi.w), 1u);
    let turn = hash(in.clip.xy);
    let spread = hash(in.clip.yx + vec2<f32>(7.0, 3.0));
    let last = vec2<i32>(size) - vec2<i32>(1);
    var light = vec3<f32>(0.0);
    var hidden = 0.0;
    for (var sl = 0u; sl < slices; sl++) {
        let angle = (f32(sl) + turn) / f32(slices) * PI;
        let dir = vec2<f32>(cos(angle), sin(angle));
        for (var side = 0u; side < 2u; side++) {
            let way = select(-1.0, 1.0, side == 0u);
            var horizon = 0.0;
            for (var j = 0u; j < count; j++) {
                let t = (f32(j) + spread) / f32(count);
                let q = vec2<i32>(vec2<f32>(px) + dir * way * (1.0 + (radius - 1.0) * t));
                if any(q < vec2<i32>(0)) || any(q > last) || !drawn_at(q) {
                    continue;
                }
                let w = view_at(q) - p;
                let apart = length(w);
                if apart < 1e-4 {
                    continue;
                }
                let rise = dot(n, w / apart);
                if rise <= horizon {
                    continue;
                }
                let near = clamp((reach + thickness - apart) / thickness, 0.0, 1.0);
                let share = (rise - horizon) * near;
                var given = textureLoad(a, q, 0).rgb;
                if post.gi2.y > 0.0 {
                    let m = textureLoad(d_map, q, 0);
                    let before = textureSampleLevel(b, smooth_sampler, (vec2<f32>(q) + vec2<f32>(0.5)) / size, 0.0).rgb;
                    given += post.gi2.y * before * m.rgb * (1.0 - m.a);
                }
                light += given * share;
                hidden += share;
                horizon = mix(horizon, rise, near);
            }
        }
    }
    let total = f32(slices * 2u);
    return vec4<f32>(light / total, 1.0 - clamp(hidden / total, 0.0, 1.0));
}

// The gathered light softened over its neighbours at about the same
// distance, so that it does not bleed across an edge.
// How much a neighbour's gathered light belongs to the pixel at `px`.
// Depth does not part two surfaces where they touch, so the normal does:
// what the box's face gathered is not the floor's at the box's foot.
fn gi_share(px: vec2<i32>, q: vec2<i32>) -> f32 {
    let z = view_at(px).z;
    let depth_share = 1.0 / (1.0 + abs(view_at(q).z - z) * 10.0 / max(abs(z), 1.0));
    let along = max(dot(textureLoad(c_map, px, 0).xyz, textureLoad(c_map, q, 0).xyz), 0.0);
    return depth_share * pow(along, 8.0) + 1e-4;
}

@fragment
fn fs_gi_blur(in: VsOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(depth));
    let texel = vec2<f32>(1.0) / vec2<f32>(textureDimensions(a));
    let px = vec2<i32>(in.uv * size);
    var sum = vec4<f32>(0.0);
    var weight = 0.0;
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            let uv = in.uv + vec2<f32>(f32(x), f32(y)) * texel;
            let w = gi_share(px, vec2<i32>(uv * size));
            sum += textureSampleLevel(a, smooth_sampler, uv, 0.0) * w;
            weight += w;
        }
    }
    return sum / weight;
}

@fragment
fn fs_gi_apply(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let col = textureSampleLevel(a, smooth_sampler, in.uv, 0.0);
    let s = textureLoad(c_map, px, 0);
    if !drawn_at(px) || s.w > 2.5 {
        return col;
    }
    // The gathered light may be smaller than the picture; scaled up plainly
    // it would cross from one surface to the other. Its pixels are read
    // whole, with enough around this one that one on the same surface is
    // among them.
    let size = vec2<f32>(textureDimensions(depth));
    let small = vec2<f32>(textureDimensions(b));
    let here = in.uv * small - vec2<f32>(0.5);
    let middle = vec2<i32>(round(here));
    var g = vec4<f32>(0.0);
    var weight = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let q = clamp(middle + vec2<i32>(x, y), vec2<i32>(0), vec2<i32>(small) - vec2<i32>(1));
            let at = vec2<i32>((vec2<f32>(q) + vec2<f32>(0.5)) / small * size);
            let tent = max(1.5 - length(vec2<f32>(q) - here), 0.01);
            let w = tent * gi_share(px, at);
            g += textureLoad(b, q, 0) * w;
            weight += w;
        }
    }
    g /= weight;
    var rgb = col.rgb * g.a;
    if post.gi.z > 0.5 {
        let m = textureLoad(d_map, px, 0);
        rgb += g.rgb * m.rgb * (1.0 - m.a) * post.gi.y;
    }
    return vec4<f32>(rgb, col.a);
}

// The lens flare's light: what is above the bias, scaled, at half the
// size.
@fragment
fn fs_flare_bright(in: VsOut) -> @location(0) vec4<f32> {
    let t = post.texel.xy * 0.5;
    let c4 = 0.25 * (
        textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(-t.x, -t.y), 0.0).rgb
        + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, -t.y), 0.0).rgb
        + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(-t.x, t.y), 0.0).rgb
        + textureSampleLevel(a, smooth_sampler, in.uv + vec2<f32>(t.x, t.y), 0.0).rgb);
    return vec4<f32>(max(c4 - vec3<f32>(post.flare.y), vec3<f32>(0.0)) * post.flare.x, 1.0);
}

// Red, green and blue each a little apart along `way`: the lens's
// chromatic aberration.
fn fringed(uv: vec2<f32>, way: vec2<f32>, apart: vec2<f32>) -> vec3<f32> {
    return vec3<f32>(
        textureSampleLevel(a, smooth_sampler, uv + way * apart, 0.0).r,
        textureSampleLevel(a, smooth_sampler, uv, 0.0).g,
        textureSampleLevel(a, smooth_sampler, uv - way * apart, 0.0).b);
}

// The built-in lens colour: a rainbow from the middle out.
fn lens_colour(t: f32) -> vec3<f32> {
    if post.flare3.x > 0.5 {
        return textureSampleLevel(b, smooth_sampler, vec2<f32>(clamp(t, 0.0, 1.0), 0.5), 0.0).rgb;
    }
    return vec3<f32>(0.5) + 0.5 * cos(6.2831853 * (vec3<f32>(t) + vec3<f32>(0.0, 0.33, 0.67)));
}

// Ghosts and a halo of the bright light: the picture turned about its
// middle and sampled at steps of the dispersal back towards the middle,
// each sample the weaker the farther from the middle it is.
@fragment
fn fs_flare_ghosts(in: VsOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(1.0) - in.uv;
    let ghost = (vec2<f32>(0.5) - uv) * post.flare.z;
    let away = length(ghost);
    let way = select(vec2<f32>(0.0), ghost / away, away > 1e-6);
    let apart = post.texel.xy * post.flare2.z;
    let corner = length(vec2<f32>(0.5));
    var sum = vec3<f32>(0.0);
    let count = u32(post.flare.w);
    for (var i = 0u; i < count; i++) {
        let at = fract(uv + ghost * f32(i));
        let w = pow(1.0 - length(vec2<f32>(0.5) - at) / corner, 10.0);
        sum += fringed(at, way, apart) * w;
    }
    sum = sum * lens_colour(length(vec2<f32>(0.5) - uv) / corner);
    let width = post.flare2.x;
    if width > 0.0 && away > 1e-6 {
        let size = vec2<f32>(textureDimensions(a));
        let stretch = mix(1.0, size.x / size.y, post.flare2.y);
        var h = vec2<f32>(way.x * stretch, way.y);
        h = normalize(h) * width;
        h.x = h.x / stretch;
        let at = uv + h;
        let w = pow(1.0 - length(vec2<f32>(0.5) - fract(at)) / corner, 5.0);
        sum += fringed(at, way, apart) * w;
    }
    return vec4<f32>(sum, 1.0);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (vec2<f32>(3.0) - 2.0 * f);
    return mix(
        mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y);
}

// The built-in dirt: soft smudges of different sizes.
fn dirt(uv: vec2<f32>) -> f32 {
    return 0.35 + 0.5 * smoothstep(0.45, 0.9, noise(uv * 6.0)) + 0.25 * noise(uv * 23.0);
}

// The built-in starburst: rays out from the middle.
fn starburst(uv: vec2<f32>) -> f32 {
    let r = uv - vec2<f32>(0.5);
    let angle = atan2(r.y, r.x);
    let rays = pow(abs(cos(angle * 6.0)), 12.0) + 0.5 * pow(abs(cos(angle * 11.0 + 1.3)), 24.0);
    return 0.3 + 0.7 * clamp(rays, 0.0, 1.0);
}

// The flare over the scene, through the dirt and the starburst, the
// starburst turned with the camera.
@fragment
fn fs_flare_apply(in: VsOut) -> @location(0) vec4<f32> {
    let col = textureSampleLevel(a, smooth_sampler, in.uv, 0.0);
    let flare = textureSampleLevel(b, smooth_sampler, in.uv, 0.0).rgb;
    let dirt_from = u32(post.flare3.y + 0.5);
    let star_from = u32(post.flare3.z + 0.5);
    var lens = vec3<f32>(1.0);
    if dirt_from == 2u {
        lens = textureSampleLevel(c_map, smooth_sampler, in.uv, 0.0).rgb;
    } else if dirt_from == 1u {
        lens = vec3<f32>(dirt(in.uv));
    } else if star_from != 0u {
        lens = vec3<f32>(0.0);
    }
    if star_from != 0u {
        let turn = post.flare3.w;
        let r = in.uv - vec2<f32>(0.5);
        let turned = vec2<f32>(cos(turn) * r.x - sin(turn) * r.y, sin(turn) * r.x + cos(turn) * r.y) + vec2<f32>(0.5);
        if star_from == 2u {
            lens += textureSampleLevel(d_map, smooth_sampler, turned, 0.0).rgb;
        } else {
            lens += vec3<f32>(starburst(turned));
        }
    }
    let lit = flare * lens;
    let alpha = max(col.a, clamp(max(lit.r, max(lit.g, lit.b)), 0.0, 1.0));
    return vec4<f32>(col.rgb + lit, alpha);
}
