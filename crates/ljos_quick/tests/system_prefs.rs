//! What the system prefers, as the scene and QML see it: a change is one
//! frame, telling the same again is none, and motion and the size of text
//! follow.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use ljos_a11y::{ColorScheme, Contrast, SystemPrefs};
use ljos_quick::{set_system_prefs, system_prefs, update_system_prefs};
use ljos_runtime::Time;

const HINTS: &str = r#"
import QtQuick
Item {
    objectName: "root"
    width: 200; height: 100
    property int scheme: Qt.styleHints.colorScheme
    property int contrast: Qt.styleHints.accessibility.contrastPreference
    property int motion: Qt.styleHints.accessibility.motionPreference
    property int told: 0
    onSchemeChanged: told++
}
"#;

#[test]
fn a_system_that_was_told_nothing_prefers_nothing() {
    let e = engine();
    assert_eq!(system_prefs(&e), SystemPrefs::default());
    let root = load(&e, HINTS);
    assert_eq!(
        num(&e, root, "scheme"),
        f64::from(ColorScheme::Light.qt_value())
    );
    assert_eq!(num(&e, root, "contrast"), 0.0);
    assert_eq!(num(&e, root, "motion"), 0.0);
}

#[test]
fn a_change_is_one_frame_and_the_same_again_is_none() {
    let e = engine();
    let root = load(&e, HINTS);
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    assert!(!s.needs_frame());
    let woke = Rc::new(Cell::new(0));
    let w = woke.clone();
    s.set_waker(move || w.set(w.get() + 1));

    // The binding's first value may have been a change of its own.
    let told = num(&e, root, "told");
    update_system_prefs(&e, |p| {
        p.color_scheme = ColorScheme::Dark;
        p.contrast = Contrast::High;
        p.reduced_motion = true;
    });
    assert!(woke.get() > 0, "a change asks for a frame");
    s.frame(&e, Time::from_millis(16));
    assert_eq!(
        num(&e, root, "scheme"),
        f64::from(ColorScheme::Dark.qt_value())
    );
    assert_eq!(num(&e, root, "contrast"), 1.0);
    assert_eq!(num(&e, root, "motion"), 1.0);
    assert_eq!(num(&e, root, "told"), told + 1.0);
    assert!(!s.needs_frame());

    // Nothing polls: the same settings told again wake nobody and tell
    // QML nothing.
    let before = woke.get();
    set_system_prefs(&e, system_prefs(&e));
    assert_eq!(woke.get(), before);
    assert!(!s.needs_frame());
    assert_eq!(num(&e, root, "told"), told + 1.0);
}

#[test]
fn a_font_scale_that_makes_no_sense_is_the_size_as_written() {
    let e = engine();
    for scale in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        update_system_prefs(&e, |p| p.font_scale = scale);
        assert_eq!(system_prefs(&e).font_scale, 1.0, "{scale}");
    }
    update_system_prefs(&e, |p| p.font_scale = 1.5);
    assert_eq!(system_prefs(&e).font_scale, 1.5);
}

const SCALED: &str = r#"
import QtQuick
Item {
    width: 400; height: 200
    Text { objectName: "scaled"; text: "Hello there"; font.pixelSize: 20 }
    Text {
        objectName: "fixed"; y: 100; text: "Hello there"
        font.pixelSize: 20; font.scaleWithSystem: false
    }
}
"#;

#[test]
fn text_grows_with_the_system_unless_it_says_not_to() {
    let e = engine();
    let root = load(&e, SCALED);
    let mut s = scene(&e, root, 400.0, 200.0);
    frame(&mut s, &e);
    let (scaled, fixed) = (find(&e, root, "scaled"), find(&e, root, "fixed"));
    let width = num(&e, scaled, "implicitWidth");
    assert_eq!(num(&e, fixed, "implicitWidth"), width);

    update_system_prefs(&e, |p| p.font_scale = 2.0);
    s.frame(&e, Time::from_millis(16));
    let grown = num(&e, scaled, "implicitWidth");
    assert!(grown > width * 1.8, "{grown} is not about twice {width}");
    assert_eq!(num(&e, fixed, "implicitWidth"), width);
}

const SLIDE: &str = r#"
import QtQuick
Item {
    width: 400; height: 200
    Rectangle {
        objectName: "box"
        width: 20; height: 20
        NumberAnimation on x { objectName: "slide"; from: 0; to: 300; duration: 1000 }
    }
}
"#;

#[test]
fn with_reduced_motion_an_animation_is_at_its_end_at_once() {
    let e = engine();
    update_system_prefs(&e, |p| p.reduced_motion = true);
    let root = load(&e, SLIDE);
    let mut s = scene(&e, root, 400.0, 200.0);
    s.frame(&e, Time::ZERO);
    s.frame(&e, Time::from_millis(16));
    let b = find(&e, root, "box");
    assert_eq!(num(&e, b, "x"), 300.0);
    // And then there is nothing left to wait for.
    let out = s.frame(&e, Time::from_millis(32));
    assert_eq!(out.next_wake, None);
    assert!(!s.needs_frame());
}

#[test]
fn without_it_the_same_animation_takes_its_time() {
    let e = engine();
    let root = load(&e, SLIDE);
    let mut s = scene(&e, root, 400.0, 200.0);
    s.frame(&e, Time::ZERO);
    s.frame(&e, Time::from_millis(16));
    let x = num(&e, find(&e, root, "box"), "x");
    assert!(x < 300.0, "{x}");
}
