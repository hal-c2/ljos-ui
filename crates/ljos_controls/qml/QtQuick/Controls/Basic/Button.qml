// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.Button {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    horizontalPadding: padding + 2
    spacing: 6

    icon.width: 24
    icon.height: 24

    // A button without text has no content to make room for.
    // TODO(image): the icon goes beside the text, as `display` says.
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
            color: control.checked || control.highlighted ? control.palette.brightText
                                                          : control.palette.buttonText
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
    }

    background: Rectangle {
        implicitWidth: 100
        implicitHeight: 40
        visible: !control.flat || control.down || control.checked || control.highlighted
        // Held down, the colour is half way to `mid`.
        color: {
            const base = control.checked || control.highlighted ? control.palette.dark
                                                                : control.palette.button
            if (!control.down)
                return base
            const mid = control.palette.mid
            return Qt.rgba(Math.round((base.r + mid.r) * 127.5 + 0.001) / 255,
                           Math.round((base.g + mid.g) * 127.5 + 0.001) / 255,
                           Math.round((base.b + mid.b) * 127.5 + 0.001) / 255, 1)
        }
        // TODO(focus): a border in `palette.highlight` while the button
        // has visual focus.
    }
}
