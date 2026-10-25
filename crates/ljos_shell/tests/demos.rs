//! Renders the two example scenes offscreen through the real shell and B0 on
//! wgpu, so they can be looked at without a window manager in the way.
//!
//! Set `LJOS_SHOTS_DIR` to a directory to also write `demo.png`,
//! `demo_clicked.png`, `demo_qml.png`, `demo_qml_clicked.png` and the
//! `demo_qml_combo.png`, `demo_qml_menu.png` and `demo_qml_dialog.png` of
//! the open popups there, and the `tiled.png` of an image that is tiled.
//! Without wgpu the tests skip the pixels and say so.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

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

/// What draws the pictures: a renderer that samples textures as a window
/// does. kittest's default filters them in its shader, for pictures that
/// are the same on every GPU, and that filter is always linear and always
/// clamped, whatever the texture's sampler says: an image that is tiled
/// came out as one copy with its edges smeared outwards, where a window
/// tiles it. Nothing else of the default is given up (no dithering, no
/// multisampling).
///
/// Without a GPU adapter the pictures are skipped: a renderer that says
/// it cannot draw.
///
/// Renderers are made one test at a time: two threads that start the
/// Vulkan loader together can crash it (with the NVIDIA driver one calls
/// into the driver before the other has finished loading it).
fn sampling<S>(builder: egui_kittest::HarnessBuilder<S>) -> egui_kittest::HarnessBuilder<S> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let options = egui_wgpu::RendererOptions {
        predictable_texture_filtering: false,
        ..egui_wgpu::RendererOptions::PREDICTABLE
    };
    match std::panic::catch_unwind(|| {
        egui_kittest::wgpu::WgpuTestRenderer::with_render_options(options)
    }) {
        Ok(renderer) => builder.renderer(renderer),
        Err(_) => builder.renderer(egui_kittest::LazyRenderer::Uninitialized {
            textures_delta: egui::TexturesDelta::default(),
            builder: None,
        }),
    }
}

fn harness(
    setup: impl FnOnce(&mut Setup<'_, String>) -> Result<ObjectId, SetupError> + 'static,
) -> Harness<'static, ShellApp<String>> {
    let config = ShellConfig::default();
    // kittest wants every input of the tree named, and takes AccessKit's
    // Splitter for one; a QML separator is a Splitter there (AccessKit has
    // no separator) and has no name, in Qt neither. What the scene says is
    // checked in features/accessibility.
    let builder = Harness::builder()
        .with_size(SIZE)
        .with_accessibility_check(false);
    let mut h = sampling(builder).build_eframe(move |cc| {
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

/// The object named `name` below `root`.
fn named(e: &ljos_engine::Engine, root: ObjectId, name: &str) -> ObjectId {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if e.read_by_name(o, "objectName").to_js_string().as_str() == name {
            return o;
        }
        stack.extend(e.children(o));
    }
    panic!("no object named {name}")
}

/// The middle of an item, in the window.
fn centre(e: &ljos_engine::Engine, item: ObjectId) -> Pos2 {
    let (mut x, mut y) = (0.0, 0.0);
    let mut at = Some(item);
    while let Some(i) = at {
        x += e.get(i, Item::X).unwrap();
        y += e.get(i, Item::Y).unwrap();
        at = e.visual_parent(i);
    }
    let (w, h) = (
        e.get(item, Item::WIDTH).unwrap(),
        e.get(item, Item::HEIGHT).unwrap(),
    );
    Pos2::new((x + w / 2.0) as f32, (y + h / 2.0) as f32)
}

#[test]
fn demo_qml_opens_its_popups() {
    let source = demo_qml::compile(&demo_qml::default_dir()).unwrap();
    let mut h = harness(move |s| {
        let e = s.engine();
        e.set_component_source(source);
        Ok(e.create_from_url("Main", None, Vec::new())
            .map_err(|e| e.to_string())?)
    });
    let root = h.state().view().scene().root().unwrap();
    let flag = |h: &Harness<'_, ShellApp<String>>, o: ObjectId, name: &str| {
        h.state().engine().read_by_name(o, name).to_boolean()
    };
    let at = |h: &Harness<'_, ShellApp<String>>, name: &str| {
        let e = h.state().engine();
        let o = named(e, root, name);
        (o, centre(e, o))
    };

    // The rows of the combo box, and a click on the last of them.
    let (combo, on_combo) = at(&h, "size");
    click(&mut h, on_combo);
    assert!(flag(&h, combo, "down"), "the combo box did not open");
    shoot(&mut h, "demo_qml_combo.png", on_combo);
    click(&mut h, on_combo + Vec2::new(0.0, 120.0));
    let e = h.state().engine();
    assert_eq!(e.read_by_name(combo, "currentText"), Value::string("Large"));
    assert_eq!(e.read_by_name(root, "size"), Value::string("Large"));
    assert!(!flag(&h, combo, "down"));

    // The menu under its button, and the dialog its first item opens.
    let (_, on_menu_button) = at(&h, "menuButton");
    // A popup is no item, and has no place among them to ask for.
    let menu = named(h.state().engine(), root, "menu");
    click(&mut h, on_menu_button);
    assert!(flag(&h, menu, "visible"), "the menu did not open");
    shoot(&mut h, "demo_qml_menu.png", on_menu_button);
    click(&mut h, on_menu_button + Vec2::new(0.0, 40.0));
    let dialog = named(h.state().engine(), root, "dialog");
    assert!(!flag(&h, menu, "visible"));
    assert!(flag(&h, dialog, "visible"), "the dialog did not open");
    shoot(&mut h, "demo_qml_dialog.png", on_menu_button);

    // The dialog is modal: the button under its shade is out of reach.
    click(&mut h, BUTTON_CENTRE);
    let e = h.state().engine();
    assert_eq!(e.read_by_name(root, "clicks"), Value::Int(0));
}

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

