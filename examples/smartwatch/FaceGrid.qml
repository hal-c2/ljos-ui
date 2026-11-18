import Ljos.Ui
import Ljos.Layout
import Ljos.Core
import "logic/watch.ts" as W

// Grid, the digital face: the time large, the day above it, three
// activity rings nested round the edge, and the heart, the steps and the
// next event below. The outer ring, move, is the one that glows.
WatchScreen {
    id: face

    readonly property real s: Math.min(face.width, face.height)

    Accessible.role: Accessible.Clock
    Accessible.name: t`Grid face`
    Accessible.description: W.hm(face.watch.hours, face.watch.minutes)

    Item {
        id: rings
        anchors.centerIn: parent
        width: face.round ? face.s - 12 : face.s - 24
        height: rings.width
        Ring {
            anchors.fill: parent
            theme: face.theme
            value: face.watch.rings.move
            tint: face.theme.magenta
            thickness: 10
            lit: true
        }
        Ring {
            anchors.fill: parent
            anchors.margins: 14
            theme: face.theme
            value: face.watch.rings.exercise
            tint: face.theme.green
            thickness: 10
        }
        Ring {
            anchors.fill: parent
            anchors.margins: 28
            theme: face.theme
            value: face.watch.rings.stand
            tint: face.theme.cyan
            thickness: 10
        }
    }

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: time.top
        anchors.bottomMargin: 2
        text: `${face.watch.weekdayName(face.watch.weekday)} ${face.watch.day} ${face.watch.monthName(face.watch.month)}`
        color: face.theme.cyan
        font.size: 18
        font.weight: Font.DemiBold
        font.letterSpacing: 3
    }
    Text {
        id: time
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: -12
        text: W.hm(face.watch.hours, face.watch.minutes)
        color: face.theme.text
        font.size: 92
        font.weight: Font.Light
        font.features: { "tnum": 1 }
    }
    Text {
        anchors.left: time.right
        anchors.leftMargin: 4
        anchors.baseline: time.baseline
        text: W.pad(face.watch.seconds)
        color: face.theme.magenta
        font.size: 22
        font.features: { "tnum": 1 }
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: time.bottom
        anchors.topMargin: 6
        spacing: 22
        Row {
            spacing: 6
            Icon {
                width: 20
                height: 20
                glyph: "heart"
                color: face.theme.magenta
            }
            Text {
                text: `${face.watch.bpm}`
                color: face.theme.text
                font.size: 18
                font.features: { "tnum": 1 }
            }
        }
        Row {
            spacing: 6
            Icon {
                width: 20
                height: 20
                glyph: "run"
                color: face.theme.green
            }
            Text {
                text: `${face.watch.steps}`
                color: face.theme.text
                font.size: 18
                font.features: { "tnum": 1 }
            }
        }
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: face.round ? 86 : 20
        text: t`10:00 Design review`
        color: face.theme.muted
        font.size: 16
    }
}
