// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Templates as T

T.ToolTip {
    id: control

    // Above the middle of what it is about.
    x: parent ? (parent.width - implicitWidth) / 2 : 0
    y: -implicitHeight - 3

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    margins: 6
    padding: 6

    contentItem: Component {
        Text {
            text: control.text
            font: control.font
            wrapMode: Text.Wrap
            color: control.palette.toolTipText
        }
    }

    background: Component {
        Rectangle {
            color: control.palette.toolTipBase
            border.color: control.palette.toolTipText
        }
    }
}
