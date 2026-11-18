import Ljos.Ui
import Ljos.Core

// A ring that fills clockwise from twelve: a track, and the share of it
// done. Only the ring that is `lit` glows.
Item {
    id: ring

    required property Theme theme
    property real value: 0
    property color tint: ring.theme.cyan
    property real thickness: 12
    property bool lit: false

    readonly property real r: Math.min(ring.width, ring.height) / 2 - ring.thickness / 2

    // A new value turns the ring, never jumps it.
    property real shown: ring.value
    Behavior on shown {
        NumberAnimation {
            duration: 450
            easing.type: Easing.OutCubic
        }
    }

    Shape {
        anchors.fill: parent
        ShapePath {
            strokeColor: ring.theme.track
            strokeWidth: ring.thickness
            fillColor: "transparent"
            PathAngleArc {
                centerX: ring.width / 2
                centerY: ring.height / 2
                radiusX: ring.r
                radiusY: ring.r
                startAngle: 0
                sweepAngle: 360
            }
        }
    }

    Neon {
        anchors.fill: parent
        theme: ring.theme
        tint: ring.tint
        spread: 0.7
        opacity: 1
        visible: ring.lit
        Shape {
            anchors.fill: parent
            ShapePath {
                strokeColor: ring.tint
                strokeWidth: ring.thickness
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathAngleArc {
                    centerX: ring.width / 2
                    centerY: ring.height / 2
                    radiusX: ring.r
                    radiusY: ring.r
                    startAngle: -90
                    sweepAngle: Math.max(0.5, 360 * Math.min(1, ring.shown))
                }
            }
        }
    }
    Shape {
        anchors.fill: parent
        visible: !ring.lit
        ShapePath {
            strokeColor: ring.tint
            strokeWidth: ring.thickness
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: ring.width / 2
                centerY: ring.height / 2
                radiusX: ring.r
                radiusY: ring.r
                startAngle: -90
                sweepAngle: Math.max(0.5, 360 * Math.min(1, ring.shown))
            }
        }
    }
}
