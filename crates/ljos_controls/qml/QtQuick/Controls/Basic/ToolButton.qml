// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.ToolButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 24
    icon.height: 24

    contentItem: Item {
        // What an application reads off the content item of a button.
        property alias text: label.text
        property alias font: label.font
        property alias color: label.color

        implicitWidth: control.text.length > 0 ? label.implicitWidth : 0
        implicitHeight: control.text.length > 0 ? label.implicitHeight : 0

        Text {
            id: label
            anchors.fill: parent
            text: control.text
            font: control.font
            color: control.palette.buttonText
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
    }

    // Faint until it is held down, so that a row of them reads as a bar.
    background: Rectangle {
        implicitWidth: 40
        implicitHeight: 40
        opacity: control.down ? 1.0 : 0.5
        color: control.down || control.checked || control.highlighted ? control.palette.mid
                                                                      : control.palette.button
    }
}
