import Ljos.Ui
import Ljos.Core
import Ljos.Layout

// How the cockpit looks and counts, and whether the drive runs.
Item {
    id: panel

    required property Theme theme
    required property Cockpit car

    component Setting: Item {
        id: setting
        required property Theme theme
        required property string name
        default property alias choices: options.data
        width: 700
        height: 64
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: 220
            text: setting.name
            color: setting.theme.text
            font.size: 19
            elide: Text.ElideRight
        }
        Row {
            id: options
            x: 230
            anchors.verticalCenter: parent.verticalCenter
            spacing: 10
        }
    }

    Column {
        anchors.left: parent.left
        anchors.leftMargin: 64
        anchors.top: parent.top
        anchors.topMargin: 28
        spacing: 6

        Setting {
            theme: panel.theme
            name: t`Look`
            Chip {
                theme: panel.theme
                label: t`Dark`
                active: panel.theme.look === "dark"
                onPressed: panel.theme.look = "dark"
            }
            Chip {
                theme: panel.theme
                label: t`Light`
                active: panel.theme.look === "light"
                onPressed: panel.theme.look = "light"
            }
            Chip {
                theme: panel.theme
                label: t`As the system`
                active: panel.theme.look === "auto"
                onPressed: panel.theme.look = "auto"
            }
        }
        Setting {
            theme: panel.theme
            name: t`Glow`
            Chip {
                theme: panel.theme
                label: t`On`
                active: panel.theme.bloom
                tint: panel.theme.magenta
                onPressed: panel.theme.bloom = true
            }
            Chip {
                theme: panel.theme
                label: t`Off`
                active: !panel.theme.bloom
                onPressed: panel.theme.bloom = false
            }
        }
        Setting {
            theme: panel.theme
            name: t`Distance`
            Chip {
                theme: panel.theme
                label: t`Kilometres`
                active: !panel.car.miles
                onPressed: panel.car.miles = false
            }
            Chip {
                theme: panel.theme
                label: t`Miles`
                active: panel.car.miles
                onPressed: panel.car.miles = true
            }
        }
        Setting {
            theme: panel.theme
            name: t`Temperature`
            Chip {
                theme: panel.theme
                label: t`°C`
                active: !panel.car.fahrenheit
                onPressed: panel.car.fahrenheit = false
            }
            Chip {
                theme: panel.theme
                label: t`°F`
                active: panel.car.fahrenheit
                onPressed: panel.car.fahrenheit = true
            }
        }
        Setting {
            theme: panel.theme
            name: t`Simulated drive`
            Chip {
                theme: panel.theme
                label: panel.car.simulate ? t`Running` : t`Stopped`
                active: panel.car.simulate
                tint: panel.theme.green
                onPressed: panel.car.simulate = !panel.car.simulate
            }
            Chip {
                theme: panel.theme
                glyph: "lamp"
                label: t`Hazard lights`
                active: panel.car.hazard
                tint: panel.theme.amber
                onPressed: panel.car.hazard = !panel.car.hazard
            }
        }
    }

    // What the demo is.
    Text {
        anchors.right: parent.right
        anchors.rightMargin: 64
        anchors.top: parent.top
        anchors.topMargin: 40
        width: 520
        text: t`Svala GT is a made-up car. Its model comes from a Blender script in this example; the map, the music and the streets are invented. Everything you see is drawn by Ljós.`
        color: panel.theme.muted
        font.size: 16
        wrap: Text.WordWrap
    }
}
