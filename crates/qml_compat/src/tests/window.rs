//! Tests of windows: `Window`, `ApplicationWindow`, screens.

use super::*;

/// What says in Ljós's API what it says in Qt Quick keeps nothing, and
/// `QtQuick.Window`, whose types are QtQuick's, leaves its line empty. A
/// window's `color` is not a `Rectangle`'s, whatever Ljós builds it from.
#[test]
fn the_items_ljos_ui_has_as_they_are_are_lowered_unchanged() {
    let source = "import QtQuick\nimport QtQuick.Window\nWindow {\n    color: \"#202020\"\n    \
                  onColorChanged: {}\n    Flickable { Image { } TextInput { Keys.onReturnPressed: {} } }\n    \
                  TextEdit { Accessible.name: \"Notes\" }\n    TextMetrics { }\n    \
                  FontMetrics { }\n    DropArea { }\n    Shortcut { sequence: \"Ctrl+Q\" }\n    \
                  property real wide: Screen.width\n}\n";
    let text = lowered(source);
    assert_eq!(text.lines().count(), source.lines().count());
    assert!(
        text.starts_with("pragma Scripts: JavaScript; import Ljos.Ui\n\nWindow {\n"),
        "{text}"
    );
    assert_eq!(
        text.lines().skip(2).collect::<Vec<_>>(),
        source.lines().skip(2).collect::<Vec<_>>()
    );
}

/// A focus scope as a file's root, a shortcut on the platform's standard
/// keys and the application's font are Ljos.Ui's as they are Qt Quick's.
#[test]
fn focus_scopes_standard_keys_and_the_application_are_lowered_unchanged() {
    let source = "import QtQuick\nFocusScope {\n    property string face: Application.font.family\n    \
                  property bool shown: Application.active\n    \
                  Shortcut { sequences: [StandardKey.Find] }\n}\n";
    assert_eq!(
        lowered(source),
        format!(
            "pragma Scripts: JavaScript; import Ljos.Ui\n{}",
            source.split_once('\n').expect("an import").1
        )
    );
}

#[test]
fn an_aliased_window_import_keeps_the_file() {
    assert!(
        kept_for("import QtQuick\nimport QtQuick.Window as W\nW.Window { }\n")
            .contains("`QtQuick.Window`")
    );
}
