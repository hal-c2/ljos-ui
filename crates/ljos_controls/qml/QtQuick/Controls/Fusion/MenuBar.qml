// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports: the bar is as tall as its items, in the window's
// colour, over a line a shade darker.
import QtQuick
import QtQuick.Templates as T

T.MenuBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    delegate: MenuBarItem {}

    contentItem: Component {
        Row {
            spacing: control.spacing
        }
    }

    background: Component {
        Rectangle {
            implicitHeight: 20
            color: control.palette.window

            Rectangle {
                y: parent.height - height
                width: parent.width
                height: 1
                color: Qt.darker(control.palette.window, 1.1)
            }
        }
    }
}
