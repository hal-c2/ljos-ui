// How the light a pixel gathers becomes a colour on screen, shared by the
// materials and the sky box. `tone` is the frame's: x the mode (0 none,
// 1 linear, 2 ACES, 3 Hejl-Dawson, 4 filmic), y the exposure, z the white
// point. What it returns is already encoded for display, except in mode 5:
// a scene drawn for post-processing keeps its light as it is, and the last
// pass tonemaps it.

// The fit to sRGB Qt's shaders use, which at the dark end is several levels
// from the true curve.
fn srgb3(c: vec3<f32>) -> vec3<f32> {
    let k = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    let s1 = sqrt(k);
    let s2 = sqrt(s1);
    let s3 = sqrt(s2);
    return 0.585122381 * s1 + 0.783140355 * s2 - 0.368262736 * s3;
}

// Hable's curve, with the constants he published.
fn hable(x: vec3<f32>) -> vec3<f32> {
    let a = 0.15;
    let b = 0.50;
    let c = 0.10;
    let d = 0.20;
    let e = 0.02;
    let f = 0.30;
    return (x * (a * x + c * b) + d * e) / (x * (a * x + b) + d * f) - e / f;
}

fn tonemap(light: vec3<f32>, tone: vec4<f32>) -> vec3<f32> {
    let mode = u32(tone.x + 0.5);
    if mode == 5u {
        return light;
    }
    if mode == 0u {
        // A picture of bytes clamps it; a reflection probe's cube keeps it.
        return light;
    }
    let exposed = max(light * tone.y, vec3<f32>(0.0));
    if mode == 1u {
        return srgb3(exposed);
    }
    let x = exposed / max(tone.z, 1e-4);
    if mode == 2u {
        // Narkowicz's fit of ACES.
        return srgb3(x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14));
    }
    if mode == 3u {
        // Hejl and Burgess-Dawson's curve has the display's gamma in it.
        let h = max(x - vec3<f32>(0.004), vec3<f32>(0.0));
        return clamp(h * (6.2 * h + 0.5) / (h * (6.2 * h + 1.7) + 0.06), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return srgb3(hable(2.0 * x) / hable(vec3<f32>(11.2)));
}
