// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.Switch {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    // A track with a handle that is at one end or the other, and follows
    // the pointer while it is dragged.
    indicator: Component {
        Rectangle {
            implicitWidth: 56
            implicitHeight: 28
            x: control.text.length > 0
               ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
               : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: 8
            color: control.checked ? control.palette.dark : control.palette.midlight
            // Where the keyboard is, when a key brought it there.
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0

            Rectangle {
                x: control.visualPosition * (parent.width - width)
                width: 28
                height: 28
                radius: 16
                color: control.down ? control.palette.light : control.palette.window
                border.width: 1
                border.color: control.palette.mid
            }
        }
    }

    contentItem: Component {
        Item {
            // What an application reads off the content item of a button.
            property alias text: label.text
            property alias font: label.font
            property alias color: label.color

            implicitWidth: label.implicitWidth + label.gap
            implicitHeight: label.implicitHeight

            Text {
                id: label
                readonly property real gap: control.indicator
                                            ? control.indicator.width + control.spacing : 0
                x: control.mirrored ? 0 : gap
                width: parent.width - gap
                height: parent.height
                text: control.text
                font: control.font
                color: control.palette.windowText
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
        }
    }
}
