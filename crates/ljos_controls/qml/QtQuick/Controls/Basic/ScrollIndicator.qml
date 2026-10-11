// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.ScrollIndicator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 2

    // Shows while the content moves and fades a while after.
    contentItem: Component {
        Rectangle {
            id: handle

            readonly property bool shown: control.active && control.size < 1.0

            implicitWidth: 2
            implicitHeight: 2
            color: control.palette.mid
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
