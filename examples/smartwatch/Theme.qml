import Ljos.Ui
import Ljos.Core

// The watch's colours: one dark look, since its screen is OLED and black
// is a pixel switched off. They are the car cockpit's, so the two read as
// one family. Asleep, the always-on face draws in `dim` alone.
Object {
    id: theme

    // Bloom on what is lit: the active ring, the hands and the one key
    // highlight of a screen. Text is never in what glows.
    property bool bloom: true

    readonly property bool dark: true
    readonly property color canvas: "#000000"
    readonly property color surface: "#0d1119"
    readonly property color raised: "#151b28"
    readonly property color sunken: "#090c13"
    readonly property color line: "#25314a"
    // The unlit part of a ring.
    readonly property color track: "#18202f"
    // A wash of the accent behind what is selected.
    readonly property color selected: "#0f3440"
    readonly property color text: "#e6f1ff"
    readonly property color muted: "#93a7c4"
    readonly property color cyan: "#19f0ff"
    readonly property color magenta: "#ff3ddb"
    readonly property color amber: "#ffb020"
    readonly property color red: "#ff4d6a"
    readonly property color green: "#3dffa8"
    readonly property color onAccent: "#041016"
    // The always-on face: grey enough to read at night, few pixels lit.
    readonly property color dim: "#8a9ab0"
    readonly property color dimLine: "#2a3342"

    readonly property real glow: 1
}
