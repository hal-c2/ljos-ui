// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
pragma Singleton
import QtQuick

// Text sizes a ratio of 1.2 apart from a body of 14 px: font.pixelSize,
// lineHeight (with lineHeightMode: Text.FixedHeight) and font.weight.
QtObject {
    readonly property int captionSize: 12
    readonly property int captionLineHeight: 16
    readonly property int captionWeight: 400
    readonly property int bodySize: 14
    readonly property int bodyLineHeight: 20
    readonly property int bodyWeight: 400
    readonly property int bodyStrongSize: 14
    readonly property int bodyStrongLineHeight: 20
    readonly property int bodyStrongWeight: 600
    readonly property int subtitleSize: 17
    readonly property int subtitleLineHeight: 24
    readonly property int subtitleWeight: 500
    readonly property int titleSize: 20
    readonly property int titleLineHeight: 24
    readonly property int titleWeight: 600
    readonly property int headlineSize: 24
    readonly property int headlineLineHeight: 32
    readonly property int headlineWeight: 600
    readonly property int displaySize: 35
    readonly property int displayLineHeight: 44
    readonly property int displayWeight: 700
}
