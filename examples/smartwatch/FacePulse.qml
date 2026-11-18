import Ljos.Ui
import Ljos.Core
import "logic/watch.ts" as W
import "logic/dial.ts" as Dial

// Pulse, the analog face: a minute track, hands that glow, a magenta
// second hand, and three complications inside the dial: the date, the
// heart rate and the move ring. On a rounded-square screen the dial sits
// in the middle and the day and the steps take the bands above and below.
WatchScreen {
    id: face

    readonly property real cx: face.width / 2
    readonly property real cy: face.height / 2
    readonly property real r: Math.min(face.width, face.height) / 2 - (face.round ? 8 : 14)
    readonly property Dial.Hands hands: Dial.hands(face.watch.hours, face.watch.minutes, face.watch.seconds)
    // Seconds since midnight, as degrees: never wraps back within a day,
    // so the hand's spring never turns it the long way round.
    readonly property real secondTurn: ((face.watch.hours * 60 + face.watch.minutes) * 60 + face.watch.seconds) * 6

    Accessible.role: Accessible.Clock
    Accessible.name: t`Pulse face`
    Accessible.description: W.hm(face.watch.hours, face.watch.minutes)

    // The minute track: every minute a tick, every fifth a longer one.
    Shape {
        anchors.fill: parent
        ShapePath {
            strokeColor: face.theme.line
            strokeWidth: 2
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathSvg {
                path: Dial.ticks(face.cx, face.cy, face.r - 4, 60, 5, 0, 8)
            }
        }
        ShapePath {
            strokeColor: face.theme.text
            strokeWidth: 4
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathSvg {
                path: Dial.ticks(face.cx, face.cy, face.r - 4, 12, 3, 22, 14)
            }
        }
    }

    // Twelve o'clock is the only numeral.
    Text {
        x: face.cx - width / 2
        y: face.cy - face.r + 34
        text: "12"
        color: face.theme.text
        font.size: 26
        font.weight: Font.DemiBold
        font.features: { "tnum": 1 }
    }

    // The date window, at three.
    Rectangle {
        id: date
        x: face.cx + face.r * 0.38
        y: face.cy - height / 2
        width: dateText.implicitWidth + 20
        height: 34
        radius: 8
        fill: face.theme.surface
        border.color: face.theme.line
        border.width: 1
        Text {
            id: dateText
            anchors.centerIn: parent
            text: `${face.watch.weekdayName(face.watch.weekday)} ${face.watch.day}`
            color: face.theme.text
            font.size: 16
            font.weight: Font.DemiBold
            font.letterSpacing: 1
        }
    }

    // The move ring at nine: the one ring of the face that glows.
    Item {
        x: face.cx - face.r * 0.38 - 70
        y: face.cy - 35
        width: 70
        height: 70
        Ring {
            anchors.fill: parent
            theme: face.theme
            value: face.watch.rings.move
            tint: face.theme.green
            thickness: 7
            lit: true
        }
        Text {
            anchors.centerIn: parent
            text: Math.round(face.watch.rings.move * 100) + "%"
            color: face.theme.text
            font.size: 15
            font.features: { "tnum": 1 }
        }
        Accessible.role: Accessible.Indicator
        Accessible.name: t`Move ring`
    }

    // The heart at six.
    Item {
        x: face.cx - width / 2
        y: face.cy + face.r * 0.36
        width: heart.width + 8 + bpm.implicitWidth
        height: 28
        Icon {
            id: heart
            width: 22
            height: 22
            anchors.verticalCenter: parent.verticalCenter
            glyph: "heart"
            color: face.theme.magenta
        }
        Text {
            id: bpm
            x: heart.width + 8
            anchors.verticalCenter: parent.verticalCenter
            text: `${face.watch.bpm}`
            color: face.theme.text
            font.size: 20
            font.weight: Font.DemiBold
            font.features: { "tnum": 1 }
        }
    }

    // A rounded-square screen has room above and below the dial.
    Text {
        visible: !face.round
        anchors.horizontalCenter: parent.horizontalCenter
        y: 18
        text: `${face.watch.weekdayName(face.watch.weekday)} ${face.watch.day} ${face.watch.monthName(face.watch.month)}`
        color: face.theme.muted
        font.size: 17
        font.letterSpacing: 2
    }
    Text {
        visible: !face.round
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 18
        text: t`${face.watch.steps} steps`
        color: face.theme.muted
        font.size: 17
    }

    // The hands glow; the hub covers where they meet.
    Neon {
        anchors.fill: parent
        theme: face.theme
        tint: face.theme.cyan
        spread: 0.5
        Item {
            x: face.cx
            y: face.cy
            rotation: face.hands.hour
            Rectangle {
                x: -5
                y: -face.r * 0.52
                width: 10
                height: face.r * 0.52 + 14
                radius: 5
                fill: face.theme.text
            }
            Rectangle {
                x: -2
                y: -face.r * 0.5
                width: 4
                height: face.r * 0.22
                radius: 2
                fill: face.theme.cyan
            }
        }
        Item {
            x: face.cx
            y: face.cy
            rotation: face.hands.minute
            Rectangle {
                x: -3.5
                y: -face.r * 0.82
                width: 7
                height: face.r * 0.82 + 16
                radius: 3.5
                fill: face.theme.cyan
            }
        }
    }
    Item {
        x: face.cx
        y: face.cy
        rotation: face.secondTurn
        Behavior on rotation {
            NumberAnimation {
                duration: 180
                easing.type: Easing.OutCubic
            }
        }
        Rectangle {
            x: -1.25
            y: -face.r * 0.9
            width: 2.5
            height: face.r * 0.9 + 30
            radius: 1.25
            fill: face.theme.magenta
        }
    }
    Rectangle {
        x: face.cx - 9
        y: face.cy - 9
        width: 18
        height: 18
        radius: 9
        fill: face.theme.canvas
        border.color: face.theme.magenta
        border.width: 3
    }
}
