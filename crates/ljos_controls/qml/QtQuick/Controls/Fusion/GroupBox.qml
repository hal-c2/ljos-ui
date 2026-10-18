// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Templates as T

T.GroupBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding,
                            implicitLabelWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    spacing: 6
    padding: 9
    topPadding: padding + (implicitLabelWidth > 0 ? implicitLabelHeight + spacing : 0)

    label: Component {
        Text {
            x: control.leftPadding
            width: control.availableWidth
            text: control.title
            font: control.font
            color: control.palette.windowText
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
    }

    // A frame that starts under the title, over the faintest shade.
    background: Component {
        Rectangle {
            y: control.topPadding - control.bottomPadding
            height: control.height - control.topPadding + control.bottomPadding
            radius: 2
            color: Qt.rgba(0, 0, 0, 3 / 255)
            border.color: control.palette.mid
        }
    }
}
