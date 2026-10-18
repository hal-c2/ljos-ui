// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.MenuBarItem {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 16
    icon.height: 16

    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            alignment: Qt.AlignLeft
            text: control.text
            font: control.font
            color: control.highlighted ? control.palette.highlightedText : control.palette.text
        }
    }

    // Shown only for the item under the pointer or the one whose menu is open.
    background: Component {
        Rectangle {
            implicitWidth: 20
            implicitHeight: 20
            color: control.palette.highlight
            visible: control.highlighted
        }
    }
}
