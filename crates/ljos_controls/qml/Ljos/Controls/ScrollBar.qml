// Ljós's own style (docs/DIALECT.md §4.16): a thin rounded handle that
// shows while the view is scrolled and fades a while after; at once it
// would flicker between the notches of a wheel. Under the pointer the
// bar shows its track and the handle darkens, so it reads as something
// to drag. A fade moves nothing, so reduced motion keeps it.
import QtQuick
import QtQuick.Templates as T
import Ljos.Design

T.ScrollBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 2
    hoverEnabled: true
    visible: control.policy !== T.ScrollBar.AlwaysOff
    // A handle is never shorter than the bar is thick.
    minimumSize: orientation === Qt.Horizontal ? height / width : width / height

    contentItem: Component {
        Rectangle {
            id: handle

            readonly property bool shown: control.policy === T.ScrollBar.AlwaysOn
                                          || (control.active && control.size < 1.0)

            implicitWidth: control.interactive ? 6 : 2
            implicitHeight: control.interactive ? 6 : 2
            radius: Math.min(width, height) / 2
            // The handle is read against the ground it lies on: 3:1.
            color: control.pressed || control.hovered ? Theme.textMuted : Theme.iconMuted
            opacity: 0

            onShownChanged: {
                if (shown) {
                    fade.stop()
                    opacity = 1
                } else {
                    fade.restart()
                }
            }
            Component.onCompleted: if (shown) opacity = 1

            SequentialAnimation {
                id: fade
                PauseAnimation { duration: 600 }
                NumberAnimation {
                    target: handle
                    property: "opacity"
                    to: 0
                    duration: Motion.exitSmall
                    easing.type: Easing.OutCubic
                }
            }
        }
    }

    // The track, only while the pointer is on an interactive bar.
    background: Component {
        Rectangle {
            radius: Math.min(width, height) / 2
            color: Theme.surfaceRaised
            visible: control.interactive && (control.hovered || control.pressed)
                     && control.size < 1.0
        }
    }
}
