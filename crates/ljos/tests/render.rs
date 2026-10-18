//! Pictures of an application without a window (the `render` feature),
//! taken of what `App` builds — native types included.

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Swatch")]
pub struct Swatch {
    #[qml(notify)]
    pub shade: String,
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Swatch]));
}

fn application(qml: &str) -> (tempfile::TempDir, ljos::App) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), qml).unwrap();
    let app = ljos::App::new("Test")
        .register(register)
        .qml_dir(dir.path(), "Main")
        .unwrap();
    (dir, app)
}

/// The RGBA of the pixel at `(x, y)` of a `width`-wide PNG.
fn pixel(png: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let image = ljos_render::RgbaImage::from_png(png).unwrap();
    assert_eq!(image.width as usize, width);
    let i = (y * width + x) * 4;
    image.data[i..i + 4].try_into().unwrap()
}

#[test]
fn render_png_draws_what_app_build_made_with_a_native_type() {
    let (_dir, app) = application(
        "import QtQuick\nimport Example\nRectangle {\n  width: 20; height: 10\n  Swatch { id: s; shade: \"red\" }\n  color: s.shade\n}\n",
    );
    let (engine, root) = app.build().unwrap();
    let png = ljos::render::render_png(&engine, root, (40, 30), 0).unwrap();
    assert_eq!(pixel(&png, 40, 5, 5), [255, 0, 0, 255]);
    // An item is drawn on white, as a window with no colour shows it.
    assert_eq!(pixel(&png, 40, 30, 20), [255, 255, 255, 255]);
}

#[test]
fn a_picture_installed_before_the_root_lets_its_timers_run() {
    ljos::render::set_fonts(ljos::render::Fonts::Bundled);
    let (_dir, app) = application(
        "import QtQuick\nRectangle {\n  width: 40; height: 30; color: \"red\"\n  Timer { interval: 50; running: true; onTriggered: parent.color = \"blue\" }\n}\n",
    );
    let picture = ljos::render::Picture::new(40, 30).settle(100);
    let engine = app.build_engine();
    picture.install(&engine);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    // No time has passed: the timer has not fired.
    let still = ljos::render::Picture::new(40, 30);
    still.install(&engine);
    assert_eq!(
        pixel(&still.png(&engine, root).unwrap(), 40, 0, 0),
        [255, 0, 0, 255]
    );
    picture.install(&engine);
    assert_eq!(
        pixel(&picture.png(&engine, root).unwrap(), 40, 0, 0),
        [0, 0, 255, 255]
    );
}

#[test]
fn a_window_keeps_its_colour_unless_asked_not_to() {
    let qml = "import QtQuick\nWindow {\n  color: \"blue\"\n  Rectangle { width: 20; height: 10; color: \"red\" }\n}\n";
    let (_dir, app) = application(qml);
    let (engine, root) = app.build().unwrap();
    let png = ljos::render::render_png(&engine, root, (40, 30), 0).unwrap();
    assert_eq!(pixel(&png, 40, 5, 5), [255, 0, 0, 255]);
    assert_eq!(pixel(&png, 40, 30, 20), [0, 0, 255, 255]);

    let (_dir, app) = application(qml);
    let (engine, root) = app.build().unwrap();
    let picture = ljos::render::Picture::new(40, 30).without_window_colour();
    picture.install(&engine);
    let png = picture.png(&engine, root).unwrap();
    assert_eq!(pixel(&png, 40, 30, 20)[3], 0);
}

#[test]
fn a_picture_waits_for_the_answers_the_scene_was_promised() {
    let (_dir, app) = application(
        "import QtQuick\nRectangle { objectName: \"root\"; width: 40; height: 30; color: \"red\" }\n",
    );
    let (engine, root) = app.build().unwrap();
    // As a fetch is answered: on the Engine thread, after real time has
    // passed that the virtual clock knows nothing of.
    let reply = Platform::of(&engine).expect();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        reply.send(move |e| {
            e.write_by_name(root, "color", Value::from("blue")).unwrap();
        });
    });
    let png = ljos::render::render_png(&engine, root, (40, 30), 0).unwrap();
    assert_eq!(pixel(&png, 40, 5, 5), [0, 0, 255, 255]);
}
