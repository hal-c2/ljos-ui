// Ljós's own style (docs/DIALECT.md §4.16): a thin rounded track on the
// edge's colour, filled from the start in the focus colour, which is
// 3:1 on the ground, so how far it is shows without the track's colour.
// Not knowing how far, a short fill sweeps along the track; with reduced
// motion it stays at the start and pulses instead, so the wait still
// shows without anything moving across.
import QtQuick
import QtQuick.Templates as T
import Ljos.Design

T.ProgressBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    contentItem: Component {
        Item {
            id: track

            implicitWidth: 200
            implicitHeight: 4
            clip: control.indeterminate

            Rectangle {
                objectName: "fill"
                visible: !control.indeterminate
                x: control.mirrored ? track.width - width : 0
                width: control.position * track.width
                height: track.height
                radius: height / 2
                color: Theme.focus
            }

            Rectangle {
                id: sweep
                objectName: "sweep"

                // How far along its way the sweep is, 0 to 1. The way is
                // worked out from the track's width as it is now, so the
                // sweep follows a track that is laid out or resized after
                // it starts. At rest it is at the start of the track.
                property real phase: way > 0 ? width / way : 0
                readonly property real way: track.width + width
                readonly property real along: phase * way - width

                visible: control.indeterminate
                x: control.mirrored ? track.width - width - along : along
                width: Motion.reduced ? track.width / 3 : track.width / 4
                height: track.height
                radius: height / 2
                color: Theme.focus
                opacity: 1

                NumberAnimation on phase {
                    from: 0
                    to: 1
                    duration: Motion.loopPanel
                    loops: Animation.Infinite
                    running: control.indeterminate && control.visible && !Motion.reduced
                    // A stopped sweep is put back at the start, where the
                    // pulse shows it.
                    onRunningChanged: if (!running) sweep.phase = Qt.binding(() => sweep.way > 0 ? sweep.width / sweep.way : 0)
                }
                SequentialAnimation on opacity {
                    running: control.indeterminate && control.visible && Motion.reduced
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) sweep.opacity = 1
                    NumberAnimation { to: 0.4; duration: 800; easing.type: Easing.InOutSine }
                    NumberAnimation { to: 1; duration: 800; easing.type: Easing.InOutSine }
                }
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 200
            implicitHeight: 4
            y: (control.height - height) / 2
            height: 4
            radius: height / 2
            color: Theme.border
        }
    }
}
