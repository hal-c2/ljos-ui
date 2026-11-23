// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
pragma Singleton
import QtQuick

// Gaps and padding in logical pixels, steps of the unit (4 px).
// A theme that sets the unit moves every step with it.
// A step stays from 0 to the largest int, whatever sets the unit.
QtObject {
    readonly property int unit: 4
    readonly property int none: 0
    readonly property int xxs: Math.max(0, unit)
    readonly property int xs: Math.max(0, Math.min(unit * 2, 2147483647))
    readonly property int sm: Math.max(0, Math.min(unit * 3, 2147483647))
    readonly property int md: Math.max(0, Math.min(unit * 4, 2147483647))
    readonly property int lg: Math.max(0, Math.min(unit * 6, 2147483647))
    readonly property int xl: Math.max(0, Math.min(unit * 8, 2147483647))
    readonly property int xxl: Math.max(0, Math.min(unit * 12, 2147483647))
    readonly property int xxxl: Math.max(0, Math.min(unit * 16, 2147483647))
}
