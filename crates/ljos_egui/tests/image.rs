//! An `Image` in a QmlView: B0 uploads the picture through egui's texture
//! manager, and a picture decoded in the background wakes egui once.

use std::rc::Rc;
use std::sync::{Mutex, PoisonError};

use egui::{Pos2, Rect, TextureId, Vec2};
use egui_kittest::Harness;
use ljos_egui::QmlView;
use ljos_engine::{Engine, ObjectId, QmlType, Value};
use ljos_quick::{Image, ImageStore, Item, add_data, image_status};
use ljos_runtime::ComputePool;

/// A 4 by 2 PNG: the left half red, the right half blue.
fn two_halves() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 4, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let row = [
            [255, 0, 0, 255],
            [255, 0, 0, 255],
            [0, 0, 255, 255],
            [0, 0, 255, 255],
        ];
        let data: Vec<u8> = row.iter().chain(&row).flatten().copied().collect();
        w.write_image_data(&data).unwrap();
    }
    out
}

fn make<T: QmlType + Default>(e: &Engine, parent: Option<ObjectId>) -> ObjectId {
    let o = e.create_native(T::default(), parent).unwrap();
    if let Some(p) = parent {
        add_data(e, p, o).unwrap();
    }
    e.complete(o).unwrap();
    o
}

fn set(e: &Engine, o: ObjectId, name: &str, v: Value) {
    e.write_by_name(o, name, v)
        .unwrap_or_else(|err| panic!("{name}: {err}"));
}

struct State {
    view: QmlView,
    rect: Rect,
}

/// A root with one 80 by 40 Image at (10, 10), not smoothed so that the
/// halves keep a hard edge.
fn items(asynchronous: bool) -> (Rc<Engine>, ObjectId, ObjectId) {
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    ImageStore::of(&e).register_resource("halves.png", two_halves());
    let root = make::<Item>(&e, None);
    let image = make::<Image>(&e, Some(root));
    e.set(image, Item::X, 10.0).unwrap();
    e.set(image, Item::Y, 10.0).unwrap();
    e.set(image, Item::WIDTH, 80.0).unwrap();
    e.set(image, Item::HEIGHT, 40.0).unwrap();
    set(&e, image, "smooth", Value::Bool(false));
    set(&e, image, "asynchronous", Value::Bool(asynchronous));
    set(&e, image, "source", Value::Url("halves.png".into()));
    (e, root, image)
}

fn state(e: &Rc<Engine>, root: ObjectId) -> State {
    State {
        view: QmlView::new(Rc::clone(e), root).unwrap(),
        rect: Rect::NOTHING,
    }
}

/// Renderers are made one test at a time. Two threads that start the
/// Vulkan loader together can crash it: with the NVIDIA driver one calls
/// into the driver before the other has finished loading it, and the
/// process dies in `vkEnumerateInstanceExtensionProperties`.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// kittest's renderer, which filters "predictably".
fn renderer() -> egui_kittest::wgpu::WgpuTestRenderer {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    egui_kittest::wgpu::WgpuTestRenderer::new()
}

fn scene(asynchronous: bool) -> (Rc<Engine>, ObjectId, Harness<'static, State>) {
    let (e, root, image) = items(asynchronous);
    let h = Harness::builder()
        .with_size(Vec2::new(200.0, 120.0))
        .renderer(egui_kittest::LazyRenderer::new(renderer))
        .build_ui_state(
            |ui, st: &mut State| st.rect = st.view.show(ui).rect,
            state(&e, root),
        );
    (e, image, h)
}

fn status(e: &Engine, o: ObjectId) -> i32 {
    e.read_by_name(o, "status").to_number() as i32
}

/// The picture's texture is one egui manages, drawn over `p`.
fn textured_at(h: &Harness<'_, State>, p: Pos2) -> bool {
    h.output().shapes.iter().any(|cs| match &cs.shape {
        egui::Shape::Mesh(m) => {
            matches!(m.texture_id, TextureId::Managed(id) if id != 0)
                && cs.clip_rect.contains(p)
                && m.calc_bounds().contains(p)
        }
        _ => false,
    })
}

