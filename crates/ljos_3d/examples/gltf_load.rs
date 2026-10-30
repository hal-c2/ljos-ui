//! How long a glTF file takes to read into a model, as a RuntimeLoader's
//! pool task does it: parse, then build (buffers, accessors, images).
//!
//! cargo run --release -p ljos_3d --features gltf --example gltf_load -- FILE [ROUNDS]

use std::time::Instant;

use ljos_3d::gltf;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: gltf_load FILE [ROUNDS]");
        std::process::exit(2);
    };
    let rounds: usize = args.next().and_then(|r| r.parse().ok()).unwrap_or(50);
    let bytes = std::fs::read(&path).unwrap_or_else(|err| {
        eprintln!("{path}: {err}");
        std::process::exit(1);
    });
    let dir = std::path::Path::new(&path)
        .parent()
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    let mut times = Vec::with_capacity(rounds);
    // The loader converts every index to u32: three of four bytes each.
    let (mut triangles, mut nodes) = (0, 0);
    for _ in 0..rounds {
        let start = Instant::now();
        let doc = gltf::parse(&bytes).expect("parses");
        let asset = doc
            .build(&mut |uri| std::fs::read(dir.join(uri)).map_err(|e| e.to_string()))
            .expect("builds");
        times.push(start.elapsed());
        nodes = asset.nodes.len();
        triangles = asset
            .meshes
            .iter()
            .flat_map(|m| &m.primitives)
            .map(|p| p.geometry.index_data().len() / 12)
            .sum::<usize>();
    }
    times.sort();
    println!(
        "{path}: {} bytes, {nodes} nodes, {triangles} triangles in its meshes; median {:?}, fastest {:?} over {rounds} rounds",
        bytes.len(),
        times[rounds / 2],
        times[0]
    );
}
