// The Fusion style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Fusion's own metrics and palette.
import QtQuick
import QtQuick.Templates as T

T.ToolSeparator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    horizontalPadding: vertical ? 6 : 2
    verticalPadding: vertical ? 2 : 6

    // A groove: a dark line with a light one beside it.
    contentItem: Component {
        Rectangle {
            implicitWidth: control.vertical ? 2 : 8
            implicitHeight: control.vertical ? 8 : 2
            color: Qt.darker(control.palette.window, 1.1)

            Rectangle {
                x: control.vertical ? 1 : 0
                y: control.vertical ? 0 : 1
                width: control.vertical ? 1 : parent.width
                height: control.vertical ? parent.height : 1
                color: control.palette.light
            }
        }
    }
}
