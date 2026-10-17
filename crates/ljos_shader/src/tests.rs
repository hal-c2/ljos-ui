use super::*;

/// The built-in artifacts are what their sources compile to.
/// `LJOS_SHADERS=write` makes them again.
#[test]
fn defaults() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("defaults");
    for (name, stage) in [
        ("default.vert", Stage::Vertex),
        ("default.frag", Stage::Fragment),
    ] {
        let source = std::fs::read_to_string(dir.join(name)).unwrap();
        let made = compile(&source, stage).unwrap().to_artifact();
        let path = dir.join(format!("{name}.ljsh"));
        if std::env::var("LJOS_SHADERS").as_deref() == Ok("write") {
            std::fs::write(&path, &made).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            made,
            "{name}.ljsh is stale: run LJOS_SHADERS=write cargo test -p ljos_shader"
        );
    }
    let program = Program::new(default_vertex(), default_fragment()).unwrap();
    assert_eq!(program.member("qt_Matrix").map(|m| m.offset), Some(0));
    assert_eq!(program.member("qt_Opacity").map(|m| m.offset), Some(64));
    assert_eq!(program.texture_names(), ["source"]);
}

#[test]
fn a_block_is_laid_out_as_std140() {
    let c = compile(
        "#version 440
        layout(location = 0) in vec2 qt_TexCoord0;
        layout(location = 0) out vec4 fragColor;
        layout(std140, binding = 0) uniform buf {
            mat4 qt_Matrix;
            float qt_Opacity;
            vec3 iResolution;
            vec2 rectSize;
            float radius;
            vec4 shadowColor;
            int count;
        };
        layout(binding = 1) uniform sampler2D iSource;
        void main() {
            fragColor = texture(iSource, qt_TexCoord0) * shadowColor * qt_Opacity
                * radius * float(count) + vec4(iResolution, rectSize.x);
        }",
        Stage::Fragment,
    )
    .unwrap();
    let block = c.block.unwrap();
    let at: Vec<(&str, u32, Type)> = block
        .members
        .iter()
        .map(|m| (m.name.as_str(), m.offset, m.ty))
        .collect();
    assert_eq!(
        at,
        [
            (
                "qt_Matrix",
                0,
                Type::Mat {
                    columns: 4,
                    rows: 4
                }
            ),
            ("qt_Opacity", 64, Type::Float),
            ("iResolution", 80, Type::Vec(3)),
            ("rectSize", 96, Type::Vec(2)),
            ("radius", 104, Type::Float),
            ("shadowColor", 112, Type::Vec(4)),
            ("count", 128, Type::Int),
        ]
    );
    assert_eq!(c.samplers.len(), 1);
    assert_eq!(c.samplers[0].name, "iSource");
    assert!(c.es300.starts_with("#version 300 es"));
    assert!(c.es300.contains(&c.samplers[0].gl_name));
    assert!(c.es300.contains(&block.gl_name));
    assert!(c.gl330.starts_with("#version 330"));
}

#[test]
fn a_mistake_is_a_log_that_names_the_line() {
    let Err(Error::Compile(log)) = compile(
        "#version 440\nlayout(location = 0) out vec4 fragColor;\nvoid main() { fragColor = nothing; }\n",
        Stage::Fragment,
    ) else {
        panic!("compiled");
    };
    assert!(log.contains("nothing"), "{log}");
    assert!(log.contains(":3:"), "{log}");
}

#[test]
fn a_qsb_names_its_source_and_artifact() {
    let dir = std::env::temp_dir().join(format!("ljos_shader_locate_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let qsb = dir.join("x.frag.qsb");
    assert!(matches!(locate(&qsb), Err(Error::Missing(_))));
    std::fs::write(&qsb, "qsb").unwrap();
    assert!(matches!(locate(&qsb), Err(Error::Qsb { .. })));
    std::fs::write(dir.join("x.frag.ljsh"), "{}").unwrap();
    assert_eq!(locate(&qsb), Ok(Located::Artifact(dir.join("x.frag.ljsh"))));
    std::fs::write(dir.join("x.frag"), "void main() {}").unwrap();
    assert_eq!(locate(&qsb), Ok(Located::Source(dir.join("x.frag"))));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn write_member_pads_matrix_columns() {
    let m = Member {
        name: "m".into(),
        offset: 0,
        ty: Type::Mat {
            columns: 2,
            rows: 2,
        },
    };
    let mut buf = [0u8; 32];
    write_member(&mut buf, &m, &[1.0, 2.0, 3.0, 4.0]);
    let f = |i: usize| f32::from_ne_bytes(buf[4 * i..4 * i + 4].try_into().unwrap());
    assert_eq!([f(0), f(1), f(4), f(5)], [1.0, 2.0, 3.0, 4.0]);
}

/// The qtdoc demos' shaders, when the corpus is checked out.
#[test]
fn the_corpus_shaders_compile() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/qtdoc/examples/demos");
    for (file, stage) in [
        (
            "photosurface/resources/shaders/gneiss.frag",
            Stage::Fragment,
        ),
        (
            "toycustomizer/qml/shaders/coloroverlay.frag",
            Stage::Fragment,
        ),
        (
            "toycustomizer/qml/shaders/buttonpressedshadow.vert",
            Stage::Vertex,
        ),
        (
            "toycustomizer/qml/shaders/buttonpressedshadow.frag",
            Stage::Fragment,
        ),
    ] {
        let Ok(source) = std::fs::read_to_string(root.join(file)) else {
            continue;
        };
        let c = compile(&source, stage).unwrap_or_else(|e| panic!("{file}: {e}"));
        let block = c.block.as_ref().unwrap();
        assert_eq!(block.members[0].name, "qt_Matrix", "{file}");
        assert_eq!(block.members[1].offset, 64, "{file}");
    }
}
