//! Whether a family is fixed-pitch, and which is the system's fixed one,
//! asked from Rust: an application picks a terminal's face, or gives a
//! monospace interface face other leading, from the font's characters
//! rather than its name.

mod common;

use ljos::item::Font;
use ljos::prelude::*;

fn family(name: &str) -> Font {
    Font {
        family: Atom::from(name),
        ..Font::default()
    }
}

#[test]
fn the_system_s_fixed_family_is_fixed_pitch_and_a_proportional_one_is_not() {
    let _engine = ljos::engine();
    ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
    let fixed = ljos::fixed_font_family();
    assert!(!fixed.is_empty() && fixed != "monospace", "{fixed:?}");
    let info = ljos::font_info(&family(&fixed));
    assert_eq!(&*info.family, fixed);
    assert!(info.fixed_pitch, "{info:?}");
    // fonts-liberation, which CI installs.
    let sans = ljos::font_info(&family("Liberation Sans"));
    assert_eq!(&*sans.family, "Liberation Sans");
    assert!(!sans.fixed_pitch, "{sans:?}");
    assert!(ljos::font_info(&family("Liberation Mono")).fixed_pitch);
}

/// A FontInfo that has told its face goes on telling the face of the text
/// engine there is now: a host that swaps engines (the bundled fonts for
/// the system's, and back) is not left with the old engine's answer, and
/// knows it at once, before any frame.
#[test]
fn a_font_info_follows_the_text_engine_it_is_given() {
    let engine = ljos::engine();
    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    let root = common::create_main(
        &engine,
        "import QtQuick\nItem {\n\
         FontInfo { id: mono; font.family: \"Liberation Mono\" }\n\
         property string family: mono.family\n\
         property bool fixed: mono.fixedPitch\n}\n",
    );
    let family = || {
        engine
            .to_js_string(&engine.read_by_name(root, "family"))
            .to_string()
    };
    let fixed = || engine.read_by_name(root, "fixed").to_boolean();
    let bundled = family();
    assert_ne!(
        bundled, "Liberation Mono",
        "only the bundled fonts at first"
    );

    ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
    // fonts-liberation, which CI installs.
    assert_eq!(family(), "Liberation Mono");
    assert!(fixed());

    // Another fresh engine: one that has changed no more than the last.
    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    assert_eq!(family(), bundled);
}

/// The test font of the scenarios, whose one face is fixed-pitch.
fn alternate_font() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/text/fonts/LjosAlternateTest-Regular.ttf")
        .canonicalize()
        .unwrap()
}

/// A root whose `family` follows a FontInfo of `family`.
fn following(engine: &Engine, family: &str) -> ObjectId {
    let root = common::create_main(
        engine,
        &format!(
            "import QtQuick\nItem {{\n\
             FontInfo {{ id: info; font.family: \"{family}\" }}\n\
             property string family: info.family\n}}\n"
        ),
    );
    // Read once, so that it has cached what it resolves to now.
    let _ = engine.read_by_name(root, "family");
    root
}

fn family_of(engine: &Engine, root: ObjectId) -> String {
    engine
        .to_js_string(&engine.read_by_name(root, "family"))
        .to_string()
}

/// One engine's FontLoader tells another engine on the thread, whose
/// effect reads its FontInfo at once: that effect may lay text out, and
/// sees the face just loaded.
#[test]
fn an_engine_that_reads_its_font_info_when_another_loads_a_font_sees_it() {
    use std::cell::RefCell;
    use std::rc::Rc;

    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    let reader = ljos::engine();
    let root = following(&reader, "Ljos Alternate Test");
    assert_ne!(family_of(&reader, root), "Ljos Alternate Test");
    let seen = Rc::new(RefCell::new(Vec::new()));
    let told = Rc::clone(&seen);
    reader
        .effect(move |e| told.borrow_mut().push(family_of(e, root)))
        .unwrap();

    let loader = ljos::engine();
    let url = format!("file://{}", alternate_font().display());
    common::create_main(
        &loader,
        &format!("import QtQuick\nFontLoader {{ source: \"{url}\" }}\n"),
    );
    assert_eq!(
        seen.borrow().last().map(String::as_str),
        Some("Ljos Alternate Test")
    );
}

