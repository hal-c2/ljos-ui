import Ljos.Ui
import Ljos.Core

// Svala GT's cockpit: the instrument cluster above, the touchscreen
// below. Drawn at 1600 by 900 and scaled to the window, text and all.
//
// Nothing runs while nothing changes. The drive steps once a second only
// while the car is in gear or still rolling; the player counts only while
// it plays; the clock wakes once a minute.
Rectangle {
    id: root

    width: 1600
    height: 900
    fill: theme.canvas

    // Named so a test (or an agent) can find them.
    Theme {
        id: theme
        objectName: "theme"
    }
    Cockpit {
        id: car
        objectName: "car"
    }

    Timer {
        objectName: "drive"
        interval: 1000
        repeat: true
        running: car.driving
        onTriggered: car.tick()
    }
    Timer {
        objectName: "player"
        interval: 1000
        repeat: true
        running: car.playing
        onTriggered: car.play()
    }

    WallClock {
        id: clock
        objectName: "clock"
    }

    Item {
        id: design
        width: 1600
        height: 900
        anchors.centerIn: parent
        scale: Math.min(root.width / 1600, root.height / 900)

        Cluster {
            x: 20
            y: 16
            width: 1560
            height: 300
            theme: theme
            car: car
        }

        // The touchscreen.
        Rectangle {
            x: 20
            y: 332
            width: 1560
            height: 552
            radius: 28
            fill: theme.surface
            border.color: theme.line
            border.width: 1
            clip: true

            StatusBar {
                id: status
                anchors.left: parent.left
                anchors.right: parent.right
                theme: theme
                car: car
                clock: clock.text
            }

            Item {
                id: page
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: status.bottom
                anchors.bottom: dock.top

                NavPanel {
                    anchors.fill: parent
                    visible: car.page === "nav"
                    theme: theme
                    car: car
                }
                MediaPanel {
                    anchors.fill: parent
                    visible: car.page === "media"
                    theme: theme
                    car: car
                }
                ClimatePanel {
                    anchors.fill: parent
                    visible: car.page === "climate"
                    theme: theme
                    car: car
                }
                CarPanel {
                    objectName: "carPanel"
                    anchors.fill: parent
                    visible: car.page === "car"
                    theme: theme
                    car: car
                }
                SettingsPanel {
                    anchors.fill: parent
                    visible: car.page === "settings"
                    theme: theme
                    car: car
                }
                LauncherPanel {
                    anchors.fill: parent
                    visible: car.page === "apps"
                    theme: theme
                    car: car
                }
            }

            Dock {
                id: dock
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                theme: theme
                car: car
            }
        }
    }
}
