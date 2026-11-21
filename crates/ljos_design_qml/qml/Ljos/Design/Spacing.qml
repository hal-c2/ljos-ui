// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
pragma Singleton
import QtQuick

// Gaps and padding in logical pixels, steps of the unit (4 px).
// A theme that sets the unit moves every step with it.
QtObject {
    readonly property int unit: 4
    readonly property int none: 0
    readonly property int xxs: unit
    readonly property int xs: unit * 2
    readonly property int sm: unit * 3
    readonly property int md: unit * 4
    readonly property int lg: unit * 6
    readonly property int xl: unit * 8
    readonly property int xxl: unit * 12
    readonly property int xxxl: unit * 16
}
