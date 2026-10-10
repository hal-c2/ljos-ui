//! Renders the two example scenes offscreen through the real shell and B0 on
//! wgpu, so they can be looked at without a window manager in the way.
//!
//! Set `LJOS_SHOTS_DIR` to a directory to also write `demo.png`,
//! `demo_clicked.png`, `demo_qml.png` and `demo_qml_clicked.png` there.
//! Without wgpu the tests skip the pixels and say so.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use egui::{Event, Modifiers, PointerButton, Pos2, Vec2};
use egui_kittest::Harness;
use ljos_engine::{ObjectId, Value};
use ljos_quick::Item;
use ljos_runtime::SystemClock;
use ljos_shell::{Setup, SetupError, ShellApp, ShellConfig};

#[path = "../examples/shared/demo_scene.rs"]
mod demo_scene;

#[path = "../examples/shared/demo_qml.rs"]
mod demo_qml;

const SIZE: Vec2 = Vec2::new(640.0, 480.0);

/// Both demos anchor a 160×56 button 24 px from the bottom-right corner.
const BUTTON_CENTRE: Pos2 = Pos2::new(SIZE.x - 24.0 - 80.0, SIZE.y - 24.0 - 28.0);

fn harness(
    setup: impl FnOnce(&mut Setup<'_, String>) -> Result<ObjectId, SetupError> + 'static,
) -> Harness<'static, ShellApp<String>> {
    let config = ShellConfig::default();
    let mut h = Harness::builder().with_size(SIZE).build_eframe(move |cc| {
        ShellApp::new(&cc.egui_ctx, &config, Arc::new(SystemClock::new()), setup).unwrap()
    });
    h.run();
    h
}

fn click(h: &mut Harness<'_, ShellApp<String>>, p: Pos2) {
    h.hover_at(p);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
}

fn shots_dir() -> Option<PathBuf> {
    std::env::var_os("LJOS_SHOTS_DIR").map(PathBuf::from)
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

/// Renders the current frame, writes it as `name` when asked to, and
/// returns the pixel at `p`; `None` when no wgpu adapter is available.
fn shoot(h: &mut Harness<'_, ShellApp<String>>, name: &str, p: Pos2) -> Option<[u8; 4]> {
    let img = match h.render() {
        Ok(img) => img,
        Err(err) => {
            eprintln!("render unavailable ({err}); skipping {name}");
            return None;
        }
    };
    let (w, ht) = img.dimensions();
    if let Some(dir) = shots_dir() {
        std::fs::create_dir_all(&dir).unwrap();
        write_png(&dir.join(name), w, ht, img.as_raw());
    }
    // The renderer may use a pixels-per-point other than 1.
    let scale = w as f32 / SIZE.x;
    let (x, y) = ((p.x * scale) as u32, (p.y * scale) as u32);
    Some(img.get_pixel(x, y).0)
}

/// A swatch-free spot inside the button: left of the centred label.
fn button_probe() -> Pos2 {
    BUTTON_CENTRE - Vec2::new(70.0, 0.0)
}

fn is_blue(px: [u8; 4]) -> bool {
    px[2] > 150 && px[0] < 100
}

fn is_red(px: [u8; 4]) -> bool {
    px[0] > 150 && px[2] < 120
}

#[test]
fn demo_renders_and_its_button_clicks() {
    let button = Rc::new(Cell::new(None));
    let slot = Rc::clone(&button);
    let mut h = harness(move |s| {
        let built = demo_scene::build(s.engine()).map_err(|e| e.to_string())?;
        let status = built.status;
        s.on_message(move |e, text| {
            let _ = e.write_by_name(status, "text", Value::string(text));
        });
        slot.set(Some(built.button));
        Ok(built.root)
    });
    let button = button.get().unwrap();
    let e = h.state().engine();
    let geometry = [Item::X, Item::Y, Item::WIDTH, Item::HEIGHT].map(|p| e.get(button, p).unwrap());
    assert_eq!(geometry, [456.0, 400.0, 160.0, 56.0], "button anchors");
    // What the example's worker thread would post.
    h.state()
        .handle()
        .send("Hello from a worker thread".to_owned())
        .unwrap();
    h.run();

    let px = shoot(&mut h, "demo.png", button_probe());
    if let Some(px) = px {
        assert!(is_blue(px), "button before the click: {px:?}");
    }

    click(&mut h, BUTTON_CENTRE);
    let px = shoot(&mut h, "demo_clicked.png", button_probe());
    if let Some(px) = px {
        assert!(is_red(px), "button after the click: {px:?}");
    }
}

#[test]
fn demo_qml_renders_and_its_button_clicks() {
    let source = demo_qml::compile(&demo_qml::default_dir()).unwrap();
    let mut h = harness(move |s| {
        let e = s.engine();
        e.set_component_source(source);
        Ok(e.create_from_url("Main", None, Vec::new())
            .map_err(|e| e.to_string())?)
    });
    let root = h.state().view().scene().root().unwrap();
    // Shot first, so a broken scene can still be looked at.
    let px = shoot(&mut h, "demo_qml.png", button_probe());
    let e = h.state().engine();
    assert_eq!(e.get(root, Item::WIDTH).unwrap(), 640.0);
    assert!(
        !e.visual_children(root).is_empty(),
        "IR children are in `data` ({:?}) but not visual children",
        e.read_by_name(root, "data")
    );
    if let Some(px) = px {
        assert!(is_blue(px), "button before the click: {px:?}");
    }

    click(&mut h, BUTTON_CENTRE);
    assert_eq!(
        h.state().engine().read_by_name(root, "clicks"),
        Value::Int(1)
    );
    let px = shoot(&mut h, "demo_qml_clicked.png", button_probe());
    if let Some(px) = px {
        assert!(is_red(px), "button after the click: {px:?}");
    }
}
