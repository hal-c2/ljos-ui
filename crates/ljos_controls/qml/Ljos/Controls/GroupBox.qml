// Ljós's own style (docs/DIALECT.md §4.16): a frame with its title above
// it, in the theme's text and a heavier weight, so the title names the
// group without being part of it.
import QtQuick
import QtQuick.Templates as T

T.GroupBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding,
                            implicitLabelWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    spacing: Theme.spacing
    padding: Theme.spacing + Theme.spacingSmall
    font.pixelSize: Theme.fontSize
    // The title sits above the frame and pushes the content down. The
    // style's own label has a line's height even with no title, so it
    // asks for room only when there is a title; a label put in its place
    // gets the room its height asks for, whatever its width.
    topPadding: padding + ((label && label.ownTitle === true ? title.length > 0
                                                             : implicitLabelHeight > 0)
                           ? implicitLabelHeight + spacing : 0)

    label: Component {
        Text {
            readonly property bool ownTitle: true
            // In from the frame's edge as the content is, so the title
            // and what it names start on one line.
            x: control.leftPadding
            width: control.availableWidth
            text: control.title
            font.pixelSize: control.font.pixelSize
            font.family: control.font.family
            font.weight: Font.DemiBold
            color: Theme.text
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: control.mirrored ? Text.AlignRight : Text.AlignLeft
        }
    }

    background: Component {
        Rectangle {
            y: control.topPadding - control.padding
            width: control.width
            height: control.height - y
            radius: Theme.radius
            color: "transparent"
            border.width: 1
            border.color: Theme.border
        }
    }
}
