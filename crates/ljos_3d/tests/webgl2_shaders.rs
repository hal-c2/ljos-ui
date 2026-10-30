//! The renderer's shaders as WebGL2 runs them: WGSL translated to GLSL ES
//! 3.00, within WebGL2's downlevel limits. A browser is not needed to find
//! a shader it would refuse.

use naga::back::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

fn translates(name: &str, source: &str) {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
    // WebGL2 has none of the optional capabilities.
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
    for (stage, entry) in [
        (naga::ShaderStage::Vertex, "vs_main"),
        (naga::ShaderStage::Fragment, "fs_main"),
    ] {
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

#[test]
fn the_lit_shader_runs_on_webgl2() {
    let source = include_str!("../src/shaders/lit.wgsl");
    translates("lit", source);
    // Every light lives in one bounded array; WebGL2's smallest uniform
    // block (16 KiB) holds the frame many times over.
    assert!(source.contains("array<Light, MAX_LIGHTS>"));
}

#[test]
fn the_blit_shader_runs_on_webgl2() {
    translates("blit", include_str!("../src/shaders/blit.wgsl"));
}
