// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.ItemDelegate {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 12
    spacing: 8

    icon.width: 24
    icon.height: 24

    // TODO(image): the icon goes beside the text, as `display` says.
    contentItem: Item {
        // What an application reads off the content item of a button.
        property alias text: label.text
        property alias font: label.font
        property alias color: label.color

        implicitWidth: label.implicitWidth
        implicitHeight: label.implicitHeight

        Text {
            id: label
            width: parent.width
            height: parent.height
            text: control.text
            font: control.font
            color: control.highlighted ? control.palette.highlightedText : control.palette.text
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
    }

    // A row shows only while something is going on in it.
    background: Rectangle {
        implicitWidth: 100
        implicitHeight: 40
        visible: control.down || control.highlighted || control.visualFocus
        color: control.down ? control.palette.midlight : control.palette.light
    }
}
