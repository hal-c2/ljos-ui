use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::fetch::InMemory;
use crate::scene::Material;

fn built(obj: &str, mtl: &str) -> Asset {
    let mut doc = parse(obj.as_bytes()).expect("parses");
    doc.read_libraries("file:///m/a.obj", &mut |_| Ok(mtl.as_bytes().to_vec()));
    doc.build(&mut |uri| Err(format!("{uri} is not here")))
        .expect("builds")
}

/// A vertex's floats: position, normal and maybe a UV.
fn vertices(p: &AssetPrimitive) -> Vec<Vec<f32>> {
    let floats: Vec<f32> = p
        .geometry
        .vertex_data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    floats
        .chunks(p.geometry.stride() / 4)
        .map(<[f32]>::to_vec)
        .collect()
}

fn triangles(p: &AssetPrimitive) -> usize {
    p.geometry.index_data().len() / 12
}

const QUAD: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\n";

#[test]
fn the_reader_is_the_extensions_in_any_case() {
    assert_eq!(reader_for("file:///a/b.obj"), Some(Reader::Obj));
    assert_eq!(reader_for("file:///a/B.OBJ"), Some(Reader::Obj));
    assert_eq!(reader_for("https://h/x.Gltf?v=1"), Some(Reader::Gltf));
    assert_eq!(reader_for("qrc:/m/x.glb#top"), Some(Reader::Gltf));
    assert_eq!(reader_for("file:///a.obj/model.fbx"), None);
    assert_eq!(reader_for("file:///a.obj/model"), None);
}

#[test]
fn polygons_are_fanned_and_negative_indices_count_back() {
    let asset = built(&format!("{QUAD}f 1 2 3 4\n{QUAD}f -4 -3 -2\n"), "");
    assert_eq!(triangles(&asset.meshes[0].primitives[0]), 3);
    let (min, max) = asset.bounds.expect("has a box");
    assert_eq!((min.x, max.x, max.y), (0.0, 1.0, 1.0));
}

#[test]
fn a_face_may_name_a_vertex_that_comes_later() {
    let asset = built(&format!("f 1 2 3\n{QUAD}"), "");
    assert_eq!(triangles(&asset.meshes[0].primitives[0]), 1);
}

#[test]
fn a_broken_face_is_a_warning_and_the_rest_are_drawn() {
    for broken in [
        "f 1 2 9",
        "f 0 1 2",
        "f -9 1 2",
        "f 1 2",
        "f 1/9 2 3",
        "f a b c",
    ] {
        let asset = built(&format!("{QUAD}f 1 2 3\n{broken}\n"), "");
        assert_eq!(triangles(&asset.meshes[0].primitives[0]), 1, "{broken}");
        assert!(
            asset.warnings.iter().any(|w| w.contains("left out")),
            "{broken}: {:?}",
            asset.warnings
        );
    }
}

#[test]
fn groups_are_models_and_each_material_a_primitive() {
    let asset = built(
        &format!(
            "mtllib a.mtl\n{QUAD}f 1 2 3\no a\nusemtl x\nf 1 2 3\nusemtl y\nf 1 2 3\nusemtl x\nf 1 2 4\ng b\nf 1 2 3\no a\nf 2 3 4\n"
        ),
        "newmtl x\nnewmtl y\n",
    );
    let names: Vec<_> = asset.nodes.iter().map(|n| n.name.as_deref()).collect();
    assert_eq!(names, [None, Some("a"), Some("b")]);
    assert_eq!(asset.roots, [0, 1, 2]);
    let a = &asset.meshes[1];
    assert_eq!(a.primitives.len(), 2, "x and y");
    assert_eq!(
        triangles(&a.primitives[0]),
        3,
        "x, and the second visit to a"
    );
    assert_eq!(triangles(&a.primitives[1]), 1);
    assert_ne!(a.primitives[0].material, a.primitives[1].material);
    let b = &asset.meshes[2].primitives[0];
    assert_eq!(
        asset.materials[b.material].name.as_deref(),
        Some("x"),
        "a material stays on until another is named"
    );
}