/// An effect of one engine that reads another engine's FontInfo while the
/// set of fonts changes sees the fonts there are now: every engine is told
/// before any effect runs.
#[test]
fn an_effect_reading_another_engine_s_font_info_sees_the_new_fonts() {
    use std::cell::RefCell;
    use std::rc::Rc;

    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    let first = ljos::engine();
    let own = following(&first, "Liberation Mono");
    let second = ljos::engine();
    let other = following(&second, "Liberation Mono");
    let bundled = family_of(&second, other);
    assert_ne!(
        bundled, "Liberation Mono",
        "only the bundled fonts at first"
    );

    let seen = Rc::new(RefCell::new(Vec::new()));
    let told = Rc::clone(&seen);
    let of = second.clone();
    first
        .effect(move |e| {
            // Its own, which makes it run when the fonts change.
            let _ = family_of(e, own);
            told.borrow_mut().push(family_of(&of, other));
        })
        .unwrap();

    // fonts-liberation, which CI installs.
    ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
    assert_eq!(
        seen.borrow().last().map(String::as_str),
        Some("Liberation Mono"),
        "{seen:?}"
    );
}

/// A host that registers a font on the text engine itself, before it makes
/// the scene: the scene's first frame finds the change no one was told of.
#[test]
fn a_font_registered_on_the_engine_before_the_scene_is_followed_on_its_first_frame() {
    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    let engine = ljos::engine();
    let root = following(&engine, "Ljos Alternate Test");
    assert_ne!(family_of(&engine, root), "Ljos Alternate Test");

    let bytes = std::fs::read(alternate_font()).unwrap();
    ljos_quick::text_engine()
        .borrow_mut()
        .register_font_faces(bytes)
        .unwrap();
    let mut scene = ljos_quick::QuickScene::new(ljos::item::kurbo::Size::new(100.0, 100.0));
    scene.set_root(&engine, root).unwrap();
    scene.frame(&engine, Time::ZERO);
    assert_eq!(family_of(&engine, root), "Ljos Alternate Test");
}

/// An effect that changes the fonts again while engines are being told of a
/// change: every engine ends on the fonts of the last change.
#[test]
fn an_effect_that_changes_the_fonts_while_they_are_told_ends_on_the_last() {
    use std::cell::Cell;
    use std::rc::Rc;

    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    let first = ljos::engine();
    let own = following(&first, "Liberation Mono");
    let second = ljos::engine();
    let other = following(&second, "Liberation Mono");
    let bundled = family_of(&second, other);

    let swapped = Rc::new(Cell::new(false));
    let once = Rc::clone(&swapped);
    first
        .effect(move |e| {
            if family_of(e, own) == "Liberation Mono" && !once.replace(true) {
                ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
            }
        })
        .unwrap();

    // fonts-liberation, which CI installs.
    ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
    assert!(swapped.get(), "the effect saw the system's fonts");
    assert_eq!(family_of(&first, own), bundled);
    assert_eq!(family_of(&second, other), bundled);
}

/// A FontInfo made while the text engine is still being read on another
/// thread: making it and reading what needs no face does not wait for the
/// engine, and once the engine is there it tells that engine's face and
/// follows the engines after it.
#[test]
fn a_font_info_made_while_the_engine_is_being_made_follows_it_once_it_is_there() {
    use std::sync::mpsc;
    use std::time::Duration;

    let (go, gate) = mpsc::channel::<()>();
    // Held until the test lets it go: one that is joined before then waits
    // out the timeout, and the engine is in use before it should be.
    ljos_quick::set_text_engine_later(std::thread::spawn(move || {
        let _ = gate.recv_timeout(Duration::from_secs(5));
        ljos_quick::TextEngine::with_system_fonts()
    }));
    let engine = ljos::engine();
    let root = common::create_main(
        &engine,
        "import QtQuick\nItem {\n\
         FontInfo { id: mono; font.family: \"Liberation Mono\"; font.pixelSize: 20 }\n\
         property int size: mono.pixelSize\n\
         property string family: mono.family\n}\n",
    );
    assert_eq!(engine.read_by_name(root, "size").to_number(), 20.0);
    assert!(
        ljos_quick::text_engine_in_use().is_none(),
        "waited for the engine before anything used it"
    );

    go.send(()).unwrap();
    // fonts-liberation, which CI installs.
    assert_eq!(family_of(&engine, root), "Liberation Mono");
    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    assert_ne!(family_of(&engine, root), "Liberation Mono");
}
