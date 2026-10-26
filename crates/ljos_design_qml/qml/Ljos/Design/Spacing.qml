// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
pragma Singleton
import QtQuick

// Gaps and padding in logical pixels, steps of 4 px.
QtObject {
    readonly property int unit: 4
    readonly property int none: 0
    readonly property int xxs: 4
    readonly property int xs: 8
    readonly property int sm: 12
    readonly property int md: 16
    readonly property int lg: 24
    readonly property int xl: 32
    readonly property int xxl: 48
    readonly property int xxxl: 64
}