#[test]
fn a_tiled_image_is_tiled_in_the_picture_too() {
    use ljos_quick::{Image, ImageStore, add_data};

    let mut h = harness(move |s| {
        let e = s.engine();
        ImageStore::of(e).register_resource("halves.png", two_halves());
        let root = e
            .create_native(Item::default(), None)
            .map_err(|e| e.to_string())?;
        e.complete(root).map_err(|e| e.to_string())?;
        let image = e
            .create_native(Image::default(), Some(root))
            .map_err(|e| e.to_string())?;
        add_data(e, root, image).map_err(|e| e.to_string())?;
        e.complete(image).map_err(|e| e.to_string())?;
        for (name, value) in [
            ("x", Value::Real(10.0)),
            ("y", Value::Real(10.0)),
            ("width", Value::Real(80.0)),
            ("height", Value::Real(40.0)),
            ("smooth", Value::Bool(false)),
            // `Image.Tile`.
            ("fillMode", Value::Int(3)),
            ("source", Value::Url("halves.png".into())),
        ] {
            e.write_by_name(image, name, value)
                .map_err(|e| e.to_string())?;
        }
        Ok(root)
    });
    h.run();
    // Twenty tiles side by side, each half red and half blue. Stretched
    // from one tile's edge, everything after the first would be one colour.
    let img = match h.render() {
        Ok(img) => img,
        Err(err) => {
            eprintln!("render unavailable ({err}); skipping the tiles");
            return;
        }
    };
    let (w, ht) = img.dimensions();
    if let Some(dir) = shots_dir() {
        std::fs::create_dir_all(&dir).unwrap();
        write_png(&dir.join("tiled.png"), w, ht, img.as_raw());
    }
    let scale = w as f32 / SIZE.x;
    // Copies of a picture that is half red and half blue: the colour
    // changes every second pixel. Stretched it changes once, and so does
    // one copy with its edges smeared outwards.
    let row: Vec<char> = (0..80)
        .map(|x| {
            let at = Pos2::new(10.5 + x as f32, 30.0);
            let px = img
                .get_pixel((at.x * scale) as u32, (at.y * scale) as u32)
                .0;
            match px {
                px if is_red(px) => 'r',
                px if is_blue(px) => 'b',
                _ => '?',
            }
        })
        .collect();
    let text: String = row.iter().collect();
    let changes = row.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(!text.contains('?'), "not the picture everywhere: {text}");
    assert_eq!(changes, 39, "the image is not tiled: {text}");
}
