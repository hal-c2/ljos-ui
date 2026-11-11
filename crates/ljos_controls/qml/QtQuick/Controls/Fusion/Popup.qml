// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Controls.Fusion
import QtQuick.Templates as T

T.Popup {
    id: control

    // What is under the popup while it dims it: the same faint black,
    // modal or not.
    T.Overlay.modal: Rectangle {
        color: Fusion.topShadow
    }
    T.Overlay.modeless: Rectangle {
        color: Fusion.topShadow
    }

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    padding: 6

    background: Component {
        Rectangle {
            color: control.palette.window
            border.color: control.palette.mid
            radius: 2
        }
    }
}
