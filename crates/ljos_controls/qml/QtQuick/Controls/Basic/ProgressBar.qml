// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.ProgressBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // The share done, filled from the start. While how much is not known a
    // block sweeps along instead, and only while the bar is shown.
    contentItem: Component {
        Item {
            id: track

            implicitWidth: 200
            implicitHeight: 6
            clip: control.indeterminate

            Rectangle {
                visible: !control.indeterminate
                x: control.mirrored ? track.width - width : 0
                width: control.position * track.width
                height: track.height
                color: control.palette.dark
            }

            Rectangle {
                id: sweep

                visible: control.indeterminate
                width: track.width / 4
                height: track.height
                color: control.palette.dark

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
            implicitWidth: 200
            implicitHeight: 6
            y: (control.height - height) / 2
            height: 6
            color: control.palette.midlight
        }
    }
}
