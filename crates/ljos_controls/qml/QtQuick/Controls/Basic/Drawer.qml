// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.Drawer {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    // The line along the side that faces the window is not content.
    topPadding: control.edge === Qt.BottomEdge ? 1 : 0
    leftPadding: control.edge === Qt.RightEdge ? 1 : 0
    rightPadding: control.edge === Qt.LeftEdge ? 1 : 0
    bottomPadding: control.edge === Qt.TopEdge ? 1 : 0

    enter: Transition {
        NumberAnimation { property: "position"; to: 1.0; duration: 300; easing.type: Easing.OutCubic }
    }
    exit: Transition {
        NumberAnimation { property: "position"; to: 0.0; duration: 300; easing.type: Easing.OutCubic }
    }

    background: Component {
        Rectangle {
            color: control.palette.window

            Rectangle {
                readonly property bool across: control.edge === Qt.LeftEdge
                                               || control.edge === Qt.RightEdge
                x: control.edge === Qt.LeftEdge ? parent.width - 1 : 0
                y: control.edge === Qt.TopEdge ? parent.height - 1 : 0
                width: across ? 1 : parent.width
                height: across ? parent.height : 1
                color: control.palette.dark
            }
        }
    }
}
