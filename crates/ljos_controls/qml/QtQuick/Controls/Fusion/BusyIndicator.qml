// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports. The ring is a drawing of our own.
import QtQuick
import QtQuick.Shapes
import QtQuick.Templates as T

T.BusyIndicator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6

    // A ring in the text's colour, solid at its head and fading to nothing
    // round to its tail, that turns while it runs and fades out when it
    // stops. The turn stops with the fade, and whenever the indicator is
    // not shown, so a stopped one costs nothing.
    contentItem: Component {
        Item {
            id: ring

            property color color: control.palette.windowText

            implicitWidth: 28
            implicitHeight: 28
            opacity: control.running ? 1 : 0
            visible: opacity > 0

            Behavior on opacity {
                NumberAnimation { duration: 250 }
            }

            Shape {
                anchors.fill: parent

                ShapePath {
                    id: band

                    readonly property real outer: Math.min(ring.width, ring.height) / 2
                    // DECISION: an eighth of the ring's width, as qml6 shows.
                    readonly property real inner: outer * 3 / 4

                    strokeWidth: -1
                    fillRule: ShapePath.OddEvenFill
                    fillGradient: ConicalGradient {
                        centerX: ring.width / 2
                        centerY: ring.height / 2
                        angle: 90
                        GradientStop { position: 0; color: ring.color }
                        GradientStop { position: 1; color: Qt.rgba(ring.color.r, ring.color.g, ring.color.b, 0) }
                    }

                    PathAngleArc {
                        centerX: ring.width / 2
                        centerY: ring.height / 2
                        radiusX: band.outer
                        radiusY: band.outer
                        sweepAngle: 360
                    }
                    PathAngleArc {
                        centerX: ring.width / 2
                        centerY: ring.height / 2
                        radiusX: band.inner
                        radiusY: band.inner
                        sweepAngle: 360
                    }
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
