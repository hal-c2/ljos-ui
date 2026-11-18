import Ljos.Ui
import Ljos.Core
import Ljos.Layout
import "logic/drive.ts" as Drive

// The instrument cluster behind the wheel: speed and power on the left,
// what the assist sees in the middle with the gear under it, charge and
// range on the right, and the telltales across the top.
Rectangle {
    id: cluster

    required property Theme theme
    required property Cockpit car

    fill: cluster.theme.surface
    radius: 28
    border.color: cluster.theme.line
    border.width: 1

    // The indicators flash at 1.25 Hz, well under three flashes a second.
    property bool blink: false
    readonly property bool signalling: cluster.car.hazard || cluster.car.signal !== "none"
    Timer {
        interval: 400
        repeat: true
        running: cluster.signalling
        onTriggered: cluster.blink = !cluster.blink
        onRunningChanged: cluster.blink = cluster.signalling
    }

    // ----- Telltales ---------------------------------------------------------

    Item {
        id: telltales
        anchors.top: parent.top
        anchors.topMargin: 14
        anchors.horizontalCenter: parent.horizontalCenter
        width: 760
        height: 34

        Neon {
            id: leftSignal
            anchors.left: parent.left
            width: 34
            height: 34
            theme: cluster.theme
            tint: cluster.theme.green
            lit: cluster.blink && (cluster.car.hazard || cluster.car.signal === "left")
            opacity: leftSignal.lit ? 1 : 0.16
            Icon {
                anchors.fill: parent
                glyph: "left"
                color: cluster.theme.green
                weight: 2.6
            }
        }
        Neon {
            id: rightSignal
            anchors.right: parent.right
            width: 34
            height: 34
            theme: cluster.theme
            tint: cluster.theme.green
            lit: cluster.blink && (cluster.car.hazard || cluster.car.signal === "right")
            opacity: rightSignal.lit ? 1 : 0.16
            Icon {
                anchors.fill: parent
                glyph: "right"
                color: cluster.theme.green
                weight: 2.6
            }
        }

        Item {
            anchors.centerIn: parent
            width: 5 * 34 + 4 * 22
            height: 26
            Icon {
                x: 0
                width: 26
                height: 26
                glyph: "lamp"
                color: cluster.theme.green
                Accessible.role: Accessible.StaticText
                Accessible.name: t`Low beams on`
            }
            Icon {
                x: 56
                width: 26
                height: 26
                glyph: "door"
                color: cluster.car.openCount > 0 ? cluster.theme.amber : cluster.theme.track
                Accessible.role: Accessible.StaticText
                Accessible.name: cluster.car.openCount > 0 ? t`A door is open` : t`All doors closed`
            }
            Icon {
                x: 112
                width: 26
                height: 26
                glyph: "park"
                color: cluster.car.gear === "P" ? cluster.theme.red : cluster.theme.track
                Accessible.role: Accessible.StaticText
                Accessible.name: cluster.car.gear === "P" ? t`Parking brake set` : t`Parking brake released`
            }
            Icon {
                x: 168
                width: 26
                height: 26
                glyph: "battery"
                color: cluster.car.charge < 0.2 ? cluster.theme.amber : cluster.theme.track
                Accessible.role: Accessible.StaticText
                Accessible.name: cluster.car.charge < 0.2 ? t`Charge low` : t`Charge fine`
            }
            Icon {
                x: 224
                width: 26
                height: 26
                glyph: "assist"
                color: cluster.car.gear === "D" ? cluster.theme.cyan : cluster.theme.track
                Accessible.role: Accessible.StaticText
                Accessible.name: cluster.car.gear === "D" ? t`Lane assist on` : t`Lane assist standing by`
            }
        }
    }

    // ----- Dials -------------------------------------------------------------

    Dial {
        id: speedDial
        x: 46
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: 10
        width: 270
        height: 270
        theme: cluster.theme
        tint: cluster.theme.cyan
        value: Drive.toUnit(cluster.car.speed, cluster.car.miles)
        maximum: cluster.car.miles ? 130 : 200
        readout: `${Drive.toUnit(cluster.car.speed, cluster.car.miles)}`
        unit: cluster.car.miles ? t`MPH` : t`KM/H`
        caption: t`Speed`
        flow: true
        flowValue: cluster.car.power / 150
    }
    Text {
        anchors.left: speedDial.right
        anchors.leftMargin: 6
        anchors.bottom: speedDial.bottom
        anchors.bottomMargin: 30
        text: t`${Math.round(cluster.car.power)} kW`
        color: cluster.car.power < 0 ? cluster.theme.green : cluster.theme.muted
        font.size: 16
        font.features: { "tnum": 1 }
    }

    Dial {
        id: chargeDial
        anchors.right: parent.right
        anchors.rightMargin: 46
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: 10
        width: 270
        height: 270
        theme: cluster.theme
        tint: cluster.car.charge < 0.2 ? cluster.theme.amber : cluster.theme.green
        value: cluster.car.charge * 100
        maximum: 100
        readout: `${Drive.toUnit(cluster.car.range, cluster.car.miles)}`
        unit: cluster.car.miles ? t`MI LEFT` : t`KM LEFT`
        caption: t`Charge ${Math.round(cluster.car.charge * 100)} %`
    }
    Text {
        anchors.right: chargeDial.left
        anchors.rightMargin: 6
        anchors.bottom: chargeDial.bottom
        anchors.bottomMargin: 30
        text: t`${Math.floor(Drive.toUnit(cluster.car.odometer, cluster.car.miles))} ${cluster.car.miles ? "mi" : "km"}`
        color: cluster.theme.muted
        font.size: 16
        font.features: { "tnum": 1 }
    }

    // ----- The road and the gear ----------------------------------------------

    AssistView {
        id: assist
        anchors.horizontalCenter: parent.horizontalCenter
        y: 58
        width: 720
        height: parent.height - 58 - 64
        theme: cluster.theme
        car: cluster.car
        clip: true
    }

    Row {
        id: gears
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 14
        spacing: 26
        Accessible.role: Accessible.StaticText
        Accessible.name: t`Gear ${cluster.car.gear}`
        Repeater {
            model: Drive.gears
            delegate: Item {
                id: slot
                required property Drive.Gear modelData
                readonly property bool engaged: slot.modelData === cluster.car.gear
                width: 34
                height: 40
                Neon {
                    anchors.fill: parent
                    theme: cluster.theme
                    tint: cluster.theme.cyan
                    visible: slot.engaged
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        anchors.bottom: parent.bottom
                        width: 22
                        height: 3
                        radius: 1.5
                        fill: cluster.theme.cyan
                    }
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 2
                    text: slot.modelData
                    color: slot.engaged ? cluster.theme.text : cluster.theme.muted
                    font.size: slot.engaged ? 26 : 20
                    font.weight: slot.engaged ? Font.Bold : Font.Normal
                }
            }
        }
    }
}
