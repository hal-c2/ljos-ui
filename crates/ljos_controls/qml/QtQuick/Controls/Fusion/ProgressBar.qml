// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports. The bar is a field as tall as a button, with the
// share done filled in the highlight.
import QtQuick
import QtQuick.Templates as T

T.ProgressBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // While how much is not known a block sweeps along instead, and only
    // while the bar is shown.
    contentItem: Component {
        Item {
            id: track

            implicitWidth: 120
            implicitHeight: 24
            clip: control.indeterminate

            Rectangle {
                visible: !control.indeterminate
                x: control.mirrored ? track.width - width : 0
                width: control.position * track.width
                height: track.height
                radius: 2
                border.color: Qt.darker(control.palette.highlight, 1.4)
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.lighter(control.palette.highlight, 1.2) }
                    GradientStop { position: 1; color: control.palette.highlight }
                }
            }

            Rectangle {
                id: sweep

                visible: control.indeterminate
                width: track.width / 4
                height: track.height
                radius: 2
                color: control.palette.highlight

                NumberAnimation on x {
                    from: -sweep.width
                    to: track.width
                    duration: 1200
                    loops: Animation.Infinite
                    running: control.indeterminate && control.visible
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
        }
    }
}
