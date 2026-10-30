//! Writes the lit cube of `tests/spike_scene.rs` to a PNG:
//! `cargo run -p ljos_3d --example lit_cube -- cube.png [width height]`.
//! The size is there so a host drawing the same scene at its own size,
//! such as a `<canvas>` through `SurfaceView`, has a reference to compare.

#[path = "../tests/spike_scene.rs"]
// The tests share the scene; the example draws it but checks nothing.
#[allow(dead_code)]
mod spike_scene;

use std::io::BufWriter;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "lit_cube.png".into());
    let mut size = args.map(|a| a.parse::<u32>().expect("a size in pixels"));
    let width = size.next().unwrap_or(spike_scene::WIDTH);
    let height = size.next().unwrap_or(spike_scene::HEIGHT);
    let mut gpu = ljos_3d::offscreen::Offscreen::new().expect("an adapter");
    let picture = gpu
        .render(&spike_scene::lit_cube(spike_scene::grey()), width, height)
        .expect("a picture");
    let file = std::fs::File::create(&path).expect("the file");
    let mut encoder = png::Encoder::new(BufWriter::new(file), picture.width, picture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&picture.pixels))
        .expect("the PNG");
    println!(
        "{path}: {}x{} on {} ({:?})",
        picture.width, picture.height, gpu.adapter.name, gpu.adapter.backend
    );
}
