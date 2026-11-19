import Ljos.Ui
import Ljos.Core
import "logic/car.ts" as Car

// The cockpit's colours. Dark is the default: a night cabin, neon on near
// black. Light is for daylight, with the same hues deepened until text on
// them reads, and the glow toned down, as neon on white only smears.
Object {
    id: theme

    property Car.Look look: "dark"
    // Bloom on the neon: gauge arcs, needles, the active control and the
    // car's light strips. Text is never in what glows.
    property bool bloom: true

    readonly property bool dark: theme.look === "dark"
        || (theme.look === "auto" && App.hints.colorScheme === ColorScheme.Dark)

    readonly property color canvas: dark ? "#07080d" : "#e9edf3"
    readonly property color surface: dark ? "#0d1119" : "#f7f9fc"
    readonly property color raised: dark ? "#151b28" : "#ffffff"
    readonly property color sunken: dark ? "#090c13" : "#dde3ec"
    readonly property color line: dark ? "#25314a" : "#c3ccd9"
    // The unlit part of a gauge.
    readonly property color track: dark ? "#18202f" : "#d4dbe6"
    // A wash of the accent behind what is selected.
    readonly property color selected: dark ? "#0f3440" : "#d2eef1"
    readonly property color text: dark ? "#e6f1ff" : "#0d1422"
    readonly property color muted: dark ? "#93a7c4" : "#46566c"
    readonly property color cyan: dark ? "#19f0ff" : "#00707d"
    readonly property color magenta: dark ? "#ff3ddb" : "#a3127f"
    readonly property color amber: dark ? "#ffb020" : "#8a5200"
    readonly property color red: dark ? "#ff4d6a" : "#b8143a"
    readonly property color green: dark ? "#3dffa8" : "#0a7046"
    readonly property color onAccent: dark ? "#041016" : "#ffffff"
    // A road sign is white with dark figures by night and by day.
    readonly property color signFace: dark ? "#eef1f6" : "#ffffff"
    readonly property color signInk: "#0d1422"
    // How strong the glow is: neon on white needs less to read as lit.
    // The map's parks and water.
    readonly property color park: dark ? "#0c1d1a" : "#cfe6d8"
    // The plinth the 3D car stands on: lit by the scene, so darker than
    // the panel around it in the light look.
    readonly property color stage: dark ? "#151b28" : "#8e9aac"
    readonly property color water: dark ? "#0d2238" : "#bcd3ea"

    readonly property real glow: dark ? 1 : 0.55
}
