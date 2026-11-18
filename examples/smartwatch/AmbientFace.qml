import Ljos.Ui
import Ljos.Core
import "logic/dial.ts" as Dial
import "logic/watch.ts" as W

// The always-on face: the time in thin grey strokes and the date, few
// pixels lit and no second hand, so the screen can sleep between minutes.
// It moves a few pixels each minute, so no pixel stays lit for hours.
WatchScreen {
    id: face

    readonly property real cx: face.width / 2 + face.drift.x
    readonly property real cy: face.height / 2 + face.drift.y
    readonly property real r: Math.min(face.width, face.height) / 2 - 40
    readonly property Dial.Spot drift: Dial.around(face.watch.minutes % 8, 8, 4)
    readonly property Dial.Hands hands: Dial.hands(face.watch.hours, face.watch.minutes, 0)

    Accessible.role: Accessible.Clock
    Accessible.name: t`Always-on face`
    Accessible.description: W.hm(face.watch.hours, face.watch.minutes)

    // Pulse sleeps as hands, Grid as digits.
    Item {
        anchors.fill: parent
        visible: face.watch.face === 0
        Shape {
            anchors.fill: parent
            ShapePath {
                strokeColor: face.theme.dimLine
                strokeWidth: 3
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathSvg {
                    path: Dial.ticks(face.cx, face.cy, face.r, 12, 3, 14, 8)
                }
            }
        }
        Item {
            x: face.cx
            y: face.cy
            rotation: face.hands.hour
            Rectangle {
                x: -3
                y: -face.r * 0.55
                width: 6
                height: face.r * 0.55
                radius: 3
                fill: "transparent"
                border.color: face.theme.dim
                border.width: 1.5
            }
        }
        Item {
            x: face.cx
            y: face.cy
            rotation: face.hands.minute
            Rectangle {
                x: -2.5
                y: -face.r * 0.86
                width: 5
                height: face.r * 0.86
                radius: 2.5
                fill: "transparent"
                border.color: face.theme.dim
                border.width: 1.5
            }
        }
        Text {
            x: face.cx - width / 2
            y: face.cy + face.r * 0.34
            text: `${face.watch.weekdayName(face.watch.weekday)} ${face.watch.day}`
            color: face.theme.dim
            font.size: 16
            font.letterSpacing: 2
        }
    }
    Item {
        anchors.fill: parent
        visible: face.watch.face === 1
        Text {
            id: time
            x: face.cx - width / 2
            y: face.cy - height / 2 - 8
            text: W.hm(face.watch.hours, face.watch.minutes)
            color: face.theme.dim
            font.size: 84
            font.weight: Font.Thin
            font.features: { "tnum": 1 }
        }
        Text {
            x: face.cx - width / 2
            anchors.top: time.bottom
            text: `${face.watch.weekdayName(face.watch.weekday)} ${face.watch.day} ${face.watch.monthName(face.watch.month)}`
            color: face.theme.dim
            font.size: 16
            font.letterSpacing: 3
        }
    }
}
