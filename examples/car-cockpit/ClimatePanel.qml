import Ljos.Ui
import Ljos.Core
import Ljos.Layout
import "logic/climate.ts" as Climate

// The cabin's climate: a set point for each side, the fan, and the
// switches for cooling, defrost, the seats and keeping both sides alike.
Item {
    id: panel

    required property Theme theme
    required property Cockpit car

    // One side's set point, large, with a step down and a step up.
    component Side: Item {
        id: side
        required property Theme theme
        required property Cockpit car
        required property bool driver
        required property real celsius
        required property int seat
        width: 330
        height: 340

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 0
            text: side.driver ? t`Driver` : t`Passenger`
            color: side.theme.muted
            font.size: 16
            font.letterSpacing: 2
        }
        Chip {
            anchors.left: parent.left
            y: 92
            theme: side.theme
            glyph: "minus"
            help: side.driver ? t`Driver cooler` : t`Passenger cooler`
            onPressed: side.car.warmer(side.driver, -0.5)
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 66
            text: Climate.shown(side.celsius, side.car.fahrenheit)
            color: side.car.climateOn ? side.theme.text : side.theme.muted
            font.size: 78
            font.weight: Font.Light
            font.features: { "tnum": 1 }
            Accessible.role: Accessible.StaticText
            Accessible.name: side.driver ? t`Driver set to ${Climate.shown(side.celsius, side.car.fahrenheit)}`
                : t`Passenger set to ${Climate.shown(side.celsius, side.car.fahrenheit)}`
        }
        Chip {
            anchors.right: parent.right
            y: 92
            theme: side.theme
            glyph: "plus"
            help: side.driver ? t`Driver warmer` : t`Passenger warmer`
            onPressed: side.car.warmer(side.driver, 0.5)
        }
        // Where the set point sits between coldest and hottest.
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 186
            width: 240
            height: 6
            radius: 3
            fill: side.theme.track
            Neon {
                anchors.fill: parent
                theme: side.theme
                tint: Climate.warmth(side.celsius) > 0.5 ? side.theme.amber : side.theme.cyan
                spread: 0.5
                visible: side.car.climateOn
                Rectangle {
                    x: Climate.warmth(side.celsius) * (parent.width - 14)
                    y: -4
                    width: 14
                    height: 14
                    radius: 7
                    fill: Climate.warmth(side.celsius) > 0.5 ? side.theme.amber : side.theme.cyan
                    Behavior on x {
                        NumberAnimation {
                            duration: 200
                            easing.type: Easing.OutCubic
                        }
                    }
                }
            }
        }
        Chip {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 236
            theme: side.theme
            glyph: "seat"
            label: side.seat === 0 ? t`Seat off` : t`Seat ${side.seat}`
            active: side.seat > 0
            tint: side.theme.amber
            onPressed: {
                if (side.driver) {
                    side.car.seatLeft = (side.car.seatLeft + 1) % 4
                } else {
                    side.car.seatRight = (side.car.seatRight + 1) % 4
                }
            }
        }
    }

    Side {
        theme: panel.theme
        car: panel.car
        anchors.left: parent.left
        anchors.leftMargin: 80
        anchors.verticalCenter: parent.verticalCenter
        driver: true
        celsius: panel.car.driverTemp
        seat: panel.car.seatLeft
    }
    Side {
        theme: panel.theme
        car: panel.car
        anchors.right: parent.right
        anchors.rightMargin: 80
        anchors.verticalCenter: parent.verticalCenter
        driver: false
        celsius: panel.car.passengerTemp
        seat: panel.car.seatRight
    }

    // The fan, in the middle.
    Item {
        id: fan
        anchors.centerIn: parent
        width: 560
        height: 340

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: panel.car.climateOn ? t`Fan ${panel.car.fan}` : t`Climate off`
            color: panel.theme.muted
            font.size: 16
            font.letterSpacing: 2
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 40
            spacing: 18
            Chip {
                Layout.alignment: Align.VCenter
                theme: panel.theme
                glyph: "minus"
                help: t`Fan slower`
                onPressed: panel.car.fan = Math.max(1, panel.car.fan - 1)
            }
            Item {
                width: Climate.fanSteps * 30
                height: 110
                Accessible.role: Accessible.Indicator
                Accessible.name: t`Fan speed ${panel.car.fan} of ${Climate.fanSteps}`
                Repeater {
                    model: Climate.fanSteps
                    delegate: Item {
                        id: step
                        required property int index
                        readonly property bool lit: panel.car.climateOn && step.index < panel.car.fan
                        x: step.index * 30
                        width: 18
                        height: 110
                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: 18
                            height: 30 + step.index * 16
                            radius: 6
                            fill: panel.theme.track
                        }
                        Neon {
                            anchors.fill: parent
                            theme: panel.theme
                            tint: panel.car.cooling ? panel.theme.cyan : panel.theme.amber
                            spread: 0.5
                            opacity: step.lit ? 1 : 0
                            Behavior on opacity {
                                NumberAnimation {
                                    duration: 100
                                    easing.type: Easing.OutCubic
                                }
                            }
                            Rectangle {
                                anchors.bottom: parent.bottom
                                width: 18
                                height: 30 + step.index * 16
                                radius: 6
                                fill: panel.car.cooling ? panel.theme.cyan : panel.theme.amber
                            }
                        }
                    }
                }
            }
            Chip {
                Layout.alignment: Align.VCenter
                theme: panel.theme
                glyph: "plus"
                help: t`Fan faster`
                onPressed: panel.car.fan = Math.min(Climate.fanSteps, panel.car.fan + 1)
            }
        }

        Grid {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 186
            columns: 2
            columnSpacing: 12
            rowSpacing: 12
            Chip {
                width: 200
                theme: panel.theme
                glyph: "power"
                label: panel.car.climateOn ? t`On` : t`Off`
                active: panel.car.climateOn
                onPressed: panel.car.climateOn = !panel.car.climateOn
            }
            Chip {
                width: 200
                theme: panel.theme
                glyph: panel.car.cooling ? "snow" : "flame"
                label: panel.car.cooling ? t`Cooling` : t`Heating`
                active: panel.car.climateOn
                tint: panel.car.cooling ? panel.theme.cyan : panel.theme.amber
                onPressed: panel.car.cooling = !panel.car.cooling
            }
            Chip {
                width: 200
                theme: panel.theme
                glyph: "defrost"
                label: t`Defrost`
                active: panel.car.defrost
                tint: panel.theme.amber
                onPressed: panel.car.defrost = !panel.car.defrost
            }
            Chip {
                width: 200
                theme: panel.theme
                glyph: "link"
                label: panel.car.linked ? t`Linked` : t`Each side`
                active: panel.car.linked
                onPressed: {
                    panel.car.linked = !panel.car.linked
                    if (panel.car.linked) {
                        panel.car.passengerTemp = panel.car.driverTemp
                    }
                }
            }
        }
    }
}
