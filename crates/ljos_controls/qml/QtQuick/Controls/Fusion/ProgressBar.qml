// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports. The bar is a field as tall as a button, with the
// share done filled in the highlight. The stripes are a drawing of our own.
import QtQuick
import QtQuick.Templates as T

T.ProgressBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // While how much is not known the bar is filled all the way and
    // stripes move along it, and only while the bar is shown.
    contentItem: Component {
        Item {
            id: track

            implicitWidth: 120
            implicitHeight: 24

            Rectangle {
                x: control.mirrored ? track.width - width : 0
                width: control.indeterminate ? track.width : control.position * track.width
                height: track.height
                radius: 2
                border.color: Qt.darker(control.palette.highlight, 1.4)
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.lighter(control.palette.highlight, 1.2) }
                    GradientStop { position: 1; color: control.palette.highlight }
                }
            }

            Item {
                x: 1
                y: 1
                width: track.width - 2
                height: track.height - 2
                clip: true
                visible: control.indeterminate

                // DECISION: the fill is washed with its lighter colour but
                // for a stripe every 31 px, 9 px wide and leaning 41
                // degrees, moving a stripe along every 750 ms: what qml6
                // shows.
                Item {
                    id: stripes

                    readonly property real gap: 31

                    width: parent.width + gap
                    height: parent.height

                    Repeater {
                        model: Math.ceil(stripes.width / stripes.gap) + 1

                        Rectangle {
                            readonly property color wash: Qt.lighter(control.palette.highlight, 1.2)

                            x: index * stripes.gap - stripes.gap / 2
                            y: -stripes.height * 3 / 4
                            width: 16.5
                            height: stripes.height * 2.5
                            rotation: 41
                            color: Qt.rgba(wash.r, wash.g, wash.b, 0.63)
                        }
                    }

                    NumberAnimation on x {
                        from: control.mirrored ? 0 : -stripes.gap
                        to: control.mirrored ? -stripes.gap : 0
                        duration: 750
                        loops: Animation.Infinite
                        running: control.indeterminate && control.visible
                    }
                }
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 120
            implicitHeight: 24
            color: control.palette.base
            border.color: Qt.darker(control.palette.button, 1.4)
            radius: 2

            // A faint shade under the top edge, as if the field were sunken.
            Rectangle {
                x: 1
                y: 1
                width: parent.width - 2
                height: 1
                color: Qt.rgba(0, 0, 0, 0.07)
            }
        }
    }
}
