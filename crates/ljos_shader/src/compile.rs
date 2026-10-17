//! GLSL 440 to GLSL ES 3.00 and GLSL 3.30, with naga.

use naga::back::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::{AddressSpace, Module, ScalarKind, ShaderStage, TypeInner, VectorSize};

use crate::rewrite::{TEXTURE_SUFFIX, rewrite};
use crate::{Block, Compiled, Error, FORMAT, Member, Sampler, Stage, Type};

/// Compiles one shader's source.
pub fn compile(source: &str, stage: Stage) -> Result<Compiled, Error> {
    let text = rewrite(source);
    let naga_stage = match stage {
        Stage::Vertex => ShaderStage::Vertex,
        Stage::Fragment => ShaderStage::Fragment,
    };
    let module = naga::front::glsl::Frontend::default()
        .parse(&naga::front::glsl::Options::from(naga_stage), &text)
        .map_err(|e| Error::Compile(e.emit_to_string(&text).trim_end().to_owned()))?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| Error::Compile(e.emit_to_string(&text).trim_end().to_owned()))?;
    let write = |version| -> Result<(String, glsl::ReflectionInfo), Error> {
        let options = glsl::Options {
            version,
            // Not ADJUST_COORDINATE_SPACE: qt_Matrix already maps to GL's
            // clip space.
            writer_flags: glsl::WriterFlags::empty(),
            ..Default::default()
        };
        let pipeline = glsl::PipelineOptions {
            shader_stage: naga_stage,
            entry_point: "main".into(),
            multiview: None,
        };
        let mut out = String::new();
        let reflection = glsl::Writer::new(
            &mut out,
            &module,
            &info,
            &options,
            &pipeline,
            naga::proc::BoundsCheckPolicies::default(),
        )
        .and_then(|mut w| w.write())
        .map_err(|e| Error::Compile(format!("cannot write GLSL {version}: {e}")))?;
        Ok((out, reflection))
    };
    let (es300, reflection) = write(glsl::Version::Embedded {
        version: 300,
        is_webgl: false,
    })?;
    let (gl330, _) = write(glsl::Version::Desktop(330))?;
    Ok(Compiled {
        format: FORMAT,
        stage,
        es300,
        gl330,
        block: block(&module, &reflection),
        samplers: samplers(&module, &reflection),
    })
}

fn block(module: &Module, reflection: &glsl::ReflectionInfo) -> Option<Block> {
    let (handle, var) = module
        .global_variables
        .iter()
        .find(|(_, v)| v.space == AddressSpace::Uniform)?;
    let TypeInner::Struct { members, span } = &module.types[var.ty].inner else {
        return None;
    };
    Some(Block {
        gl_name: reflection.uniforms.get(&handle)?.clone(),
        size: *span,
        members: members
            .iter()
            .filter_map(|m| {
                Some(Member {
                    name: m.name.clone()?,
                    offset: m.offset,
                    ty: ty(&module.types[m.ty].inner),
                })
            })
            .collect(),
    })
}

fn dim(size: VectorSize) -> u8 {
    size as u8
}

fn ty(inner: &TypeInner) -> Type {
    match *inner {
        TypeInner::Scalar(s) => match s.kind {
            ScalarKind::Float => Type::Float,
            ScalarKind::Sint => Type::Int,
            ScalarKind::Uint => Type::Uint,
            _ => Type::Other,
        },
        TypeInner::Vector { size, scalar } => match scalar.kind {
            ScalarKind::Float => Type::Vec(dim(size)),
            ScalarKind::Sint => Type::IVec(dim(size)),
            _ => Type::Other,
        },
        TypeInner::Matrix { columns, rows, .. } => Type::Mat {
            columns: dim(columns),
            rows: dim(rows),
        },
        _ => Type::Other,
    }
}

fn samplers(module: &Module, reflection: &glsl::ReflectionInfo) -> Vec<Sampler> {
    let mut found: Vec<(u32, Sampler)> = reflection
        .texture_mapping
        .iter()
        .filter_map(|(gl_name, mapping)| {
            let var = &module.global_variables[mapping.texture];
            let name = var.name.as_deref()?;
            Some((
                var.binding.as_ref().map_or(0, |b| b.binding),
                Sampler {
                    name: name.strip_suffix(TEXTURE_SUFFIX).unwrap_or(name).to_owned(),
                    gl_name: gl_name.clone(),
                },
            ))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    found.into_iter().map(|(_, s)| s).collect()
}
