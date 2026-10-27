//! What the system prefers reaches QML through `Qt.styleHints`: the
//! host reports the platform's settings, and what the application set
//! itself stays over them.

mod common;

use ljos::system::{
    Contrast, DesktopSettings, HostPrefs, SettingValue, report_system_prefs, system_prefs,
    update_system_prefs,
};

const MAIN: &str = r#"
import QtQuick
Item {
    property bool calm: Qt.styleHints.accessibility.motionPreference
                        === Qt.MotionPreference.ReducedMotion
    property int contrast: Qt.styleHints.accessibility.contrastPreference
    property real scale: Qt.styleHints.accessibility.fontScale
}
"#;

#[test]
fn a_desktops_settings_reach_qml() {
    let engine = ljos::engine();
    let root = common::create_main(&engine, MAIN);
    let mut desktop = DesktopSettings::default();
    assert!(desktop.set(
        "org.gnome.desktop.interface",
        "enable-animations",
        &SettingValue::Bool(false),
    ));
    assert!(desktop.set(
        "org.freedesktop.appearance",
        "contrast",
        &SettingValue::Uint(1),
    ));
    assert!(!desktop.set("org.example", "contrast", &SettingValue::Uint(1)));
    report_system_prefs(&engine, desktop.prefs());

    assert!(engine.read_by_name(root, "calm").to_boolean());
    assert_eq!(engine.read_by_name(root, "contrast").to_number(), 1.0);
    assert_eq!(system_prefs(&engine).contrast, Contrast::High);
}

#[test]
fn what_the_application_set_stays_over_the_hosts_reports() {
    let engine = ljos::engine();
    let root = common::create_main(&engine, MAIN);
    update_system_prefs(&engine, |p| p.reduced_motion = true);

    report_system_prefs(
        &engine,
        HostPrefs {
            reduced_motion: Some(false),
            font_scale: Some(1.5),
            ..HostPrefs::default()
        },
    );
    assert!(engine.read_by_name(root, "calm").to_boolean());
    assert_eq!(engine.read_by_name(root, "scale").to_number(), 1.5);

    // Set back to what the host said, it follows the host again.
    update_system_prefs(&engine, |p| p.reduced_motion = false);
    report_system_prefs(
        &engine,
        HostPrefs {
            reduced_motion: Some(true),
            ..HostPrefs::default()
        },
    );
    assert!(engine.read_by_name(root, "calm").to_boolean());
}