#[test]
fn a_face_with_no_normals_takes_its_own() {
    let asset = built(&format!("{QUAD}f 1 2 3\nf 1 4 2\n"), "");
    let p = &asset.meshes[0].primitives[0];
    let v = vertices(p);
    assert_eq!(v.len(), 6, "flat faces share no corners");
    assert_eq!(v[0][3..6], [0.0, 0.0, 1.0]);
    assert_eq!(v[3][3..6], [0.0, 0.0, -1.0]);
}

#[test]
fn a_smoothing_group_shares_its_normals() {
    // Two faces, one facing +Z, one +X, meeting along x = 1.
    let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 1 0 -1\nv 1 1 -1\ns 1\nf 1 2 3\nf 2 4 5 3\n";
    let asset = built(obj, "");
    let v = vertices(&asset.meshes[0].primitives[0]);
    assert_eq!(v.len(), 5, "the corners along the edge are shared");
    let shared = &v[1][3..6];
    // The bigger face counts for more.
    assert!(shared[0] > shared[2] && shared[2] > 0.25, "{shared:?}");
    let far = &v[0][3..6];
    assert_eq!(far, [0.0, 0.0, 1.0]);

    let off = built(&obj.replace("s 1", "s off"), "");
    assert_eq!(vertices(&off.meshes[0].primitives[0]).len(), 9);
}

#[test]
fn the_files_normals_and_uvs_are_kept_with_v_turned_over() {
    let obj = format!("{QUAD}vt 0.25 0.25\nvn 0 2 0\nf 1/1/1 2/1/1 3//1\n");
    let v = vertices(&built(&obj, "").meshes[0].primitives[0]);
    assert_eq!(v[0][3..6], [0.0, 1.0, 0.0], "made unit");
    assert_eq!(v[0][6..8], [0.25, 0.75]);
    assert_eq!(v[2][6..8], [0.0, 0.0], "a corner with none");
}

#[test]
fn a_material_is_read_into_the_scene_material() {
    let mtl = "# a comment\nnewmtl m\nKd 0.5 1 0\nKe 2 0 0.5\nd 0.25\nNs 0\nillum 2\nnewmtl n\nTr 0.25\nNs 1000\nillum 0\nKd 0.5\n";
    let asset = built(
        &format!("mtllib a.mtl\n{QUAD}usemtl m\nf 1 2 3\nusemtl n\nf 1 2 4\n"),
        mtl,
    );
    let m = &asset.materials[0].material;
    assert!((m.base_color.r - 0.735).abs() < 0.01, "linear to sRGB");
    assert_eq!((m.base_color.g, m.base_color.b), (1.0, 0.0));
    assert_eq!(m.emissive, [2.0, 0.0, 0.5]);
    assert_eq!(m.opacity, 0.25);
    assert_eq!(m.roughness, 1.0, "no shine at all");
    assert_eq!(m.metalness, 0.0);
    assert_eq!(m.lighting, Lighting::Fragment);
    let n = &asset.materials[1].material;
    assert_eq!(n.opacity, 0.75);
    assert!(n.roughness > 0.1 && n.roughness < 0.3, "{}", n.roughness);
    assert_eq!(n.lighting, Lighting::None);
    assert_eq!(n.base_color.r, n.base_color.g, "one number is a grey");
    assert!(Material::Principled(m.clone()).blends());
}

#[test]
fn what_a_file_lacks_is_a_warning() {
    let mut doc = parse(format!("mtllib gone.mtl\n{QUAD}usemtl m\nf 1 2 3\n").as_bytes()).unwrap();
    doc.read_libraries("file:///a.obj", &mut |_| Err("no such file".to_owned()));
    let asset = doc.build(&mut |_| unreachable!("no pictures")).unwrap();
    let said = asset.warnings.join("\n");
    assert!(said.contains("gone.mtl"), "{said}");
    assert!(said.contains("material m"), "{said}");
    assert_eq!(asset.meshes[0].primitives[0].material, 0);
    assert_eq!(asset.materials[0].material.roughness, 1.0);
}

fn red_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[255, 0, 0, 255]).unwrap();
    writer.finish().unwrap();
    out
}