fn assert_painted(h: &mut Harness<'_, State>) {
    let origin = h.state().rect.min;
    let left = origin + Vec2::new(20.0, 30.0);
    let right = origin + Vec2::new(80.0, 30.0);
    let outside = origin + Vec2::new(120.0, 30.0);
    assert!(textured_at(h, left) && textured_at(h, right));
    assert!(!textured_at(h, outside));
    match h.render() {
        Ok(img) => {
            let px = |p: Pos2| img.get_pixel(p.x as u32, p.y as u32).0;
            assert_eq!(px(left), [255, 0, 0, 255]);
            assert_eq!(px(right), [0, 0, 255, 255]);
            assert_ne!(px(outside), [0, 0, 255, 255]);
        }
        // No GPU adapter here: the textured mesh above is the evidence.
        Err(err) => eprintln!("render unavailable ({err}); checked the shapes only"),
    }
}

#[test]
fn an_image_is_painted_through_a_texture() {
    let (e, image, mut h) = scene(false);
    assert_eq!(status(&e, image), image_status::READY);
    h.run();
    assert_painted(&mut h);
}

#[test]
fn a_picture_decoded_in_the_background_wakes_egui_and_then_it_idles() {
    let (e, image, mut h) = scene(true);
    // The first pass binds the context; whether the decode is done by then
    // or not, the worker is what asks for the pass that shows it.
    h.step();
    ComputePool::global().wait_idle();
    if status(&e, image) == image_status::LOADING {
        assert!(h.ctx.has_requested_repaint(), "the result woke egui");
        h.step();
    }
    assert_eq!(status(&e, image), image_status::READY);
    h.run();
    assert_painted(&mut h);

    // Nothing left to do: no frames, no repaints.
    let frames = h.state().view.stats().frames;
    for _ in 0..3 {
        h.step();
        assert!(!h.ctx.has_requested_repaint());
    }
    assert_eq!(h.state().view.stats().frames, frames);
    assert_eq!(ImageStore::of(&e).pending(), 0);
}

/// egui_kittest's renderer filters textures in the shader, "predictably":
/// always linear and always clamped, whatever the texture's sampler says.
/// Tiling and `smooth: false` are sampler settings, so they are checked
/// with a renderer that samples as a window's does.
fn sampling_renderer() -> Option<egui_kittest::wgpu::WgpuTestRenderer> {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    // The default options are those of a window: hardware sampling.
    std::panic::catch_unwind(|| {
        egui_kittest::wgpu::WgpuTestRenderer::with_render_options(Default::default())
    })
    .ok()
}

#[test]
fn tiling_and_nearest_sampling_reach_the_gpu() {
    let Some(renderer) = sampling_renderer() else {
        eprintln!("no GPU adapter; nothing to look at");
        return;
    };
    let (e, root, image) = items(false);
    let mut h = Harness::builder()
        .with_size(Vec2::new(200.0, 120.0))
        .renderer(renderer)
        .build_ui_state(
            |ui, st: &mut State| st.rect = st.view.show(ui).rect,
            state(&e, root),
        );
    h.run();
    let origin = h.state().rect.min;
    let row = |h: &mut Harness<'_, State>, y: f32| -> String {
        let img = h.render().expect("a renderer was made");
        (0..80)
            .map(|x| {
                let p = origin + Vec2::new(10.5 + x as f32, 10.0 + y);
                // The window's options dither, so colours are near, not exact.
                match img.get_pixel(p.x as u32, p.y as u32).0 {
                    [r, g, b, _] if r > 250 && g < 5 && b < 5 => 'r',
                    [r, g, b, _] if r < 5 && g < 5 && b > 250 => 'b',
                    _ => '?',
                }
            })
            .collect()
    };
    // Stretched without smoothing: two flat halves with a hard edge.
    let stretched = format!("{}{}", "r".repeat(40), "b".repeat(40));
    for y in [0.0, 20.0, 39.0] {
        assert_eq!(row(&mut h, y), stretched, "row {y}");
    }
    // Tiled: copies of the 4 by 2 picture, the middle one centred, so the
    // columns start at 38 + 4n: the row begins half-way into a copy.
    set(
        &e,
        image,
        "fillMode",
        Value::Int(ljos_quick::fill_mode::TILE),
    );
    h.run();
    let tiled = "bbrr".repeat(20);
    for y in [0.0, 20.0, 39.0] {
        assert_eq!(row(&mut h, y), tiled, "row {y}");
    }
}
