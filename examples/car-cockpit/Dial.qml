import Ljos.Ui
import Ljos.Core
import "logic/dial.ts" as Geo

// A round gauge: a track, a lit arc to the value, ticks, a needle and the
// value written large in the middle. The arc and the needle glow; the
// numbers do not.
Item {
    id: dial

    required property Theme theme
    property real value: 0
    property real maximum: 100
    property color tint: dial.theme.cyan
    property string readout: ""
    property string unit: ""
    property string caption: ""
    // A second, thinner arc inside: -1 to 1, drawn from the top, for what
    // flows both ways (power drawn, power won back).
    property bool flow: false
    property real flowValue: 0

    readonly property real start: 135
    readonly property real sweep: 270
    readonly property real cx: dial.width / 2
    readonly property real cy: dial.height / 2
    readonly property real radius: Math.min(dial.width, dial.height) / 2 - 14

    // What the arc and needle show: they follow the value as a well damped
    // spring, so a new value mid-flight turns them, never jumps them.
    property real shown: dial.value
    Behavior on shown {
        SpringAnimation {
            spring: 3
            damping: 0.9
            epsilon: 0.05
        }
    }
    readonly property real f: Geo.fraction(dial.shown, dial.maximum)

    Accessible.role: Accessible.Indicator
    Accessible.name: dial.caption
    Accessible.description: dial.readout + " " + dial.unit

    // The track and the ticks are not lit.
    Shape {
        anchors.fill: parent
        ShapePath {
            strokeColor: dial.theme.track
            strokeWidth: 10
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: dial.cx
                centerY: dial.cy
                radiusX: dial.radius
                radiusY: dial.radius
                startAngle: dial.start
                sweepAngle: dial.sweep
            }
        }
        ShapePath {
            strokeColor: dial.theme.muted
            strokeWidth: 1.5
            fillColor: "transparent"
            PathSvg {
                path: Geo.ticks(dial.cx, dial.cy, dial.radius - 14, dial.start, dial.sweep, 20, 4)
            }
        }
    }

    Neon {
        anchors.fill: parent
        theme: dial.theme
        tint: dial.tint
        Shape {
            anchors.fill: parent
            ShapePath {
                strokeColor: dial.tint
                strokeWidth: 10
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathAngleArc {
                    centerX: dial.cx
                    centerY: dial.cy
                    radiusX: dial.radius
                    radiusY: dial.radius
                    startAngle: dial.start
                    sweepAngle: Math.max(0.5, dial.sweep * dial.f)
                }
            }
            ShapePath {
                strokeColor: dial.flowValue < 0 ? dial.theme.green : dial.theme.magenta
                strokeWidth: dial.flow ? 4 : 0
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathAngleArc {
                    centerX: dial.cx
                    centerY: dial.cy
                    radiusX: dial.radius - 34
                    radiusY: dial.radius - 34
                    startAngle: 270
                    sweepAngle: 120 * Math.max(-1, Math.min(1, dial.flowValue))
                }
            }
        }
        // The needle: from inside the ticks to the arc.
        Item {
            x: dial.cx
            y: dial.cy
            rotation: dial.start + dial.sweep * dial.f + 90
            Rectangle {
                x: -2
                y: -dial.radius + 2
                width: 4
                height: dial.radius * 0.42
                radius: 2
                fill: dial.theme.dark ? dial.theme.text : dial.tint
            }
        }
    }

    Item {
        anchors.centerIn: parent
        width: big.implicitWidth
        height: big.implicitHeight + small.implicitHeight
        Text {
            id: big
            anchors.horizontalCenter: parent.horizontalCenter
            text: dial.readout
            color: dial.theme.text
            font.size: Math.round(dial.radius * 0.5)
            font.weight: Font.DemiBold
            font.features: { "tnum": 1 }
        }
        Text {
            id: small
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: big.bottom
            text: dial.unit
            color: dial.theme.muted
            font.size: 15
            font.letterSpacing: 2
        }
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 6
        text: dial.caption
        color: dial.theme.muted
        font.size: 14
        font.letterSpacing: 1.5
    }
}
