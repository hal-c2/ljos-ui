import Ljos.Ui
import Ljos.Core
import Ljos.Layout
import "logic/car.ts" as Car
import "logic/climate.ts" as Climate
import "logic/drive.ts" as Drive

// The row along the bottom of the touchscreen: the gear selector, the
// driver's temperature, the pages, and the volume.
Item {
    id: dock

    required property Theme theme
    required property Cockpit car

    height: 92

    Rectangle {
        anchors.fill: parent
        fill: dock.theme.surface
        Rectangle {
            width: parent.width
            height: 1
            fill: dock.theme.line
        }
    }

    // The gear selector. In the demo the drive follows it.
    Row {
        id: gears
        x: 24
        anchors.verticalCenter: parent.verticalCenter
        spacing: 6
        Repeater {
            model: Drive.gears
            delegate: Chip {
                id: gearChip
                required property Drive.Gear modelData
                width: 56
                theme: dock.theme
                label: gearChip.modelData
                help: t`Shift to ${gearChip.modelData}`
                active: dock.car.gear === gearChip.modelData
                onPressed: dock.car.shift(gearChip.modelData)
            }
        }
    }

    Row {
        id: temp
        anchors.left: gears.right
        anchors.leftMargin: 36
        anchors.verticalCenter: parent.verticalCenter
        spacing: 6
        Chip {
            theme: dock.theme
            glyph: "left"
            help: t`Driver cooler`
            onPressed: dock.car.warmer(true, -0.5)
        }
        Text {
            width: 84
            Layout.alignment: Align.VCenter
            align: Text.AlignHCenter
            elide: Text.ElideRight
            text: Climate.shown(dock.car.driverTemp, dock.car.fahrenheit)
            color: dock.car.climateOn ? dock.theme.text : dock.theme.muted
            font.size: 26
            font.features: { "tnum": 1 }
        }
        Chip {
            theme: dock.theme
            glyph: "right"
            help: t`Driver warmer`
            onPressed: dock.car.warmer(true, 0.5)
        }
    }

    Row {
        anchors.centerIn: parent
        anchors.horizontalCenterOffset: 120
        spacing: 10
        Repeater {
            model: Car.apps
            delegate: Chip {
                id: app
                required property Car.App modelData
                width: 64
                theme: dock.theme
                glyph: app.modelData.glyph
                help: dock.car.pageTitle(app.modelData.page)
                active: dock.car.page === app.modelData.page
                onPressed: dock.car.page = app.modelData.page
            }
        }
    }

    Row {
        anchors.right: parent.right
        anchors.rightMargin: 24
        anchors.verticalCenter: parent.verticalCenter
        spacing: 6
        Chip {
            theme: dock.theme
            glyph: "minus"
            help: t`Quieter`
            onPressed: dock.car.volume = Math.max(0, dock.car.volume - 1)
        }
        Item {
            width: 72
            height: 56
            Accessible.role: Accessible.Indicator
            Accessible.name: t`Volume ${dock.car.volume} of 10`
            Icon {
                anchors.horizontalCenter: parent.horizontalCenter
                y: 4
                width: 24
                height: 24
                glyph: "volume"
                color: dock.car.volume === 0 ? dock.theme.muted : dock.theme.text
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                y: 32
                text: `${dock.car.volume}`
                color: dock.theme.muted
                font.size: 15
                font.features: { "tnum": 1 }
            }
        }
        Chip {
            theme: dock.theme
            glyph: "plus"
            help: t`Louder`
            onPressed: dock.car.volume = Math.min(10, dock.car.volume + 1)
        }
    }
}