#[test]
fn files_are_found_beside_the_file_that_names_them() {
    let fetch = InMemory::default()
        .with(
            "https://h/m/a%20b.obj",
            format!("mtllib sub/my lib.mtl\n{QUAD}usemtl m\nf 1 2 3\n"),
        )
        .with(
            "https://h/m/sub/my%20lib.mtl",
            "newmtl m\nmap_Kd -clamp on -s 2 2 ../pic.png\nmap_bump -bm 0.5 pic.png\nmap_Ke C:\\art\\pic.png\nmap_d pic.png\n",
        )
        .with("https://h/m/pic.png", red_png())
        .with("https://h/m/sub/pic.png", red_png());
    let got = Rc::new(RefCell::new(None));
    let g = got.clone();
    load(&fetch, &(), "https://h/m/a%20b.obj", move |_, asset| {
        *g.borrow_mut() = Some(asset);
    });
    let asset = got.borrow_mut().take().expect("answered").expect("loaded");
    let t = &asset.materials[0].textures;
    let base = t.base_color.as_ref().expect("the one above");
    assert_eq!(base.sampler.wrap_s, Wrap::ClampToEdge);
    assert_eq!(t.normal.as_ref().unwrap().sampler.wrap_s, Wrap::Repeat);
    assert_eq!(t.normal_scale, 0.5);
    assert!(t.emissive.is_some(), "a Windows path's last part");
    assert_eq!(asset.materials[0].material.emissive, [1.0; 3]);
    assert_eq!(t.opacity_channel, Channel::R, "an opaque picture is grey");
    assert!(asset.warnings.iter().any(|w| w.contains("-s")));
    let asked = fetch.asked();
    assert_eq!(
        asked
            .iter()
            .filter(|u| *u == "https://h/m/sub/pic.png")
            .count(),
        1,
        "one fetch for each picture: {asked:?}"
    );
}

#[test]
fn load_any_goes_by_the_extension() {
    let fetch = InMemory::default().with("https://h/a.OBJ", format!("{QUAD}f 1 2 3\n"));
    let answer = |url: &str| {
        let got = Rc::new(RefCell::new(None));
        let g = got.clone();
        load_any(&fetch, &(), url, move |_, asset| {
            *g.borrow_mut() = Some(asset.map(|a| a.nodes.len()));
        });
        got.borrow_mut().take().expect("answered")
    };
    assert_eq!(answer("https://h/a.OBJ"), Ok(1));
    assert!(matches!(answer("https://h/a.fbx"), Err(LoadError::File(_))));
}

#[test]
fn what_is_not_an_obj_file_is_an_error() {
    for bad in [
        "",
        "# nothing\n",
        "v 1 2\n",
        "v nan 0 0\n",
        "vn 1\n",
        "vt x\n",
    ] {
        assert!(
            matches!(parse(bad.as_bytes()), Err(LoadError::Obj(_))),
            "{bad:?}"
        );
    }
    assert!(parse(b"glTF\0\0").is_err());
    assert!(parse(QUAD.as_bytes()).is_err(), "no faces");
}

#[test]
fn shininess_runs_from_rough_to_smooth() {
    assert_eq!(roughness_of(0.0), 1.0);
    assert!(roughness_of(10.0) > roughness_of(100.0));
    assert!(roughness_of(1e30) >= 0.0);
    assert_eq!(roughness_of(-5.0), 1.0);
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

fn survives(bytes: &[u8]) {
    if let Ok(mut doc) = parse(bytes) {
        doc.read_libraries("file:///a.obj", &mut |_| {
            Ok(b"newmtl m\nKd 1\nmap_Kd a.png\n".to_vec())
        });
        let _ = doc.build(&mut |_| Ok(vec![0; 8]));
    }
}

#[test]
fn every_prefix_and_many_mutations_are_survived() {
    let text = format!(
        "mtllib a.mtl\n{QUAD}vt 0 0\nvn 0 0 1\no a b\ns 1\nusemtl m\nf 1/1/1 2/1/1 3/1/1 4/1/1\ng c\ns off\nf -1 -2 -3\nl 1 2\np 1\n"
    );
    let bytes = text.as_bytes();
    for end in 0..bytes.len() {
        survives(&bytes[..end]);
    }
    let mut next = xorshift(0x1234_5678);
    let alphabet = b"0123456789-/ \n.\\vfgos";
    for _ in 0..4000 {
        let mut b = bytes.to_vec();
        for _ in 0..(next() % 4 + 1) {
            let at = next() as usize % b.len();
            b[at] = alphabet[next() as usize % alphabet.len()];
        }
        survives(&b);
    }
}
