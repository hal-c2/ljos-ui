use std::path::{Path, PathBuf};

use super::*;

// The scenarios' models, made by features/quick3d/models/gen.py.
fn models() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../features/quick3d/models")
}

fn model(name: &str) -> Vec<u8> {
    std::fs::read(models().join(name)).unwrap()
}

fn beside(uri: &str) -> Result<Vec<u8>, String> {
    std::fs::read(models().join(uri)).map_err(|e| e.to_string())
}

fn load(name: &str) -> Asset {
    parse(&model(name)).unwrap().build(&mut beside).unwrap()
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-4
}

#[test]
fn a_square_with_its_buffer_inside_is_one_unlit_red_model() {
    let doc = parse(&model("square.gltf")).unwrap();
    assert!(doc.external_uris().is_empty(), "a data URI is no file");
    let asset = doc.build(&mut |_| panic!("nothing to fetch")).unwrap();
    assert_eq!(asset.roots, [0]);
    assert_eq!(asset.nodes[0].name.as_deref(), Some("square"));
    let m = &asset.materials[0].material;
    assert_eq!(m.base_color, Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(m.lighting, Lighting::None);
    let (min, max) = asset.bounds.unwrap();
    assert!(near(min, Vec3::new(-50.0, -50.0, 0.0)) && near(max, Vec3::new(50.0, 50.0, 0.0)));
    let mesh = asset.meshes[0].primitives[0].geometry.to_mesh().unwrap();
    assert_eq!(mesh.positions.len(), 4);
    assert_eq!(mesh.indices, [0, 1, 2, 0, 2, 3]);
    assert_eq!(mesh.uvs[2], [1.0, 0.0]);
    assert!(asset.warnings.is_empty(), "{:?}", asset.warnings);
}

#[test]
fn a_matrix_and_a_scale_place_the_children_of_a_parent() {
    let doc = parse(&model("pair.gltf")).unwrap();
    assert_eq!(doc.external_uris(), ["pair.bin"]);
    let asset = doc.build(&mut beside).unwrap();
    assert_eq!(asset.roots, [0]);
    assert_eq!(asset.nodes[0].children, [1, 2]);
    assert!(near(
        asset.nodes[1].transform.position,
        Vec3::new(-50.0, 0.0, 0.0)
    ));
    assert!(near(asset.nodes[1].transform.scale, Vec3::ONE));
    assert!(near(
        asset.nodes[2].transform.scale,
        Vec3::new(0.5, 0.5, 0.5)
    ));
    // Left: -50 ± 20; right: 50 ± 10.
    let (min, max) = asset.bounds.unwrap();
    assert!(near(min, Vec3::new(-70.0, -20.0, 0.0)), "{min:?}");
    assert!(near(max, Vec3::new(60.0, 20.0, 0.0)), "{max:?}");
    let colour = |n: usize| {
        let mesh = asset.nodes[n].mesh.unwrap();
        asset.materials[asset.meshes[mesh].primitives[0].material]
            .material
            .base_color
    };
    assert_eq!(colour(1), Color::rgb(0.0, 1.0, 0.0));
    assert_eq!(colour(2), Color::rgb(0.0, 0.0, 1.0));
}

#[test]
fn a_binary_file_keeps_its_texture_beside_its_factors() {
    let asset = load("cube.glb");
    let m = &asset.materials[0];
    assert_eq!(m.material.metalness, 0.0);
    assert_eq!(m.material.roughness, 0.5);
    assert_eq!(m.material.lighting, Lighting::Fragment);
    assert_eq!(m.material.cull_mode, CullMode::Back);
    // 0.35 linear is about 0.626 in sRGB.
    assert!((m.material.base_color.g - 0.626).abs() < 1e-3);
    let base = m.textures.base_color.as_ref().unwrap();
    assert_eq!((base.image.width, base.image.height), (4, 4));
    assert_eq!(
        &base.image.rgba[..8],
        &[255, 255, 255, 255, 160, 160, 160, 255]
    );
    assert_eq!(base.uv_set, 0);
    assert_eq!(
        base.sampler,
        Sampler {
            mag: Filter::Nearest,
            min: Filter::Nearest,
            mipmap: None,
            wrap_s: Wrap::ClampToEdge,
            wrap_t: Wrap::ClampToEdge,
        }
    );
    assert!(m.textures.normal.is_none());
    let g = &asset.meshes[0].primitives[0].geometry;
    let semantics: Vec<_> = g.attributes().iter().map(|a| a.semantic).collect();
    assert_eq!(
        semantics,
        [
            Semantic::Position,
            Semantic::Normal,
            Semantic::TexCoord0,
            Semantic::Tangent,
            Semantic::Index
        ]
    );
    assert_eq!(g.stride(), (3 + 3 + 2 + 4) * 4);
    let (min, max) = asset.bounds.unwrap();
    assert!(near(min, Vec3::new(-50.0, -100.0, -50.0)), "{min:?}");
    assert!(near(max, Vec3::new(50.0, 100.0, 50.0)), "{max:?}");
}

#[test]
fn the_scene_gets_a_group_for_each_node_and_a_model_for_each_primitive() {
    let asset = load("pair.gltf");
    let mut scene = Scene::new();
    let root = scene.add(None, Node::new(NodeKind::Group));
    asset.add_to(&mut scene, root);
    let walked = scene.walk();
    // The root, the pair, left and right, and a model under each side.
    assert_eq!(walked.len(), 6);
    let models: Vec<_> = walked
        .iter()
        .filter(|(_, n, _)| matches!(n.kind, NodeKind::Model(_)))
        .map(|(_, _, world)| world.translation())
        .collect();
    assert_eq!(models.len(), 2);
    assert!(models.iter().any(|t| near(*t, Vec3::new(-50.0, 0.0, 0.0))));
    assert!(models.iter().any(|t| near(*t, Vec3::new(50.0, 0.0, 0.0))));
}

#[test]
fn a_primitive_without_a_material_gets_the_one_gltf_defines() {
    let text = std::str::from_utf8(&model("square.gltf"))
        .unwrap()
        .replace("\"material\": 0", "\"mode\": 4");
    let asset = parse(text.as_bytes()).unwrap().build(&mut beside).unwrap();
    let p = &asset.meshes[0].primitives[0];
    let m = &asset.materials[p.material].material;
    assert_eq!((m.metalness, m.roughness), (1.0, 1.0));
    assert_eq!(m.base_color, Color::WHITE);
}

#[test]
fn a_buffer_that_cannot_be_read_names_its_file() {
    let doc = parse(&model("pair.gltf")).unwrap();
    let err = doc
        .build(&mut |uri| Err(format!("{uri}: no such file")))
        .unwrap_err();
    assert!(
        matches!(&err, LoadError::Buffer { uri, .. } if uri == "pair.bin"),
        "{err}"
    );
    assert!(err.to_string().contains("pair.bin"));
}

#[test]
fn a_buffer_shorter_than_it_says_is_an_error() {
    let err = parse(&model("pair.gltf"))
        .unwrap()
        .build(&mut |_| Ok(vec![0; 10]))
        .unwrap_err();
    assert!(err.to_string().contains("10 bytes"), "{err}");
}

#[test]
fn json_that_does_not_parse_is_an_error_not_a_panic() {
    let err = parse(br#"{"asset": {"version": "2.0"}, "nodes": ["#).unwrap_err();
    assert!(matches!(err, LoadError::Parse(_)), "{err}");
}

#[test]
fn a_reference_to_what_the_file_lacks_is_an_error() {
    let err = parse(br#"{"asset":{"version":"2.0"},"nodes":[{"mesh":3}]}"#).unwrap_err();
    assert!(err.to_string().contains("nodes[0].mesh"), "{err}");
}

#[test]
fn a_file_without_nodes_is_an_error() {
    let err = parse(br#"{"asset":{"version":"2.0"}}"#)
        .unwrap()
        .build(&mut beside)
        .unwrap_err();
    assert_eq!(err, LoadError::Empty);
}

#[test]
fn nodes_that_loop_are_refused() {
    let err = parse(br#"{"asset":{"version":"2.0"},"nodes":[{"children":[1]},{"children":[0]}]}"#)
        .unwrap()
        .build(&mut beside)
        .unwrap_err();
    assert!(err.to_string().contains("ancestor"), "{err}");
}

#[test]
fn a_file_without_a_scene_shows_the_nodes_nothing_holds() {
    let asset = parse(br#"{"asset":{"version":"2.0"},"nodes":[{"children":[2]},{},{}]}"#)
        .unwrap()
        .build(&mut beside)
        .unwrap();
    assert_eq!(asset.roots, [0, 1]);
    assert_eq!(asset.bounds, None);
}

#[test]
fn an_accessor_past_its_view_leaves_its_primitive_out_with_a_warning() {
    let text = std::str::from_utf8(&model("square.gltf"))
        .unwrap()
        .replacen("\"count\": 4", "\"count\": 400", 1);
    let asset = parse(text.as_bytes()).unwrap().build(&mut beside).unwrap();
    assert!(asset.meshes[0].primitives.is_empty());
    assert!(
        asset.warnings[0].contains("runs past"),
        "{:?}",
        asset.warnings
    );
}

#[test]
fn an_index_past_the_vertices_leaves_its_primitive_out() {
    // The square's six indices end its buffer; make the first one 9.
    let doc = parse(&model("square.gltf")).unwrap();
    let mut buffer = data_uri(
        doc.document
            .buffers()
            .next()
            .map(|b| match b.source() {
                ::gltf::buffer::Source::Uri(u) => u.to_owned(),
                ::gltf::buffer::Source::Bin => unreachable!(),
            })
            .unwrap()
            .as_str(),
    )
    .unwrap();
    let at = buffer.len() - 12;
    buffer[at..at + 2].copy_from_slice(&9u16.to_le_bytes());
    let text = std::str::from_utf8(&model("square.gltf"))
        .unwrap()
        .to_owned();
    let start = text.find("data:").unwrap();
    let end = start + text[start..].find('"').unwrap();
    let text = format!("{}{}{}", &text[..start], "pair-broken.bin", &text[end..]);
    let asset = parse(text.as_bytes())
        .unwrap()
        .build(&mut |_| Ok(buffer.clone()))
        .unwrap();
    assert!(asset.meshes[0].primitives.is_empty());
    assert!(
        asset.warnings[0].contains("index 9"),
        "{:?}",
        asset.warnings
    );
}

#[test]
fn an_image_that_does_not_decode_is_a_warning_and_the_factors_stay() {
    let mut glb = model("cube.glb");
    // The PNG's signature, wherever the chunk put it, made into nonsense.
    let at = glb.windows(4).position(|w| w == b"\x89PNG").unwrap();
    glb[at..at + 4].copy_from_slice(b"JUNK");
    let asset = parse(&glb).unwrap().build(&mut beside).unwrap();
    assert!(asset.materials[0].textures.base_color.is_none());
    assert_eq!(asset.materials[0].material.roughness, 0.5);
    assert!(
        asset.warnings[0].contains("image 0"),
        "{:?}",
        asset.warnings
    );
}

#[test]
fn base64_decodes_with_and_without_padding() {
    assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
    assert_eq!(base64("aGVsbG8").unwrap(), b"hello");
    assert_eq!(base64("aGVsbG8h").unwrap(), b"hello!");
    assert_eq!(base64("").unwrap(), b"");
    assert!(base64("a").is_none());
    assert!(base64("a*==").is_none());
    assert!(data_uri("data:text/plain,hello").is_err());
}

// What a reader of files from anywhere must survive: every prefix of a
// real file, and a real file with bytes changed. Neither may panic, and
// each is either an asset or an error.
fn survives(bytes: &[u8]) {
    if let Ok(doc) = parse(bytes) {
        let _ = doc.build(&mut |_| Ok(vec![0; 64]));
    }
}

#[test]
fn every_prefix_of_a_binary_file_is_survived() {
    let glb = model("cube.glb");
    for end in 0..glb.len() {
        survives(&glb[..end]);
    }
}

#[test]
fn every_prefix_of_a_json_file_is_survived() {
    let gltf = model("pair.gltf");
    for end in 0..gltf.len() {
        survives(&gltf[..end]);
    }
}

// A fixed sequence, so a failure is the same failure every run.
fn xorshift(mut state: u32) -> impl FnMut() -> u32 {
    move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    }
}

fn rounds() -> usize {
    std::env::var("LJOS_GLTF_FUZZ_ROUNDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(4000)
}

#[test]
fn changed_numbers_in_a_json_file_are_survived() {
    // Changing a digit keeps the JSON parsing, so the change reaches the
    // indices, offsets, lengths and counts behind it.
    // A GLB's chunks have lengths, so its JSON only has digits changed.
    for (name, may_grow) in [("pair.gltf", true), ("cube.glb", false)] {
        let file = model(name);
        let json_end = if may_grow {
            file.len()
        } else {
            20 + u32::from_le_bytes([file[12], file[13], file[14], file[15]]) as usize
        };
        let digits: Vec<usize> = (0..json_end)
            .filter(|i| file[*i].is_ascii_digit())
            .collect();
        let mut next = xorshift(0x9e37_79b9);
        for _ in 0..rounds() {
            let mut bytes = file.clone();
            for _ in 0..1 + next() % 3 {
                let at = digits[next() as usize % digits.len()];
                bytes[at] = b'0' + (next() % 10) as u8;
            }
            if may_grow && next().is_multiple_of(4) {
                let at = digits[next() as usize % digits.len()];
                bytes.splice(at..at, *b"99999999");
            }
            survives(&bytes);
        }
    }
}

#[test]
fn changed_bytes_in_a_binary_file_are_survived() {
    let glb = model("cube.glb");
    let mut next = xorshift(0x2545_f491);
    for _ in 0..rounds() {
        let mut bytes = glb.clone();
        for _ in 0..1 + next() % 4 {
            let at = next() as usize % bytes.len();
            bytes[at] = next() as u8;
        }
        survives(&bytes);
    }
}

#[test]
fn garbage_is_survived() {
    survives(b"");
    survives(b"glTF");
    survives(b"glTF\x02\0\0\0\xff\xff\xff\xff");
    survives(b"glTF\x02\0\0\0\x14\0\0\0\xff\xff\xff\x7fJSON");
    survives(&[0xff; 300]);
    survives(br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":18446744073709551615}]}"#);
    survives(
        br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":4,"uri":"data:;base64,AAAAAA=="}],
            "bufferViews":[{"buffer":0,"byteOffset":18446744073709551610,"byteLength":100,"byteStride":4}],
            "accessors":[{"bufferView":0,"byteOffset":18446744073709551610,"componentType":5126,"count":18446744073709551615,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":0}]}],
            "nodes":[{"mesh":0,"matrix":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}]}"#,
    );
}

#[test]
fn a_material_s_textures_become_its_maps_sharing_one_picture() {
    let asset = load("cube.glb");
    let Material::Principled(m) = scene_material(&asset.materials[0]) else {
        panic!("a glTF material is principled");
    };
    let base = m.maps.base_color.as_ref().expect("the base colour map");
    let decoded = asset.materials[0].textures.base_color.as_ref().unwrap();
    // The same picture, not a copy: one upload for every material using it.
    assert_eq!(base.image.id(), decoded.image.id());
    assert_eq!(base.image.size(), [4, 4]);
    assert_eq!(base.image.encoding(), PixelEncoding::Srgb8Straight);
    assert!(base.image.opaque());
    assert_eq!(
        base.sampling,
        Sampling {
            min: texture::Filter::Nearest,
            mag: texture::Filter::Nearest,
            mip: texture::Filter::None,
            tiling_u: Tiling::ClampToEdge,
            tiling_v: Tiling::ClampToEdge,
            generate_mipmaps: false,
        }
    );
    assert!(m.maps.metalness.is_none() && m.maps.normal.is_none());
    // Building the material again keeps the picture's identity.
    let Material::Principled(again) = scene_material(&asset.materials[0]) else {
        unreachable!()
    };
    assert_eq!(again.maps.base_color.unwrap().image.id(), base.image.id());
}

#[test]
fn metal_roughness_occlusion_and_normal_textures_read_their_channels() {
    let image = Arc::new(TextureImage::new(1, 1, vec![10, 20, 30, 255]));
    let at = |uv_set| {
        Some(TextureRef {
            image: image.clone(),
            uv_set,
            sampler: Sampler::default(),
        })
    };
    let material = AssetMaterial {
        name: None,
        material: gltf_default(),
        textures: MaterialTextures {
            metallic_roughness: at(0),
            normal: at(0),
            normal_scale: 0.5,
            occlusion: at(0),
            occlusion_strength: 0.25,
            emissive: at(1),
            ..MaterialTextures::default()
        },
    };
    let Material::Principled(m) = scene_material(&material) else {
        unreachable!()
    };
    let maps = &m.maps;
    assert_eq!(maps.metalness_channel, Channel::B);
    assert_eq!(maps.roughness_channel, Channel::G);
    assert!(maps.metalness.is_some() && maps.roughness.is_some());
    assert_eq!(maps.occlusion_channel, Channel::R);
    assert_eq!((maps.normal_strength, maps.occlusion_amount), (0.5, 0.25));
    // Trilinear, as glTF leaves unset filters to the viewer.
    let s = maps.normal.as_ref().unwrap().sampling;
    assert!(s.mipmapped() && s.mip == texture::Filter::Linear);
    // A texture on the second UV set is drawn with it.
    assert_eq!(maps.emissive.as_ref().unwrap().uv_set, 1);
    assert_eq!(maps.normal.as_ref().unwrap().uv_set, 0);
}

#[test]
fn a_texture_on_the_second_uv_set_is_drawn_with_it() {
    let asset = load("uv2.gltf");
    assert!(asset.warnings.is_empty(), "{:?}", asset.warnings);
    let mesh = asset.meshes[0].primitives[0].geometry.to_mesh().unwrap();
    assert_eq!(mesh.uvs[0], [0.25, 0.25]);
    assert_eq!(mesh.uv1.len(), mesh.positions.len());
    let Material::Principled(m) = scene_material(&asset.materials[0]) else {
        unreachable!()
    };
    assert_eq!(m.maps.base_color.unwrap().uv_set, 1);
}

#[test]
fn a_skin_s_joints_and_inverse_bind_matrices_are_read() {
    let asset = load("skin.gltf");
    assert!(asset.warnings.is_empty(), "{:?}", asset.warnings);
    let skin = &asset.skins[0];
    assert_eq!(skin.joints, [0, 1]);
    assert_eq!(
        skin.inverse_bind[1].translation(),
        Vec3::new(0.0, -50.0, 0.0)
    );
    assert_eq!(asset.nodes[2].skin, Some(0));
    let mesh = asset.meshes[0].primitives[0].geometry.to_mesh().unwrap();
    assert_eq!(mesh.joints[0], [0, 0, 0, 0]);
    assert_eq!(mesh.joints[5], [1, 0, 0, 0]);
    assert_eq!(mesh.weights[5], [1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn a_placed_skin_names_the_scene_nodes_of_its_joints() {
    let asset = load("skin.gltf");
    let mut scene = Scene::new();
    let loader = scene.add(None, Node::new(NodeKind::Group));
    let placed = asset.add_to(&mut scene, loader);
    let [(node, model)] = placed.models[..] else {
        panic!("one model: {:?}", placed.models)
    };
    assert_eq!(node, 2);
    let skin = scene.node(model).unwrap().skin.as_ref().unwrap();
    assert_eq!(
        skin.joints,
        [placed.nodes[0].unwrap(), placed.nodes[1].unwrap()]
    );
    // The knee turned a quarter turn swings the strip's top to the left.
    let palette = skin.palette(&scene);
    let top = crate::instancing::skin_point(
        &palette,
        [1, 0, 0, 0],
        [1.0, 0.0, 0.0, 0.0],
        Vec3::new(10.0, 100.0, 0.0),
    );
    assert!(near(top, Vec3::new(-50.0, 60.0, 0.0)), "{top:?}");
}

#[test]
fn morph_targets_move_the_vertices_by_their_weights() {
    let asset = load("morph.gltf");
    assert!(asset.warnings.is_empty(), "{:?}", asset.warnings);
    let mesh = asset.meshes[0].primitives[0].geometry.to_mesh().unwrap();
    assert!(mesh.positions.iter().all(|p| p[0] == 0.0 || p[0] == 100.0));
    let (min, max) = asset.bounds.unwrap();
    assert!(near(min, Vec3::new(0.0, -50.0, 0.0)) && near(max, Vec3::new(100.0, 50.0, 0.0)));
}
