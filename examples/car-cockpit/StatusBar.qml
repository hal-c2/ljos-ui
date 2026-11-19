import Ljos.Ui
import Ljos.Core
import Ljos.Layout
import "logic/drive.ts" as Drive

// The strip across the top of the touchscreen: gear, lock, the time,
// charge and range, and what plays.
Item {
    id: bar

    required property Theme theme
    required property Cockpit car
    // The time to show, as the app's clock gives it.
    required property string clock

    height: 52

    Text {
        id: gear
        x: 28
        anchors.verticalCenter: parent.verticalCenter
        text: bar.car.gear
        color: bar.theme.cyan
        font.size: 22
        font.weight: Font.Bold
        Accessible.role: Accessible.StaticText
        Accessible.name: t`Gear ${bar.car.gear}`
    }
    Icon {
        id: lock
        anchors.left: gear.right
        anchors.leftMargin: 22
        anchors.verticalCenter: parent.verticalCenter
        width: 22
        height: 22
        glyph: bar.car.locked ? "lock" : "unlock"
        color: bar.car.locked ? bar.theme.magenta : bar.theme.muted
        Accessible.role: Accessible.StaticText
        Accessible.name: bar.car.locked ? t`Locked` : t`Unlocked`
    }
    Text {
        anchors.left: lock.right
        anchors.leftMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        width: 360
        text: bar.car.playing ? t`♪ ${bar.car.nowPlaying.title} · ${bar.car.nowPlaying.artist}` : ""
        color: bar.theme.muted
        font.size: 15
        elide: Text.ElideRight
    }

    Text {
        anchors.centerIn: parent
        text: bar.clock
        color: bar.theme.text
        font.size: 20
        font.weight: Font.DemiBold
        font.features: { "tnum": 1 }
    }

    Row {
        anchors.right: parent.right
        anchors.rightMargin: 28
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10
        Text {
            Layout.alignment: Align.VCenter
            text: t`${Drive.toUnit(bar.car.range, bar.car.miles)} ${bar.car.miles ? "mi" : "km"}`
            color: bar.theme.text
            font.size: 17
            font.features: { "tnum": 1 }
        }
        // The battery: a frame and a fill.
        Item {
            Layout.alignment: Align.VCenter
            width: 40
            height: 18
            Accessible.role: Accessible.Indicator
            Accessible.name: t`Charge ${Math.round(bar.car.charge * 100)} %`
            Rectangle {
                width: 36
                height: 18
                radius: 4
                fill: "transparent"
                border.color: bar.theme.muted
                border.width: 1.5
            }
            Rectangle {
                x: 37
                y: 6
                width: 3
                height: 6
                radius: 1
                fill: bar.theme.muted
            }
            Rectangle {
                x: 3
                y: 3
                width: 30 * bar.car.charge
                height: 12
                radius: 2
                fill: bar.car.charge < 0.2 ? bar.theme.amber : bar.theme.green
            }
        }
    }
}
