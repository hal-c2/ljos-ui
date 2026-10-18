// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Templates as T

T.BusyIndicator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6

    // A ring of dots, darker towards the head, that turns while it runs and
    // fades out when it stops. The turn stops with the fade, and whenever
    // the indicator is not shown, so a stopped one costs nothing.
    contentItem: Component {
        Item {
            id: ring

            implicitWidth: 28
            implicitHeight: 28
            opacity: control.running ? 1 : 0
            visible: opacity > 0

            Behavior on opacity {
                NumberAnimation { duration: 250 }
            }

            Repeater {
                model: 8

                Rectangle {
                    readonly property real turn: index / 8 * 2 * Math.PI
                    readonly property real reach: (ring.width - width) / 2

                    x: reach + Math.sin(turn) * reach
                    y: reach - Math.cos(turn) * reach
                    width: ring.width / 6
                    height: width
                    radius: width / 2
                    color: control.palette.dark
                    opacity: 0.25 + 0.75 * index / 7
                }
            }

            RotationAnimation on rotation {
                from: 0
                to: 360
                duration: 1200
                loops: Animation.Infinite
                running: ring.visible
            }
        }
    }
}
