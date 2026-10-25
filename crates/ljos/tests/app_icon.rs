//! The application's icon, from pixels or a picture file, and a window's
//! own in place of it: what the shell gives each platform's windows and
//! the Dock. What the window manager is given is checked off-screen on
//! X11 (`crates/ljos_bdd/tests/x11_icon.rs`).

use ljos::prelude::*;
use ljos::window::{Icon, IconError};
use ljos_quick::ImageStore;

fn pixel(rgba: [u8; 4]) -> Icon {
    Icon::from_rgba(1, 1, rgba.to_vec()).unwrap()
}

#[test]
fn the_app_builder_gives_the_application_its_icon() {
    let engine = ljos::App::new("Notes")
        .icon(pixel([1, 2, 3, 255]))
        .build_engine();
    let platform = Platform::of(&engine);
    assert_eq!(platform.application_icon(), Some(pixel([1, 2, 3, 255])));
    // A window without its own shows the application's.
    assert_eq!(platform.window_icon(None), Some(pixel([1, 2, 3, 255])));
}

#[test]
fn pixels_that_do_not_fill_the_size_are_no_icon() {
    assert!(matches!(
        Icon::from_rgba(2, 2, vec![0; 4]),
        Err(IconError::Size { expected: 16, .. })
    ));
}

#[test]
fn an_icon_from_a_file_is_given_once_it_is_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("icon.png");
    let png = ljos_render::RgbaImage::filled(3, 2, [255, 0, 0, 128]);
    std::fs::write(&path, png.to_png().unwrap()).unwrap();

    let engine = ljos::App::new("Notes").icon_file(&path).build_engine();
    assert_eq!(Platform::of(&engine).application_icon(), None);
    ImageStore::of(&engine).finish(&engine);
    let icon = Platform::of(&engine).application_icon().expect("an icon");
    assert_eq!((icon.width(), icon.height()), (3, 2));
    // Straight again, as it was written.
    assert_eq!(&icon.rgba()[..4], &[255, 0, 0, 128]);
}

#[test]
fn a_file_that_cannot_be_read_is_a_warning_and_no_icon() {
    let dir = tempfile::tempdir().unwrap();
    let engine = ljos::App::new("Notes")
        .icon_file(dir.path().join("missing.png"))
        .build_engine();
    ImageStore::of(&engine).finish(&engine);
    assert_eq!(Platform::of(&engine).application_icon(), None);
    let log = engine.take_log();
    assert!(
        log.iter().any(|e| e.level == LogLevel::Warning
            && e.message.contains("missing.png")
            && e.message.contains("icon")),
        "{log:?}"
    );
}

#[test]
fn a_window_shows_its_own_icon_in_place_of_the_applications() {
    let engine = ljos::App::new("Notes")
        .icon(pixel([1, 1, 1, 255]))
        .build_engine();
    let window = engine
        .create_by_name("QtQuick", "Window", None)
        .expect("a Window");
    let platform = Platform::of(&engine);
    platform.set_window_icon(window, Some(pixel([2, 2, 2, 255])));
    assert_eq!(
        platform.window_icon(Some(window)),
        Some(pixel([2, 2, 2, 255]))
    );
    platform.set_window_icon(window, None);
    assert_eq!(
        platform.window_icon(Some(window)),
        Some(pixel([1, 1, 1, 255]))
    );
}
