// The Fusion style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Fusion's own metrics and palette.
import QtQuick
import QtQuick.Templates as T
import QtQuick.Controls.Fusion

T.ToolBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    horizontalPadding: 6
    // Room for the dark line on the side away from the window's edge.
    topPadding: control.position === T.ToolBar.Footer ? 1 : 0
    bottomPadding: control.position === T.ToolBar.Footer ? 0 : 1

    background: Component {
        Rectangle {
            implicitHeight: 26
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.lighter(control.palette.window, 1.04) }
                GradientStop { position: 1; color: control.palette.window }
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Fusion.lightShade
            }
            Rectangle {
                y: parent.height - height
                width: parent.width
                height: 1
                color: Fusion.darkShade
            }
        }
    }
}
