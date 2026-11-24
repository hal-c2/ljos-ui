// Ljós's own style (docs/DIALECT.md §4.16): the style's loading icon,
// the one an empty state shows while it loads, turning while `running`;
// with reduced motion it pulses instead, so the wait still shows without
// anything turning. It fades in and out as it starts and stops, and runs
// no animation while it is not shown.
import QtQuick
import QtQuick.Templates as T
import Ljos.Design

T.BusyIndicator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: Theme.spacingSmall

    contentItem: Component {
        Item {
            id: ring

            implicitWidth: Theme.iconSize + 8
            implicitHeight: Theme.iconSize + 8
            opacity: control.running ? 1 : 0
            visible: opacity > 0

            Behavior on opacity {
                NumberAnimation { duration: Motion.enterSmall; easing.type: Easing.OutCubic }
            }

            Icon {
                id: glyph
                objectName: "glyph"
                anchors.centerIn: parent
                name: "circle-dashed"
                size: Math.min(ring.width, ring.height)
                color: Theme.iconMuted
                rotation: 0
                opacity: 1

                RotationAnimation on rotation {
                    running: ring.visible && control.visible && !Motion.reduced
                    from: 0
                    to: 360
                    duration: 1600
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) glyph.rotation = 0
                }
                SequentialAnimation on opacity {
                    running: ring.visible && control.visible && Motion.reduced
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) glyph.opacity = 1
                    NumberAnimation { to: 0.4; duration: 800; easing.type: Easing.InOutSine }
                    NumberAnimation { to: 1; duration: 800; easing.type: Easing.InOutSine }
                }
            }
        }
    }
}
