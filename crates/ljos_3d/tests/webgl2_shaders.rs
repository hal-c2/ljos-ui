//! The renderer's shaders as WebGL2 runs them: WGSL translated to GLSL ES
//! 3.00, within WebGL2's downlevel limits. A browser is not needed to find
//! a shader it would refuse.

use naga::back::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

fn translates(name: &str, source: &str) {
    translates_entries(name, source, &["fs_main"]);
}

fn translates_entries(name: &str, source: &str, fragments: &[&str]) {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
    // WebGL2 has none of the optional capabilities.
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
    let stages = std::iter::once((naga::ShaderStage::Vertex, "vs_main"))
        .chain(fragments.iter().map(|f| (naga::ShaderStage::Fragment, *f)));
    for (stage, entry) in stages {
        let options = glsl::Options {
            version: glsl::Version::Embedded {
                version: 300,
                is_webgl: true,
            },
            ..Default::default()
        };
        let pipeline = glsl::PipelineOptions {
            shader_stage: stage,
            entry_point: entry.into(),
            multiview: None,
        };
        let mut out = String::new();
        glsl::Writer::new(
            &mut out,
            &module,
            &info,
            &options,
            &pipeline,
            naga::proc::BoundsCheckPolicies::default(),
        )
        .and_then(|mut w| w.write())
        .unwrap_or_else(|e| panic!("{name} {entry}: {e}"));
        assert!(out.starts_with("#version 300 es"), "{name} {entry}");
    }
}

const TONE: &str = include_str!("../src/shaders/tone.wgsl");

#[test]
fn the_lit_shader_runs_on_webgl2() {
    let source = include_str!("../src/shaders/lit.wgsl");
    translates("lit", &format!("{TONE}{source}"));
    // Seven maps, the probe's two cubes and the shadow maps' array:
    // within the 16 textures a WebGL2 fragment shader may sample.
    let textures = source.matches(": texture_").count();
    assert!(textures <= 16, "{textures} textures");
    // Every light lives in one bounded array; WebGL2's smallest uniform
    // block (16 KiB) holds the frame many times over.
    assert!(source.contains("array<Light, MAX_LIGHTS>"));
}

#[test]
fn the_blit_shader_runs_on_webgl2() {
    translates("blit", include_str!("../src/shaders/blit.wgsl"));
}

#[test]
fn the_sky_box_runs_on_webgl2() {
    let source = include_str!("../src/shaders/sky.wgsl");
    translates("sky", &format!("{TONE}{source}"));
}

#[test]
fn the_light_probes_passes_run_on_webgl2() {
    translates_entries(
        "ibl",
        include_str!("../src/shaders/ibl.wgsl"),
        &[
            "fs_equirect",
            "fs_downsample",
            "fs_specular",
            "fs_irradiance",
        ],
    );
}

#[test]
fn the_shadow_pass_runs_on_webgl2() {
    translates("shadow", include_str!("../src/shaders/shadow.wgsl"));
}
