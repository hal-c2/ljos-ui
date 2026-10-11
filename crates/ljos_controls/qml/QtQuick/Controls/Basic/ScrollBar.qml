// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.ScrollBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 2
    visible: control.policy !== T.ScrollBar.AlwaysOff
    // A handle is never shorter than the bar is thick.
    minimumSize: orientation === Qt.Horizontal ? height / width : width / height

    // The handle. It shows while the bar is in use and fades a while after:
    // at once it would flicker between the notches of a wheel.
    contentItem: Component {
        Rectangle {
            id: handle

            readonly property bool shown: control.policy === T.ScrollBar.AlwaysOn
                                          || (control.active && control.size < 1.0)

            implicitWidth: control.interactive ? 6 : 2
            implicitHeight: control.interactive ? 6 : 2
            radius: width / 2
            color: control.pressed ? control.palette.dark : control.palette.mid
            opacity: 0.0

            onShownChanged: {
                if (shown) {
                    fade.stop()
                    opacity = 0.75
                } else {
                    fade.restart()
                }
            }
            Component.onCompleted: if (shown) opacity = 0.75

            SequentialAnimation {
                id: fade
                PauseAnimation { duration: 450 }
                NumberAnimation { target: handle; property: "opacity"; to: 0.0; duration: 200 }
            }
        }
    }
}
