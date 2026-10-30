// Makes a light probe's maps, once per probe: its equirectangular image
// laid on a cube with mipmaps, the radiance prefiltered for roughnesses
// from 0 to 1, and the irradiance each normal gathers. Each pass draws one
// face of one level; `params.face` says which.
//
// The prefiltering is the split sum's: GGX importance samples about each
// direction, read from the level of the cube whose texels are as large as
// the sample's share of the lobe ("filtered importance sampling"), which
// keeps a few dozen samples free of speckle.

const PI: f32 = 3.14159265;

struct Params {
    face: u32,
    // 0 sRGB premultiplied, 1 Radiance's shared exponent, 2 sRGB straight.
    encoding: u32,
    samples: u32,
    _pad: u32,
    roughness: f32,
    // The solid angle of one texel of the cube's first level.
    texel: f32,
    // The cube's last level.
    last_level: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var cube: texture_cube<f32>;
@group(0) @binding(3) var cube_sampler: sampler;

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

// The direction through a face's texel, in the cube's own convention.
fn face_dir(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let a = uv.x * 2.0 - 1.0;
    let b = uv.y * 2.0 - 1.0;
    var d: vec3<f32>;
    switch face {
        case 0u: { d = vec3<f32>(1.0, -b, -a); }
        case 1u: { d = vec3<f32>(-1.0, -b, a); }
        case 2u: { d = vec3<f32>(a, 1.0, b); }
        case 3u: { d = vec3<f32>(a, -1.0, -b); }
        case 4u: { d = vec3<f32>(a, -b, 1.0); }
        default: { d = vec3<f32>(-a, -b, -1.0); }
    }
    return normalize(d);
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

fn texel(x: i32, y: i32) -> vec3<f32> {
    let t = textureLoad(source, vec2<i32>(x, y), 0);
    if params.encoding == 1u {
        let e = round(t.a * 255.0);
        if e == 0.0 {
            return vec3<f32>(0.0);
        }
        return round(t.rgb * 255.0) * exp2(e - 136.0);
    }
    var c = t.rgb;
    if params.encoding == 0u && t.a > 0.0 && t.a < 1.0 {
        c = c / t.a;
    }
    return vec3<f32>(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b));
}

// The first level from the image: longitude across it, the image's middle
// looking along -z, the zenith at its top. Filtered by hand, as a shared
// exponent cannot be filtered before it is decoded.
@fragment
fn fs_equirect(in: VsOut) -> @location(0) vec4<f32> {
    let d = face_dir(params.face, in.uv);
    let size = vec2<i32>(textureDimensions(source));
    let u = atan2(d.x, -d.z) / (2.0 * PI) + 0.5;
    let v = acos(clamp(d.y, -1.0, 1.0)) / PI;
    let p = vec2<f32>(u, v) * vec2<f32>(size) - vec2<f32>(0.5);
    let f = fract(p);
    let i = vec2<i32>(floor(p));
    let x0 = (i.x % size.x + size.x) % size.x;
    let x1 = (x0 + 1) % size.x;
    let y0 = clamp(i.y, 0, size.y - 1);
    let y1 = clamp(i.y + 1, 0, size.y - 1);
    let top = mix(texel(x0, y0), texel(x1, y0), f.x);
    let bottom = mix(texel(x0, y1), texel(x1, y1), f.x);
    return vec4<f32>(mix(top, bottom, f.y), 1.0);
}

// The next level of the cube from the one above it, which is all `cube`
// shows.
@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(cube, cube_sampler, face_dir(params.face, in.uv), 0.0).rgb, 1.0);
}

// Van der Corput's radical inverse, by halving: WebGL2's shaders have no
// bit reversal.
fn radical_inverse(i: u32) -> f32 {
    var b = i;
    var r = 0.0;
    var f = 0.5;
    loop {
        if b == 0u {
            break;
        }
        if (b & 1u) != 0u {
            r += f;
        }
        b = b >> 1u;
        f *= 0.5;
    }
    return r;
}

fn basis(n: vec3<f32>, t: vec3<f32>) -> vec3<f32> {
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if abs(n.y) > 0.999 {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let x = normalize(cross(up, n));
    let y = cross(n, x);
    return x * t.x + y * t.y + n * t.z;
}

// The level a sample of probability `pdf` among `params.samples` reads.
fn lod_for(pdf: f32) -> f32 {
    let solid_angle = 1.0 / (f32(params.samples) * max(pdf, 1e-6));
    return clamp(0.5 * log2(solid_angle / params.texel) + 1.0, 0.0, params.last_level);
}

@fragment
fn fs_specular(in: VsOut) -> @location(0) vec4<f32> {
    let n = face_dir(params.face, in.uv);
    if params.roughness <= 0.0 {
        return vec4<f32>(textureSampleLevel(cube, cube_sampler, n, 0.0).rgb, 1.0);
    }
    // As the materials' highlights square it.
    let r = max(params.roughness, 0.045);
    let a = r * r;
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    for (var i = 0u; i < params.samples; i++) {
        let xi = vec2<f32>((f32(i) + 0.5) / f32(params.samples), radical_inverse(i));
        let phi = 2.0 * PI * xi.x;
        let cos_t = sqrt((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y));
        let sin_t = sqrt(1.0 - cos_t * cos_t);
        let h = basis(n, vec3<f32>(cos(phi) * sin_t, sin(phi) * sin_t, cos_t));
        // The view along the normal, as the split sum takes it.
        let l = 2.0 * dot(n, h) * h - n;
        let nl = dot(n, l);
        if nl > 0.0 {
            let d = (cos_t * cos_t * (a * a - 1.0) + 1.0);
            let ggx = a * a / (PI * d * d);
            // With the view along the normal, n·h over 4 v·h is a quarter.
            let lod = lod_for(ggx * 0.25);
            sum += textureSampleLevel(cube, cube_sampler, l, lod).rgb * nl;
            weight += nl;
        }
    }
    return vec4<f32>(sum / max(weight, 1e-6), 1.0);
}

// The mean radiance over the hemisphere about each normal, weighted by
// its cosine: the irradiance over pi, which is what a white Lambert
// surface shows.
@fragment
fn fs_irradiance(in: VsOut) -> @location(0) vec4<f32> {
    let n = face_dir(params.face, in.uv);
    var sum = vec3<f32>(0.0);
    for (var i = 0u; i < params.samples; i++) {
        let xi = vec2<f32>((f32(i) + 0.5) / f32(params.samples), radical_inverse(i));
        let phi = 2.0 * PI * xi.x;
        let cos_t = sqrt(1.0 - xi.y);
        let sin_t = sqrt(xi.y);
        let l = basis(n, vec3<f32>(cos(phi) * sin_t, sin(phi) * sin_t, cos_t));
        sum += textureSampleLevel(cube, cube_sampler, l, lod_for(cos_t / PI)).rgb;
    }
    return vec4<f32>(sum / f32(params.samples), 1.0);
}
